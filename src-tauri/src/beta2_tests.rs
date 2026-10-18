use crate::{
    db::Store,
    fsops,
    model::*,
    support,
    transfer::{self, Control},
};
use serde_json::json;
#[test]
fn native_fractional_palette_geometry_normalizes_without_weakening_settings_validation() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(&dir.path().join("library")).unwrap();
    // 349 physical pixels / 125% DPI is 279.2 logical pixels. Previously
    // floating::resize rounded this to 279 and failed before collapsing.
    assert!(s.save_settings(&json!({"key":"floatingSize","value":{"width":279,"height":460}})).is_err());
    for (w,h) in [(279.2,460.),(279.33,319.33),(900.4,1000.4)] {
        let (width,height)=crate::window_geometry::expanded_size(w,h);
        s.save_settings(&json!({"key":"floatingSize","value":{"width":width,"height":height}})).unwrap();
        let saved=s.bootstrap().unwrap()["settings"]["floatingSize"].clone();
        assert_eq!(saved,json!({"width":width,"height":height}));
    }
}
#[path = "o4_recovery_tests.rs"]
mod o4_recovery_tests;
use std::{
    fs,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};

#[test]
fn staged_transfer_keeps_queries_and_note_edits_responsive_and_rolls_back_cancel() {
    let root = tempfile::tempdir().unwrap();
    let files = root.path().join("files");
    fs::create_dir(&files).unwrap();
    let original = files.join("large.txt");
    fs::write(&original, vec![b'x'; 1024 * 1024]).unwrap();
    let mut sender = Store::open(&root.path().join("sender")).unwrap();
    sender
        .add_file_with_ai(&fsops::inspect(&original).unwrap(), false)
        .unwrap();
    let plan = sender.transfer_plan(&json!({})).unwrap();
    let fingerprint = plan.info["fingerprint"].as_str().unwrap().to_string();
    let archive = files.join("pack.sqtagpack");
    transfer::export(plan, &archive, &fingerprint, &Control::default()).unwrap();
    let fingerprint = transfer::preview(&archive, &Control::default()).unwrap()["fingerprint"]
        .as_str()
        .unwrap()
        .to_string();
    let mut receiver = Store::open(&root.path().join("receiver")).unwrap();
    let existing = files.join("existing.txt");
    fs::write(&existing, b"existing").unwrap();
    receiver
        .add_file_with_ai(&fsops::inspect(&existing).unwrap(), false)
        .unwrap();
    let fid = receiver.query(&Query::default()).unwrap()["files"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let store = Arc::new(Mutex::new(receiver));
    let control = Arc::new(Control::default());
    let (paused_tx, paused_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let once = AtomicBool::new(false);
    let weak = Arc::downgrade(&control);
    control.set_chunk_hook(move || {
        if weak.upgrade().unwrap().status()["phase"] == "复制文件"
            && !once.swap(true, Ordering::SeqCst)
        {
            paused_tx.send(()).unwrap();
            resume_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        }
    });
    let worker_store = store.clone();
    let worker_control = control.clone();
    let destination = files.clone();
    let worker = std::thread::spawn(move || {
        worker_control.run_named("package.import", || {
            transfer::import_shared(
                &worker_store,
                &archive,
                &destination,
                &fingerprint,
                &worker_control,
            )
        })
    });
    paused_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    {
        let mut library = store
            .try_lock()
            .expect("staging must not hold the Store lock");
        assert_eq!(library.query(&Query::default()).unwrap()["total"], 1);
        let record = library.file(&fid).unwrap();
        library
            .save_note(
                &json!({"id":fid,"text":"edited during transfer","version":record.note_version}),
            )
            .unwrap();
    }
    control.cancel.store(true, Ordering::SeqCst);
    resume_tx.send(()).unwrap();
    assert!(worker
        .join()
        .unwrap()
        .unwrap_err()
        .starts_with("TRANSFER_CANCELLED:"));
    let library = store.lock().unwrap();
    assert_eq!(library.file(&fid).unwrap().note, "edited during transfer");
    assert_eq!(library.query(&Query::default()).unwrap()["total"], 1);
    assert!(!library.root.join("transfer-state.json").exists());
    assert_eq!(fs::read_dir(&files).unwrap().count(), 3);
    assert_eq!(control.status()["history"][0]["state"], "cancelled");
}

#[test]
fn transfer_task_history_is_bounded_and_failure_and_cancel_are_distinct() {
    let control = Control::default();
    for _ in 0..14 {
        control
            .run_named("package.inspect", || Ok(json!({"fileCount":1})))
            .unwrap();
    }
    control
        .run_named::<serde_json::Value>("package.import", || {
            Err(err("TRANSFER_CANCELLED", "cancelled"))
        })
        .unwrap_err();
    control
        .run_named::<serde_json::Value>("package.import", || {
            Err(err("CHECKSUM_MISMATCH", "private filename"))
        })
        .unwrap_err();
    let status = control.status();
    assert_eq!(status["busy"], false);
    assert_eq!(status["history"].as_array().unwrap().len(), 12);
    assert_eq!(status["history"][0]["state"], "failed");
    assert_eq!(status["history"][1]["state"], "cancelled");
}

#[test]
fn diagnostic_allowlist_omits_all_private_text_and_export_does_not_overwrite() {
    let snapshot = json!({"schema":2,"files":3,"tags":4,"unavailable":1,"notice":"PRIVATE_PATH","lastBackup":{"path":"PRIVATE_PATH"},"ai":{"failed":2},"settings":{"key":"SECRET","endpoint":"PRIVATE_ENDPOINT"},"note":"PRIVATE_NOTE","name":"PRIVATE_NAME"});
    let tasks = json!({"history":[{"kind":"package.import","state":"failed","error":"PRIVATE_ERROR","result":{"path":"PRIVATE_PATH"}}]});
    let report = support::diagnostic_report(&snapshot, &tasks);
    assert!(!report.contains("PRIVATE_"));
    assert!(!report.contains("SECRET"));
    assert!(report.contains("导入：失败"));
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("report.md");
    support::export_report(&path, &report).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), report);
    assert!(support::export_report(&path, "replacement").is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), report);
}

