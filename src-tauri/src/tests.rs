use crate::{backup, db::Store, fsops, model::*};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::{fs, path::Path, time::Instant};

#[test]
#[ignore = "Requires an observed visible Windows Shell fixture at SHIQIAN_PROBE_X/Y"]
fn native_shell_target_probe() {
    let x = std::env::var("SHIQIAN_PROBE_X").unwrap().parse().unwrap();
    let y = std::env::var("SHIQIAN_PROBE_Y").unwrap().parse().unwrap();
    let expected = std::env::var("SHIQIAN_PROBE_PATH").unwrap();
    let result = crate::shell_target::file_at_point(x, y);
    if expected == "__reject__" {
        assert!(result.is_err());
        println!("Rejected non-file Shell target");
        return;
    }
    let actual = result.unwrap();
    assert_eq!(
        fsops::canonical_display(Path::new(&actual)).unwrap(),
        fsops::canonical_display(Path::new(&expected)).unwrap()
    );
    println!("Resolved exact native Shell file: {actual}");
}

#[test]
fn floating_presets_initialize_once_and_preserve_empty_choice() {
    let mut f = Fixture::new();
    let existing = f.store.create_tag("灵感").unwrap();
    let presets = f.store.floating_presets().unwrap();
    assert_eq!(presets["ids"].as_array().unwrap().len(), 4);
    assert!(presets["ids"]
        .as_array()
        .unwrap()
        .contains(&json!(existing.id)));
    assert_eq!(f.store.floating_presets().unwrap()["ids"], presets["ids"]);
    f.store.save_floating_presets(&[]).unwrap();
    assert_eq!(f.store.floating_presets().unwrap()["ids"], json!([]));
    assert!(f.store.save_floating_presets(&["missing".into()]).is_err());
    assert!(f
        .store
        .save_floating_presets(&[existing.id.clone(), existing.id])
        .is_err());
}

#[test]
fn floating_annotation_imports_and_is_idempotent_undo_preserves_other_tags() {
    let mut f = Fixture::new();
    let old = f.store.create_tag("已有标签").unwrap();
    let new = f.store.create_tag("浮窗新标签").unwrap();
    let file = f.add("中文 图片.png", b"fixture");
    f.tag(&[&file.id], &[&old.id]);
    let v = json!({"tagId":new.id,"paths":[file.path,file.path],"ids":[file.id]});
    assert_eq!(f.store.annotate(&v).unwrap()["applied"], 1);
    let version = f.store.file(&file.id).unwrap().version;
    assert_eq!(f.store.annotate(&v).unwrap()["applied"], 0);
    assert_eq!(f.store.file(&file.id).unwrap().version, version);
    f.store.undo_last().unwrap();
    let after = f.store.file(&file.id).unwrap();
    assert_eq!(after.tags.len(), 1);
    assert_eq!(after.tags[0].id, old.id);
    assert!(Path::new(&after.path).exists());
    let external = f.files.join("外部文件.jpg");
    fs::write(&external, b"new").unwrap();
    let result = f
        .store
        .annotate(&json!({"tagId":new.id,"paths":[external]}))
        .unwrap();
    assert_eq!(result["imported"], 1);
    assert_eq!(result["applied"], 1);
}

#[test]
fn floating_annotation_failure_rolls_back_imports_and_rejects_bad_targets() {
    let mut f = Fixture::new();
    let tag = f.store.create_tag("标注").unwrap();
    let path = f.files.join("待加入.txt");
    fs::write(&path, b"abc").unwrap();
    assert!(f
        .store
        .annotate(&json!({"tagId":tag.id,"paths":[path],"ids":["missing-record"]}))
        .is_err());
    assert_eq!(f.query(q())["total"], 0);
    assert!(f.store.undo.is_empty());
    for v in [
        json!({"tagId":"missing","paths":[path]}),
        json!({"tagId":tag.id,"paths":[f.files]}),
        json!({"tagId":tag.id}),
        json!({"tagId":tag.id,"paths":[f.store.root.join("metadata.sqlite")]}),
    ] {
        assert!(f.store.annotate(&v).is_err());
    }
    assert!(path.exists());
}

#[test]
fn floating_presets_follow_rename_delete_and_backup_restore() {
    let mut f = Fixture::new();
    let tag = f.store.create_tag("我的预设").unwrap();
    f.store.save_floating_presets(&[tag.id.clone()]).unwrap();
    let backup = f.files.join("presets.sqtagbackup");
    f.store.export_backup(&backup).unwrap();
    f.store
        .mutate(
            "tags.rename",
            &json!({"id":tag.id,"version":tag.version,"name":"重命名预设"}),
        )
        .unwrap();
    assert_eq!(
        f.store.floating_presets().unwrap()["tags"][0]["name"],
        "重命名预设"
    );
    f.store
        .mutate("tags.delete", &json!({"id":tag.id,"version":tag.version+1}))
        .unwrap();
    assert_eq!(f.store.floating_presets().unwrap()["ids"], json!([]));
    f.store.restore_backup(&backup).unwrap();
    assert_eq!(f.store.floating_presets().unwrap()["ids"], json!([tag.id]));
}

