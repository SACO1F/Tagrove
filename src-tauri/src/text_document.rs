//! Whole UTF-8 documents; explicit, revision-checked atomic Markdown saves.
use crate::{db::Store, fsops, model::*};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};
const MAX: usize = 1024 * 1024;
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn source(f: &FileRecord) -> Result<(File, Vec<u8>)> {
    if !matches!(f.extension.as_str(), "md" | "markdown" | "txt" | "log") {
        return Err(err("PREVIEW_UNSUPPORTED", "此格式不支持文本阅读"));
    }
    if f.status != "available" {
        return Err(err("FILE_UNAVAILABLE", "文件不可用，请刷新状态"));
    }
    let file = File::open(&f.path).map_err(|e| err("ACCESS_DENIED", e))?;
    let meta = file.metadata().map_err(|e| err("ACCESS_DENIED", e))?;
    if fsops::identity(&file) != f.identity || fsops::revision(&meta) != f.revision {
        return Err(err("FILE_CHANGED", "文件已变化，请重新读取"));
    }
    let mut bytes = Vec::new();
    (&file)
        .take(MAX as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| err("ACCESS_DENIED", e))?;
    if bytes.len() > MAX {
        return Err(err("PREVIEW_TOO_LARGE", "文本超过 1 MiB，请打开原文件"));
    }
    Ok((file, bytes))
}
pub fn read(f: &FileRecord) -> Result<Value> {
    let (file, bytes) = source(f)?;
    let text = std::str::from_utf8(bytes.strip_prefix(&[239, 187, 191]).unwrap_or(&bytes))
        .map_err(|_| {
            err(
                "TEXT_ENCODING",
                "此阅读器支持 UTF-8 文本，请使用原程序转换编码",
            )
        })?;
    if fsops::revision(&file.metadata().map_err(|e| err("ACCESS_DENIED", e))?) != f.revision {
        return Err(err("FILE_CHANGED", "读取期间文件已变化，请重试"));
    }
    Ok(json!({"text":text,"revision":f.revision,"digest":hash(&bytes)}))
}
impl Store {
    pub fn save_markdown(&mut self, v: &Value) -> Result<Value> {
        let id = str_arg(v, "id")?;
        let f = self.refresh_file(id)?;
        if !matches!(f.extension.as_str(), "md" | "markdown") {
            return Err(err("EDIT_UNSUPPORTED", "仅 Markdown 支持内置编辑"));
        }
        if str_arg(v, "revision")? != f.revision {
            return Err(err(
                "FILE_CHANGED",
                "原文件已在外部修改；编辑草稿已保留，请核对后重新读取",
            ));
        }
        let (guard, old) = source(&f)?;
        if guard
            .metadata()
            .map_err(|e| err("ACCESS_DENIED", e))?
            .permissions()
            .readonly()
        {
            return Err(err("ACCESS_DENIED", "原文件为只读，尚未写入"));
        }
        if str_arg(v, "digest")? != hash(&old) {
            return Err(err("FILE_CHANGED", "原文件内容已变化；编辑草稿已保留"));
        }
        let old_text = std::str::from_utf8(old.strip_prefix(&[239, 187, 191]).unwrap_or(&old))
            .map_err(|_| err("TEXT_ENCODING", "仅支持 UTF-8 编辑"))?;
        let mut text = str_arg(v, "text")?.to_owned();
        if old_text.contains("\r\n") && !old_text.replace("\r\n", "").contains('\n') {
            text = text.replace("\r\n", "\n").replace('\n', "\r\n");
        }
        let mut bytes = if old.starts_with(&[239, 187, 191]) {
            vec![239, 187, 191]
        } else {
            vec![]
        };
        bytes.extend_from_slice(text.as_bytes());
        if bytes.len() > MAX {
            return Err(err(
                "PREVIEW_TOO_LARGE",
                "编辑内容超过 1 MiB，尚未写入原文件",
            ));
        }
        if old != bytes {
            let parent = Path::new(&f.path)
                .parent()
                .ok_or_else(|| err("INVALID_PATH", "文件路径无效"))?;
            let mut temp = tempfile::Builder::new()
                .prefix(".shiqian-edit-")
                .suffix(".tmp")
                .tempfile_in(parent)
                .map_err(|e| err("SAVE_FAILED", e))?;
            temp.write_all(&bytes)
                .and_then(|_| temp.as_file().sync_all())
                .map_err(|e| err("SAVE_FAILED", e))?;
            let latest = fsops::inspect(Path::new(&f.path))?;
            if latest.identity != f.identity || latest.revision != f.revision {
                return Err(err("FILE_CHANGED", "保存前原文件已变化，草稿已保留"));
            }
            // A final content check also catches equal-size edits with restored timestamps.
            let (_, check) = source(&f)?;
            if hash(&check) != hash(&old) {
                return Err(err("FILE_CHANGED", "保存前原文件内容已变化，草稿已保留"));
            }
            drop(guard);
            temp.persist(&f.path)
                .map_err(|e| err("SAVE_FAILED", e.error))?;
            let m = fsops::inspect(Path::new(&f.path))?;
            self.conn.execute("UPDATE files SET bytes=?,modified=?,revision=?,identity=?,version=version+1 WHERE id=?", rusqlite::params![m.bytes,m.modified,m.revision,m.identity,id]).map_err(sql_err)?;
        }
        let fresh = self.file(id)?;
        read(&fresh)
    }
}