#[test]
fn recovery_notice_survives_restart_and_backup_status_is_local() {
    let root = tempfile::tempdir().unwrap();
    let library_path = root.path().join("library");
    let mut library = Store::open(&library_path).unwrap();
    library.recovery_notice = "synthetic interrupted task".into();
    library.remember_recovery().unwrap();
    let archive = root.path().join("backup.sqtagbackup");
    library.export_backup(&archive).unwrap();
    assert!(
        library.support_snapshot().unwrap()["lastBackup"]["createdAt"]
            .as_i64()
            .unwrap()
            > 0
    );
    drop(library);
    let mut library = Store::open(&library_path).unwrap();
    assert_eq!(library.recovery_notice, "synthetic interrupted task");
    library.restore_backup(&archive).unwrap();
    assert_eq!(
        library.support_snapshot().unwrap()["lastBackup"]["kind"],
        "beforeRestore"
    );
    assert_eq!(
        library
            .conn
            .query_row(
                "SELECT COUNT(*) FROM settings WHERE key='recoveryNotice'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn retry_only_failed_ai_preserves_completed_and_cancelled_tasks() {
    let root = tempfile::tempdir().unwrap();
    let mut library = Store::open(&root.path().join("library")).unwrap();
    library.conn.execute("INSERT INTO settings(key,value) VALUES('ai',?)", [json!({"enabled":true,"endpoint":"https://example.test/v1","model":"synthetic","allowNewTags":true}).to_string()]).unwrap();
    for (i, state) in [
        "failed",
        "done",
        "cancelled",
        "unsupported",
        "running",
        "queued",
    ]
    .iter()
    .enumerate()
    {
        let path = root.path().join(format!("{i}.txt"));
        fs::write(&path, b"synthetic").unwrap();
        library
            .add_file_with_ai(&fsops::inspect(&path).unwrap(), false)
            .unwrap();
        library.conn.execute("INSERT INTO ai_jobs(file_id,status,error,updated_at) SELECT id,?,'synthetic',1 FROM files WHERE name=?", rusqlite::params![state,format!("{i}.txt")]).unwrap();
    }
    assert_eq!(library.retry_failed_ai().unwrap()["queued"], 1);
    let summary = library.ai_summary().unwrap();
    assert_eq!(summary["failed"], 0);
    assert_eq!(summary["queued"], 2);
    assert_eq!(summary["done"], 1);
    assert_eq!(summary["cancelled"], 1);
}

#[test]
fn floating_position_is_local_validated_and_removed_from_transport_backup() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open(&root.path().join("library")).unwrap();
    store
        .save_settings(&json!({"key":"floatingPosition","value":{"x":-1200,"y":80}}))
        .unwrap();
    for value in [
        json!({"x":2147483648_i64,"y":0}),
        json!({"x":0.5,"y":0}),
        json!({"x":0}),
    ] {
        assert!(store
            .save_settings(&json!({"key":"floatingPosition","value":value}))
            .is_err());
    }
    drop(store);
    let store = Store::open(&root.path().join("library")).unwrap();
    assert_eq!(
        store.bootstrap().unwrap()["settings"]["floatingPosition"]["x"],
        -1200
    );
    let backup = root.path().join("position.sqtagbackup");
    store.export_backup(&backup).unwrap();
    let inspected = crate::backup::inspect_backup(&backup, &store.root).unwrap();
    let count: i64 = inspected
        .conn
        .query_row(
            "SELECT COUNT(*) FROM settings WHERE key='floatingPosition'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn column_controls_preference_is_boolean_and_persists() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("library");
    let mut store = Store::open(&path).unwrap();
    assert!(store.bootstrap().unwrap()["settings"]["showColumnControls"].is_null());
    for value in [true, false] {
        store
            .save_settings(&json!({"key":"showColumnControls","value":value}))
            .unwrap();
        assert_eq!(
            store.bootstrap().unwrap()["settings"]["showColumnControls"],
            value
        );
    }
    for value in [json!("true"), json!(1), json!(null), json!([])] {
        assert!(store
            .save_settings(&json!({"key":"showColumnControls","value":value}))
            .is_err());
        assert_eq!(
            store.bootstrap().unwrap()["settings"]["showColumnControls"],
            false
        );
    }
    store
        .save_settings(&json!({"key":"showColumnControls","value":true}))
        .unwrap();
    drop(store);
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.bootstrap().unwrap()["settings"]["showColumnControls"],
        true
    );
}

#[test]
fn appearance_preferences_validate_and_survive_reopen() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("library");
    let mut store = Store::open(&path).unwrap();
    for color in [
        "green",
        "blue",
        "purple",
        "rose",
        "amber",
        "teal",
        "terracotta",
        "indigo",
        "vermilion",
        "moss",
        "cocoa",
        "graphite",
    ] {
        store
            .save_settings(&json!({"key":"accentColor","value":color}))
            .unwrap();
        assert_eq!(store.bootstrap().unwrap()["settings"]["accentColor"], color);
    }
    for language in ["zh-CN", "en"] {
        store
            .save_settings(&json!({"key":"language","value":language}))
            .unwrap();
    }
    for size in 12..=16 {
        store
            .save_settings(&json!({"key":"uiFontSize","value":size}))
            .unwrap();
    }
    for (key, value) in [
        ("uiFontSize", json!(11)),
        ("uiFontSize", json!(17)),
        ("uiFontSize", json!(13.5)),
        ("uiFontSize", json!("14")),
        ("uiFontSize", json!(null)),
        ("accentColor", json!("#ff0000")),
        ("language", json!("fr")),
        ("language", json!(true)),
        ("accentColor", json!(null)),
    ] {
        assert!(store
            .save_settings(&json!({"key":key,"value":value}))
            .is_err());
    }
    drop(store);
    let store = Store::open(&path).unwrap();
    let settings = store.bootstrap().unwrap()["settings"].clone();
    assert_eq!(settings["language"], "en");
    assert_eq!(settings["accentColor"], "graphite");
    assert_eq!(settings["uiFontSize"], 16);
}

