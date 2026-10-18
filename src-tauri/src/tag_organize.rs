use crate::{
    db::{iv, sv, validate_tag, Guard, Step, Store, Undo},
    model::*,
};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::collections::HashSet;

fn step(sql: &str, args: Vec<rusqlite::types::Value>) -> Step {
    Step {
        sql: sql.into(),
        args,
    }
}
fn setting(s: &Store, k: &str) -> Result<String> {
    Ok(s.conn
        .query_row("SELECT value FROM settings WHERE key=?", [k], |r| r.get(0))
        .optional()
        .map_err(sql_err)?
        .unwrap_or_else(|| "[]".into()))
}
fn put(conn: &rusqlite::Connection, k: &str, value: &str) -> Result<()> {
    conn.execute("INSERT INTO settings(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![k,value]).map_err(sql_err)?;
    Ok(())
}
impl Store {
    pub fn collection_save(&mut self, v: &Value) -> Result<Value> {
        let mut all: Vec<Value> = serde_json::from_str(&setting(self, "savedCollections")?)
            .map_err(|_| err("INVALID_INPUT", "保存的筛选数据无效"))?;
        if let Some(expected) = v.get("expected") {
            if all.iter().find(|c| c["id"] == v["id"]) != Some(expected) {
                return Err(err("VERSION_CONFLICT", "集合已变化，请重新加载"));
            }
        }
        let tid = v["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(id);
        let name = validate_tag(str_arg(v, "name")?)?;
        if all
            .iter()
            .any(|x| x["id"] != tid && x["name"].as_str().map(key) == Some(key(&name)))
        {
            return Err(err("NAME_CONFLICT", "已存在同名筛选集合"));
        }
        if all.len() >= 50 && !all.iter().any(|x| x["id"] == tid) {
            return Err(err("LIMIT_REACHED", "最多保存 50 个筛选集合"));
        }
        let mut q: Query = serde_json::from_value(v["query"].clone())
            .map_err(|_| err("INVALID_INPUT", "筛选条件无效"))?;
        if q.search_mode == "semantic" || !q.semantic_session.is_empty() {
            return Err(err(
                "COLLECTION_SEMANTIC",
                "语义结果是临时会话，不能保存为集合；请先切换精确检索或应用普通筛选",
            ));
        }
        if !["all", "inbox", "starred", "recent", "unavailable"].contains(&q.scope.as_str())
            || !["all", "any"].contains(&q.mode.as_str())
            || !["", "metadata", "content", "unified"].contains(&q.search_mode.as_str())
            || !["", "all", "name", "body"].contains(&q.search_scope.as_str())
            || q.text.len() > 2048
            || q.directory.len() > 16384
            || q.include.len() + q.exclude.len() > 200
            || q.kinds
                .iter()
                .any(|k| !["image", "pdf", "text", "office", "other", "web"].contains(&k.as_str()))
            || !["", "available", "missing", "offline", "inaccessible"].contains(&q.status.as_str())
            || !["added", "modified", "name", "size", "relevance"].contains(&q.sort.as_str())
            || !["asc", "desc"].contains(&q.direction.as_str())
        {
            return Err(err("INVALID_INPUT", "筛选条件超出支持范围"));
        }
        let tags = self.tags()?;
        q.source_host = crate::web_reading::host_filter(&q.source_host)?;
        if !["", "web_page", "web_excerpt", "web_image"].contains(&q.capture_kind.as_str()) {
            return Err(err("INVALID_INPUT", "采集类型无效"));
        }
        if q.include
            .iter()
            .chain(&q.exclude)
            .any(|id| !tags.iter().any(|t| &t.id == id))
        {
            return Err(err("TAG_NOT_FOUND", "筛选标签已删除，请调整条件"));
        }
        if q.include.iter().any(|id| q.exclude.contains(id)) {
            return Err(err("INVALID_INPUT", "同一标签不能同时包含和排除"));
        }
        q.include.sort();
        q.include.dedup();
        q.exclude.sort();
        q.exclude.dedup();
        q.offset = 0;
        q.limit = 100;
        let item = json!({"id":tid,"name":name,"query":q});
        if let Some(i) = all.iter().position(|x| x["id"] == tid) {
            all[i] = item.clone();
        } else {
            all.push(item.clone());
        }
        put(&self.conn, "savedCollections", &json!(all).to_string())?;
        Ok(item)
    }
    pub fn collection_delete(&self, v: &Value) -> Result<Value> {
        let id = str_arg(v, "id")?;
        let mut all: Vec<Value> = serde_json::from_str(&setting(self, "savedCollections")?)
            .map_err(|_| err("INVALID_INPUT", "筛选数据无效"))?;
        all.retain(|x| x["id"] != id);
        put(&self.conn, "savedCollections", &json!(all).to_string())?;
        Ok(json_ok())
    }
    pub fn merge_preview(&self, v: &Value) -> Result<Value> {
        let source = str_arg(v, "source")?;
        let target = str_arg(v, "target")?;
        if source == target {
            return Err(err("INVALID_INPUT", "请选择两个不同的标签"));
        }
        let pool = self.tag_pool()?;
        let a = pool
            .iter()
            .find(|t| t.id == source)
            .ok_or_else(|| err("TAG_NOT_FOUND", "源标签已变化"))?;
        let b = pool
            .iter()
            .find(|t| t.id == target)
            .ok_or_else(|| err("TAG_NOT_FOUND", "目标标签已变化"))?;
        if a.created_by == "folder" || b.created_by == "folder" {
            return Err(err("FOLDER_TAG_NOT_ALLOWED", "文件夹标签不参与合并"));
        }
        let (affected, overlap):(i64,i64)=self.conn.query_row("SELECT COUNT(*),COALESCE(SUM(EXISTS(SELECT 1 FROM file_tags b WHERE b.file_id=a.file_id AND b.tag_id=?)),0) FROM file_tags a JOIN files f ON f.id=a.file_id WHERE a.tag_id=? AND f.removed_at IS NULL",params![target,source],|r|Ok((r.get(0)?,r.get(1)?))).map_err(sql_err)?;
        let mut st=self.conn.prepare("SELECT f.id,f.version FROM files f JOIN file_tags t ON t.file_id=f.id WHERE t.tag_id=? ORDER BY f.id").map_err(sql_err)?;
        let files: Vec<(String, i64)> = st
            .query_map([source], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(sql_err)?
            .collect::<std::result::Result<_, _>>()
            .map_err(sql_err)?;
        let mut alias_keys: HashSet<String> = a
            .aliases
            .iter()
            .chain(&b.aliases)
            .chain(std::iter::once(&a.name))
            .map(|n| key(n))
            .collect();
        alias_keys.remove(&key(&b.name));
        let alias_conflicts: Vec<String> = if alias_keys.len() > 20 {
            vec![format!(
                "合并后有 {} 个别名，超过 20 个上限，请先整理别名",
                alias_keys.len()
            )]
        } else {
            vec![]
        };
        Ok(
            json!({"source":a,"target":b,"affected":affected,"overlap":overlap,"files":files,"aliasCount":alias_keys.len(),"aliasConflicts":alias_conflicts,"pins":setting(self,"floatingTags")?,"collections":setting(self,"savedCollections")?}),
        )
    }
    pub fn merge_tags(&mut self, v: &Value) -> Result<Value> {
        let preview = self.merge_preview(v)?;
        if v["preview"] != preview {
            return Err(err(
                "VERSION_CONFLICT",
                "标签或关联已变化，请重新查看合并预览",
            ));
        }
        let source = str_arg(v, "source")?;
        let target = str_arg(v, "target")?;
        let source_aliases = crate::tag_aliases::names(&self.conn, source)?;
        let target_aliases = crate::tag_aliases::names(&self.conn, target)?;
        let mut merged_aliases = target_aliases.clone();
        for a in source_aliases.iter().chain(std::iter::once(
            &preview["source"]["name"].as_str().unwrap().to_string(),
        )) {
            if key(a) != key(preview["target"]["name"].as_str().unwrap())
                && !merged_aliases.iter().any(|b| key(a) == key(b))
            {
                merged_aliases.push(a.clone());
            }
        }
        if merged_aliases.len() > 20 {
            return Err(err("LIMIT_REACHED", "合并后别名超过 20 项，请先整理"));
        }
        let old_pins = setting(self, "floatingTags")?;
        let old_collections = setting(self, "savedCollections")?;
        let mut pins: Vec<String> =
            serde_json::from_str(&old_pins).map_err(|_| err("INVALID_INPUT", "浮窗预设无效"))?;
        for p in &mut pins {
            if p == source {
                *p = target.into();
            }
        }
        let mut seen = HashSet::new();
        pins.retain(|p| seen.insert(p.clone()));
        let new_pins = json!(pins).to_string();
        let mut collections: Vec<Value> = serde_json::from_str(&old_collections)
            .map_err(|_| err("INVALID_INPUT", "筛选集合无效"))?;
        for c in &mut collections {
            for k in ["include", "exclude"] {
                let mut ids = string_list(&c["query"], k);
                for id in &mut ids {
                    if id == source {
                        *id = target.into();
                    }
                }
                ids.sort();
                ids.dedup();
                c["query"][k] = json!(ids);
            }
            let include = string_list(&c["query"], "include");
            let exclude = string_list(&c["query"], "exclude");
            if include.iter().any(|id| exclude.contains(id)) {
                return Err(err(
                    "COLLECTION_CONFLICT",
                    "合并会使筛选集合同时包含和排除同一标签，请先修改该集合",
                ));
            }
        }
        let new_collections = json!(collections).to_string();
        let tx = self.conn.transaction().map_err(sql_err)?;
        let mut undo = Undo {
            id: uuid::Uuid::new_v4().to_string(),
            label: "合并标签".into(),
            steps: vec![],
            guards: vec![],
        };
        let old: (String, i64, i64, String) = tx
            .query_row(
                "SELECT name,version,last_used,created_by FROM tags WHERE id=?",
                [source],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .map_err(sql_err)?;
        undo.steps.push(step(
            "INSERT INTO tags(id,name,name_key,version,last_used,created_by) VALUES(?,?,?,?,?,?)",
            vec![
                sv(source),
                sv(&old.0),
                sv(key(&old.0)),
                iv(old.1 + 1),
                iv(old.2),
                sv(&old.3),
            ],
        ));
        let files: Vec<(String, i64)> = serde_json::from_value(preview["files"].clone())
            .map_err(|_| err("INVALID_INPUT", "预览无效"))?;
        for (fid, version) in files {
            let src = crate::auto_tags::association_step(&tx, &fid, source)?
                .ok_or_else(|| err("VERSION_CONFLICT", "关联已变化"))?;
            let dst = crate::auto_tags::association_step(&tx, &fid, target)?;
            undo.steps.push(step(
                "DELETE FROM file_tags WHERE file_id=? AND tag_id=?",
                vec![sv(&fid), sv(target)],
            ));
            if let Some(dst) = dst {
                undo.steps.push(dst);
            } else {
                tx.execute("INSERT INTO file_tags(file_id,tag_id,source,ai_meta) VALUES(?,?,'manual','{}')",params![fid,target]).map_err(sql_err)?;
            }
            undo.steps.push(src);
            tx.execute("UPDATE files SET version=version+1 WHERE id=?", [&fid])
                .map_err(sql_err)?;
            undo.steps.push(step(
                "UPDATE files SET version=version+1 WHERE id=?",
                vec![sv(&fid)],
            ));
            undo.guards.push(Guard {
                table: "files".into(),
                id: fid,
                version: version + 1,
            });
        }
        let target_origin: String = tx
            .query_row("SELECT created_by FROM tags WHERE id=?", [target], |r| {
                r.get(0)
            })
            .map_err(sql_err)?;
        let target_version = preview["target"]["version"].as_i64().unwrap();
        tx.execute(
            "UPDATE tags SET version=version+1,created_by='manual' WHERE id=?",
            [target],
        )
        .map_err(sql_err)?;
        undo.steps.push(step(
            "UPDATE tags SET version=version+1,created_by=? WHERE id=?",
            vec![sv(target_origin), sv(target)],
        ));
        undo.guards.push(Guard {
            table: "tags".into(),
            id: target.into(),
            version: target_version + 1,
        });
        let source_restore = crate::tag_aliases::restore_steps(&tx, source)?;
        let target_restore = crate::tag_aliases::restore_steps(&tx, target)?;
        undo.steps.push(step(
            "DELETE FROM tag_aliases WHERE tag_id=?",
            vec![sv(target)],
        ));
        undo.steps.extend(source_restore);
        undo.steps.extend(target_restore);
        tx.execute(
            "DELETE FROM tag_aliases WHERE tag_id IN (?,?)",
            params![source, target],
        )
        .map_err(sql_err)?;
        for alias in &merged_aliases {
            tx.execute(
                "INSERT INTO tag_aliases(alias_key,name,tag_id) VALUES(?,?,?)",
                params![key(alias), alias, target],
            )
            .map_err(sql_err)?;
        }
        tx.execute("DELETE FROM tags WHERE id=?", [source])
            .map_err(sql_err)?;
        undo.guards.push(Guard {
            table: "tags".into(),
            id: source.into(),
            version: -1,
        });
        for (k, old, new) in [
            ("floatingTags", old_pins, new_pins),
            ("savedCollections", old_collections, new_collections),
        ] {
            if old != new {
                put(&tx, k, &new)?;
                // Never overwrite settings edited after the merge.
                undo.steps.push(step(
                    "UPDATE settings SET value=? WHERE key=? AND value=?",
                    vec![sv(old), sv(k), sv(new)],
                ));
            }
        }
        tx.commit().map_err(sql_err)?;
        self.remember(undo);
        Ok(json!({"affected":preview["affected"],"overlap":preview["overlap"]}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Store, Vec<String>, Tag, Tag) {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Store::open(&dir.path().join("library")).unwrap();
        s.save_settings(&json!({"key":"folderAutoTagging","value":false}))
            .unwrap();
        let a = s.create_tag("参考旧标签").unwrap();
        let b = s.create_tag("参考资料").unwrap();
        let mut ids = vec![];
        for n in 0..3 {
            let p = dir.path().join(format!("{n}.txt"));
            std::fs::write(&p, b"synthetic").unwrap();
            let m = crate::fsops::inspect(&p).unwrap();
            s.add_file(&m).unwrap();
            let fid: String = s
                .conn
                .query_row("SELECT id FROM files WHERE path=?", [m.path], |r| r.get(0))
                .unwrap();
            ids.push(fid);
        }
        for i in 0..3 {
            s.conn
                .execute(
                    "INSERT INTO file_tags(file_id,tag_id) VALUES(?,?)",
                    params![ids[i], if i < 2 { &a.id } else { &b.id }],
                )
                .unwrap();
        }
        s.conn
            .execute(
                "INSERT INTO file_tags(file_id,tag_id) VALUES(?,?)",
                params![ids[0], b.id],
            )
            .unwrap();
        s.save_floating_presets(&[a.id.clone(), b.id.clone()])
            .unwrap();
        (dir, s, ids, a, b)
    }
    fn query(t: &str) -> Query {
        Query {
            scope: "all".into(),
            mode: "all".into(),
            sort: "added".into(),
            direction: "desc".into(),
            include: vec![t.into()],
            limit: 100,
            ..Default::default()
        }
    }
    #[test]
    fn merge_deduplicates_preserves_and_undoes() {
        let (_dir, mut s, ids, a, b) = fixture();
        s.conn
            .execute(
                "UPDATE files SET note='保留备注',favorite=1 WHERE id=?",
                [&ids[0]],
            )
            .unwrap();
        s.collection_save(&json!({"name":"参考集合","query":query(&a.id)}))
            .unwrap();
        let v = json!({"source":a.id,"target":b.id});
        let p = s.merge_preview(&v).unwrap();
        assert_eq!(p["affected"], 2);
        assert_eq!(p["overlap"], 1);
        s.merge_tags(&json!({"source":a.id,"target":b.id,"preview":p}))
            .unwrap();
        assert_eq!(s.file(&ids[0]).unwrap().note, "保留备注");
        assert!(s.file(&ids[0]).unwrap().favorite);
        assert_eq!(
            s.conn
                .query_row(
                    "SELECT COUNT(*) FROM file_tags WHERE tag_id=?",
                    [&b.id],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            3
        );
        assert!(!s.tags().unwrap().iter().any(|t| t.id == a.id));
        assert_eq!(s.floating_presets().unwrap()["ids"], json!([b.id]));
        assert!(setting(&s, "savedCollections").unwrap().contains(&b.id));
        s.undo_last().unwrap();
        assert!(s.tags().unwrap().iter().any(|t| t.id == a.id));
        assert_eq!(s.floating_presets().unwrap()["ids"], json!([a.id, b.id]));
        assert!(setting(&s, "savedCollections").unwrap().contains(&a.id));
        assert_eq!(s.file(&ids[1]).unwrap().tags[0].id, a.id);
    }
    #[test]
    fn merge_conflicts_roll_back_and_guard_undo() {
        let (_dir, mut s, ids, a, b) = fixture();
        let v = json!({"source":a.id,"target":b.id});
        let p = s.merge_preview(&v).unwrap();
        s.conn
            .execute("UPDATE files SET version=version+1 WHERE id=?", [&ids[0]])
            .unwrap();
        assert!(s
            .merge_tags(&json!({"source":a.id,"target":b.id,"preview":p}))
            .is_err());
        assert!(s.tags().unwrap().iter().any(|t| t.id == a.id));
        let mut q = query(&a.id);
        q.exclude = vec![b.id.clone()];
        s.collection_save(&json!({"name":"冲突集合","query":q}))
            .unwrap();
        let p = s.merge_preview(&v).unwrap();
        assert!(s
            .merge_tags(&json!({"source":a.id,"target":b.id,"preview":p}))
            .unwrap_err()
            .starts_with("COLLECTION_CONFLICT"));
        put(&s.conn, "savedCollections", "[]").unwrap();
        let p = s.merge_preview(&v).unwrap();
        s.merge_tags(&json!({"source":a.id,"target":b.id,"preview":p}))
            .unwrap();
        s.conn
            .execute("UPDATE files SET version=version+1 WHERE id=?", [&ids[0]])
            .unwrap();
        assert!(s.undo_last().is_err());
        assert!(!s.tags().unwrap().iter().any(|t| t.id == a.id));
    }
    #[test]
    fn collections_dynamic_persistent_and_missing_tags_rejected() {
        let (dir, mut s, ids, a, _) = fixture();
        let mut q = query(&a.id);
        q.sort = "size".into();
        let c = s
            .collection_save(&json!({"name":"动态集合","query":q}))
            .unwrap();
        assert_eq!(s.query(&q).unwrap()["total"], 2);
        s.conn
            .execute(
                "INSERT INTO file_tags(file_id,tag_id) VALUES(?,?)",
                params![ids[2], a.id],
            )
            .unwrap();
        assert_eq!(s.query(&q).unwrap()["total"], 3);
        assert!(s
            .collection_save(&json!({"name":"动态集合","query":q}))
            .is_err());
        q.include = vec!["deleted".into()];
        assert!(s
            .collection_save(&json!({"name":"无效","query":q}))
            .is_err());
        drop(s);
        let s = Store::open(&dir.path().join("library")).unwrap();
        assert!(setting(&s, "savedCollections")
            .unwrap()
            .contains(c["id"].as_str().unwrap()));
        s.collection_delete(&json!({"id":c["id"]})).unwrap();
        assert_eq!(setting(&s, "savedCollections").unwrap(), "[]");
    }
    #[test]
    fn folder_and_unconfirmed_ai_never_merge() {
        let (_dir, s, _, a, b) = fixture();
        s.conn
            .execute("UPDATE tags SET created_by='folder' WHERE id=?", [&a.id])
            .unwrap();
        assert!(s
            .merge_preview(&json!({"source":a.id,"target":b.id}))
            .is_err());
        s.conn
            .execute("UPDATE tags SET created_by='ai' WHERE id=?", [&a.id])
            .unwrap();
        assert!(s
            .merge_preview(&json!({"source":a.id,"target":b.id}))
            .is_err());
    }
}
