use crate::{
    db::{validate_database, Store},
    model::*,
};
use rusqlite::{backup::Backup, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
    time::Duration,
};
use zip::{write::SimpleFileOptions, ZipArchive, ZipWriter};

#[derive(Serialize, Deserialize)]
struct Manifest {
    backup_format: u32,
    schema_version: u32,
    app_version: String,
    created_at: i64,
    library_id: String,
    file_count: i64,
    tag_count: i64,
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub struct Inspected {
    pub conn: Connection,
    pub _dir: tempfile::TempDir,
    pub info: Value,
}

pub fn inspect_backup(path: &Path, root: &Path) -> Result<Inspected> {
    let file = File::open(path).map_err(|e| err("BACKUP_INVALID", e))?;
    let mut zip = ZipArchive::new(file).map_err(|e| err("BACKUP_INVALID", e))?;
    if zip.len() != 3 {
        return Err(err("BACKUP_INVALID", "备份文件结构不正确"));
    }
    let names = ["manifest.toml", "metadata.sqlite", "checksums.sha256"];
    let mut seen = std::collections::HashSet::new();
    for i in 0..zip.len() {
        let f = zip.by_index(i).map_err(|e| err("BACKUP_INVALID", e))?;
        if !names.contains(&f.name())
            || !seen.insert(f.name().to_string())
            || f.is_dir()
            || f.size() > 256 * 1024 * 1024
        {
            return Err(err("BACKUP_INVALID", "备份包含未知、重复或过大文件"));
        }
    }
    let mut get = |name: &str, max: u64| -> Result<Vec<u8>> {
        let mut f = zip.by_name(name).map_err(|e| err("BACKUP_INVALID", e))?;
        if f.size() > max {
            return Err(err("BACKUP_INVALID", "备份内容过大"));
        }
        let mut bytes = vec![];
        (&mut f)
            .take(max + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| err("BACKUP_INVALID", e))?;
        if bytes.len() as u64 > max {
            return Err(err("BACKUP_INVALID", "备份内容超出上限"));
        }
        Ok(bytes)
    };
    let manifest_text = String::from_utf8(get("manifest.toml", 16 * 1024)?)
        .map_err(|e| err("BACKUP_INVALID", e))?;
    let manifest: Manifest =
        toml::from_str(&manifest_text).map_err(|e| err("BACKUP_INVALID", e))?;
    if manifest.backup_format != 1 || !(1..=7).contains(&manifest.schema_version) {
        return Err(err(
            "BACKUP_VERSION_UNSUPPORTED",
            "此备份需要其他版本的拾签",
        ));
    }
    let expected =
        String::from_utf8(get("checksums.sha256", 1024)?).map_err(|e| err("BACKUP_INVALID", e))?;
    let bytes = get("metadata.sqlite", 256 * 1024 * 1024)?;
    if expected.trim() != format!("{}  metadata.sqlite", sha(&bytes)) {
        return Err(err("BACKUP_INVALID", "备份校验失败，文件可能已损坏"));
    }
    let dir = tempfile::Builder::new()
        .prefix("restore-inspect-")
        .tempdir_in(root)
        .map_err(|e| err("STORAGE_UNAVAILABLE", e))?;
    let db = dir.path().join("metadata.sqlite");
    fs::write(&db, bytes).map_err(|e| err("STORAGE_UNAVAILABLE", e))?;
    let conn =
        Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(sql_err)?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(sql_err)?;
    validate_database(&conn)?;
    let unsafe_schema: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('trigger','view')",
            [],
            |r| r.get(0),
        )
        .map_err(sql_err)?;
    if unsafe_schema != 0 {
        return Err(err("BACKUP_INVALID", "备份含不支持的数据库结构"));
    }
    let actual_files: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM files WHERE removed_at IS NULL",
            [],
            |r| r.get(0),
        )
        .map_err(sql_err)?;
    let actual_tags: i64 = conn
        .query_row("SELECT COUNT(*) FROM tags", [], |r| r.get(0))
        .map_err(sql_err)?;
    let library_id: String = conn
        .query_row("SELECT value FROM library_meta WHERE key='id'", [], |r| {
            r.get(0)
        })
        .map_err(sql_err)?;
    if actual_files != manifest.file_count
        || actual_tags != manifest.tag_count
        || library_id != manifest.library_id
    {
        return Err(err("BACKUP_INVALID", "备份清单与数据库不一致"));
    }
    let info = json!({"fileCount":actual_files,"tagCount":actual_tags,"createdAt":manifest.created_at,"version":manifest.app_version,"libraryId":manifest.library_id});
    Ok(Inspected {
        _dir: dir,
        conn,
        info,
    })
}