#[test]
fn legacy_backup_restores_reading_without_restart_and_never_overwrites_backup() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = Store::open(&dir.path().join("library")).unwrap();
    let original = dir.path().join("sample.md");
    fs::write(&original, "# original").unwrap();
    s.add_file_with_ai(&fsops::inspect(&original).unwrap(), false)
        .unwrap();
    let f = s
        .query(&Query {
            limit: 1,
            ..Default::default()
        })
        .unwrap()["files"][0]
        .clone();
    s.conn.execute_batch("DROP TABLE reader_states").unwrap();
    let archive = dir.path().join("legacy.sqtagbackup");
    s.export_backup(&archive).unwrap();
    let bytes = fs::read(&archive).unwrap();
    assert!(s.export_backup(&archive).is_err());
    assert_eq!(fs::read(&archive).unwrap(), bytes);
    let result = s.restore_backup(&archive).unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(
        s.reader_state(&json!({"id":f["id"]})).unwrap()["value"],
        json!({})
    );
    s.save_reader_state(&json!({"id":f["id"],"revision":f["revision"],"identity":f["identity"],"value":{"flowZoom":125}})).unwrap();
    assert_eq!(
        s.reader_state(&json!({"id":f["id"]})).unwrap()["value"]["flowZoom"],
        125
    );
    assert_eq!(fs::read_to_string(original).unwrap(), "# original");
    assert!(std::path::Path::new(result["recoveryBackup"].as_str().unwrap()).is_file());
}

