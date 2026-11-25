// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

use crate::error::Result;
use chrono::Local;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

/// Default block size for chunked synchronization (128 KiB, within 64–256 KiB spec).
pub const CHUNK_SIZE: usize = 128 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChunkInfo {
    pub index: usize,
    pub offset: u64,
    pub len: usize,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEntry {
    pub rel_path: String,
    pub size: u64,
    pub mtime: i64,
    pub file_hash: String,
    pub chunks: Vec<ChunkInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MerkleTree {
    pub root_hash: String,
    pub files: BTreeMap<String, FileEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct TreeDiff {
    /// Relative paths and chunk indices that differ or are missing on `target` compared to `source`.
    pub chunks_to_fetch: Vec<(String, usize)>,
    /// Files present in `target` and `source` that were modified concurrently (conflict candidates).
    pub conflicts: Vec<String>,
}

/// Compute SHA-256 hex digest of a byte slice.
pub fn hash_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Compute SHA-256 hex digest of an entire file.
pub fn hash_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK_SIZE];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Build a Merkle tree over `root_dir`, hashing files in 128 KiB chunks.
pub fn build_merkle_tree(root_dir: &Path) -> Result<MerkleTree> {
    let mut files = BTreeMap::new();

    if !root_dir.exists() {
        return Ok(MerkleTree {
            root_hash: hash_bytes(b"empty"),
            files,
        });
    }

    for entry in WalkDir::new(root_dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let rel = match path.strip_prefix(root_dir) {
            Ok(p) => normalize_rel_path(p),
            Err(_) => continue,
        };
        if rel.contains(".sync-conflict-") {
            continue;
        }

        let meta = entry.metadata()?;
        let size = meta.len();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let (file_hash, chunks) = hash_file_chunks(path)?;
        files.insert(
            rel.clone(),
            FileEntry {
                rel_path: rel,
                size,
                mtime,
                file_hash,
                chunks,
            },
        );
    }

    let mut root_hasher = Sha256::new();
    for (rel, entry) in &files {
        root_hasher.update(rel.as_bytes());
        root_hasher.update(entry.file_hash.as_bytes());
    }
    let root_hash = format!("{:x}", root_hasher.finalize());

    Ok(MerkleTree { root_hash, files })
}

fn hash_file_chunks(path: &Path) -> Result<(String, Vec<ChunkInfo>)> {
    let mut file = File::open(path)?;
    let mut file_hasher = Sha256::new();
    let mut chunks = Vec::new();
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut offset = 0u64;
    let mut index = 0usize;

    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        let slice = &buf[..n];
        file_hasher.update(slice);
        chunks.push(ChunkInfo {
            index,
            offset,
            len: n,
            hash: hash_bytes(slice),
        });
        offset += n as u64;
        index += 1;
    }

    Ok((format!("{:x}", file_hasher.finalize()), chunks))
}

/// Summary of differences between two Merkle trees (Compare & Sync preview).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncPreview {
    pub to_download: Vec<(String, u64)>,
    pub to_upload: Vec<(String, u64)>,
    pub conflicts: Vec<String>,
}

/// Compare `remote` and `local` trees to build a delta preview before synchronizing.
pub fn preview_sync(remote: &MerkleTree, local: &MerkleTree) -> SyncPreview {
    let mut preview = SyncPreview::default();

    for (rel, r_entry) in &remote.files {
        match local.files.get(rel) {
            None => preview.to_download.push((rel.clone(), r_entry.size)),
            Some(l_entry) if l_entry.file_hash != r_entry.file_hash => {
                if l_entry.mtime > r_entry.mtime {
                    preview.conflicts.push(rel.clone());
                } else {
                    preview.to_download.push((rel.clone(), r_entry.size));
                }
            }
            _ => {}
        }
    }

    for (rel, l_entry) in &local.files {
        match remote.files.get(rel) {
            None => preview.to_upload.push((rel.clone(), l_entry.size)),
            Some(r_entry) if r_entry.file_hash != l_entry.file_hash && l_entry.mtime > r_entry.mtime => {
                preview.to_upload.push((rel.clone(), l_entry.size));
            }
            _ => {}
        }
    }

    preview
}

