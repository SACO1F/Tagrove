use crate::{
    db::Store,
    fsops,
    model::*,
    transfer::{self, Control},
};
use serde_json::json;
use std::{fs, path::Path, sync::atomic::Ordering, time::Instant};
struct Fixture {
    _root: tempfile::TempDir,
    store: Store,
    files: std::path::PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let files = root.path().join("文件");
        fs::create_dir(&files).unwrap();
        let store = Store::open(&root.path().join("库")).unwrap();
        Self {
            _root: root,
            store,
            files,
        }
    }
    fn add(&mut self, name: &str) -> FileRecord {
        let path = self.files.join(name);
        fs::write(&path, b"synthetic portable content").unwrap();
        self.store
            .add_file_with_ai(&fsops::inspect(&path).unwrap(), false)
            .unwrap();
        let result = self.store.query(&Query::default()).unwrap();
        let fid = result["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == name)
            .unwrap()["id"]
            .as_str()
            .unwrap();
        self.store.file(fid).unwrap()
    }
    fn pack(&self, ids: &[String]) -> std::path::PathBuf {
        let plan = self.store.transfer_plan(&json!({"ids":ids})).unwrap();
        let fingerprint = plan.info["fingerprint"].as_str().unwrap().to_string();
        let path = self.files.join(format!("{}.sqtagpack", id()));
        transfer::export(plan, &path, &fingerprint, &Control::default()).unwrap();
        path
    }
}
#[test]
fn full_folder_names_survive_package_handoff_and_legacy_names_recover_identity() {
    use sha2::{Digest, Sha256};
    let name = "这是需要完整保留的文件夹名称".repeat(4);
    let mut a = Fixture::new();
    a.files = a.files.join(&name);
    fs::create_dir_all(&a.files).unwrap();
    let f = a.add("长目录.txt");
    let tag = f.tags.iter().find(|t| t.source == "folder").unwrap();
    assert_eq!(tag.name, name);
    let tid = tag.id.clone();
    let pack = a.pack(&[f.id.clone()]);
    let mut b = Fixture::new();
    let control = Control::default();
    let preview = transfer::preview(&pack, &control).unwrap();
    let imported = b
        .store
        .import_package(
            &pack,
            &b.files.clone(),
            preview["fingerprint"].as_str().unwrap(),
            &control,
        )
        .unwrap();
    let copy = b.store.file(imported["ids"][0].as_str().unwrap()).unwrap();
    assert!(copy
        .tags
        .iter()
        .any(|t| t.source == "folder" && t.name == name));
    let legacy = format!(
        "{}…{}",
        name.chars().take(31).collect::<String>(),
        &format!("{:x}", Sha256::digest(name.as_bytes()))[..8]
    );
    a.store
        .conn
        .execute(
            "UPDATE tags SET name=?,name_key=? WHERE id=?",
            rusqlite::params![legacy, key(&legacy), tid],
        )
        .unwrap();
    a.store.restore_full_folder_names().unwrap();
    let restored = a.store.file(&f.id).unwrap();
    assert!(restored.tags.iter().any(|t| t.id == tid && t.name == name));
    a.store.restore_full_folder_names().unwrap();
    let root = a.store.root.clone();
    drop(a.store);
    let store = Store::open(&root).unwrap();
    assert!(store
        .file(&f.id)
        .unwrap()
        .tags
        .iter()
        .any(|t| t.id == tid && t.name == name));
}
#[test]
fn transfer_round_trip_additive_tags_notes_favorites_and_ai_provenance() {
    let mut a = Fixture::new();
    let f = a.add("中文 原图.txt");
    let manual = a.store.create_tag("手工").unwrap();
    let ai = a.store.create_tag_with_source("待确认", "ai").unwrap();
    let accepted = a.store.create_tag_with_source("已确认", "ai").unwrap();
    a.store
        .conn
        .execute(
            "UPDATE files SET note='资料备注',note_key='资料备注',favorite=1 WHERE id=?",
            [&f.id],
        )
        .unwrap();
    for (tid, source, meta) in [
        (&manual.id, "manual", json!({})),
        (
            &ai.id,
            "ai",
            json!({"model":"synthetic","reason":"test","updatedAt":now(),"confirmed":false}),
        ),
        (
            &accepted.id,
            "ai",
            json!({"model":"synthetic","reason":"test","updatedAt":now(),"confirmed":true}),
        ),
    ] {
        a.store
            .conn
            .execute(
                "INSERT INTO file_tags(file_id,tag_id,source,ai_meta) VALUES(?,?,?,?)",
                rusqlite::params![f.id, tid, source, meta.to_string()],
            )
            .unwrap();
    }
    let path = a.pack(&[f.id.clone()]);
    let mut b = Fixture::new();
    let old = b.add("已有.txt");
    let reused = b.store.create_tag("手工").unwrap();
    let control = Control::default();
    let preview = transfer::preview(&path, &control).unwrap();
    assert_eq!(preview["fileCount"], 1);
    let imported = b
        .store
        .import_package(
            &path,
            &b.files.clone(),
            preview["fingerprint"].as_str().unwrap(),
            &control,
        )
        .unwrap();
    let record = b.store.file(imported["ids"][0].as_str().unwrap()).unwrap();
    assert_eq!(record.name, "中文 原图.txt");
    assert_eq!(record.note, "资料备注");
    assert!(record.favorite);
    assert_eq!(record.added, f.added);
    assert_eq!(record.modified, f.modified);
    assert_eq!(fs::read(&record.path).unwrap(), fs::read(&f.path).unwrap());
    assert_eq!(
        record.tags.iter().find(|t| t.name == "手工").unwrap().id,
        reused.id
    );
    assert!(
        !record
            .tags
            .iter()
            .find(|t| t.name == "待确认")
            .unwrap()
            .ai
            .as_ref()
            .unwrap()
            .confirmed
    );
    assert!(!b
        .store
        .tag_pool()
        .unwrap()
        .iter()
        .any(|t| t.name == "待确认"));
    assert!(b.store.floating_presets().unwrap()["ids"]
        .as_array()
        .unwrap()
        .contains(&json!(
            record.tags.iter().find(|t| t.name == "已确认").unwrap().id
        )));
    assert_eq!(b.store.file(&old.id).unwrap().note, "");
    assert!(record.ai_task.is_none());
    b.store.validate().unwrap();
    assert!(b
        .store
        .import_package(
            &path,
            &b.files.clone(),
            preview["fingerprint"].as_str().unwrap(),
            &control
        )
        .unwrap_err()
        .contains("PACKAGE_ALREADY_IMPORTED"));
    assert_eq!(b.store.bootstrap().unwrap()["counts"]["all"], 2);
    let mut zip = zip::ZipArchive::new(fs::File::open(path).unwrap()).unwrap();
    use std::io::Read;
    let mut manifest = String::new();
    zip.by_name("manifest.json")
        .unwrap()
        .read_to_string(&mut manifest)
        .unwrap();
    assert!(!manifest.contains(&a.files.to_string_lossy().to_string()));
    assert!(!manifest.contains("endpoint"));
}
#[test]
fn transfer_missing_changed_preview_and_existing_outputs_are_rejected() {
    let mut a = Fixture::new();
    let f = a.add("资料.txt");
    let plan = a.store.transfer_plan(&json!({"ids":[f.id]})).unwrap();
    let fingerprint = plan.info["fingerprint"].as_str().unwrap().to_string();
    fs::write(&f.path, b"changed").unwrap();
    let target = a.files.join("pack.sqtagpack");
    assert!(transfer::export(plan, &target, &fingerprint, &Control::default()).is_err());
    assert!(!target.exists());
    fs::remove_file(&f.path).unwrap();
    let plan = a.store.transfer_plan(&json!({"ids":[f.id]})).unwrap();
    assert_eq!(plan.info["missing"].as_array().unwrap().len(), 1);
    let mut a = Fixture::new();
    let f = a.add("资料.txt");
    let target = a.pack(&[f.id]);
    let before = fs::read(&target).unwrap();
    let plan = a.store.transfer_plan(&json!({})).unwrap();
    let fingerprint = plan.info["fingerprint"].as_str().unwrap().to_string();
    assert!(transfer::export(plan, &target, &fingerprint, &Control::default()).is_err());
    assert_eq!(before, fs::read(target).unwrap());
}
fn corrupt(input: &Path, output: &Path, extra: bool) {
    use std::io::{Read, Write};
    let mut archive = zip::ZipArchive::new(fs::File::open(input).unwrap()).unwrap();
    let mut zip = zip::ZipWriter::new(fs::File::create(output).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).unwrap();
        let name = f.name().to_string();
        let mut bytes = vec![];
        f.read_to_end(&mut bytes).unwrap();
        if !extra && name.starts_with("files/") {
            bytes[0] ^= 1;
        }
        zip.start_file(name, options).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    if extra {
        zip.start_file("../escape.txt", options).unwrap();
        zip.write_all(b"escape").unwrap();
    }
    zip.finish().unwrap();
}
#[test]
fn transfer_corruption_and_traversal_never_change_library_or_destination() {
    let mut a = Fixture::new();
    let f = a.add("文件.txt");
    let pack = a.pack(&[f.id]);
    let mut b = Fixture::new();
    for extra in [false, true] {
        let bad = a.files.join(format!("bad-{extra}.sqtagpack"));
        corrupt(&pack, &bad, extra);
        assert!(transfer::preview(&bad, &Control::default()).is_err());
        assert!(b
            .store
            .import_package(&bad, &b.files.clone(), "unused", &Control::default())
            .is_err());
        assert_eq!(b.store.bootstrap().unwrap()["counts"]["all"], 0);
        assert_eq!(fs::read_dir(&b.files).unwrap().count(), 0);
    }
}
#[test]
fn transfer_cancellation_rolls_back_without_touching_originals() {
    let mut a = Fixture::new();
    let f = a.add("文件.txt");
    let plan = a.store.transfer_plan(&json!({"ids":[f.id]})).unwrap();
    let fingerprint = plan.info["fingerprint"].as_str().unwrap().to_string();
    let c = Control::default();
    c.cancel.store(true, Ordering::SeqCst);
    let path = a.files.join("cancel.sqtagpack");
    assert!(transfer::export(plan, &path, &fingerprint, &c)
        .unwrap_err()
        .contains("TRANSFER_CANCELLED"));
    assert!(!path.exists());
    assert!(Path::new(&f.path).exists());
}
#[test]
fn transfer_database_failure_cleans_only_own_copies() {
    let mut a = Fixture::new();
    let f = a.add("文件.txt");
    let pack = a.pack(&[f.id]);
    let mut b = Fixture::new();
    fs::write(b.files.join("保留.txt"), b"keep").unwrap();
    let preview = transfer::preview(&pack, &Control::default()).unwrap();
    b.store.conn.execute_batch("CREATE TRIGGER reject_import BEFORE INSERT ON files BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;").unwrap();
    assert!(b
        .store
        .import_package(
            &pack,
            &b.files.clone(),
            preview["fingerprint"].as_str().unwrap(),
            &Control::default()
        )
        .is_err());
    assert_eq!(b.store.bootstrap().unwrap()["counts"]["all"], 0);
    assert_eq!(fs::read_dir(&b.files).unwrap().count(), 1);
    assert_eq!(fs::read(b.files.join("保留.txt")).unwrap(), b"keep");
    assert!(!b.store.root.join("transfer-state.json").exists());
}
#[test]
fn transfer_interrupted_import_recovery_preserves_copies_and_explains_result() {
    let mut f = Fixture::new();
    let root = f.store.root.clone();
    let retained = f.files.join("未完成资料");
    fs::create_dir(&retained).unwrap();
    fs::write(retained.join("保留.txt"), b"keep").unwrap();
    fs::write(
        root.join("transfer-state.json"),
        json!({"packageId":id(),"receipt":"packageReceipt:test","path":retained}).to_string(),
    )
    .unwrap();
    drop(f.store);
    f.store = Store::open(&root).unwrap();
    assert!(f.store.recovery_notice.contains("导入中断"));
    assert!(retained.join("保留.txt").exists());
    assert!(!root.join("transfer-state.json").exists());
}
#[test]
fn beta_large_library_query_baseline() {
    let f = Fixture::new();
    let t = Instant::now();
    f.store.conn.execute_batch("BEGIN").unwrap();
    for i in 0..10000 {
        let fid = format!("benchmark-{i}");
        f.store.conn.execute("INSERT INTO files(id,path,path_key,parent,name,name_key,extension,kind,bytes,modified,added,revision,identity,note,note_key) VALUES(?,?,?,'synthetic',?,?,'txt','text',100,0,0,'synthetic','synthetic',?,?)",rusqlite::params![fid,fid,fid,format!("资料{i}.txt"),format!("资料{i}.txt"),format!("项目{i}"),format!("项目{i}")]).unwrap();
    }
    f.store.conn.execute_batch("COMMIT").unwrap();
    let seed = t.elapsed();
    let start = Instant::now();
    let q = Query {
        text: "项目9999".into(),
        limit: 100,
        ..Default::default()
    };
    for _ in 0..100 {
        assert_eq!(f.store.query(&q).unwrap()["total"], 1);
    }
    println!(
        "BETA_BASELINE 10000 rows: seed={seed:?}, 100 search+page={:?}",
        start.elapsed()
    );
    f.store.validate().unwrap();
}

