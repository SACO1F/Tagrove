//! Local support data. Diagnostic reports use an explicit allowlist, never raw settings/errors.
use crate::{db::Store, model::*};
use rusqlite::OptionalExtension;
use serde_json::{json, Value};
use std::{io::Write, path::Path};

/// Runtime identity from the SQLite connection actually used by this executable.
/// No user tables, settings, paths or writes are needed for these probes.
pub fn sqlite_runtime_snapshot(conn: &rusqlite::Connection) -> Result<Value> {
    let (version, source_id): (String, String) = conn
        .query_row("SELECT sqlite_version(), sqlite_source_id()", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .map_err(sql_err)?;
    let option = |name: &str| -> Result<bool> {
        conn.query_row("SELECT sqlite_compileoption_used(?)", [name], |r| {
            r.get::<_, bool>(0)
        })
        .map_err(sql_err)
    };
    let json: bool = conn
        .query_row("SELECT json_valid('{\"probe\":true}')", [], |r| r.get(0))
        .map_err(sql_err)?;
    let fts5: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_module_list WHERE name='fts5')",
            [],
            |r| r.get(0),
        )
        .map_err(sql_err)?;
    Ok(json!({
        "version":version,"sourceId":source_id,"versionNumber":rusqlite::version_number(),
        "capabilities":{"fts5":fts5,"json":json,"compileFts5":option("ENABLE_FTS5")?,
                        "omitWal":option("OMIT_WAL")?,"omitBackup":option("OMIT_BACKUP")?},
        "applicationVersion":env!("CARGO_PKG_VERSION"),"os":std::env::consts::OS,"arch":std::env::consts::ARCH
    }))
}

fn valid_sqlite_version(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 3 && p.bytes().all(|b| b.is_ascii_digit()))
}
fn valid_sqlite_source_id(value: &str) -> bool {
    let parts: Vec<_> = value.split(' ').collect();
    if parts.len() != 3 || parts[0].len() != 10 || parts[1].len() != 8 || parts[2].len() != 64 {
        return false;
    }
    parts[0].bytes().enumerate().all(|(i, b)| {
        if i == 4 || i == 7 {
            b == b'-'
        } else {
            b.is_ascii_digit()
        }
    }) && parts[1].bytes().enumerate().all(|(i, b)| {
        if i == 2 || i == 5 {
            b == b':'
        } else {
            b.is_ascii_digit()
        }
    }) && parts[2].bytes().all(|b| b.is_ascii_hexdigit())
}

