mod ai;
mod ai_review;
#[cfg(test)]
mod ai_review_tests;
mod ai_worker;
mod annotation;
#[cfg(test)]
mod auto_tag_tests;
mod auto_tags;
mod backup;
#[cfg(test)]
mod beta2_tests;
mod cache;
mod capture;
mod capture_options;
mod web_reading;
mod web_browser;
pub mod capture_bridge;
pub mod capture_browser;
mod db;
mod floating;
mod fsops;
pub mod image_capture;
mod model;
mod natural_filter;
mod office;
mod office_layout;
mod path_repair;
mod pdf_index;
#[cfg(test)]
mod performance_tests;
mod reader_state;
mod search;
mod search_quality;
mod semantic;
mod shell_target;
mod support;
mod tag_aliases;
mod tag_export;
mod tag_groups;
mod tag_organize;
#[cfg(test)]
mod tests;
mod text_document;
mod transfer;
#[cfg(test)]
mod transfer_tests;
mod watch;
mod window_geometry;

use crate::{db::Store, model::*};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

#[derive(Clone)]
pub struct AppState {
    store: Arc<Mutex<Store>>,
    jobs: Arc<Mutex<HashMap<String, (ImportJob, Arc<AtomicBool>)>>>,
    drag_active: Arc<AtomicBool>,
    drag_cancel: Arc<AtomicBool>,
    ai_generation: Arc<AtomicU64>,
    transfer: Arc<transfer::Control>,
    repair: Arc<path_repair::Control>,
    watch: Arc<watch::Control>,
    refreshing: Arc<AtomicBool>,
    semantic: Arc<semantic::Control>,
}
impl AppState {
    fn new(root: &Path) -> Result<Self> {
        Ok(Self {
            store: Arc::new(Mutex::new(Store::open(root)?)),
            jobs: Arc::new(Mutex::new(HashMap::new())),
            drag_active: Arc::new(AtomicBool::new(false)),
            drag_cancel: Arc::new(AtomicBool::new(false)),
            ai_generation: Arc::new(AtomicU64::new(0)),
            transfer: Arc::new(transfer::Control::default()),
            repair: Arc::new(path_repair::Control::default()),
            watch: Arc::new(watch::Control::default()),
            refreshing: Arc::new(AtomicBool::new(false)),
            semantic: Arc::new(semantic::Control::default()),
        })
    }
    fn store(&self) -> Result<std::sync::MutexGuard<'_, Store>> {
        self.store
            .lock()
            .map_err(|_| err("INTERNAL_ERROR", "资料库锁异常，请重启"))
    }
    fn blocks_exit(&self) -> bool {
        self.repair.busy.load(Ordering::SeqCst)
            || self.transfer.busy.load(Ordering::SeqCst)
            || self
                .jobs
                .lock()
                .map(|jobs| jobs.values().any(|(job, _)| !job.done))
                .unwrap_or(true)
    }
    fn start_import(&self, app: tauri::AppHandle, v: &Value) -> Result<Value> {
        if self.transfer.busy.load(Ordering::SeqCst) {
            return Err(err("TRANSFER_BUSY", "请等待当前资料包或恢复任务完成"));
        }
        let paths = string_list(v, "paths");
        if paths.is_empty() {
            return Err(err("INVALID_INPUT", "请选择文件或文件夹"));
        }
        let recursive = v["recursive"].as_bool().unwrap_or(true);
        let jid = id();
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut jobs = self
                .jobs
                .lock()
                .map_err(|_| err("INTERNAL_ERROR", "任务锁异常"))?;
            if jobs.values().any(|(j, _)| !j.done) {
                return Err(err("IMPORT_BUSY", "已有加入任务正在进行"));
            }
            jobs.clear();
            jobs.insert(
                jid.clone(),
                (
                    ImportJob {
                        id: jid.clone(),
                        ..Default::default()
                    },
                    cancel.clone(),
                ),
            );
        }
        let state = self.clone();
        let thread_id = jid.clone();
        std::thread::spawn(move || {
            let mut progress = ImportJob {
                id: thread_id.clone(),
                ..Default::default()
            };
            let root = state.store().map(|s| s.root.clone()).unwrap_or_default();
            let mut stack: Vec<(PathBuf, usize)> =
                paths.into_iter().map(|p| (PathBuf::from(p), 0)).collect();
            let mut visited = std::collections::HashSet::new();
            while let Some((path, depth)) = stack.pop() {
                if cancel.load(Ordering::Relaxed) {
                    progress.cancelled = true;
                    break;
                }
                let outcome = (|| -> Result<()> {
                    let meta =
                        std::fs::symlink_metadata(&path).map_err(|e| err("ACCESS_DENIED", e))?;
                    if fsops::is_link(&meta) || fsops::is_hidden(&meta) {
                        progress.skipped += 1;
                        return Ok(());
                    }
                    let canonical = fsops::canonical_display(&path)?;
                    if Path::new(&canonical).starts_with(&root) {
                        progress.skipped += 1;
                        return Ok(());
                    }
                    if !visited.insert(canonical.clone()) {
                        progress.skipped += 1;
                        return Ok(());
                    }
                    if meta.is_dir() {
                        if depth > 0 && !recursive {
                            progress.skipped += 1;
                            return Ok(());
                        }
                        for entry in
                            std::fs::read_dir(&canonical).map_err(|e| err("ACCESS_DENIED", e))?
                        {
                            match entry {
                                Ok(e) => stack.push((e.path(), depth + 1)),
                                Err(e) => {
                                    progress.failed += 1;
                                    if progress.errors.len() < 100 {
                                        progress.errors.push(format!("{}：{e}", path.display()));
                                        progress.failed_paths.push(canonical.clone());
                                    }
                                }
                            }
                        }
                    } else if meta.is_file() {
                        progress.discovered += 1;
                        let m = fsops::inspect(Path::new(&canonical))?;
                        let mut import_store = state.store()?;
                        let added = import_store.add_file(&m)?;
                        if added {
                            progress.added += 1;
                            {
                                let fid: String = import_store
                                    .conn
                                    .query_row(
                                        "SELECT id FROM files WHERE path=?",
                                        [&m.path],
                                        |r| r.get(0),
                                    )
                                    .map_err(sql_err)?;
                                import_store.semantic_enqueue(&fid, &thread_id)?;
                                if progress.added_ids.len() < semantic::MAX_FILES {
                                    progress.added_ids.push(fid);
                                }
                            }
                        } else {
                            progress.existing += 1;
                        }
                        progress.processed += 1;
                    } else {
                        progress.skipped += 1;
                    }
                    Ok(())
                })();
                if let Err(e) = outcome {
                    progress.failed += 1;
                    progress.processed += 1;
                    if progress.errors.len() < 100 {
                        progress.errors.push(format!("{}：{e}", path.display()));
                        progress.failed_paths.push(path.to_string_lossy().into());
                    }
                }
                if let Ok(mut jobs) = state.jobs.lock() {
                    if let Some((j, _)) = jobs.get_mut(&thread_id) {
                        *j = progress.clone();
                    }
                }
                if progress.processed % 50 == 0 {
                    let _ = app.emit("import-progress", &progress);
                }
            }
            progress.done = true;
            if let Ok(mut jobs) = state.jobs.lock() {
                if let Some((j, _)) = jobs.get_mut(&thread_id) {
                    *j = progress.clone();
                }
            }
            let _ = app.emit("import-progress", &progress);
            let _ = app.emit("library-changed", ());
        });
        Ok(json!({"id":jid}))
    }
    fn refresh_all(&self, app: tauri::AppHandle) -> Result<Value> {
        if self.refreshing.swap(true, Ordering::SeqCst) {
            return Ok(json!({"busy":true}));
        }
        let paths = match self.store().and_then(|s| s.all_paths()) {
            Ok(paths) => paths,
            Err(e) => {
                self.refreshing.store(false, Ordering::SeqCst);
                return Err(e);
            }
        };
        let state = self.clone();
        std::thread::spawn(move || {
            let mut changed = false;
            let mut last = std::time::Instant::now();
            for (i, (fid, _)) in paths.iter().enumerate() {
                if state.watch.stopped.load(Ordering::SeqCst) {
                    break;
                }
                if let Ok(mut s) = state.store() {
                    changed |= s.refresh_metadata(fid).unwrap_or(false);
                }
                if changed && last.elapsed() >= std::time::Duration::from_secs(2) {
                    let _ = app.emit("library-changed", ());
                    changed = false;
                    last = std::time::Instant::now();
                }
                if i % 32 == 31 {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
            }
            state.refreshing.store(false, Ordering::SeqCst);
            if changed {
                let _ = app.emit("library-changed", ());
            }
        });
        Ok(json_ok())
    }
}

