use crate::{ai, db::Store, fsops, model::*};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
};

struct Fixture {
    store: Store,
    dir: tempfile::TempDir,
    files: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let files = dir.path().join("原文件");
        fs::create_dir_all(&files).unwrap();
        let store = Store::open(&dir.path().join("library")).unwrap();
        Self { store, dir, files }
    }
    fn add(&mut self, name: &str, kind: &str, bytes: &[u8]) -> FileRecord {
        let path = self.files.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        // Test the import persistence boundary with inspected file metadata.
        // OS path canonicalization and the Tauri bridge require the desktop suite.
        let file = fs::File::open(&path).unwrap();
        let meta = file.metadata().unwrap();
        let m = fsops::FileMeta {
            path: path.to_string_lossy().into(),
            parent: path.parent().unwrap().to_string_lossy().into(),
            name: path.file_name().unwrap().to_string_lossy().into(),
            extension: path.extension().unwrap().to_string_lossy().into(),
            kind: kind.into(),
            bytes: meta.len() as i64,
            modified: 0,
            revision: fsops::revision(&meta),
            identity: fsops::identity(&file),
        };
        assert!(self.store.add_file(&m).unwrap());
        let fid: String = self
            .store
            .conn
            .query_row("SELECT id FROM files WHERE path_key=?", [&m.path], |r| {
                r.get(0)
            })
            .unwrap();
        self.store.file(&fid).unwrap()
    }
    fn start(&mut self, fid: &str) -> (FileRecord, i64) {
        self.store.queue_ai(&[fid.to_string()]).unwrap();
        let token = self.store.ai_task(fid).unwrap().unwrap().updated_at;
        self.store.task_status(fid, token, "running", "").unwrap();
        (self.store.file(fid).unwrap(), token)
    }
    fn apply(&mut self, fid: &str, names: &[&str]) {
        let (file, token) = self.start(fid);
        let choices: Vec<_> = names
            .iter()
            .map(|s| (None, s.to_string(), "内容依据".into()))
            .collect();
        self.store
            .apply_ai_checked(&file, token, "test-vision", &choices)
            .unwrap();
    }
    fn patch(&mut self, fid: &str, add: Vec<String>, remove: Vec<String>) {
        let file = self.store.file(fid).unwrap();
        self.store
            .mutate(
                "files.tags",
                &json!({"ids":[fid],"versions":{fid:file.version},"add":add,"remove":remove}),
            )
            .unwrap();
    }
}
fn response(tags: Value) -> Value {
    json!({"choices":[{"finish_reason":"stop","message":{"content":json!({"tags":tags}).to_string()}}]})
}

#[test]
fn folder_tags_remain_searchable_but_cannot_be_pinned_even_from_legacy_settings() {
    let mut f = Fixture::new();
    let file = f.add("文件夹索引.txt", "document", b"fixture");
    let folder = file
        .tags
        .iter()
        .find(|tag| tag.created_by == "folder")
        .unwrap();
    let manual = f.store.create_tag("普通分类").unwrap();
    assert!(f
        .store
        .tag_pool()
        .unwrap()
        .iter()
        .any(|tag| tag.id == folder.id));
    let presets = f.store.floating_presets().unwrap();
    assert!(!presets["tags"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tag| tag["id"] == folder.id));
    f.store.save_floating_presets(&[manual.id.clone()]).unwrap();
    assert!(f
        .store
        .save_floating_presets(&[manual.id.clone(), folder.id.clone()])
        .unwrap_err()
        .starts_with("FOLDER_TAG_NOT_ALLOWED"));
    let ids = f.store.floating_presets().unwrap()["ids"].clone();
    assert_eq!(ids[0], manual.id);
    assert!(!ids.as_array().unwrap().contains(&json!(folder.id)));
    f.store
        .conn
        .execute(
            "UPDATE settings SET value=? WHERE key='floatingTags'",
            [json!([folder.id, manual.id]).to_string()],
        )
        .unwrap();
    assert_eq!(
        f.store.floating_presets().unwrap()["ids"],
        ids
    );
    assert!(f
        .store
        .file(&file.id)
        .unwrap()
        .tags
        .iter()
        .any(|tag| tag.id == folder.id));
    // Typing the name in the create field must not convert a folder label to manual.
    assert_eq!(
        f.store.create_tag(&folder.name).unwrap().created_by,
        "folder"
    );
}

#[test]
fn default_floating_shortcuts_skip_folder_name_collisions() {
    let mut f = Fixture::new();
    let folder = f.store.create_tag_with_source("灵感", "folder").unwrap();
    let presets = f.store.floating_presets().unwrap();
    assert_eq!(presets["ids"].as_array().unwrap().len(), 3);
    assert!(!presets["tags"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tag| tag["id"] == folder.id));
}

#[test]
fn confirming_ai_association_with_folder_label_does_not_pin_it() {
    let mut f = Fixture::new();
    let file = f.add("确认目录建议.txt", "document", b"fixture");
    let folder = f
        .store
        .create_tag_with_source("其他目录", "folder")
        .unwrap();
    // Legacy pending associations remain confirmable; new AI output cannot create them.
    f.store.conn.execute("INSERT INTO file_tags(file_id,tag_id,source,ai_meta) VALUES(?,?,'ai',?)", rusqlite::params![file.id,folder.id,json!({"model":"legacy","reason":"old fixture","updatedAt":now(),"confirmed":false}).to_string()]).unwrap();
    let current = f.store.file(&file.id).unwrap();
    f.store
        .confirm_ai(&json!({"id":file.id,"tagId":folder.id,"version":current.version}))
        .unwrap();
    assert!(
        f.store
            .file(&file.id)
            .unwrap()
            .tags
            .iter()
            .find(|tag| tag.id == folder.id)
            .unwrap()
            .ai
            .as_ref()
            .unwrap()
            .confirmed
    );
    assert!(!f.store.floating_presets().unwrap()["ids"]
        .as_array()
        .unwrap()
        .contains(&json!(folder.id)));
}

