// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};
use tokio::sync::mpsc;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct FolderChangeEvent {
    pub folder_id: String,
    pub path: PathBuf,
}

/// Lightweight cross-platform folder watcher that polls file metadata snapshots.
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
                prev = curr;
                if tx
                    .send(FolderChangeEvent {
                        folder_id: folder_id.clone(),
                        path: path.clone(),
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