impl Store {
    pub fn export_backup(&self, path: &Path) -> Result<Value> {
        if path.extension().and_then(|s| s.to_str()) != Some("sqtagbackup") {
            return Err(err("INVALID_BACKUP_PATH", "备份扩展名必须是 .sqtagbackup"));
        }
        let dir = tempfile::Builder::new()
            .prefix("backup-")
            .tempdir_in(&self.root)
            .map_err(|e| err("STORAGE_UNAVAILABLE", e))?;
        let snapshot = dir.path().join("metadata.sqlite");
        let mut target = Connection::open(&snapshot).map_err(sql_err)?;
        {
            let backup = Backup::new(&self.conn, &mut target).map_err(sql_err)?;
            backup
                .run_to_completion(64, Duration::from_millis(1), None)
                .map_err(sql_err)?;
        }
        // Credentials and service consent remain local; a restored backup never enables uploads.
        target
            .execute(
                "DELETE FROM settings WHERE key IN ('ai','semanticConfig','browserCaptureEnabled')",
                [],
            )
            .map_err(sql_err)?;
        target
            .execute(
                "DELETE FROM settings WHERE key IN ('lastBackup','recoveryNotice','floatingPosition')",
                [],
            )
            .map_err(sql_err)?;
        target.execute("UPDATE ai_jobs SET status='cancelled',error='从备份恢复后请重新识别' WHERE status IN ('queued','running')",[]).map_err(sql_err)?;
        validate_database(&target)?;
        let manifest = Manifest {
            backup_format: 1,
            schema_version: target
                .pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
                .map_err(sql_err)?,
            app_version: env!("CARGO_PKG_VERSION").into(),
            created_at: now(),
            library_id: target
                .query_row("SELECT value FROM library_meta WHERE key='id'", [], |r| {
                    r.get(0)
                })
                .map_err(sql_err)?,
            file_count: target
                .query_row(
                    "SELECT COUNT(*) FROM files WHERE removed_at IS NULL",
                    [],
                    |r| r.get(0),
                )
                .map_err(sql_err)?,
            tag_count: target
                .query_row("SELECT COUNT(*) FROM tags", [], |r| r.get(0))
                .map_err(sql_err)?,
        };
        target.close().map_err(|(_, e)| sql_err(e))?;
        let bytes = fs::read(snapshot).map_err(|e| err("BACKUP_FAILED", e))?;
        let parent = path
            .parent()
            .ok_or_else(|| err("INVALID_BACKUP_PATH", "请选择备份位置"))?;
        let mut output =
            tempfile::NamedTempFile::new_in(parent).map_err(|e| err("BACKUP_FAILED", e))?;
        {
            let mut zip = ZipWriter::new(output.as_file_mut());
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            for (name, content) in [
                (
                    "manifest.toml",
                    toml::to_string(&manifest)
                        .map_err(|e| err("BACKUP_FAILED", e))?
                        .into_bytes(),
                ),
                (
                    "checksums.sha256",
                    format!("{}  metadata.sqlite\n", sha(&bytes)).into_bytes(),
                ),
                ("metadata.sqlite", bytes),
            ] {
                zip.start_file(name, options)
                    .map_err(|e| err("BACKUP_FAILED", e))?;
                zip.write_all(&content)
                    .map_err(|e| err("BACKUP_FAILED", e))?;
            }
            zip.finish().map_err(|e| err("BACKUP_FAILED", e))?;
        }
        output
            .as_file()
            .sync_all()
            .map_err(|e| err("BACKUP_FAILED", e))?;
        output
            .persist_noclobber(path)
            .map_err(|e| err("BACKUP_FAILED", format!("{e}；请选择未使用的备份文件名")))?;
        let notice = self
            .remember_backup(path, "manual")
            .err()
            .map(|_| "备份已保存，最近备份状态暂未更新");
        Ok(
            json!({"path":path.to_string_lossy(),"fileCount":manifest.file_count,"tagCount":manifest.tag_count,"notice":notice}),
        )
    }
    pub fn restore_backup(&mut self, path: &Path) -> Result<Value> {
        let inspected = inspect_backup(path, &self.root)?;
        let backups = self.root.join("backups");
        fs::create_dir_all(&backups).map_err(|e| err("RESTORE_FAILED", e))?;
        let recovery = backups.join(format!("before-restore-{}-{}.sqtagbackup", now(), id()));
        self.export_backup(&recovery)?;
        let marker = self.root.join("restore-state.toml");
        {
            let mut f = File::create(&marker).map_err(|e| err("RESTORE_FAILED", e))?;
            let text = toml::to_string(&std::collections::BTreeMap::from([(
                "recovery_backup",
                recovery.to_string_lossy().to_string(),
            )]))
            .map_err(|e| err("RESTORE_FAILED", e))?;
            f.write_all(text.as_bytes())
                .map_err(|e| err("RESTORE_FAILED", e))?;
            f.sync_all().map_err(|e| err("RESTORE_FAILED", e))?;
        }
        // The destination transaction used by SQLite Backup is atomic on interruption.
        let outcome = (|| -> Result<()> {
            let backup = Backup::new(&inspected.conn, &mut self.conn).map_err(sql_err)?;
            backup
                .run_to_completion(64, Duration::from_millis(1), None)
                .map_err(sql_err)
        })();
        if let Err(e) = outcome {
            return Err(err(
                "RESTORE_FAILED",
                format!("{e}；恢复前备份：{}", recovery.display()),
            ));
        }
        crate::auto_tags::migrate(&self.conn)?;
        crate::tag_aliases::migrate(&self.conn)?;
        crate::capture::migrate(&self.conn)?;
        crate::reader_state::migrate(&self.conn)?;
        self.rebuild_web_descriptors()?;
        crate::db::ensure_performance_indexes(&self.conn)?;
        // Enforce local consent even when the backup came from another writer.
        self.conn
            .execute(
                "DELETE FROM settings WHERE key IN ('ai','semanticConfig','browserCaptureEnabled')",
                [],
            )
            .map_err(sql_err)?;
        self.conn.execute("UPDATE ai_jobs SET status='cancelled',error='从备份恢复后请重新识别' WHERE status IN ('queued','running')", []).map_err(sql_err)?;
        self.validate()?;
        crate::watch::pause_all(self)?;
        self.conn
            .execute_batch("PRAGMA foreign_keys=ON; PRAGMA wal_checkpoint(FULL);")
            .map_err(sql_err)?;
        self.undo.clear();
        // Derived text belongs to the pre-restore library. Require explicit opt-in again.
        self.search_control("clear")?;
        self.semantic_clear()?;
        crate::ai::write_secret(&self.root, "semantic-key.dpapi", "")?;
        self.recovery_notice.clear();
        self.clear_cache()?;
        fs::remove_file(marker).map_err(|e| err("RESTORE_FAILED", e))?;
        let notice = self
            .remember_backup(&recovery, "beforeRestore")
            .err()
            .map(|_| "恢复已完成，最近备份状态暂未更新");
        Ok(
            json!({"ok":true,"recoveryBackup":recovery.to_string_lossy(),"info":inspected.info,"notice":notice}),
        )
    }
    pub fn clear_cache(&self) -> Result<Value> {
        crate::cache::clear(&self.root)
    }
}