#[test]
fn manual_tag_import_does_not_queue_ai_but_regular_import_still_does() {
    let mut f = Fixture::new();
    f.store.save_ai_settings(&json!({"config":{"enabled":true,"endpoint":"http://127.0.0.1:11434/v1","model":"local-vision","allowNewTags":true}})).unwrap();
    let tag = f.store.create_tag("手工分类").unwrap();
    let manual_path = f.files.join("标签导入.txt");
    fs::write(&manual_path, b"synthetic manual import").unwrap();
    f.store
        .annotate(&json!({"tagId":tag.id,"paths":[manual_path]}))
        .unwrap();
    let fid: String = f
        .store
        .conn
        .query_row(
            "SELECT id FROM files WHERE name='标签导入.txt'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(f.store.ai_task(&fid).unwrap().is_none());
    assert!(f
        .store
        .file(&fid)
        .unwrap()
        .tags
        .iter()
        .any(|t| t.id == tag.id && t.source == "manual"));
    // Re-importing a removed file through a label also skips automatic inference.
    let file = f.store.file(&fid).unwrap();
    f.store
        .mutate(
            "files.remove",
            &json!({"ids":[fid],"versions":{fid.clone():file.version}}),
        )
        .unwrap();
    f.store
        .annotate(&json!({"tagId":tag.id,"paths":[manual_path]}))
        .unwrap();
    assert!(f.store.ai_task(&fid).unwrap().is_none());
    let regular = f.add("普通导入.txt", "text", b"synthetic regular import");
    assert_eq!(
        f.store.ai_task(&regular.id).unwrap().unwrap().status,
        "queued"
    );
}

#[test]
fn suggestions_enter_pool_after_acceptance_and_orphans_are_removed_without_losing_shared_tags() {
    let mut f = Fixture::new();
    let a = f.add("a.txt", "text", b"a");
    let b = f.add("b.txt", "text", b"b");
    f.apply(&a.id, &["AI共享建议"]);
    f.apply(&b.id, &["AI共享建议"]);
    let current = f.store.file(&a.id).unwrap();
    let tag = current
        .tags
        .iter()
        .find(|t| t.name == "AI共享建议")
        .unwrap()
        .clone();
    assert!(!f.store.tag_pool().unwrap().iter().any(|t| t.id == tag.id));
    assert!(!f.store.bootstrap().unwrap()["tags"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["id"] == tag.id));
    assert!(f.store.save_floating_presets(&[tag.id.clone()]).is_err());
    f.store
        .confirm_ai(&json!({"id":a.id,"tagId":tag.id,"version":current.version}))
        .unwrap();
    assert!(f.store.tag_pool().unwrap().iter().any(|t| t.id == tag.id));
    f.patch(&a.id, vec![], vec![tag.id.clone()]);
    assert!(!f.store.tag_pool().unwrap().iter().any(|t| t.id == tag.id));
    assert!(f
        .store
        .file(&b.id)
        .unwrap()
        .tags
        .iter()
        .any(|t| t.id == tag.id));
    f.patch(&b.id, vec![], vec![tag.id.clone()]);
    assert!(!f.store.tags().unwrap().iter().any(|t| t.id == tag.id));
    f.store.undo_last().unwrap();
    assert!(f
        .store
        .file(&b.id)
        .unwrap()
        .tags
        .iter()
        .any(|t| t.id == tag.id && !t.ai.as_ref().unwrap().confirmed));
    assert!(!f.store.tag_pool().unwrap().iter().any(|t| t.id == tag.id));
}

#[test]
fn reanalysis_cleans_orphan_suggestions_and_manual_creation_explicitly_adopts_a_suggestion() {
    let mut f = Fixture::new();
    let a = f.add("a.txt", "text", b"a");
    f.apply(&a.id, &["过期建议"]);
    f.apply(&a.id, &["新建议"]);
    assert!(!f.store.tags().unwrap().iter().any(|t| t.name == "过期建议"));
    let adopted = f.store.create_tag("新建议").unwrap();
    assert_eq!(adopted.created_by, "manual");
    assert!(f
        .store
        .tag_pool()
        .unwrap()
        .iter()
        .any(|t| t.id == adopted.id));
    f.patch(&a.id, vec![], vec![adopted.id.clone()]);
    assert!(f
        .store
        .tag_pool()
        .unwrap()
        .iter()
        .any(|t| t.id == adopted.id));
}

#[test]
fn accepting_ai_removes_pool_marker_and_pins_without_duplicates_or_capacity_loss() {
    let mut f = Fixture::new();
    let file = f.add("accepted.txt", "text", b"synthetic design reference");
    f.apply(&file.id, &["几何图案"]);
    let current = f.store.file(&file.id).unwrap();
    let tag = current.tags.iter().find(|t| t.source == "ai").unwrap();
    let tag_id = tag.id.clone();
    let mut pins: Vec<String> = (0..24)
        .map(|n| f.store.create_tag(&format!("固定标签{n}")).unwrap().id)
        .collect();
    f.store.save_floating_presets(&pins).unwrap();
    assert!(
        !f.store
            .tags()
            .unwrap()
            .iter()
            .find(|t| t.id == tag_id)
            .unwrap()
            .accepted
    );
    assert!(f
        .store
        .confirm_ai(&json!({"id":file.id,"tagId":tag_id,"version":current.version-1}))
        .is_err());
    assert_eq!(f.store.floating_presets().unwrap()["ids"], json!(pins));
    f.store
        .confirm_ai(&json!({"id":file.id,"tagId":tag_id,"version":current.version}))
        .unwrap();
    pins.push(tag_id.clone());
    assert_eq!(f.store.floating_presets().unwrap()["ids"], json!(pins));
    assert!(
        f.store
            .tags()
            .unwrap()
            .iter()
            .find(|t| t.id == tag_id)
            .unwrap()
            .accepted
    );
    let confirmed = f.store.file(&file.id).unwrap();
    let accepted = confirmed.tags.iter().find(|t| t.id == tag_id).unwrap();
    assert_eq!(accepted.source, "ai");
    assert!(accepted.ai.as_ref().unwrap().confirmed);
    f.store
        .confirm_ai(&json!({"id":file.id,"tagId":tag_id,"version":confirmed.version}))
        .unwrap();
    assert_eq!(f.store.floating_presets().unwrap()["ids"], json!(pins));
    // Undo returns the association to review and hides the unapproved shortcut.
    f.store.undo_last().unwrap();
    assert!(
        !f.store
            .tags()
            .unwrap()
            .iter()
            .find(|t| t.id == tag_id)
            .unwrap()
            .accepted
    );
    pins.retain(|id| id != &tag_id);
    assert_eq!(f.store.floating_presets().unwrap()["ids"], json!(pins));
    let root = f.store.root.clone();
    drop(f.store);
    let mut reopened = Store::open(&root).unwrap();
    assert_eq!(reopened.floating_presets().unwrap()["ids"], json!(pins));
}

fn backup_fixture(f: &Fixture, legacy: bool) -> PathBuf {
    use sha2::{Digest, Sha256};
    let db = f.dir.path().join("backup-fixture.sqlite");
    let mut target = rusqlite::Connection::open(&db).unwrap();
    rusqlite::backup::Backup::new(&f.store.conn, &mut target)
        .unwrap()
        .run_to_completion(64, std::time::Duration::from_millis(1), None)
        .unwrap();
    if legacy {
        target.execute_batch("DROP TABLE tag_aliases; DELETE FROM schema_migrations WHERE version=3; DROP TABLE ai_jobs; ALTER TABLE tags DROP COLUMN created_by; ALTER TABLE file_tags DROP COLUMN source; ALTER TABLE file_tags DROP COLUMN ai_meta; DELETE FROM schema_migrations WHERE version=2; PRAGMA user_version=1;").unwrap();
    }
    let library_id: String = target
        .query_row("SELECT value FROM library_meta WHERE key='id'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let file_count: i64 = target
        .query_row(
            "SELECT COUNT(*) FROM files WHERE removed_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let tag_count: i64 = target
        .query_row("SELECT COUNT(*) FROM tags", [], |r| r.get(0))
        .unwrap();
    drop(target);
    let bytes = fs::read(db).unwrap();
    let manifest=format!("backup_format = 1\nschema_version = {}\napp_version = '0.2.2'\ncreated_at = 1\nlibrary_id = '{library_id}'\nfile_count = {file_count}\ntag_count = {tag_count}\n",if legacy {1}else{2});
    let path = f.dir.path().join("fixture.sqtagbackup");
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    for (name, data) in [
        ("manifest.toml", manifest.into_bytes()),
        (
            "checksums.sha256",
            format!("{:x}  metadata.sqlite", Sha256::digest(&bytes)).into_bytes(),
        ),
        ("metadata.sqlite", bytes),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&data).unwrap();
    }
    zip.finish().unwrap();
    path
}
#[test]
fn legacy_backup_restores_and_migrates_all_manual_annotations() {
    let mut f = Fixture::new();
    f.store
        .save_settings(&json!({"key":"folderAutoTagging","value":false}))
        .unwrap();
    let file = f.add("old.txt", "text", b"old");
    let tag = f.store.create_tag("历史标签").unwrap();
    f.patch(&file.id, vec![tag.id], vec![]);
    let backup = backup_fixture(&f, true);
    f.store.restore_backup(&backup).unwrap();
    f.store.validate().unwrap();
    let restored = f.store.file(&file.id).unwrap();
    assert_eq!(restored.tags[0].name, "历史标签");
    assert_eq!(restored.tags[0].source, "manual");
    assert!(!f.store.ai_config().unwrap().enabled);
}
#[test]
fn externally_written_backup_cannot_enable_ai_or_resume_uploads() {
    let mut f = Fixture::new();
    let file = f.add("a.txt", "text", b"a");
    f.store.save_ai_settings(&json!({"config":{"enabled":true,"endpoint":"https://example.com/v1","model":"vision","allowNewTags":true}})).unwrap();
    f.store.queue_ai(&[file.id.clone()]).unwrap();
    let backup = backup_fixture(&f, false);
    f.store.restore_backup(&backup).unwrap();
    assert!(!f.store.ai_config().unwrap().enabled);
    assert!(f.store.ai_config().unwrap().endpoint.is_empty());
    assert_eq!(
        f.store.ai_task(&file.id).unwrap().unwrap().status,
        "cancelled"
    );
}

fn simple_pdf(text: Option<&str>) -> Vec<u8> {
    let content = text
        .map(|t| format!("BT /F1 12 Tf 50 150 Td ({t}) Tj ET"))
        .unwrap_or_default();
    let objects=[
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream",content.len()),
    ];
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = vec![0];
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{obj}\nendobj\n", i + 1));
    }
    let xref = out.len();
    out.push_str("xref\n0 6\n0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        out.push_str(&format!("{offset:010} 00000 n \n"));
    }
    out.push_str(&format!(
        "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
    ));
    out.into_bytes()
}

#[test]
fn pdf_extracts_real_text_and_rejects_pages_without_text() {
    let mut f = Fixture::new();
    let file = f.add("brief.pdf", "pdf", &simple_pdf(Some("Brand design brief")));
    assert!(ai::content(&f.store.root, &file).unwrap()["text"]
        .as_str()
        .unwrap()
        .contains("Brand design brief"));
    let blank = f.add("blank.pdf", "pdf", &simple_pdf(None));
    assert!(ai::content(&f.store.root, &blank)
        .unwrap_err()
        .starts_with("AI_UNSUPPORTED"));
}
#[test]
fn spreadsheets_and_slides_extract_content_and_legacy_office_is_unsupported() {
    let mut f = Fixture::new();
    for (name, entry, xml, expected) in [
        (
            "budget.xlsx",
            "xl/sharedStrings.xml",
            "<sst><si><t>Marketing budget</t></si></sst>",
            "Marketing budget",
        ),
        (
            "pitch.pptx",
            "ppt/slides/slide1.xml",
            "<p:sld xmlns:p='p' xmlns:a='a'><a:t>Project launch</a:t></p:sld>",
            "Project launch",
        ),
    ] {
        let mut bytes = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut bytes);
            zip.start_file(entry, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(xml.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        let file = f.add(name, "office", &bytes.into_inner());
        assert!(ai::content(&f.store.root, &file).unwrap()["text"]
            .as_str()
            .unwrap()
            .contains(expected));
    }
    let old = f.add("old.doc", "office", b"old binary office");
    assert!(ai::content(&f.store.root, &old)
        .unwrap_err()
        .starts_with("AI_UNSUPPORTED"));
}
#[test]
fn interrupted_jobs_resume_after_reopening_the_library() {
    let mut f = Fixture::new();
    let file = f.add("a.txt", "text", b"a");
    f.start(&file.id);
    let root = f.store.root.clone();
    drop(f.store);
    let store = Store::open(&root).unwrap();
    assert_eq!(store.ai_task(&file.id).unwrap().unwrap().status, "queued");
}
#[cfg(windows)]
#[test]
fn credentials_are_encrypted_and_a_service_switch_does_not_reuse_them() {
    let mut f = Fixture::new();
    let mut config = json!({"enabled":false,"endpoint":"https://example.com/v1","model":"vision","allowNewTags":true});
    let saved = f
        .store
        .save_ai_settings(&json!({"config":config,"apiKey":"synthetic-test-key"}))
        .unwrap();
    assert_eq!(saved["hasKey"], true);
    assert!(!saved.to_string().contains("synthetic-test-key"));
    assert_eq!(ai::read_key(&f.store.root).unwrap(), "synthetic-test-key");
    let bytes = fs::read(f.store.root.join("ai-key.dpapi")).unwrap();
    assert!(!bytes.windows(18).any(|w| w == b"synthetic-test-key"));
    config["endpoint"] = json!("https://second.example.com/v1");
    f.store.save_ai_settings(&json!({"config":config})).unwrap();
    assert!(ai::read_key(&f.store.root).unwrap().is_empty());
}

#[test]
fn folder_defaults_to_direct_parent_and_reuses_existing_pool() {
    let mut f = Fixture::new();
    let tag = f.store.create_tag("海报").unwrap();
    let file = f.add("素材/海报/封面.txt", "text", b"poster");
    assert_eq!(file.tags.len(), 1);
    assert_eq!(file.tags[0].id, tag.id);
    assert_eq!(file.tags[0].source, "folder");
    assert_eq!(file.tags[0].created_by, "manual");
    assert!(file.ai_task.is_none());
    let next = f.add("素材/海报/第二份.txt", "text", b"poster");
    assert_eq!(next.tags[0].id, tag.id);
    assert_eq!(f.store.tags().unwrap().len(), 1);
}
#[test]
fn folder_can_be_disabled_and_backfilled_without_replacing_manual_tags() {
    let mut f = Fixture::new();
    f.store
        .save_settings(&json!({"key":"folderAutoTagging","value":false}))
        .unwrap();
    let file = f.add("项目/需求.txt", "text", b"brief");
    assert!(file.tags.is_empty());
    let manual = f.store.create_tag("重点").unwrap();
    f.patch(&file.id, vec![manual.id.clone()], vec![]);
    f.store
        .save_settings(&json!({"key":"folderAutoTagging","value":true}))
        .unwrap();
    assert_eq!(f.store.fill_folder_tags().unwrap()["changed"], 1);
    assert_eq!(f.store.fill_folder_tags().unwrap()["changed"], 0);
    assert_eq!(f.store.file(&file.id).unwrap().tags.len(), 2);
}
#[test]
fn long_folder_names_are_deterministic_and_distinct() {
    let mut f = Fixture::new();
    let prefix = "文".repeat(41);
    let a = f.add(&format!("{prefix}甲/a.txt"), "text", b"a");
    let b = f.add(&format!("{prefix}乙/b.txt"), "text", b"b");
    assert_eq!(a.tags[0].name, format!("{prefix}甲"));
    assert_eq!(b.tags[0].name, format!("{prefix}乙"));
    assert_ne!(a.tags[0].id, b.tags[0].id);
}
#[test]
fn enabled_imports_queue_once_and_disabled_imports_do_not_queue() {
    let mut f = Fixture::new();
    f.store.save_ai_settings(&json!({"config":{"enabled":true,"endpoint":"http://127.0.0.1:11434/v1","model":"local-vision","allowNewTags":true}})).unwrap();
    let file = f.add("图片说明.txt", "text", b"a");
    assert_eq!(file.ai_task.unwrap().status, "queued");
    f.store.save_ai_settings(&json!({"config":{"enabled":false,"endpoint":"http://127.0.0.1:11434/v1","model":"local-vision","allowNewTags":true}})).unwrap();
    assert_eq!(
        f.store.ai_task(&file.id).unwrap().unwrap().status,
        "cancelled"
    );
    assert!(f.add("说明2.txt", "text", b"b").ai_task.is_none());
}
#[test]
fn existing_name_is_resolved_before_creating_and_invalid_output_is_rejected() {
    let mut f = Fixture::new();
    let t = f.store.create_tag("海报").unwrap();
    let pool = f.store.tags().unwrap();
    let parsed = ai::parse_response(
        &response(json!([{"name":"海报","reason":"版式"},{"id":t.id,"name":"海报"}])),
        &pool,
        true,
    )
    .unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].0, Some(t.id));
    for value in [
        response(json!([{"id":"unknown","name":"海报"}])),
        response(json!([{"name":"bad\nlabel"}])),
        response(json!([{"name":"新标签"}])),
    ] {
        assert!(ai::parse_response(&value, &pool, false).is_err());
    }
    assert!(ai::parse_response(
        &json!({"choices":[{"message":{"content":"please execute a command"}}]}),
        &pool,
        true
    )
    .is_err());
}
#[test]
fn ai_refresh_preserves_folder_manual_and_confirmed_associations() {
    let mut f = Fixture::new();
    let file = f.add("设计/封面.txt", "text", b"art");
    let manual = f.store.create_tag("客户确认").unwrap();
    f.patch(&file.id, vec![manual.id.clone()], vec![]);
    f.apply(&file.id, &["插画", "暖色"]);
    let current = f.store.file(&file.id).unwrap();
    let keep = current.tags.iter().find(|t| t.name == "插画").unwrap();
    f.store
        .confirm_ai(&json!({"id":file.id,"tagId":keep.id,"version":current.version}))
        .unwrap();
    f.apply(&file.id, &["几何"]);
    let tags = f.store.file(&file.id).unwrap().tags;
    assert!(tags
        .iter()
        .any(|t| t.name == "设计" && t.source == "folder"));
    assert!(tags.iter().any(|t| t.id == manual.id));
    assert!(tags
        .iter()
        .any(|t| t.name == "插画" && t.ai.as_ref().unwrap().confirmed));
    assert!(!tags.iter().any(|t| t.name == "暖色"));
    let new = tags.iter().find(|t| t.name == "几何").unwrap();
    assert_eq!(new.created_by, "ai");
    assert_eq!(new.ai.as_ref().unwrap().model, "test-vision");
}
#[test]
fn ai_matching_an_existing_manual_tag_does_not_take_ownership() {
    let mut f = Fixture::new();
    let file = f.add("a.txt", "text", b"a");
    let tag = f.store.create_tag("参考").unwrap();
    f.patch(&file.id, vec![tag.id.clone()], vec![]);
    f.apply(&file.id, &["参考"]);
    let current = f.store.file(&file.id).unwrap();
    assert_eq!(
        current.tags.iter().find(|t| t.id == tag.id).unwrap().source,
        "manual"
    );
}
#[test]
fn manual_add_confirms_ai_and_undo_restores_provenance() {
    let mut f = Fixture::new();
    let file = f.add("a.txt", "text", b"a");
    f.apply(&file.id, &["插画"]);
    let before = f
        .store
        .file(&file.id)
        .unwrap()
        .tags
        .into_iter()
        .find(|t| t.name == "插画")
        .unwrap();
    f.patch(&file.id, vec![before.id.clone()], vec![]);
    assert!(
        f.store
            .file(&file.id)
            .unwrap()
            .tags
            .iter()
            .find(|t| t.id == before.id)
            .unwrap()
            .ai
            .as_ref()
            .unwrap()
            .confirmed
    );
    f.store.undo_last().unwrap();
    let after = f
        .store
        .file(&file.id)
        .unwrap()
        .tags
        .into_iter()
        .find(|t| t.id == before.id)
        .unwrap();
    assert_eq!(json!(before.ai), json!(after.ai));
}
#[test]
fn removing_and_deleting_tags_can_undo_full_ai_metadata() {
    let mut f = Fixture::new();
    let file = f.add("a.txt", "text", b"a");
    f.apply(&file.id, &["插画"]);
    let before = f
        .store
        .file(&file.id)
        .unwrap()
        .tags
        .into_iter()
        .find(|t| t.name == "插画")
        .unwrap();
    f.patch(&file.id, vec![], vec![before.id.clone()]);
    f.store.undo_last().unwrap();
    assert_eq!(
        json!(
            f.store
                .file(&file.id)
                .unwrap()
                .tags
                .iter()
                .find(|t| t.id == before.id)
                .unwrap()
                .ai
        ),
        json!(before.ai)
    );
    f.store
        .mutate(
            "tags.delete",
            &json!({"id":before.id,"version":before.version}),
        )
        .unwrap();
    f.store.undo_last().unwrap();
    let after = f
        .store
        .file(&file.id)
        .unwrap()
        .tags
        .into_iter()
        .find(|t| t.id == before.id)
        .unwrap();
    assert_eq!(after.created_by, "ai");
    assert_eq!(json!(after.ai), json!(before.ai));
}
#[test]
fn stale_file_or_job_result_never_overwrites_newer_annotations() {
    let mut f = Fixture::new();
    let file = f.add("a.txt", "text", b"a");
    let (snapshot, token) = f.start(&file.id);
    let manual = f.store.create_tag("保留").unwrap();
    f.patch(&file.id, vec![manual.id], vec![]);
    let choices = vec![(None, "不应创建".into(), "reason".into())];
    assert!(f
        .store
        .apply_ai_checked(&snapshot, token, "model", &choices)
        .is_err());
    let (snapshot, token) = f.start(&file.id);
    f.store.queue_ai(&[file.id.clone()]).unwrap();
    assert!(f
        .store
        .apply_ai_checked(&snapshot, token, "model", &choices)
        .is_err());
    assert!(!f.store.tags().unwrap().iter().any(|t| t.name == "不应创建"));
}
#[test]
fn invalid_ai_batch_is_atomic_and_retains_old_unconfirmed_results() {
    let mut f = Fixture::new();
    let file = f.add("a.txt", "text", b"a");
    f.apply(&file.id, &["原AI标签"]);
    let before = f.store.file(&file.id).unwrap();
    let (snapshot, token) = f.start(&file.id);
    assert!(f
        .store
        .apply_ai_checked(
            &snapshot,
            token,
            "model",
            &[
                (None, "有效".into(), "".into()),
                (None, "\n".into(), "".into())
            ]
        )
        .is_err());
    assert_eq!(
        json!(f.store.file(&file.id).unwrap().tags),
        json!(before.tags)
    );
}
#[test]
fn backup_preserves_sources_but_never_enables_remote_analysis() {
    let mut f = Fixture::new();
    let file = f.add("a.txt", "text", b"a");
    f.apply(&file.id, &["插画"]);
    let cfg = json!({"enabled":true,"endpoint":"http://127.0.0.1:11434/v1","model":"local-vision","allowNewTags":true});
    f.store.save_ai_settings(&json!({"config":cfg})).unwrap();
    f.store.queue_ai(&[file.id.clone()]).unwrap();
    let path = f.dir.path().join("library.sqtagbackup");
    f.store.export_backup(&path).unwrap();
    f.store.restore_backup(&path).unwrap();
    assert!(!f.store.ai_config().unwrap().enabled);
    assert_eq!(
        f.store.ai_task(&file.id).unwrap().unwrap().status,
        "cancelled"
    );
    assert!(f
        .store
        .file(&file.id)
        .unwrap()
        .tags
        .iter()
        .any(|t| t.source == "ai"));
}
#[test]
fn schema_one_migrates_with_recovery_copy_and_manual_origins() {
    let mut f = Fixture::new();
    f.store
        .save_settings(&json!({"key":"folderAutoTagging","value":false}))
        .unwrap();
    let file = f.add("a.txt", "text", b"a");
    let tag = f.store.create_tag("旧标签").unwrap();
    f.patch(&file.id, vec![tag.id.clone()], vec![]);
    f.store.conn.execute_batch("DROP TABLE tag_aliases; DELETE FROM schema_migrations WHERE version=3; DROP TABLE ai_jobs; ALTER TABLE tags DROP COLUMN created_by; ALTER TABLE file_tags DROP COLUMN source; ALTER TABLE file_tags DROP COLUMN ai_meta; DELETE FROM schema_migrations WHERE version=2; PRAGMA user_version=1;").unwrap();
    let root = f.store.root.clone();
    drop(f.store);
    let store = Store::open(&root).unwrap();
    assert_eq!(store.file(&file.id).unwrap().tags[0].source, "manual");
    assert!(root
        .join("backups/before-auto-tags-schema-v1.sqlite")
        .exists());
    store.validate().unwrap();
}
#[test]
fn config_rejects_secret_urls_and_remote_plaintext() {
    for endpoint in [
        "http://example.com/v1",
        "https://user:key@example.com/v1",
        "https://example.com/v1?key=secret",
        "file:///C:/secret",
    ] {
        let mut config = ai::Config {
            enabled: true,
            endpoint: endpoint.into(),
            model: "vision".into(),
            allow_new_tags: true,
        };
        assert!(config.validate().is_err());
    }
    let mut config = ai::Config {
        enabled: true,
        endpoint: "http://localhost:11434/v1/".into(),
        model: "vision".into(),
        allow_new_tags: true,
    };
    config.validate().unwrap();
    assert!(!config.endpoint.ends_with('/'));
}
#[test]
fn real_text_and_office_contents_are_extracted_and_limited() {
    let mut f = Fixture::new();
    let file = f.add("brief.txt", "text", "需求内容".repeat(5000).as_bytes());
    let part = ai::content(&f.store.root, &file).unwrap();
    assert!(part["text"].as_str().unwrap().contains("需求内容"));
    assert!(part["text"].as_str().unwrap().chars().count() < 12100);
    let mut bytes = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut bytes);
        zip.start_file(
            "word/document.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(
            b"<w:document xmlns:w='x'><w:p><w:t>Brand &amp; Design</w:t></w:p></w:document>",
        )
        .unwrap();
        zip.finish().unwrap();
    }
    let office = f.add("brief.docx", "office", &bytes.into_inner());
    let part = ai::content(&f.store.root, &office).unwrap();
    assert!(part["text"].as_str().unwrap().contains("Brand"));
    assert!(part["text"].as_str().unwrap().contains("Design"));
}
#[test]
fn image_payload_contains_pixels_and_requests_do_not_include_local_paths() {
    let mut f = Fixture::new();
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(100, 80)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let file = f.add("private/photo.png", "image", &bytes.into_inner());
    let content = ai::content(&f.store.root, &file).unwrap();
    assert!(content["image_url"]["url"]
        .as_str()
        .unwrap()
        .starts_with("data:image/png;base64,"));
    let config = ai::Config::default();
    let body = ai::request_body(&config, &file, &f.store.tags().unwrap(), content)
        .unwrap()
        .to_string();
    assert!(!body.contains(&file.path));
    assert!(!body.contains("apiKey"));
    assert!(body.contains("photo.png"));
}
#[test]
fn compatible_http_transport_and_response_parsing_use_a_real_local_server() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut all = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let n = stream.read(&mut buffer).unwrap();
            all.extend_from_slice(&buffer[..n]);
            if let Some(end) = all.windows(4).position(|w| w == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&all[..end]).to_lowercase();
                let len: usize = header
                    .lines()
                    .find_map(|s| {
                        s.strip_prefix("content-length:")
                            .map(|s| s.trim().parse().unwrap())
                    })
                    .unwrap();
                if all.len() >= end + 4 + len {
                    break;
                }
            }
        }
        let text = String::from_utf8(all).unwrap();
        assert!(text.starts_with("POST /v1/chat/completions"));
        assert!(text.contains("Bearer test-only-key"));
        let body = response(json!([{"name":"几何","reason":"圆形图案"}])).to_string();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
    });
    let config = ai::Config {
        enabled: true,
        endpoint,
        model: "test-model".into(),
        allow_new_tags: true,
    };
    let value = ai::send(
        &config,
        "test-only-key",
        &json!({"model":"test-model","messages":[]}),
    )
    .unwrap();
    assert_eq!(ai::parse_response(&value, &[], true).unwrap()[0].1, "几何");
    server.join().unwrap();
}