#[test]
fn transfer_duplicate_names_remain_distinct() {
    let mut a = Fixture::new();
    let f = a.add("同名.txt");
    let another = a.files.join("另一目录");
    fs::create_dir(&another).unwrap();
    fs::write(another.join("同名.txt"), b"different").unwrap();
    a.store
        .add_file_with_ai(&fsops::inspect(&another.join("同名.txt")).unwrap(), false)
        .unwrap();
    let pack = a.pack(&[]);
    let mut b = Fixture::new();
    let preview = transfer::preview(&pack, &Control::default()).unwrap();
    let result = b
        .store
        .import_package(
            &pack,
            &b.files.clone(),
            preview["fingerprint"].as_str().unwrap(),
            &Control::default(),
        )
        .unwrap();
    let records: Vec<_> = result["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| b.store.file(i.as_str().unwrap()).unwrap())
        .collect();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].name, records[1].name);
    assert_ne!(records[0].path, records[1].path);
    assert_ne!(
        fs::read(&records[0].path).unwrap(),
        fs::read(&records[1].path).unwrap()
    );
    assert!(Path::new(&f.path).exists());
}
#[test]
fn transfer_changed_manifest_requires_new_preview() {
    let mut a = Fixture::new();
    let f = a.add("文件.txt");
    let pack = a.pack(&[f.id]);
    let mut b = Fixture::new();
    assert!(b
        .store
        .import_package(&pack, &b.files.clone(), "stale", &Control::default())
        .unwrap_err()
        .contains("TRANSFER_CHANGED"));
    assert_eq!(fs::read_dir(&b.files).unwrap().count(), 0);
}
#[test]
fn beta_wal_recovers_after_real_process_abort() {
    let root = tempfile::tempdir().unwrap();
    let dbroot = root.path().join("crash-library");
    let store = Store::open(&dbroot).unwrap();
    drop(store);
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "transfer_tests::beta_abort_child",
            "--nocapture",
        ])
        .env("SHIQIAN_CRASH_TEST_ROOT", &dbroot)
        .status()
        .unwrap();
    assert!(!status.success());
    let store = Store::open(&dbroot).unwrap();
    store.validate().unwrap();
    assert_eq!(
        store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM tags WHERE name='已提交'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(
        store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM tags WHERE name='未提交'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}
#[test]
#[ignore = "Only invoked by the guarded process-abort recovery test"]
fn beta_abort_child() {
    let root =
        std::env::var_os("SHIQIAN_CRASH_TEST_ROOT").expect("Must be an isolated recovery test");
    let mut store = Store::open(Path::new(&root)).unwrap();
    store.create_tag("已提交").unwrap();
    store.conn.execute_batch("BEGIN IMMEDIATE").unwrap();
    store.create_tag("未提交").unwrap();
    std::process::abort();
}

#[test]
fn transfer_supports_long_chinese_source_and_destination_paths() {
    let mut a = Fixture::new();
    let long = "长路径项目资料文件夹".repeat(3);
    let source = a.files.join(&long).join(&long).join(&long).join(&long);
    fs::create_dir_all(&source).unwrap();
    let path = source.join("完整文件名.txt");
    fs::write(&path, b"long path content").unwrap();
    assert!(path.to_string_lossy().len() > 260);
    a.store
        .add_file_with_ai(&fsops::inspect(&path).unwrap(), false)
        .unwrap();
    let pack = a.pack(&[]);
    let mut b = Fixture::new();
    let destination = b.files.join(&long).join(&long).join(&long);
    fs::create_dir_all(&destination).unwrap();
    let preview = transfer::preview(&pack, &Control::default()).unwrap();
    let result = b
        .store
        .import_package(
            &pack,
            &destination,
            preview["fingerprint"].as_str().unwrap(),
            &Control::default(),
        )
        .unwrap();
    let imported = b.store.file(result["ids"][0].as_str().unwrap()).unwrap();
    assert_eq!(imported.name, "完整文件名.txt");
    assert_eq!(fs::read(&imported.path).unwrap(), b"long path content");
}

fn add_transfer_fixture_at(store: &mut Store, path: &Path, content: &[u8]) -> FileRecord {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
    let meta = fsops::inspect(path).unwrap();
    store.add_file_with_ai(&meta, false).unwrap();
    let fid: String = store
        .conn
        .query_row("SELECT id FROM files WHERE path=?", [&meta.path], |r| {
            r.get(0)
        })
        .unwrap();
    store.file(&fid).unwrap()
}

fn rewrite_transfer_manifest(
    input: &Path,
    output: &Path,
    edit: impl FnOnce(&mut serde_json::Value),
) {
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    let mut archive = zip::ZipArchive::new(fs::File::open(input).unwrap()).unwrap();
    let mut bytes = vec![];
    archive
        .by_name("manifest.json")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    let mut manifest: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    edit(&mut manifest);
    let content = serde_json::to_vec(&manifest).unwrap();
    let checksum = format!("{:x}", Sha256::digest(&content));
    let mut output = zip::ZipWriter::new(fs::File::create(output).unwrap());
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let mut bytes = vec![];
        entry.read_to_end(&mut bytes).unwrap();
        let name = entry.name();
        output
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        output
            .write_all(match name {
                "manifest.json" => &content,
                "manifest.sha256" => checksum.as_bytes(),
                _ => &bytes,
            })
            .unwrap();
    }
    output.finish().unwrap();
}