impl Store {
    pub fn support_snapshot(&self) -> Result<Value> {
        let files: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM files WHERE removed_at IS NULL",
                [],
                |r| r.get(0),
            )
            .map_err(sql_err)?;
        let tags: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM tags", [], |r| r.get(0))
            .map_err(sql_err)?;
        let unavailable: i64 = self.conn.query_row("SELECT COUNT(*) FROM files WHERE removed_at IS NULL AND status IN ('missing','offline','inaccessible')", [], |r| r.get(0)).map_err(sql_err)?;
        let schema: i64 = self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(sql_err)?;
        let last: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key='lastBackup'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(sql_err)?;
        Ok(
            json!({"version":env!("CARGO_PKG_VERSION"),"schema":schema,"files":files,"tags":tags,"unavailable":unavailable,"ai":self.ai_summary()?,"lastBackup":last.and_then(|s|serde_json::from_str::<Value>(&s).ok()),"notice":self.recovery_notice}),
        )
    }
    /// Explicit diagnostics only: do not scan the cache during routine status polling.
    pub fn diagnostic_snapshot(&self) -> Result<Value> {
        let mut value = self.support_snapshot()?;
        value["sqlite"] = self.sqlite_diagnostics()?;
        let bytes = |names: &[&str]| -> u64 {
            names
                .iter()
                .filter_map(|name| std::fs::symlink_metadata(self.root.join(name)).ok())
                .filter(|m| m.is_file() && !crate::fsops::is_link(m))
                .map(|m| m.len())
                .sum()
        };
        let state = |sql: &str| -> Result<String> {
            self.conn.query_row(sql, [], |r| r.get(0)).map_err(sql_err)
        };
        let count = |sql: &str| -> Result<i64> {
            self.conn.query_row(sql, [], |r| r.get(0)).map_err(sql_err)
        };
        value["storage"] = json!({"databaseBytes":bytes(&["library.sqlite","library.sqlite-wal","library.sqlite-shm"]),"textIndexBytes":bytes(&["cache/search/index.sqlite","cache/search/index.sqlite-wal","cache/search/index.sqlite-shm"]),"semanticIndexBytes":bytes(&["cache/semantic/index.sqlite","cache/semantic/index.sqlite-wal","cache/semantic/index.sqlite-shm"]),"previews":crate::cache::stats(&self.root).ok()});
        value["derived"] = json!({"textState":state("SELECT state FROM search.control WHERE id=1")?,"semanticState":state("SELECT state FROM semantic.control WHERE id=1")?,"textDocuments":count("SELECT COUNT(*) FROM search.docs WHERE state='done'")?,"textFailed":count("SELECT COUNT(*) FROM search.docs WHERE state='failed'")?,"readingStates":count("SELECT COUNT(*) FROM reader_states")?,"restorePending":self.root.join("restore-state.toml").exists()});
        Ok(value)
    }
    pub fn sqlite_diagnostics(&self) -> Result<Value> {
        let mut value = sqlite_runtime_snapshot(&self.conn)?;
        let mode = |sql: &str| -> Result<String> {
            self.conn.query_row(sql, [], |r| r.get(0)).map_err(sql_err)
        };
        value["journalModes"] = json!({"main":mode("PRAGMA main.journal_mode")?,
            "textIndex":mode("PRAGMA search.journal_mode")?,"semanticIndex":mode("PRAGMA semantic.journal_mode")?});
        Ok(value)
    }
    pub fn retry_failed_ai(&self) -> Result<Value> {
        if !self.ai_config()?.enabled {
            return Err(err("AI_NOT_ENABLED", "请先配置并启用 AI 自动标注"));
        }
        // Only failed tasks, never already completed, unsupported or user-cancelled work.
        let count = self.conn.execute("UPDATE ai_jobs SET status='queued',error='',updated_at=MAX(updated_at+1,?) WHERE status='failed' AND file_id IN (SELECT id FROM files WHERE removed_at IS NULL)", [now()]).map_err(sql_err)?;
        Ok(json!({"queued":count}))
    }
    pub fn remember_backup(&self, path: &Path, kind: &str) -> Result<()> {
        self.conn.execute("INSERT INTO settings(key,value) VALUES('lastBackup',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [json!({"createdAt":now(),"path":path.to_string_lossy(),"kind":kind}).to_string()]).map_err(sql_err)?;
        Ok(())
    }
    pub fn remember_recovery(&self) -> Result<()> {
        self.conn.execute("INSERT INTO settings(key,value) VALUES('recoveryNotice',?) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [json!(self.recovery_notice).to_string()]).map_err(sql_err)?;
        Ok(())
    }
}

pub fn diagnostic_report(snapshot: &Value, transfer: &Value) -> String {
    let mut report = format!("# 拾签本地诊断\n\n版本：{}\n系统：{} {}\n资料库格式：{}\n文件记录：{}\n标签：{}\n不可用文件：{}\n恢复提示：{}\n", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH, snapshot["schema"], snapshot["files"], snapshot["tags"], snapshot["unavailable"], if snapshot["notice"].as_str().unwrap_or("").is_empty() {"无"} else {"有，请在本机查看"});
    let sqlite = &snapshot["sqlite"];
    if let Some(version) = sqlite["version"]
        .as_str()
        .filter(|s| valid_sqlite_version(s))
    {
        report.push_str(&format!("SQLite 运行时：{version}\n"));
    }
    if let Some(source_id) = sqlite["sourceId"]
        .as_str()
        .filter(|s| valid_sqlite_source_id(s))
    {
        report.push_str(&format!("SQLite source ID：{source_id}\n"));
    }
    if sqlite["capabilities"].is_object() {
        for (key, label) in [("fts5", "FTS5"), ("json", "JSON")] {
            report.push_str(&format!(
                "SQLite {label}：{}\n",
                if sqlite["capabilities"][key].as_bool() == Some(true) {
                    "可用"
                } else {
                    "未确认"
                }
            ));
        }
    }
    report.push_str("\nAI 任务计数：\n");
    for (key, label) in [
        ("queued", "等待"),
        ("running", "处理中"),
        ("done", "完成"),
        ("failed", "失败"),
        ("unsupported", "未支持"),
        ("cancelled", "取消"),
    ] {
        report.push_str(&format!(
            "- {label}：{}\n",
            snapshot["ai"][key].as_u64().unwrap_or(0)
        ));
    }
    report.push_str(&format!(
        "\n资料包任务：{}\n已处理字节：{}\n已处理文件：{}\n",
        if transfer["busy"].as_bool().unwrap_or(false) {
            "进行中"
        } else {
            "空闲"
        },
        transfer["bytes"].as_u64().unwrap_or(0),
        transfer["completed"].as_u64().unwrap_or(0)
    ));
    if let Some(history) = transfer["history"].as_array() {
        report.push_str("\n本次运行的资料包任务（最多 12 项）：\n");
        for task in history.iter().take(12) {
            // Names, paths, arbitrary errors, AI models/endpoints and results are deliberately omitted.
            let kind = match task["kind"].as_str().unwrap_or("") {
                "package.inspect" => "校验",
                "package.import" => "导入",
                "package.export" => "导出",
                "backup.restore" => "恢复",
                _ => "资料包",
            };
            let state = match task["state"].as_str().unwrap_or("") {
                "done" => "完成",
                "cancelled" => "取消",
                "failed" => "失败",
                _ => "进行中",
            };
            report.push_str(&format!("- {kind}：{state}\n"));
        }
    }
    report.push_str("\n存储占用（字节，当前快照）：\n");
    for (key, label) in [
        ("databaseBytes", "标注库及 WAL"),
        ("textIndexBytes", "正文索引及 WAL"),
        ("semanticIndexBytes", "语义索引及 WAL"),
    ] {
        report.push_str(&format!(
            "- {label}：{}\n",
            snapshot["storage"][key].as_u64().unwrap_or(0)
        ));
    }
    if snapshot["storage"]["previews"].is_object() {
        report.push_str(&format!(
            "- 预览缓存：{} 字节 / {} 项\n",
            snapshot["storage"]["previews"]["bytes"]
                .as_u64()
                .unwrap_or(0),
            snapshot["storage"]["previews"]["entries"]
                .as_u64()
                .unwrap_or(0)
        ));
    } else {
        report.push_str("- 预览缓存：暂不可用\n");
    }
    report.push_str("\n派生数据与恢复状态：\n");
    for (key, label) in [("textState", "正文任务"), ("semanticState", "语义任务")] {
        // Never include arbitrary service error text in exported reports.
        let state = match snapshot["derived"][key].as_str().unwrap_or("") {
            "active" => "运行中",
            "completed" => "完成",
            "idle" => "空闲",
            "disabled" => "关闭",
            "paused" => "暂停",
            "cancelled" => "取消",
            "interrupted" => "中断",
            "running" => "处理中",
            "failed" => "失败",
            _ => "未知",
        };
        report.push_str(&format!("- {label}：{state}\n"));
    }
    report.push_str(&format!(
        "- 正文已索引：{}；失败：{}\n- 阅读状态：{}（最多 500 条）\n- 待完成恢复：{}\n",
        snapshot["derived"]["textDocuments"].as_u64().unwrap_or(0),
        snapshot["derived"]["textFailed"].as_u64().unwrap_or(0),
        snapshot["derived"]["readingStates"].as_u64().unwrap_or(0),
        if snapshot["derived"]["restorePending"].as_bool() == Some(true) {
            "是"
        } else {
            "否"
        }
    ));
    report.push_str("\n此报告仅包含版本、计数和任务状态，不包含文件名、文件路径、正文、备注、标签名称、AI 服务地址或密钥。\n报告由用户在本机导出，不会自动上传。\n");
    report
}
pub fn export_report(path: &Path, text: &str) -> Result<Value> {
    if path.extension().and_then(|s| s.to_str()) != Some("md") {
        return Err(err("INVALID_INPUT", "诊断报告扩展名应为 .md"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| err("INVALID_INPUT", "请选择保存位置"))?;
    let mut output =
        tempfile::NamedTempFile::new_in(parent).map_err(|e| err("DIAGNOSTICS_FAILED", e))?;
    output
        .write_all(text.as_bytes())
        .map_err(|e| err("DIAGNOSTICS_FAILED", e))?;
    output
        .as_file()
        .sync_all()
        .map_err(|e| err("DIAGNOSTICS_FAILED", e))?;
    output
        .persist_noclobber(path)
        .map_err(|e| err("DIAGNOSTICS_FAILED", format!("{e}；请选择未使用的文件名")))?;
    Ok(json!({"path":path.to_string_lossy()}))
}