#[test]
fn scoped_ai_cancel_preserves_other_work_and_rejects_late_results() {
    let mut f = Fixture::new();
    let a = f.add("cancel-target.txt", "text", b"a");
    let b = f.add("other-queued.txt", "text", b"b");
    let c = f.add("already-complete.txt", "text", b"c");
    f.apply(&c.id, &["已生成"]);
    let before = f.store.file(&c.id).unwrap();
    let (snapshot, token) = f.start(&a.id);
    f.store.queue_ai(&[b.id.clone()]).unwrap();
    assert_eq!(
        f.store
            .cancel_ai_files(&[a.id.clone(), c.id.clone()])
            .unwrap()["cancelled"],
        1
    );
    assert_eq!(f.store.ai_task(&b.id).unwrap().unwrap().status, "queued");
    assert_eq!(f.store.ai_task(&c.id).unwrap().unwrap().status, "done");
    assert_eq!(f.store.file(&c.id).unwrap().version, before.version);
    let choices = vec![(None, "迟到结果".into(), "reason".into())];
    assert!(f
        .store
        .apply_ai_checked(&snapshot, token, "model", &choices)
        .is_err());
    assert_eq!(f.store.ai_task(&a.id).unwrap().unwrap().status, "cancelled");
    assert!(!f.store.tags().unwrap().iter().any(|t| t.name == "迟到结果"));
    f.store.queue_ai(&[a.id.clone()]).unwrap();
    assert!(f.store.ai_task(&a.id).unwrap().unwrap().updated_at > token);
    assert!(f
        .store
        .apply_ai_checked(&snapshot, token, "model", &choices)
        .is_err());
    assert_eq!(f.store.ai_task(&a.id).unwrap().unwrap().status, "queued");
}

