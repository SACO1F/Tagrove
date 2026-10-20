//! O4 recovery tests use synthetic originals and temporary libraries only.
use crate::{db::Store, fsops, model::Query, search};
use rusqlite::params;
use serde_json::{json, Value};
use std::{
    fs,
    path::Path,
    process::Command,
    sync::{Arc, Barrier},
    thread,
    time::Duration,
};

fn add(s: &mut Store, path: &Path, body: &str) -> String {
    fs::write(path, body).unwrap();
    s.add_file_with_ai(&fsops::inspect(path).unwrap(), false)
        .unwrap();
    s.conn
        .query_row(
            "SELECT id FROM files WHERE path=?",
            [path.to_string_lossy().as_ref()],
            |r| r.get(0),
        )
        .unwrap()
}
fn snapshot(s: &Store, id: &str) -> Value {
    serde_json::to_value(s.file(id).unwrap()).unwrap()
}
fn drain(s: &Store) {
    while let Some(w) = s.search_next().unwrap() {
        assert!(s.search_commit(&w, search::read(&w)).unwrap());
    }
}
fn hits(s: &Store) -> Value {
    s.query(&Query {
        text: "recoveryneedle".into(),
        search_mode: "content".into(),
        limit: 100,
        ..Default::default()
    })
    .unwrap()["total"]
        .clone()
}
fn integrity(s: &Store) {
    for db in ["main", "search", "semantic"] {
        let result: String = s
            .conn
            .query_row(&format!("PRAGMA {db}.integrity_check"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, "ok", "{db}");
    }
    s.validate().unwrap();
}
fn checkpoint(s: &Store, db: &str, mode: &str) {
    let row: (i64, i64, i64) = s
        .conn
        .query_row(&format!("PRAGMA {db}.wal_checkpoint({mode})"), [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    // Concurrent automatic checkpoints may own the checkpoint lock. A busy
    // result is expected backpressure; after the writer joins TRUNCATE must finish.
    if mode == "PASSIVE" {
        assert!(matches!(row.0, 0 | 1));
    } else {
        assert_eq!(row, (0, 0, 0));
    }
}

#[test]
fn o4_restore_to_new_library_rebuilds_index_preserves_exact_annotations() {
    let d = crate::fsops::TmpDir::new();
    let mut s = Store::open(&d.path().join("source")).unwrap();
    let p = d.path().join("original.md");
    let id = add(&mut s, &p, "# recoveryneedle\nOriginal remains unchanged.");
    let bytes = fs::read(&p).unwrap();
    let tag = s.create_tag("confirmed fixture").unwrap();
    s.conn
        .execute(
            "INSERT INTO file_tags(file_id,tag_id,source) VALUES(?,?,'manual')",
            params![id, tag.id],
        )
        .unwrap();
    s.save_note(&json!({"id":id,"version":0,"text":"keep this note"}))
        .unwrap();
    s.conn
        .execute("UPDATE files SET favorite=1 WHERE id=?", [&id])
        .unwrap();
    s.search_control("enable").unwrap();
    drain(&s);
    assert_eq!(hits(&s), 1);
    let expected = snapshot(&s, &id);
    let backup = d.path().join("roundtrip.sqtagbackup");
    s.export_backup(&backup).unwrap();
    let backup_bytes = fs::read(&backup).unwrap();
    let mut restored = Store::open(&d.path().join("destination")).unwrap();
    let extra = add(
        &mut restored,
        &d.path().join("extra.txt"),
        "must not survive restore",
    );
    let result = restored.restore_backup(&backup).unwrap();
    assert!(Path::new(result["recoveryBackup"].as_str().unwrap()).is_file());
    assert!(restored.file(&extra).is_err());
    assert_eq!(snapshot(&restored, &id), expected);
    assert!(!restored.search_enabled().unwrap());
    assert!(!restored.semantic_config().unwrap().enabled);
    restored.search_control("enable").unwrap();
    drain(&restored);
    assert_eq!(hits(&restored), 1);
    assert_eq!(snapshot(&restored, &id), expected);
    integrity(&restored);
    drop(restored);
    let reopened = Store::open(&d.path().join("destination")).unwrap();
    assert_eq!(snapshot(&reopened, &id), expected);
    assert_eq!(hits(&reopened), 1);
    assert_eq!(fs::read(&p).unwrap(), bytes);
    assert_eq!(fs::read(&backup).unwrap(), backup_bytes);
    integrity(&reopened);
}

#[test]
fn o4_detached_writer_cancel_rebuild_rejects_delayed_result() {
    let d = crate::fsops::TmpDir::new();
    let mut s = Store::open(&d.path().join("library")).unwrap();
    let id = add(&mut s, &d.path().join("sample.md"), "recoveryneedle");
    let before = snapshot(&s, &id);
    s.search_control("enable").unwrap();
    for action in ["pause", "cancel", "rebuild"] {
        s.search_control("resume").unwrap();
        let work = s.search_next().unwrap().unwrap();
        let root = s.root.clone();
        let (go, wait) = std::sync::mpsc::channel();
        let writer = thread::spawn(move || {
            let text = search::read(&work);
            wait.recv().unwrap();
            search::commit_detached(&root, &work, text).unwrap()
        });
        s.search_control(action).unwrap();
        go.send(()).unwrap();
        assert!(!writer.join().unwrap(), "late work after {action}");
        assert_eq!(snapshot(&s, &id), before);
    }
    s.search_control("resume").unwrap();
    drain(&s);
    assert_eq!(hits(&s), 1);
    integrity(&s);
}

#[test]
fn o4_parallel_fts_writer_annotation_backup_and_passive_checkpoints() {
    let d = crate::fsops::TmpDir::new();
    let mut s = Store::open(&d.path().join("library")).unwrap();
    let mut ids = vec![];
    for n in 0..48 {
        ids.push(add(
            &mut s,
            &d.path().join(format!("sample{n}.md")),
            &format!("recoveryneedle {n} {}", "reference content ".repeat(500)),
        ));
    }
    s.search_control("enable").unwrap();
    let mut work = vec![];
    while let Some(w) = s.search_next().unwrap() {
        work.push(w);
    }
    assert_eq!(work.len(), 48);
    let root = s.root.clone();
    let gate = Arc::new(Barrier::new(2));
    let other = gate.clone();
    let writer = thread::spawn(move || {
        let detached = search::writer(&root).unwrap();
        other.wait();
        for w in work {
            assert!(detached.search_commit(&w, search::read(&w)).unwrap());
            thread::sleep(Duration::from_millis(2));
        }
    });
    gate.wait();
    for n in 0..24 {
        let version: i64 = s
            .conn
            .query_row(
                "SELECT note_version FROM files WHERE id=?",
                [&ids[0]],
                |r| r.get(0),
            )
            .unwrap();
        s.save_note(&json!({"id":ids[0],"version":version,"text":format!("front annotation {n}")}))
            .unwrap();
        for db in ["main", "search", "semantic"] {
            checkpoint(&s, db, "PASSIVE");
        }
        if n == 6 {
            s.export_backup(&d.path().join("concurrent.sqtagbackup"))
                .unwrap();
        }
        thread::sleep(Duration::from_millis(3));
    }
    writer.join().unwrap();
    for db in ["main", "search", "semantic"] {
        checkpoint(&s, db, "TRUNCATE");
    }
    assert_eq!(hits(&s), 48);
    assert_eq!(s.file(&ids[0]).unwrap().note, "front annotation 23");
    integrity(&s);
    let mut restored = Store::open(&d.path().join("backup-check")).unwrap();
    restored
        .restore_backup(&d.path().join("concurrent.sqtagbackup"))
        .unwrap();
    assert_eq!(restored.file(&ids[0]).unwrap().note, "front annotation 6");
    assert_eq!(
        restored
            .query(&Query {
                limit: 100,
                ..Default::default()
            })
            .unwrap()["total"],
        48
    );
    integrity(&restored);
}

#[test]
fn o4_semantic_writer_checkpoint_and_cancel_keeps_manual_annotations() {
    let d = crate::fsops::TmpDir::new();
    let mut s = Store::open(&d.path().join("library")).unwrap();
    let mut ids = vec![];
    for n in 0..24 {
        ids.push(add(
            &mut s,
            &d.path().join(format!("vector{n}.txt")),
            "recoveryneedle local synthetic content",
        ));
    }
    s.search_control("enable").unwrap();
    drain(&s);
    s.semantic_save(&json!({"config":{"enabled":true,"endpoint":"http://127.0.0.1:9999/v1","model":"fixture","maxRequests":100,"minSimilarity":0.35},"consent":true})).unwrap();
    let (_, works, config) = s.semantic_plan().unwrap();
    assert_eq!(works.len(), 24);
    let generation = s.semantic_begin().unwrap();
    let root = s.root.clone();
    let gate = Arc::new(Barrier::new(2));
    let other = gate.clone();
    let worker = thread::spawn(move || {
        let mut detached = search::writer(&root).unwrap();
        detached
            .conn
            .execute(
                "ATTACH DATABASE ? AS semantic",
                [root
                    .join("cache/semantic/index.sqlite")
                    .to_string_lossy()
                    .as_ref()],
            )
            .unwrap();
        other.wait();
        for w in works {
            assert!(detached
                .semantic_commit(
                    &w,
                    &config,
                    generation,
                    &vec![vec![1.0, 0.0]; w.samples.len()]
                )
                .unwrap());
            thread::sleep(Duration::from_millis(2));
        }
    });
    gate.wait();
    for n in 0..12 {
        let version: i64 = s
            .conn
            .query_row(
                "SELECT note_version FROM files WHERE id=?",
                [&ids[0]],
                |r| r.get(0),
            )
            .unwrap();
        s.save_note(
            &json!({"id":ids[0],"version":version,"text":format!("manual annotation {n}")}),
        )
        .unwrap();
        checkpoint(&s, "semantic", "PASSIVE");
        thread::sleep(Duration::from_millis(3));
    }
    worker.join().unwrap();
    checkpoint(&s, "semantic", "TRUNCATE");
    assert_eq!(s.semantic_status().unwrap()["files"], 24);
    let before = snapshot(&s, &ids[0]);
    let (_, works, c) = s.semantic_plan_mode(Some(&ids), "rebuild").unwrap();
    let g = s.semantic_begin().unwrap();
    s.semantic_clear().unwrap();
    assert!(!s
        .semantic_commit(
            &works[0],
            &c,
            g,
            &vec![vec![1.0, 0.0]; works[0].samples.len()]
        )
        .unwrap());
    assert_eq!(snapshot(&s, &ids[0]), before);
    assert_eq!(s.semantic_status().unwrap()["files"], 0);
    assert_eq!(hits(&s), 24);
    integrity(&s);
}

// Invoked in its own test process by the parent below. exit deliberately bypasses
// Store/Connection destructors; this checks process interruption, not power loss.
#[test]
fn o4_transaction_exit_child() {
    let Some(root) = std::env::var_os("TAGROVE_O4_CRASH_ROOT") else {
        return;
    };
    let s = Store::open(Path::new(&root)).unwrap();
    s.conn
        .execute_batch("PRAGMA wal_autocheckpoint=0; BEGIN IMMEDIATE;")
        .unwrap();
    s.conn
        .execute(
            "UPDATE files SET note=?,favorite=1",
            ["committed marker ".repeat(2048)],
        )
        .unwrap();
    if std::env::var("TAGROVE_O4_CRASH_PHASE").unwrap() == "after" {
        s.conn.execute_batch("COMMIT").unwrap();
    }
    std::process::exit(73);
}

#[test]
fn o4_process_exit_before_and_after_commit_recovers_atomic_state() {
    let d = crate::fsops::TmpDir::new();
    for phase in ["before", "after"] {
        let root = d.path().join(phase);
        let mut s = Store::open(&root).unwrap();
        let p = d.path().join(format!("{phase}.md"));
        let id = add(&mut s, &p, "original bytes");
        let before = snapshot(&s, &id);
        drop(s);
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "o4_transaction_exit_child",
                "--test-threads=1",
                "--nocapture",
            ])
            .env("TAGROVE_O4_CRASH_ROOT", &root)
            .env("TAGROVE_O4_CRASH_PHASE", phase)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(73));
        assert!(root.join("library.sqlite-wal").is_file());
        let s = Store::open(&root).unwrap();
        if phase == "before" {
            assert_eq!(snapshot(&s, &id), before);
        } else {
            let f = s.file(&id).unwrap();
            assert_eq!(f.note, "committed marker ".repeat(2048));
            assert!(f.favorite);
        }
        integrity(&s);
        assert_eq!(fs::read(&p).unwrap(), b"original bytes");
    }
}
