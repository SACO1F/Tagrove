//! Portable originals + annotations. No database, settings, paths or credentials in the archive.
use crate::{
    db::{validate_tag_source, Store},
    fsops,
    model::*,
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
};
use unicode_segmentation::UnicodeSegmentation;
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

const MAX_FILES: usize = 10_000;
const MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_MANIFEST: u64 = 32 * 1024 * 1024;
#[derive(Default)]
pub struct Control {
    pub busy: AtomicBool,
    pub cancel: AtomicBool,
    pub bytes: AtomicU64,
    pub completed: AtomicU64,
    total: AtomicU64,
    file_total: AtomicU64,
    phase: Mutex<String>,
    task: Mutex<Value>,
    history: Mutex<Vec<Value>>,
    #[cfg(test)]
    chunk_hook: Mutex<Option<Box<dyn Fn() + Send>>>,
}
struct BusyGuard<'a>(&'a Control);
impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.busy.store(false, Ordering::SeqCst);
    }
}
impl Control {
    #[cfg(test)]
    pub fn set_chunk_hook(&self, hook: impl Fn() + Send + 'static) {
        *self.chunk_hook.lock().unwrap() = Some(Box::new(hook));
    }
    pub fn run_named<T: Serialize>(&self, kind: &str, f: impl FnOnce() -> Result<T>) -> Result<T> {
        if self
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(err("TRANSFER_BUSY", "另一项资料包任务正在进行"));
        }
        let _guard = BusyGuard(self);
        self.cancel.store(false, Ordering::SeqCst);
        self.bytes.store(0, Ordering::SeqCst);
        self.total.store(0, Ordering::SeqCst);
        self.completed.store(0, Ordering::SeqCst);
        self.file_total.store(0, Ordering::SeqCst);
        if let Ok(mut task) = self.task.lock() {
            *task = json!({"id":id(),"kind":kind,"startedAt":now(),"state":"running"});
        }
        self.phase("校验");
        let result = f();
        if let Ok(mut task) = self.task.lock() {
            task["finishedAt"] = json!(now());
            task["state"] = json!(match &result {
                Ok(_) => "done",
                Err(e) if e.starts_with("TRANSFER_CANCELLED:") => "cancelled",
                Err(_) => "failed",
            });
            match &result {
                Ok(value) => task["result"] = serde_json::to_value(value).unwrap_or(Value::Null),
                Err(error) => task["error"] = json!(error),
            }
            if let Ok(mut history) = self.history.lock() {
                history.insert(0, task.clone());
                history.truncate(12);
            }
        }
        result
    }
    fn phase(&self, text: &str) {
        if let Ok(mut p) = self.phase.lock() {
            *p = text.into();
        }
    }
    fn progress(&self, text: &str, bytes: u64, files: usize) {
        self.bytes.store(0, Ordering::SeqCst);
        self.total.store(bytes, Ordering::SeqCst);
        self.completed.store(0, Ordering::SeqCst);
        self.file_total.store(files as u64, Ordering::SeqCst);
        self.phase(text);
    }
    pub fn status(&self) -> Value {
        json!({"busy":self.busy.load(Ordering::SeqCst),"bytes":self.bytes.load(Ordering::SeqCst),"totalBytes":self.total.load(Ordering::SeqCst),"completed":self.completed.load(Ordering::SeqCst),"fileTotal":self.file_total.load(Ordering::SeqCst),"phase":self.phase.lock().map(|p|p.clone()).unwrap_or_default(),"task":self.task.lock().map(|t|t.clone()).unwrap_or_default(),"history":self.history.lock().map(|h|h.clone()).unwrap_or_default()})
    }
    fn check(&self) -> Result<()> {
        if self.cancel.load(Ordering::SeqCst) {
            Err(err(
                "TRANSFER_CANCELLED",
                "资料包任务已取消，原文件与原有标注保留",
            ))
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    web_source: Option<crate::capture::Source>,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent_folder: Option<String>,
    entry: String,
    bytes: u64,
    sha256: String,
    note: String,
    favorite: bool,
    added: i64,
    modified: i64,
    tags: Vec<Tag>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    format: u32,
    package_id: String,
    app_version: String,
    created_at: i64,
    files: Vec<Entry>,
}
pub struct Plan {
    files: Vec<FileRecord>,
    pub info: Value,
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn digest<R: Read, W: Write>(
    input: &mut R,
    output: &mut W,
    expected: u64,
    control: &Control,
) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut count = 0u64;
    loop {
        control.check()?;
        let n = input.read(&mut buffer).map_err(|e| err("TRANSFER_IO", e))?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > expected {
            return Err(err("FILE_CHANGED", "文件长度发生变化或超过资料包声明"));
        }
        output
            .write_all(&buffer[..n])
            .map_err(|e| err("TRANSFER_IO", e))?;
        hasher.update(&buffer[..n]);
        control.bytes.fetch_add(n as u64, Ordering::SeqCst);
        #[cfg(test)]
        if let Some(hook) = control.chunk_hook.lock().unwrap().as_ref() {
            hook();
        }
    }
    if count != expected {
        return Err(err("FILE_CHANGED", "文件长度与资料包声明不一致"));
    }
    Ok(format!("{:x}", hasher.finalize()))
}
fn valid_time(value: i64) -> bool {
    (0..=253_402_300_799_999).contains(&value)
}
fn safe_name(name: &str) -> bool {
    // All archive names are generated. Imported filenames must work on Windows too.
    if name.is_empty()
        || name.len() > 240
        || name.ends_with(['.', ' '])
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
    {
        return false;
    }
    let base = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    ![
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ]
    .contains(&base.as_str())
}
fn summary(m: &Manifest, fingerprint: &str) -> Value {
    let names: HashSet<_> = m
        .files
        .iter()
        .flat_map(|f| f.tags.iter().map(|t| key(&t.name)))
        .collect();
    json!({"fingerprint":fingerprint,"packageId":m.package_id,"fileCount":m.files.len(),"tagCount":names.len(),"bytes":m.files.iter().map(|f|f.bytes).sum::<u64>(),"version":m.app_version,"createdAt":m.created_at,"files":m.files.iter().take(100).map(|f|json!({"name":f.name,"tagCount":f.tags.len(),"favorite":f.favorite,"hasNote":!f.note.is_empty()})).collect::<Vec<_>>()})
}
impl Store {
    pub fn transfer_plan(&self, v: &Value) -> Result<Plan> {
        let mut ids = string_list(v, "ids");
        if ids.is_empty() {
            let tag = v["tagId"].as_str().unwrap_or("");
            let mut s=self.conn.prepare("SELECT id FROM files WHERE removed_at IS NULL AND (?='' OR EXISTS(SELECT 1 FROM file_tags WHERE file_id=files.id AND tag_id=?)) ORDER BY id LIMIT 10001").map_err(sql_err)?;
            ids = s
                .query_map(params![tag, tag], |r| r.get(0))
                .map_err(sql_err)?
                .collect::<std::result::Result<Vec<String>, _>>()
                .map_err(sql_err)?;
        }
        ids.sort();
        ids.dedup();
        if ids.is_empty() || ids.len() > MAX_FILES {
            return Err(err(
                "TRANSFER_LIMIT",
                "请选择 1 至 10,000 个文件，可分批交接",
            ));
        }
        let mut files = Vec::new();
        let mut missing = vec![];
        let mut bytes = 0u64;
        for fid in ids {
            let f = self.file(&fid)?;
            if f.web_source.is_none() && !safe_name(&f.name) {
                return Err(err(
                    "TRANSFER_FILENAME",
                    format!("文件名过长或无法跨设备使用：{}", f.name),
                ));
            }
            match fsops::inspect(Path::new(&f.path)) {
                Ok(meta) if meta.identity == f.identity && meta.revision == f.revision => {
                    bytes = bytes
                        .checked_add(meta.bytes as u64)
                        .ok_or_else(|| err("TRANSFER_LIMIT", "资料过大"))?;
                }
                _ => missing.push(f.name.clone()),
            }
            files.push(f);
        }
        if bytes > MAX_BYTES {
            return Err(err("TRANSFER_LIMIT", "单个资料包最多 8 GB，请分批交接"));
        }
        let fingerprint = sha(&serde_json::to_vec(&files).map_err(|e| err("TRANSFER_FAILED", e))?);
        let info = json!({"fileCount":files.len(),"bytes":bytes,"missing":missing,"fingerprint":fingerprint,"files":files.iter().take(100).map(|f|json!({"name":f.name,"tagCount":f.tags.len(),"favorite":f.favorite,"hasNote":!f.note.is_empty()})).collect::<Vec<_>>()});
        Ok(Plan { files, info })
    }
}
pub fn export(plan: Plan, path: &Path, expected: &str, control: &Control) -> Result<Value> {
    if plan.info["fingerprint"].as_str() != Some(expected) {
        return Err(err("TRANSFER_CHANGED", "资料已变化，请重新预览"));
    }
    if !plan.info["missing"].as_array().unwrap().is_empty() {
        return Err(err(
            "TRANSFER_MISSING",
            "部分原文件缺失或变化，请刷新或重新关联后导出",
        ));
    }
    if path.extension().and_then(|e| e.to_str()) != Some("sqtagpack") {
        return Err(err("TRANSFER_PATH", "资料包扩展名必须为 .sqtagpack"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| err("TRANSFER_PATH", "请选择保存位置"))?;
    let mut output = tempfile::NamedTempFile::new_in(parent).map_err(|e| err("TRANSFER_IO", e))?;
    let mut manifest = Manifest {
        format: if plan.files.iter().any(|f|f.web_source.as_ref().is_some_and(|s|s.kind=="web_image")) {7} else if plan.files.iter().any(|f| f.web_source.as_ref().is_some_and(|s| s.reading.is_some())) { 6 } else if plan
            .files
            .iter()
            .any(|f| f.web_source.as_ref().is_some_and(|s| s.kind == "web_image"))
        {
            5
        } else if plan.files.iter().any(|f| f.web_source.is_some()) {
            4
        } else {
            3
        },
        package_id: id(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        created_at: now(),
        files: vec![],
    };
    control.progress(
        "打包文件",
        plan.files.iter().map(|f| f.bytes as u64).sum(),
        plan.files.len(),
    );
    {
        let mut zip = ZipWriter::new(output.as_file_mut());
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (i, f) in plan.files.iter().enumerate() {
            let meta = fs::symlink_metadata(&f.path).map_err(|e| err("TRANSFER_IO", e))?;
            if fsops::is_link(&meta) || !meta.is_file() {
                return Err(err("TRANSFER_LINK", "资料包不接受链接或目录"));
            }
            let mut input = File::open(&f.path).map_err(|e| err("TRANSFER_IO", e))?;
            let before = input.metadata().map_err(|e| err("TRANSFER_IO", e))?;
            if fsops::identity(&input) != f.identity || fsops::revision(&before) != f.revision {
                return Err(err("FILE_CHANGED", &f.name));
            }
            let portable = f
                .web_source
                .as_ref()
                .map(|w| {
                    if w.kind == "web_image" {
                        crate::image_capture::portable_name(w)
                    } else {
                        crate::capture::portable_name(&w.title)
                    }
                })
                .unwrap_or_else(|| f.name.clone());
            let entry = format!("files/{i:05}/{}", portable);
            zip.start_file(&entry, options)
                .map_err(|e| err("TRANSFER_IO", e))?;
            let hash = digest(&mut input, &mut zip, before.len(), control)?;
            if f.web_source
                .as_ref()
                .and_then(|s| s.image.as_ref())
                .is_some_and(|image| image.sha256 != hash)
            {
                return Err(err(
                    "FILE_CHANGED",
                    "采集图片内容已变化，请核对原图后重新关联",
                ));
            }
            if fsops::revision(&input.metadata().map_err(|e| err("TRANSFER_IO", e))?) != f.revision
            {
                return Err(err("FILE_CHANGED", &f.name));
            }
            control.completed.store((i + 1) as u64, Ordering::SeqCst);
            manifest.files.push(Entry {
                web_source: f.web_source.clone(),
                name: portable,
                parent_folder: Some(
                    Path::new(&f.parent)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .filter(|n| safe_name(n))
                        .unwrap_or("未分类")
                        .to_owned(),
                ),
                entry,
                bytes: before.len(),
                sha256: hash,
                note: f.note.clone(),
                favorite: f.favorite,
                added: f.added,
                modified: f.modified,
                tags: f.tags.clone(),
            });
        }
        let content = serde_json::to_vec(&manifest).map_err(|e| err("TRANSFER_FAILED", e))?;
        if content.len() as u64 > MAX_MANIFEST {
            return Err(err("TRANSFER_LIMIT", "标注清单过大，请分批交接"));
        }
        zip.start_file("manifest.json", options)
            .map_err(|e| err("TRANSFER_IO", e))?;
        zip.write_all(&content).map_err(|e| err("TRANSFER_IO", e))?;
        zip.start_file("manifest.sha256", options)
            .map_err(|e| err("TRANSFER_IO", e))?;
        zip.write_all(sha(&content).as_bytes())
            .map_err(|e| err("TRANSFER_IO", e))?;
        zip.finish().map_err(|e| err("TRANSFER_IO", e))?;
    }
    control.check()?;
    output
        .as_file()
        .sync_all()
        .map_err(|e| err("TRANSFER_IO", e))?;
    output
        .persist_noclobber(path)
        .map_err(|e| err("TRANSFER_IO", format!("{e}；请选择未使用的文件名")))?;
    Ok(
        json!({"path":path.to_string_lossy(),"fileCount":manifest.files.len(),"bytes":manifest.files.iter().map(|f|f.bytes).sum::<u64>()}),
    )
}
fn inspect(path: &Path, control: &Control) -> Result<(ZipArchive<File>, Manifest, String)> {
    let mut zip = ZipArchive::new(File::open(path).map_err(|e| err("TRANSFER_IO", e))?)
        .map_err(|e| err("INVALID_PACKAGE", e))?;
    if zip.len() < 3 || zip.len() > MAX_FILES + 2 {
        return Err(err("INVALID_PACKAGE", "资料包文件数异常"));
    }
    let content = {
        let mut f = zip
            .by_name("manifest.json")
            .map_err(|e| err("INVALID_PACKAGE", e))?;
        if f.size() > MAX_MANIFEST {
            return Err(err("INVALID_PACKAGE", "资料清单过大"));
        }
        let mut b = vec![];
        (&mut f)
            .take(MAX_MANIFEST + 1)
            .read_to_end(&mut b)
            .map_err(|e| err("INVALID_PACKAGE", e))?;
        if b.len() as u64 > MAX_MANIFEST {
            return Err(err("INVALID_PACKAGE", "资料清单过大"));
        }
        b
    };
    let checksum = {
        let mut f = zip
            .by_name("manifest.sha256")
            .map_err(|e| err("INVALID_PACKAGE", e))?;
        let mut b = vec![];
        (&mut f)
            .take(65)
            .read_to_end(&mut b)
            .map_err(|e| err("INVALID_PACKAGE", e))?;
        b
    };
    if checksum != sha(&content).as_bytes() {
        return Err(err("CHECKSUM_MISMATCH", "资料清单校验失败"));
    }
    let m: Manifest = serde_json::from_slice(&content).map_err(|e| err("INVALID_PACKAGE", e))?;
    if !valid_time(m.created_at)
        || m.app_version.len() > 100
        || ![1, 2, 3, 4, 5, 6, 7].contains(&m.format)
        || uuid::Uuid::parse_str(&m.package_id).is_err()
        || m.files.is_empty()
        || m.files.len() > MAX_FILES
        || m.files.len() + 2 != zip.len()
    {
        return Err(err("INVALID_PACKAGE", "不支持的资料包版本或清单"));
    }
    let mut expected = HashSet::from(["manifest.json".to_string(), "manifest.sha256".to_string()]);
    let mut total = 0u64;
    for (i, f) in m.files.iter().enumerate() {
        if let Some(web) = &f.web_source {
            web.validate()?;
            if web.reading.is_some() && m.format < 6 {
                return Err(err("INVALID_PACKAGE", "阅读副本需要资料包格式 6"));
            }
            let image = web.kind == "web_image";
            if let Some(img)=&web.image {if m.format<7 && (u64::from(img.width)*u64::from(img.height)>40_000_000 || !matches!(img.mime.as_str(),"image/png"|"image/jpeg"|"image/webp") || f.bytes>16*1024*1024) {return Err(err("INVALID_PACKAGE","增强图片需要资料包格式 7"));}}

            if (image
                && (m.format < 5
                    || f.sha256 != web.image.as_ref().unwrap().sha256
                    || !f.name.ends_with(&format!(
                        ".{}",
                        crate::image_capture::extension(web.image.as_ref().unwrap())
                    ))))
                || (!image && (m.format < 4 || !f.name.ends_with(".sqweb")))
            {
                return Err(err("INVALID_PACKAGE", "网页资料包格式无效"));
            }
        }
        if f.name.ends_with(".sqweb") && f.web_source.is_none() {
            return Err(err("INVALID_PACKAGE", "网页资料缺少来源信息"));
        }
        if !safe_name(&f.name)
            || (m.format >= 2 && f.parent_folder.is_none())
            || f.parent_folder
                .as_ref()
                .map(|name| !safe_name(name))
                .unwrap_or(false)
            || f.entry != format!("files/{i:05}/{}", f.name)
            || !expected.insert(f.entry.clone())
            || f.sha256.len() != 64
            || !f.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || f.note.graphemes(true).count() > 10000
            || !valid_time(f.added)
            || !valid_time(f.modified)
            || f.tags.len() > 1000
        {
            return Err(err("INVALID_PACKAGE", "无效的文件名称、大小或标注"));
        }
        let mut tag_names = HashSet::new();
        for t in &f.tags {
            validate_tag_source(
                &t.name,
                if t.source == "folder" || t.created_by == "folder" {
                    "folder"
                } else {
                    "manual"
                },
            )?;
            if t.aliases.len() > 20
                || (!t.aliases.is_empty() && (m.format < 3 || t.created_by != "manual"))
            {
                return Err(err("INVALID_PACKAGE", "标签别名格式无效"));
            }
            let mut alias_keys = HashSet::from([key(&t.name)]);
            for a in &t.aliases {
                if validate_tag_source(a, "manual")? != *a || !alias_keys.insert(key(a)) {
                    return Err(err("INVALID_PACKAGE", "标签别名重复或无效"));
                }
            }
            if t.ai
                .as_ref()
                .map(|a| {
                    !valid_time(a.updated_at) || a.model.len() > 1000 || a.reason.len() > 10000
                })
                .unwrap_or(false)
            {
                return Err(err("INVALID_PACKAGE", "无效的 AI 来源信息"));
            }
            if !tag_names.insert(key(&t.name))
                || !["manual", "folder", "ai"].contains(&t.source.as_str())
                || !["manual", "folder", "ai"].contains(&t.created_by.as_str())
                || (t.source == "ai" && t.ai.is_none())
            {
                return Err(err("INVALID_PACKAGE", "无效的标签来源或重复标签"));
            }
        }
        total = total
            .checked_add(f.bytes)
            .ok_or_else(|| err("INVALID_PACKAGE", "资料过大"))?;
        if total > MAX_BYTES {
            return Err(err("TRANSFER_LIMIT", "资料包超过 8 GB"));
        }
    }
    // Validate the sender namespace before receiver-specific coalescing.
    let mut namespace = std::collections::HashMap::new();
    let mut per_owner: std::collections::HashMap<String, HashSet<String>> =
        std::collections::HashMap::new();
    for t in m
        .files
        .iter()
        .flat_map(|f| &f.tags)
        .filter(|t| t.created_by != "folder")
    {
        let owner = key(&t.name);
        for n in std::iter::once(&t.name).chain(&t.aliases) {
            if namespace
                .insert(key(n), owner.clone())
                .is_some_and(|previous| previous != owner)
            {
                return Err(err("INVALID_PACKAGE", "资料包内规范名称与别名关系冲突"));
            }
        }
        let aliases = per_owner.entry(owner).or_default();
        aliases.extend(t.aliases.iter().map(|a| key(a)));
        if aliases.len() > 20 {
            return Err(err("INVALID_PACKAGE", "资料包内同一标签的别名超过 20 个"));
        }
    }
    let mut actual = HashSet::new();
    for i in 0..zip.len() {
        let f = zip.by_index(i).map_err(|e| err("INVALID_PACKAGE", e))?;
        if !expected.contains(f.name())
            || !actual.insert(f.name().to_string())
            || f.is_dir()
            || f.unix_mode()
                .map(|m| m & 0o170000 == 0o120000)
                .unwrap_or(false)
        {
            return Err(err("INVALID_PACKAGE", "资料包含额外、重复或链接条目"));
        }
    }
    control.progress("校验文件", total, m.files.len());
    for (i, f) in m.files.iter().enumerate() {
        let mut z = zip
            .by_name(&f.entry)
            .map_err(|e| err("INVALID_PACKAGE", e))?;
        if z.size() != f.bytes
            || digest(&mut z, &mut std::io::sink(), f.bytes, control)? != f.sha256
        {
            return Err(err("CHECKSUM_MISMATCH", &f.name));
        }
        control.completed.store((i + 1) as u64, Ordering::SeqCst);
    }
    Ok((zip, m, sha(&content)))
}
fn alias_conflicts(m: &Manifest, s: &Store) -> Result<Vec<String>> {
    let mut space = std::collections::HashMap::new();
    for t in s.tags()?.into_iter().filter(|t| t.created_by != "folder") {
        for n in std::iter::once(&t.name).chain(&t.aliases) {
            space.insert(key(n), key(&t.name));
        }
    }
    let folder_names: HashSet<String> = s
        .tags()?
        .into_iter()
        .filter(|t| t.created_by == "folder")
        .map(|t| key(&t.name))
        .collect();
    let mut counts: std::collections::HashMap<String, HashSet<String>> = s
        .tags()?
        .into_iter()
        .filter(|t| t.created_by != "folder")
        .map(|t| (key(&t.name), t.aliases.iter().map(|a| key(a)).collect()))
        .collect();
    let mut conflicts = vec![];
    for t in m
        .files
        .iter()
        .flat_map(|f| &f.tags)
        .filter(|t| t.created_by != "folder")
    {
        let owner = space
            .get(&key(&t.name))
            .cloned()
            .unwrap_or_else(|| key(&t.name));
        if !t.aliases.is_empty() && folder_names.contains(&key(&t.name)) {
            conflicts.push(format!(
                "「{}」与本机文件夹标签同名，不能导入其别名",
                t.name
            ));
        }
        let aliases = counts.entry(owner.clone()).or_default();
        aliases.extend(t.aliases.iter().map(|a| key(a)).filter(|a| a != &owner));
        if aliases.len() > 20 {
            conflicts.push(format!("「{}」导入后别名超过 20 个", t.name));
        }
        for n in std::iter::once(&t.name).chain(&t.aliases) {
            if let Some(existing) = space.get(&key(n)) {
                if existing != &owner {
                    conflicts.push(format!(
                        "「{n}」已属于另一个标签，不能作为「{}」的别名",
                        t.name
                    ));
                }
            } else {
                space.insert(key(n), owner.clone());
            }
        }
    }
    conflicts.sort();
    conflicts.dedup();
    Ok(conflicts)
}
pub fn preview_in_library(path: &Path, control: &Control, s: &Store) -> Result<Value> {
    let (_, m, fingerprint) = inspect(path, control)?;
    let mut value = summary(&m, &fingerprint);
    value["aliasConflicts"] = json!(alias_conflicts(&m, s)?);
    value["aliasCount"] = json!(m
        .files
        .iter()
        .flat_map(|f| &f.tags)
        .flat_map(|t| &t.aliases)
        .map(|a| key(a))
        .collect::<HashSet<_>>()
        .len());
    Ok(value)
}
pub fn preview(path: &Path, control: &Control) -> Result<Value> {
    let (_, m, fingerprint) = inspect(path, control)?;
    Ok(summary(&m, &fingerprint))
}
/// Owns only copies created by this task. Dropping before commit rolls them back.
pub struct PreparedImport {
    root: PathBuf,
    folder: PathBuf,
    marker: PathBuf,
    owns_marker: bool,
    manifest: Manifest,
    extracted: Vec<fsops::FileMeta>,
    created: Vec<PathBuf>,
    directories: Vec<PathBuf>,
    committed: bool,
}
impl Drop for PreparedImport {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        let mut cleaned = true;
        for path in &self.created {
            if fs::remove_file(path).is_err() {
                cleaned = false;
            }
        }
        for path in self.directories.iter().rev() {
            if fs::remove_dir(path).is_err() {
                cleaned = false;
            }
        }
        if fs::remove_dir(&self.folder).is_err() {
            cleaned = false;
        }
        if cleaned && self.owns_marker {
            let _ = fs::remove_file(&self.marker);
        }
    }
}
impl PreparedImport {
    fn finish(mut self, mut value: Value) -> Value {
        self.committed = true;
        if fs::remove_file(&self.marker).is_err() {
            value["notice"] = json!("导入已完成；下次启动将核对并清理恢复记录");
        }
        value
    }
}
fn prepare_import(
    root: &Path,
    path: &Path,
    destination: &Path,
    expected: &str,
    control: &Control,
    preflight: impl FnOnce(&str) -> Result<()>,
) -> Result<PreparedImport> {
    // Archive verification, extraction and file inspection never require a Store lock.
    let (mut zip, m, fingerprint) = inspect(path, control)?;
    if fingerprint != expected {
        return Err(err("TRANSFER_CHANGED", "资料包已变化，请重新预览"));
    }
    preflight(&m.package_id)?;
    let parent = fs::canonicalize(destination).map_err(|e| err("TRANSFER_PATH", e))?;
    if !parent.is_dir()
        || parent.starts_with(fs::canonicalize(root).map_err(|e| err("TRANSFER_PATH", e))?)
    {
        return Err(err("TRANSFER_PATH", "请选择应用资料库之外的文件夹"));
    }
    let marker = root.join("transfer-state.json");
    if marker.exists() {
        return Err(err(
            "TRANSFER_RECOVERY",
            "存在未处理的资料包中断记录，请重启后查看恢复提示",
        ));
    }
    let folder = parent.join(format!("拾签资料-{}", m.package_id));
    fs::create_dir(&folder).map_err(|e| {
        err(
            "TRANSFER_PATH",
            format!("{e}；请另选目录，已有文件夹不会覆盖"),
        )
    })?;
    let mut prepared = PreparedImport {
        root: root.into(),
        folder,
        marker,
        owns_marker: false,
        manifest: m,
        extracted: vec![],
        created: vec![],
        directories: vec![],
        committed: false,
    };
    let receipt = format!("packageReceipt:{}", prepared.manifest.package_id);
    let mut journal = tempfile::NamedTempFile::new_in(root).map_err(|e| err("TRANSFER_IO", e))?;
    journal.write_all(json!({"packageId":prepared.manifest.package_id,"path":prepared.folder.to_string_lossy(),"receipt":receipt}).to_string().as_bytes()).map_err(|e| err("TRANSFER_IO", e))?;
    journal
        .as_file()
        .sync_all()
        .map_err(|e| err("TRANSFER_IO", e))?;
    journal
        .persist_noclobber(&prepared.marker)
        .map_err(|e| err("TRANSFER_IO", e))?;
    prepared.owns_marker = true;
    control.progress(
        "复制文件",
        prepared.manifest.files.iter().map(|f| f.bytes).sum(),
        prepared.manifest.files.len(),
    );
    let mut used_paths = HashSet::new();
    for (i, f) in prepared.manifest.files.iter().enumerate() {
        control.check()?;
        // v2 stores only the direct parent name, never the sender's absolute path.
        // Legacy v1 packages can recover a category from their folder-origin tag.
        let folder_name = f
            .parent_folder
            .as_deref()
            .or_else(|| {
                f.tags
                    .iter()
                    .find(|t| t.source == "folder" && safe_name(&t.name))
                    .map(|t| t.name.as_str())
            })
            .unwrap_or("未分类");
        let directory = prepared.folder.join(folder_name);
        if !prepared
            .directories
            .iter()
            .any(|p| key(&p.to_string_lossy()) == key(&directory.to_string_lossy()))
        {
            fs::create_dir(&directory).map_err(|e| err("TRANSFER_IO", e))?;
            prepared.directories.push(directory.clone());
        }
        let mut target = directory.join(&f.name);
        let mut suffix = 2;
        while !used_paths.insert(key(&target.to_string_lossy())) {
            let original = Path::new(&f.name);
            let stem = original
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&f.name);
            let mut extension = original
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| format!(".{s}"))
                .unwrap_or_default();
            // Truncate on a Unicode boundary to keep the collision name portable.
            while extension.len() > 200 {
                extension.pop();
            }
            let tail = format!(" ({suffix}){extension}");
            let mut short = stem.to_owned();
            while short.len() + tail.len() > 240 {
                short.pop();
            }
            target = directory.join(format!("{short}{tail}"));
            suffix += 1;
        }
        let mut out = File::options()
            .create_new(true)
            .write(true)
            .open(&target)
            .map_err(|e| err("TRANSFER_IO", e))?;
        prepared.created.push(target.clone());
        let mut z = zip
            .by_name(&f.entry)
            .map_err(|e| err("INVALID_PACKAGE", e))?;
        if digest(&mut z, &mut out, f.bytes, control)? != f.sha256 {
            return Err(err("CHECKSUM_MISMATCH", &f.name));
        }
        if f.modified >= 0 {
            out.set_times(std::fs::FileTimes::new().set_modified(
                std::time::UNIX_EPOCH + std::time::Duration::from_millis(f.modified as u64),
            ))
            .map_err(|e| err("TRANSFER_IO", e))?;
        }
        out.sync_all().map_err(|e| err("TRANSFER_IO", e))?;
        drop(out);
        prepared.extracted.push(fsops::inspect(&target)?);
        control.completed.store((i + 1) as u64, Ordering::SeqCst);
    }
    // Detect replacements/changes during staging before entering the database transaction.
    for meta in &prepared.extracted {
        control.check()?;
        let current = fsops::inspect(Path::new(&meta.path))?;
        if current.identity != meta.identity || current.revision != meta.revision {
            return Err(err(
                "FILE_CHANGED",
                "接收文件在导入期间发生变化，请重新导入",
            ));
        }
    }
    Ok(prepared)
}
pub fn import_shared(
    store: &Mutex<Store>,
    path: &Path,
    destination: &Path,
    expected: &str,
    control: &Control,
) -> Result<Value> {
    let root = store
        .lock()
        .map_err(|_| err("INTERNAL_ERROR", "资料库锁异常"))?
        .root
        .clone();
    let prepared = prepare_import(&root, path, destination, expected, control, |package_id| {
        store
            .lock()
            .map_err(|_| err("INTERNAL_ERROR", "资料库锁异常"))?
            .check_package_receipt(package_id)
    })?;
    control.check()?;
    let value = store
        .lock()
        .map_err(|_| err("INTERNAL_ERROR", "资料库锁异常"))?
        .commit_package(&prepared, control)?;
    Ok(prepared.finish(value))
}
impl Store {
    fn check_package_receipt(&self, package_id: &str) -> Result<()> {
        let receipt = format!("packageReceipt:{package_id}");
        if self
            .conn
            .query_row("SELECT value FROM settings WHERE key=?", [&receipt], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .map_err(sql_err)?
            .is_some()
        {
            return Err(err(
                "PACKAGE_ALREADY_IMPORTED",
                "本库已导入这份资料包；请查看原导入文件，避免重复",
            ));
        }
        Ok(())
    }
    #[cfg(test)]
    pub fn import_package(
        &mut self,
        path: &Path,
        destination: &Path,
        expected: &str,
        control: &Control,
    ) -> Result<Value> {
        let root = self.root.clone();
        let prepared = prepare_import(&root, path, destination, expected, control, |package_id| {
            self.check_package_receipt(package_id)
        })?;
        let value = self.commit_package(&prepared, control)?;
        Ok(prepared.finish(value))
    }
    fn commit_package(&mut self, prepared: &PreparedImport, control: &Control) -> Result<Value> {
        if self.root != prepared.root {
            return Err(err("TRANSFER_CHANGED", "资料库发生变化，请重新导入"));
        }
        let m = &prepared.manifest;
        let folder = &prepared.folder;
        let extracted = &prepared.extracted;
        if !alias_conflicts(m, self)?.is_empty() {
            return Err(err(
                "TAG_NAME_CONFLICT",
                "资料包别名与当前标签冲突，请先整理标签再导入",
            ));
        }
        let receipt = format!("packageReceipt:{}", m.package_id);
        self.check_package_receipt(&m.package_id)?;
        control.check()?;
        control.progress("保存标注", 0, m.files.len());
        self.conn
            .execute_batch("SAVEPOINT package_import")
            .map_err(sql_err)?;
        let commit = (|| -> Result<Value> {
            let mut file_ids = vec![];
            let saved: Option<String> = self
                .conn
                .query_row(
                    "SELECT value FROM settings WHERE key='floatingTags'",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .map_err(sql_err)?;
            let mut pins: Vec<String> = saved
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();
            for (i, (f, meta)) in m.files.iter().zip(extracted).enumerate() {
                control.check()?;
                let fid = id();
                self.conn.execute("INSERT INTO files(id,path,path_key,parent,name,name_key,extension,kind,bytes,modified,added,revision,identity,note,note_key,favorite) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",params![fid,meta.path,meta.path,meta.parent,meta.name,key(&meta.name),meta.extension,meta.kind,meta.bytes,meta.modified,f.added,meta.revision,meta.identity,f.note,key(&f.note),f.favorite]).map_err(sql_err)?;
                if let Some(web) = &f.web_source {
                    web.validate()?;
                    self.conn
                        .execute(
                            "INSERT INTO capture_receipts VALUES(?,?,?,?)",
                            params![id(), "imported", crate::capture::source_dedupe(web), fid],
                        )
                        .map_err(sql_err)?;
                    let name = if web.kind == "web_image" {
                        crate::image_capture::portable_name(web)
                    } else {
                        web.display_title().to_owned()
                    };
                    let kind = if web.kind == "web_image" {
                        "image"
                    } else {
                        "web"
                    };
                    self.conn.execute("UPDATE files SET name=?,name_key=?,kind=?,web_source=?,web_key=?,modified=? WHERE id=?",params![name,key(&name),kind,serde_json::to_string(web).map_err(|e|err("INVALID_PACKAGE",e))?,key(&web.text()),web.captured_at,fid]).map_err(sql_err)?;
                }
                for t in &f.tags {
                    let tag = self.create_tag_with_source(&t.name, &t.created_by)?;
                    if !t.aliases.is_empty() {
                        self.import_aliases(&tag.id, &t.aliases)?;
                    }
                    let ai =
                        t.ai.as_ref()
                            .map(|a| serde_json::to_string(a).unwrap())
                            .unwrap_or_else(|| "{}".into());
                    self.conn
                        .execute(
                            "INSERT INTO file_tags(file_id,tag_id,source,ai_meta) VALUES(?,?,?,?) ON CONFLICT(file_id,tag_id) DO UPDATE SET source=excluded.source,ai_meta=excluded.ai_meta WHERE (CASE excluded.source WHEN 'folder' THEN 4 WHEN 'manual' THEN 3 ELSE 1+COALESCE(json_extract(excluded.ai_meta,'$.confirmed'),0) END) > (CASE file_tags.source WHEN 'folder' THEN 4 WHEN 'manual' THEN 3 ELSE 1+COALESCE(json_extract(file_tags.ai_meta,'$.confirmed'),0) END)",
                            params![fid, tag.id, t.source, ai],
                        )
                        .map_err(sql_err)?;
                    if t.source == "ai"
                        && t.ai.as_ref().map(|a| a.confirmed).unwrap_or(false)
                        && tag.created_by != "folder"
                        && !pins.contains(&tag.id)
                    {
                        pins.push(tag.id);
                    }
                }
                file_ids.push(fid);
                control.completed.store((i + 1) as u64, Ordering::SeqCst);
            }
            self.conn.execute("INSERT INTO settings(key,value) VALUES('floatingTags',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[json!(pins).to_string()]).map_err(sql_err)?;
            self.conn
                .execute(
                    "INSERT INTO settings(key,value) VALUES(?,?)",
                    params![
                        receipt,
                        json!({"path":folder.to_string_lossy(),"ids":file_ids,"importedAt":now()})
                            .to_string()
                    ],
                )
                .map_err(sql_err)?;
            self.validate()?;
            control.check()?;
            Ok(
                json!({"path":folder.to_string_lossy(),"fileCount":file_ids.len(),"ids":file_ids,"packageId":m.package_id}),
            )
        })();
        match commit {
            Ok(value) => {
                if let Err(e) = self.conn.execute_batch("RELEASE package_import") {
                    let _ = self
                        .conn
                        .execute_batch("ROLLBACK TO package_import; RELEASE package_import");
                    return Err(sql_err(e));
                }
                self.undo.clear();
                Ok(value)
            }
            Err(e) => {
                let _ = self
                    .conn
                    .execute_batch("ROLLBACK TO package_import; RELEASE package_import");
                Err(e)
            }
        }
    }
    pub fn recover_transfer(&mut self) -> Result<()> {
        let marker = self.root.join("transfer-state.json");
        if !marker.exists() {
            return Ok(());
        }
        let bytes = fs::read(&marker).map_err(|e| err("TRANSFER_RECOVERY", e))?;
        let record: Value =
            serde_json::from_slice(&bytes).map_err(|e| err("TRANSFER_RECOVERY", e))?;
        self.validate()?;
        let receipt = record["receipt"]
            .as_str()
            .ok_or_else(|| err("TRANSFER_RECOVERY", "资料包恢复记录损坏，请保留数据目录"))?;
        let committed = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM settings WHERE key=?",
                [receipt],
                |r| r.get::<_, i64>(0),
            )
            .map_err(sql_err)?
            != 0;
        let notice = if committed {
            "上次资料包导入已完成，资料库完整性已验证。".to_string()
        } else {
            format!("上次资料包导入中断，数据库未加入该批记录。已复制的文件保留在 {}；请检查后移走或删除该文件夹，再重新导入。",record["path"].as_str().unwrap_or("导入目录"))
        };
        let recovery = self.root.join("backups");
        fs::create_dir_all(&recovery).map_err(|e| err("TRANSFER_RECOVERY", e))?;
        let mut report = File::options()
            .create_new(true)
            .write(true)
            .open(recovery.join(format!("transfer-recovery-{}.txt", id())))
            .map_err(|e| err("TRANSFER_RECOVERY", e))?;
        report
            .write_all(notice.as_bytes())
            .map_err(|e| err("TRANSFER_RECOVERY", e))?;
        report.sync_all().map_err(|e| err("TRANSFER_RECOVERY", e))?;
        self.recovery_notice = format!("{} {}", self.recovery_notice, notice).trim().into();
        self.remember_recovery()?;
        fs::remove_file(marker).map_err(|e| err("TRANSFER_RECOVERY", e))?;
        Ok(())
    }
}