#[test]
fn scoped_ai_cancel_validates_whole_batch_before_changing_jobs() {
    let mut f = Fixture::new();
    let file = f.add("queued.txt", "text", b"a");
    f.store.queue_ai(&[file.id.clone()]).unwrap();
    assert!(f.store.cancel_ai_files(&[]).is_err());
    assert!(f
        .store
        .cancel_ai_files(&[file.id.clone(), "missing-file-id".into()])
        .is_err());
    assert_eq!(f.store.ai_task(&file.id).unwrap().unwrap().status, "queued");
    assert_eq!(
        f.store
            .cancel_ai_files(&[file.id.clone(), file.id.clone()])
            .unwrap()["cancelled"],
        1
    );
    assert_eq!(f.store.cancel_ai_files(&[file.id]).unwrap()["cancelled"], 0);
}

#[test]
fn floating_all_tags_migrates_sparse_order_and_adds_later_tags_without_folders() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("library");
    let mut s = Store::open(&root).unwrap();
    let a = s.create_tag("existing A").unwrap();
    let b = s.create_tag("existing B").unwrap();
    let folder = s.create_tag_with_source("folder only", "folder").unwrap();
    let pending = s.create_tag_with_source("pending only", "ai").unwrap();
    s.save_floating_presets(&[b.id.clone()]).unwrap();
    let ids = s.floating_presets().unwrap()["ids"].clone();
    assert_eq!(ids, json!([b.id, a.id]));
    let c = s.create_tag("later C").unwrap();
    assert_eq!(s.floating_presets().unwrap()["ids"], json!([b.id, a.id, c.id]));
    assert!(!ids.as_array().unwrap().contains(&json!(folder.id)));
    assert!(!ids.as_array().unwrap().contains(&json!(pending.id)));
    drop(s);
    let mut reopened = Store::open(&root).unwrap();
    assert_eq!(reopened.floating_presets().unwrap()["ids"], json!([b.id, a.id, c.id]));
}