/// Compare `remote` (source) against `local` (target) and return only changed/missing chunks.
pub fn diff_trees(remote: &MerkleTree, local: &MerkleTree) -> TreeDiff {
    if remote.root_hash == local.root_hash {
        return TreeDiff::default();
    }

    let mut diff = TreeDiff::default();

    for (rel_path, remote_file) in &remote.files {
        match local.files.get(rel_path) {
            None => {
                if remote_file.chunks.is_empty() {
                    diff.chunks_to_fetch.push((rel_path.clone(), 0));
                } else {
                    for chunk in &remote_file.chunks {
                        diff.chunks_to_fetch.push((rel_path.clone(), chunk.index));
                    }
                }
            }
            Some(local_file) => {
                if remote_file.file_hash == local_file.file_hash {
                    continue;
                }
                // Detect potential concurrent edit if local is newer or same mtime with different hash
                if local_file.mtime > remote_file.mtime {
                    diff.conflicts.push(rel_path.clone());
                }
                for (idx, r_chunk) in remote_file.chunks.iter().enumerate() {
                    let same = local_file
                        .chunks
                        .get(idx)
                        .map(|l| l.hash == r_chunk.hash)
                        .unwrap_or(false);
                    if !same {
                        diff.chunks_to_fetch.push((rel_path.clone(), r_chunk.index));
                    }
                }
            }
        }
    }

    diff
}

/// Read a single chunk by index from a file.
pub fn read_chunk(path: &Path, chunk_index: usize) -> Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let offset = (chunk_index as u64) * (CHUNK_SIZE as u64);
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; CHUNK_SIZE];
    let n = file.read(&mut buf)?;
    buf.truncate(n);
    Ok(buf)
}

/// Write a chunk at `chunk_index` into `dest_path`, creating parent dirs if needed.
pub fn write_chunk(dest_path: &Path, chunk_index: usize, data: &[u8]) -> Result<()> {
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .open(dest_path)?;
    let offset = (chunk_index as u64) * (CHUNK_SIZE as u64);
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(data)?;
    file.flush()?;
    Ok(())
}

/// Create a timestamped conflict copy `<name>.sync-conflict-<date>.<ext>`.
pub fn create_conflict_copy(path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let stamp = Local::now().format("%Y%m%d-%H%M%S");
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = path.extension().and_then(|e| e.to_str());
    let conflict_name = match ext {
        Some(e) => format!("{stem}.sync-conflict-{stamp}.{e}"),
        None => format!("{stem}.sync-conflict-{stamp}"),
    };
    let conflict_path = path.with_file_name(conflict_name);
    fs::copy(path, &conflict_path)?;
    Ok(Some(conflict_path))
}

fn normalize_rel_path(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merkle_diff_detects_changed_chunk() {
        let dir_a = std::env::temp_dir().join(format!("liersync_a_{}", uuid::Uuid::new_v4()));
        let dir_b = std::env::temp_dir().join(format!("liersync_b_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir_a).unwrap();
        fs::create_dir_all(&dir_b).unwrap();

        let data_a = vec![b'A'; CHUNK_SIZE + 1024];
        let mut data_b = data_a.clone();
        data_b[CHUNK_SIZE + 10] = b'Z'; // only chunk 1 differs

        fs::write(dir_a.join("sample.bin"), &data_a).unwrap();
        fs::write(dir_b.join("sample.bin"), &data_b).unwrap();

        let tree_a = build_merkle_tree(&dir_a).unwrap();
        let tree_b = build_merkle_tree(&dir_b).unwrap();

        let diff = diff_trees(&tree_a, &tree_b);
        assert_eq!(diff.chunks_to_fetch, vec![("sample.bin".to_string(), 1)]);

        let _ = fs::remove_dir_all(&dir_a);
        let _ = fs::remove_dir_all(&dir_b);
    }
}
