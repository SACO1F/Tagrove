//! Preview tokens own all candidate paths. Caller can select only a previewed candidate.
use crate::{
    db::{sv, Step, Store, Undo},
    fsops::{self, FileMeta},
    model::*,
};
use rusqlite::OptionalExtension;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

#[derive(Default)]
pub struct Control {
    pub busy: AtomicBool,
    pub cancel: AtomicBool,
    plans: Mutex<HashMap<String, Plan>>,
}
struct Busy<'a>(&'a AtomicBool);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
impl Control {
    pub fn run<T>(&self, f: impl FnOnce() -> Result<T>) -> Result<T> {
        if self.busy.swap(true, Ordering::SeqCst) {
            return Err(err("REPAIR_BUSY", "路径修复正在进行"));
        }
        let _guard = Busy(&self.busy);
        self.cancel.store(false, Ordering::SeqCst);
        f()
    }
    pub fn remember(&self, p: Plan) -> Result<Value> {
        let v = p.view();
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| err("INTERNAL_ERROR", "预览锁异常"))?;
        plans.clear();
        plans.insert(p.token.clone(), p);
        Ok(v)
    }
    pub fn get(&self, token: &str) -> Result<Plan> {
        let plans = self
            .plans
            .lock()
            .map_err(|_| err("INTERNAL_ERROR", "预览锁异常"))?;
        let p = plans
            .get(token)
            .filter(|p| now() - p.created < 15 * 60 * 1000)
            .ok_or_else(|| err("PREVIEW_EXPIRED", "预览已过期，请重新扫描"))?;
        Ok(p.clone())
    }
    pub fn forget(&self, token: &str) {
        if let Ok(mut p) = self.plans.lock() {
            p.remove(token);
        }
    }
}
fn check(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::SeqCst) {
        Err(err("REPAIR_CANCELLED", "扫描已取消，文件引用未修改"))
    } else {
        Ok(())
    }
}
fn lexical(s: &str) -> Result<PathBuf> {
    let p = PathBuf::from(s.trim().trim_matches('"'));
    if !p.is_absolute()
        || p.components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(err("INVALID_PATH", "请输入绝对目录，不能包含 .."));
    }
    Ok(p)
}
pub fn relative(path: &Path, root: &Path) -> Option<PathBuf> {
    let parts: Vec<_> = path.components().collect();
    let prefix: Vec<_> = root.components().collect();
    if prefix.len() > parts.len() {
        return None;
    }
    for (a, b) in parts.iter().zip(&prefix) {
        if cfg!(windows) {
            if key(&a.as_os_str().to_string_lossy()) != key(&b.as_os_str().to_string_lossy()) {
                return None;
            }
        } else if a != b {
            return None;
        }
    }
    Some(
        parts[prefix.len()..]
            .iter()
            .map(|c| c.as_os_str())
            .collect(),
    )
}
fn setting(s: &Store, k: &str) -> Result<Option<String>> {
    s.conn
        .query_row("SELECT value FROM settings WHERE key=?", [k], |r| r.get(0))
        .optional()
        .map_err(sql_err)
}
#[derive(Clone)]
pub struct Snapshot {
    files: Vec<FileRecord>,
    occupied: HashMap<String, String>,
    collections: Option<String>,
    folder: Option<String>,
    app_root: PathBuf,
}
pub fn snapshot(s: &Store, old: &str) -> Result<Snapshot> {
    let root = lexical(old)?;
    let mut files = vec![];
    for (fid, p) in s.all_paths()? {
        if relative(Path::new(&p), &root).is_some() {
            files.push(s.file(&fid)?);
            if files.len() > 1000 {
                return Err(err(
                    "REPAIR_LIMIT",
                    "每轮最多修复 1000 条记录，请选择更小的旧目录",
                ));
            }
        }
    }
    if files.is_empty() {
        return Err(err(
            "REPAIR_EMPTY",
            "此旧目录下没有有效文件记录，请核对记录中的原路径",
        ));
    }
    let mut st = s
        .conn
        .prepare("SELECT path,id FROM files")
        .map_err(sql_err)?;
    let rows = st
        .query_map([], |r| {
            Ok((key(&r.get::<_, String>(0)?), r.get::<_, String>(1)?))
        })
        .map_err(sql_err)?;
    let occupied = rows
        .collect::<std::result::Result<HashMap<_, _>, _>>()
        .map_err(sql_err)?;
    Ok(Snapshot {
        files,
        occupied,
        collections: setting(s, "savedCollections")?,
        folder: setting(s, "folderAutoTagging")?,
        app_root: s.root.clone(),
    })
}
#[derive(Clone)]
struct Candidate {
    id: String,
    meta: FileMeta,
    verified: bool,
    reason: String,
    hash: Option<String>,
    source: Option<(FileMeta, String)>,
}
#[derive(Clone)]
struct Row {
    file: FileRecord,
    candidates: Vec<Candidate>,
    status: String,
}
#[derive(Clone)]
pub struct Plan {
    token: String,
    created: i64,
    old: PathBuf,
    new: PathBuf,
    rows: Vec<Row>,
    collections: Option<String>,
    folder: Option<String>,
    mapped: String,
    mapped_count: usize,
    warnings: Vec<String>,
}
impl Plan {
    fn view(&self) -> Value {
        json!({"token":self.token,"oldRoot":self.old,"newRoot":self.new,"collectionCount":self.mapped_count,"warnings":self.warnings,
          "rows":self.rows.iter().map(|r|json!({"id":r.file.id,"name":r.file.name,"oldPath":r.file.path,"status":r.status,"candidates":r.candidates.iter().map(|c|json!({"id":c.id,"path":c.meta.path,"verified":c.verified,"reason":c.reason})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
    }
}
fn scan(root: &Path, app: &Path, cancel: &AtomicBool) -> Result<(Vec<FileMeta>, usize)> {
    let mut dirs = vec![(root.to_owned(), 0usize)];
    let mut files = vec![];
    let mut skipped = 0;
    let mut entries = 0;
    while let Some((dir, depth)) = dirs.pop() {
        check(cancel)?;
        let iter = match fs::read_dir(&dir) {
            Ok(d) => d,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        for e in iter {
            check(cancel)?;
            entries += 1;
            if entries > 20_000 {
                return Err(err(
                    "REPAIR_LIMIT",
                    "新目录超过 20,000 个项目，请缩小扫描范围",
                ));
            }
            let e = match e {
                Ok(e) => e,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            let meta = match fs::symlink_metadata(e.path()) {
                Ok(m) => m,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            if fsops::is_link(&meta) || relative(&e.path(), app).is_some() {
                skipped += 1;
                continue;
            }
            if meta.is_dir() {
                if depth >= 32 {
                    skipped += 1;
                } else {
                    dirs.push((e.path(), depth + 1));
                }
            } else if meta.is_file() {
                match fsops::inspect(&e.path()) {
                    Ok(m) => files.push(m),
                    Err(_) => skipped += 1,
                }
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((files, skipped))
}
fn digest(m: &FileMeta, budget: &mut u64, cancel: &AtomicBool) -> Result<Option<String>> {
    check(cancel)?;
    if m.bytes < 0 || m.bytes as u64 > 64 * 1024 * 1024 || m.bytes as u64 > *budget {
        return Ok(None);
    }
    let mut f = File::open(&m.path).map_err(|_| err("FILE_CHANGED", "文件读取失败，请重新扫描"))?;
    if fsops::revision(&f.metadata().map_err(|_| err("FILE_CHANGED", "文件变化"))?) != m.revision
        || fsops::identity(&f) != m.identity
    {
        return Err(err("FILE_CHANGED", "文件已变化，请重新扫描"));
    }
    let mut h = Sha256::new();
    let mut buf = [0; 128 * 1024];
    let mut total = 0u64;
    loop {
        check(cancel)?;
        let n = f
            .read(&mut buf)
            .map_err(|_| err("FILE_CHANGED", "文件读取失败"))?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > m.bytes as u64 {
            return Err(err("FILE_CHANGED", "文件读取期间变化"));
        }
        h.update(&buf[..n]);
    }
    if total != m.bytes as u64
        || fsops::revision(&f.metadata().map_err(|_| err("FILE_CHANGED", "文件变化"))?)
            != m.revision
    {
        return Err(err("FILE_CHANGED", "文件读取期间变化"));
    }
    *budget -= total;
    Ok(Some(format!("{:x}", h.finalize())))
}
pub fn preview(snapshot: Snapshot, old: &str, new: &str, cancel: &AtomicBool) -> Result<Plan> {
    let old = lexical(old)?;
    let new = PathBuf::from(fsops::canonical_display(&lexical(new)?)?);
    if !new.is_dir() || relative(&new, &snapshot.app_root).is_some() {
        return Err(err("INVALID_PATH", "请选择应用数据目录以外的新文件夹"));
    }
    if key(&old.to_string_lossy()) == key(&new.to_string_lossy()) {
        return Err(err("INVALID_PATH", "旧目录和新目录相同"));
    }
    let (inventory, skipped) = scan(&new, &snapshot.app_root, cancel)?;
    let mut warnings = vec![];
    if skipped > 0 {
        warnings.push(format!("跳过 {skipped} 个不可读、链接或超深度项目"));
    }
    let mut budget = 256 * 1024 * 1024;
    let mut rows = vec![];
    for f in snapshot.files {
        check(cancel)?;
        let expected = new.join(
            relative(Path::new(&f.path), &old)
                .ok_or_else(|| err("INVALID_PATH", "记录不在旧目录"))?,
        );
        let source = fsops::inspect(Path::new(&f.path))
            .ok()
            .filter(|m| m.identity == f.identity && m.revision == f.revision);
        let source_hash = source
            .as_ref()
            .map(|m| digest(m, &mut budget, cancel))
            .transpose()?
            .flatten();
        let mut possible: Vec<_> = inventory
            .iter()
            .filter(|m| {
                (!f.identity.is_empty() && m.identity == f.identity)
                    || key(&m.path) == key(&expected.to_string_lossy())
                    || (key(&m.name) == key(&f.name)
                        && m.bytes == f.bytes
                        && m.extension == f.extension)
            })
            .collect();
        possible.sort_by_key(|m| {
            if !f.identity.is_empty() && m.identity == f.identity {
                0
            } else if key(&m.path) == key(&expected.to_string_lossy()) {
                1
            } else {
                2
            }
        });
        let overflow = possible.len() > 8;
        let mut candidates = vec![];
        let mut collision = false;
        for m in possible.into_iter().take(8) {
            check(cancel)?;
            if key(&m.path) == key(&f.path) {
                continue;
            }
            if snapshot
                .occupied
                .get(&key(&m.path))
                .is_some_and(|id| id != &f.id)
            {
                collision = true;
                continue;
            }
            let identity = !f.identity.is_empty() && m.identity == f.identity;
            if !identity && (m.bytes != f.bytes || m.extension != f.extension) {
                collision = true;
                continue;
            }
            let compared = if let Some(_) = &source_hash {
                digest(m, &mut budget, cancel)?
            } else {
                None
            };
            if let (Some(a), Some(b)) = (&source_hash, &compared) {
                if a != b {
                    collision = true;
                    continue;
                }
            }
            let hash = compared;
            let verified = identity && m.revision == f.revision || hash.is_some();
            let reason = if identity && m.revision == f.revision {
                "文件身份一致"
            } else if hash.is_some() {
                "原文件与候选内容一致"
            } else if identity {
                "同一文件，但内容已变化；请确认"
            } else {
                "仅位置、名称及大小线索；内容未验证，请确认"
            };
            candidates.push(Candidate {
                id: id(),
                meta: m.clone(),
                verified,
                reason: reason.into(),
                hash,
                source: source
                    .as_ref()
                    .zip(source_hash.as_ref())
                    .map(|(m, h)| (m.clone(), h.clone())),
            });
        }
        let status = if candidates.is_empty() {
            if collision {
                "conflict"
            } else {
                "missing"
            }
        } else if candidates.len() > 1 || overflow {
            "ambiguous"
        } else if candidates[0].verified {
            "matched"
        } else {
            "confirm"
        };
        rows.push(Row {
            file: f,
            candidates,
            status: status.into(),
        });
    }
    let mut collections: Vec<Value> =
        serde_json::from_str(snapshot.collections.as_deref().unwrap_or("[]"))
            .map_err(|_| err("INVALID_INPUT", "保存的筛选数据无效"))?;
    let mut mapped_count = 0;
    for c in &mut collections {
        if let Some(p) = c["query"]["directory"]
            .as_str()
            .filter(|p| !p.is_empty())
            .and_then(|p| relative(Path::new(p), &old))
        {
            c["query"]["directory"] = json!(new.join(p));
            mapped_count += 1;
        }
    }
    Ok(Plan {
        token: id(),
        created: now(),
        old,
        new,
        rows,
        collections: snapshot.collections,
        folder: snapshot.folder,
        mapped: json!(collections).to_string(),
        mapped_count,
        warnings,
    })
}
pub fn verify(p: &Plan, v: &Value, cancel: &AtomicBool) -> Result<Vec<(FileRecord, FileMeta)>> {
    let choices = v["choices"]
        .as_array()
        .filter(|a| !a.is_empty() && a.len() <= 1000)
        .ok_or_else(|| err("INVALID_INPUT", "请选择需要修复的文件"))?;
    let mut ids = HashSet::new();
    let mut paths = HashSet::new();
    let mut out = vec![];
    let mut budget = 256 * 1024 * 1024;
    for choice in choices {
        check(cancel)?;
        let row = p
            .rows
            .iter()
            .find(|r| Some(r.file.id.as_str()) == choice["id"].as_str())
            .ok_or_else(|| err("INVALID_INPUT", "文件不在预览中"))?;
        let c = row
            .candidates
            .iter()
            .find(|c| Some(c.id.as_str()) == choice["candidate"].as_str())
            .ok_or_else(|| err("INVALID_INPUT", "候选不在预览中"))?;
        if !c.verified && choice["confirmed"].as_bool() != Some(true) {
            return Err(err("REPAIR_CONFIRM_REQUIRED", "未验证的候选需要逐项确认"));
        }
        if !ids.insert(row.file.id.clone()) || !paths.insert(key(&c.meta.path)) {
            return Err(err("PATH_ALREADY_LINKED", "多条记录不能关联同一文件"));
        }
        let fresh = fsops::inspect(Path::new(&c.meta.path))?;
        if fresh.path != c.meta.path
            || fresh.revision != c.meta.revision
            || fresh.identity != c.meta.identity
        {
            return Err(err("FILE_CHANGED", "候选文件已变化，请重新扫描"));
        }
        if let Some(h) = &c.hash {
            if digest(&fresh, &mut budget, cancel)?.as_ref() != Some(h) {
                return Err(err("FILE_CHANGED", "候选内容已变化，请重新扫描"));
            }
            if let Some((source, hash)) = &c.source {
                if digest(source, &mut budget, cancel)?.as_ref() != Some(hash) {
                    return Err(err("FILE_CHANGED", "原文件内容已变化，请重新扫描"));
                }
            }
        }
        out.push((row.file.clone(), fresh));
    }
    Ok(out)
}
pub fn apply(
    s: &mut Store,
    p: &Plan,
    v: &Value,
    checked: Vec<(FileRecord, FileMeta)>,
) -> Result<Value> {
    if now() - p.created >= 15 * 60 * 1000 {
        return Err(err("PREVIEW_EXPIRED", "预览已过期，请重新扫描"));
    }
    if setting(s, "savedCollections")? != p.collections
        || setting(s, "folderAutoTagging")? != p.folder
    {
        return Err(err("VERSION_CONFLICT", "设置已变化，请重新扫描"));
    }
    for (old, m) in &checked {
        let current = s.file(&old.id)?;
        if current.version != old.version
            || current.path != old.path
            || current.revision != old.revision
            || current.identity != old.identity
        {
            return Err(err("VERSION_CONFLICT", "文件记录已变化，请重新扫描"));
        }
        let fresh = fsops::inspect(Path::new(&m.path))?;
        if fresh.path != m.path || fresh.revision != m.revision || fresh.identity != m.identity {
            return Err(err("FILE_CHANGED", "候选文件已变化，请重新扫描"));
        }
    }
    s.conn
        .execute_batch("SAVEPOINT path_repair_batch")
        .map_err(sql_err)?;
    let outcome = (|| {
        let mut entries = vec![];
        for (f, m) in &checked {
            entries
                .push(s.relink_operation(&json!({"id":f.id,"version":f.version,"path":m.path}))?);
        }
        let mut undo = Undo {
            id: uuid::Uuid::new_v4().to_string(),
            label: format!("修复 {} 个文件路径", checked.len()),
            steps: vec![],
            guards: vec![],
        };
        for entry in entries.into_iter().rev() {
            undo.steps.extend(entry.steps);
            undo.guards.extend(entry.guards);
        }
        let map = v["mapCollections"].as_bool().unwrap_or(true) && p.mapped_count > 0;
        if map {
            s.conn.execute("INSERT INTO settings(key,value) VALUES('savedCollections',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[&p.mapped]).map_err(sql_err)?;
            undo.steps.push(if let Some(old) = &p.collections {
                Step {
                    sql: "UPDATE settings SET value=? WHERE key='savedCollections' AND value=?"
                        .into(),
                    args: vec![sv(old), sv(&p.mapped)],
                }
            } else {
                Step {
                    sql: "DELETE FROM settings WHERE key='savedCollections' AND value=?".into(),
                    args: vec![sv(&p.mapped)],
                }
            });
        }
        Ok((
            undo,
            json!({"repaired":checked.len(),"collections":if map{p.mapped_count}else{0},"oldRoot":p.old,"newRoot":p.new,"mapCollections":v["mapCollections"].as_bool().unwrap_or(true)}),
        ))
    })();
    match outcome {
        Ok((undo, value)) => {
            s.conn
                .execute_batch("RELEASE path_repair_batch")
                .map_err(sql_err)?;
            s.remember(undo);
            Ok(value)
        }
        Err(e) => {
            let _ = s
                .conn
                .execute_batch("ROLLBACK TO path_repair_batch; RELEASE path_repair_batch");
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        _dir: crate::fsops::TmpDir,
        old: PathBuf,
        new: PathBuf,
        store: Store,
        ids: Vec<String>,
    }
    impl Fixture {
        fn new(count: usize) -> Self {
            let dir = crate::fsops::TmpDir::new();
            let old = dir.path().join("原目录");
            let new = dir.path().join("新目录");
            fs::create_dir(&old).unwrap();
            let mut store = Store::open(&dir.path().join("library")).unwrap();
            let mut ids = vec![];
            for i in 0..count {
                let p = old.join(format!("资料{i}.txt"));
                fs::write(&p, format!("内容{i}")).unwrap();
                store
                    .add_file_with_ai(&fsops::inspect(&p).unwrap(), false)
                    .unwrap();
                ids.push(
                    store
                        .all_paths()
                        .unwrap()
                        .into_iter()
                        .find(|(_, path)| Path::new(path).file_name() == p.file_name())
                        .unwrap()
                        .0,
                );
            }
            Self {
                _dir: dir,
                old,
                new,
                store,
                ids,
            }
        }
        fn plan(&self) -> Plan {
            preview(
                snapshot(&self.store, self.old.to_str().unwrap()).unwrap(),
                self.old.to_str().unwrap(),
                self.new.to_str().unwrap(),
                &AtomicBool::new(false),
            )
            .unwrap()
        }
        fn all(p: &Plan) -> Value {
            json!({"mapCollections":true,"choices":p.rows.iter().filter(|r|!r.candidates.is_empty()).map(|r|json!({"id":r.file.id,"candidate":r.candidates[0].id,"confirmed":true})).collect::<Vec<_>>()})
        }
        fn commit(&mut self, p: &Plan, v: &Value) -> Result<Value> {
            let checked = verify(p, v, &AtomicBool::new(false))?;
            apply(&mut self.store, p, v, checked)
        }
    }
    #[test]
    fn directory_move_preserves_annotations_maps_collections_and_undo() {
        let mut f = Fixture::new(3);
        let id = f.ids[0].clone();
        let tag = f.store.create_tag("保留标注").unwrap();
        f.store
            .conn
            .execute(
                "INSERT INTO file_tags(file_id,tag_id,source) VALUES(?,?,'manual')",
                [&id, &tag.id],
            )
            .unwrap();
        f.store
            .conn
            .execute(
                "UPDATE files SET note='备注',note_key='备注',favorite=1 WHERE id=?",
                [&id],
            )
            .unwrap();
        let old = json!([{"id":"saved","name":"目录筛选","query":{"directory":f.old}}]).to_string();
        f.store
            .conn
            .execute(
                "INSERT INTO settings(key,value) VALUES('savedCollections',?)",
                [&old],
            )
            .unwrap();
        fs::rename(&f.old, &f.new).unwrap();
        let p = f.plan();
        assert!(p.rows.iter().all(|r| r.status == "matched"));
        assert_eq!(p.mapped_count, 1);
        let v = Fixture::all(&p);
        assert_eq!(f.commit(&p, &v).unwrap()["repaired"], 3);
        let file = f.store.file(&id).unwrap();
        assert_eq!(file.note, "备注");
        assert!(file.favorite);
        assert_eq!(Path::new(&file.parent), f.new);
        assert!(file.tags.iter().any(|t| t.id == tag.id));
        assert!(file
            .tags
            .iter()
            .any(|t| t.name == "新目录" && t.source == "folder"));
        assert!(setting(&f.store, "savedCollections")
            .unwrap()
            .unwrap()
            .contains("新目录"));
        assert_eq!(f.store.undo.len(), 1);
        f.store.undo_last().unwrap();
        assert_eq!(Path::new(&f.store.file(&id).unwrap().parent), f.old);
        assert_eq!(setting(&f.store, "savedCollections").unwrap(), Some(old));
        assert!(f
            .store
            .file(&id)
            .unwrap()
            .tags
            .iter()
            .any(|t| t.name == "原目录" && t.source == "folder"));
    }
    #[test]
    fn renamed_file_is_found_by_identity_and_copy_is_verified_by_hash() {
        let mut f = Fixture::new(1);
        fs::create_dir(&f.new).unwrap();
        fs::copy(f.old.join("资料0.txt"), f.new.join("资料0.txt")).unwrap();
        let p = f.plan();
        assert_eq!(p.rows[0].status, "matched");
        assert!(p.rows[0].candidates[0].hash.is_some());
        fs::write(f.new.join("资料0.txt"), "错内0").unwrap();
        assert!(f.commit(&p, &Fixture::all(&p)).is_err());
        let p = f.plan();
        assert_eq!(p.rows[0].status, "conflict");
        fs::rename(f.old.join("资料0.txt"), f.new.join("改名.txt")).unwrap();
        let p = f.plan();
        assert!(p.rows[0]
            .candidates
            .iter()
            .any(|c| c.verified && c.meta.name == "改名.txt"));
    }
    #[test]
    fn missing_source_requires_confirmation_and_ambiguous_choice() {
        let mut f = Fixture::new(1);
        fs::remove_file(f.old.join("资料0.txt")).unwrap();
        for sub in ["甲", "乙"] {
            let d = f.new.join(sub);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("资料0.txt"), "内容0").unwrap();
        }
        let p = f.plan();
        assert_eq!(p.rows[0].status, "ambiguous");
        assert_eq!(p.rows[0].candidates.len(), 2);
        let mut v = Fixture::all(&p);
        v["choices"][0]["confirmed"] = json!(false);
        assert!(f
            .commit(&p, &v)
            .unwrap_err()
            .starts_with("REPAIR_CONFIRM_REQUIRED"));
        v["choices"][0]["candidate"] = json!("forged");
        assert!(f.commit(&p, &v).is_err());
        assert_eq!(f.commit(&p, &Fixture::all(&p)).unwrap()["repaired"], 1);
    }
    #[test]
    fn stale_preview_and_target_change_are_rejected() {
        let mut f = Fixture::new(1);
        fs::rename(&f.old, &f.new).unwrap();
        let p = f.plan();
        f.store
            .conn
            .execute("UPDATE files SET version=version+1 WHERE id=?", [&f.ids[0]])
            .unwrap();
        assert!(f
            .commit(&p, &Fixture::all(&p))
            .unwrap_err()
            .starts_with("VERSION_CONFLICT"));
        let p = f.plan();
        fs::write(f.new.join("资料0.txt"), "后续内容变化").unwrap();
        assert!(f.commit(&p, &Fixture::all(&p)).is_err());
        assert_eq!(f.store.undo.len(), 0);
        assert_eq!(Path::new(&f.store.file(&f.ids[0]).unwrap().parent), f.old);
    }
    #[test]
    fn batch_collision_rolls_back_first_update_and_undo_history() {
        let mut f = Fixture::new(2);
        fs::rename(&f.old, &f.new).unwrap();
        let p = f.plan();
        let v = Fixture::all(&p);
        let last = p.rows.last().unwrap().candidates[0].meta.clone();
        f.store.add_file_with_ai(&last, false).unwrap();
        assert!(f
            .commit(&p, &v)
            .unwrap_err()
            .starts_with("PATH_ALREADY_LINKED"));
        for id in &f.ids {
            assert_eq!(Path::new(&f.store.file(id).unwrap().parent), f.old);
        }
        assert_eq!(f.store.undo.len(), 0);
    }
    #[test]
    fn saved_filter_settings_conflict_and_later_changes_survive_undo() {
        let mut f = Fixture::new(1);
        let old = json!([{"query":{"directory":f.old}}]).to_string();
        f.store
            .conn
            .execute(
                "INSERT INTO settings(key,value) VALUES('savedCollections',?)",
                [&old],
            )
            .unwrap();
        fs::rename(&f.old, &f.new).unwrap();
        let p = f.plan();
        f.store
            .conn
            .execute(
                "UPDATE settings SET value='[]' WHERE key='savedCollections'",
                [],
            )
            .unwrap();
        assert!(f
            .commit(&p, &Fixture::all(&p))
            .unwrap_err()
            .starts_with("VERSION_CONFLICT"));
        f.store
            .conn
            .execute(
                "UPDATE settings SET value=? WHERE key='savedCollections'",
                [old],
            )
            .unwrap();
        let p = f.plan();
        f.commit(&p, &Fixture::all(&p)).unwrap();
        f.store
            .conn
            .execute(
                "UPDATE settings SET value='[]' WHERE key='savedCollections'",
                [],
            )
            .unwrap();
        f.store.undo_last().unwrap();
        assert_eq!(
            setting(&f.store, "savedCollections").unwrap(),
            Some("[]".into())
        );
    }
    #[test]
    fn cancellation_expiry_and_component_boundaries() {
        let f = Fixture::new(1);
        fs::create_dir(&f.new).unwrap();
        assert!(preview(
            snapshot(&f.store, f.old.to_str().unwrap()).unwrap(),
            f.old.to_str().unwrap(),
            f.new.to_str().unwrap(),
            &AtomicBool::new(true)
        )
        .unwrap_err_text()
        .starts_with("REPAIR_CANCELLED"));
        assert!(relative(&f.old.with_file_name("原目录2").join("a"), &f.old).is_none());
        assert!(lexical("relative/path").is_err());
        let control = Control::default();
        let mut p = f.plan();
        p.created = now() - 16 * 60 * 1000;
        let token = p.token.clone();
        control.remember(p).unwrap();
        assert!(control.get(&token).is_err());
        let failed: Result<()> = control.run(|| Err(err("TEST", "取消")));
        assert!(failed.is_err());
        assert!(!control.busy.load(Ordering::SeqCst));
    }
    // Avoid Debug requirements for a deliberately private plan with source metadata.
    trait ErrorText {
        fn unwrap_err_text(self) -> String;
    }
    impl ErrorText for Result<Plan> {
        fn unwrap_err_text(self) -> String {
            match self {
                Err(e) => e,
                Ok(_) => panic!("expected error"),
            }
        }
    }
}
