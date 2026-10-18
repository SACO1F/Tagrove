//! Batch review is local: a checked preview, one atomic commit, and bounded durable history.
use crate::{auto_tags::cleanup_orphan_ai_tags, db::Store, fsops, model::*};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

const MAX_ITEMS: usize = 200;
const HISTORY_LIMIT: usize = 50;
const PENDING: &str = "ft.source='ai' AND COALESCE(json_extract(ft.ai_meta,'$.confirmed'),0)=0 AND f.removed_at IS NULL";

impl Store {
    pub fn ai_review_list(&self, v: &Value) -> Result<Value> {
        let offset = v["offset"].as_u64().unwrap_or(0).min(1_000_000) as i64;
        let text = v["text"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(100)
            .collect::<String>();
        // instr treats punctuation as literal data, not SQL LIKE wildcards.
        let filter = format!("{PENDING} AND (instr(lower(f.name),lower(?1))>0 OR EXISTS(SELECT 1 FROM file_tags ft2 JOIN tags t2 ON t2.id=ft2.tag_id WHERE ft2.file_id=f.id AND ft2.source='ai' AND COALESCE(json_extract(ft2.ai_meta,'$.confirmed'),0)=0 AND instr(lower(t2.name),lower(?1))>0))");
        let total: i64 = self.conn.query_row(&format!("SELECT count(DISTINCT f.id) FROM file_tags ft JOIN files f ON f.id=ft.file_id JOIN tags t ON t.id=ft.tag_id WHERE {filter}"), [&text], |r|r.get(0)).map_err(sql_err)?;
        let suggestions: i64 = self.conn.query_row(&format!("SELECT count(*) FROM file_tags ft JOIN files f ON f.id=ft.file_id JOIN tags t ON t.id=ft.tag_id WHERE {filter}"), [&text], |r|r.get(0)).map_err(sql_err)?;
        let mut stmt = self.conn.prepare(&format!("SELECT f.id FROM file_tags ft JOIN files f ON f.id=ft.file_id JOIN tags t ON t.id=ft.tag_id WHERE {filter} GROUP BY f.id ORDER BY max(COALESCE(json_extract(ft.ai_meta,'$.updatedAt'),0)) DESC,f.id LIMIT 40 OFFSET ?2")).map_err(sql_err)?;
        let ids = stmt
            .query_map(params![text, offset], |r| r.get::<_, String>(0))
            .map_err(sql_err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(sql_err)?;
        let files = ids
            .iter()
            .map(|id| self.file(id))
            .collect::<Result<Vec<_>>>()?;
        Ok(
            json!({"files":files,"total":total,"suggestions":suggestions,"offset":offset,"hasMore":offset+40<total,"summary":self.ai_summary()?,"enabled":self.ai_config()?.enabled,"historyLimit":HISTORY_LIMIT,"batchLimit":MAX_ITEMS}),
        )
    }

    pub fn ai_review_history(&self) -> Result<Value> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key='aiReviewHistory'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_err)?;
        let history: Value = match raw {
            None => json!([]),
            Some(s) if s.len() <= 8 * 1024 * 1024 => serde_json::from_str(&s)
                .map_err(|_| err("AI_HISTORY_INVALID", "审核记录损坏，请先备份资料库"))?,
            Some(_) => {
                return Err(err(
                    "AI_HISTORY_INVALID",
                    "审核记录超出读取上限，请先备份资料库",
                ))
            }
        };
        if history.as_array().is_none_or(|a| a.len() > HISTORY_LIMIT) {
            return Err(err(
                "AI_HISTORY_INVALID",
                "审核记录格式无效，请先备份资料库",
            ));
        }
        for record in history.as_array().unwrap() {
            let valid = record["id"].as_str().is_some_and(|s| s.len() <= 64)
                && record["createdAt"].as_i64().is_some()
                && record["fileCount"].as_u64().is_some()
                && record["count"].as_u64().is_some()
                && matches!(record["action"].as_str(), Some("accept" | "reject"))
                && record["items"].as_array().is_some_and(|items| {
                    !items.is_empty()
                        && items.len() <= MAX_ITEMS
                        && items.iter().all(|item| {
                            ["fileId", "tagId", "name", "tagName"]
                                .iter()
                                .all(|k| item[k].as_str().is_some_and(|s| s.len() <= 4096))
                                && item["ai"]["model"].as_str().is_some_and(|s| s.len() <= 200)
                                && item["ai"]["reason"]
                                    .as_str()
                                    .is_some_and(|s| s.chars().count() <= 500)
                                && item["ai"]["updatedAt"].as_i64().is_some()
                        })
                });
            if !valid {
                return Err(err(
                    "AI_HISTORY_INVALID",
                    "审核记录格式无效，请先备份资料库",
                ));
            }
        }
        Ok(history)
    }