#[test]
fn transfer_groups_three_pic_and_five_desktop_files_by_original_parent() {
    let mut sender = Fixture::new();
    for (folder, count) in [("Pic", 3), ("Desktop", 5)] {
        for index in 0..count {
            let path = sender.files.join(folder).join(format!("图片-{index}.txt"));
            let file = add_transfer_fixture_at(
                &mut sender.store,
                &path,
                format!("{folder}/{index}").as_bytes(),
            );
            // Original directory grouping must survive removing its automatic tag.
            if index == 0 {
                sender
                    .store
                    .conn
                    .execute("DELETE FROM file_tags WHERE file_id=?", [&file.id])
                    .unwrap();
            }
        }
    }
    let package = sender.pack(&[]);
    let mut recipient = Fixture::new();
    let preview = transfer::preview(&package, &Control::default()).unwrap();
    let result = recipient
        .store
        .import_package(
            &package,
            &recipient.files.clone(),
            preview["fingerprint"].as_str().unwrap(),
            &Control::default(),
        )
        .unwrap();
    let root = Path::new(result["path"].as_str().unwrap());
    assert_eq!(fs::read_dir(root).unwrap().count(), 2);
    for (folder, count) in [("Pic", 3), ("Desktop", 5)] {
        assert_eq!(fs::read_dir(root.join(folder)).unwrap().count(), count);
        for index in 0..count {
            assert_eq!(
                fs::read(root.join(folder).join(format!("图片-{index}.txt"))).unwrap(),
                format!("{folder}/{index}").as_bytes()
            );
        }
    }
    assert_eq!(result["fileCount"], 8);
    recipient.store.validate().unwrap();
}

