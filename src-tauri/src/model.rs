use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};
use unicode_casefold::UnicodeCaseFold;
use unicode_normalization::UnicodeNormalization;

pub type Result<T> = std::result::Result<T, String>;
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
pub fn key(s: &str) -> String {
    s.nfc().collect::<String>().case_fold().collect()
}
pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub fn err(code: &str, message: impl std::fmt::Display) -> String {
    format!("{code}: {message}")
}
pub fn sql_err(e: rusqlite::Error) -> String {
    err("DATABASE_ERROR", e)
}
pub fn str_arg<'a>(v: &'a Value, name: &str) -> Result<&'a str> {
    v.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| err("INVALID_INPUT", format!("缺少参数 {name}")))
}
pub fn string_list(v: &Value, name: &str) -> Vec<String> {
    v.get(name)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    pub id: String,
    pub name: String,
    pub count: i64,
    pub version: i64,
    #[serde(default)]
    pub created_by: String,
    #[serde(default)]
    pub accepted: bool,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub ai: Option<AiProvenance>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProvenance {
    pub model: String,
    pub reason: String,
    pub updated_at: i64,
    pub confirmed: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTask {
    pub status: String,
    pub error: String,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRecord {
    pub id: String,
    pub path: String,
    pub parent: String,
    pub name: String,
    pub extension: String,
    pub kind: String,
    pub bytes: i64,
    pub modified: i64,
    pub added: i64,
    pub status: String,
    pub error: String,
    pub favorite: bool,
    pub version: i64,
    pub note: String,
    pub note_version: i64,
    pub revision: String,
    pub identity: String,
    pub tags: Vec<Tag>,
    pub ai_task: Option<AiTask>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_match: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_match: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_source: Option<crate::capture::Source>,
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Query {
    pub scope: String,
    pub text: String,
    pub search_mode: String,
    pub search_scope: String,
    pub semantic_session: String,
    #[serde(skip)]
    pub ranked_ids: Option<Vec<String>>,
    #[serde(skip)]
    pub supplemental_ids: Option<Vec<String>>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub mode: String,
    pub kinds: Vec<String>,
    pub status: String,
    pub directory: String,
    pub source_host: String,
    pub capture_kind: String,
    pub recursive: bool,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub sort: String,
    pub direction: String,
    pub offset: usize,
    pub limit: usize,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportJob {
    pub id: String,
    pub discovered: usize,
    pub processed: usize,
    pub added: usize,
    #[serde(default)]
    pub added_ids: Vec<String>,
    pub existing: usize,
    pub skipped: usize,
    pub failed: usize,
    pub errors: Vec<String>,
    #[serde(default)]
    pub failed_paths: Vec<String>,
    pub done: bool,
    pub cancelled: bool,
}

pub fn json_ok() -> Value {
    json!({"ok": true})
}