#[test]
fn interrupted_restore_revokes_uploads_and_finishes_before_marker_removal() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("library");
    let mut s = Store::open(&root).unwrap();
    let p = dir.path().join("private.md");
    fs::write(&p, "private body").unwrap();
    s.add_file_with_ai(&fsops::inspect(&p).unwrap(), false)
        .unwrap();
    s.conn.execute("INSERT INTO settings(key,value) VALUES('ai',?)",[json!({"enabled":true,"endpoint":"https://example.test/v1","model":"fixture","allowNewTags":true}).to_string()]).unwrap();
    let id: String = s
        .conn
        .query_row("SELECT id FROM files", [], |r| r.get(0))
        .unwrap();
    s.conn.execute("INSERT OR REPLACE INTO ai_jobs(file_id,status,error,updated_at) VALUES(?,'running','',1)",[id]).unwrap();
    s.search_control("enable").unwrap();
    fs::write(
        root.join("restore-state.toml"),
        "recovery_backup='retained'",
    )
    .unwrap();
    drop(s);
    let s = Store::open(&root).unwrap();
    assert!(!s.ai_config().unwrap().enabled);
    assert!(!s.semantic_config().unwrap().enabled);
    assert!(!s.search_enabled().unwrap());
    let status: String = s
        .conn
        .query_row("SELECT status FROM ai_jobs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "cancelled");
    assert!(!root.join("restore-state.toml").exists());
    assert!(!s.recovery_notice.is_empty());
    assert_eq!(fs::read_to_string(p).unwrap(), "private body");
    s.validate().unwrap();
}

#[test]
fn extended_diagnostics_are_numeric_allowlisted_and_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("library")).unwrap();
    let before: i64 = s
        .conn
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .unwrap();
    let snapshot = s.diagnostic_snapshot().unwrap();
    assert!(snapshot["storage"]["databaseBytes"].as_u64().unwrap() > 0);
    assert_eq!(snapshot["derived"]["readingStates"], 0);
    let mut injected = snapshot.clone();
    injected["derived"]["textState"] = json!("PRIVATE_ERROR");
    injected["storage"]["databaseBytes"] = json!("PRIVATE_PATH");
    injected["sqlite"]["version"] = json!("PRIVATE_ENDPOINT");
    injected["sqlite"]["sourceId"] = json!("PRIVATE_PATH");
    let report = support::diagnostic_report(&injected, &json!({}));
    assert!(!report.contains("PRIVATE_"));
    assert!(report.contains("存储占用"));
    let after: i64 = s
        .conn
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, after);
}

#[test]
fn sqlite_runtime_probe_is_read_only_and_capabilities_really_work() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("library")).unwrap();
    let before: i64 = s
        .conn
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .unwrap();
    let runtime = s.sqlite_diagnostics().unwrap();
    let expected: String = s
        .conn
        .query_row("SELECT sqlite_version()", [], |r| r.get(0))
        .unwrap();
    assert_eq!(runtime["version"], expected);
    assert_eq!(runtime["capabilities"]["fts5"], true);
    assert_eq!(runtime["capabilities"]["json"], true);
    assert_eq!(runtime["capabilities"]["omitBackup"], false);
    assert_eq!(runtime["capabilities"]["omitWal"], false);
    assert_eq!(runtime["journalModes"]["main"], "wal");
    assert_eq!(runtime["journalModes"]["textIndex"], "wal");
    assert_eq!(runtime["journalModes"]["semanticIndex"], "wal");
    let report = support::diagnostic_report(&json!({"sqlite":runtime.clone()}), &json!({}));
    assert!(report.contains(runtime["version"].as_str().unwrap()));
    assert!(report.contains(runtime["sourceId"].as_str().unwrap()));
    let after: i64 = s
        .conn
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, after);
    // Exercise trigram FTS5, JSON and the enabled Rust backup API in separate memory DBs.
    let source = rusqlite::Connection::open_in_memory().unwrap();
    source.execute_batch("CREATE VIRTUAL TABLE probe USING fts5(body,tokenize='trigram'); INSERT INTO probe VALUES('courtyard facade');").unwrap();
    let hits: i64 = source
        .query_row(
            "SELECT count(*) FROM probe WHERE probe MATCH 'courtyard'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hits, 1);
    let mut target = rusqlite::Connection::open_in_memory().unwrap();
    {
        let backup = rusqlite::backup::Backup::new(&source, &mut target).unwrap();
        backup
            .run_to_completion(16, std::time::Duration::from_millis(1), None)
            .unwrap();
    }
    let copied: i64 = target
        .query_row(
            "SELECT count(*) FROM probe WHERE probe MATCH 'courtyard'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(copied, 1);
    println!("O4_SQLITE_RUNTIME {}", runtime);
}

#[test]
fn tag_drop_tilt_preference_is_boolean_and_survives_restart() {
    let root = tempfile::tempdir().unwrap();
    let mut store = Store::open(root.path()).unwrap();
    for value in [true, false] {
        store.save_settings(&json!({"key":"tagDropTilt","value":value})).unwrap();
        assert_eq!(store.bootstrap().unwrap()["settings"]["tagDropTilt"], value);
    }
    for value in [json!("false"), json!(1), json!(null)] {
        assert!(store.save_settings(&json!({"key":"tagDropTilt","value":value})).is_err());
    }
    drop(store);
    let store = Store::open(root.path()).unwrap();
    assert_eq!(store.bootstrap().unwrap()["settings"]["tagDropTilt"], false);
}
