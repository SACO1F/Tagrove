//! Reading copies never replace original capture provenance or receipt dedupe keys.
use crate::{
    capture::ReadingCopy,
    db::{iv, sv, Guard, Step, Store, Undo},
    model::*,
};
use rusqlite::params;
use serde_json::{json, Value};

pub fn host_filter(value: &str) -> Result<String> {
    if value.trim().is_empty() {
        return Ok(String::new());
    }
    let raw = value.trim();
    let parsed = reqwest::Url::parse(&format!("https://{raw}/"))
        .map_err(|_| err("INVALID_FILTER", "请输入来源网站的完整域名"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| err("INVALID_FILTER", "来源网站无效"))?;
    if parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
        || raw.contains(['/', '?', '#', '@', '*', '\\'])
        || raw.len() > 253
    {
        return Err(err("INVALID_FILTER", "仅填写域名，不包含协议、端口或路径"));
    }
    Ok(host.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn capture(store: &mut Store, title: &str, url: &str, text: &str) -> String {
        store.capture(&json!({"requestId":id(),"kind":if text.is_empty(){"web_page"}else{"web_excerpt"},"title":title,"url":url,"excerpt":text})).unwrap()["id"].as_str().unwrap().to_owned()
    }
    fn update(store: &mut Store, fid: &str, text: &str) -> Value {
        let f = store.file(fid).unwrap();
        store
            .update_web_reading(
                &json!({"id":fid,"version":f.version,"title":"整理后的标题","text":text}),
            )
            .unwrap()
    }
    #[test]
    fn copies_preserve_provenance_receipts_conflicts_and_undo() {
        let d = crate::fsops::TmpDir::new();
        let mut s = Store::open(&d.path().join("lib")).unwrap();
        let fid = capture(
            &mut s,
            "原始标题",
            "https://example.test/page#source",
            "原始摘录",
        );
        let old = s.file(&fid).unwrap();
        let bytes = std::fs::read(&old.path).unwrap();
        update(&mut s, &fid, "修改后的正文与关键词");
        let f = s.file(&fid).unwrap();
        let src = f.web_source.as_ref().unwrap();
        assert_eq!(src.title, "原始标题");
        assert_eq!(src.excerpt, "原始摘录");
        assert_eq!(
            src.captured_at,
            old.web_source.as_ref().unwrap().captured_at
        );
        assert_eq!(src.reading_text(), "修改后的正文与关键词");
        assert_eq!(src.format, 3);
        assert_eq!(std::fs::read(&old.path).unwrap(), bytes);
        assert_ne!(f.path, old.path);
        let descriptor: crate::capture::Source =
            serde_json::from_slice(&std::fs::read(&f.path).unwrap()).unwrap();
        assert_eq!(descriptor.reading_text(), "修改后的正文与关键词");
        assert_ne!(f.revision, old.revision);
        assert_eq!(f.version, old.version + 1);
        assert_eq!(
            capture(&mut s, "重复采集", "https://example.test/page", "原始摘录"),
            fid
        );
        assert!(s
            .update_web_reading(
                &json!({"id":fid,"version":old.version,"title":"冲突","text":"不能覆盖"})
            )
            .unwrap_err()
            .contains("VERSION_CONFLICT"));
        let n = s.undo.len();
        update(&mut s, &fid, "修改后的正文与关键词");
        assert_eq!(s.undo.len(), n);
        s.undo_last().unwrap();
        assert!(s.file(&fid).unwrap().web_source.unwrap().reading.is_none());
        update(&mut s, &fid, "新内容");
        let f = s.file(&fid).unwrap();
        s.update_web_reading(&json!({"id":fid,"version":f.version,"reset":true}))
            .unwrap();
        assert_eq!(s.file(&fid).unwrap().name, "原始标题");
        s.undo_last().unwrap();
        assert_eq!(
            s.file(&fid).unwrap().web_source.unwrap().reading_text(),
            "新内容"
        );
        for text in ["\0".to_string(), "字".repeat(20001)] {
            assert!(s.update_web_reading(&json!({"id":fid,"version":s.file(&fid).unwrap().version,"title":"标题","text":text})).is_err());
        }
        s.conn.execute_batch("CREATE TRIGGER fail_copy BEFORE UPDATE ON files BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
        let before = serde_json::to_value(s.file(&fid).unwrap()).unwrap();
        assert!(s
            .update_web_reading(
                &json!({"id":fid,"version":before["version"],"title":"失败","text":"失败写入"})
            )
            .is_err());
        assert_eq!(serde_json::to_value(s.file(&fid).unwrap()).unwrap(), before);
    }
    #[test]
    fn current_copy_search_reindexes_without_stale_original_body() {
        let d = crate::fsops::TmpDir::new();
        let mut s = Store::open(d.path()).unwrap();
        let fid = capture(&mut s, "资料", "https://example.test/", "旧正文独有");
        s.search_control("enable").unwrap();
        let work = s.search_next().unwrap().unwrap();
        s.search_commit(&work, crate::search::read(&work)).unwrap();
        update(&mut s, &fid, "新正文独有 <script>plain text</script>");
        for mode in ["unified", "content"] {
            assert_eq!(
                s.query(&Query {
                    text: "旧正文独有".into(),
                    search_mode: mode.into(),
                    limit: 100,
                    ..Default::default()
                })
                .unwrap()["total"],
                0
            );
        }
        let next = s.search_next().unwrap().unwrap();
        s.search_commit(&next, crate::search::read(&next)).unwrap();
        let found = s
            .query(&Query {
                text: "新正文独有".into(),
                search_mode: "content".into(),
                limit: 100,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(found["total"], 1);
        assert!(found["files"][0]["contentMatch"]["snippet"]
            .as_str()
            .unwrap()
            .contains("新正文独有"));
    }
    #[test]
    fn website_filters_are_exact_host_and_collections_remember_conditions() {
        let d = crate::fsops::TmpDir::new();
        let mut s = Store::open(d.path()).unwrap();
        capture(&mut s, "A", "https://example.test/a", "摘录");
        capture(&mut s, "B", "http://example.test:8080/b", "");
        capture(&mut s, "C", "https://sub.example.test/a", "");
        capture(&mut s, "D", "https://example.test.evil.test/a", "");
        let mut q = Query {
            source_host: "EXAMPLE.TEST".into(),
            limit: 100,
            scope: "all".into(),
            mode: "all".into(),
            sort: "added".into(),
            direction: "desc".into(),
            ..Default::default()
        };
        assert_eq!(s.query(&q).unwrap()["total"], 2);
        q.capture_kind = "web_excerpt".into();
        assert_eq!(s.query(&q).unwrap()["total"], 1);
        let collection = s
            .collection_save(&json!({"name":"网页摘录集合","query":q}))
            .unwrap();
        assert_eq!(collection["query"]["sourceHost"], "example.test");
        assert_eq!(collection["query"]["captureKind"], "web_excerpt");
        for bad in [
            "https://example.test",
            "example.test/path",
            "*.example.test",
            "x@y.test",
            "example.test:99",
        ] {
            q.source_host = bad.into();
            assert!(s.query(&q).is_err());
        }
        assert_eq!(s.capture_sites().unwrap().as_array().unwrap().len(), 3);
    }
    #[test]
    fn copy_backup_and_package_keep_original_and_edited_content() {
        let d = crate::fsops::TmpDir::new();
        let mut s = Store::open(&d.path().join("lib")).unwrap();
        let fid = capture(&mut s, "采集标题", "https://example.test/a", "原始正文");
        update(&mut s, &fid, "阅读副本");
        let archive = d.path().join("copy.sqtagbackup");
        s.export_backup(&archive).unwrap();
        let mut restored = Store::open(&d.path().join("restored")).unwrap();
        restored.restore_backup(&archive).unwrap();
        let source = restored.file(&fid).unwrap().web_source.unwrap();
        assert_eq!(source.excerpt, "原始正文");
        assert_eq!(source.reading_text(), "阅读副本");
        let plan = s.transfer_plan(&json!({"ids":[fid]})).unwrap();
        let fingerprint = plan.info["fingerprint"].as_str().unwrap().to_owned();
        let pack = d.path().join("copy.sqtagpack");
        let control = crate::transfer::Control::default();
        crate::transfer::export(plan, &pack, &fingerprint, &control).unwrap();
        let preview = crate::transfer::preview(&pack, &control).unwrap();
        let mut receiver = Store::open(&d.path().join("receiver")).unwrap();
        let destination = d.path().join("received");
        std::fs::create_dir(&destination).unwrap();
        receiver
            .import_package(
                &pack,
                &destination,
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
        assert_eq!(files["files"][0]["name"], "整理后的标题");
        assert_eq!(files["files"][0]["webSource"]["excerpt"], "原始正文");
        assert_eq!(
            files["files"][0]["webSource"]["reading"]["text"],
            "阅读副本"
        );
        assert!(Path::new(files["files"][0]["path"].as_str().unwrap()).exists());
    }
}

impl Store {
    pub fn update_web_reading(&mut self, v: &Value) -> Result<Value> {
        let fid = str_arg(v, "id")?;
        let file = self.file(fid)?;
        let active: bool = self
            .conn
            .query_row(
                "SELECT removed_at IS NULL FROM files WHERE id=?",
                [fid],
                |r| r.get(0),
            )
            .map_err(sql_err)?;
        if !active || file.kind != "web" {
            return Err(err("INVALID_INPUT", "仅支持编辑网页链接或摘录的阅读副本"));
        }
        if v["version"].as_i64() != Some(file.version) {
            return Err(err(
                "VERSION_CONFLICT",
                "资料已变化；草稿已保留，请重新读取后核对",
            ));
        }
        let mut source = file
            .web_source
            .clone()
            .ok_or_else(|| err("CAPTURE_INVALID", "网页来源缺失"))?;
        let old = serde_json::to_string(&source).map_err(|e| err("CAPTURE_INVALID", e))?;
        if v["reset"].as_bool() == Some(true) {
            if source.reading.is_none() {
                return Ok(json!({"file":file,"changed":false}));
            }
            source.reading = None;
            source.format = 1;
        } else {
            let title = str_arg(v, "title")?.trim().to_owned();
            let text = str_arg(v, "text")?.to_owned();
            if title == source.display_title() && text == source.reading_text() {
                return Ok(json!({"file":file,"changed":false}));
            }
            source.format = 3;
            source.reading = Some(ReadingCopy {
                title,
                text,
                updated_at: now(),
            });
        }
        source.validate()?;
        let raw = serde_json::to_string(&source).map_err(|e| err("CAPTURE_INVALID", e))?;
        use std::io::Write;
        let folder = self.root.join("captures");
        std::fs::create_dir_all(&folder).map_err(|e| err("CAPTURE_STORAGE", e))?;
        if crate::fsops::is_link(
            &std::fs::symlink_metadata(&folder).map_err(|e| err("CAPTURE_STORAGE", e))?,
        ) {
            return Err(err("CAPTURE_STORAGE", "采集目录不能是链接"));
        }
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
        let outcome = (|| -> Result<()> {
            let m = crate::fsops::inspect(&path)?;
            let tx = self.conn.transaction().map_err(sql_err)?;
            let changed = tx.execute("UPDATE files SET name=?,name_key=?,web_source=?,web_key=?,path=?,path_key=?,parent=?,bytes=?,revision=?,identity=?,modified=?,version=version+1 WHERE id=? AND version=? AND removed_at IS NULL", params![source.display_title(),key(source.display_title()),raw,key(&source.text()),m.path,m.path,m.parent,m.bytes,m.revision,m.identity,m.modified,fid,file.version]).map_err(sql_err)?;
            if changed != 1 {
                return Err(err("VERSION_CONFLICT", "资料已变化，请重新读取"));
            }
            tx.commit().map_err(sql_err)?;
            Ok(())
        })();
        if let Err(error) = outcome {
            let _ = std::fs::remove_file(&path);
            return Err(error);
        }
        self.undo.push(Undo { id:id(),label:format!("编辑网页阅读副本「{}」", source.display_title()),
            steps:vec![Step { sql:"UPDATE files SET name=?,name_key=?,web_source=?,web_key=?,path=?,path_key=?,parent=?,bytes=?,revision=?,identity=?,modified=?,version=version+1 WHERE id=?".into(),args:vec![sv(&file.name),sv(key(&file.name)),sv(old),sv(key(&file.web_source.as_ref().unwrap().text())),sv(&file.path),sv(&file.path),sv(file.parent),iv(file.bytes),sv(file.revision),sv(file.identity),iv(file.modified),sv(fid)] }],
            guards:vec![Guard{table:"files".into(),id:fid.into(),version:file.version+1}] });
        if self.undo.len() > 30 {
            self.undo.remove(0);
        }
        Ok(json!({"file":self.file(fid)?,"changed":true,"undoId":self.undo.last().map(|u|u.id.clone())}))
    }
    pub fn capture_sites(&self) -> Result<Value> {
        let mut st = self
            .conn
            .prepare("SELECT web_source FROM files WHERE removed_at IS NULL AND web_source<>''")
            .map_err(sql_err)?;
        let mut counts = std::collections::BTreeMap::<String, u64>::new();
        for row in st
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(sql_err)?
        {
            let raw = row.map_err(sql_err)?;
            let source: crate::capture::Source =
                serde_json::from_str(&raw).map_err(|_| err("CAPTURE_INVALID", "网页来源无效"))?;
            if let Ok(url) = reqwest::Url::parse(&source.url) {
                if let Some(host) = url.host_str() {
                    *counts.entry(host.to_lowercase()).or_default() += 1;
                }
            }
        }
        Ok(json!(counts
            .into_iter()
            .map(|(host, count)| json!({"host":host,"count":count}))
            .collect::<Vec<_>>()))
    }
}
