//! Rebuildable, local-only text index. Never writes original files or annotations.
use crate::{db::Store, fsops, model::*};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::{fs::File, io::Read, path::Path};

pub const READ_LIMIT: usize = 2 * 1024 * 1024;
const TEXT_BUDGET: i64 = 64 * 1024 * 1024;
pub const PARSER: i64 = 3;

// The writer owns its connections. WAL readers and hand annotation APIs never
// wait on the application's Store mutex while a large FTS record is inserted.
#[cfg(test)]
pub fn commit_detached(root: &Path, work: &Work, text: Result<Text>) -> Result<bool> {
    writer(root)?.search_commit(work, text)
}
pub fn writer(root: &Path) -> Result<Store> {
    let conn = rusqlite::Connection::open(root.join("library.sqlite")).map_err(sql_err)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(sql_err)?;
    conn.execute(
        "ATTACH DATABASE ? AS search",
        [root
            .join("cache/search/index.sqlite")
            .to_string_lossy()
            .as_ref()],
    )
    .map_err(sql_err)?;
    conn.execute_batch("PRAGMA search.secure_delete=ON")
        .map_err(sql_err)?;
    Ok(Store {
        conn,
        root: root.into(),
        undo: vec![],
        recovery_notice: String::new(),
    })
}

pub fn attach(store: &Store) -> Result<()> {
    match attach_once(store) {
        Ok(()) => Ok(()),
        Err(e) if e.contains("not a database") || e.contains("malformed") => {
            let _ = store.conn.execute_batch("DETACH DATABASE search");
            let dir = store.root.join("cache/search");
            for name in ["index.sqlite", "index.sqlite-wal", "index.sqlite-shm"] {
                let source = dir.join(name);
                if source.exists() {
                    std::fs::rename(&source, dir.join(format!("{name}.corrupt-{}", now())))
                        .map_err(|e| err("STORAGE_UNAVAILABLE", e))?;
                }
            }
            attach_once(store)
        }
        Err(e) => Err(e),
    }
}
fn attach_once(store: &Store) -> Result<()> {
    let dir = store.root.join("cache").join("search");
    std::fs::create_dir_all(&dir).map_err(|e| err("STORAGE_UNAVAILABLE", e))?;
    store
        .conn
        .execute(
            "ATTACH DATABASE ? AS search",
            [dir.join("index.sqlite").to_string_lossy().as_ref()],
        )
        .map_err(sql_err)?;
    store.conn.execute_batch("PRAGMA search.journal_mode=WAL; PRAGMA search.secure_delete=ON;
        CREATE TABLE IF NOT EXISTS search.control(id INTEGER PRIMARY KEY CHECK(id=1),enabled INTEGER NOT NULL DEFAULT 0,state TEXT NOT NULL DEFAULT 'disabled',generation INTEGER NOT NULL DEFAULT 0);
        INSERT OR IGNORE INTO search.control(id) VALUES(1);
        CREATE TABLE IF NOT EXISTS search.docs(id TEXT UNIQUE NOT NULL,path TEXT NOT NULL,revision TEXT NOT NULL,identity TEXT NOT NULL,parser INTEGER NOT NULL,state TEXT NOT NULL,error TEXT NOT NULL DEFAULT '',body TEXT NOT NULL DEFAULT '',body_key TEXT NOT NULL DEFAULT '',text_bytes INTEGER NOT NULL DEFAULT 0,encoding TEXT NOT NULL DEFAULT '',truncated INTEGER NOT NULL DEFAULT 0,updated_at INTEGER NOT NULL DEFAULT 0);
        CREATE INDEX IF NOT EXISTS search.docs_state ON docs(state,id);
        CREATE VIRTUAL TABLE IF NOT EXISTS search.body_fts USING fts5(body_key, tokenize='trigram');
        UPDATE search.docs SET state='queued' WHERE state='running';").map_err(sql_err)?;
    let mut st = store
        .conn
        .prepare("PRAGMA search.table_info(docs)")
        .map_err(sql_err)?;
    let names = st
        .query_map([], |r| r.get::<_, String>(1))
        .map_err(sql_err)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(sql_err)?;
    drop(st);
    for (name, default) in [("locations", "'[]'"), ("coverage", "''")] {
        if !names.iter().any(|n| n == name) {
            store
                .conn
                .execute_batch(&format!(
                    "ALTER TABLE search.docs ADD COLUMN {name} TEXT NOT NULL DEFAULT {default}"
                ))
                .map_err(sql_err)?;
        }
    }
    Ok(())
}

#[derive(Clone)]
pub struct Work {
    pub file: FileRecord,
    pub generation: i64,
}
pub struct Text {
    pub(crate) body: String,
    pub(crate) normalized: String,
    pub(crate) encoding: String,
    pub(crate) truncated: bool,
    pub(crate) locations: Vec<Value>,
    pub(crate) coverage: String,
}

impl Store {
    pub fn search_current(&self, generation: i64) -> Result<bool> {
        self.conn.query_row("SELECT enabled=1 AND state='active' AND generation=? FROM search.control WHERE id=1", [generation], |r| r.get(0)).map_err(sql_err)
    }
    pub fn search_enabled(&self) -> Result<bool> {
        self.conn
            .query_row("SELECT enabled FROM search.control WHERE id=1", [], |r| {
                r.get(0)
            })
            .map_err(sql_err)
    }
    pub fn search_control(&self, action: &str) -> Result<Value> {
        let (enabled, state) = match action {
            "enable" | "resume" | "rebuild" => (true, "active"),
            "pause" => (self.search_enabled()?, "paused"),
            "cancel" => (self.search_enabled()?, "cancelled"),
            "disable" | "clear" => (false, "disabled"),
            _ => return Err(err("INVALID_INPUT", "不支持的索引操作")),
        };
        self.conn
            .execute_batch("SAVEPOINT search_control;")
            .map_err(sql_err)?;
        let result = (|| {
            self.conn.execute("UPDATE search.control SET enabled=?,state=?,generation=generation+1 WHERE id=1", params![enabled,state]).map_err(sql_err)?;
            self.conn
                .execute(
                    "UPDATE search.docs SET state='queued' WHERE state='running'",
                    [],
                )
                .map_err(sql_err)?;
            if matches!(action, "clear" | "rebuild") {
                self.conn
                    .execute_batch("DROP TABLE search.body_fts; CREATE VIRTUAL TABLE search.body_fts USING fts5(body_key, tokenize='trigram'); DELETE FROM search.docs;")
                    .map_err(sql_err)?;
            }
            Ok(json_ok())
        })();
        match result {
            Ok(v) => {
                self.conn
                    .execute_batch("RELEASE search_control")
                    .map_err(sql_err)?;
                if matches!(action, "clear" | "rebuild") {
                    self.conn
                        .execute_batch("PRAGMA search.wal_checkpoint(TRUNCATE); VACUUM search;")
                        .map_err(sql_err)?;
                }
                Ok(v)
            }
            Err(e) => {
                let _ = self
                    .conn
                    .execute_batch("ROLLBACK TO search_control; RELEASE search_control");
                Err(e)
            }
        }
    }
    pub fn search_status(&self) -> Result<Value> {
        let (enabled, state): (bool, String) = self
            .conn
            .query_row(
                "SELECT enabled,state FROM search.control WHERE id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(sql_err)?;
        let eligible: i64 = self.conn.query_row("SELECT count(*) FROM files WHERE removed_at IS NULL AND status='available' AND extension IN ('txt','md','markdown','docx','xlsx','csv','tsv','pdf','sqweb')",[],|r|r.get(0)).map_err(sql_err)?;
        let mut counts = serde_json::Map::new();
        let mut st = self.conn.prepare("SELECT d.state,count(*) FROM search.docs d JOIN files f ON f.id=d.id WHERE f.removed_at IS NULL AND f.status='available' AND d.revision=f.revision AND d.identity=f.identity AND d.path=f.path AND d.parser=3 GROUP BY d.state").map_err(sql_err)?;
        for row in st
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map_err(sql_err)?
        {
            let (k, v) = row.map_err(sql_err)?;
            counts.insert(k, json!(v));
        }
        let known: i64 = counts.values().filter_map(Value::as_i64).sum();
        counts.insert("stale".into(), json!((eligible - known).max(0)));
        let truncated: i64 = self.conn.query_row("SELECT count(*) FROM search.docs d JOIN files f ON f.id=d.id WHERE f.removed_at IS NULL AND d.state='done' AND d.truncated=1 AND d.revision=f.revision AND d.identity=f.identity AND d.path=f.path AND d.parser=3",[],|r|r.get(0)).map_err(sql_err)?;
        let root = self.root.join("cache/search");
        let bytes: u64 = ["index.sqlite", "index.sqlite-wal", "index.sqlite-shm"]
            .iter()
            .filter_map(|n| std::fs::metadata(root.join(n)).ok())
            .map(|m| m.len())
            .sum();
        let mut st=self.conn.prepare("SELECT f.name,d.error FROM search.docs d JOIN files f ON f.id=d.id WHERE f.removed_at IS NULL AND d.state='failed' ORDER BY d.updated_at DESC LIMIT 5").map_err(sql_err)?;
        let errors = st
            .query_map([], |r| {
                Ok(json!({"name":r.get::<_,String>(0)?,"error":r.get::<_,String>(1)?}))
            })
            .map_err(sql_err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql_err)?;
        Ok(
            json!({"enabled":enabled,"state":state,"eligible":eligible,"counts":counts,"truncated":truncated,"bytes":bytes,"readLimit":READ_LIMIT,"textBudget":TEXT_BUDGET,"errors":errors}),
        )
    }
    // One bounded page per tick, only records already in the library.
    pub fn search_next(&self) -> Result<Option<Work>> {
        let (enabled, state, generation): (bool, String, i64) = self
            .conn
            .query_row(
                "SELECT enabled,state,generation FROM search.control WHERE id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(sql_err)?;
        if !enabled || state != "active" {
            return Ok(None);
        }
        let fid: Option<String>=self.conn.query_row("SELECT f.id FROM files f LEFT JOIN search.docs d ON f.id=d.id WHERE f.removed_at IS NULL AND f.status='available' AND f.extension IN ('txt','md','markdown','docx','xlsx','csv','tsv','pdf','sqweb') AND (d.id IS NULL OR d.revision<>f.revision OR d.identity<>f.identity OR d.path<>f.path OR d.parser<>? OR d.state='queued') ORDER BY f.id LIMIT 1",[PARSER],|r|r.get(0)).optional().map_err(sql_err)?;
        let Some(fid) = fid else {
            self.conn.execute_batch("DELETE FROM search.body_fts WHERE rowid IN (SELECT d.rowid FROM search.docs d WHERE NOT EXISTS(SELECT 1 FROM files f WHERE f.id=d.id AND f.removed_at IS NULL)); DELETE FROM search.docs WHERE NOT EXISTS(SELECT 1 FROM files f WHERE f.id=docs.id AND f.removed_at IS NULL);").map_err(sql_err)?;
            return Ok(None);
        };
        let file = self.file_metadata(&fid)?;
        self.conn.execute("INSERT INTO search.docs(id,path,revision,identity,parser,state) VALUES(?,?,?,?,?,'running') ON CONFLICT(id) DO UPDATE SET path=excluded.path,revision=excluded.revision,identity=excluded.identity,parser=excluded.parser,state='running',error=''",params![file.id,file.path,file.revision,file.identity,PARSER]).map_err(sql_err)?;
        Ok(Some(Work { file, generation }))
    }
    pub fn search_commit(&self, work: &Work, text: Result<Text>) -> Result<bool> {
        let (enabled, state, generation): (bool, String, i64) = self
            .conn
            .query_row(
                "SELECT enabled,state,generation FROM search.control WHERE id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(sql_err)?;
        if !enabled || state != "active" || generation != work.generation {
            return Ok(false);
        }
        let f = match self.file_metadata(&work.file.id) {
            Ok(f) => f,
            Err(_) => return Ok(false),
        };
        if f.revision != work.file.revision
            || f.identity != work.file.identity
            || f.path != work.file.path
        {
            return Ok(false);
        }
        if text.is_ok() {
            let actual = fsops::inspect(Path::new(&f.path))?;
            if actual.revision != f.revision || actual.identity != f.identity {
                return Ok(false);
            }
        }
        let rowid: i64 = self
            .conn
            .query_row("SELECT rowid FROM search.docs WHERE id=?", [&f.id], |r| {
                r.get(0)
            })
            .map_err(sql_err)?;
        let used: i64 = self
            .conn
            .query_row(
                "SELECT coalesce(sum(text_bytes),0) FROM search.docs WHERE id<>?",
                [&f.id],
                |r| r.get(0),
            )
            .map_err(sql_err)?;
        let text = text.and_then(|t| {
            if used
                + (t.body.len()
                    + t.normalized.len()
                    + json!(t.locations).to_string().len()
                    + t.coverage.len()) as i64
                > TEXT_BUDGET
            {
                Err(err("INDEX_FULL", "正文容量已达 64 MiB，请清理或重建索引"))
            } else {
                Ok(t)
            }
        });
        self.conn
            .execute_batch("SAVEPOINT search_commit")
            .map_err(sql_err)?;
        let result = (|| {
            // Obtain only the derived database's writer lock, then re-check the
            // command generation. Pause/rebuild may have arrived during parsing.
            self.conn
                .execute(
                    "UPDATE search.control SET generation=generation WHERE id=1",
                    [],
                )
                .map_err(sql_err)?;
            let valid: bool=self.conn.query_row("SELECT enabled=1 AND state='active' AND generation=? FROM search.control WHERE id=1",[work.generation],|r|r.get(0)).map_err(sql_err)?;
            if !valid {
                return Ok(false);
            }
            self.conn
                .execute("DELETE FROM search.body_fts WHERE rowid=?", [rowid])
                .map_err(sql_err)?;
            match text {
                Ok(t) => {
                    let normalized = t.normalized;
                    let locations =
                        serde_json::to_string(&t.locations).map_err(|e| err("INDEX_INVALID", e))?;
                    let bytes =
                        (t.body.len() + normalized.len() + locations.len() + t.coverage.len())
                            as i64;
                    self.conn.execute("UPDATE search.docs SET body=?,body_key=?,text_bytes=?,encoding=?,truncated=?,locations=?,coverage=?,state='done',error='',updated_at=? WHERE id=?",params![t.body,normalized,bytes,t.encoding,t.truncated,locations,t.coverage,now(),f.id]).map_err(sql_err)?;
                    self.conn
                        .execute(
                            "INSERT INTO search.body_fts(rowid,body_key) VALUES(?,?)",
                            params![rowid, normalized],
                        )
                        .map_err(sql_err)?;
                }
                Err(e) => {
                    self.conn.execute("UPDATE search.docs SET body='',body_key='',text_bytes=0,locations='[]',coverage='',state='failed',error=?,updated_at=? WHERE id=?",params![e,now(),f.id]).map_err(sql_err)?;
                }
            }
            Ok(true)
        })();
        match result {
            Ok(v) => {
                self.conn
                    .execute_batch("RELEASE search_commit")
                    .map_err(sql_err)?;
                Ok(v)
            }
            Err(e) => {
                let _ = self
                    .conn
                    .execute_batch("ROLLBACK TO search_commit; RELEASE search_commit");
                Err(e)
            }
        }
    }
    pub fn search_snippet(&self, f: &FileRecord, terms: &[String]) -> Result<Option<Value>> {
        if terms.is_empty() {
            return Ok(None);
        }
        let term = terms.first().map(String::as_str).unwrap_or("");
        let found: Option<(String,String,String,bool,i64,i64,i64)> = self.conn.query_row(
            "SELECT locations,coverage,encoding,truncated,updated_at,length(body),instr(body_key,?) FROM search.docs WHERE id=? AND revision=? AND identity=? AND path=? AND parser=? AND state='done'",
            params![term,f.id,f.revision,f.identity,f.path,PARSER], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional().map_err(sql_err)?;
        let Some((locations, coverage, encoding, truncated, indexed, length, mut hit)) = found
        else {
            return Ok(None);
        };
        let locations: Vec<Value> =
            serde_json::from_str(&locations).map_err(|e| err("INDEX_INVALID", e))?;
        let mut best: Option<(usize, Value)> = None;
        for (term_index, candidate) in terms.iter().enumerate() {
            if term_index > 0 {
                hit = self
                    .conn
                    .query_row(
                        "SELECT instr(body_key,?) FROM search.docs WHERE id=?",
                        params![candidate, f.id],
                        |r| r.get(0),
                    )
                    .map_err(sql_err)?;
            }
            if hit <= 0 {
                continue;
            }
            let location = locations.iter().find(|l| {
                let start = l["keyStart"].as_i64().unwrap_or(1);
                hit >= start && hit < start + l["keyLength"].as_i64().unwrap_or(0)
            });
            let low = location.and_then(|l| l["start"].as_i64()).unwrap_or(1);
            let high = location
                .and_then(|l| l["length"].as_i64())
                .map(|n| low + n)
                .unwrap_or(length + 1);
            let adjusted = location
                .map(|l| low + (hit - l["keyStart"].as_i64().unwrap_or(low)).max(0))
                .unwrap_or(hit);
            let start = (adjusted - 35).max(low).min(high.max(low));
            let count = 150.min((high - start).max(0));
            let snippet: String = self
                .conn
                .query_row(
                    "SELECT substr(body,?,?) FROM search.docs WHERE id=?",
                    params![start, count, f.id],
                    |r| r.get(0),
                )
                .map_err(sql_err)?;
            let normalized = key(&snippet);
            let matched: Vec<_> = terms
                .iter()
                .filter(|t| normalized.contains(t.as_str()))
                .collect();
            let score = matched.len();
            if best.as_ref().is_some_and(|(old, _)| *old >= score) {
                continue;
            }
            let source = location.and_then(|l| l["label"].as_str()).unwrap_or("正文");
            best = Some((
                score,
                json!({"snippet":format!("{}{}{}",if start>low{"…"}else{""},snippet,if high>start+count{"…"}else{""}),"source":source,"location":location,"coverage":coverage,"encoding":encoding,"truncated":truncated,"indexedAt":indexed,"characterOffset":start-1,"matchedTerms":matched,"queryTerms":terms.len()}),
            ));
            if score == terms.len() {
                break;
            }
        }
        Ok(best.map(|(_, value)| value))
    }

    pub fn search_probe(&self, after: &str) -> Result<Vec<FileRecord>> {
        let mut st=self.conn.prepare("SELECT id FROM files WHERE removed_at IS NULL AND extension IN ('txt','md','markdown','docx','xlsx','csv','tsv','pdf','sqweb') AND id>? ORDER BY id LIMIT 16").map_err(sql_err)?;
        let ids = st
            .query_map([after], |r| r.get::<_, String>(0))
            .map_err(sql_err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql_err)?;
        ids.iter().map(|id| self.file_metadata(id)).collect()
    }
}

pub fn terms(text: &str) -> Result<Vec<String>> {
    if text.chars().count() > 256 {
        return Err(err("SEARCH_TOO_LONG", "正文搜索最多 256 个字符"));
    }
    let terms: Vec<_> = text.split_whitespace().map(key).collect();
    if terms.len() > 16 {
        return Err(err("SEARCH_TOO_LONG", "正文搜索最多 16 个关键词"));
    }
    Ok(terms)
}
pub fn fts_phrase(term: &str) -> String {
    format!("\"{}\"", term.replace('"', "\"\""))
}
#[cfg(test)]
pub fn read(work: &Work) -> Result<Text> {
    read_controlled(work, || Ok(true))
}
pub fn read_controlled(work: &Work, current: impl FnMut() -> Result<bool>) -> Result<Text> {
    let f = &work.file;
    if let Some(source) = &f.web_source {
        source.validate()?;
        let body = source.text();
        return Ok(Text {
            normalized: key(&body),
            body,
            encoding: "UTF-8".into(),
            truncated: false,
            locations: vec![],
            coverage: "已保存的网页标题、链接与摘录".into(),
        });
    }

    if f.extension.eq_ignore_ascii_case("pdf") {
        return crate::pdf_index::indexed(f, current);
    }
    if matches!(
        f.extension.to_lowercase().as_str(),
        "docx" | "xlsx" | "csv" | "tsv"
    ) {
        return crate::office::indexed(f);
    }
    let mut file = File::open(&f.path).map_err(|e| err("ACCESS_DENIED", e))?;
    let before = file.metadata().map_err(|e| err("ACCESS_DENIED", e))?;
    if fsops::revision(&before) != f.revision || fsops::identity(&file) != f.identity {
        return Err(err("FILE_CHANGED", "文件已改变，等待重新检查"));
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take((READ_LIMIT + 4) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| err("ACCESS_DENIED", e))?;
    let truncated = before.len() > READ_LIMIT as u64;
    bytes.truncate(READ_LIMIT);
    let (body, encoding) = if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let decoder = if bytes[0] == 0xff {
            encoding_rs::UTF_16LE
        } else {
            encoding_rs::UTF_16BE
        };
        let mut end = bytes.len() - ((bytes.len() - 2) % 2);
        if truncated && end >= 4 {
            let unit = if bytes[0] == 0xff {
                u16::from_le_bytes([bytes[end - 2], bytes[end - 1]])
            } else {
                u16::from_be_bytes([bytes[end - 2], bytes[end - 1]])
            };
            if (0xd800..=0xdbff).contains(&unit) {
                end -= 2;
            }
        }
        let (text, _, errors) = decoder.decode(&bytes[2..end]);
        if errors {
            return Err(err("ENCODING_UNSUPPORTED", "UTF-16 解码失败"));
        }
        (text.into_owned(), decoder.name().into())
    } else {
        let data = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
        match std::str::from_utf8(data) {
            Ok(t) => (t.into(), "UTF-8".into()),
            Err(e) if truncated && e.error_len().is_none() => (
                String::from_utf8_lossy(&data[..e.valid_up_to()]).into_owned(),
                "UTF-8".into(),
            ),
            Err(_) => {
                let (text, _, errors) = encoding_rs::GB18030.decode(data);
                if errors {
                    return Err(err(
                        "ENCODING_UNSUPPORTED",
                        "无法按 UTF-8、UTF-16 或 GB18030 解码",
                    ));
                }
                (text.into_owned(), "GB18030".into())
            }
        }
    };
    if body.contains('\0') {
        return Err(err("BINARY_FILE", "检测到二进制内容，未建立正文索引"));
    }
    if fsops::revision(&file.metadata().map_err(|e| err("ACCESS_DENIED", e))?) != f.revision {
        return Err(err("FILE_CHANGED", "读取期间文件改变"));
    }
    Ok(Text {
        normalized: key(&body),
        body,
        encoding,
        truncated,
        locations: vec![],
        coverage: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        dir: crate::fsops::TmpDir,
        store: Store,
    }
    impl Fixture {
        fn new() -> Self {
            let dir = crate::fsops::TmpDir::new();
            let store = Store::open(&dir.path().join("library")).unwrap();
            store.search_control("enable").unwrap();
            Self { dir, store }
        }
        fn add(&mut self, name: &str, body: &[u8]) -> FileRecord {
            let p = self.dir.path().join(name);
            std::fs::write(&p, body).unwrap();
            self.store
                .add_file_with_ai(&fsops::inspect(&p).unwrap(), false)
                .unwrap();
            let fid: String = self
                .store
                .conn
                .query_row(
                    "SELECT id FROM files WHERE path=?",
                    [fsops::canonical_display(&p).unwrap()],
                    |r| r.get(0),
                )
                .unwrap();
            self.store.file(&fid).unwrap()
        }
        fn drain(&self) {
            while let Some(w) = self.store.search_next().unwrap() {
                assert!(self.store.search_commit(&w, read(&w)).unwrap());
            }
        }
        fn query(&self, text: &str) -> Value {
            self.store
                .query(&Query {
                    text: text.into(),
                    search_mode: "content".into(),
                    limit: 100,
                    ..Default::default()
                })
                .unwrap()
        }
    }
    #[test]
    fn query_baseline_32_exact_names_and_bound_evidence() {
        let mut f = Fixture::new();
        let queries = [
            "学习资料",
            "卷子",
            "风景",
            "建筑外墙",
            "室内",
            "数学",
            "语文",
            "英语",
            "物理",
            "化学",
            "历史",
            "地理",
            "生物",
            "政治",
            "学习路线",
            "项目规划",
            "设计",
            "参考素材",
            "会议记录",
            "预算",
            "合同",
            "方案",
            "报告",
            "旅行",
            "人物",
            "动物",
            "插画",
            "线稿",
            "黑白",
            "工作",
            "备考",
            "季度总结",
        ];
        for q in queries {
            f.add(&format!("{}.txt", q), "合成测试正文".as_bytes());
            f.add(&format!("旧版{}参考.txt", q), "合成测试正文".as_bytes());
        }
        f.drain();
        for q in queries {
            let result = f
                .store
                .query(&Query {
                    text: q.into(),
                    search_mode: "unified".into(),
                    sort: "relevance".into(),
                    limit: 5,
                    ..Default::default()
                })
                .unwrap();
            assert_eq!(result["files"][0]["name"], format!("{}.txt", q));
            assert_eq!(result["files"][0]["searchMatch"]["queryText"], q);
            assert_eq!(
                result["files"][0]["searchMatch"]["sources"][0]["kind"],
                "filename"
            );
            assert_eq!(
                result["files"][0]["searchMatch"]["sources"][0]["exact"],
                true
            );
        }
        let empty = f
            .store
            .query(&Query {
                search_mode: "unified".into(),
                limit: 100,
                ..Default::default()
            })
            .unwrap();
        assert!(empty["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v.get("searchMatch").is_none() && v.get("contentMatch").is_none()));
    }
    #[test]
    fn snippet_prefers_more_distinct_query_terms() {
        let mut f = Fixture::new();
        let text = format!("苹果{}蜂鸟与苹果一起出现。", "普通正文。".repeat(400));
        f.add("证据.txt", text.as_bytes());
        f.drain();
        let result = f
            .store
            .query(&Query {
                text: "苹果 蜂鸟".into(),
                search_mode: "content".into(),
                limit: 10,
                ..Default::default()
            })
            .unwrap();
        let clip = &result["files"][0]["contentMatch"];
        assert!(clip["snippet"].as_str().unwrap().contains("蜂鸟"));
        assert_eq!(clip["matchedTerms"].as_array().unwrap().len(), 2);
        assert_eq!(clip["queryTerms"], 2);
    }
    #[test]
    fn cross_field_search_and_scope_keep_actual_body_anchor() {
        let mut f = Fixture::new();
        let mixed = f.add("学习资料.txt", "数学卷子与答案。".as_bytes());
        let body = f.add("正文.txt", "学习方法和卷子练习".as_bytes());
        f.drain();
        let q = Query {
            text: "学习 卷子".into(),
            search_mode: "unified".into(),
            sort: "relevance".into(),
            limit: 100,
            ..Default::default()
        };
        let r = f.store.query(&q).unwrap();
        assert_eq!(r["total"], 2);
        assert_eq!(r["files"][0]["id"], mixed.id);
        assert!(r["files"][0]["contentMatch"]["snippet"]
            .as_str()
            .unwrap()
            .contains("卷子"));
        assert_eq!(r["files"][0]["contentMatch"]["queryText"], "学习 卷子");
        assert_eq!(
            f.store
                .query(&Query {
                    search_scope: "name".into(),
                    ..q.clone()
                })
                .unwrap()["total"],
            0
        );
        let scoped = f
            .store
            .query(&Query {
                search_scope: "body".into(),
                ..q.clone()
            })
            .unwrap();
        assert_eq!(scoped["total"], 1);
        assert_eq!(scoped["files"][0]["id"], body.id);
        f.store.search_control("disable").unwrap();
        assert_eq!(f.store.query(&q).unwrap()["total"], 0);
        assert!(f
            .store
            .query(&Query {
                search_scope: "body".into(),
                ..q.clone()
            })
            .is_err());
        assert_eq!(
            f.store
                .query(&Query {
                    search_scope: "body".into(),
                    text: String::new(),
                    ..q
                })
                .unwrap()["total"],
            2
        );
    }
    #[test]
    fn relevance_exact_name_then_alias_then_substring_then_body() {
        let mut f = Fixture::new();
        let exact = f.add("卷子.txt", "普通正文".as_bytes());
        let name = f.add("旧版卷子.txt", "普通正文".as_bytes());
        let alias = f.add("资料.txt", "普通正文".as_bytes());
        let body = f.add("正文.txt", "数学卷子答案".as_bytes());
        f.drain();
        let tag = f.store.create_tag("试卷").unwrap();
        f.store.import_aliases(&tag.id, &["卷子".into()]).unwrap();
        f.store
            .conn
            .execute(
                "INSERT INTO file_tags(file_id,tag_id,source) VALUES(?,?,'manual')",
                params![alias.id, tag.id],
            )
            .unwrap();
        let q = Query {
            text: "卷子".into(),
            search_mode: "unified".into(),
            sort: "relevance".into(),
            limit: 1,
            ..Default::default()
        };
        for (i, id) in [exact.id, alias.id, name.id, body.id].iter().enumerate() {
            let r = f
                .store
                .query(&Query {
                    offset: i,
                    ..q.clone()
                })
                .unwrap();
            assert_eq!(r["total"], 4);
            assert_eq!(r["files"][0]["id"], *id);
        }
        assert_eq!(
            f.store
                .query(&Query {
                    search_scope: "name".into(),
                    ..q
                })
                .unwrap()["total"],
            2
        );
    }
    #[test]
    fn short_chinese_note_tag_queries_and_literal_syntax() {
        let mut f = Fixture::new();
        let image = f.add("IMG001.txt", "无关正文".as_bytes());
        f.drain();
        f.store
            .conn
            .execute(
                "UPDATE files SET note=?,note_key=? WHERE id=?",
                params!["建筑外墙风景室内", key("建筑外墙风景室内"), image.id],
            )
            .unwrap();
        for text in ["风景", "建筑外墙", "室内", "墙", "景", "外墙 室内"] {
            let q = Query {
                text: text.into(),
                search_mode: "unified".into(),
                limit: 100,
                ..Default::default()
            };
            let r = f.store.query(&q).unwrap();
            assert_eq!(r["total"], 1, "{}", text);
            assert!(r["files"][0].get("contentMatch").is_none());
        }
        let q = Query {
            text: "' OR 1=1 --".into(),
            search_mode: "unified".into(),
            limit: 100,
            ..Default::default()
        };
        assert_eq!(f.store.query(&q).unwrap()["total"], 0);
        assert!(f
            .store
            .query(&Query {
                search_scope: "unknown".into(),
                ..q
            })
            .is_err());
    }
    #[test]
    fn unified_search_deduplicates_and_paginates_before_enrichment() {
        let mut f = Fixture::new();
        let meta = f.add("预算名称.txt", "名称命中，正文没有关键词".as_bytes());
        let body = f.add("正文资料.txt", "这里是预算计划，后面是旅行攻略".as_bytes());
        let both = f.add("预算双重.txt", "正文也有预算".as_bytes());
        let other = f.add("无关.txt", "普通内容".as_bytes());
        f.drain();
        let q = Query {
            text: "预算".into(),
            search_mode: "unified".into(),
            sort: "relevance".into(),
            direction: "desc".into(),
            limit: 2,
            ..Default::default()
        };
        let a = f.store.query(&q).unwrap();
        assert_eq!(a["total"], 3);
        assert_eq!(a["hasMore"], true);
        let ids: Vec<_> = a["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect();
        assert!(ids.contains(&meta.id.as_str()));
        assert!(ids.contains(&both.id.as_str()));
        let b = f
            .store
            .query(&Query {
                offset: 2,
                ..q.clone()
            })
            .unwrap();
        assert_eq!(b["files"][0]["id"], body.id);
        assert_eq!(b["hasMore"], false);
        assert!(b["files"][0]["contentMatch"]["snippet"]
            .as_str()
            .unwrap()
            .contains("预算"));
        let m = a["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == meta.id)
            .unwrap();
        assert!(m.get("contentMatch").is_none());
        let b = f
            .store
            .query(&Query {
                text: "旅行".into(),
                ..q.clone()
            })
            .unwrap();
        assert_eq!(b["total"], 1);
        assert_eq!(b["files"][0]["contentMatch"]["queryText"], "旅行");
        let all = f
            .store
            .query(&Query {
                text: String::new(),
                limit: 100,
                ..q.clone()
            })
            .unwrap();
        assert_eq!(all["total"], 4);
        assert!(all["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v.get("contentMatch").is_none()));
        f.store
            .conn
            .execute(
                "UPDATE files SET note='独有备注',note_key=? WHERE id=?",
                params![key("独有备注"), other.id],
            )
            .unwrap();
        assert_eq!(
            f.store
                .query(&Query {
                    text: "独有备注".into(),
                    ..q.clone()
                })
                .unwrap()["total"],
            1
        );
        f.store.search_control("disable").unwrap();
        let off = f.store.query(&q).unwrap();
        assert_eq!(off["total"], 2);
        assert!(off["files"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v.get("contentMatch").is_none()));
    }
    #[test]
    fn unified_search_filters_and_stale_docs() {
        let mut f = Fixture::new();
        let body = f.add("正文.txt", "蓝莓内容".as_bytes());
        let tagged = f.add("标签资料.txt", "无关键词".as_bytes());
        f.drain();
        let tag = f.store.create_tag("蓝莓标签").unwrap();
        f.store
            .conn
            .execute(
                "INSERT INTO file_tags(file_id,tag_id,source) VALUES(?,?,'manual')",
                params![tagged.id, tag.id],
            )
            .unwrap();
        let q = Query {
            text: "蓝莓".into(),
            search_mode: "unified".into(),
            limit: 100,
            ..Default::default()
        };
        assert_eq!(f.store.query(&q).unwrap()["total"], 2);
        assert_eq!(
            f.store
                .query(&Query {
                    include: vec![tag.id.clone()],
                    ..q.clone()
                })
                .unwrap()["total"],
            1
        );
        assert_eq!(
            f.store
                .query(&Query {
                    exclude: vec![tag.id.clone()],
                    ..q.clone()
                })
                .unwrap()["total"],
            1
        );
        assert_eq!(
            f.store
                .query(&Query {
                    kinds: vec!["image".into()],
                    ..q.clone()
                })
                .unwrap()["total"],
            0
        );
        f.store
            .conn
            .execute("UPDATE files SET revision=revision+1 WHERE id=?", [body.id])
            .unwrap();
        assert_eq!(f.store.query(&q).unwrap()["total"], 1);
        let injected:Query=serde_json::from_value(json!({"searchMode":"unified","text":"不会命中","supplementalIds":[tagged.id],"rankedIds":[tagged.id]})).unwrap();
        assert_eq!(f.store.query(&injected).unwrap()["total"], 0);
    }
    #[test]
    fn pdf_search_returns_page_and_respects_filters_and_clear() {
        let mut f = Fixture::new();
        let file = f.add(
            "pages.pdf",
            &crate::pdf_index::fixture(&["first Apple", "second Hummingbird", ""]),
        );
        f.drain();
        assert_eq!(
            f.query("Apple")["files"][0]["contentMatch"]["location"]["page"],
            1
        );
        let r = f.query("Hummingbird");
        assert_eq!(r["total"], 1);
        assert_eq!(r["files"][0]["contentMatch"]["source"], "第 2 页");
        assert_eq!(r["files"][0]["contentMatch"]["location"]["page"], 2);
        assert!(r["files"][0]["contentMatch"]["snippet"]
            .as_str()
            .unwrap()
            .contains("Hummingbird"));
        assert!(r["files"][0]["contentMatch"]["coverage"]
            .as_str()
            .unwrap()
            .contains("1 页无"));
        assert_eq!(f.query("Apple Hummingbird")["total"], 1);
        assert!(f.query("")["files"][0]["contentMatch"].is_null());
        let q = Query {
            search_mode: "content".into(),
            text: "Apple".into(),
            kinds: vec!["image".into()],
            ..Default::default()
        };
        assert_eq!(f.store.query(&q).unwrap()["total"], 0);
        let before = serde_json::to_value(f.store.file(&file.id).unwrap()).unwrap();
        f.store.search_control("clear").unwrap();
        assert_eq!(
            serde_json::to_value(f.store.file(&file.id).unwrap()).unwrap(),
            before
        );
        assert_eq!(
            std::fs::read(&file.path).unwrap(),
            crate::pdf_index::fixture(&["first Apple", "second Hummingbird", ""]).as_slice()
        );
    }
    #[test]
    fn pdf_empty_corrupt_encrypted_and_large_files_have_explicit_errors() {
        let mut f = Fixture::new();
        let empty = f.add("scan.pdf", &crate::pdf_index::fixture(&["", ""]));
        let corrupt = f.add("bad.pdf", b"%PDF-1.4 invalid");
        let huge = f.add("huge.pdf", b"%PDF-1.4");
        File::options()
            .write(true)
            .open(&huge.path)
            .unwrap()
            .set_len(crate::pdf_index::SOURCE_LIMIT + 1)
            .unwrap();
        f.store.refresh_metadata(&huge.id).unwrap();
        let mut encrypted =
            pdf_extract::Document::load_mem(&crate::pdf_index::fixture(&["secret"])).unwrap();
        encrypted.trailer.set(
            "ID",
            vec![
                pdf_extract::Object::string_literal("0123456789abcdef"),
                pdf_extract::Object::string_literal("0123456789abcdef"),
            ],
        );
        let state = pdf_extract::EncryptionState::try_from(pdf_extract::EncryptionVersion::V1 {
            document: &encrypted,
            owner_password: "owner",
            user_password: "secret",
            permissions: pdf_extract::Permissions::COPYABLE,
        })
        .unwrap();
        encrypted.encrypt(&state).unwrap();
        let mut bytes = vec![];
        encrypted.save_to(&mut bytes).unwrap();
        let locked = f.add("locked.pdf", &bytes);
        let mut empty_password =
            pdf_extract::Document::load_mem(&crate::pdf_index::fixture(&["permission protected"]))
                .unwrap();
        empty_password.trailer.set(
            "ID",
            vec![
                pdf_extract::Object::string_literal("0123456789abcdef"),
                pdf_extract::Object::string_literal("0123456789abcdef"),
            ],
        );
        let state = pdf_extract::EncryptionState::try_from(pdf_extract::EncryptionVersion::V1 {
            document: &empty_password,
            owner_password: "owner",
            user_password: "",
            permissions: pdf_extract::Permissions::COPYABLE,
        })
        .unwrap();
        empty_password.encrypt(&state).unwrap();
        let mut bytes = vec![];
        empty_password.save_to(&mut bytes).unwrap();
        let empty_locked = f.add("empty-password.pdf", &bytes);
        f.drain();
        for (id, code) in [
            (empty.id, "PDF_NO_TEXT"),
            (corrupt.id, "PDF_INVALID"),
            (locked.id, "PDF_ENCRYPTED"),
            (empty_locked.id, "PDF_ENCRYPTED"),
        ] {
            let e: String = f
                .store
                .conn
                .query_row("SELECT error FROM search.docs WHERE id=?", [id], |r| {
                    r.get(0)
                })
                .unwrap();
            assert!(e.contains(code), "{e}");
        }
        let fresh = f.store.file_metadata(&huge.id).unwrap();
        assert!(read(&Work {
            file: fresh,
            generation: 0
        })
        .err()
        .unwrap()
        .contains("INDEX_TOO_LARGE"));
        assert_eq!(f.store.search_status().unwrap()["counts"]["failed"], 5);
    }
    #[test]
    fn pdf_page_and_text_limits_report_partial_coverage() {
        let mut f = Fixture::new();
        f.add(
            "many.pdf",
            &crate::pdf_index::fixture(&vec!["ordinary page"; 501]),
        );
        let text = "bounded ".repeat(40_000) + "outsidecoverage";
        f.add("page-limit.pdf", &crate::pdf_index::fixture(&[&text]));
        let full = "documentbound ".repeat(20_000);
        f.add(
            "body-limit.pdf",
            &crate::pdf_index::fixture(&vec![full.as_str(); 12]),
        );
        f.drain();
        let r = f.query("ordinary");
        assert_eq!(r["files"][0]["contentMatch"]["truncated"], true);
        assert!(r["files"][0]["contentMatch"]["coverage"]
            .as_str()
            .unwrap()
            .contains("500/501"));
        assert_eq!(f.query("outsidecoverage")["total"], 0);
        let r = f.query("documentbound");
        assert_eq!(r["files"][0]["contentMatch"]["truncated"], true);
        let max: i64 = f
            .store
            .conn
            .query_row(
                "SELECT max(length(CAST(body AS BLOB))) FROM search.docs",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(max <= READ_LIMIT as i64);
    }
    #[test]
    fn pdf_cancel_between_pages_and_changed_file_cannot_commit() {
        let mut f = Fixture::new();
        let file = f.add(
            "cancel.pdf",
            &crate::pdf_index::fixture(&["first", "second", "third"]),
        );
        let work = f.store.search_next().unwrap().unwrap();
        let mut checks = 0;
        let result = read_controlled(&work, || {
            checks += 1;
            Ok(checks < 3)
        });
        assert!(result.err().unwrap().contains("INDEX_CANCELLED"));
        f.store.search_control("cancel").unwrap();
        assert!(!f.store.search_commit(&work, read(&work)).unwrap());
        f.store.search_control("resume").unwrap();
        let work = f.store.search_next().unwrap().unwrap();
        let text = read(&work);
        std::fs::write(&file.path, crate::pdf_index::fixture(&["new content"])).unwrap();
        assert!(!f.store.search_commit(&work, text).unwrap());
        f.store.refresh_metadata(&file.id).unwrap();
        f.drain();
        assert_eq!(f.query("first")["total"], 0);
        assert_eq!(f.query("new content")["total"], 1);
    }
    #[test]
    fn pdf_parser_upgrade_and_restart_preserve_annotation_and_pause() {
        let mut f = Fixture::new();
        let file = f.add("migration.md", b"migration text");
        f.drain();
        f.store
            .conn
            .execute("UPDATE search.docs SET parser=2", [])
            .unwrap();
        assert_eq!(f.query("migration")["total"], 0);
        assert_eq!(f.store.search_status().unwrap()["counts"]["stale"], 1);
        f.store.search_control("pause").unwrap();
        let root = f.store.root.clone();
        drop(f.store);
        f.store = Store::open(&root).unwrap();
        assert!(f.store.search_next().unwrap().is_none());
        assert_eq!(f.store.file(&file.id).unwrap().name, "migration.md");
        f.store.search_control("resume").unwrap();
        f.drain();
        assert_eq!(f.query("migration")["total"], 1);
    }
    #[test]
    fn unsupported_pdf_font_does_not_stop_following_tasks() {
        let mut f = Fixture::new();
        let mut doc =
            pdf_extract::Document::load_mem(&crate::pdf_index::fixture(&["unsupported"])).unwrap();
        for object in doc.objects.values_mut() {
            if let Ok(dict) = object.as_dict_mut() {
                if dict.has_type(b"Font") {
                    dict.set("Subtype", "Type0");
                    dict.set("Encoding", "UniGB-UCS2-H");
                    dict.set(
                        "DescendantFonts",
                        vec![
                            pdf_extract::dictionary! {"Type"=>"Font","Subtype"=>"CIDFontType0"}
                                .into(),
                        ],
                    );
                }
            }
        }
        let mut bytes = vec![];
        doc.save_to(&mut bytes).unwrap();
        let bad = f.add("unsupported.pdf", &bytes);
        f.add(
            "after.pdf",
            &crate::pdf_index::fixture(&["worker survives"]),
        );
        f.drain();
        let error: String = f
            .store
            .conn
            .query_row("SELECT error FROM search.docs WHERE id=?", [bad.id], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(error.contains("PDF_FONT_UNSUPPORTED"), "{error}");
        assert_eq!(f.query("survives")["total"], 1);
    }
    #[test]
    fn snippets_follow_current_query_and_empty_query_has_no_excerpt() {
        let mut f = Fixture::new();
        f.add(
            "two.md",
            format!("开头苹果{}尾部蜂鸟", "普通正文。".repeat(400)).as_bytes(),
        );
        f.drain();
        for text in ["", "   "] {
            assert!(f.query(text)["files"][0].get("contentMatch").is_none());
        }
        let a = f.query("苹果");
        let b = f.query("蜂鸟");
        assert_eq!(a["files"][0]["contentMatch"]["queryText"], "苹果");
        assert_eq!(b["files"][0]["contentMatch"]["queryText"], "蜂鸟");
        assert_ne!(
            a["files"][0]["contentMatch"]["snippet"],
            b["files"][0]["contentMatch"]["snippet"]
        );
        assert!(b["files"][0]["contentMatch"]["snippet"]
            .as_str()
            .unwrap()
            .contains("蜂鸟"));
    }
    #[test]
    fn production_fts_chinese_short_mixed_and_literal_syntax() {
        let mut f = Fixture::new();
        f.add(
            "ordinary.md",
            "蓝色猫咪在公园。Rust API 中文 \"hello\" C++ a/b [x] OR NOT NEAR *".as_bytes(),
        );
        f.add("other.txt", b"no match");
        f.drain();
        for t in [
            "猫",
            "猫咪",
            "蓝色猫咪",
            "Rust API",
            "中文",
            "\"hello\"",
            "C++",
            "a/b",
            "[x]",
            "OR",
            "NOT",
            "NEAR",
            "*",
        ] {
            assert_eq!(f.query(t)["total"], 1, "{t}");
        }
        assert_eq!(f.query("猫不存在")["total"], 0);
        assert_eq!(f.query("猫咪 no match")["total"], 0);
        let r = f.query("猫咪");
        assert!(r["files"][0]["contentMatch"]["snippet"]
            .as_str()
            .unwrap()
            .contains("猫咪"));
        assert_eq!(
            f.store
                .query(&Query {
                    text: "猫咪".into(),
                    limit: 100,
                    ..Default::default()
                })
                .unwrap()["total"],
            0
        );
        assert_eq!(f.query("\" OR 1=1 --")["total"], 0);
        assert!(terms(&"a".repeat(257)).is_err());
        assert!(terms(&"a ".repeat(17)).is_err());
    }
    #[test]
    fn content_filters_keep_tag_scope_directory_and_sort() {
        let mut f = Fixture::new();
        let a = f.add("a.md", b"shared body");
        let b = f.add("b.txt", b"shared body");
        f.drain();
        let tag = f.store.create_tag("scope").unwrap();
        f.store
            .conn
            .execute(
                "INSERT INTO file_tags(file_id,tag_id) VALUES(?,?)",
                params![a.id, tag.id],
            )
            .unwrap();
        let q = Query {
            search_mode: "content".into(),
            text: "shared".into(),
            include: vec![tag.id.clone()],
            sort: "name".into(),
            direction: "asc".into(),
            limit: 1,
            ..Default::default()
        };
        assert_eq!(f.store.query(&q).unwrap()["files"][0]["id"], a.id);
        assert_eq!(
            f.store
                .query(&Query {
                    exclude: vec![tag.id],
                    include: vec![],
                    ..q.clone()
                })
                .unwrap()["files"][0]["id"],
            b.id
        );
        assert_eq!(
            f.store
                .query(&Query {
                    scope: "starred".into(),
                    ..q.clone()
                })
                .unwrap()["total"],
            0
        );
        assert_eq!(
            f.store
                .query(&Query {
                    directory: "Z:\\not-here".into(),
                    ..q
                })
                .unwrap()["total"],
            0
        );
    }
    #[test]
    fn pause_cancel_generation_and_restart_checkpoint() {
        let mut f = Fixture::new();
        f.add("a.txt", b"checkpoint");
        let w = f.store.search_next().unwrap().unwrap();
        f.store.search_control("pause").unwrap();
        assert!(!f.store.search_commit(&w, read(&w)).unwrap());
        assert!(f.store.search_next().unwrap().is_none());
        f.store.search_control("resume").unwrap();
        let root = f.store.root.clone();
        let pending = f.store.search_next().unwrap().unwrap();
        drop(f.store);
        f.store = Store::open(&root).unwrap();
        assert_eq!(f.store.search_status().unwrap()["counts"]["queued"], 1);
        f.drain();
        assert_eq!(f.query("checkpoint")["total"], 1);
        f.store.search_control("cancel").unwrap();
        assert!(!f.store.search_commit(&pending, read(&pending)).unwrap());
        assert_eq!(f.query("checkpoint")["total"], 1);
        assert!(f.store.search_next().unwrap().is_none());
    }
    #[test]
    fn encodings_binary_and_truncation_are_visible() {
        let mut f = Fixture::new();
        let mut utf16 = vec![0xff, 0xfe];
        for u in "编码测试".encode_utf16() {
            utf16.extend(u.to_le_bytes());
        }
        f.add("utf16.txt", &utf16);
        let (gb, _, _) = encoding_rs::GB18030.encode("中文编码");
        f.add("gb.md", &gb);
        f.add("binary.txt", b"bad\0binary");
        let mut large = vec![b'a'; READ_LIMIT + 50];
        large[..6].copy_from_slice(b"needle");
        f.add("large.txt", &large);
        f.drain();
        assert_eq!(f.query("编码")["total"], 2);
        assert_eq!(
            f.query("needle")["files"][0]["contentMatch"]["truncated"],
            true
        );
        assert_eq!(f.store.search_status().unwrap()["counts"]["failed"], 1);
        assert!(f.store.search_status().unwrap()["errors"][0]["error"]
            .as_str()
            .unwrap()
            .contains("BINARY_FILE"));
    }
    #[test]
    fn stale_commit_and_removed_files_cannot_match() {
        let mut f = Fixture::new();
        let file = f.add("a.md", b"old phrase");
        let w = f.store.search_next().unwrap().unwrap();
        let text = read(&w);
        std::fs::write(&file.path, b"new changed phrase").unwrap();
        assert!(!f.store.search_commit(&w, text).unwrap());
        f.store.refresh_metadata(&file.id).unwrap();
        f.drain();
        assert_eq!(f.query("old")["total"], 0);
        assert_eq!(f.query("changed")["total"], 1);
        f.store
            .conn
            .execute("UPDATE files SET removed_at=1 WHERE id=?", [&file.id])
            .unwrap();
        assert_eq!(f.query("changed")["total"], 0);
        f.drain();
        assert_eq!(
            f.store
                .conn
                .query_row("SELECT count(*) FROM search.docs", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn clear_rebuild_restore_preserve_annotations_and_original() {
        let mut f = Fixture::new();
        let file = f.add("keep.txt", b"preserved body");
        f.store
            .save_note(&json!({"id":file.id,"version":0,"text":"my note"}))
            .unwrap();
        f.store
            .conn
            .execute("UPDATE files SET favorite=1 WHERE id=?", [&file.id])
            .unwrap();
        f.drain();
        let baseline = serde_json::to_value(f.store.file(&file.id).unwrap()).unwrap();
        f.store.search_control("rebuild").unwrap();
        f.drain();
        assert_eq!(
            serde_json::to_value(f.store.file(&file.id).unwrap()).unwrap(),
            baseline
        );
        let backup = f.dir.path().join("safe.sqtagbackup");
        f.store.export_backup(&backup).unwrap();
        f.store.restore_backup(&backup).unwrap();
        assert!(!f.store.search_enabled().unwrap());
        assert_eq!(f.store.search_status().unwrap()["eligible"], 1);
        assert_eq!(std::fs::read(&file.path).unwrap(), b"preserved body");
        assert_eq!(
            serde_json::to_value(f.store.file(&file.id).unwrap()).unwrap(),
            baseline
        );
        f.store.search_control("enable").unwrap();
        f.drain();
        assert_eq!(f.query("preserved")["total"], 1);
        f.store.search_control("clear").unwrap();
        assert_eq!(f.store.search_status().unwrap()["counts"].get("done"), None);
    }
    #[test]
    fn capacity_failure_is_visible_without_overwriting_annotations() {
        let mut f = Fixture::new();
        let a = f.add("a.md", b"some body");
        f.drain();
        f.store
            .conn
            .execute(
                "UPDATE search.docs SET text_bytes=? WHERE id=?",
                params![TEXT_BUDGET, a.id],
            )
            .unwrap();
        f.add("b.md", b"another body");
        f.drain();
        assert_eq!(f.store.search_status().unwrap()["counts"]["failed"], 1);
        assert!(f.store.search_status().unwrap()["errors"][0]["error"]
            .as_str()
            .unwrap()
            .contains("INDEX_FULL"));
        assert_eq!(f.query("some")["total"], 1);
    }
    #[test]
    fn corrupt_derived_cache_recovers_without_losing_library() {
        let mut f = Fixture::new();
        let file = f.add("a.md", b"keep original");
        let root = f.store.root.clone();
        drop(f.store);
        std::fs::write(root.join("cache/search/index.sqlite"), b"not a database").unwrap();
        f.store = Store::open(&root).unwrap();
        assert_eq!(f.store.file(&file.id).unwrap().path, file.path);
        assert!(!f.store.search_enabled().unwrap());
        assert_eq!(std::fs::read(&file.path).unwrap(), b"keep original");
    }
    #[test]
    fn detached_writer_does_not_need_annotation_write_lock() {
        let mut f = Fixture::new();
        let file = f.add("independent.md", b"independent body");
        let w = f.store.search_next().unwrap().unwrap();
        let text = read(&w);
        let annotation_writer =
            rusqlite::Connection::open(f.store.root.join("library.sqlite")).unwrap();
        annotation_writer.execute_batch("BEGIN IMMEDIATE").unwrap();
        assert!(commit_detached(&f.store.root, &w, text).unwrap());
        annotation_writer.execute_batch("ROLLBACK").unwrap();
        assert_eq!(f.query("independent")["total"], 1);
        f.store.search_control("rebuild").unwrap();
        let w = f.store.search_next().unwrap().unwrap();
        f.store.search_control("cancel").unwrap();
        assert!(!commit_detached(&f.store.root, &w, read(&w)).unwrap());
        assert_eq!(f.store.file(&file.id).unwrap().note, "");
    }
    #[test]
    #[ignore = "Explicit real-text throughput and query benchmark"]
    fn text_index_performance() {
        let mut f = Fixture::new();
        let body = "合成项目资料：蓝色猫咪和 Rust API，正文性能测试。\n".repeat(20);
        for i in 0..1000 {
            f.add(&format!("record-{i:04}.md"), body.as_bytes());
        }
        let start = std::time::Instant::now();
        let writer = writer(&f.store.root).unwrap();
        while let Some(w) = f.store.search_next().unwrap() {
            assert!(writer.search_commit(&w, read(&w)).unwrap());
        }
        println!(
            "TEXT_INDEX files=1000 source_bytes={} elapsed_ms={} disk_bytes={}",
            body.len() * 1000,
            start.elapsed().as_millis(),
            f.store.search_status().unwrap()["bytes"]
        );
        for term in ["猫", "猫咪", "蓝色猫咪", "Rust API"] {
            for offset in [0, 900] {
                let q = Query {
                    search_mode: "content".into(),
                    text: term.into(),
                    limit: 100,
                    offset,
                    ..Default::default()
                };
                let start = std::time::Instant::now();
                let r = f.store.query(&q).unwrap();
                assert_eq!(r["total"], 1000);
                println!(
                    "TEXT_QUERY term={term} offset={offset} elapsed_us={}",
                    start.elapsed().as_micros()
                );
            }
        }
    }
}
