// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

use crate::config::AppLanguage;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};
use tokio::sync::mpsc;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct FolderChangeEvent {
    pub folder_id: String,
    pub path: PathBuf,
    pub added: Vec<String>,
    pub modified: Vec<String>,
    pub removed: Vec<String>,
}

impl FolderChangeEvent {
    pub fn format_alert(&self, lang: AppLanguage) -> String {
        let mut parts = Vec::new();
        if !self.added.is_empty() {
            parts.push(match lang {
                AppLanguage::Ru => format!("+{} добавл.", self.added.len()),
                AppLanguage::En => format!("+{} added", self.added.len()),
            });
        }
        if !self.modified.is_empty() {
            parts.push(match lang {
                AppLanguage::Ru => format!("~{} измен.", self.modified.len()),
                AppLanguage::En => format!("~{} modified", self.modified.len()),
            });
        }
        if !self.removed.is_empty() {
            parts.push(match lang {
                AppLanguage::Ru => format!("-{} удал.", self.removed.len()),
                AppLanguage::En => format!("-{} deleted", self.removed.len()),
            });
        }
        let sample = self
            .removed
            .first()
            .or_else(|| self.modified.first())
            .or_else(|| self.added.first())
            .cloned()
            .unwrap_or_default();
        let detail = parts.join(", ");
        if sample.is_empty() {
            detail
        } else {
            format!("{detail} ({sample})")
        }
    }
}

pub fn diff_snapshots(
    prev: &BTreeMap<String, (u64, i64)>,
    curr: &BTreeMap<String, (u64, i64)>,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut added = Vec::new();
    let mut modified = Vec::new();
    let mut removed = Vec::new();
    for (k, v) in curr {
        match prev.get(k) {
            None => added.push(k.clone()),
            Some(old_v) if old_v != v => modified.push(k.clone()),
            _ => {}
        }
    }
    for k in prev.keys() {
        if !curr.contains_key(k) {
            removed.push(k.clone());
        }
    }
    (added, modified, removed)
}

/// Lightweight cross-platform folder watcher that polls file metadata snapshots
/// and reports additions, edits, and deletions.
pub fn spawn_folder_watcher(
    folder_id: String,
    path: PathBuf,
    interval: Duration,
    tx: mpsc::Sender<FolderChangeEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut prev = snapshot_dir(&path);
        loop {
            tokio::time::sleep(interval).await;
            let curr = snapshot_dir(&path);
            if curr != prev {
                let (added, modified, removed) = diff_snapshots(&prev, &curr);
                prev = curr;
                if tx
                    .send(FolderChangeEvent {
                        folder_id: folder_id.clone(),
                        path: path.clone(),
                        added,
                        modified,
                        removed,
                    })
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    })
}

fn snapshot_dir(root: &Path) -> BTreeMap<String, (u64, i64)> {
    let mut map = BTreeMap::new();
    if !root.exists() {
        return map;
    }
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(root) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if rel_str.contains(".sync-conflict-") {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            map.insert(rel_str, (meta.len(), mtime));
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_snapshot_detects_added_modified_removed() {
        let mut prev = BTreeMap::new();
        prev.insert("keep.txt".to_string(), (10, 100));
        prev.insert("edit.txt".to_string(), (20, 100));
        prev.insert("gone.txt".to_string(), (30, 100));

        let mut curr = BTreeMap::new();
        curr.insert("keep.txt".to_string(), (10, 100));
        curr.insert("edit.txt".to_string(), (25, 105));
        curr.insert("new.txt".to_string(), (40, 110));

        let (added, modified, removed) = diff_snapshots(&prev, &curr);
        assert_eq!(added, vec!["new.txt"]);
        assert_eq!(modified, vec!["edit.txt"]);
        assert_eq!(removed, vec!["gone.txt"]);
    }
}
