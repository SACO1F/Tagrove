//! Disposable previews only. Never traverse directories or follow reparse points.
use crate::{fsops, model::*};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};
pub const IMAGE_BUDGET: u64 = 384 * 1024 * 1024;
pub const SUMMARY_BUDGET: u64 = 128 * 1024 * 1024;
const ENTRY_BUDGET: usize = 10_000;
struct Entry {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
    summary: bool,
}
fn kind(name: &str) -> Option<bool> {
    let (digest, suffix) = name.split_once('-')?;
    if digest.len() != 64 || !digest.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match suffix {
        "420-v1.png" | "1800-v1.png" => Some(false),
        "420-v1.office-v044.json" => Some(true),
        _ => None,
    }
}
fn entries(root: &Path) -> Result<Vec<Entry>> {
    let folder = root.join("cache");
    let meta = fs::symlink_metadata(&folder).map_err(|e| err("CACHE_ERROR", e))?;
    if fsops::is_link(&meta) || !meta.is_dir() {
        return Err(err("CACHE_ERROR", "缓存目录不可用，请检查本地存储"));
    }
    let mut out = vec![];
    for (i, e) in fs::read_dir(folder)
        .map_err(|e| err("CACHE_ERROR", e))?
        .enumerate()
    {
        let e = e.map_err(|e| err("CACHE_ERROR", e))?;
        let Some(summary) = kind(&e.file_name().to_string_lossy()) else {
            continue;
        };
        let Ok(m) = fs::symlink_metadata(e.path()) else {
            continue;
        };
        if !m.is_file() || fsops::is_link(&m) {
            continue;
        }
        out.push(Entry {
            path: e.path(),
            bytes: m.len(),
            modified: m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            summary,
        });
        if i % 200 == 199 {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    Ok(out)
}
pub fn stats(root: &Path) -> Result<Value> {
    let e = entries(root)?;
    Ok(
        json!({"bytes":e.iter().map(|e|e.bytes).sum::<u64>(),"entries":e.len(),"imageBudget":IMAGE_BUDGET,"summaryBudget":SUMMARY_BUDGET,"entryBudgetPerTier":ENTRY_BUDGET}),
    )
}
pub fn maintain(root: &Path) -> Result<Value> {
    prune(root, IMAGE_BUDGET, SUMMARY_BUDGET, ENTRY_BUDGET, false)
}
pub fn clear(root: &Path) -> Result<Value> {
    prune(root, 0, 0, 0, true)
}
fn prune(root: &Path, image: u64, summary: u64, count: usize, clear: bool) -> Result<Value> {
    let mut e = entries(root)?;
    e.sort_by_key(|e| e.modified);
    let mut removed = 0;
    let mut freed = 0u64;
    for tier in [false, true] {
        let budget = if tier { summary } else { image };
        let mut total: u64 = e
            .iter()
            .filter(|e| e.summary == tier)
            .map(|e| e.bytes)
            .sum();
        let mut n = e.iter().filter(|e| e.summary == tier).count();
        for entry in e.iter().filter(|e| e.summary == tier) {
            if !clear && total <= budget && n <= count {
                break;
            }
            // Recheck leaf immediately before deletion; originals/unknown files stay untouched.
            let Ok(m) = fs::symlink_metadata(&entry.path) else {
                continue;
            };
            if !m.is_file()
                || fsops::is_link(&m)
                || m.len() != entry.bytes
                || m.modified().ok() != Some(entry.modified)
            {
                continue;
            }
            if fs::remove_file(&entry.path).is_ok() {
                removed += 1;
                freed += entry.bytes;
                total = total.saturating_sub(entry.bytes);
                n -= 1;
            }
        }
    }
    Ok(json!({"removed":removed,"freedBytes":freed}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tier_budget_clear_preserves_unknown_files_and_directories() {
        let d = tempfile::tempdir().unwrap();
        let c = d.path().join("cache");
        fs::create_dir(&c).unwrap();
        for i in 0..4 {
            fs::write(c.join(format!("{i:064x}-420-v1.png")), [0; 10]).unwrap();
        }
        fs::write(
            c.join(format!("{:064x}-420-v1.office-v044.json", 10)),
            [0; 7],
        )
        .unwrap();
        fs::write(c.join("原文件.txt"), b"keep").unwrap();
        fs::create_dir(c.join("nested")).unwrap();
        assert_eq!(prune(d.path(), 20, 8, 100, false).unwrap()["removed"], 2);
        assert_eq!(stats(d.path()).unwrap()["bytes"], 27);
        assert_eq!(clear(d.path()).unwrap()["removed"], 3);
        assert_eq!(fs::read(c.join("原文件.txt")).unwrap(), b"keep");
        assert!(c.join("nested").is_dir());
        assert_eq!(stats(d.path()).unwrap()["bytes"], 0);
    }
    #[test]
    fn entry_budget_applies_independently_of_bytes() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir(d.path().join("cache")).unwrap();
        for i in 0..3 {
            fs::write(
                d.path().join("cache").join(format!("{i:064x}-1800-v1.png")),
                [],
            )
            .unwrap();
        }
        assert_eq!(prune(d.path(), 100, 100, 1, false).unwrap()["removed"], 2);
        assert_eq!(stats(d.path()).unwrap()["entries"], 1);
    }
}
