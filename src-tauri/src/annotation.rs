use crate::{
    db::{sv, Guard, Step, Store, Undo},
    fsops,
    model::*,
};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::{collections::HashSet, path::Path};

impl Store {
    pub fn create_workspace_tag(&mut self, name: &str) -> Result<Tag> {
        let mut pins = string_list(&self.floating_presets()?, "ids");
        self.conn
            .execute_batch("SAVEPOINT workspace_tag")
            .map_err(sql_err)?;
        let result = (|| {
            let tag = self.create_tag(name)?;
            if tag.created_by != "folder" && !pins.contains(&tag.id) {
                pins.push(tag.id.clone());
                self.save_floating_presets(&pins)?;
            }
            Ok(tag)
        })();
        match result {
            Ok(tag) => {
                self.conn
                    .execute_batch("RELEASE workspace_tag")
                    .map_err(sql_err)?;
                Ok(tag)
            }
            Err(error) => {
                let _ = self
                    .conn
                    .execute_batch("ROLLBACK TO workspace_tag; RELEASE workspace_tag");
                Err(error)
            }
        }
    }
    pub fn floating_presets(&mut self) -> Result<Value> {
        let saved: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key='floatingTags'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_err)?;
        let ids = if let Some(saved) = saved {
            serde_json::from_str::<Vec<String>>(&saved).unwrap_or_default()
        } else {
            let mut ids = vec![];
            for name in ["灵感", "参考素材", "待处理", "已完成"] {
                let tag = self.create_tag(name)?;
                if tag.created_by != "folder" {
                    ids.push(tag.id);
                }
            }
            self.save_floating_presets(&ids)?;
            ids
        };
        let previous = ids.clone();
        let tags = self.floating_tag_pool()?;
        let mut seen = HashSet::new();
        let mut ids: Vec<_> = ids
            .into_iter()
            .filter(|id| tags.iter().any(|t| &t.id == id) && seen.insert(id.clone()))
            .collect();
        // Saved shortcuts now define order, not visibility. Include all eligible
        // ordinary tags, including those created before this behavior existed.
        for tag in &tags {
            if seen.insert(tag.id.clone()) {
                ids.push(tag.id.clone());
            }
        }
        if ids != previous {
            self.save_floating_presets(&ids)?;
        }
        Ok(json!({"ids":ids,"tags":tags}))
    }
    pub fn save_floating_presets(&mut self, ids: &[String]) -> Result<Value> {
        let tags = self.tag_pool()?;
        if ids
            .iter()
            .any(|id| tags.iter().any(|t| &t.id == id && t.created_by == "folder"))
        {
            return Err(err(
                "FOLDER_TAG_NOT_ALLOWED",
                "文件夹标签不能添加到标签浮窗",
            ));
        }
        let tags = self.floating_tag_pool()?;
        let mut unique = HashSet::new();
        if ids
            .iter()
            .any(|id| !unique.insert(id) || !tags.iter().any(|t| &t.id == id))
        {
            return Err(err("TAG_NOT_FOUND", "标签已变化，请刷新后重试"));
        }
        self.conn.execute("INSERT INTO settings(key,value) VALUES('floatingTags',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [json!(ids).to_string()]).map_err(sql_err)?;
        Ok(json_ok())
    }
    pub fn floating_tag_pool(&self) -> Result<Vec<Tag>> {
        Ok(self
            .tag_pool()?
            .into_iter()
            .filter(|tag| tag.created_by != "folder")
            .collect())
    }
    /// Import and annotate as a single database operation. Failed batches leave no
    /// partial imports. Undo removes only newly added associations, retaining files.
    pub fn annotate(&mut self, v: &Value) -> Result<Value> {
        let tid = str_arg(v, "tagId")?;
        let tag = self
            .tag_pool()?
            .into_iter()
            .find(|t| t.id == tid)
            .ok_or_else(|| err("TAG_NOT_FOUND", "标签已删除，请重新选择"))?;
        let paths = string_list(v, "paths");
        let ids = string_list(v, "ids");
        if paths.len() + ids.len() == 0 || paths.len() + ids.len() > 1000 {
            return Err(err("INVALID_INPUT", "每次请选择 1～1000 个文件"));
        }
        let mut metas = vec![];
        for path in &paths {
            let m = fsops::inspect(Path::new(path))?;
            if Path::new(&m.path).starts_with(&self.root) {
                return Err(err("APP_DATA_EXCLUDED", "不能标注应用资料库文件"));
            }
            metas.push(m);
        }
        self.conn
            .execute_batch("SAVEPOINT floating_annotation")
            .map_err(sql_err)?;
        let mut undo = Undo {
            id: uuid::Uuid::new_v4().to_string(),
            label: format!("浮窗标注「{}」", tag.name),
            steps: vec![],
            guards: vec![],
        };
        let result = (|| -> Result<Value> {
            let mut unique: HashSet<String> = ids.into_iter().collect();
            let mut imported = 0;
            for m in metas {
                if self.add_file_with_ai(&m, false)? {
                    imported += 1;
                }
                let fid: String = self
                    .conn
                    .query_row("SELECT id FROM files WHERE path_key=?", [&m.path], |r| {
                        r.get(0)
                    })
                    .map_err(sql_err)?;
                unique.insert(fid);
            }
            let mut applied = 0;
            let mut names = vec![];
            for fid in &unique {
                let f = self.file(fid)?;
                let active: bool = self
                    .conn
                    .query_row(
                        "SELECT removed_at IS NULL FROM files WHERE id=?",
                        [fid],
                        |r| r.get(0),
                    )
                    .map_err(sql_err)?;
                if !active {
                    return Err(err("FILE_REMOVED", "文件记录已移除，请刷新"));
                }
                names.push(f.name);
                let previous = crate::auto_tags::association_step(&self.conn, fid, tid)?;
                let added = self
                    .conn
                    .execute(
                        "INSERT OR IGNORE INTO file_tags(file_id,tag_id) VALUES(?,?)",
                        params![fid, tid],
                    )
                    .map_err(sql_err)?;
                let confirmed = if added == 0 {
                    self.conn.execute("UPDATE file_tags SET ai_meta=json_set(ai_meta,'$.confirmed',json('true')) WHERE file_id=? AND tag_id=? AND source='ai' AND json_extract(ai_meta,'$.confirmed')=0",params![fid,tid]).map_err(sql_err)?
                } else {
                    0
                };
                if added + confirmed > 0 {
                    applied += 1;
                    self.conn
                        .execute("UPDATE files SET version=version+1 WHERE id=?", [fid])
                        .map_err(sql_err)?;
                    undo.steps.push(Step {
                        sql: "DELETE FROM file_tags WHERE file_id=? AND tag_id=?".into(),
                        args: vec![sv(fid), sv(tid)],
                    });
                    if let Some(previous) = previous {
                        undo.steps.push(previous);
                    }
                    undo.steps.push(Step {
                        sql: "UPDATE files SET version=version+1 WHERE id=?".into(),
                        args: vec![sv(fid)],
                    });
                    undo.guards.push(Guard {
                        table: "files".into(),
                        id: fid.clone(),
                        version: f.version + 1,
                    });
                }
            }
            self.conn
                .execute(
                    "UPDATE tags SET last_used=? WHERE id=?",
                    params![now(), tid],
                )
                .map_err(sql_err)?;
            Ok(
                json!({"tagName":tag.name,"applied":applied,"existing":unique.len()-applied,"imported":imported,"names":names}),
            )
        })();
        match result {
            Ok(mut value) => {
                if let Err(e) = self.conn.execute_batch("RELEASE floating_annotation") {
                    let _ = self.conn.execute_batch(
                        "ROLLBACK TO floating_annotation; RELEASE floating_annotation",
                    );
                    return Err(sql_err(e));
                }
                if !undo.steps.is_empty() {
                    value["undoId"] = json!(undo.id.clone());
                    self.undo.push(undo);
                    if self.undo.len() > 20 {
                        self.undo.remove(0);
                    }
                }
                Ok(value)
            }
            Err(e) => {
                let _ = self
                    .conn
                    .execute_batch("ROLLBACK TO floating_annotation; RELEASE floating_annotation");
                Err(e)
            }
        }
    }
}