fn start_index(state: AppState, app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let writer = match state.store().and_then(|s| search::writer(&s.root)) {
            Ok(w) => w,
            Err(_) => return,
        };
        let mut last_event = std::time::Instant::now();
        let mut dirty = false;
        let mut probe_time = std::time::Instant::now();
        let mut probe_cursor = String::new();
        while !state.watch.stopped.load(Ordering::SeqCst) {
            if probe_time.elapsed().as_secs() >= 5 && !state.blocks_exit() {
                let files = state
                    .store()
                    .and_then(|s| {
                        if s.search_enabled()? {
                            s.search_probe(&probe_cursor)
                        } else {
                            Ok(vec![])
                        }
                    })
                    .unwrap_or_default();
                probe_cursor = files.last().map(|f| f.id.clone()).unwrap_or_default();
                for f in files {
                    let changed = crate::fsops::inspect(Path::new(&f.path))
                        .map(|m| {
                            m.revision != f.revision
                                || m.identity != f.identity
                                || f.status != "available"
                        })
                        .unwrap_or(f.status == "available");
                    if changed {
                        dirty |= state
                            .store()
                            .and_then(|mut s| s.refresh_metadata(&f.id))
                            .unwrap_or(false);
                    }
                }
                probe_time = std::time::Instant::now();
            }
            let work = if state.blocks_exit() {
                None
            } else {
                state.store().and_then(|s| s.search_next()).ok().flatten()
            };
            if let Some(work) = work {
                let text =
                    search::read_controlled(&work, || writer.search_current(work.generation));
                let committed = match writer.search_commit(&work, text) {
                    Ok(done) => done,
                    Err(e) => {
                        if let Ok(s) = state.store() {
                            let _=s.conn.execute("UPDATE search.docs SET state='failed',error=?,updated_at=? WHERE id=? AND state='running' AND EXISTS(SELECT 1 FROM search.control WHERE generation=?)",rusqlite::params![e,now(),work.file.id,work.generation]);
                        }
                        true
                    }
                };
                dirty |= committed;
                if dirty && last_event.elapsed().as_secs() >= 2 {
                    let _ = app.emit("library-changed", ());
                    last_event = std::time::Instant::now();
                    dirty = false;
                }
                std::thread::sleep(std::time::Duration::from_millis(80));
            } else {
                if dirty {
                    let _ = app.emit("library-changed", ());
                    last_event = std::time::Instant::now();
                    dirty = false;
                }
                std::thread::sleep(std::time::Duration::from_millis(1500));
            }
        }
    });
}

