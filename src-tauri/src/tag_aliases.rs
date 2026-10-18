use crate::{
    db::{sv, validate_tag, Guard, Step, Store, Undo},
    model::*,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

pub fn migrate(conn: &Connection) -> Result<()> {
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(sql_err)?;
    if version == 2 {
        conn.execute_batch("BEGIN IMMEDIATE; CREATE TABLE tag_aliases(alias_key TEXT PRIMARY KEY,name TEXT NOT NULL,tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE); CREATE INDEX tag_alias_owner ON tag_aliases(tag_id); INSERT INTO schema_migrations VALUES(3,CAST(strftime('%s','now') AS INTEGER)*1000); PRAGMA user_version=3; COMMIT;").map_err(sql_err)?;
    }
    Ok(())
}
pub fn names(conn: &Connection, tid: &str) -> Result<Vec<String>> {
    let mut st = conn
        .prepare("SELECT name FROM tag_aliases WHERE tag_id=? ORDER BY alias_key")
        .map_err(sql_err)?;
    let rows = st.query_map([tid], |r| r.get(0)).map_err(sql_err)?;
    rows.collect::<std::result::Result<_, _>>().map_err(sql_err)
}
pub fn matches(tag: &Tag, name: &str) -> bool {
    key(&tag.name) == key(name) || tag.aliases.iter().any(|a| key(a) == key(name))
}
pub fn check_name(conn: &Connection, name: &str, owner: &str) -> Result<()> {
    let conflict: Option<String>=conn.query_row("SELECT name FROM tags WHERE id<>?1 AND name_key=?2 AND created_by<>'folder' UNION ALL SELECT t.name FROM tag_aliases a JOIN tags t ON t.id=a.tag_id WHERE a.alias_key=?2 AND a.tag_id<>?1 LIMIT 1",params![owner,key(name)],|r|r.get(0)).optional().map_err(sql_err)?;
    if let Some(target) = conflict {
        return Err(err(
            "TAG_NAME_CONFLICT",
            format!("「{name}」已属于标签「{target}」，请使用该标签或先合并"),
        ));
    }
    Ok(())
}
pub fn restore_steps(conn: &Connection, tid: &str) -> Result<Vec<Step>> {
    let mut out = vec![Step {
        sql: "DELETE FROM tag_aliases WHERE tag_id=?".into(),
        args: vec![sv(tid)],
    }];
    for name in names(conn, tid)? {
        out.push(Step {
            sql: "INSERT INTO tag_aliases(alias_key,name,tag_id) VALUES(?,?,?)".into(),
            args: vec![sv(key(&name)), sv(name), sv(tid)],
        });
    }
    Ok(out)
}
pub fn validate(conn: &Connection) -> Result<()> {
    let mut st=conn.prepare("SELECT a.name,a.alias_key,t.created_by,t.name FROM tag_aliases a JOIN tags t ON t.id=a.tag_id").map_err(sql_err)?;
    let rows = st
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(sql_err)?;
    for row in rows {
        let (name, k, source, canonical) = row.map_err(sql_err)?;
        if validate_tag(&name)? != name
            || key(&name) != k
            || source != "manual"
            || key(&canonical) == k
        {
            return Err(err("BACKUP_INVALID", "别名关系无效"));
        }
    }
    let bad:i64=conn.query_row("SELECT COUNT(*) FROM tag_aliases a JOIN tags t ON t.name_key=a.alias_key WHERE t.created_by<>'folder'",[],|r|r.get(0)).map_err(sql_err)?;
    if bad > 0 {
        return Err(err("BACKUP_INVALID", "规范名称与别名冲突"));
    }
    let too_many:i64=conn.query_row("SELECT COUNT(*) FROM (SELECT tag_id FROM tag_aliases GROUP BY tag_id HAVING COUNT(*)>20)",[],|r|r.get(0)).map_err(sql_err)?;
    if too_many > 0 {
        return Err(err("BACKUP_INVALID", "每个标签最多 20 个别名"));
    }
    Ok(())
}
impl Store {
    pub fn aliases_preview(&self, v: &Value) -> Result<Value> {
        let tid = str_arg(v, "id")?;
        let tag = self
            .tag_pool()?
            .into_iter()
            .find(|t| t.id == tid)
            .ok_or_else(|| err("TAG_NOT_FOUND", "标签已删除或尚未确认"))?;
        if tag.created_by == "folder" {
            return Err(err("FOLDER_TAG_NOT_ALLOWED", "文件夹标签不设置别名"));
        }
        if v["version"].as_i64() != Some(tag.version) {
            return Err(err("VERSION_CONFLICT", "标签已变化，请重新加载"));
        }
        let name = validate_tag(str_arg(v, "name")?)?;
        let input = v["aliases"]
            .as_array()
            .ok_or_else(|| err("INVALID_INPUT", "请提供别名列表"))?;
        if input.len() > 20 {
            return Err(err("LIMIT_REACHED", "每个标签最多 20 个别名"));
        }
        let mut aliases = vec![];
        let mut seen = HashSet::from([key(&name)]);
        let mut conflicts = vec![];
        if let Err(e) = check_name(&self.conn, &name, tid) {
            conflicts.push(e);
        }
        // Canonical names also remain unique across folder tags.
        let exists: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM tags WHERE name_key=? AND id<>?",
                params![key(&name), tid],
                |r| r.get(0),
            )
            .map_err(sql_err)?;
        if exists > 0 && conflicts.is_empty() {
            conflicts.push("已存在同名文件夹标签".into());
        }
        for item in input {
            let alias = validate_tag(
                item.as_str()
                    .ok_or_else(|| err("INVALID_TAG", "别名必须是文字"))?,
            )?;
            if !seen.insert(key(&alias)) {
                return Err(err("TAG_NAME_CONFLICT", "别名不能重复或等于规范名称"));
            }
            if let Err(e) = check_name(&self.conn, &alias, tid) {
                conflicts.push(e);
            }
            aliases.push(alias);
        }
        aliases.sort_by_key(|a| key(a));
        let collections: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key='savedCollections'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_err)?;
        let collections: Vec<Value> =
            serde_json::from_str(collections.as_deref().unwrap_or("[]")).unwrap_or_default();
        let references: Vec<Value> = collections
            .into_iter()
            .filter(|c| {
                string_list(&c["query"], "include").contains(&tid.to_string())
                    || string_list(&c["query"], "exclude").contains(&tid.to_string())
            })
            .map(|c| json!({"id":c["id"],"name":c["name"]}))
            .collect();
        Ok(
            json!({"tag":tag,"name":name,"aliases":aliases,"affected":tag.count,"collections":references,"conflicts":conflicts}),
        )
    }
    pub fn aliases_update(&mut self, v: &Value) -> Result<Value> {
        let preview = self.aliases_preview(v)?;
        if v["preview"] != preview {
            return Err(err("VERSION_CONFLICT", "标签或引用已变化，请重新预览"));
        }
        if !preview["conflicts"].as_array().unwrap().is_empty() {
            return Err(err("TAG_NAME_CONFLICT", "存在名称冲突，请先处理"));
        }
        let tag: Tag =
            serde_json::from_value(preview["tag"].clone()).map_err(|e| err("INVALID_INPUT", e))?;
        let name = preview["name"].as_str().unwrap();
        let aliases = string_list(&preview, "aliases");
        let tx = self.conn.transaction().map_err(sql_err)?;
        let mut steps = restore_steps(&tx, &tag.id)?;
        steps.push(Step {
            sql: "UPDATE tags SET name=?,name_key=?,created_by=?,version=version+1 WHERE id=?"
                .into(),
            args: vec![
                sv(&tag.name),
                sv(key(&tag.name)),
                sv(&tag.created_by),
                sv(&tag.id),
            ],
        });
        tx.execute("DELETE FROM tag_aliases WHERE tag_id=?", [&tag.id])
            .map_err(sql_err)?;
        tx.execute(
            "UPDATE tags SET name=?,name_key=?,created_by='manual',version=version+1 WHERE id=?",
            params![name, key(name), tag.id],
        )
        .map_err(sql_err)?;
        for a in &aliases {
            tx.execute(
                "INSERT INTO tag_aliases(alias_key,name,tag_id) VALUES(?,?,?)",
                params![key(a), a, tag.id],
            )
            .map_err(sql_err)?;
        }
        tx.commit().map_err(sql_err)?;
        let uid = id();
        self.remember(Undo {
            id: uid.clone(),
            label: "修改标签别名".into(),
            steps,
            guards: vec![Guard {
                table: "tags".into(),
                id: tag.id,
                version: tag.version + 1,
            }],
        });
        Ok(json!({"ok":true,"undoId":uid}))
    }
    // An entire package shares one canonical namespace. Reject ambiguity before
    // committing any associations; the surrounding savepoint rolls back all writes.
    pub fn import_aliases(&mut self, tid: &str, incoming: &[String]) -> Result<()> {
        let mut all = names(&self.conn, tid)?;
        let canonical: String = self
            .conn
            .query_row("SELECT name FROM tags WHERE id=?", [tid], |r| r.get(0))
            .map_err(sql_err)?;
        for a in incoming {
            let a = validate_tag(a)?;
            check_name(&self.conn, &a, tid)?;
            if key(&a) != key(&canonical) && !all.iter().any(|n| key(n) == key(&a)) {
                all.push(a);
            }
        }
        if all.len() > 20 {
            return Err(err("LIMIT_REACHED", "导入后别名超过 20 项"));
        }
        let before = names(&self.conn, tid)?;
        for a in &all {
            self.conn
                .execute(
                    "INSERT OR IGNORE INTO tag_aliases(alias_key,name,tag_id) VALUES(?,?,?)",
                    params![key(a), a, tid],
                )
                .map_err(sql_err)?;
        }
        if all.len() != before.len() {
            self.conn
                .execute("UPDATE tags SET version=version+1 WHERE id=?", [tid])
                .map_err(sql_err)?;
        }
        Ok(())
    }
    pub fn collections_reorder(&self, v: &Value) -> Result<Value> {
        let raw: String = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key='savedCollections'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_err)?
            .unwrap_or_else(|| "[]".into());
        let all: Vec<Value> =
            serde_json::from_str(&raw).map_err(|_| err("INVALID_INPUT", "集合数据无效"))?;
        if v["expected"] != json!(all) {
            return Err(err("VERSION_CONFLICT", "集合列表已变化，请重新加载"));
        }
        let ids = string_list(v, "ids");
        let mut index: HashMap<String, Value> = all
            .into_iter()
            .map(|c| (c["id"].as_str().unwrap_or("").to_string(), c))
            .collect();
        if ids.len() != index.len() {
            return Err(err("INVALID_INPUT", "排序必须包含全部集合"));
        }
        let mut next = vec![];
        for id in ids {
            next.push(
                index
                    .remove(&id)
                    .ok_or_else(|| err("INVALID_INPUT", "集合 ID 重复或已删除"))?,
            );
        }
        self.conn
            .execute(
                "UPDATE settings SET value=? WHERE key='savedCollections'",
                [json!(next).to_string()],
            )
            .map_err(sql_err)?;
        Ok(json_ok())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ai, fsops,
        model::Query,
        transfer::{self, Control},
    };
    fn setup() -> (tempfile::TempDir, Store, String, Tag) {
        let d = tempfile::tempdir().unwrap();
        let mut s = Store::open(&d.path().join("library")).unwrap();
        s.save_settings(&json!({"key":"folderAutoTagging","value":false}))
            .unwrap();
        let path = d.path().join("sample.txt");
        std::fs::write(&path, "synthetic unrelated text").unwrap();
        s.add_file_with_ai(&fsops::inspect(&path).unwrap(), false)
            .unwrap();
        let fid: String = s
            .conn
            .query_row("SELECT id FROM files", [], |r| r.get(0))
            .unwrap();
        let t = s.create_tag("参考资料").unwrap();
        s.conn
            .execute(
                "INSERT INTO file_tags(file_id,tag_id) VALUES(?,?)",
                params![fid, t.id],
            )
            .unwrap();
        (d, s, fid, t)
    }
    fn update(s: &mut Store, t: &Tag, name: &str, aliases: &[&str]) {
        let mut v = json!({"id":t.id,"version":t.version,"name":name,"aliases":aliases});
        v["preview"] = s.aliases_preview(&v).unwrap();
        s.aliases_update(&v).unwrap();
    }
    fn q(t: &str) -> Query {
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
    fn aliases_resolve_search_ai_and_manual_without_duplicate_associations() {
        let (_d, mut s, fid, t) = setup();
        update(
            &mut s,
            &t,
            &t.name,
            &["Learning resources", "学习资料", "Cafe\u{301}"],
        );
        assert_eq!(s.create_tag("LEARNING RESOURCES").unwrap().id, t.id);
        assert_eq!(s.create_tag("Café").unwrap().id, t.id);
        let query = Query {
            text: "学习资料".into(),
            limit: 100,
            ..Default::default()
        };
        assert_eq!(s.query(&query).unwrap()["total"], 1);
        assert_eq!(
            s.query(&query).unwrap()["files"][0]["tags"][0]["aliases"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        let pool = s.tag_pool().unwrap();
        let response = json!({"choices":[{"message":{"content":json!({"tags":[{"name":"学习资料"},{"id":t.id,"name":"Learning resources"},{"name":"参考资料"}]}).to_string()}}]});
        let choices = ai::parse_response(&response, &pool, false).unwrap();
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].0, Some(t.id.clone()));
        assert_eq!(choices[0].1, "参考资料");
        assert_eq!(s.file(&fid).unwrap().tags.len(), 1);
        validate(&s.conn).unwrap();
    }
    #[test]
    fn conflicts_limits_folder_and_unconfirmed_ai_are_rejected() {
        let (_d, mut s, _fid, t) = setup();
        let other = s.create_tag("照片").unwrap();
        update(&mut s, &other, &other.name, &["相片"]);
        for names in [vec!["照片"], vec!["相片"]] {
            let p = s
                .aliases_preview(
                    &json!({"id":t.id,"version":t.version,"name":t.name,"aliases":names}),
                )
                .unwrap();
            assert!(!p["conflicts"].as_array().unwrap().is_empty());
        }
        assert!(s
            .aliases_preview(
                &json!({"id":t.id,"version":t.version,"name":t.name,"aliases":["x","X"]})
            )
            .is_err());
        assert!(s
            .aliases_preview(
                &json!({"id":t.id,"version":t.version,"name":t.name,"aliases":vec!["x";21]})
            )
            .is_err());
        let folder = s.create_tag_with_source("文件夹", "folder").unwrap();
        assert!(s
            .aliases_preview(
                &json!({"id":folder.id,"version":folder.version,"name":folder.name,"aliases":[]})
            )
            .is_err());
        let ai = s.create_tag_with_source("未确认", "ai").unwrap();
        assert!(s
            .aliases_preview(&json!({"id":ai.id,"version":ai.version,"name":ai.name,"aliases":[]}))
            .is_err());
    }
    #[test]
    fn rename_alias_delete_and_undo_keep_references_and_prevent_name_reuse() {
        let (_d, mut s, fid, t) = setup();
        update(&mut s, &t, &t.name, &["资料"]);
        let t = s
            .tags()
            .unwrap()
            .into_iter()
            .find(|x| x.id == t.id)
            .unwrap();
        s.mutate(
            "tags.rename",
            &json!({"id":t.id,"version":t.version,"name":"资料"}),
        )
        .unwrap();
        assert!(names(&s.conn, &t.id).unwrap().is_empty());
        s.undo_last().unwrap();
        assert_eq!(s.file(&fid).unwrap().tags[0].name, "参考资料");
        assert_eq!(names(&s.conn, &t.id).unwrap(), vec!["资料"]);
        let t = s
            .tags()
            .unwrap()
            .into_iter()
            .find(|x| x.id == t.id)
            .unwrap();
        s.mutate("tags.delete", &json!({"id":t.id,"version":t.version}))
            .unwrap();
        assert!(names(&s.conn, &t.id).unwrap().is_empty());
        s.undo_last().unwrap();
        assert_eq!(s.file(&fid).unwrap().tags[0].aliases, vec!["资料"]);
        let t = s
            .tags()
            .unwrap()
            .into_iter()
            .find(|x| x.id == t.id)
            .unwrap();
        update(&mut s, &t, &t.name, &[]);
        s.create_tag("资料").unwrap();
        assert!(s.undo_last().unwrap_err().starts_with("UNDO_CONFLICT"));
        assert!(names(&s.conn, &t.id).unwrap().is_empty());
    }
    #[test]
    fn merge_moves_all_names_and_undo_restores_each_owner() {
        let (_d, mut s, fid, a) = setup();
        update(&mut s, &a, &a.name, &["资料"]);
        let b = s.create_tag("研究资料").unwrap();
        update(&mut s, &b, &b.name, &["Research"]);
        s.collection_save(&json!({"name":"资料集合","query":q(&a.id)}))
            .unwrap();
        let p = s
            .merge_preview(&json!({"source":a.id,"target":b.id}))
            .unwrap();
        s.merge_tags(&json!({"source":a.id,"target":b.id,"preview":p}))
            .unwrap();
        assert_eq!(s.file(&fid).unwrap().tags[0].id, b.id);
        assert_eq!(s.create_tag("参考资料").unwrap().id, b.id);
        assert_eq!(names(&s.conn, &b.id).unwrap().len(), 3);
        s.undo_last().unwrap();
        assert_eq!(names(&s.conn, &a.id).unwrap(), vec!["资料"]);
        assert_eq!(names(&s.conn, &b.id).unwrap(), vec!["Research"]);
        assert_eq!(s.file(&fid).unwrap().tags[0].id, a.id);
        validate(&s.conn).unwrap();
    }
    #[test]
    fn previews_guard_stale_versions_and_collection_references() {
        let (_d, mut s, _fid, t) = setup();
        let mut v = json!({"id":t.id,"version":t.version,"name":t.name,"aliases":["资料"]});
        v["preview"] = s.aliases_preview(&v).unwrap();
        s.collection_save(&json!({"name":"新集合","query":q(&t.id)}))
            .unwrap();
        assert!(s
            .aliases_update(&v)
            .unwrap_err()
            .starts_with("VERSION_CONFLICT"));
        v["preview"] = s.aliases_preview(&v).unwrap();
        assert_eq!(v["preview"]["collections"].as_array().unwrap().len(), 1);
        s.aliases_update(&v).unwrap();
        assert!(s.aliases_preview(&v).is_err());
    }
    #[test]
    fn migration_snapshot_and_backup_roundtrip_keep_aliases() {
        let (d, mut s, fid, t) = setup();
        let root = s.root.clone();
        s.conn.execute_batch("DROP TABLE tag_aliases;DELETE FROM schema_migrations WHERE version=3;PRAGMA user_version=2;").unwrap();
        drop(s);
        s = Store::open(&root).unwrap();
        let snapshot = root.join("backups/before-tag-aliases-schema-v2.sqlite");
        assert!(snapshot.is_file());
        let old = Connection::open(&snapshot).unwrap();
        assert_eq!(
            old.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
        update(&mut s, &t, &t.name, &["资料"]);
        let pack = d.path().join("backup.sqtagbackup");
        s.export_backup(&pack).unwrap();
        let mut restored = Store::open(&d.path().join("restored")).unwrap();
        restored.restore_backup(&pack).unwrap();
        assert_eq!(restored.file(&fid).unwrap().tags[0].aliases, vec!["资料"]);
        drop(restored);
        let restored = Store::open(&d.path().join("restored")).unwrap();
        assert_eq!(names(&restored.conn, &t.id).unwrap(), vec!["资料"]);
    }
    #[test]
    fn packages_transfer_aliases_and_preview_receiver_conflicts_before_commit() {
        let (d, mut s, fid, t) = setup();
        update(&mut s, &t, &t.name, &["资料"]);
        let plan = s.transfer_plan(&json!({"ids":[fid]})).unwrap();
        let hash = plan.info["fingerprint"].as_str().unwrap().to_owned();
        let pack = d.path().join("资料.sqtagpack");
        transfer::export(plan, &pack, &hash, &Control::default()).unwrap();
        let dest = d.path().join("receive");
        std::fs::create_dir(&dest).unwrap();
        let mut b = Store::open(&d.path().join("other")).unwrap();
        let p = transfer::preview_in_library(&pack, &Control::default(), &b).unwrap();
        assert_eq!(p["aliasCount"], 1);
        assert!(p["aliasConflicts"].as_array().unwrap().is_empty());
        let r = b
            .import_package(
                &pack,
                &dest,
                p["fingerprint"].as_str().unwrap(),
                &Control::default(),
            )
            .unwrap();
        let copy = b.file(r["ids"][0].as_str().unwrap()).unwrap();
        assert_eq!(copy.tags[0].aliases, vec!["资料"]);
        assert_eq!(b.create_tag("资料").unwrap().id, copy.tags[0].id);
        let mut c = Store::open(&d.path().join("conflicting")).unwrap();
        c.create_tag("资料").unwrap();
        let dest = d.path().join("conflict-receive");
        std::fs::create_dir(&dest).unwrap();
        let p = transfer::preview_in_library(&pack, &Control::default(), &c).unwrap();
        assert!(!p["aliasConflicts"].as_array().unwrap().is_empty());
        assert!(c
            .import_package(
                &pack,
                &dest,
                p["fingerprint"].as_str().unwrap(),
                &Control::default()
            )
            .unwrap_err()
            .starts_with("TAG_NAME_CONFLICT"));
        assert_eq!(c.query(&Query::default()).unwrap()["total"], 0);
    }
    #[test]
    fn saved_collections_rename_conditions_reorder_and_stale_updates() {
        let (_d, mut s, _fid, t) = setup();
        let a = s
            .collection_save(&json!({"name":"第一","query":q(&t.id)}))
            .unwrap();
        let b = s
            .collection_save(&json!({"name":"第二","query":q(&t.id)}))
            .unwrap();
        s.collections_reorder(&json!({"ids":[b["id"],a["id"]],"expected":[a,b]}))
            .unwrap();
        assert!(s
            .collections_reorder(&json!({"ids":[a["id"],b["id"]],"expected":[a,b]}))
            .is_err());
        let mut query = q(&t.id);
        query.text = "新条件".into();
        let edited = s
            .collection_save(&json!({"id":a["id"],"expected":a,"name":"重命名","query":query}))
            .unwrap();
        assert_eq!(edited["query"]["text"], "新条件");
        assert!(s
            .collection_save(&json!({"id":a["id"],"expected":a,"name":"过期","query":q(&t.id)}))
            .is_err());
    }
    #[test]
    fn receiving_aliases_coalesces_two_names_and_preserves_manual_provenance() {
        let (d, mut a, fid, t) = setup();
        a.conn
            .execute("DELETE FROM file_tags WHERE file_id=?", [&fid])
            .unwrap();
        let ai = a.create_tag_with_source("AA自动", "ai").unwrap();
        let manual = a.create_tag("ZZ手工").unwrap();
        a.conn.execute("INSERT INTO file_tags(file_id,tag_id,source,ai_meta) VALUES(?,?,'ai',?)",params![fid,ai.id,json!({"model":"synthetic","reason":"fixture","updatedAt":now(),"confirmed":false}).to_string()]).unwrap();
        a.conn
            .execute(
                "INSERT INTO file_tags(file_id,tag_id) VALUES(?,?)",
                params![fid, manual.id],
            )
            .unwrap();
        let plan = a.transfer_plan(&json!({"ids":[fid]})).unwrap();
        let hash = plan.info["fingerprint"].as_str().unwrap().to_owned();
        let pack = d.path().join("coalesce.sqtagpack");
        transfer::export(plan, &pack, &hash, &Control::default()).unwrap();
        let mut b = Store::open(&d.path().join("coalesce-receiver")).unwrap();
        let canonical = b.create_tag("统一标签").unwrap();
        update(&mut b, &canonical, &canonical.name, &["AA自动", "ZZ手工"]);
        let dest = d.path().join("coalesce-copies");
        std::fs::create_dir(&dest).unwrap();
        let p = transfer::preview_in_library(&pack, &Control::default(), &b).unwrap();
        assert!(p["aliasConflicts"].as_array().unwrap().is_empty());
        let r = b
            .import_package(
                &pack,
                &dest,
                p["fingerprint"].as_str().unwrap(),
                &Control::default(),
            )
            .unwrap();
        let copy = b.file(r["ids"][0].as_str().unwrap()).unwrap();
        assert_eq!(copy.tags.len(), 1);
        assert_eq!(copy.tags[0].id, canonical.id);
        assert_eq!(copy.tags[0].source, "manual");
        assert!(copy.tags[0].ai.is_none());
        assert!(a.tags().unwrap().iter().any(|x| x.id == t.id));
    }
    #[test]
    fn merge_preview_exposes_alias_limit_before_commit() {
        let (_d, mut s, _fid, t) = setup();
        let aliases: Vec<String> = (0..20).map(|i| format!("别名{i}")).collect();
        let refs: Vec<&str> = aliases.iter().map(String::as_str).collect();
        update(&mut s, &t, &t.name, &refs);
        let other = s.create_tag("另一个标签").unwrap();
        let p = s
            .merge_preview(&json!({"source":t.id,"target":other.id}))
            .unwrap();
        assert_eq!(p["aliasCount"], 21);
        assert_eq!(p["aliasConflicts"].as_array().unwrap().len(), 1);
        assert!(s
            .merge_tags(&json!({"source":t.id,"target":other.id,"preview":p}))
            .is_err());
        assert_eq!(names(&s.conn, &t.id).unwrap().len(), 20);
    }
}