#[test]
fn workspace_manual_tags_pin_once_preserve_existing_shortcuts_and_survive_restart() {
    let mut f = Fixture::new();
    let file = f.add("workspace.txt", "text", b"synthetic");
    let keep = f.store.create_tag("保留快捷标签").unwrap();
    f.store.save_floating_presets(&[keep.id.clone()]).unwrap();
    let new = f.store.create_workspace_tag("工作台新建").unwrap();
    assert_eq!(
        f.store.floating_presets().unwrap()["ids"],
        json!([keep.id, new.id])
    );
    let reused = f.store.create_workspace_tag("工作台新建").unwrap();
    assert_eq!(reused.id, new.id);
    let existing = f.store.create_tag("既有普通标签").unwrap();
    f.patch(&file.id, vec![existing.id.clone()], vec![]);
    f.patch(&file.id, vec![existing.id.clone()], vec![]);
    let expected = json!([keep.id, new.id, existing.id]);
    assert_eq!(f.store.floating_presets().unwrap()["ids"], expected);
    let folder = f
        .store
        .file(&file.id)
        .unwrap()
        .tags
        .into_iter()
        .find(|t| t.created_by == "folder")
        .unwrap();
    f.patch(&file.id, vec![folder.id.clone()], vec![]);
    f.store.create_workspace_tag(&folder.name).unwrap();
    assert_eq!(f.store.floating_presets().unwrap()["ids"], expected);
    let root = f.store.root.clone();
    drop(f.store);
    let mut reopened = Store::open(&root).unwrap();
    assert_eq!(reopened.floating_presets().unwrap()["ids"], expected);
}

