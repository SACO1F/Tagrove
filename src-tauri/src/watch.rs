//! Bounded polling reconciliation. No native event subscription or filesystem mutation.
use crate::{db::Store, fsops, model::*, path_repair::relative};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
};
const MAX_RULES: usize = 8;
const MAX_ENTRIES: usize = 20_000;
const BATCH: usize = 64;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    pub path: String,
    pub recursive: bool,
    pub ignore: Vec<String>,
    pub ai: bool,
    pub paused: bool,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    host: String,
    rules: Vec<Rule>,
}
fn host(s: &Store) -> String {
    format!(
        "{}:{}",
        std::env::var("COMPUTERNAME").unwrap_or_default(),
        key(&s.root.to_string_lossy())
    )
}
fn raw(s: &Store) -> Result<String> {
    Ok(s.conn
        .query_row(
            "SELECT value FROM settings WHERE key='watchFolders'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql_err)?
        .unwrap_or_default())
}
fn version(raw: &str) -> String {
    format!("{:x}", Sha256::digest(raw.as_bytes()))
}
fn valid_shape(r: &Rule) -> bool {
    !r.id.is_empty()
        && r.id.len() <= 128
        && r.path.len() <= 16384
        && Path::new(&r.path).is_absolute()
        && !Path::new(&r.path)
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        && r.ignore.len() <= 64
        && r.ignore.iter().all(|p| {
            !p.is_empty()
                && p.chars().count() <= 128
                && !Path::new(p).is_absolute()
                && !p.replace('\\', "/").split('/').any(|s| s == "..")
        })
}
pub fn rules(s: &Store) -> Result<(Vec<Rule>, String)> {
    let raw = raw(s)?;
    let mut saved: Saved = if raw.is_empty() {
        Saved::default()
    } else {
        serde_json::from_str(&raw).map_err(|_| {
            err(
                "WATCH_CONFIG_INVALID",
                "监听规则无法读取，请检查备份或重新配置",
            )
        })?
    };
    if saved.rules.len() > MAX_RULES
        || saved.rules.iter().any(|r| !valid_shape(r))
        || saved
            .rules
            .iter()
            .map(|r| &r.id)
            .collect::<HashSet<_>>()
            .len()
            != saved.rules.len()
    {
        return Err(err("WATCH_CONFIG_INVALID", "监听规则超出范围"));
    }
    if saved.host != host(s) {
        for r in &mut saved.rules {
            r.paused = true;
        }
    }
    Ok((saved.rules, version(&raw)))
}
fn put(s: &Store, rules: Vec<Rule>) -> Result<()> {
    s.conn.execute("INSERT INTO settings(key,value) VALUES('watchFolders',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[serde_json::to_string(&Saved{host:host(s),rules}).map_err(|e|err("INVALID_INPUT",e))?]).map_err(sql_err)?;
    Ok(())
}
pub fn pause_all(s: &Store) -> Result<()> {
    let (mut rs, _) = rules(s).unwrap_or_default();
    for r in &mut rs {
        r.paused = true;
    }
    put(s, rs)
}
pub fn change(s: &Store, action: &str, v: &Value) -> Result<()> {
    let (mut rs, ver) = rules(s)?;
    if v["version"].as_str() != Some(ver.as_str()) {
        return Err(err("VERSION_CONFLICT", "监听规则已变化，请刷新后重试"));
    }
    match action {
        "watch.save" => {
            let mut r: Rule = serde_json::from_value(v["rule"].clone())
                .map_err(|_| err("INVALID_INPUT", "监听规则无效"))?;
            if r.id.is_empty() {
                r.id = id();
            } else if !rs.iter().any(|old| old.id == r.id) {
                return Err(err("NOT_FOUND", "监听规则不存在"));
            }
            r.ignore = r
                .ignore
                .iter()
                .map(|x| x.trim().replace('\\', "/"))
                .filter(|x| !x.is_empty())
                .collect();
            r.ignore.sort();
            r.ignore.dedup();
            if !valid_shape(&r) {
                return Err(err(
                    "INVALID_INPUT",
                    "最多 64 条忽略规则，每条 128 字；请输入绝对目录",
                ));
            }
            let input = Path::new(&r.path);
            let meta = fs::symlink_metadata(input)
                .map_err(|_| err("WATCH_OFFLINE", "目录不可用，不能保存新规则"))?;
            if !meta.is_dir() || fsops::is_link(&meta) {
                return Err(err("INVALID_PATH", "请选择实际文件夹，不支持链接目录"));
            }
            r.path = fsops::canonical_display(input)?;
            if relative(Path::new(&r.path), &s.root).is_some() {
                return Err(err("APP_DATA_EXCLUDED", "不能监听应用数据目录"));
            }
            if rs.iter().any(|old| {
                old.id != r.id
                    && (relative(Path::new(&old.path), Path::new(&r.path)).is_some()
                        || relative(Path::new(&r.path), Path::new(&old.path)).is_some())
            }) {
                return Err(err(
                    "WATCH_OVERLAP",
                    "监听目录不能重复或互相包含，请编辑已有规则",
                ));
            }
            if let Some(old) = rs.iter_mut().find(|old| old.id == r.id) {
                *old = r;
            } else {
                if rs.len() >= MAX_RULES {
                    return Err(err("LIMIT_REACHED", "最多监听 8 个目录"));
                }
                rs.push(r);
            }
        }
        "watch.pause" => {
            let id = str_arg(v, "id")?;
            let r = rs
                .iter_mut()
                .find(|r| r.id == id)
                .ok_or_else(|| err("NOT_FOUND", "监听规则不存在"))?;
            r.paused = v["paused"]
                .as_bool()
                .ok_or_else(|| err("INVALID_INPUT", "缺少暂停状态"))?;
        }
        "watch.remove" => {
            let id = str_arg(v, "id")?;
            if !rs.iter().any(|r| r.id == id) {
                return Err(err("NOT_FOUND", "监听规则不存在"));
            }
            rs.retain(|r| r.id != id);
        }
        _ => return Err(err("INVALID_INPUT", "未知监听操作")),
    }
    put(s, rs)
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub id: String,
    pub state: String,
    pub scanned: usize,
    pub pending: usize,
    pub added: usize,
    pub moved: usize,
    pub changed: usize,
    pub missing: usize,
    pub skipped: usize,
    pub last_scan: i64,
    pub errors: Vec<String>,
}
#[derive(Default)]
pub struct Control {
    pub generation: AtomicU64,
    pub force: AtomicBool,
    pub stopped: AtomicBool,
    pub gate: Mutex<()>,
    status: Mutex<HashMap<String, Status>>,
}
impl Control {
    pub fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.force.store(true, Ordering::SeqCst);
    }
    pub fn cancelled(&self, epoch: u64) -> bool {
        self.stopped.load(Ordering::SeqCst) || self.generation.load(Ordering::SeqCst) != epoch
    }
    pub fn publish(&self, s: Status) {
        if let Ok(mut all) = self.status.lock() {
            all.insert(s.id.clone(), s);
        }
    }
    pub fn list(&self, s: &Store) -> Result<Value> {
        let (rs, version) = rules(s)?;
        let mut all = self
            .status
            .lock()
            .map_err(|_| err("INTERNAL_ERROR", "监听状态锁异常"))?;
        all.retain(|id, _| rs.iter().any(|r| &r.id == id));
        Ok(
            json!({"version":version,"rules":rs.iter().map(|r|{let mut status=all.get(&r.id).cloned().unwrap_or_else(||Status{id:r.id.clone(),state:"waiting".into(),..Default::default()});if r.paused{status.state="paused".into();status.pending=0;}json!({"rule":r,"status":status})}).collect::<Vec<_>>()}),
        )
    }
}
// '*' matches zero or more characters including '/', '?' one character. Matching is case-folded.
fn wildcard(pattern: &str, text: &str) -> bool {
    let p: Vec<_> = key(pattern).chars().collect();
    let t: Vec<_> = key(text).chars().collect();
    let (mut i, mut j, mut star, mut mark) = (0, 0, None, 0);
    while j < t.len() {
        if i < p.len() && (p[i] == '?' || p[i] == t[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            star = Some(i);
            i += 1;
            mark = j;
        } else if let Some(s) = star {
            mark += 1;
            j = mark;
            i = s + 1;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == '*' {
        i += 1;
    }
    i == p.len()
}
impl Rule {
    fn ignored(&self, rel: &str) -> bool {
        let rel = rel.replace('\\', "/");
        self.ignore
            .iter()
            .any(|p| wildcard(p, &rel) || rel.split('/').any(|part| wildcard(p, part)))
    }
    pub fn includes(&self, path: &Path) -> bool {
        relative(path, Path::new(&self.path)).is_some_and(|rel| {
            !rel.as_os_str().is_empty()
                && (self.recursive || rel.components().count() == 1)
                && !self.ignored(&rel.to_string_lossy())
        })
    }
}
#[derive(Clone, Debug)]
pub struct Entry {
    pub path: String,
    pub revision: String,
}
pub struct Scan {
    pub entries: Vec<Entry>,
    pub seen: usize,
    pub skipped: usize,
    pub incomplete: bool,
}
pub fn scan(r: &Rule, app_root: &Path, cancel: impl Fn() -> bool) -> Result<Scan> {
    let root = Path::new(&r.path);
    let meta = fs::symlink_metadata(root)
        .map_err(|_| err("WATCH_OFFLINE", "监听目录暂不可用，保留全部标注"))?;
    if !meta.is_dir()
        || fsops::is_link(&meta)
        || fsops::canonical_display(root)
            .map(|p| key(&p) != key(&r.path))
            .unwrap_or(true)
        || relative(root, app_root).is_some()
    {
        return Err(err(
            "WATCH_OFFLINE",
            "目录位置变化或为链接，请检查后修改规则",
        ));
    }
    let mut out = Scan {
        entries: vec![],
        seen: 0,
        skipped: 0,
        incomplete: false,
    };
    let mut stack = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        if cancel() {
            return Err(err("WATCH_CANCELLED", "监听扫描已停止"));
        }
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => {
                out.incomplete = true;
                out.skipped += 1;
                continue;
            }
        };
        for entry in entries {
            if cancel() {
                return Err(err("WATCH_CANCELLED", "监听扫描已停止"));
            }
            out.seen += 1;
            if out.seen > MAX_ENTRIES {
                return Err(err("WATCH_LIMIT", "目录超过 20,000 个条目，请缩小监听范围"));
            }
            let e = match entry {
                Ok(e) => e,
                Err(_) => {
                    out.incomplete = true;
                    out.skipped += 1;
                    continue;
                }
            };
            let p = e.path();
            let rel = relative(&p, root).ok_or_else(|| err("INVALID_PATH", "扫描越出目录范围"))?;
            if p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(".shiqian-edit-"))
                || r.ignored(&rel.to_string_lossy())
                || relative(&p, app_root).is_some()
            {
                out.skipped += 1;
                continue;
            }
            let m = match fs::symlink_metadata(&p) {
                Ok(m) => m,
                Err(_) => {
                    out.incomplete = true;
                    out.skipped += 1;
                    continue;
                }
            };
            if fsops::is_link(&m) || fsops::is_hidden(&m) {
                out.skipped += 1;
                continue;
            }
            if m.is_dir() {
                if r.recursive && depth < 32 {
                    stack.push((p, depth + 1));
                } else {
                    out.skipped += 1;
                    if r.recursive {
                        out.incomplete = true;
                    }
                }
            } else if m.is_file() {
                out.entries.push(Entry {
                    path: p.to_string_lossy().into(),
                    revision: fsops::revision(&m),
                });
            }
            if out.seen % 200 == 0 {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
    }
    out.entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}
#[derive(Clone)]
pub struct Known {
    pub id: String,
    pub path: String,
    pub revision: String,
    pub status: String,
    pub removed: bool,
}
pub fn known(s: &Store, r: &Rule) -> Result<Vec<Known>> {
    // Narrow candidates by indexed parent; includes() remains the authority for
    // component boundaries, recursive mode and ignore rules. SQLite NOCASE only
    // folds ASCII, so retain the full scan for roots with Unicode case variants.
    let unicode_case = r
        .path
        .chars()
        .any(|c| !c.is_ascii() && c.to_lowercase().to_string() != c.to_uppercase().to_string());
    let parent = Path::new(&r.path)
        .to_string_lossy()
        .replace('/', std::path::MAIN_SEPARATOR_STR);
    let parent = parent.trim_end_matches(['\\', '/']);
    let prefix = format!("{parent}{}", std::path::MAIN_SEPARATOR);
    let upper = format!(
        "{parent}{}",
        char::from_u32(std::path::MAIN_SEPARATOR as u32 + 1).unwrap()
    );
    let (sql, args) = if unicode_case {
        (
            "SELECT id,path,revision,status,removed_at IS NOT NULL FROM files",
            vec![],
        )
    } else if r.recursive {
        ("SELECT id,path,revision,status,removed_at IS NOT NULL FROM files WHERE parent=? COLLATE NOCASE OR (parent>=? COLLATE NOCASE AND parent<? COLLATE NOCASE)", vec![parent.to_owned(),prefix,upper])
    } else {
        ("SELECT id,path,revision,status,removed_at IS NOT NULL FROM files WHERE parent=? COLLATE NOCASE",vec![if parent.is_empty() || (cfg!(windows) && parent.ends_with(':')) {prefix} else {parent.into()}])
    };
    let mut stmt = s.conn.prepare_cached(sql).map_err(sql_err)?;
    let mut rows = stmt
        .query(rusqlite::params_from_iter(args))
        .map_err(sql_err)?;
    let mut out = vec![];
    while let Some(row) = rows.next().map_err(sql_err)? {
        let k = Known {
            id: row.get(0).map_err(sql_err)?,
            path: row.get(1).map_err(sql_err)?,
            revision: row.get(2).map_err(sql_err)?,
            status: row.get(3).map_err(sql_err)?,
            removed: row.get(4).map_err(sql_err)?,
        };
        if r.includes(Path::new(&k.path)) {
            out.push(k);
            if out.len() > MAX_ENTRIES {
                return Err(err("WATCH_LIMIT", "目录关联记录超过 20,000 条，请缩小范围"));
            }
        }
    }
    Ok(out)
}
struct Observation {
    revision: String,
    first: i64,
    seen: usize,
    done: bool,
}
pub struct Runtime {
    pub rule: Rule,
    pub status: Status,
    observed: HashMap<String, Observation>,
    pub missing_cursor: usize,
    pending_cursor: usize,
}
impl Runtime {
    pub fn new(rule: Rule) -> Self {
        Self {
            status: Status {
                id: rule.id.clone(),
                state: "waiting".into(),
                ..Default::default()
            },
            rule,
            observed: HashMap::new(),
            missing_cursor: 0,
            pending_cursor: 0,
        }
    }
    pub fn reset(&mut self) {
        self.observed.clear();
        self.status.pending = 0;
    }
    pub fn observe(&mut self, scan: &Scan, known: &[Known], at: i64) -> Vec<Entry> {
        let seen: HashSet<_> = scan.entries.iter().map(|e| &e.path).collect();
        self.observed.retain(|p, _| seen.contains(p));
        let existing: HashMap<_, _> = known.iter().map(|k| (&k.path, k)).collect();
        let mut ready = vec![];
        for e in &scan.entries {
            let o = self
                .observed
                .entry(e.path.clone())
                .or_insert_with(|| Observation {
                    revision: e.revision.clone(),
                    first: at,
                    seen: 0,
                    done: existing.get(&e.path).is_some_and(|k| {
                        k.removed || (k.revision == e.revision && k.status == "available")
                    }),
                });
            if o.revision != e.revision {
                *o = Observation {
                    revision: e.revision.clone(),
                    first: at,
                    seen: 0,
                    done: false,
                };
            }
            o.seen += 1;
            if !o.done && o.seen >= 2 && at - o.first >= 3000 {
                ready.push(e.clone());
            }
        }
        self.status.pending = self.observed.values().filter(|o| !o.done).count();
        self.status.scanned = scan.seen;
        self.status.skipped = scan.skipped;
        if !ready.is_empty() {
            let offset = self.pending_cursor % ready.len();
            ready.rotate_left(offset);
            self.pending_cursor = (offset + BATCH) % ready.len();
        }
        ready.truncate(BATCH);
        ready
    }
    pub fn done(&mut self, path: &str) {
        if let Some(o) = self.observed.get_mut(path) {
            o.done = true;
        }
        self.status.pending = self.observed.values().filter(|o| !o.done).count();
    }
}
#[derive(Debug, PartialEq)]
pub enum Applied {
    Added,
    Moved,
    Changed,
    Unchanged,
    Skipped,
}
pub fn apply_file(s: &mut Store, r: &Rule, e: &Entry) -> Result<Applied> {
    let m = fsops::inspect(Path::new(&e.path))?;
    if m.revision != e.revision {
        return Err(err("FILE_CHANGED", "文件仍在写入，下一轮重试"));
    }
    if !r.includes(Path::new(&m.path)) || relative(Path::new(&m.path), &s.root).is_some() {
        return Err(err("APP_DATA_EXCLUDED", "文件已越出监听范围"));
    }
    let old: Option<(String, bool)> = s
        .conn
        .query_row(
            "SELECT id,removed_at IS NOT NULL FROM files WHERE path_key=?",
            [&m.path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(sql_err)?;
    if let Some((id, removed)) = old {
        if removed {
            return Ok(Applied::Skipped);
        }
        let before = s.file(&id)?;
        let after = s.refresh_file(&id)?;
        if after.identity != m.identity || after.status != "available" {
            return Err(err(
                "FILE_REPLACED",
                "原位置被其他文件占用，请使用路径修复确认",
            ));
        }
        return Ok(
            if before.revision != after.revision || before.status != after.status {
                Applied::Changed
            } else {
                Applied::Unchanged
            },
        );
    }
    if !m.identity.is_empty() {
        let matches:Vec<(String,String,bool,String)>=s.conn.prepare("SELECT id,path,removed_at IS NOT NULL,revision FROM files WHERE identity=? LIMIT 3").map_err(sql_err)?.query_map([&m.identity],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).map_err(sql_err)?.collect::<std::result::Result<_,_>>().map_err(sql_err)?;
        if !matches.is_empty() {
            if matches.len() != 1 {
                return Err(err("WATCH_AMBIGUOUS", "同身份记录不唯一，请用路径修复确认"));
            }
            let (id, old, removed, revision) = &matches[0];
            if *removed {
                return Ok(Applied::Skipped);
            }
            if !r.includes(Path::new(old)) || Path::new(old).exists() || revision != &m.revision {
                return Err(err(
                    "WATCH_NEEDS_REPAIR",
                    "已有关联记录；位置或内容变化需通过路径修复确认",
                ));
            }
            let file = s.file(id)?;
            let _undo =
                s.relink_operation(&json!({"id":id,"path":m.path,"version":file.version}))?;
            return Ok(Applied::Moved);
        }
    }
    Ok(if s.add_file_with_ai(&m, r.ai)? {
        Applied::Added
    } else {
        Applied::Unchanged
    })
}
pub fn refresh_missing(s: &mut Store, k: &Known) -> Result<bool> {
    if k.removed {
        return Ok(false);
    }
    let before = s.file(&k.id)?;
    if before.path != k.path {
        return Ok(false);
    }
    let after = s.refresh_file(&k.id)?;
    Ok(before.status != after.status || before.revision != after.revision)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    struct Fixture {
        _dir: crate::fsops::TmpDir,
        s: Store,
        root: PathBuf,
        r: Rule,
    }
    impl Fixture {
        fn new() -> Self {
            let d = crate::fsops::TmpDir::new();
            let root = d.path().join("监听资料");
            fs::create_dir(&root).unwrap();
            let s = Store::open(&d.path().join("library")).unwrap();
            let r = Rule {
                id: "rule".into(),
                path: fsops::canonical_display(&root).unwrap(),
                recursive: true,
                ignore: vec!["*.tmp".into(), "~$*".into(), "跳过目录".into()],
                ai: false,
                paused: false,
            };
            Self {
                _dir: d,
                s,
                root,
                r,
            }
        }
        fn write(&self, name: &str, body: &str) -> Entry {
            let p = self.root.join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, body).unwrap();
            let m = fsops::inspect(&p).unwrap();
            Entry {
                path: m.path,
                revision: m.revision,
            }
        }
        fn save(&self, mut rule: Rule) {
            rule.id = String::new();
            let (_, v) = rules(&self.s).unwrap();
            change(&self.s, "watch.save", &json!({"version":v,"rule":rule})).unwrap();
        }
        fn file(&self, path: &str) -> FileRecord {
            let id: String = self
                .s
                .conn
                .query_row("SELECT id FROM files WHERE path=?", [path], |r| r.get(0))
                .unwrap();
            self.s.file(&id).unwrap()
        }
    }
    #[test]
    fn rules_conflict_overlap_pause_remove_and_persistence() {
        let f = Fixture::new();
        let (_, before) = rules(&f.s).unwrap();
        f.save(f.r.clone());
        let (rs, v) = rules(&f.s).unwrap();
        assert_eq!(rs.len(), 1);
        assert!(change(
            &f.s,
            "watch.pause",
            &json!({"id":rs[0].id,"version":before,"paused":true})
        )
        .unwrap_err()
        .starts_with("VERSION_CONFLICT"));
        let mut other = f.r.clone();
        other.id = "".into();
        other.path = f.root.join("子目录").to_string_lossy().into();
        fs::create_dir(&other.path).unwrap();
        assert!(
            change(&f.s, "watch.save", &json!({"version":v,"rule":other}))
                .unwrap_err()
                .starts_with("WATCH_OVERLAP")
        );
        change(
            &f.s,
            "watch.pause",
            &json!({"id":rs[0].id,"version":v,"paused":true}),
        )
        .unwrap();
        let (rs, v) = rules(&f.s).unwrap();
        assert!(rs[0].paused);
        let reopen = Store::open(&f.s.root).unwrap();
        assert_eq!(rules(&reopen).unwrap().0, rs);
        change(&f.s, "watch.remove", &json!({"id":rs[0].id,"version":v})).unwrap();
        assert!(rules(&f.s).unwrap().0.is_empty());
    }
    #[test]
    fn scan_recursive_ignore_unicode_scope_appdata_and_cancellation() {
        let f = Fixture::new();
        f.write("普通.txt", "a");
        f.write("缓存.tmp", "x");
        f.write("~$临时.docx", "x");
        f.write("子目录/正文.txt", "b");
        f.write("跳过目录/不要.txt", "c");
        let scan = super::scan(&f.r, &f.s.root, || false).unwrap();
        assert_eq!(scan.entries.len(), 2);
        assert!(wildcard("图?.JPG", "图片.jpg"));
        assert!(wildcard("素材/**", "素材/子/图.png"));
        let mut r = f.r.clone();
        r.recursive = false;
        assert_eq!(
            super::scan(&r, &f.s.root, || false).unwrap().entries.len(),
            1
        );
        assert!(!r.includes(&f.root.join("子目录/正文.txt")));
        assert!(!r.includes(&f.root.with_file_name("监听资料其他").join("x.txt")));
        assert!(super::scan(&f.r, &f.s.root, || true)
            .err()
            .unwrap()
            .starts_with("WATCH_CANCELLED"));
        let mut all = f.r.clone();
        all.path = f._dir.path().to_string_lossy().into();
        assert!(super::scan(&all, &f.s.root, || false)
            .unwrap()
            .entries
            .iter()
            .all(|e| relative(Path::new(&e.path), &f.s.root).is_none()));
    }
    #[test]
    fn debounce_changed_file_reset_batch_limit_and_dedup() {
        let f = Fixture::new();
        for i in 0..70 {
            f.write(&format!("{i:03}.txt"), "a");
        }
        let scan = super::scan(&f.r, &f.s.root, || false).unwrap();
        let mut rt = Runtime::new(f.r.clone());
        assert!(rt.observe(&scan, &[], 0).is_empty());
        assert!(rt.observe(&scan, &[], 1000).is_empty());
        let ready = rt.observe(&scan, &[], 4000);
        assert_eq!(ready.len(), BATCH);
        for e in ready {
            rt.done(&e.path);
        }
        assert_eq!(rt.observe(&scan, &[], 5000).len(), 6);
        rt.reset();
        assert!(rt.observe(&scan, &[], 10000).is_empty());
        f.write("000.txt", "changed-size");
        let changed = super::scan(&f.r, &f.s.root, || false).unwrap();
        assert!(!rt
            .observe(&changed, &[], 14000)
            .iter()
            .any(|e| e.path.ends_with("000.txt")));
        assert!(rt
            .observe(&changed, &[], 18000)
            .iter()
            .any(|e| e.path.ends_with("000.txt")));
    }
    #[test]
    fn watch_import_idempotence_removed_records_and_ai_opt_in() {
        let mut f = Fixture::new();
        f.s.save_ai_settings(&json!({"config":{"enabled":true,"endpoint":"http://127.0.0.1:11434/v1","model":"local","allowNewTags":true}})).unwrap();
        let e = f.write("普通.txt", "manual policy");
        assert_eq!(apply_file(&mut f.s, &f.r, &e).unwrap(), Applied::Added);
        let a = f.file(&e.path);
        assert!(f.s.ai_task(&a.id).unwrap().is_none());
        assert_eq!(apply_file(&mut f.s, &f.r, &e).unwrap(), Applied::Unchanged);
        f.s.mutate(
            "files.remove",
            &json!({"ids":[a.id],"versions":{a.id.clone():a.version}}),
        )
        .unwrap();
        assert_eq!(apply_file(&mut f.s, &f.r, &e).unwrap(), Applied::Skipped);
        let count: i64 =
            f.s.conn
                .query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))
                .unwrap();
        assert_eq!(count, 1);
        let mut ai = f.r.clone();
        ai.ai = true;
        let e = f.write("AI.txt", "ai policy");
        apply_file(&mut f.s, &ai, &e).unwrap();
        let a = f.file(&e.path);
        assert_eq!(f.s.ai_task(&a.id).unwrap().unwrap().status, "queued");
    }
    #[test]
    fn same_identity_rename_keeps_annotations_and_changed_missing_preserve_records() {
        let mut f = Fixture::new();
        let e = f.write("旧名.txt", "content");
        apply_file(&mut f.s, &f.r, &e).unwrap();
        let a = f.file(&e.path);
        let tag = f.s.create_tag("保留标签").unwrap();
        f.s.mutate(
            "files.tags",
            &json!({"ids":[a.id],"versions":{a.id.clone():a.version},"add":[tag.id],"remove":[]}),
        )
        .unwrap();
        let a = f.s.file(&a.id).unwrap();
        f.s.mutate(
            "files.favorite",
            &json!({"ids":[a.id],"versions":{a.id.clone():a.version},"value":true}),
        )
        .unwrap();
        f.s.save_note(&json!({"id":a.id,"version":a.note_version,"text":"保留备注"}))
            .unwrap();
        let new = f.root.join("子目录/新名.txt");
        fs::create_dir(new.parent().unwrap()).unwrap();
        fs::rename(&e.path, &new).unwrap();
        let m = fsops::inspect(&new).unwrap();
        assert_eq!(
            apply_file(
                &mut f.s,
                &f.r,
                &Entry {
                    path: m.path.clone(),
                    revision: m.revision
                }
            )
            .unwrap(),
            Applied::Moved
        );
        let a = f.s.file(&a.id).unwrap();
        assert_eq!(a.path, m.path);
        assert_eq!(a.note, "保留备注");
        assert!(a.favorite);
        assert!(a.tags.iter().any(|t| t.id == tag.id));
        let changed = f.write("子目录/新名.txt", "longer content");
        assert_eq!(
            apply_file(&mut f.s, &f.r, &changed).unwrap(),
            Applied::Changed
        );
        let k = known(&f.s, &f.r).unwrap().pop().unwrap();
        fs::rename(&k.path, f.root.join("隔离改名.txt")).unwrap();
        assert!(refresh_missing(&mut f.s, &k).unwrap());
        let a = f.s.file(&a.id).unwrap();
        assert_eq!(a.status, "missing");
        assert_eq!(a.note, "保留备注");
        assert!(a.favorite);
    }
    #[test]
    fn replacement_changed_snapshot_and_hardlink_never_reassign() {
        let mut f = Fixture::new();
        let e = f.write("原件.txt", "old");
        apply_file(&mut f.s, &f.r, &e).unwrap();
        let a = f.file(&e.path);
        let second = f.root.join("硬链接.txt");
        fs::hard_link(&e.path, &second).unwrap();
        let m = fsops::inspect(&second).unwrap();
        assert!(apply_file(
            &mut f.s,
            &f.r,
            &Entry {
                path: m.path,
                revision: m.revision
            }
        )
        .unwrap_err()
        .starts_with("WATCH_NEEDS_REPAIR"));
        fs::rename(&e.path, f.root.join("保留原件.txt")).unwrap();
        let replaced = f.write("原件.txt", "different file");
        assert!(apply_file(&mut f.s, &f.r, &replaced)
            .unwrap_err()
            .starts_with("FILE_REPLACED"));
        assert_eq!(f.s.file(&a.id).unwrap().identity, a.identity);
        let changed = f.write("写入中.txt", "initial");
        f.write("写入中.txt", "larger changed content");
        assert!(apply_file(&mut f.s, &f.r, &changed)
            .unwrap_err()
            .starts_with("FILE_CHANGED"));
    }
    #[test]
    fn off_scope_appdata_patterns_and_foreign_host_fail_closed() {
        let f = Fixture::new();
        let (_, v) = rules(&f.s).unwrap();
        let mut r = f.r.clone();
        r.id = "".into();
        r.path = f.s.root.to_string_lossy().into();
        assert!(change(&f.s, "watch.save", &json!({"version":v,"rule":r}))
            .unwrap_err()
            .starts_with("APP_DATA_EXCLUDED"));
        let mut r = f.r.clone();
        r.id = "".into();
        r.ignore = vec!["../*".into()];
        assert!(change(&f.s, "watch.save", &json!({"version":v,"rule":r})).is_err());
        f.s.conn
            .execute(
                "INSERT INTO settings VALUES('watchFolders',?)",
                [serde_json::to_string(&Saved {
                    host: "other-machine".into(),
                    rules: vec![f.r.clone()],
                })
                .unwrap()],
            )
            .unwrap();
        assert!(rules(&f.s).unwrap().0[0].paused);
        let control = Control::default();
        let epoch = control.generation.load(Ordering::SeqCst);
        assert!(!control.cancelled(epoch));
        control.invalidate();
        assert!(control.cancelled(epoch));
        assert!(control.force.load(Ordering::SeqCst));
    }
    #[test]
    fn restore_pauses_automation_and_rule_removal_keeps_files() {
        let mut f = Fixture::new();
        f.save(f.r.clone());
        let e = f.write("保留资料.txt", "keep");
        apply_file(&mut f.s, &f.r, &e).unwrap();
        let a = f.file(&e.path);
        let backup = f._dir.path().join("backup.sqtagbackup");
        f.s.export_backup(&backup).unwrap();
        f.s.restore_backup(&backup).unwrap();
        assert!(rules(&f.s).unwrap().0.iter().all(|r| r.paused));
        let (rs, v) = rules(&f.s).unwrap();
        change(&f.s, "watch.remove", &json!({"id":rs[0].id,"version":v})).unwrap();
        assert_eq!(f.s.file(&a.id).unwrap().path, e.path);
        assert!(Path::new(&e.path).exists());
    }
}
