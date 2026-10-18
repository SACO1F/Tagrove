use crate::{db::Store, fsops, model::*};
use rusqlite::params;
use serde_json::{json, Value};
use std::fs;

struct Fixture {
    dir: tempfile::TempDir,
    store: Store,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("library")).unwrap();
        Self { dir, store }
    }
    fn add(&mut self, name: &str) -> FileRecord {
        let path = self.dir.path().join(name);
        fs::write(&path, "测试正文").unwrap();
        self.store
            .add_file(&fsops::inspect(&path).unwrap())
            .unwrap();
        let id: String = self
            .store
            .conn
            .query_row(
                "SELECT id FROM files WHERE path=?",
                [fsops::canonical_display(&path).unwrap()],
                |r| r.get(0),
            )
            .unwrap();
        self.store.file(&id).unwrap()
    }
    fn suggestion(&mut self, fid: &str, name: &str) -> String {
        let t = self.store.create_tag_with_source(name, "ai").unwrap();
        self.store.conn.execute("INSERT INTO file_tags(file_id,tag_id,source,ai_meta) VALUES(?,?,'ai',?)",params![fid,t.id,json!({"model":"local-test","reason":"测试依据","updatedAt":123,"confirmed":false}).to_string()]).unwrap();
        t.id
    }
    fn payload(&self, action: &str, pairs: &[(&str, &str)]) -> Value {
        json!({"action":action,"items":pairs.iter().map(|(fid,tid)|{let file=self.store.file(fid).unwrap();let tag=file.tags.iter().find(|t|t.id==*tid).unwrap();json!({"fileId":fid,"tagId":tid,"version":file.version,"tagVersion":tag.version})}).collect::<Vec<_>>()})
    }
    fn commit(&mut self, mut p: Value) -> Value {
        p["token"] = self.store.ai_review_preview(&p).unwrap()["token"].clone();
        self.store.ai_review_commit(&p).unwrap()
    }
}

#[test]
fn review_accept_shared_and_multiple_tags_once_per_file_and_pin() {
    let mut f = Fixture::new();
    let a = f.add("a.md");
    let b = f.add("b.md");
    let t = f.suggestion(&a.id, "素材");
    f.suggestion(&b.id, "素材");
    let t2 = f.suggestion(&a.id, "灵感新");
    assert_eq!(
        f.store.ai_review_list(&json!({})).unwrap()["suggestions"],
        3
    );
    let result = f.commit(f.payload("accept", &[(&a.id, &t), (&b.id, &t), (&a.id, &t2)]));
    assert_eq!(result["applied"], 3);
    assert_eq!(f.store.file(&a.id).unwrap().version, a.version + 1);
    assert_eq!(f.store.file(&b.id).unwrap().version, b.version + 1);
    assert!(f
        .store
        .file(&a.id)
        .unwrap()
        .tags
        .iter()
        .filter(|t| t.source == "ai")
        .all(|t| t.ai.as_ref().unwrap().confirmed));
    let pins = string_list(&f.store.floating_presets().unwrap(), "ids");
    assert_eq!(pins.iter().filter(|id| **id == t).count(), 1);
    assert!(pins.contains(&t2));
    assert_eq!(f.store.ai_review_list(&json!({})).unwrap()["total"], 0);
    assert_eq!(f.store.ai_review_history().unwrap()[0]["count"], 3);
}