#[test]
fn transfer_same_folder_name_collisions_keep_both_files_and_contents() {
    let mut sender = Fixture::new();
    add_transfer_fixture_at(
        &mut sender.store,
        &sender.files.join("A/Pic/原图.txt"),
        b"first",
    );
    add_transfer_fixture_at(
        &mut sender.store,
        &sender.files.join("B/Pic/原图.txt"),
        b"second",
    );
    let package = sender.pack(&[]);
    let mut recipient = Fixture::new();
    let preview = transfer::preview(&package, &Control::default()).unwrap();
    let result = recipient
        .store
        .import_package(
            &package,
            &recipient.files.clone(),
            preview["fingerprint"].as_str().unwrap(),
            &Control::default(),
        )
        .unwrap();
    let folder = Path::new(result["path"].as_str().unwrap()).join("Pic");
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 2);
    assert!(folder.join("原图.txt").is_file());
    assert!(folder.join("原图 (2).txt").is_file());
    let mut contents: Vec<_> = fs::read_dir(folder)
        .unwrap()
        .map(|f| fs::read(f.unwrap().path()).unwrap())
        .collect();
    contents.sort();
    assert_eq!(contents, vec![b"first".to_vec(), b"second".to_vec()]);
}

#[test]
fn transfer_legacy_v1_groups_folder_tags_and_rejects_unsafe_v2_folder() {
    let mut sender = Fixture::new();
    add_transfer_fixture_at(
        &mut sender.store,
        &sender.files.join("Pic/测试.txt"),
        b"legacy",
    );
    let package = sender.pack(&[]);
    let legacy = sender.files.join("legacy.sqtagpack");
    rewrite_transfer_manifest(&package, &legacy, |m| {
        m["format"] = json!(1);
        for entry in m["files"].as_array_mut().unwrap() {
            entry.as_object_mut().unwrap().remove("parentFolder");
        }
    });
    let mut recipient = Fixture::new();
    let preview = transfer::preview(&legacy, &Control::default()).unwrap();
    let result = recipient
        .store
        .import_package(
            &legacy,
            &recipient.files.clone(),
            preview["fingerprint"].as_str().unwrap(),
            &Control::default(),
        )
        .unwrap();
    assert!(Path::new(result["path"].as_str().unwrap())
        .join("Pic/测试.txt")
        .is_file());
    for name in ["../escape", "C:\\outside", "CON", ".."] {
        let invalid = sender.files.join(format!("{}.sqtagpack", id()));
        rewrite_transfer_manifest(&package, &invalid, |m| {
            m["files"][0]["parentFolder"] = json!(name)
        });
        assert!(
            transfer::preview(&invalid, &Control::default()).is_err(),
            "{name}"
        );
    }
}
