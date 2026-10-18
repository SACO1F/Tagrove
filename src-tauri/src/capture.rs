//! User-triggered web records. SQLite is authoritative; .sqweb is a portable descriptor,
//! never a downloaded page. Browser captures may queue AI when enabled; no folder tags are created.
use crate::{db::Store, fsops, model::*};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Write};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Source {
    pub format: u32,
    pub kind: String,
    pub title: String,
    pub url: String,
    pub excerpt: String,
    pub captured_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<ImageSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reading: Option<ReadingCopy>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadingCopy {
    pub title: String,
    pub text: String,
    pub updated_at: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageSource {
    pub url: String,
    pub final_url: String,
    pub sha256: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub request_id: String,
    pub kind: String,
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub excerpt: String,
    #[serde(default)]
    pub tag_ids: Vec<String>,
    #[serde(default)]
    pub page_text: String,
}
pub fn checked_url(text: &str) -> Result<String> {
    if text.len() > 8192 || text.chars().any(char::is_control) {
        return Err(err("CAPTURE_URL", "链接无效或过长"));
    }
    let u = reqwest::Url::parse(text)
        .map_err(|_| err("CAPTURE_URL", "请输入完整的 HTTP 或 HTTPS 链接"))?;
    if !matches!(u.scheme(), "http" | "https")
        || u.host_str().is_none()
        || !u.username().is_empty()
        || u.password().is_some()
    {
        return Err(err(
            "CAPTURE_URL",
            "只支持不含登录凭据的 HTTP 或 HTTPS 链接",
        ));
    }
    Ok(u.to_string())
}
impl Source {
    pub fn validate(&self) -> Result<()> {
        checked_url(&self.url)?;
        if let Some(copy) = &self.reading {
            if self.kind == "web_image" || self.format != 3 || copy.title.trim().is_empty()
                || copy.title.chars().count() > 300 || copy.title.chars().any(char::is_control)
                || copy.text.chars().count() > 20000 || copy.text.contains('\0')
                || !(0..=253402300799999).contains(&copy.updated_at) {
                return Err(err("CAPTURE_INVALID", "阅读副本无效"));
            }
        }
        if self.title.trim().is_empty()
            || self.title.chars().count() > 300
            || self.title.chars().any(char::is_control)
            || self.excerpt.chars().count() > 20000
            || self.excerpt.contains('\0')
            || !(0..=253402300799999).contains(&self.captured_at)
        {
            return Err(err("CAPTURE_INVALID", "标题、摘录或资料类型无效"));
        }
        if self.kind == "web_image" {
            let image = self
                .image
                .as_ref()
                .ok_or_else(|| err("CAPTURE_INVALID", "图片缺少来源信息"))?;
            checked_url(&image.url)?;
            checked_url(&image.final_url)?;
            if self.format != 2
                || !self.excerpt.is_empty()
                || image.sha256.len() != 64
                || !image
                    .sha256
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                || !matches!(
                    image.mime.as_str(),
                    "image/png" | "image/jpeg" | "image/webp" | "image/gif" | "image/bmp"
                )
                || image.width == 0
                || image.height == 0
                || u64::from(image.width) * u64::from(image.height) > 100_000_000
            {
                return Err(err("CAPTURE_INVALID", "图片来源信息无效"));
            }
        } else if self.format != if self.reading.is_some() { 3 } else { 1 }
            || self.image.is_some()
            || !matches!(self.kind.as_str(), "web_page" | "web_excerpt")
            || (self.kind == "web_page" && !self.excerpt.is_empty())
            || (self.kind == "web_excerpt" && self.excerpt.trim().is_empty())
        {
            return Err(err("CAPTURE_INVALID", "标题、摘录或资料类型无效"));
        }
        Ok(())
    }
    pub fn text(&self) -> String {
        format!(
            "{}\n{}\n{}\n{}",
            self.display_title(),
            self.url,
            self.reading_text(),
            self.image.as_ref().map(|i| i.url.as_str()).unwrap_or("")
        )
    }
    pub fn display_title(&self) -> &str {
        self.reading.as_ref().map(|r| r.title.as_str()).unwrap_or(&self.title)
    }
    pub fn reading_text(&self) -> &str {
        self.reading.as_ref().map(|r| r.text.as_str()).unwrap_or(&self.excerpt)
    }
}
pub fn migrate(conn: &Connection) -> Result<()> {
    let v: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(sql_err)?;
    if v == 3 {
        let mut st = conn.prepare("PRAGMA table_info(files)").map_err(sql_err)?;
        let cols = st
            .query_map([], |r| r.get::<_, String>(1))
            .map_err(sql_err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql_err)?;
        drop(st);
        conn.execute_batch("SAVEPOINT web_migrate;")
            .map_err(sql_err)?;
        if !cols.iter().any(|c| c == "web_source") {
            conn.execute_batch("ALTER TABLE files ADD COLUMN web_source TEXT NOT NULL DEFAULT '';ALTER TABLE files ADD COLUMN web_key TEXT NOT NULL DEFAULT '';").map_err(sql_err)?;
        }
        conn.execute_batch("CREATE TABLE IF NOT EXISTS capture_receipts(request_id TEXT PRIMARY KEY,payload_hash TEXT NOT NULL,dedupe_key TEXT NOT NULL,file_id TEXT NOT NULL REFERENCES files(id));CREATE INDEX IF NOT EXISTS capture_dedupe ON capture_receipts(dedupe_key,file_id);INSERT OR IGNORE INTO schema_migrations VALUES(4,CAST(strftime('%s','now') AS INTEGER)*1000);PRAGMA user_version=4;RELEASE web_migrate;").map_err(sql_err)?;
    }
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(sql_err)?;
    if version == 4 {
        conn.execute_batch("BEGIN IMMEDIATE;INSERT OR IGNORE INTO schema_migrations VALUES(5,CAST(strftime('%s','now') AS INTEGER)*1000);PRAGMA user_version=5;COMMIT;").map_err(sql_err)?;
    }
    conn.execute_batch("BEGIN IMMEDIATE;INSERT OR IGNORE INTO schema_migrations VALUES(6,CAST(strftime('%s','now') AS INTEGER)*1000);PRAGMA user_version=6;COMMIT;").map_err(sql_err)?;
    conn.execute_batch("BEGIN IMMEDIATE;INSERT OR IGNORE INTO schema_migrations VALUES(7,CAST(strftime('%s','now') AS INTEGER)*1000);PRAGMA user_version=7;COMMIT;").map_err(sql_err)?;
    Ok(())
}
pub fn validate(conn: &Connection) -> Result<()> {
    let mut st = conn
        .prepare("SELECT web_source,kind FROM files WHERE web_source<>'' OR kind='web'")
        .map_err(sql_err)?;
    let rows = st
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(sql_err)?;
    for row in rows {
        let (raw, kind) = row.map_err(sql_err)?;
        let s: Source =
            serde_json::from_str(&raw).map_err(|_| err("BACKUP_INVALID", "网页资料结构无效"))?;
        s.validate()?;
        if s.image.as_ref().is_some_and(|img|u64::from(img.width)*u64::from(img.height)>40_000_000 || !matches!(img.mime.as_str(),"image/png"|"image/jpeg"|"image/webp")) && conn.pragma_query_value(None,"user_version",|r|r.get::<_,i64>(0)).map_err(sql_err)?<7 {return Err(err("BACKUP_INVALID","增强图片需要新版资料库"));}
        if s.reading.is_some() && conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0)).map_err(sql_err)? < 6 {
            return Err(err("BACKUP_INVALID", "阅读副本需要新版资料库"));
        }
        if s.kind == "web_image"
            && conn
                .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .map_err(sql_err)?
                < 5
        {
            return Err(err("BACKUP_INVALID", "图片来源需要新版资料库"));
        }
        if kind
            != if s.kind == "web_image" {
                "image"
            } else {
                "web"
            }
        {
            return Err(err("BACKUP_INVALID", "网页资料类型不一致"));
        }
    }
    Ok(())
}
pub fn source_dedupe(source: &Source) -> String {
    if let Some(image) = &source.image {
        return format!("image:{}", image.sha256);
    }
    let mut canonical = reqwest::Url::parse(&source.url).expect("validated URL");
    canonical.set_fragment(None);
    format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&json!([source.kind, canonical.as_str(), source.excerpt])).unwrap()
        )
    )
}
pub fn portable_name(title: &str) -> String {
    let mut s: String = title
        .chars()
        .filter(|c| !c.is_control() && !"<>:\"/\\|?*".contains(*c))
        .take(65)
        .collect();
    s = s.trim_matches(['.', ' ']).to_owned();
    format!("网页-{}.sqweb", if s.is_empty() { "资料" } else { &s })
}
impl Store {
    pub fn capture(&mut self, v: &Value) -> Result<Value> {
        let req: Request = serde_json::from_value(v.clone())
            .map_err(|_| err("CAPTURE_INVALID", "采集请求无效"))?;
        if uuid::Uuid::parse_str(&req.request_id).is_err() {
            return Err(err("CAPTURE_INVALID", "采集请求标识无效"));
        }
        let tags=self.capture_tag_ids(v)?;
        let auto_ai=tags.is_empty() && self.ai_config()?.enabled;
        if req.page_text.chars().count()>12000 || req.page_text.contains('\0') || (!req.page_text.is_empty()&&req.kind!="web_page"){return Err(err("CAPTURE_INVALID","网页正文最多 12000 字符，仅用于网页链接"));}
        let reading=if req.page_text.trim().is_empty(){None}else{Some(ReadingCopy{title:req.title.trim().into(),text:req.page_text.trim().into(),updated_at:now()})};
        let source = Source {
            format: if reading.is_some(){3}else{1},
            kind: req.kind,
            title: req.title.trim().to_owned(),
            url: checked_url(&req.url)?,
            excerpt: req.excerpt.trim().to_owned(),
            captured_at: now(),
            image: None,
            reading,
        };
        source.validate()?;
        let mut payload = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&json!([
                    source.kind,
                    source.title,
                    source.url,
                    source.excerpt
                ]))
                .unwrap()
            )
        );
        if !tags.is_empty() || !req.page_text.is_empty(){payload=format!("{:x}",Sha256::digest(serde_json::to_vec(&json!([payload,tags,req.page_text])).unwrap()));}
        let dedupe = source_dedupe(&source);
        let receipt:Option<(String,String,Option<i64>)>=self.conn.query_row("SELECT c.payload_hash,c.file_id,f.removed_at FROM capture_receipts c JOIN files f ON f.id=c.file_id WHERE request_id=?",[&req.request_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(sql_err)?;
        if let Some((hash, fid, removed)) = receipt {
            if hash != payload {
                return Err(err(
                    "CAPTURE_CONFLICT",
                    "请求标识对应的内容已变化，请重新保存",
                ));
            }
            if removed.is_some() {
                return Err(err("CAPTURE_REMOVED", "这条资料已移除，请重新发起采集"));
            }
            crate::capture_options::apply(&self.conn,&fid,&tags,false)?;
            return Ok(json!({"id":fid,"duplicate":true,"aiQueued":false}));
        }
        let existing:Option<String>=self.conn.query_row("SELECT c.file_id FROM capture_receipts c JOIN files f ON f.id=c.file_id WHERE c.dedupe_key=? AND f.removed_at IS NULL LIMIT 1",[&dedupe],|r|r.get(0)).optional().map_err(sql_err)?;
        if let Some(fid) = existing {
            self.conn
                .execute(
                    "INSERT INTO capture_receipts VALUES(?,?,?,?)",
                    params![req.request_id, payload, dedupe, fid],
                )
                .map_err(sql_err)?;
            crate::capture_options::apply(&self.conn,&fid,&tags,false)?;
            return Ok(json!({"id":fid,"duplicate":true,"aiQueued":false}));
        }
        let folder = self.root.join("captures");
        fs::create_dir_all(&folder).map_err(|e| err("CAPTURE_STORAGE", e))?;
        if fsops::is_link(&fs::symlink_metadata(&folder).map_err(|e| err("CAPTURE_STORAGE", e))?) {
            return Err(err("CAPTURE_STORAGE", "采集目录不能是链接"));
        }
        let fid = id();
        let path = folder.join(format!("{fid}.sqweb"));
        let raw = serde_json::to_string(&source).map_err(|e| err("CAPTURE_INVALID", e))?;
        let mut output =
            tempfile::NamedTempFile::new_in(&folder).map_err(|e| err("CAPTURE_STORAGE", e))?;
        output
            .write_all(raw.as_bytes())
            .map_err(|e| err("CAPTURE_STORAGE", e))?;
        output
            .as_file()
            .sync_all()
            .map_err(|e| err("CAPTURE_STORAGE", e))?;
        output
            .persist_noclobber(&path)
            .map_err(|e| err("CAPTURE_STORAGE", e))?;
        let outcome = (|| {
            let m = fsops::inspect(&path)?;
            let tx = self.conn.transaction().map_err(sql_err)?;
            tx.execute("INSERT INTO files(id,path,path_key,parent,name,name_key,extension,kind,bytes,modified,added,revision,identity,web_source,web_key) VALUES(?,?,?,?,?,?,'sqweb','web',?,?,?,?,?,?,?)",params![fid,m.path,m.path,m.parent,source.title,key(&source.title),m.bytes,source.captured_at,source.captured_at,m.revision,m.identity,raw,key(&source.text())]).map_err(sql_err)?;
            tx.execute(
                "INSERT INTO capture_receipts VALUES(?,?,?,?)",
                params![req.request_id, payload, dedupe, fid],
            )
            .map_err(sql_err)?;
            crate::capture_options::apply(&tx,&fid,&tags,auto_ai)?;
            tx.commit().map_err(sql_err)?;
            Ok(json!({"id":fid,"duplicate":false,"aiQueued":auto_ai}))
        })();
        if outcome.is_err() {
            let _ = fs::remove_file(path);
        }
        outcome
    }
    /// Restore metadata carries complete web records; regenerate only their managed descriptors.
    pub fn rebuild_web_descriptors(&self) -> Result<()> {
        let mut st = self
            .conn
            .prepare("SELECT id,web_source FROM files WHERE web_source<>''")
            .map_err(sql_err)?;
        let rows = st
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(sql_err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql_err)?;
        drop(st);
        let folder = self.root.join("captures");
        fs::create_dir_all(&folder).map_err(|e| err("CAPTURE_STORAGE", e))?;
        if fsops::is_link(&fs::symlink_metadata(&folder).map_err(|e| err("CAPTURE_STORAGE", e))?) {
            return Err(err("CAPTURE_STORAGE", "采集目录不能是链接"));
        }
        for (fid, raw) in rows {
            let source: Source = serde_json::from_str(&raw)
                .map_err(|_| err("BACKUP_INVALID", "网页资料结构无效"))?;
            source.validate()?;
            if source.kind == "web_image" {
                continue;
            }
            if uuid::Uuid::parse_str(&fid).is_err() {
                return Err(err("BACKUP_INVALID", "网页资料 ID 无效"));
            }
            let source: Source =
                serde_json::from_str(&raw).map_err(|_| err("BACKUP_INVALID", "网页资料无效"))?;
            source.validate()?;
            let path = folder.join(format!("{}-{}.sqweb", fid, id()));
            let mut temp =
                tempfile::NamedTempFile::new_in(&folder).map_err(|e| err("CAPTURE_STORAGE", e))?;
            temp.write_all(raw.as_bytes())
                .map_err(|e| err("CAPTURE_STORAGE", e))?;
            temp.as_file()
                .sync_all()
                .map_err(|e| err("CAPTURE_STORAGE", e))?;
            temp.persist_noclobber(&path)
                .map_err(|e| err("CAPTURE_STORAGE", e))?;
            let m = fsops::inspect(&path)?;
            self.conn.execute("UPDATE files SET path=?,path_key=?,parent=?,revision=?,identity=?,bytes=?,status='available',error='' WHERE id=?",params![m.path,m.path,m.parent,m.revision,m.identity,m.bytes,fid]).map_err(sql_err)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn payload(title: &str, url: &str, excerpt: &str) -> Value {
        json!({"requestId":id(),"kind":if excerpt.is_empty(){"web_page"}else{"web_excerpt"},"title":title,"url":url,"excerpt":excerpt})
    }
    #[test]
    fn capture_is_idempotent_validated_and_never_auto_labels_or_ai() {
        let d = tempfile::tempdir().unwrap();
        let mut s = Store::open(&d.path().join("library")).unwrap();
        let v = payload("学习资料 | 官网", "https://example.test/guide#one", "");
        let first = s.capture(&v).unwrap();
        assert_eq!(s.capture(&v).unwrap()["id"], first["id"]);
        let mut changed = v.clone();
        changed["title"] = json!("改动");
        assert!(s
            .capture(&changed)
            .unwrap_err()
            .contains("CAPTURE_CONFLICT"));
        assert_eq!(
            s.capture(&payload("第二次收藏", "https://example.test/guide#two", ""))
                .unwrap()["id"],
            first["id"]
        );
        assert_eq!(
            s.query(&Query {
                limit: 100,
                ..Default::default()
            })
            .unwrap()["total"],
            1
        );
        let f = s.file(first["id"].as_str().unwrap()).unwrap();
        assert_eq!(f.kind, "web");
        assert!(f.tags.is_empty());
        assert!(f.ai_task.is_none());
        assert!(f.web_source.is_some());
        for url in [
            "javascript:alert(1)",
            "file:///C:/private",
            "https://user:password@example.test/",
            "data:text/html,unsafe",
        ] {
            assert!(s.capture(&payload("坏链接", url, "")).is_err());
        }
        s.conn
            .execute("UPDATE files SET removed_at=1 WHERE id=?", [&f.id])
            .unwrap();
        assert!(s.capture(&v).unwrap_err().contains("CAPTURE_REMOVED"));
        assert_ne!(
            s.capture(&payload("重新收藏", "https://example.test/guide", ""))
                .unwrap()["id"],
            f.id
        );
    }
    #[test]
    fn web_metadata_body_search_backup_and_transfer_keep_sources() {
        let d = tempfile::tempdir().unwrap();
        let mut sender = Store::open(&d.path().join("sender")).unwrap();
        let created = sender
            .capture(&payload(
                "建筑参考",
                "https://example.test/article?a=1",
                "室内风景与建筑外墙。<script>untrusted</script>",
            ))
            .unwrap();
        let fid = created["id"].as_str().unwrap();
        let tag = sender.create_tag("项目甲").unwrap();
        sender
            .conn
            .execute(
                "INSERT INTO file_tags(file_id,tag_id) VALUES(?,?)",
                params![fid, tag.id],
            )
            .unwrap();
        sender
            .conn
            .execute(
                "UPDATE files SET note='备注保留',note_key='备注保留',favorite=1 WHERE id=?",
                [fid],
            )
            .unwrap();
        for text in ["建筑参考", "example.test", "室内风景", "建筑 外墙"] {
            assert_eq!(
                sender
                    .query(&Query {
                        text: text.into(),
                        search_mode: "unified".into(),
                        limit: 100,
                        ..Default::default()
                    })
                    .unwrap()["total"],
                1
            );
        }
        sender.search_control("enable").unwrap();
        let work = sender.search_next().unwrap().unwrap();
        sender
            .search_commit(&work, crate::search::read(&work))
            .unwrap();
        assert_eq!(
            sender
                .query(&Query {
                    text: "室内".into(),
                    search_mode: "content".into(),
                    limit: 100,
                    ..Default::default()
                })
                .unwrap()["total"],
            1
        );
        let archive = d.path().join("backup.sqtagbackup");
        sender.export_backup(&archive).unwrap();
        let mut restored = Store::open(&d.path().join("restored")).unwrap();
        restored.restore_backup(&archive).unwrap();
        let record = restored.file(fid).unwrap();
        assert!(PathBuf::from(&record.path).starts_with(&restored.root));
        assert_eq!(
            record.web_source.as_ref().unwrap().excerpt,
            "室内风景与建筑外墙。<script>untrusted</script>"
        );
        let plan = sender.transfer_plan(&json!({"ids":[fid]})).unwrap();
        let pack = d.path().join("web.sqtagpack");
        let control = crate::transfer::Control::default();
        crate::transfer::export(
            plan,
            &pack,
            &sender.transfer_plan(&json!({"ids":[fid]})).unwrap().info["fingerprint"]
                .as_str()
                .unwrap(),
            &control,
        )
        .unwrap();
        let preview = crate::transfer::preview(&pack, &control).unwrap();
        let mut receiver = Store::open(&d.path().join("receiver")).unwrap();
        fs::create_dir(d.path().join("received")).unwrap();
        receiver
            .import_package(
                &pack,
                &d.path().join("received"),
                preview["fingerprint"].as_str().unwrap(),
                &control,
            )
            .unwrap();
        let files = receiver
            .query(&Query {
                limit: 100,
                ..Default::default()
            })
            .unwrap();
        let file = &files["files"][0];
        assert_eq!(file["webSource"]["title"], "建筑参考");
        assert_eq!(file["note"], "备注保留");
        assert_eq!(file["favorite"], true);
        assert_eq!(file["tags"][0]["name"], "项目甲");
        assert_eq!(
            receiver
                .capture(&payload(
                    "再次保存",
                    "https://example.test/article?a=1",
                    "室内风景与建筑外墙。<script>untrusted</script>"
                ))
                .unwrap()["id"],
            file["id"]
        );
        receiver.validate().unwrap();
    }
    use std::path::PathBuf;
}

#[cfg(test)]
mod legacy_tests {
    use super::*;
    #[test]
    fn actual_v3_columns_and_backups_remain_readable() {
        let d = tempfile::tempdir().unwrap();
        let mut old = Store::open(&d.path().join("old")).unwrap();
        old.create_tag("旧标签").unwrap();
        old.conn.execute_batch("DROP TABLE capture_receipts;ALTER TABLE files DROP COLUMN web_source;ALTER TABLE files DROP COLUMN web_key;DELETE FROM schema_migrations WHERE version=4;PRAGMA user_version=3;").unwrap();
        old.validate().unwrap();
        let backup = d.path().join("v3.sqtagbackup");
        old.export_backup(&backup).unwrap();
        let mut fresh = Store::open(&d.path().join("fresh")).unwrap();
        fresh.restore_backup(&backup).unwrap();
        assert!(fresh.tags().unwrap().iter().any(|t| t.name == "旧标签"));
        fresh.capture(&json!({"requestId":id(),"kind":"web_page","title":"迁移后","url":"https://example.test/after","excerpt":""})).unwrap();
        fresh.validate().unwrap();
    }
}
#[cfg(test)]
mod maintenance_tests {
    use super::*;
    fn request(title: &str) -> Value {
        json!({"requestId":id(),"kind":"web_excerpt","title":title,"url":format!("https://example.test/maintenance/{}", id()),"excerpt":"存储恢复验收".repeat(1800)})
    }
    #[test]
    fn blocked_capture_directory_keeps_request_retriable() {
        let d = tempfile::tempdir().unwrap();
        let mut store = Store::open(d.path()).unwrap();
        let directory = d.path().join("captures");
        fs::write(&directory, b"synthetic obstruction").unwrap();
        let req = request("目录恢复");
        assert!(store
            .capture(&req)
            .unwrap_err()
            .starts_with("CAPTURE_STORAGE:"));
        assert_eq!(
            store
                .conn
                .query_row("SELECT count(*) FROM files", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            store
                .conn
                .query_row("SELECT count(*) FROM capture_receipts", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        fs::remove_file(&directory).unwrap();
        let first = store.capture(&req).unwrap();
        assert_eq!(first["duplicate"], false);
        assert_eq!(store.capture(&req).unwrap()["id"], first["id"]);
        store.validate().unwrap();
    }
    #[test]
    fn sqlite_capacity_failure_rolls_back_and_same_id_recovers() {
        let d = tempfile::tempdir().unwrap();
        let mut store = Store::open(d.path()).unwrap();
        let baseline = store.capture(&json!({"requestId":id(),"kind":"web_page","title":"保留资料","url":"https://example.test/baseline","excerpt":""})).unwrap();
        let pages: i64 = store
            .conn
            .pragma_query_value(None, "page_count", |r| r.get(0))
            .unwrap();
        let max: i64 = store
            .conn
            .pragma_query_value(None, "max_page_count", |r| r.get(0))
            .unwrap();
        store
            .conn
            .pragma_update(None, "max_page_count", pages)
            .unwrap();
        let req = request("容量恢复");
        let error = store.capture(&req).unwrap_err();
        assert!(
            error.contains("DATABASE_ERROR") && error.contains("full"),
            "{error}"
        );
        assert_eq!(
            store
                .conn
                .query_row("SELECT count(*) FROM files", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .conn
                .query_row("SELECT count(*) FROM capture_receipts", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(fs::read_dir(d.path().join("captures")).unwrap().count(), 1);
        store
            .conn
            .pragma_update(None, "max_page_count", max)
            .unwrap();
        let first = store.capture(&req).unwrap();
        assert_ne!(first["id"], baseline["id"]);
        assert_eq!(store.capture(&req).unwrap()["id"], first["id"]);
        store.validate().unwrap();
    }
}