#[test]
fn review_reject_cleans_only_orphans_and_preserves_confirmed_folder_manual() {
    let mut f = Fixture::new();
    let a = f.add("a.md");
    let b = f.add("b.md");
    let t = f.suggestion(&a.id, "共享");
    f.suggestion(&b.id, "共享");
    let orphan = f.suggestion(&a.id, "孤立");
    let accepted = f.suggestion(&a.id, "已确认");
    f.store.conn.execute("UPDATE file_tags SET ai_meta=json_set(ai_meta,'$.confirmed',json('true')) WHERE tag_id=?",[&accepted]).unwrap();
    let manual = f.store.create_tag("手动").unwrap();
    f.store
        .conn
        .execute(
            "INSERT INTO file_tags(file_id,tag_id) VALUES(?,?)",
            params![a.id, manual.id],
        )
        .unwrap();
    let before = f.store.file(&a.id).unwrap().tags.len();
    assert_eq!(
        f.commit(f.payload("reject", &[(&a.id, &t), (&a.id, &orphan)]))["applied"],
        2
    );
    assert_eq!(f.store.file(&a.id).unwrap().tags.len(), before - 2);
    let tags = f.store.tag_pool().unwrap();
    assert!(tags.iter().any(|tag| tag.id == accepted));
    assert_eq!(
        f.store
            .conn
            .query_row("SELECT count(*) FROM tags WHERE id=?", [orphan], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        f.store
            .file(&b.id)
            .unwrap()
            .tags
            .iter()
            .filter(|tag| tag.id == t)
            .count(),
        1
    );
    let invalid = f.payload("accept", &[(&a.id, &accepted)]);
    assert_eq!(
        f.store.ai_review_preview(&invalid).unwrap()["conflicts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn review_conflict_is_atomic_and_repeat_commit_never_duplicates_history() {
    let mut f = Fixture::new();
    let a = f.add("a.md");
    let b = f.add("b.md");
    let t = f.suggestion(&a.id, "共享");
    f.suggestion(&b.id, "共享");
    let mut p = f.payload("accept", &[(&a.id, &t), (&b.id, &t)]);
    p["token"] = f.store.ai_review_preview(&p).unwrap()["token"].clone();
    f.store
        .conn
        .execute("UPDATE files SET version=version+1 WHERE id=?", [&b.id])
        .unwrap();
    let r = f.store.ai_review_commit(&p).unwrap();
    assert_eq!(r["applied"], 0);
    assert_eq!(r["conflicts"][0]["name"], "b.md");
    assert!(
        !f.store
            .file(&a.id)
            .unwrap()
            .tags
            .iter()
            .find(|tag| tag.id == t)
            .unwrap()
            .ai
            .as_ref()
            .unwrap()
            .confirmed
    );
    assert_eq!(f.store.ai_review_history().unwrap(), json!([]));
    let mut fresh = f.payload("accept", &[(&a.id, &t), (&b.id, &t)]);
    fresh["token"] = f.store.ai_review_preview(&fresh).unwrap()["token"].clone();
    assert_eq!(f.store.ai_review_commit(&fresh).unwrap()["applied"], 2);
    assert_eq!(f.store.ai_review_commit(&fresh).unwrap()["applied"], 0);
    assert_eq!(
        f.store
            .ai_review_history()
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn review_preview_detects_changed_provenance_and_disk_file() {
    let mut f = Fixture::new();
    let a = f.add("a.md");
    let t = f.suggestion(&a.id, "素材");
    let mut p = f.payload("accept", &[(&a.id, &t)]);
    p["token"] = f.store.ai_review_preview(&p).unwrap()["token"].clone();
    f.store
        .conn
        .execute(
            "UPDATE file_tags SET ai_meta=json_set(ai_meta,'$.reason','新依据') WHERE tag_id=?",
            [&t],
        )
        .unwrap();
    assert_eq!(f.store.ai_review_commit(&p).unwrap()["applied"], 0);
    fs::write(&a.path, "原文件已变化较长正文").unwrap();
    assert!(
        f.store.ai_review_preview(&p).unwrap()["conflicts"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("原文件")
    );
}

#[test]
fn review_history_failure_rolls_back_labels_versions_and_floating_pins() {
    let mut f = Fixture::new();
    let a = f.add("a.md");
    let t = f.suggestion(&a.id, "素材");
    let mut p = f.payload("reject", &[(&a.id, &t)]);
    p["token"] = f.store.ai_review_preview(&p).unwrap()["token"].clone();
    let pins = f.store.floating_presets().unwrap();
    f.store.conn.execute_batch("CREATE TEMP TRIGGER fail_audit BEFORE INSERT ON settings WHEN NEW.key='aiReviewHistory' BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    assert!(f.store.ai_review_commit(&p).is_err());
    assert_eq!(f.store.file(&a.id).unwrap().version, a.version);
    assert!(f
        .store
        .file(&a.id)
        .unwrap()
        .tags
        .iter()
        .any(|tag| tag.id == t));
    assert_eq!(f.store.floating_presets().unwrap(), pins);
    assert_eq!(f.store.ai_review_history().unwrap(), json!([]));
}

#[test]
fn review_history_survives_reopen_and_backup_old_backup_has_no_history() {
    let mut f = Fixture::new();
    let a = f.add("a.md");
    let t = f.suggestion(&a.id, "素材");
    let old = f.dir.path().join("old.sqtagbackup");
    f.store.export_backup(&old).unwrap();
    f.commit(f.payload("accept", &[(&a.id, &t)]));
    let history = f.store.ai_review_history().unwrap();
    let path = f.dir.path().join("review.sqtagbackup");
    f.store.export_backup(&path).unwrap();
    let mut restored = Store::open(&f.dir.path().join("restored")).unwrap();
    restored.restore_backup(&path).unwrap();
    assert_eq!(restored.ai_review_history().unwrap(), history);
    assert!(!restored.ai_config().unwrap().enabled);
    drop(restored);
    let mut reopened = Store::open(&f.dir.path().join("restored")).unwrap();
    assert_eq!(reopened.ai_review_history().unwrap(), history);
    reopened.restore_backup(&old).unwrap();
    assert_eq!(reopened.ai_review_history().unwrap(), json!([]));
    assert_eq!(
        reopened
            .conn
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        7
    );
}

#[test]
fn review_rejects_duplicates_non_ai_and_oversized_input() {
    let mut f = Fixture::new();
    let a = f.add("a.md");
    let t = f.suggestion(&a.id, "素材");
    let one = f.payload("accept", &[(&a.id, &t)]);
    let item = one["items"][0].clone();
    assert!(f
        .store
        .ai_review_preview(&json!({"action":"accept","items":[item,item]}))
        .is_err());
    assert!(f
        .store
        .ai_review_preview(&json!({"action":"accept","items":vec![item.clone();201]}))
        .is_err());
    assert!(f
        .store
        .ai_review_preview(&json!({"action":"drop","items":[item]}))
        .is_err());
    let folder = f
        .store
        .file(&a.id)
        .unwrap()
        .tags
        .into_iter()
        .find(|tag| tag.source == "folder")
        .unwrap();
    assert_eq!(
        f.store
            .ai_review_preview(&f.payload("reject", &[(&a.id, &folder.id)]))
            .unwrap()["conflicts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(f
        .store
        .save_settings(&json!({"key":"aiReviewHistory","value":[]}))
        .is_err());
}

#[test]
fn review_list_is_paged_literal_filtered_and_disabled_ai_still_reviewable() {
    let mut f = Fixture::new();
    for n in 0..43 {
        let a = f.add(&format!("样本{n}.md"));
        f.suggestion(&a.id, "100%素材");
    }
    let page = f.store.ai_review_list(&json!({"text":"%"})).unwrap();
    assert_eq!(page["total"], 43);
    assert_eq!(page["files"].as_array().unwrap().len(), 40);
    assert_eq!(page["hasMore"], true);
    assert_eq!(page["enabled"], false);
    assert_eq!(
        f.store.ai_review_list(&json!({"offset":40})).unwrap()["files"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        f.store.ai_review_list(&json!({"text":"_"})).unwrap()["total"],
        0
    );
    for n in 0..52 {
        let a = f.add(&format!("审核{n}.md"));
        let t = f.suggestion(&a.id, "历史");
        f.commit(f.payload("reject", &[(&a.id, &t)]));
    }
    assert_eq!(
        f.store
            .ai_review_history()
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        50
    );
}
