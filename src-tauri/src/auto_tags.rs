use crate::{
    db::{iv, sv, validate_tag, Guard, Step, Store, Undo},
    model::*,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::path::Path;
use unicode_segmentation::UnicodeSegmentation;

pub(crate) fn cleanup_orphan_ai_tags(conn: &Connection) -> Result<Vec<(String, Step)>> {
    let mut statement = conn.prepare("SELECT id,name,name_key,version,last_used FROM tags WHERE created_by='ai' AND NOT EXISTS(SELECT 1 FROM file_tags WHERE tag_id=tags.id)").map_err(sql_err)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(sql_err)?;
    let rows = rows
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(sql_err)?;
    drop(statement);
    let mut restores = vec![];
    for (id, name, name_key, version, used) in rows {
        conn.execute("DELETE FROM tags WHERE id=?", [&id])
            .map_err(sql_err)?;
        restores.push((id.clone(), Step {
            sql: "INSERT INTO tags(id,name,name_key,version,last_used,created_by) VALUES(?,?,?,?,?,'ai')".into(),
            args: vec![sv(id), sv(name), sv(name_key), iv(version), iv(used)],
        }));
    }
    Ok(restores)
}

pub fn migrate(conn: &Connection) -> Result<()> {
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(sql_err)?;
    if version == 1 {
        conn.execute_batch(
            "BEGIN IMMEDIATE;
            ALTER TABLE tags ADD COLUMN created_by TEXT NOT NULL DEFAULT 'manual';
            ALTER TABLE file_tags ADD COLUMN source TEXT NOT NULL DEFAULT 'manual';
            ALTER TABLE file_tags ADD COLUMN ai_meta TEXT NOT NULL DEFAULT '{}';
            CREATE TABLE ai_jobs(file_id TEXT PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,
              status TEXT NOT NULL,error TEXT NOT NULL DEFAULT '',updated_at INTEGER NOT NULL);
            INSERT INTO schema_migrations VALUES(2,CAST(strftime('%s','now') AS INTEGER)*1000);
            PRAGMA user_version=2; COMMIT;",
        )
        .map_err(sql_err)?;
    }
    Ok(())
}

pub fn association_step(conn: &Connection, fid: &str, tid: &str) -> Result<Option<Step>> {
    conn.query_row(
        "SELECT source,ai_meta FROM file_tags WHERE file_id=? AND tag_id=?",
        params![fid, tid],
        |r| {
            Ok(Step {
                sql: "INSERT INTO file_tags(file_id,tag_id,source,ai_meta) VALUES(?,?,?,?)".into(),
                args: vec![
                    sv(fid),
                    sv(tid),
                    sv(r.get::<_, String>(0)?),
                    sv(r.get::<_, String>(1)?),
                ],
            })
        },
    )
    .optional()
    .map_err(sql_err)
}

impl Store {
    pub fn folder_tags_enabled(&self) -> Result<bool> {
        let setting: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key='folderAutoTagging'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_err)?;
        Ok(setting.as_deref() != Some("false"))
    }

    pub fn sync_folder_tag(&mut self, fid: &str, parent: &str) -> Result<bool> {
        if !self.folder_tags_enabled()? {
            return Ok(false);
        }
        let Some(name) = Path::new(parent).file_name().and_then(|n| n.to_str()) else {
            return Ok(false);
        };
        let name = name.trim();
        if name.is_empty() {
            return Ok(false);
        }
        let tag = self.create_tag_with_source(name, "folder")?;
        let removed = self
            .conn
            .execute(
                "DELETE FROM file_tags WHERE file_id=? AND source='folder' AND tag_id<>?",
                params![fid, tag.id],
            )
            .map_err(sql_err)?;
        let added = self.conn.execute("INSERT INTO file_tags(file_id,tag_id,source) VALUES(?,?,'folder') ON CONFLICT(file_id,tag_id) DO UPDATE SET source='folder',ai_meta='{}' WHERE source='ai' AND json_extract(ai_meta,'$.confirmed')=0",params![fid,tag.id]).map_err(sql_err)?;
        if removed + added > 0 {
            self.conn
                .execute("UPDATE files SET version=version+1 WHERE id=?", [fid])
                .map_err(sql_err)?;
        }
        Ok(removed + added > 0)
    }
    // Recover only exact legacy generated names. User-renamed tags are untouched.
    pub fn restore_full_folder_names(&self) -> Result<()> {
        use sha2::{Digest, Sha256};
        let mut st=self.conn.prepare("SELECT t.id,t.name,f.parent FROM tags t JOIN file_tags ft ON ft.tag_id=t.id JOIN files f ON f.id=ft.file_id WHERE t.created_by='folder' AND ft.source='folder' AND t.name LIKE '%…________' AND f.removed_at IS NULL").map_err(sql_err)?;
        let rows = st
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(sql_err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql_err)?;
        drop(st);
        let mut candidates = std::collections::HashMap::<String, (String, String, bool)>::new();
        for (tid, old, parent) in rows {
            let Some(name) = Path::new(&parent).file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let name = name.trim();
            if name.graphemes(true).count() <= 40 {
                continue;
            }
            let legacy = format!(
                "{}…{}",
                name.graphemes(true).take(31).collect::<String>(),
                &format!("{:x}", Sha256::digest(name.as_bytes()))[..8]
            );
            if old != legacy {
                continue;
            }
            let entry = candidates
                .entry(tid)
                .or_insert((old, name.to_owned(), false));
            if entry.1 != name {
                entry.2 = true;
            }
        }
        self.conn
            .execute_batch("SAVEPOINT folder_names")
            .map_err(sql_err)?;
        let result = (|| {
            for (tid, (old, name, ambiguous)) in candidates {
                if ambiguous {
                    continue;
                }
                crate::db::validate_tag_source(&name, "folder")?;
                self.conn.execute("UPDATE tags SET name=?,name_key=?,version=version+1 WHERE id=? AND name=? AND NOT EXISTS(SELECT 1 FROM tags other WHERE other.name_key=? AND other.id<>?)",params![name,key(&name),tid,old,key(&name),tid]).map_err(sql_err)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => self
                .conn
                .execute_batch("RELEASE folder_names")
                .map_err(sql_err),
            Err(e) => {
                let _ = self
                    .conn
                    .execute_batch("ROLLBACK TO folder_names;RELEASE folder_names");
                Err(e)
            }
        }
    }

    pub fn fill_folder_tags(&mut self) -> Result<Value> {
        let files = self.all_paths()?;
        self.conn
            .execute_batch("SAVEPOINT folder_fill")
            .map_err(sql_err)?;
        let result = (|| {
            let mut changed = 0;
            for (fid, _) in &files {
                let f = self.file(fid)?;
                if self.sync_folder_tag(fid, &f.parent)? {
                    changed += 1;
                }
            }
            Ok(json!({"changed":changed,"total":files.len()}))
        })();
        match result {
            Ok(value) => {
                self.conn
                    .execute_batch("RELEASE folder_fill")
                    .map_err(sql_err)?;
                Ok(value)
            }
            Err(e) => {
                let _ = self
                    .conn
                    .execute_batch("ROLLBACK TO folder_fill; RELEASE folder_fill");
                Err(e)
            }
        }
    }

    pub fn ai_task(&self, fid: &str) -> Result<Option<AiTask>> {
        self.conn
            .query_row(
                "SELECT status,error,updated_at FROM ai_jobs WHERE file_id=?",
                [fid],
                |r| {
                    Ok(AiTask {
                        status: r.get(0)?,
                        error: r.get(1)?,
                        updated_at: r.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(sql_err)
    }
    pub fn queue_ai(&self, ids: &[String]) -> Result<()> {
        if ids.is_empty() || ids.len() > 1000 {
            return Err(err("INVALID_INPUT", "每次请选择 1～1000 个文件"));
        }
        for fid in ids {
            self.file(fid)?;
        }
        for fid in ids {
            // Running requests use a token (updated_at) and cannot overwrite a newer request.
            self.conn.execute("INSERT INTO ai_jobs(file_id,status,updated_at) VALUES(?,'queued',?) ON CONFLICT(file_id) DO UPDATE SET status='queued',error='',updated_at=MAX(ai_jobs.updated_at+1,excluded.updated_at)",params![fid,now()]).map_err(sql_err)?;
        }
        Ok(())
    }
    pub fn cancel_ai_files(&self, ids: &[String]) -> Result<Value> {
        if ids.is_empty() || ids.len() > 1000 {
            return Err(err("INVALID_INPUT", "每次请选择 1～1000 个文件"));
        }
        for fid in ids {
            self.file(fid)?;
        }
        self.conn
            .execute_batch("SAVEPOINT ai_cancel_files")
            .map_err(sql_err)?;
        let result = (|| {
            let mut cancelled = 0;
            for fid in ids {
                cancelled += self.conn.execute("UPDATE ai_jobs SET status='cancelled',error='已取消，可重新识别' WHERE file_id=? AND status IN ('queued','running')", [fid]).map_err(sql_err)?;
            }
            Ok(json!({ "cancelled": cancelled }))
        })();
        match result {
            Ok(value) => {
                self.conn
                    .execute_batch("RELEASE ai_cancel_files")
                    .map_err(sql_err)?;
                Ok(value)
            }
            Err(error) => {
                let _ = self
                    .conn
                    .execute_batch("ROLLBACK TO ai_cancel_files; RELEASE ai_cancel_files");
                Err(error)
            }
        }
    }
    pub fn task_status(&self, fid: &str, token: i64, status: &str, error: &str) -> Result<()> {
        self.conn
            .execute(
                "UPDATE ai_jobs SET status=?,error=? WHERE file_id=? AND updated_at=?",
                params![status, error, fid, token],
            )
            .map_err(sql_err)?;
        Ok(())
    }
    pub fn take_ai_job(&mut self) -> Result<Option<(FileRecord, i64)>> {
        let next:Option<(String,i64)>=self.conn.query_row("SELECT j.file_id,j.updated_at FROM ai_jobs j JOIN files f ON f.id=j.file_id WHERE j.status='queued' AND f.removed_at IS NULL ORDER BY j.updated_at LIMIT 1",[],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(sql_err)?;
        if let Some((fid, token)) = next {
            self.task_status(&fid, token, "running", "")?;
            match self.refresh_file(&fid) {
                Ok(f) => Ok(Some((f, token))),
                Err(e) => {
                    self.task_status(&fid, token, "failed", &e)?;
                    Ok(None)
                }
            }
        } else {
            Ok(None)
        }
    }
    pub fn ai_summary(&self) -> Result<Value> {
        let mut result = serde_json::Map::new();
        for status in [
            "queued",
            "running",
            "done",
            "failed",
            "unsupported",
            "cancelled",
        ] {
            let count:i64=self.conn.query_row("SELECT COUNT(*) FROM ai_jobs j JOIN files f ON f.id=j.file_id WHERE j.status=? AND f.removed_at IS NULL",[status],|r|r.get(0)).map_err(sql_err)?;
            result.insert(status.into(), json!(count));
        }
        Ok(Value::Object(result))
    }
    pub fn confirm_ai(&mut self, v: &Value) -> Result<Value> {
        let fid = str_arg(v, "id")?;
        let tag_id = str_arg(v, "tagId")?;
        let file = self.file(fid)?;
        if v["version"].as_i64() != Some(file.version) {
            return Err(err("VERSION_CONFLICT", "文件标注已更新，请重试"));
        }
        let tag = file
            .tags
            .iter()
            .find(|t| t.id == tag_id)
            .ok_or_else(|| err("TAG_NOT_FOUND", "标签已移除"))?;
        if tag.source != "ai" || tag.ai.as_ref().is_none_or(|a| a.confirmed) {
            return Ok(json_ok());
        }
        let old = association_step(&self.conn, fid, tag_id)?.unwrap();
        // Floating shortcuts are preferences. Add once without replacing the
        // user's other pins; keep the association's provenance for reanalysis.
        let mut pins = string_list(&self.floating_presets()?, "ids");
        if tag.created_by != "folder" && !pins.iter().any(|id| id == tag_id) {
            pins.push(tag_id.into());
        }
        let tx = self.conn.transaction().map_err(sql_err)?;
        tx.execute("UPDATE file_tags SET ai_meta=json_set(ai_meta,'$.confirmed',json('true')) WHERE file_id=? AND tag_id=?",params![fid,tag_id]).map_err(sql_err)?;
        tx.execute("UPDATE files SET version=version+1 WHERE id=?", [fid])
            .map_err(sql_err)?;
        tx.execute("INSERT INTO settings(key,value) VALUES('floatingTags',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [json!(pins).to_string()]).map_err(sql_err)?;
        tx.commit().map_err(sql_err)?;
        self.remember(Undo {
            id: uuid::Uuid::new_v4().to_string(),
            label: "确认 AI 标注".into(),
            steps: vec![
                Step {
                    sql: "DELETE FROM file_tags WHERE file_id=? AND tag_id=?".into(),
                    args: vec![sv(fid), sv(tag_id)],
                },
                old,
                Step {
                    sql: "UPDATE files SET version=version+1 WHERE id=?".into(),
                    args: vec![sv(fid)],
                },
            ],
            guards: vec![Guard {
                table: "files".into(),
                id: fid.into(),
                version: file.version + 1,
            }],
        });
        Ok(json_ok())
    }

    pub fn apply_ai(
        &mut self,
        file: &FileRecord,
        token: i64,
        model: &str,
        choices: &[(Option<String>, String, String)],
    ) -> Result<()> {
        let current = self.refresh_file(&file.id)?;
        if current.version != file.version
            || current.revision != file.revision
            || current.identity != file.identity
            || current.status != "available"
        {
            return Err(err("FILE_CHANGED", "识别期间文件或标注已变化，请重新识别"));
        }
        self.apply_ai_checked(file, token, model, choices)
    }
    pub(crate) fn apply_ai_checked(
        &mut self,
        file: &FileRecord,
        token: i64,
        model: &str,
        choices: &[(Option<String>, String, String)],
    ) -> Result<()> {
        let current = self.file(&file.id)?;
        if current.version != file.version
            || current.revision != file.revision
            || current.identity != file.identity
            || current.status != "available"
        {
            return Err(err(
                "FILE_CHANGED",
                "识别期间文件记录或标注已变化，请重新识别",
            ));
        }
        if self
            .ai_task(&file.id)?
            .is_none_or(|j| j.updated_at != token || j.status != "running")
        {
            return Err(err("AI_CANCELLED", "识别请求已被更新或取消"));
        }
        // Validate every choice before making any changes; labels are data, never commands.
        let pool = self.tag_pool()?;
        let choices: Vec<_> = choices.iter().filter(|(tid,name,_)|
            !crate::ai::is_folder_suggestion(&pool, tid.as_deref(), name)
        ).collect();
        for (tid, name, reason) in &choices {
            validate_tag(name)?;
            if reason.chars().count() > 500 {
                return Err(err("AI_RESPONSE_INVALID", "标签说明过长"));
            }
            if let Some(tid) = tid {
                if !pool
                    .iter()
                    .any(|t| &t.id == tid && key(&t.name) == key(name))
                {
                    return Err(err("TAG_CHANGED", "已有标签池已变化，请重新识别"));
                }
            }
        }
        self.conn
            .execute_batch("SAVEPOINT ai_apply")
            .map_err(sql_err)?;
        let result = (|| {
            self.conn.execute("DELETE FROM file_tags WHERE file_id=? AND source='ai' AND COALESCE(json_extract(ai_meta,'$.confirmed'),0)=0",[&file.id]).map_err(sql_err)?;
            for (tid, name, reason) in &choices {
                let tid = match tid {
                    Some(tid) => tid.clone(),
                    None => self.create_tag_with_source(name, "ai")?.id,
                };
                let metadata = AiProvenance {
                    model: model.into(),
                    reason: reason.into(),
                    updated_at: now(),
                    confirmed: false,
                };
                self.conn.execute("INSERT OR IGNORE INTO file_tags(file_id,tag_id,source,ai_meta) VALUES(?,?,'ai',?)",params![file.id,tid,json!(metadata).to_string()]).map_err(sql_err)?;
                self.conn
                    .execute(
                        "UPDATE tags SET last_used=? WHERE id=?",
                        params![now(), tid],
                    )
                    .map_err(sql_err)?;
            }
            self.conn
                .execute("UPDATE files SET version=version+1 WHERE id=?", [&file.id])
                .map_err(sql_err)?;
            self.task_status(&file.id, token, "done", "")?;
            cleanup_orphan_ai_tags(&self.conn)?;
            Ok(())
        })();
        match result {
            Ok(()) => self.conn.execute_batch("RELEASE ai_apply").map_err(sql_err),
            Err(e) => {
                let _ = self
                    .conn
                    .execute_batch("ROLLBACK TO ai_apply; RELEASE ai_apply");
                Err(e)
            }
        }
    }
}
