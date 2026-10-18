use crate::{db::Store, model::*};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TagGroups {
    pub version: i64,
    pub groups: Vec<Group>,
    pub members: HashMap<String, String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub id: String,
    pub name: String,
}
impl Store {
    pub fn tag_groups(&self) -> Result<TagGroups> {
        let raw: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key='tagGroups'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_err)?;
        let mut state: TagGroups = serde_json::from_str(
            raw.as_deref()
                .unwrap_or("{\"version\":0,\"groups\":[],\"members\":{}}"),
        )
        .map_err(|_| err("TAG_GROUPS_INVALID", "分类配置损坏，请从标注备份恢复"))?;
        let tags: HashSet<_> = self
            .tag_pool()?
            .into_iter()
            .filter(|t| t.created_by != "folder")
            .map(|t| t.id)
            .collect();
        let groups: HashSet<_> = state.groups.iter().map(|g| g.id.clone()).collect();
        state
            .members
            .retain(|id, g| tags.contains(id) && groups.contains(g));
        Ok(state)
    }
    pub fn tag_groups_update(&self, v: &Value) -> Result<Value> {
        let mut state = self.tag_groups()?;
        if v["version"].as_i64() != Some(state.version) {
            return Err(err(
                "TAG_GROUPS_STALE",
                "分类已在其他窗口更新，请刷新后重试",
            ));
        }
        match str_arg(v, "op")? {
            "create" | "rename" => {
                let name = str_arg(v, "name")?.trim();
                if name.is_empty()
                    || name.chars().count() > 40
                    || name.chars().any(char::is_control)
                    || key(name) == key("未分类")
                {
                    return Err(err(
                        "INVALID_INPUT",
                        "分类名称需为 1～40 字，且不能使用未分类",
                    ));
                }
                let id = v["id"].as_str().unwrap_or("");
                if state
                    .groups
                    .iter()
                    .any(|g| g.id != id && key(&g.name) == key(name))
                {
                    return Err(err("TAG_GROUPS_DUPLICATE", "分类名称已存在"));
                }
                if v["op"] == "create" {
                    if state.groups.len() >= 100 {
                        return Err(err("INVALID_INPUT", "最多创建 100 个分类"));
                    }
                    state.groups.push(Group {
                        id: uuid::Uuid::new_v4().to_string(),
                        name: name.into(),
                    });
                } else {
                    let g = state
                        .groups
                        .iter_mut()
                        .find(|g| g.id == id)
                        .ok_or_else(|| err("NOT_FOUND", "分类已不存在"))?;
                    g.name = name.into();
                }
            }
            "delete" => {
                let id = str_arg(v, "id")?;
                if !state.groups.iter().any(|g| g.id == id) {
                    return Err(err("NOT_FOUND", "分类已不存在"));
                }
                state.groups.retain(|g| g.id != id);
                state.members.retain(|_, g| g != id);
            }
            "assign" => {
                let id = str_arg(v, "tagId")?;
                let group = str_arg(v, "groupId")?;
                if !self
                    .tag_pool()?
                    .iter()
                    .any(|t| t.id == id && t.created_by != "folder")
                {
                    return Err(err("INVALID_INPUT", "仅可为已确认的普通标签分类"));
                }
                if !group.is_empty() && !state.groups.iter().any(|g| g.id == group) {
                    return Err(err("NOT_FOUND", "分类已不存在"));
                }
                if group.is_empty() {
                    state.members.remove(id);
                } else {
                    state.members.insert(id.into(), group.into());
                }
            }
            "move" => {
                let id = str_arg(v, "id")?;
                let from = state
                    .groups
                    .iter()
                    .position(|g| g.id == id)
                    .ok_or_else(|| err("NOT_FOUND", "分类已不存在"))?;
                let delta = v["delta"]
                    .as_i64()
                    .filter(|d| *d == 1 || *d == -1)
                    .ok_or_else(|| err("INVALID_INPUT", "分类移动方向无效"))?;
                let to = from as i64 + delta;
                if to < 0 || to >= state.groups.len() as i64 {
                    return Err(err("INVALID_INPUT", "分类已到边界"));
                }
                state.groups.swap(from, to as usize);
            }
            _ => return Err(err("INVALID_INPUT", "分类操作无效")),
        }
        state.version += 1;
        self.conn.execute("INSERT INTO settings(key,value) VALUES('tagGroups',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![json!(state).to_string()]).map_err(sql_err)?;
        Ok(json!(state))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn category_lifecycle_preserves_tags_and_detects_concurrent_changes() {
        let d = tempfile::tempdir().unwrap();
        let mut s = Store::open(d.path()).unwrap();
        let tag = s.create_tag("数学").unwrap();
        let created = s
            .tag_groups_update(&json!({"op":"create","name":"学习","version":0}))
            .unwrap();
        let id = created["groups"][0]["id"].as_str().unwrap();
        s.tag_groups_update(&json!({"op":"assign","tagId":tag.id,"groupId":id,"version":1}))
            .unwrap();
        assert!(s
            .tag_groups_update(&json!({"op":"rename","id":id,"name":"学校","version":1}))
            .unwrap_err()
            .starts_with("TAG_GROUPS_STALE"));
        s.tag_groups_update(&json!({"op":"rename","id":id,"name":"学校","version":2}))
            .unwrap();
        s.tag_groups_update(&json!({"op":"delete","id":id,"version":3}))
            .unwrap();
        assert!(s.tag_groups().unwrap().members.is_empty());
        assert!(s.tags().unwrap().iter().any(|t| t.id == tag.id));
        drop(s);
        let s = Store::open(d.path()).unwrap();
        assert_eq!(s.tag_groups().unwrap().version, 4);
    }
    #[test]
    fn folder_and_unconfirmed_ai_tags_cannot_be_grouped() {
        let d = tempfile::tempdir().unwrap();
        let mut s = Store::open(&d.path().join("library")).unwrap();
        let p = d.path().join("文档.txt");
        std::fs::write(&p, "正文").unwrap();
        s.add_file(&crate::fsops::inspect(&p).unwrap()).unwrap();
        let folder = s
            .tags()
            .unwrap()
            .into_iter()
            .find(|t| t.created_by == "folder")
            .unwrap();
        let ai = s.create_tag("未确认候选").unwrap();
        s.conn
            .execute("UPDATE tags SET created_by='ai' WHERE id=?", [&ai.id])
            .unwrap();
        for id in [folder.id, ai.id] {
            assert!(s
                .tag_groups_update(&json!({"op":"assign","tagId":id,"groupId":"","version":0}))
                .is_err());
        }
        assert_eq!(s.tag_groups().unwrap().version, 0);
    }
    #[test]
    fn group_order_and_orphan_cleanup_survive_restart() {
        let d = tempfile::tempdir().unwrap();
        let mut s = Store::open(d.path()).unwrap();
        let tag = s.create_tag("语文").unwrap();
        let first = s
            .tag_groups_update(&json!({"op":"create","name":"学习","version":0}))
            .unwrap();
        let id = first["groups"][0]["id"].as_str().unwrap();
        s.tag_groups_update(&json!({"op":"create","name":"工作","version":1}))
            .unwrap();
        s.tag_groups_update(&json!({"op":"assign","tagId":tag.id,"groupId":id,"version":2}))
            .unwrap();
        s.tag_groups_update(&json!({"op":"move","id":id,"delta":1,"version":3}))
            .unwrap();
        assert_eq!(s.tag_groups().unwrap().groups[1].name, "学习");
        s.conn
            .execute("DELETE FROM tags WHERE id=?", [tag.id])
            .unwrap();
        assert!(s.tag_groups().unwrap().members.is_empty());
        drop(s);
        let s = Store::open(d.path()).unwrap();
        assert_eq!(s.tag_groups().unwrap().groups[1].name, "学习");
        assert!(s.tag_groups().unwrap().members.is_empty());
    }
    #[test]
    fn duplicate_reserved_names_and_invalid_assignments_rejected() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(d.path()).unwrap();
        assert!(s
            .tag_groups_update(&json!({"op":"create","name":"未分类","version":0}))
            .is_err());
        s.tag_groups_update(&json!({"op":"create","name":"学习","version":0}))
            .unwrap();
        assert!(s
            .tag_groups_update(&json!({"op":"create","name":"学习","version":1}))
            .is_err());
        assert!(s
            .tag_groups_update(&json!({"op":"assign","tagId":"unknown","groupId":"","version":1}))
            .is_err());
        assert_eq!(s.tag_groups().unwrap().version, 1);
    }
}