struct Fixture {
    store: Store,
    _dir: tempfile::TempDir,
    files: std::path::PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let files = dir.path().join("原文件");
        fs::create_dir(&files).unwrap();
        let mut store = Store::open(&dir.path().join("library")).unwrap();
        // Legacy workflow tests exercise explicitly assigned tags. Automatic
        // folder defaults and queue behavior have their own integration tests.
        store
            .save_settings(&json!({"key":"folderAutoTagging","value":false}))
            .unwrap();
        Self {
            _dir: dir,
            store,
            files,
        }
    }
    fn add(&mut self, name: &str, content: &[u8]) -> FileRecord {
        let path = self.files.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
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
    fn patch(&mut self, action: &str, ids: &[&str], extra: Value) -> Result<Value> {
        let mut v = extra;
        v["ids"] = json!(ids);
        v["versions"] = json!({});
        for id in ids {
            v["versions"][id] = json!(self.store.file(id)?.version);
        }
        self.store.mutate(action, &v)
    }
    fn tag(&mut self, ids: &[&str], tags: &[&str]) {
        self.patch("files.tags", ids, json!({"add":tags})).unwrap();
    }
    fn query(&self, q: Query) -> Value {
        self.store.query(&q).unwrap()
    }
}
fn q() -> Query {
    Query {
        limit: 100,
        ..Default::default()
    }
}

#[test]
fn cache_identity_changes_even_with_same_size_and_time() {
    let mut f = Fixture::new();
    let a = f.add("a.png", b"synthetic");
    let mut replacement = a.clone();
    replacement.identity = "different-filesystem-object".into();
    assert_ne!(
        fsops::cache_path(&f.store.root, &a, false),
        fsops::cache_path(&f.store.root, &replacement, false)
    );
}

#[test]
fn long_utf8_preview_does_not_split_a_character() {
    let mut f = Fixture::new();
    let content = "中".repeat(70_000);
    let a = f.add("long.txt", content.as_bytes());
    let result = fsops::preview(&f.store.root, &a, true).unwrap();
    assert_eq!(result["truncated"], true);
    assert_eq!(
        result["text"].as_str().unwrap(),
        "中".repeat((200 * 1024) / 3)
    );
}