fn start_watch(state: AppState, app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut runtimes: HashMap<String, watch::Runtime> = HashMap::new();
        let mut previous_epoch = 0;
        let mut last = 0;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(250));
            if state.watch.stopped.load(Ordering::SeqCst) {
                return;
            }
            let forced = state.watch.force.swap(false, Ordering::SeqCst);
            if !forced && now() - last < 5000 {
                continue;
            }
            let epoch = state.watch.generation.load(Ordering::SeqCst);
            if epoch != previous_epoch || forced {
                for rt in runtimes.values_mut() {
                    rt.reset();
                    rt.status.errors.clear();
                }
                previous_epoch = epoch;
            }
            let config = state.store().and_then(|s| watch::rules(&s));
            let (rules, config_version) = match config {
                Ok(v) => v,
                Err(_) => {
                    last = now();
                    continue;
                }
            };
            runtimes.retain(|id, _| rules.iter().any(|r| &r.id == id));
            let mut library_changed = false;
            for rule in rules {
                if state.watch.cancelled(epoch) {
                    break;
                }
                if runtimes.get(&rule.id).is_none_or(|rt| rt.rule != rule) {
                    if let Some(rt) = runtimes.get_mut(&rule.id) {
                        rt.rule = rule.clone();
                        rt.reset();
                        rt.status.errors.clear();
                        rt.status.missing = 0;
                    } else {
                        runtimes.insert(rule.id.clone(), watch::Runtime::new(rule.clone()));
                    }
                }
                let rt = runtimes.get_mut(&rule.id).unwrap();
                if rule.paused {
                    rt.reset();
                    rt.status.state = "paused".into();
                    state.watch.publish(rt.status.clone());
                    continue;
                }
                if state.blocks_exit() {
                    rt.status.state = "deferred".into();
                    state.watch.publish(rt.status.clone());
                    continue;
                }
                rt.status.state = "scanning".into();
                state.watch.publish(rt.status.clone());
                let app_root = match state.store() {
                    Ok(s) => s.root.clone(),
                    Err(_) => continue,
                };
                let result = watch::scan(&rule, &app_root, || state.watch.cancelled(epoch));
                if state.watch.cancelled(epoch) {
                    break;
                }
                let known = match state.store().and_then(|s| watch::known(&s, &rule)) {
                    Ok(k) => k,
                    Err(e) => {
                        rt.status.state = "error".into();
                        rt.status.errors = vec![e];
                        state.watch.publish(rt.status.clone());
                        continue;
                    }
                };
                let scan = match result {
                    Ok(scan) => scan,
                    Err(e) => {
                        rt.reset();
                        rt.status.state = if e.starts_with("WATCH_OFFLINE:") {
                            "offline"
                        } else {
                            "error"
                        }
                        .into();
                        rt.status.errors = vec![e.clone()];
                        rt.status.last_scan = now();
                        if e.starts_with("WATCH_OFFLINE:") {
                            for k in known
                                .iter()
                                .filter(|k| {
                                    !k.removed
                                        && !matches!(k.status.as_str(), "missing" | "offline")
                                })
                                .take(64)
                            {
                                let Ok(_gate) = state.watch.gate.lock() else {
                                    break;
                                };
                                if state.watch.cancelled(epoch) || state.blocks_exit() {
                                    break;
                                }
                                if let Ok(mut s) = state.store() {
                                    if watch::rules(&s).is_ok_and(|(_, v)| v == config_version) {
                                        library_changed |=
                                            watch::refresh_missing(&mut s, k).unwrap_or(false);
                                    }
                                }
                            }
                        }
                        state.watch.publish(rt.status.clone());
                        continue;
                    }
                };
                let ready = rt.observe(&scan, &known, now());
                let mut deferred = false;
                for entry in ready {
                    {
                        let Ok(_gate) = state.watch.gate.lock() else {
                            break;
                        };
                        if state.watch.cancelled(epoch) || state.blocks_exit() {
                            deferred = true;
                            break;
                        }
                        let Ok(mut s) = state.store() else { break };
                        if !watch::rules(&s).is_ok_and(|(_, v)| v == config_version) {
                            break;
                        }
                        match watch::apply_file(&mut s, &rule, &entry) {
                            Ok(applied) => {
                                match applied {
                                    watch::Applied::Added => {
                                        rt.status.added += 1;
                                        library_changed = true;
                                    }
                                    watch::Applied::Moved => {
                                        rt.status.moved += 1;
                                        library_changed = true;
                                    }
                                    watch::Applied::Changed => {
                                        rt.status.changed += 1;
                                        library_changed = true;
                                    }
                                    watch::Applied::Skipped => rt.status.skipped += 1,
                                    watch::Applied::Unchanged => (),
                                }
                                rt.done(&entry.path);
                            }
                            Err(e) => {
                                library_changed |= e.starts_with("FILE_REPLACED:");
                                if !e.starts_with("FILE_CHANGED:") && !rt.status.errors.contains(&e)
                                {
                                    rt.status.errors.push(e.clone());
                                    if rt.status.errors.len() > 5 {
                                        rt.status.errors.remove(0);
                                    }
                                }
                                if e.starts_with("WATCH_NEEDS_REPAIR:")
                                    || e.starts_with("WATCH_AMBIGUOUS:")
                                    || e.starts_with("FILE_REPLACED:")
                                {
                                    rt.done(&entry.path);
                                }
                            }
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                if !scan.incomplete && !state.watch.cancelled(epoch) {
                    let known = state
                        .store()
                        .and_then(|s| watch::known(&s, &rule))
                        .unwrap_or(known);
                    let present: std::collections::HashSet<_> =
                        scan.entries.iter().map(|e| &e.path).collect();
                    let missing: Vec<_> = known
                        .iter()
                        .filter(|k| !k.removed && !present.contains(&k.path))
                        .collect();
                    rt.status.missing = missing.len();
                    for i in 0..missing.len().min(64) {
                        let k = missing[(rt.missing_cursor + i) % missing.len()];
                        if k.status == "missing" {
                            continue;
                        }
                        let Ok(_gate) = state.watch.gate.lock() else {
                            break;
                        };
                        if state.watch.cancelled(epoch) || state.blocks_exit() {
                            deferred = true;
                            break;
                        }
                        if let Ok(mut s) = state.store() {
                            if watch::rules(&s).is_ok_and(|(_, v)| v == config_version) {
                                library_changed |=
                                    watch::refresh_missing(&mut s, k).unwrap_or(false);
                            }
                        }
                    }
                    if !missing.is_empty() {
                        rt.missing_cursor = (rt.missing_cursor + 64) % missing.len();
                    }
                }
                if scan.incomplete
                    && !rt
                        .status
                        .errors
                        .iter()
                        .any(|e| e == "部分目录不可读、隐藏或超深度，未据此判定文件缺失")
                {
                    rt.status
                        .errors
                        .push("部分目录不可读、隐藏或超深度，未据此判定文件缺失".into());
                }
                rt.status.state = if deferred {
                    "deferred"
                } else if !rt.status.errors.is_empty() {
                    "warning"
                } else if rt.status.pending > 0 {
                    "waiting"
                } else {
                    "watching"
                }
                .into();
                rt.status.last_scan = now();
                state.watch.publish(rt.status.clone());
            }
            if library_changed {
                let _ = app.emit("library-changed", ());
            }
            last = now();
        }
    });
}