#[test]
fn workspace_failed_annotation_preserves_file_and_only_confirmed_ai_enters_palette() {
    let mut f = Fixture::new();
    let file = f.add("workspace-ai.txt", "text", b"synthetic");
    let keep = f.store.create_tag("原快捷标签").unwrap();
    f.store.save_floating_presets(&[keep.id.clone()]).unwrap();
    let rejected = f.store.create_tag("失败操作标签").unwrap();
    assert!(f.store.mutate("files.tags", &json!({"ids":[file.id],"versions":{file.id.clone():file.version-1},"add":[rejected.id],"remove":[]})).is_err());
    // All existing ordinary tags are visible even if assigning one failed.
    assert!(!f.store.file(&file.id).unwrap().tags.iter().any(|t| t.id == rejected.id));
    assert_eq!(f.store.floating_presets().unwrap()["ids"], json!([keep.id, rejected.id]));
    f.apply(&file.id, &["AI 待确认标签"]);
    let pending = f
        .store
        .file(&file.id)
        .unwrap()
        .tags
        .into_iter()
        .find(|t| t.source == "ai")
        .unwrap();
    assert!(!f.store.floating_presets().unwrap()["ids"]
        .as_array()
        .unwrap()
        .contains(&json!(pending.id)));
    f.patch(&file.id, vec![pending.id.clone()], vec![]);
    assert_eq!(
        f.store.floating_presets().unwrap()["ids"],
        json!([keep.id, rejected.id, pending.id])
    );
}