#[test]
fn unicode_tag_identity_and_validation() {
    let mut f = Fixture::new();
    let a = f.store.create_tag("  Straße  ").unwrap();
    assert_eq!(a.id, f.store.create_tag("STRASSE").unwrap().id);
    let b = f.store.create_tag("Café").unwrap();
    assert_eq!(b.id, f.store.create_tag("Cafe\u{301}").unwrap().id);
    for invalid in ["", "  ", "a\nb"] {
        assert!(f.store.create_tag(invalid).is_err());
    }
    assert!(f.store.create_tag(&"标".repeat(41)).is_err());
    assert!(f.store.create_tag(&"👨‍👩‍👧".repeat(40)).is_ok());
}
#[test]
fn duplicate_import_is_idempotent() {
    let mut f = Fixture::new();
    let a = f.add("中文 空格.txt", b"hello");
    let m = fsops::inspect(Path::new(&a.path)).unwrap();
    assert!(!f.store.add_file(&m).unwrap());
    assert_eq!(f.query(q())["total"], 1);
    assert_eq!(f.store.file(&a.id).unwrap().version, 1);
}
#[test]
fn copies_at_different_paths_are_separate() {
    let mut f = Fixture::new();
    f.add("a.txt", b"same");
    f.add("b.txt", b"same");
    assert_eq!(f.query(q())["total"], 2);
}
#[test]
fn remove_readd_preserves_identity_and_annotation() {
    let mut f = Fixture::new();
    let a = f.add("a.txt", b"source");
    let t = f.store.create_tag("用途").unwrap();
    f.tag(&[&a.id], &[&t.id]);
    f.store
        .save_note(&json!({"id":a.id,"text":"note","version":0}))
        .unwrap();
    f.patch("files.remove", &[&a.id], json!({})).unwrap();
    assert!(Path::new(&a.path).exists());
    assert_eq!(f.query(q())["total"], 0);
    assert!(f
        .store
        .add_file(&fsops::inspect(Path::new(&a.path)).unwrap())
        .unwrap());
    let r = f.store.file(&a.id).unwrap();
    assert_eq!(r.note, "note");
    assert_eq!(r.tags[0].id, t.id);
    assert!(f
        .store
        .undo_last()
        .unwrap_err()
        .starts_with("UNDO_CONFLICT"));
}
#[test]
fn batch_tag_undo_only_reverts_actual_changes() {
    let mut f = Fixture::new();
    let a = f.add("a.txt", b"a");
    let b = f.add("b.txt", b"b");
    let t = f.store.create_tag("项目").unwrap();
    f.tag(&[&a.id], &[&t.id]);
    f.tag(&[&a.id, &b.id], &[&t.id]);
    f.store.undo_last().unwrap();
    assert_eq!(f.store.file(&a.id).unwrap().tags.len(), 1);
    assert!(f.store.file(&b.id).unwrap().tags.is_empty());
    f.store.undo_last().unwrap();
    assert!(f.store.file(&a.id).unwrap().tags.is_empty());
}
#[test]
fn stale_batch_rolls_back_all_files() {
    let mut f = Fixture::new();
    let a = f.add("a.txt", b"a");
    let b = f.add("b.txt", b"b");
    let t = f.store.create_tag("标注").unwrap();
    let v = json!({"ids":[a.id,b.id],"versions":{a.id.clone():1,b.id.clone():88},"add":[t.id]});
    assert!(f.store.mutate("files.tags", &v).is_err());
    assert!(f.store.file(&a.id).unwrap().tags.is_empty());
    assert!(f.store.undo.is_empty());
}
#[test]
fn note_versions_are_independent_and_conflicts_preserve_saved_text() {
    let mut f = Fixture::new();
    let a = f.add("a.txt", b"a");
    f.patch("files.favorite", &[&a.id], json!({"value":true}))
        .unwrap();
    f.store
        .save_note(&json!({"id":a.id,"text":"已保存","version":0}))
        .unwrap();
    assert!(f
        .store
        .save_note(&json!({"id":a.id,"text":"旧版覆盖","version":0}))
        .is_err());
    assert_eq!(f.store.file(&a.id).unwrap().note, "已保存");
    f.store.undo_last().unwrap();
    assert_eq!(f.store.file(&a.id).unwrap().note, "已保存");
}
#[test]
fn note_limit_uses_visible_characters() {
    let mut f = Fixture::new();
    let a = f.add("a.txt", b"a");
    assert!(f
        .store
        .save_note(&json!({"id":a.id,"text":"字".repeat(10001),"version":0}))
        .is_err());
    assert!(f
        .store
        .save_note(&json!({"id":a.id,"text":"👨‍👩‍👧".repeat(10000),"version":0}))
        .is_ok());
}
#[test]
fn text_search_is_literal_and_terms_match_across_fields() {
    let mut f = Fixture::new();
    let a = f.add("100%_产品.txt", b"a");
    f.add("100xy产品.txt", b"b");
    let t = f.store.create_tag("设计").unwrap();
    f.tag(&[&a.id], &[&t.id]);
    f.store
        .save_note(&json!({"id":a.id,"text":"客户 Café","version":0}))
        .unwrap();
    for term in ["100%_", "产品 设计 客户 cafe\u{301}"] {
        let mut query = q();
        query.text = term.into();
        assert_eq!(f.query(query)["total"], 1);
    }
    let mut query = q();
    query.text = "' OR 1=1 --".into();
    assert_eq!(f.query(query)["total"], 0);
}
#[test]
fn all_any_exclude_and_inbox_filters() {
    let mut f = Fixture::new();
    let a = f.add("a.txt", b"a");
    let b = f.add("b.txt", b"b");
    f.add("c.txt", b"c");
    let x = f.store.create_tag("X").unwrap();
    let y = f.store.create_tag("Y").unwrap();
    f.tag(&[&a.id, &b.id], &[&x.id]);
    f.tag(&[&b.id], &[&y.id]);
    let mut query = q();
    query.include = vec![x.id.clone(), y.id.clone()];
    assert_eq!(f.query(query.clone())["total"], 1);
    query.mode = "any".into();
    assert_eq!(f.query(query.clone())["total"], 2);
    query.include = vec![x.id];
    query.exclude = vec![y.id];
    assert_eq!(f.query(query.clone())["total"], 1);
    query.exclude = query.include.clone();
    assert!(f.store.query(&query).is_err());
    let mut query = q();
    query.scope = "inbox".into();
    assert_eq!(f.query(query)["total"], 1);
}
#[test]
fn directory_filter_respects_boundaries() {
    let mut f = Fixture::new();
    f.add("a/x.txt", b"a");
    f.add("a/sub/y.txt", b"b");
    f.add("ab/z.txt", b"c");
    let mut query = q();
    query.directory = f.files.join("a").to_string_lossy().into();
    query.recursive = true;
    assert_eq!(f.query(query.clone())["total"], 2);
    query.recursive = false;
    assert_eq!(f.query(query)["total"], 1);
}
#[test]
fn pagination_has_no_duplicates_and_stable_ties() {
    let mut f = Fixture::new();
    for i in 0..12 {
        f.add(&format!("{i}.txt"), b"a");
    }
    let mut query = q();
    query.limit = 5;
    query.sort = "size".into();
    let first = f.query(query.clone());
    query.offset = 5;
    let second = f.query(query.clone());
    query.offset = 10;
    let third = f.query(query);
    let ids: std::collections::HashSet<_> = [&first, &second, &third]
        .into_iter()
        .flat_map(|v| {
            v["files"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f["id"].as_str().unwrap())
        })
        .collect();
    assert_eq!(ids.len(), 12);
    assert_eq!(third["hasMore"], false);
}
#[test]
fn rename_delete_and_chained_undo_restore_tag_links() {
    let mut f = Fixture::new();
    let a = f.add("a.txt", b"a");
    let t = f.store.create_tag("旧名称").unwrap();
    f.tag(&[&a.id], &[&t.id]);
    f.store
        .mutate(
            "tags.rename",
            &json!({"id":t.id,"name":"新名称","version":1}),
        )
        .unwrap();
    f.store
        .mutate("tags.delete", &json!({"id":t.id,"version":2}))
        .unwrap();
    assert!(f.store.file(&a.id).unwrap().tags.is_empty());
    f.store.undo_last().unwrap();
    assert_eq!(f.store.file(&a.id).unwrap().tags[0].name, "新名称");
    f.store.undo_last().unwrap();
    assert_eq!(f.store.file(&a.id).unwrap().tags[0].name, "旧名称");
    f.store.undo_last().unwrap();
    assert!(f.store.file(&a.id).unwrap().tags.is_empty());
}
#[test]
fn undo_does_not_hide_unrelated_version_conflicts() {
    let mut f = Fixture::new();
    let a = f.add("a.txt", b"a");
    let b = f.add("b.txt", b"b");
    f.patch("files.favorite", &[&a.id], json!({"value":true}))
        .unwrap();
    f.patch("files.favorite", &[&b.id], json!({"value":true}))
        .unwrap();
    f.store
        .conn
        .execute("UPDATE files SET version=version+1 WHERE id=?", [&a.id])
        .unwrap();
    f.store.undo_last().unwrap();
    assert!(f
        .store
        .undo_last()
        .unwrap_err()
        .starts_with("UNDO_CONFLICT"));
}
#[test]
fn undo_depth_is_bounded() {
    let mut f = Fixture::new();
    let a = f.add("a.txt", b"a");
    for i in 0..25 {
        f.patch("files.favorite", &[&a.id], json!({"value":i%2==0}))
            .unwrap();
    }
    assert_eq!(f.store.undo.len(), 20);
    for _ in 0..20 {
        f.store.undo_last().unwrap();
    }
    assert!(f.store.undo_last().is_err());
}
#[test]
fn missing_and_relink_preserve_record_id() {
    let mut f = Fixture::new();
    let a = f.add("a.txt", b"a");
    let renamed = f.files.join("moved.txt");
    fs::rename(&a.path, &renamed).unwrap();
    assert_eq!(f.store.refresh_file(&a.id).unwrap().status, "missing");
    f.store
        .relink(&json!({"id":a.id,"path":renamed,"version":1}))
        .unwrap();
    assert_eq!(f.store.file(&a.id).unwrap().name, "moved.txt");
    f.store.undo_last().unwrap();
    assert_eq!(f.store.file(&a.id).unwrap().path, a.path);
    assert_eq!(fs::read(renamed).unwrap(), b"a");
}
#[test]
fn replacement_at_same_path_requires_confirmation() {
    let mut f = Fixture::new();
    let a = f.add("a.txt", b"a");
    fs::rename(&a.path, f.files.join("original.txt")).unwrap();
    fs::write(&a.path, b"replacement").unwrap();
    assert_eq!(f.store.refresh_file(&a.id).unwrap().status, "inaccessible");
    assert!(f
        .store
        .add_file(&fsops::inspect(Path::new(&a.path)).unwrap())
        .is_err());
    assert_eq!(f.store.file(&a.id).unwrap().identity, a.identity);
}
#[test]
fn relink_cannot_overwrite_another_record() {
    let mut f = Fixture::new();
    let a = f.add("a.txt", b"a");
    let b = f.add("b.txt", b"b");
    assert!(f
        .store
        .relink(&json!({"id":a.id,"path":b.path,"version":1}))
        .is_err());
    assert_eq!(f.store.file(&a.id).unwrap().path, a.path);
}
#[test]
fn app_data_is_excluded() {
    let mut f = Fixture::new();
    let m = fsops::inspect(&f.store.root.join("library.sqlite")).unwrap();
    assert!(f.store.add_file(&m).is_err());
}
#[test]
fn image_preview_resizes_caches_and_tracks_revision() {
    let f = Fixture::new();
    let mut f = f;
    let path = f.files.join("image.png");
    image::RgbImage::from_pixel(1000, 500, image::Rgb([44, 102, 84]))
        .save(&path)
        .unwrap();
    f.store.add_file(&fsops::inspect(&path).unwrap()).unwrap();
    let id = f.store.all_paths().unwrap()[0].0.clone();
    let a = f.store.file(&id).unwrap();
    let preview = fsops::preview(&f.store.root, &a, false).unwrap();
    let data = STANDARD.decode(preview["data"].as_str().unwrap()).unwrap();
    let image = image::load_from_memory(&data).unwrap();
    assert_eq!((image.width(), image.height()), (420, 210));
    assert!(fsops::cache_path(&f.store.root, &a, false).exists());
    image::RgbImage::new(600, 800).save(&path).unwrap();
    let new = f.store.refresh_file(&id).unwrap();
    assert_ne!(a.revision, new.revision);
    assert!(!fsops::cache_path(&f.store.root, &new, false).exists());
}
#[test]
fn utf_text_preview_and_corrupt_image_degrade() {
    let mut f = Fixture::new();
    let a = f.add(
        "中文.txt",
        "<script>alert('plain')</script> 中文".as_bytes(),
    );
    let r = fsops::preview(&f.store.root, &a, true).unwrap();
    assert!(r["text"].as_str().unwrap().contains("<script>"));
    let a = f.add("utf16.txt", &[255, 254, 0x2d, 0x4e, 0x87, 0x65]);
    assert_eq!(
        fsops::preview(&f.store.root, &a, true).unwrap()["text"],
        "中文"
    );
    let b = f.add("broken.png", b"broken");
    assert!(fsops::preview(&f.store.root, &b, false).is_err());
}
#[test]
fn backup_roundtrip_captures_wal_and_keeps_protective_backup() {
    let mut f = Fixture::new();
    let a = f.add("a.txt", b"original");
    let t = f.store.create_tag("备份").unwrap();
    f.tag(&[&a.id], &[&t.id]);
    f.store
        .save_note(&json!({"id":a.id,"text":"原备注","version":0}))
        .unwrap();
    let backup = f.files.join("test.sqtagbackup");
    f.store.export_backup(&backup).unwrap();
    let inspected = backup::inspect_backup(&backup, &f.store.root).unwrap();
    assert_eq!(inspected.info["fileCount"], 1);
    drop(inspected);
    f.store
        .save_note(&json!({"id":a.id,"text":"后备注","version":1}))
        .unwrap();
    f.patch("files.remove", &[&a.id], json!({})).unwrap();
    let r = f.store.restore_backup(&backup).unwrap();
    assert_eq!(f.store.file(&a.id).unwrap().note, "原备注");
    assert_eq!(f.store.file(&a.id).unwrap().tags[0].id, t.id);
    assert!(Path::new(r["recoveryBackup"].as_str().unwrap()).exists());
    assert_eq!(fs::read(&a.path).unwrap(), b"original");
    assert!(f.store.undo.is_empty());
    assert!(!f.store.root.join("restore-state.toml").exists());
}
#[test]
fn corrupt_backup_does_not_change_library() {
    let mut f = Fixture::new();
    let a = f.add("a.txt", b"a");
    let broken = f.files.join("broken.sqtagbackup");
    fs::write(&broken, b"not a backup").unwrap();
    assert!(f.store.restore_backup(&broken).is_err());
    assert_eq!(f.store.file(&a.id).unwrap().name, a.name);
    assert_eq!(f.query(q())["total"], 1);
}
#[test]
fn backup_checksum_rejects_tampering() {
    use std::io::{Read, Write};
    let mut f = Fixture::new();
    f.add("a.txt", b"a");
    let path = f.files.join("ok.sqtagbackup");
    f.store.export_backup(&path).unwrap();
    let mut reader = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    let bad = f.files.join("bad.sqtagbackup");
    let mut writer = zip::ZipWriter::new(fs::File::create(&bad).unwrap());
    for i in 0..reader.len() {
        let mut file = reader.by_index(i).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        if file.name() == "metadata.sqlite" {
            bytes[100] ^= 1;
        }
        writer
            .start_file(file.name(), zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    writer.finish().unwrap();
    assert!(backup::inspect_backup(&bad, &f.store.root)
        .err()
        .unwrap()
        .contains("校验失败"));
}
#[test]
fn restart_persists_settings_and_annotations() {
    let mut f = Fixture::new();
    let a = f.add("a.txt", b"a");
    f.store
        .save_settings(&json!({"key":"theme","value":"dark"}))
        .unwrap();
    f.store
        .save_settings(&json!({"key":"galleryColumns","value":5}))
        .unwrap();
    f.store
        .save_settings(&json!({"key":"sidebarCollapsed","value":true}))
        .unwrap();
    f.store
        .save_note(&json!({"id":a.id,"text":"persisted","version":0}))
        .unwrap();
    let root = f.store.root.clone();
    drop(f.store);
    let reopened = Store::open(&root).unwrap();
    assert_eq!(reopened.file(&a.id).unwrap().note, "persisted");
    assert_eq!(reopened.bootstrap().unwrap()["settings"]["theme"], "dark");
    assert_eq!(
        reopened.bootstrap().unwrap()["settings"]["galleryColumns"],
        5
    );
    assert_eq!(
        reopened.bootstrap().unwrap()["settings"]["sidebarCollapsed"],
        true
    );
}
#[test]
fn gallery_preferences_reject_invalid_values_without_overwriting_saved_choice() {
    let mut f = Fixture::new();
    f.store
        .save_settings(&json!({"key":"galleryColumns","value":4}))
        .unwrap();
    f.store
        .save_settings(&json!({"key":"sidebarCollapsed","value":false}))
        .unwrap();
    for value in [
        json!(0),
        json!(1),
        json!(9),
        json!(3.5),
        json!("4"),
        Value::Null,
    ] {
        assert!(f
            .store
            .save_settings(&json!({"key":"galleryColumns","value":value}))
            .is_err());
    }
    assert!(f
        .store
        .save_settings(&json!({"key":"sidebarCollapsed","value":"true"}))
        .is_err());
    assert_eq!(
        f.store.bootstrap().unwrap()["settings"]["galleryColumns"],
        4
    );
    assert_eq!(
        f.store.bootstrap().unwrap()["settings"]["sidebarCollapsed"],
        false
    );
    let backup = f.files.join("layout.sqtagbackup");
    f.store.export_backup(&backup).unwrap();
    f.store
        .save_settings(&json!({"key":"galleryColumns","value":8}))
        .unwrap();
    f.store.restore_backup(&backup).unwrap();
    assert_eq!(
        f.store.bootstrap().unwrap()["settings"]["galleryColumns"],
        4
    );
}

#[test]
fn sidebar_width_is_validated_persisted_and_restored_from_backup() {
    let mut f = Fixture::new();
    f.store
        .save_settings(&json!({"key":"sidebarWidth","value":288}))
        .unwrap();
    for invalid in [
        json!(179),
        json!(361),
        json!(215.5),
        json!("216"),
        Value::Null,
    ] {
        assert!(f
            .store
            .save_settings(&json!({"key":"sidebarWidth","value":invalid}))
            .is_err());
    }
    assert_eq!(
        f.store.bootstrap().unwrap()["settings"]["sidebarWidth"],
        288
    );
    let backup = f.files.join("sidebar-width.sqtagbackup");
    f.store.export_backup(&backup).unwrap();
    f.store
        .save_settings(&json!({"key":"sidebarWidth","value":180}))
        .unwrap();
    f.store.restore_backup(&backup).unwrap();
    assert_eq!(
        f.store.bootstrap().unwrap()["settings"]["sidebarWidth"],
        288
    );
    let root = f.store.root.clone();
    drop(f.store);
    assert_eq!(
        Store::open(&root).unwrap().bootstrap().unwrap()["settings"]["sidebarWidth"],
        288
    );
}

#[test]
fn inspector_width_is_validated_persisted_and_restored_from_backup() {
    let mut f = Fixture::new();
    f.store
        .save_settings(&json!({"key":"inspectorWidth","value":620}))
        .unwrap();
    for invalid in [
        json!(299),
        json!(901),
        json!(419.5),
        json!("420"),
        Value::Null,
    ] {
        assert!(f
            .store
            .save_settings(&json!({"key":"inspectorWidth","value":invalid}))
            .is_err());
    }
    assert_eq!(
        f.store.bootstrap().unwrap()["settings"]["inspectorWidth"],
        620
    );
    let backup = f.files.join("inspector-width.sqtagbackup");
    f.store.export_backup(&backup).unwrap();
    f.store
        .save_settings(&json!({"key":"inspectorWidth","value":300}))
        .unwrap();
    f.store.restore_backup(&backup).unwrap();
    assert_eq!(
        f.store.bootstrap().unwrap()["settings"]["inspectorWidth"],
        620
    );
    let root = f.store.root.clone();
    drop(f.store);
    assert_eq!(
        Store::open(&root).unwrap().bootstrap().unwrap()["settings"]["inspectorWidth"],
        620
    );
}

#[test]
fn floating_window_preferences_validate_dimensions_and_preserve_pinning_choice() {
    let mut f = Fixture::new();
    let size = json!({"width":496,"height":572});
    f.store
        .save_settings(&json!({"key":"floatingSize","value":size}))
        .unwrap();
    f.store
        .save_settings(&json!({"key":"floatingAlwaysOnTop","value":false}))
        .unwrap();
    for invalid in [
        json!({"width":279,"height":460}),
        json!({"width":901,"height":460}),
        json!({"width":340,"height":64}),
        json!({"width":340,"height":1001}),
        json!({"width":340.5,"height":460}),
        Value::Null,
    ] {
        assert!(f
            .store
            .save_settings(&json!({"key":"floatingSize","value":invalid}))
            .is_err());
    }
    assert!(f
        .store
        .save_settings(&json!({"key":"floatingAlwaysOnTop","value":"false"}))
        .is_err());
    assert_eq!(
        f.store.bootstrap().unwrap()["settings"]["floatingSize"],
        size
    );
    let root = f.store.root.clone();
    drop(f.store);
    let boot = Store::open(&root).unwrap().bootstrap().unwrap();
    assert_eq!(boot["settings"]["floatingSize"], size);
    assert_eq!(boot["settings"]["floatingAlwaysOnTop"], false);
}
#[test]
fn future_schema_is_rejected() {
    let f = Fixture::new();
    let root = f.store.root.clone();
    f.store
        .conn
        .pragma_update(None, "user_version", 999)
        .unwrap();
    drop(f.store);
    assert!(Store::open(&root)
        .err()
        .unwrap()
        .starts_with("VERSION_UNSUPPORTED"));
}
#[test]
fn interrupted_restore_marker_is_validated_on_startup() {
    let f = Fixture::new();
    let root = f.store.root.clone();
    fs::write(
        root.join("restore-state.toml"),
        "recovery_backup='retained'",
    )
    .unwrap();
    drop(f.store);
    let reopened = Store::open(&root).unwrap();
    assert!(!reopened.recovery_notice.is_empty());
    assert!(!root.join("restore-state.toml").exists());
}
#[test]
fn representative_ten_thousand_file_search_benchmark() {
    let mut f = Fixture::new();
    let start = Instant::now();
    let tags: Vec<_> = (0..40)
        .map(|i| f.store.create_tag(&format!("项目{i}")).unwrap())
        .collect();
    let tx = f.store.conn.transaction().unwrap();
    for i in 0..10000 {
        let fid = format!("bench-{i}");
        let name = format!("资料{i} 产品设计.txt");
        let note = format!(
            "项目调研 客户反馈 第{i}条资料，需要持续整理和对照使用。{}",
            "用户体验与设计流程。".repeat(i % 10 + 1)
        );
        tx.execute("INSERT INTO files(id,path,path_key,parent,name,name_key,extension,kind,bytes,modified,added,revision,note,note_key) VALUES(?,?,?,?,?,?, 'txt','text',1024,100,100,'rev',?,?)",rusqlite::params![fid,format!("C:\\fixtures\\{name}"),format!("C:\\fixtures\\{name}"),"C:\\fixtures",name,key(&name),note,key(&note)]).unwrap();
        for j in 0..3 {
            tx.execute(
                "INSERT INTO file_tags(file_id,tag_id) VALUES(?,?)",
                rusqlite::params![fid, tags[(i + j * 7) % tags.len()].id],
            )
            .unwrap();
        }
    }
    tx.commit().unwrap();
    let mut times = vec![];
    for i in 0..120 {
        let mut query = q();
        query.text = if i % 3 == 0 {
            "产品 客户反馈".into()
        } else {
            format!("资料{}", i % 100)
        };
        query.include = vec![tags[i % 40].id.clone()];
        if i % 2 == 0 {
            query.mode = "any".into();
            query.include.push(tags[(i + 7) % 40].id.clone());
        }
        let timer = Instant::now();
        let result = f.query(query);
        assert!(result["total"].is_number());
        times.push(timer.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(f64::total_cmp);
    println!("BENCH: 10,000 files, 40 tags, 30,000 links, varied Chinese notes; 120 queries; p50={:.2}ms p95={:.2}ms max={:.2}ms seed+query={:.2}s",times[60],times[114],times[119],start.elapsed().as_secs_f64());
    assert!(times[114] < 300.0, "search p95 exceeded 300ms");
}

#[test]
fn tag_export_copies_all_matches_without_overwriting_originals_or_names() {
    let mut f = Fixture::new();
    let tag = f.store.create_tag("导出素材").unwrap();
    let out = f.files.join("exports");
    fs::create_dir(&out).unwrap();
    fs::create_dir(out.join("导出素材")).unwrap();
    fs::write(out.join("导出素材/保留.txt"), b"keep").unwrap();
    let mut ids = vec![];
    for i in 0..105 {
        fs::create_dir(f.files.join(format!("folder-{i}"))).unwrap();
        ids.push(
            f.add(
                &format!("folder-{i}/同名.txt"),
                format!("content-{i}").as_bytes(),
            )
            .id,
        );
    }
    f.tag(
        &ids.iter().map(String::as_str).collect::<Vec<_>>(),
        &[&tag.id],
    );
    let result =
        crate::tag_export::export(f.store.tag_export_plan(&tag.id).unwrap(), &out).unwrap();
    assert_eq!(result["copied"], 105);
    assert_eq!(result["failed"], 0);
    let target = Path::new(result["path"].as_str().unwrap());
    assert_eq!(target.file_name().unwrap(), "导出素材 (2)");
    assert_eq!(fs::read_dir(target).unwrap().count(), 106);
    let content = fs::read_to_string(target.join("_拾签导出清单.txt")).unwrap();
    assert!(content.contains("已复制：105"));
    assert_eq!(fs::read(out.join("导出素材/保留.txt")).unwrap(), b"keep");
    for i in 0..105 {
        assert_eq!(
            fs::read(f.files.join(format!("folder-{i}/同名.txt"))).unwrap(),
            format!("content-{i}").as_bytes()
        );
    }
}
#[test]
fn tag_export_reports_missing_files_and_excludes_removed_records() {
    let mut f = Fixture::new();
    let tag = f.store.create_tag("导出测试").unwrap();
    let a = f.add("正常.txt", b"a");
    let b = f.add("缺失.txt", b"b");
    let c = f.add("移除.txt", b"c");
    f.tag(&[&a.id, &b.id, &c.id], &[&tag.id]);
    f.patch("files.remove", &[&c.id], json!({})).unwrap();
    fs::remove_file(&b.path).unwrap();
    let result =
        crate::tag_export::export(f.store.tag_export_plan(&tag.id).unwrap(), &f.files).unwrap();
    assert_eq!(result["total"], 2);
    assert_eq!(result["copied"], 1);
    assert_eq!(result["failed"], 1);
    assert_eq!(result["failures"][0]["path"], b.path);
    assert!(!Path::new(result["path"].as_str().unwrap())
        .join("移除.txt")
        .exists());
}
#[test]
fn tag_export_rejects_empty_and_app_data_and_sanitizes_folder_names() {
    let mut f = Fixture::new();
    let tag = f.store.create_tag("CON").unwrap();
    assert!(f
        .store
        .tag_export_plan(&tag.id)
        .err()
        .unwrap()
        .contains("EXPORT_EMPTY"));
    let a = f.add("_拾签导出清单.txt", b"original manifest-name file");
    f.tag(&[&a.id], &[&tag.id]);
    assert!(
        crate::tag_export::export(f.store.tag_export_plan(&tag.id).unwrap(), &f.store.root)
            .is_err()
    );
    let result =
        crate::tag_export::export(f.store.tag_export_plan(&tag.id).unwrap(), &f.files).unwrap();
    let target = Path::new(result["path"].as_str().unwrap());
    assert_eq!(target.file_name().unwrap(), "_CON");
    assert_eq!(
        fs::read(target.join("_拾签导出清单 (2).txt")).unwrap(),
        b"original manifest-name file"
    );
    assert_eq!(fs::read(&a.path).unwrap(), b"original manifest-name file");
}

#[test]
fn pdf_reader_bypasses_thumbnail_and_checks_original_revision() {
    let mut f = Fixture::new();
    let original = b"%PDF-1.7\nreader document\n%%EOF";
    let file = f.add("reader.pdf", original);
    fs::write(fsops::cache_path(&f.store.root, &file, false), b"thumbnail").unwrap();
    assert_eq!(
        fsops::preview(&f.store.root, &file, false).unwrap()["kind"],
        "image"
    );
    let data = fsops::pdf_document(&f.store.root, &file).unwrap();
    assert_eq!(data["kind"], "pdf");
    assert_eq!(
        STANDARD.decode(data["data"].as_str().unwrap()).unwrap(),
        original
    );
    fs::write(
        &file.path,
        b"%PDF-1.7\nchanged document with a new size\n%%EOF",
    )
    .unwrap();
    assert!(fsops::pdf_document(&f.store.root, &file).is_err());
}

#[test]
fn pdf_reader_bounds_size_and_rejects_other_types_or_missing_files() {
    let mut f = Fixture::new();
    let text = f.add("not-pdf.txt", b"text");
    assert!(fsops::pdf_document(&f.store.root, &text).is_err());
    let path = f.files.join("large.pdf");
    let handle = fs::File::create(&path).unwrap();
    handle.set_len(64 * 1024 * 1024 + 1).unwrap();
    drop(handle);
    f.store.add_file(&fsops::inspect(&path).unwrap()).unwrap();
    let id = f
        .store
        .all_paths()
        .unwrap()
        .into_iter()
        .find(|(_, p)| p.ends_with("large.pdf"))
        .unwrap()
        .0;
    let large = f.store.file(&id).unwrap();
    assert!(fsops::pdf_document(&f.store.root, &large).is_err());
    let small = f.add("missing.pdf", b"%PDF-1.7\n%%EOF");
    fs::remove_file(&small.path).unwrap();
    assert!(fsops::pdf_document(&f.store.root, &small).is_err());
}

#[test]
fn html_preview_reads_existing_other_records_and_enforces_encoding_bounds() {
    let mut f = Fixture::new();
    let html = f.add("page.html", "<h1>中文静态页面</h1>".as_bytes());
    assert_eq!(html.kind, "other");
    fs::write(fsops::cache_path(&f.store.root, &html, false), b"not html").unwrap();
    assert_eq!(
        fsops::html_document(&f.store.root, &html).unwrap()["text"],
        "<h1>中文静态页面</h1>"
    );
    let utf = f.add("unicode.htm", &[255, 254, 0x2d, 0x4e, 0x87, 0x65]);
    assert_eq!(
        fsops::html_document(&f.store.root, &utf).unwrap()["text"],
        "中文"
    );
    let text = f.add("other.txt", b"not html");
    assert!(fsops::html_document(&f.store.root, &text).is_err());
    let large = f.add("too-big.html", &vec![b'a'; 2 * 1024 * 1024 + 1]);
    assert!(fsops::html_document(&f.store.root, &large).is_err());
    fs::write(&html.path, b"changed").unwrap();
    assert!(fsops::html_document(&f.store.root, &html).is_err());
}

#[test]
fn floating_history_undo_identity_rejects_stale_and_replayed_requests() {
    let mut f = Fixture::new();
    let file = f.add("history.txt", b"history");
    let a = f.store.create_tag("第一步").unwrap();
    let b = f.store.create_tag("第二步").unwrap();
    f.tag(&[&file.id], &[&a.id]);
    let first = f.store.undo.last().unwrap().id.clone();
    f.tag(&[&file.id], &[&b.id]);
    let second = f.store.undo.last().unwrap().id.clone();
    assert_ne!(first, second);
    assert!(f
        .store
        .undo_checked(&first)
        .unwrap_err()
        .starts_with("UNDO_CHANGED"));
    assert_eq!(f.store.file(&file.id).unwrap().tags.len(), 2);
    f.store.undo_checked(&second).unwrap();
    assert!(f
        .store
        .undo_checked(&second)
        .unwrap_err()
        .starts_with("UNDO_CHANGED"));
    assert_eq!(f.store.file(&file.id).unwrap().tags[0].id, a.id);
    // Advancing guards does not change earlier entry identities.
    f.store.undo_checked(&first).unwrap();
    assert!(f.store.file(&file.id).unwrap().tags.is_empty());
}
#[test]
fn floating_annotation_returns_its_own_undo_identity_only_for_changes() {
    let mut f = Fixture::new();
    let file = f.add("annotation-history.txt", b"history");
    let tag = f.store.create_tag("浮窗步骤").unwrap();
    let payload = json!({"tagId":tag.id,"ids":[file.id],"paths":[]});
    let result = f.store.annotate(&payload).unwrap();
    assert_eq!(result["undoId"], f.store.undo.last().unwrap().id);
    let repeated = f.store.annotate(&payload).unwrap();
    assert!(repeated.get("undoId").is_none());
    f.store
        .undo_checked(result["undoId"].as_str().unwrap())
        .unwrap();
    assert!(f.store.file(&file.id).unwrap().tags.is_empty());
}

#[test]
fn mutation_noop_does_not_borrow_an_earlier_undo_identity() {
    let mut f = Fixture::new();
    let file = f.add("toast-noop.txt", b"fixture");
    let tag = f.store.create_tag("步骤身份").unwrap();
    let first = f
        .patch(
            "files.tags",
            &[&file.id],
            json!({"add":[tag.id],"remove":[]}),
        )
        .unwrap();
    let same = f
        .patch(
            "files.tags",
            &[&file.id],
            json!({"add":[tag.id],"remove":[]}),
        )
        .unwrap();
    assert!(same["undoId"].is_null());
    assert_eq!(
        f.store.undo.last().unwrap().id,
        first["undoId"].as_str().unwrap()
    );
    f.store
        .undo_checked(first["undoId"].as_str().unwrap())
        .unwrap();
    assert!(f.store.file(&file.id).unwrap().tags.is_empty());
}

#[test]
fn markdown_atomic_save_preserves_associations_bom_crlf_and_rejects_stale_source() {
    let mut f = Fixture::new();
    let file = f.add("说明.md", "\u{feff}# 原内容\r\n正文\r\n".as_bytes());
    let tag = f.store.create_tag("编辑保留").unwrap();
    f.tag(&[&file.id], &[&tag.id]);
    let before = crate::text_document::read(&f.store.file(&file.id).unwrap()).unwrap();
    let save = json!({"id":file.id,"revision":before["revision"],"digest":before["digest"],"text":"# 新内容\n正文更新\n"});
    let result = f.store.save_markdown(&save).unwrap();
    assert_eq!(
        fs::read(&file.path).unwrap(),
        "\u{feff}# 新内容\r\n正文更新\r\n".as_bytes()
    );
    let current = f.store.file(&file.id).unwrap();
    assert_eq!(current.tags[0].id, tag.id);
    assert_eq!(current.status, "available");
    assert_eq!(
        current.identity,
        fsops::inspect(Path::new(&file.path)).unwrap().identity
    );
    assert_eq!(result["text"], "# 新内容\r\n正文更新\r\n");
    assert!(f
        .store
        .save_markdown(&save)
        .unwrap_err()
        .starts_with("FILE_CHANGED"));
    let mut bad =
        json!({"id":file.id,"revision":result["revision"],"digest":"wrong","text":"不得写入"});
    assert!(f
        .store
        .save_markdown(&bad)
        .unwrap_err()
        .starts_with("FILE_CHANGED"));
    bad["digest"] = result["digest"].clone();
    fs::write(&file.path, "外部修改").unwrap();
    assert!(f
        .store
        .save_markdown(&bad)
        .unwrap_err()
        .starts_with("FILE_CHANGED"));
    assert_eq!(fs::read_to_string(&file.path).unwrap(), "外部修改");
    assert!(fs::read_dir(&f.files).unwrap().all(|e| !e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".shiqian-edit-")));
}
#[test]
fn markdown_editor_rejects_partial_non_utf8_readonly_and_non_markdown_writes() {
    let mut f = Fixture::new();
    let huge = f.add("huge.md", &vec![b'a'; 1024 * 1024 + 1]);
    assert!(crate::text_document::read(&huge)
        .unwrap_err()
        .starts_with("PREVIEW_TOO_LARGE"));
    let binary = f.add("wrong.md", &[255, 254, 0]);
    assert!(crate::text_document::read(&binary)
        .unwrap_err()
        .starts_with("TEXT_ENCODING"));
    let plain = f.add("plain.txt", b"original");
    assert!(f
        .store
        .save_markdown(&json!({"id":plain.id}))
        .unwrap_err()
        .starts_with("EDIT_UNSUPPORTED"));
    assert_eq!(fs::read(&plain.path).unwrap(), b"original");
    let file = f.add("readonly.md", b"original");
    let doc = crate::text_document::read(&file).unwrap();
    let mut permissions = fs::metadata(&file.path).unwrap().permissions();
    let original = permissions.clone();
    permissions.set_readonly(true);
    fs::set_permissions(&file.path, permissions).unwrap();
    let saved = f.store.save_markdown(
        &json!({"id":file.id,"revision":doc["revision"],"digest":doc["digest"],"text":"new"}),
    );
    fs::set_permissions(&file.path, original).unwrap();
    assert!(saved.unwrap_err().starts_with("ACCESS_DENIED"));
    assert_eq!(fs::read(&file.path).unwrap(), b"original");
}