    pub fn ai_review_preview(&self, v: &Value) -> Result<Value> {
        let action = str_arg(v, "action")?;
        if !["accept", "reject"].contains(&action) {
            return Err(err("INVALID_INPUT", "请选择接受或拒绝"));
        }
        let items = v["items"]
            .as_array()
            .filter(|a| !a.is_empty() && a.len() <= MAX_ITEMS)
            .ok_or_else(|| err("INVALID_INPUT", "每次审核 1 到 200 个建议"))?;
        let mut seen = HashSet::new();
        let mut snapshots = vec![];
        let mut conflicts = vec![];
        for item in items {
            let fid = str_arg(item, "fileId")?;
            let tid = str_arg(item, "tagId")?;
            if !seen.insert((fid, tid)) {
                return Err(err("INVALID_INPUT", "审核列表包含重复建议"));
            }
            let file = match self.file(fid) {
                Ok(f) => f,
                Err(_) => {
                    conflicts.push(json!({"fileId":fid,"name":fid,"reason":"文件已移除"}));
                    continue;
                }
            };
            let conflict = |reason: &str| json!({"fileId":fid,"name":file.name,"reason":reason});
            if item["version"].as_i64() != Some(file.version) {
                conflicts.push(conflict("文件标注已变化，请重新选择"));
                continue;
            }
            let Some(tag) = file.tags.iter().find(|t| t.id == tid) else {
                conflicts.push(conflict("建议标签已移除"));
                continue;
            };
            if tag.version != item["tagVersion"].as_i64().unwrap_or(-1)
                || tag.source != "ai"
                || tag.ai.as_ref().is_none_or(|a| a.confirmed)
            {
                conflicts.push(conflict("标签已修改或已审核"));
                continue;
            }
            if file.status == "available"
                && fsops::inspect(Path::new(&file.path)).map_or(true, |meta| {
                    meta.revision != file.revision || meta.identity != file.identity
                })
            {
                conflicts.push(conflict("原文件已变化，请刷新文件状态后重新审核"));
                continue;
            }
            snapshots.push(json!({"fileId":fid,"name":file.name,"tagId":tid,"tagName":tag.name,"version":file.version,"tagVersion":tag.version,"revision":file.revision,"identity":file.identity,"status":file.status,"createdBy":tag.created_by,"ai":tag.ai}));
        }
        snapshots.sort_by_key(|s| {
            (
                s["fileId"].as_str().unwrap().to_string(),
                s["tagId"].as_str().unwrap().to_string(),
            )
        });
        let token = format!(
            "{:x}",
            Sha256::digest(
                json!({"action":action,"items":snapshots})
                    .to_string()
                    .as_bytes()
            )
        );
        let files: HashSet<_> = snapshots
            .iter()
            .map(|s| s["fileId"].as_str().unwrap())
            .collect();
        Ok(
            json!({"action":action,"items":snapshots,"token":token,"conflicts":conflicts,"fileCount":files.len(),"count":snapshots.len()}),
        )
    }

    pub fn ai_review_commit(&mut self, v: &Value) -> Result<Value> {
        let preview = self.ai_review_preview(v)?;
        if !preview["conflicts"].as_array().unwrap().is_empty() {
            return Ok(json!({"applied":0,"conflicts":preview["conflicts"]}));
        }
        if v["token"].as_str() != preview["token"].as_str() {
            return Ok(
                json!({"applied":0,"conflicts":[{"name":"审核预览","reason":"预览已过期，请重新预览"}]}),
            );
        }
        let action = str_arg(v, "action")?;
        let items = preview["items"].as_array().unwrap();
        let mut history = self.ai_review_history()?.as_array().unwrap().clone();
        // One file version increment even when several suggestions belong to the same file.
        let files: BTreeMap<_, _> = items
            .iter()
            .map(|s| {
                (
                    s["fileId"].as_str().unwrap().to_string(),
                    s["version"].as_i64().unwrap(),
                )
            })
            .collect();
        self.conn
            .execute_batch("SAVEPOINT ai_review")
            .map_err(sql_err)?;
        let result: Result<Value> = (|| {
            let mut pins = string_list(&self.floating_presets()?, "ids");
            for item in items {
                let fid = item["fileId"].as_str().unwrap();
                let tid = item["tagId"].as_str().unwrap();
                if action == "accept" {
                    self.conn.execute("UPDATE file_tags SET ai_meta=json_set(ai_meta,'$.confirmed',json('true')) WHERE file_id=? AND tag_id=?",params![fid,tid]).map_err(sql_err)?;
                    if item["createdBy"] != "folder" && !pins.iter().any(|id| id == tid) {
                        pins.push(tid.to_string());
                    }
                } else {
                    self.conn
                        .execute(
                            "DELETE FROM file_tags WHERE file_id=? AND tag_id=?",
                            params![fid, tid],
                        )
                        .map_err(sql_err)?;
                }
            }
            for fid in files.keys() {
                self.conn
                    .execute("UPDATE files SET version=version+1 WHERE id=?", [fid])
                    .map_err(sql_err)?;
            }
            cleanup_orphan_ai_tags(&self.conn)?;
            pins.retain(|tid| {
                self.conn
                    .query_row("SELECT EXISTS(SELECT 1 FROM tags WHERE id=?)", [tid], |r| {
                        r.get::<_, bool>(0)
                    })
                    .unwrap_or(false)
            });
            self.conn.execute("INSERT INTO settings(key,value) VALUES('floatingTags',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[json!(pins).to_string()]).map_err(sql_err)?;
            let record = json!({"id":id(),"createdAt":now(),"action":action,"fileCount":files.len(),"count":items.len(),"items":items});
            history.insert(0, record.clone());
            history.truncate(HISTORY_LIMIT);
            while history.len() > 1 && json!(history).to_string().len() > 6 * 1024 * 1024 {
                history.pop();
            }
            self.conn.execute("INSERT INTO settings(key,value) VALUES('aiReviewHistory',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[json!(history).to_string()]).map_err(sql_err)?;
            Ok(
                json!({"applied":items.len(),"files":files.len(),"conflicts":[],"recordId":record["id"]}),
            )
        })();
        match result {
            Ok(value) => {
                self.conn
                    .execute_batch("RELEASE ai_review")
                    .map_err(sql_err)?;
                Ok(value)
            }
            Err(e) => {
                let _ = self
                    .conn
                    .execute_batch("ROLLBACK TO ai_review; RELEASE ai_review");
                Err(e)
            }
        }
    }
}
