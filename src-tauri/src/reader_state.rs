//! Bounded, local reading metadata. Never stores document text or passwords.
use crate::{db::Store, model::*};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

pub fn migrate(conn: &rusqlite::Connection) -> Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS reader_states(id TEXT PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE, revision TEXT NOT NULL, identity TEXT NOT NULL, value TEXT NOT NULL, touched INTEGER NOT NULL);").map_err(sql_err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsops;
    #[test]
    fn reader_state_reopens_and_rejects_stale_revision() {
        let root = crate::fsops::TmpDir::new();
        let path = root.path().join("source.md");
        std::fs::write(&path, "# example").unwrap();
        let library = root.path().join("library");
        let mut store = Store::open(&library).unwrap();
        store
            .add_file_with_ai(&fsops::inspect(&path).unwrap(), false)
            .unwrap();
        let id: String = store
            .conn
            .query_row("SELECT id FROM files LIMIT 1", [], |r| r.get(0))
            .unwrap();
        let f = store.file(&id).unwrap();
        let value = json!({"flowZoom":150,"markdownTop":230.5,"textEditing":true});
        let payload = json!({"id":id,"revision":f.revision,"identity":f.identity,"value":value});
        store.save_reader_state(&payload).unwrap();
        drop(store);
        let mut store = Store::open(&library).unwrap();
        assert_eq!(
            store.reader_state(&json!({"id":id})).unwrap()["value"],
            value
        );
        std::fs::write(&path, "# externally modified longer content").unwrap();
        store.refresh_metadata(&id).unwrap();
        assert_eq!(
            store.reader_state(&json!({"id":id})).unwrap()["value"],
            json!({})
        );
        assert!(store
            .save_reader_state(&payload)
            .unwrap_err()
            .starts_with("FILE_CHANGED"));
    }
    #[test]
    fn reader_state_rejects_document_text_and_unbounded_values() {
        let root = crate::fsops::TmpDir::new();
        let path = root.path().join("test.md");
        std::fs::write(&path, "text").unwrap();
        let mut store = Store::open(&root.path().join("library")).unwrap();
        store
            .add_file_with_ai(&fsops::inspect(&path).unwrap(), false)
            .unwrap();
        let id: String = store
            .conn
            .query_row("SELECT id FROM files LIMIT 1", [], |r| r.get(0))
            .unwrap();
        let f = store.file(&id).unwrap();
        for value in [
            json!({"draft":"private document text"}),
            json!({"top":1e9}),
            json!({"bad.key":0}),
            json!({"nested":{}}),
        ] {
            assert!(store
                .save_reader_state(
                    &json!({"id":id,"revision":f.revision,"identity":f.identity,"value":value})
                )
                .is_err());
        }
        assert_eq!(
            store.reader_state(&json!({"id":id})).unwrap()["value"],
            json!({})
        );
    }
}
impl Store {
    pub fn reader_state(&self, v: &Value) -> Result<Value> {
        let f = self.file(str_arg(v, "id")?)?;
        let stored: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM reader_states WHERE id=? AND revision=? AND identity=?",
                params![f.id, f.revision, f.identity],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_err)?;
        Ok(
            json!({"revision": f.revision, "identity": f.identity, "value": stored.and_then(|s| serde_json::from_str::<Value>(&s).ok()).unwrap_or(json!({}))}),
        )
    }
    pub fn save_reader_state(&mut self, v: &Value) -> Result<Value> {
        let f = self.file(str_arg(v, "id")?)?;
        if str_arg(v, "revision")? != f.revision || str_arg(v, "identity")? != f.identity {
            return Err(err("FILE_CHANGED", "文件已变化，旧阅读位置未写入"));
        }
        let valid = v["value"].as_object().is_some_and(|o| {
            o.len() <= 128
                && o.iter().all(|(k, v)| {
                    !k.is_empty()
                        && k.len() <= 64
                        && k.bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"-_:".contains(&c))
                        && (v
                            .as_f64()
                            .is_some_and(|n| n.is_finite() && n.abs() <= 100_000_000.0)
                            || v.is_boolean()
                            || v.as_str().is_some_and(|s| {
                                matches!(
                                    s,
                                    "auto"
                                        | "page"
                                        | "width"
                                        | "fit"
                                        | "read"
                                        | "edit"
                                        | "source"
                                        | "preview"
                                )
                            }))
                })
        });
        if !valid {
            return Err(err("INVALID_INPUT", "阅读状态无效"));
        }
        let tx = self.conn.transaction().map_err(sql_err)?;
        tx.execute("INSERT INTO reader_states(id,revision,identity,value,touched) VALUES(?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,identity=excluded.identity,value=excluded.value,touched=excluded.touched",
            params![f.id,f.revision,f.identity,v["value"].to_string(),now()]).map_err(sql_err)?;
        tx.execute("DELETE FROM reader_states WHERE id IN (SELECT id FROM files WHERE removed_at IS NOT NULL) OR id NOT IN (SELECT id FROM reader_states ORDER BY touched DESC,rowid DESC LIMIT 500)", []).map_err(sql_err)?;
        tx.commit().map_err(sql_err)?;
        Ok(json_ok())
    }
}