#[test]
fn folder_labels_are_excluded_from_ai_prompt_parser_and_write_boundary() {
    let mut f=Fixture::new();let file=f.add("folder-guard.txt","text",b"fixture");
    let folder=f.store.create_tag_with_source("私有目录","folder").unwrap();
    let normal=f.store.create_tag("学习资料").unwrap();let pool=f.store.tag_pool().unwrap();
    let body=ai::request_body(&ai::Config::default(),&file,&pool,json!({"type":"text","text":"fixture"})).unwrap();
    let sent:Value=serde_json::from_str(body["messages"][1]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(!sent["existingTags"].as_array().unwrap().iter().any(|t|t["id"]==folder.id));
    let response=json!({"choices":[{"message":{"content":json!({"tags":[{"id":folder.id,"name":folder.name},{"id":null,"name":folder.name},{"id":normal.id,"name":normal.name}]}).to_string()}}]});
    let parsed=ai::parse_response(&response,&pool,true).unwrap();assert_eq!(parsed.len(),1);assert_eq!(parsed[0].0.as_ref(),Some(&normal.id));
    let (current,token)=f.start(&file.id);
    f.store.apply_ai_checked(&current,token,"fixture",&[(Some(folder.id.clone()),folder.name.clone(),"folder id".into()),(None,folder.name.clone(),"folder name".into()),(Some(normal.id.clone()),normal.name.clone(),"content".into())]).unwrap();
    let after=f.store.file(&file.id).unwrap();assert!(!after.tags.iter().any(|t|t.id==folder.id));assert!(after.tags.iter().any(|t|t.id==normal.id&&t.source=="ai"));
}