fn semantic_selection(v: &Value) -> Result<Option<Vec<String>>> {
    let Some(raw) = v.get("ids") else {
        return Ok(None);
    };
    let mut ids: Vec<String> = serde_json::from_value(raw.clone())
        .map_err(|_| err("INVALID_INPUT", "语义文件范围无效"))?;
    if ids.is_empty()
        || ids.len() > semantic::MAX_FILES
        || ids.iter().any(|id| id.is_empty() || id.len() > 200)
    {
        return Err(err("INVALID_INPUT", "语义文件范围为 1～500 个文件"));
    }
    ids.sort();
    ids.dedup();
    Ok(Some(ids))
}
fn semantic_build(state: &AppState, app: &tauri::AppHandle, v: &Value) -> Result<Value> {
    let (preview, work, config, generation, key, epoch) = {
        let store = state.store()?;
        let ids = semantic_selection(v)?;
        let (p, w, c) =
            store.semantic_plan_mode(ids.as_deref(), v["mode"].as_str().unwrap_or("update"))?;
        if p["fingerprint"].as_str() != v["fingerprint"].as_str() {
            return Err(err("SEMANTIC_STALE", "发送范围已变化，请重新预览"));
        }
        if state.semantic.busy.swap(true, Ordering::SeqCst) {
            return Err(err("SEMANTIC_BUSY", "请等待当前请求结束"));
        }
        let init = (|| {
            let k = ai::read_secret(&store.root, "semantic-key.dpapi")?;
            let g = store.semantic_begin()?;
            for item in &w {
                store.semantic_enqueue(&item.id, &format!("task-{g}"))?;
                store.semantic_queue_state(&item.id, "waiting", "")?;
            }
            Ok((g, k))
        })();
        let (g, k) = match init {
            Ok(x) => x,
            Err(e) => {
                state.semantic.busy.store(false, Ordering::SeqCst);
                return Err(e);
            }
        };
        state.semantic.epoch.fetch_add(1, Ordering::SeqCst);
        (p, w, c, g, k, state.semantic.epoch.load(Ordering::SeqCst))
    };
    let state = state.clone();
    let app = app.clone();
    std::thread::spawn(move || {
        let mut stopped = false;
        let mut error = String::new();
        for w in work {
            if state.semantic.epoch.load(Ordering::SeqCst) != epoch
                || state.watch.stopped.load(Ordering::SeqCst)
            {
                stopped = true;
                break;
            }
            let ready = (|| {
                let s = state.store()?;
                if !s.semantic_file_current(&w.id)? {
                    return Err(err(
                        "SEMANTIC_STALE",
                        "源文件已变化，请刷新状态并等待正文重新索引",
                    ));
                }
                s.semantic_queue_state(&w.id, "running", "")?;
                if w.reuse.is_none() {
                    s.conn.execute("UPDATE semantic.control SET requested=requested+1 WHERE id=1 AND generation=?",[generation]).map_err(sql_err)?;
                }
                Ok(())
            })();
            if let Err(e) = ready {
                if let Ok(s) = state.store() {
                    let _ = s.semantic_queue_state(&w.id, "failed", &e);
                }
                error = e;
                break;
            }
            if state.semantic.epoch.load(Ordering::SeqCst) != epoch {
                stopped = true;
                break;
            }
            let vectors = match &w.reuse {
                Some(v) => Ok(v.clone()),
                None => semantic::embed(
                    &config,
                    &key,
                    &w.samples.iter().map(|s| s.text.clone()).collect::<Vec<_>>(),
                ),
            };
            if state.semantic.epoch.load(Ordering::SeqCst) != epoch {
                stopped = true;
                break;
            }
            let result = vectors.and_then(|vec| {
                state
                    .store()?
                    .semantic_commit(&w, &config, generation, &vec)
            });
            match result {
                Ok(true) => {
                    if let Ok(s) = state.store() {
                        let _ = s.semantic_queue_state(&w.id, "done", "");
                    }
                    let _ = app.emit("library-changed", json_ok());
                }
                Ok(false) => {
                    stopped = true;
                    break;
                }
                Err(e) => {
                    if let Ok(s) = state.store() {
                        let _ = s.semantic_queue_state(&w.id, "failed", &e);
                    }
                    error = e;
                    break;
                }
            }
        }
        if let Ok(s) = state.store() {
            let status = if !error.is_empty() {
                "failed"
            } else if stopped {
                "interrupted"
            } else {
                "completed"
            };
            let _=s.conn.execute("UPDATE semantic.queue SET state='interrupted' WHERE state IN ('running','waiting')",[]);
            let _=s.conn.execute("UPDATE semantic.control SET state=?,failed=failed+?,error=? WHERE id=1 AND generation=?",rusqlite::params![status,if error.is_empty(){0}else{1},error,generation]);
        }
        state.semantic.busy.store(false, Ordering::SeqCst);
        let _ = app.emit("library-changed", json_ok());
    });
    Ok(preview)
}
fn dispatch(state: &AppState, app: &tauri::AppHandle, action: &str, v: &Value) -> Result<Value> {
    match action {
        "semantic.status" => state.store()?.semantic_status(),
        "semantic.settings.save" => {
            let s = state.store()?;
            let result = s.semantic_save(v)?;
            state.semantic.invalidate();
            Ok(result)
        }
        "semantic.test" => {
            let (config, key) = {
                let store = state.store()?;
                (
                    store.semantic_config()?,
                    ai::read_secret(&store.root, "semantic-key.dpapi")?,
                )
            };
            semantic::connection_test(&config, &key)
        }
        "semantic.queue" => state.store()?.semantic_queue(),
        "semantic.queue.remove" => {
            let ids = semantic_selection(v)?.ok_or_else(|| err("INVALID_INPUT", "请选择文件"))?;
            state.store()?.semantic_queue_remove(&ids)
        }
        "semantic.plan" => {
            let ids = semantic_selection(v)?;
            Ok(state
                .store()?
                .semantic_plan_mode(ids.as_deref(), v["mode"].as_str().unwrap_or("update"))?
                .0)
        }
        "semantic.build" => semantic_build(state, app, v),
        "semantic.cancel" => {
            state.semantic.invalidate();
            let s = state.store()?;
            s.conn.execute_batch("UPDATE semantic.queue SET state='cancelled' WHERE state IN ('running','waiting');UPDATE semantic.control SET generation=generation+1,state='cancelled' WHERE id=1;").map_err(sql_err)?;
            s.semantic_status()
        }
        "semantic.clear" => {
            state.semantic.invalidate();
            state.store()?.semantic_clear()
        }
        "filters.preview" => state.store()?.natural_preview(v),
        "semantic.search" => {
            let text = str_arg(v, "text")?.trim();
            if text.is_empty() || text.chars().count() > 256 {
                return Err(err("SEMANTIC_QUERY", "请输入 1～256 字符的查询"));
            }
            let (c, key, epoch) = {
                let s = state.store()?;
                let c = s.semantic_config()?;
                if !c.enabled {
                    return Err(err("SEMANTIC_DISABLED", "请在偏好设置启用语义检索"));
                }
                if s.semantic_ready(&c)? == 0 {
                    return Err(err("SEMANTIC_EMPTY", "请先手动建立语义索引"));
                }
                (
                    c,
                    ai::read_secret(&s.root, "semantic-key.dpapi")?,
                    state.semantic.epoch.load(Ordering::SeqCst),
                )
            };
            let vectors = semantic::embed(&c, &key, &[text.to_string()])?;
            let s = state.store()?;
            if state.semantic.epoch.load(Ordering::SeqCst) != epoch || s.semantic_config()? != c {
                return Err(err("SEMANTIC_STALE", "设置或索引已变化，请重试"));
            }
            let hits = s.semantic_rank(&c, &vectors[0])?;
            let count = hits.len();
            let session = state.semantic.remember(&c, text, hits)?;
            Ok(json!({"session":session,"count":count,"model":c.model}))
        }
        "ai.settings" => state.store()?.ai_settings(),
        "ai.settings.save" => {
            let mut store = state.store()?;
            let result = store.save_ai_settings(v)?;
            state.ai_generation.fetch_add(1, Ordering::SeqCst);
            Ok(result)
        }
        "ai.test" => {
            let (config, key) = {
                let s = state.store()?;
                (s.ai_config()?, ai::read_key(&s.root)?)
            };
            ai::test_connection(config, &key)
        }
        "ai.enqueue" => {
            let store = state.store()?;
            if !store.ai_config()?.enabled {
                return Err(err(
                    "AI_NOT_ENABLED",
                    "请先在偏好设置配置并启用 AI 自动标注",
                ));
            }
            store.queue_ai(&string_list(v, "ids"))?;
            Ok(json_ok())
        }
        "ai.retry_failed" => {
            let store = state.store()?;
            store.retry_failed_ai()
        }
        "ai.review.list" => state.store()?.ai_review_list(v),
        "ai.review.history" => state.store()?.ai_review_history(),
        "ai.review.preview" => state.store()?.ai_review_preview(v),
        "ai.review.commit" => state.store()?.ai_review_commit(v),
        "watch.list" => {
            let store = state.store()?;
            state.watch.list(&store)
        }
        "watch.save" | "watch.pause" | "watch.remove" => {
            let _gate = state
                .watch
                .gate
                .lock()
                .map_err(|_| err("INTERNAL_ERROR", "监听锁异常"))?;
            let store = state.store()?;
            watch::change(&store, action, v)?;
            state.watch.invalidate();
            state.watch.list(&store)
        }
        "watch.scan" => {
            let _gate = state
                .watch
                .gate
                .lock()
                .map_err(|_| err("INTERNAL_ERROR", "监听锁异常"))?;
            let store = state.store()?;
            let (rules, _) = watch::rules(&store)?;
            if !rules.iter().any(|r| !r.paused) {
                return Err(err("WATCH_PAUSED", "请先恢复至少一个监听目录"));
            }
            state.watch.invalidate();
            Ok(json_ok())
        }
        "index.status" => state.store()?.search_status(),
        "index.control" => state.store()?.search_control(str_arg(v, "action")?),
        "tasks.status" => {
            let import = state
                .jobs
                .lock()
                .map_err(|_| err("INTERNAL_ERROR", "任务锁异常"))?
                .values()
                .next()
                .map(|(job, _)| job.clone());
            let (ai, watch) = {
                let s = state.store()?;
                (s.ai_summary()?, state.watch.list(&s)?)
            };
            let index = state.store()?.search_status()?;
            Ok(
                json!({"transfer":state.transfer.status(),"import":import,"ai":ai,"watch":watch,"index":index}),
            )
        }
        "capture.extension.open" => {
            if capture_browser::Browser::parse(&v["browser"])? != capture_browser::Browser::Edge {
                return Err(err("INVALID_INPUT", "Chrome 商店版暂未发布，请使用本版本的开发者安装说明"));
            }
            if std::env::var_os("SHIQIAN_QA_BRIDGE_NO_REGISTRY").is_some() {
                std::fs::write(state.store()?.root.join("qa-store-open.json"), json!({"url":capture_browser::EDGE_STORE_URL}).to_string()).map_err(|e|err("OPEN_FAILED",e))?;
            } else {
                open::that(capture_browser::EDGE_STORE_URL).map_err(|e| err("OPEN_FAILED",e))?;
            }
            Ok(json!({"url":capture_browser::EDGE_STORE_URL}))
        }
        "capture.bridge.status" => capture_bridge::status(&*state.store()?, capture_browser::Browser::parse(&v["browser"])?),
        "capture.bridge.register" => capture_bridge::register(&*state.store()?, capture_browser::Browser::parse(&v["browser"])?),
        "capture.bridge.unregister" => capture_bridge::unregister(&*state.store()?, capture_browser::Browser::parse(&v["browser"])?),
        "capture.bridge.disable" => {
            state
                .store()?
                .conn
                .execute("DELETE FROM settings WHERE key='browserCaptureEnabled'", [])
                .map_err(sql_err)?;
            Ok(json_ok())
        }
        "capture.create" => {
            if state.blocks_exit() {
                return Err(err("TRANSFER_BUSY", "请等待资料交接完成"));
            }
            state.store()?.capture(v)
        }
        "capture.sites" => state.store()?.capture_sites(),
        "capture.reading.save" => {
            if state.blocks_exit() { return Err(err("TRANSFER_BUSY", "请等待资料交接完成")); }
            state.store()?.update_web_reading(v)
        }
        "support.status" => state.store()?.support_snapshot(),
        "diagnostics.sqlite" => state.store()?.sqlite_diagnostics(),
        "diagnostics.preview" | "diagnostics.export" => {
            let snapshot = state.store()?.diagnostic_snapshot()?;
            let text = support::diagnostic_report(&snapshot, &state.transfer.status());
            if action == "diagnostics.export" {
                support::export_report(Path::new(str_arg(v, "path")?), &text)
            } else {
                Ok(json!({"text":text}))
            }
        }
        "recovery.dismiss" => {
            let mut store = state.store()?;
            store
                .conn
                .execute("DELETE FROM settings WHERE key='recoveryNotice'", [])
                .map_err(sql_err)?;
            store.recovery_notice.clear();
            Ok(json_ok())
        }
        "ai.cancel" => {
            let store = state.store()?;
            if v.get("ids").is_some() {
                return store.cancel_ai_files(&string_list(v, "ids"));
            }
            state.ai_generation.fetch_add(1, Ordering::SeqCst);
            store.conn.execute("UPDATE ai_jobs SET status='cancelled',error='已取消，可重新识别' WHERE status IN ('queued','running')",[]).map_err(sql_err)?;
            Ok(json_ok())
        }
        "ai.confirm" => state.store()?.confirm_ai(v),
        "folders.apply" => state.store()?.fill_folder_tags(),
        "floating.open" => floating::open(app, state),
        "floating.close" => floating::close(app, state),
        "floating.presets" => state.store()?.floating_presets(),
        "floating.save" => state.store()?.save_floating_presets(&string_list(v, "ids")),
        "floating.state" => floating::options(app),
        "floating.visibility" => Ok(floating::visibility(app)),
        "floating.resize" => floating::resize(
            app,
            state,
            v["collapsed"]
                .as_bool()
                .ok_or_else(|| err("INVALID_INPUT", "缺少收起状态"))?,
        ),
        "floating.topmost" => floating::topmost(
            app,
            state,
            v["value"]
                .as_bool()
                .ok_or_else(|| err("INVALID_INPUT", "缺少置顶状态"))?,
        ),
        "theme.sync" => floating::sync_theme(app, str_arg(v, "theme")?),
        "floating.drag" => floating::start_drag(app, state, str_arg(v, "tagId")?, v["accent"].as_str()),
        "floating.cancel" => {
            state.drag_cancel.store(true, Ordering::Relaxed);
            Ok(json_ok())
        }
        "annotation.apply" => {
            let result = state.store()?.annotate(v);
            floating::finish(app, result.clone());
            result
        }
        "annotation.reject" => {
            floating::finish(app, Err(err("DROP_TARGET", "请把标签放到具体文件卡片上")));
            Ok(json_ok())
        }
        "main.show" => floating::main_show(app),
        "main.close" => {
            if state.transfer.busy.load(Ordering::SeqCst) {
                return Err(err("TRANSFER_BUSY", "后台任务尚未完成，请等待或取消后退出"));
            }
            if state.blocks_exit() {
                return Err(err("IMPORT_BUSY", "加入任务尚未停止，请等待清理完成后退出"));
            }
            floating::save_geometry(app, state);
            // Closing the workspace exits the application, including a visible
            // or hidden palette. Minimize keeps the floating workflow available.
            app.exit(0);
            Ok(json_ok())
        }
        "import" => state.start_import(app.clone(), v),
        "import.status" => {
            let jobs = state
                .jobs
                .lock()
                .map_err(|_| err("INTERNAL_ERROR", "任务锁异常"))?;
            Ok(json!(jobs.values().next().map(|(j, _)| j.clone())))
        }
        "import.cancel" => {
            if let Ok(jobs) = state.jobs.lock() {
                for (_, cancel) in jobs.values() {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            Ok(json_ok())
        }
        "refresh" => state.refresh_all(app.clone()),
        "text.document" | "preview" | "pdf.document" | "html.document" | "office.document"
        | "office.summary" => {
            let (f, root) = {
                let mut s = state.store()?;
                let f = s.refresh_file(str_arg(v, "id")?)?;
                (f, s.root.clone())
            };
            if action == "text.document" {
                text_document::read(&f)
            } else if action == "office.summary" {
                office::summary(&root, &f)
            } else if action == "office.document" {
                office::document(&f, v["sheet"].as_u64().unwrap_or(0) as usize)
            } else if action == "html.document" {
                fsops::html_document(&root, &f)
            } else if action == "pdf.document" {
                fsops::pdf_document(&root, &f)
            } else {
                fsops::preview(&root, &f, v["large"].as_bool().unwrap_or(false))
            }
        }
        a if a.starts_with("browser.") => web_browser::dispatch(app,state,a,v),
        "office.engines" => { let root=state.store()?.root.clone(); office_layout::detect(&root) },
        "office.component" => { open::that("https://www.libreoffice.org/download/download-libreoffice/").map_err(|e|err("OPEN_FAILED",e))?; Ok(json_ok()) },
        "office.normalized" => {
            let (f,root)={let mut s=state.store()?;(s.refresh_file(str_arg(v,"id")?)?,s.root.clone())};
            office_layout::normalized(&root,&f,str_arg(v,"engine")?,v["sheet"].as_u64().unwrap_or(0) as usize)
        },
        "office.layout" => {
            let (f,root)={let mut s=state.store()?; (s.refresh_file(str_arg(v,"id")?)?,s.root.clone())};
            office_layout::document(&root,&f,str_arg(v,"engine")?)
        },
        "text.save" => state.store()?.save_markdown(v),
        "preview.cache" => {
            let s = state.store()?;
            let f = s.file(str_arg(v, "id")?)?;
            if v["revision"].as_str() != Some(&f.revision) {
                return Err(err("FILE_CHANGED", "文件版本已变化"));
            }
            fsops::store_pdf_preview(
                &s.root,
                &f,
                v["large"].as_bool().unwrap_or(false),
                str_arg(v, "data")?,
            )
        }
        "capture.source.open" => {
            let f = state.store()?.file(str_arg(v, "id")?)?;
            let source = f
                .web_source
                .ok_or_else(|| err("INVALID_FILE", "资料没有网页来源"))?;
            let url = crate::capture::checked_url(&source.url)?;
            open::that(url).map_err(|e| err("OPEN_FAILED", e))?;
            Ok(json_ok())
        }
        "files.open" | "files.reveal" => {
            let f = state.store()?.refresh_file(str_arg(v, "id")?)?;
            if action == "files.reveal" {
                fsops::reveal(&f.path)?;
            } else {
                if f.status != "available" {
                    return Err(err("FILE_UNAVAILABLE", f.error));
                }
                let target = f
                    .web_source
                    .as_ref()
                    .filter(|s| s.kind != "web_image")
                    .map(|s| s.url.as_str())
                    .unwrap_or(&f.path);
                open::that(target).map_err(|e| err("OPEN_FAILED", e))?;
            }
            Ok(json_ok())
        }
        "data.reveal" => {
            open::that(state.store()?.root.clone()).map_err(|e| err("OPEN_FAILED", e))?;
            Ok(json_ok())
        }
        "package.status" => Ok(state.transfer.status()),
        "package.cancel" => {
            if state.transfer.status()["task"]["kind"] == "backup.restore" {
                return Err(err("TRANSFER_BUSY", "恢复已开始，请等待完成"));
            }
            state.transfer.cancel.store(true, Ordering::SeqCst);
            Ok(json_ok())
        }
        "package.plan" => Ok(state.store()?.transfer_plan(v)?.info),
        "package.inspect" => state.transfer.run_named("package.inspect", || {
            let store = state.store()?;
            transfer::preview_in_library(Path::new(str_arg(v, "path")?), &state.transfer, &store)
        }),
        "package.export" => state.transfer.run_named("package.export", || {
            let plan = state.store()?.transfer_plan(v)?;
            transfer::export(
                plan,
                Path::new(str_arg(v, "path")?),
                str_arg(v, "fingerprint")?,
                &state.transfer,
            )
        }),
        "package.import" => state.transfer.run_named("package.import", || {
            if state
                .jobs
                .lock()
                .map_err(|_| err("INTERNAL_ERROR", "任务锁异常"))?
                .values()
                .any(|(j, _)| !j.done)
            {
                return Err(err("IMPORT_BUSY", "请先等待文件加入任务完成"));
            }
            transfer::import_shared(
                &state.store,
                Path::new(str_arg(v, "path")?),
                Path::new(str_arg(v, "destination")?),
                str_arg(v, "fingerprint")?,
                &state.transfer,
            )
        }),
        "backup.inspect" => {
            let root = state.store()?.root.clone();
            Ok(backup::inspect_backup(Path::new(str_arg(v, "path")?), &root)?.info)
        }
        "bootstrap" => state.store()?.bootstrap(),
        "query" => {
            let q: Query =
                serde_json::from_value(v.clone()).map_err(|e| err("INVALID_INPUT", e))?;
            if q.search_mode == "semantic"
                || (q.search_mode == "unified" && !q.semantic_session.is_empty())
            {
                let store = state.store()?;
                match state.semantic.resolve(&store, &q) {
                    Err(e)
                        if q.search_mode == "unified"
                            && (e.starts_with("SEMANTIC_EXPIRED:")
                                || e.starts_with("SEMANTIC_DISABLED:")) =>
                    {
                        let mut local = store.query(&q)?;
                        local["searchWarning"] = json!(
                            "智能补充已过期或关闭，已保留本地搜索结果；需要时重新发送智能补充"
                        );
                        Ok(local)
                    }
                    result => result,
                }
            } else {
                state.store()?.query(&q)
            }
        }
        "file" => Ok(json!(state.store()?.file(str_arg(v, "id")?)?)),
        "tags.export" => {
            let plan = state.store()?.tag_export_plan(str_arg(v, "id")?)?;
            tag_export::export(plan, Path::new(str_arg(v, "destination")?))
        }
        "export.reveal" => {
            let path = Path::new(str_arg(v, "path")?);
            if !path.is_dir() {
                return Err(err("EXPORT_DESTINATION", "导出文件夹已不存在"));
            }
            open::that(path).map_err(|e| err("OPEN_FAILED", e))?;
            Ok(json_ok())
        }
        "tag-groups.get" => Ok(json!(state.store()?.tag_groups()?)),
        "tag-groups.update" => state.store()?.tag_groups_update(v),
        "tag.color" => state.store()?.tag_color(v),
        "tag.create" => Ok(json!(state
            .store()?
            .create_workspace_tag(str_arg(v, "name")?)?)),
        "note.save" => state.store()?.save_note(v),
        "tags.aliases.preview" => state.store()?.aliases_preview(v),
        "tags.aliases.update" => state.store()?.aliases_update(v),
        "collections.reorder" => state.store()?.collections_reorder(v),
        "collections.save" => state.store()?.collection_save(v),
        "collections.delete" => state.store()?.collection_delete(v),
        "tags.merge.preview" => state.store()?.merge_preview(v),
        "tags.merge" => state.store()?.merge_tags(v),
        "reader.get" => state.store()?.reader_state(v),
        "reader.save" => state.store()?.save_reader_state(v),
        "settings.save" => state.store()?.save_settings(v),
        "files.tags" | "files.favorite" | "files.remove" | "tags.rename" | "tags.delete" => {
            state.store()?.mutate(action, v)
        }
        "repair.cancel" => {
            state.repair.cancel.store(true, Ordering::SeqCst);
            Ok(json_ok())
        }
        "repair.preview" => state.repair.run(|| {
            if state.blocks_exit()
                && (state.transfer.busy.load(Ordering::SeqCst)
                    || state
                        .jobs
                        .lock()
                        .map(|j| j.values().any(|(j, _)| !j.done))
                        .unwrap_or(true))
            {
                return Err(err("TRANSFER_BUSY", "请等待导入或资料交接完成"));
            }
            let old = str_arg(v, "oldRoot")?;
            let new = str_arg(v, "newRoot")?;
            let snapshot = {
                let store = state.store()?;
                path_repair::snapshot(&store, old)?
            };
            let plan = path_repair::preview(snapshot, old, new, &state.repair.cancel)?;
            state.repair.remember(plan)
        }),
        "repair.apply" => state.repair.run(|| {
            if state.transfer.busy.load(Ordering::SeqCst) {
                return Err(err("TRANSFER_BUSY", "请等待资料交接完成"));
            }
            let token = str_arg(v, "token")?;
            let plan = state.repair.get(token)?;
            let checked = path_repair::verify(&plan, v, &state.repair.cancel)?;
            if state.repair.cancel.load(Ordering::SeqCst) {
                return Err(err("REPAIR_CANCELLED", "修复已取消，文件引用未修改"));
            }
            let result = {
                let mut store = state.store()?;
                path_repair::apply(&mut store, &plan, v, checked)?
            };
            state.repair.forget(token);
            Ok(result)
        }),
        "files.relink" => state.store()?.relink(v),
        "undo" => {
            let mut store = state.store()?;
            if let Some(expected) = v.get("expectedId").and_then(Value::as_str) {
                store.undo_checked(expected)
            } else {
                store.undo_last()
            }
        }
        "cache.stats" => {
            let root = state.store()?.root.clone();
            cache::stats(&root)
        }
        "cache.clear" => {
            let root = state.store()?.root.clone();
            cache::clear(&root)
        }
        "backup.export" => state.store()?.export_backup(Path::new(str_arg(v, "path")?)),
        "backup.restore" => state.transfer.run_named("backup.restore", || {
            state.semantic.invalidate();
            if state
                .jobs
                .lock()
                .map_err(|_| err("INTERNAL_ERROR", "任务锁异常"))?
                .values()
                .any(|(j, _)| !j.done)
            {
                return Err(err("IMPORT_BUSY", "请等待加入任务完成或取消后再恢复"));
            }
            let _gate = state
                .watch
                .gate
                .lock()
                .map_err(|_| err("INTERNAL_ERROR", "监听锁异常"))?;
            state.watch.invalidate();
            let mut store = state.store()?;
            state.ai_generation.fetch_add(1, Ordering::SeqCst);
            let result = store.restore_backup(Path::new(str_arg(v, "path")?))?;
            Ok(result)
        }),
        _ => Err(err("UNKNOWN_ACTION", action)),
    }
}
#[tauri::command]
async fn api(
    app: tauri::AppHandle,
    webview: tauri::Webview,
    state: tauri::State<'_, AppState>,
    action: String,
    payload: Option<Value>,
) -> Result<Value> {
    if !matches!(webview.label(), "main" | "floating") { return Err(err("ACCESS_DENIED", "外部网页不能访问资料库")); }
    let url=webview.url().map_err(|e|err("ACCESS_DENIED",e))?;
    if !(url.scheme()=="tauri" || (url.scheme()=="http" && matches!(url.host_str(),Some("tauri.localhost")|Some("localhost")))) { return Err(err("ACCESS_DENIED","外部网页不能访问资料库")); }
    let state = state.inner().clone();
    let payload = payload.unwrap_or(json!({}));
    tauri::async_runtime::spawn_blocking(move || {
        let result = dispatch(&state, &app, &action, &payload);
        if result.is_ok()
            && matches!(
                action.as_str(),
                "capture.create"
                    | "capture.reading.save"
                    | "tag.create"
                    | "tag.color"
                    | "tag-groups.update"
                    | "text.save"
                    | "index.control"
                    | "semantic.settings.save"
                    | "semantic.clear"
                    | "semantic.cancel"
                    | "tags.rename"
                    | "tags.delete"
                    | "tags.aliases.update"
                    | "collections.reorder"
                    | "tags.merge"
                    | "collections.save"
                    | "collections.delete"
                    | "repair.apply"
                    | "files.tags"
                    | "files.remove"
                    | "files.favorite"
                    | "undo"
                    | "settings.save"
                    | "floating.save"
                    | "floating.topmost"
                    | "floating.resize"
                    | "folders.apply"
                    | "ai.settings.save"
                    | "ai.review.commit"
                    | "ai.enqueue"
                    | "ai.retry_failed"
                    | "ai.cancel"
                    | "ai.confirm"
                    | "backup.restore"
                    | "package.import"
                    | "recovery.dismiss"
                    | "backup.export"
            )
        {
            let _ = app.emit("library-changed", ());
        }
        result
    })
    .await
    .map_err(|e| err("INTERNAL_ERROR", e))?
}
pub fn run() {
    let mut context = tauri::generate_context!();
    // An explicit library directory also isolates WebView state for testing or
    // managed deployments. Normal installations use the OS app-data directory.
    if let Some(root) = std::env::var_os("SHIQIAN_DATA_DIR") {
        // An isolated library must not activate a user's already-running normal
        // instance. Keep the single-instance lock scoped to its data directory.
        use sha2::{Digest, Sha256};
        let absolute = std::path::absolute(root).expect("Invalid SHIQIAN_DATA_DIR");
        let digest = format!(
            "{:x}",
            Sha256::digest(absolute.to_string_lossy().to_lowercase().as_bytes())
        );
        context.config_mut().identifier = format!("local.shiqian.isolated.{}", &digest[..16]);
        for window in &mut context.config_mut().app.windows {
            window.create = false;
        }
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app,_,_|{if let Some(w)=app.get_webview_window("main"){let _=w.unminimize();let _=w.show();let _=w.set_focus();}}))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app|{
            let root=std::env::var_os("SHIQIAN_DATA_DIR").map(PathBuf::from).unwrap_or(app.path().app_data_dir()?);
            let state=match AppState::new(&root){Ok(state)=>state,Err(error)=>{
                app.dialog().message(format!("资料库无法打开，已停止写入。\n\n{error}\n\n数据目录：{}\n请保留该目录与备份；如果提示版本过新，请使用创建该库的较新版本。",root.display())).title("拾签 · 资料库不可用").kind(tauri_plugin_dialog::MessageDialogKind::Error).blocking_show();
                app.handle().exit(1);return Ok(());
            }};app.manage(state.clone());
            if std::env::var_os("SHIQIAN_DATA_DIR").is_some(){
                for config in &app.config().app.windows {tauri::WebviewWindowBuilder::from_config(app,config)?.data_directory(root.join("webview")).build()?;}
            }
            let ai_app=app.handle().clone();
            ai_worker::start_worker(state.store.clone(),state.ai_generation.clone(),move || {let _=ai_app.emit("library-changed",());});
            capture_bridge::start(state.store.clone(),state.watch.clone(),state.transfer.clone(),app.handle().clone());
            start_watch(state.clone(),app.handle().clone());
            start_index(state.clone(),app.handle().clone());
            {
                let root = state.store()?.root.clone();
                let state = state.clone();
                std::thread::spawn(move || {
                    while !state.watch.stopped.load(Ordering::SeqCst) {
                        // Avoid database locks and yield filesystem work behind foreground tasks.
                        if !state.blocks_exit() { let _ = cache::maintain(&root); }
                        for _ in 0..240 {
                            if state.watch.stopped.load(Ordering::SeqCst) { return; }
                            std::thread::sleep(std::time::Duration::from_millis(250));
                        }
                    }
                });
            }
            let _=state.refresh_all(app.handle().clone());Ok(())
        })
        .on_window_event(|window, event| {
            // If the main WebView closes without its JS close handler (for example
            // during startup), auxiliary windows must not leave an orphan process.
            if matches!(event, tauri::WindowEvent::Destroyed) && window.label() == "main" {
                window.app_handle().exit(0);
            }
            if window.label() == "floating" && matches!(event, tauri::WindowEvent::Destroyed) {
                let _ = window.app_handle().emit("floating-visibility", json!({"visible":false}));
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if let Some(state) = window.app_handle().try_state::<AppState>() {
                    if window.label() == "main" && state.blocks_exit() {
                        api.prevent_close();
                        let _ = window.emit("exit-blocked", ());
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![api])
        .build(context).expect("拾签启动失败")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if let Some(state) = app.try_state::<AppState>() {
                    if state.blocks_exit() {
                        api.prevent_exit();
                        let _ = app.emit("exit-blocked", ());
                    } else {
                        if let Ok(_gate)=state.watch.gate.lock(){state.watch.stopped.store(true,Ordering::SeqCst);state.watch.invalidate();}
                        floating::save_geometry(app, &state);
                    }
                }
            }
        });
}
