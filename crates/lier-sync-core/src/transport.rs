// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

use crate::clipboard::{get_clipboard_text, set_clipboard_text, ClipboardGuard};
use crate::config::SyncDirection;
use crate::discovery::{DiscoveryService, PeerInfo};
use crate::error::{LierError, Result};
use crate::identity::DeviceIdentity;
use crate::merkle::{
    build_merkle_tree, create_conflict_copy, diff_trees, hash_bytes, hash_file, preview_sync,
    read_chunk, write_chunk, SyncPreview, CHUNK_SIZE,
};
use crate::protocol::{read_frame, write_frame, Frame};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

pub const DEFAULT_P2P_PORT: u16 = 47789;

#[derive(Debug, Clone)]
pub enum CoreEvent {
    IncomingFile {
        from_peer: String,
        file_name: String,
        rel_path: Option<String>,
        saved_path: PathBuf,
        bytes: u64,
    },
    ClipboardReceived {
        from_peer: String,
        preview: String,
        full_text: String,
    },
    SyncActivity {
        folder_id: String,
        message: String,
    },
    Error(String),
}

#[derive(Clone)]
pub struct SharedFolderRegistry {
    folders: Arc<RwLock<HashMap<String, PathBuf>>>,
}

impl SharedFolderRegistry {
    pub fn new() -> Self {
        Self {
            folders: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register(&self, folder_id: String, path: PathBuf) {
        self.folders.write().await.insert(folder_id, path);
    }

    pub async fn unregister(&self, folder_id: &str) {
        self.folders.write().await.remove(folder_id);
    }

    pub async fn get(&self, folder_id: &str) -> Option<PathBuf> {
        self.folders.read().await.get(folder_id).cloned()
    }
}

impl Default for SharedFolderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Start the P2P TCP server handling Quick Send, Folder Sync, and Clipboard Push.
pub async fn start_p2p_server(
    bind_addr: SocketAddr,
    identity: DeviceIdentity,
    receive_dir: PathBuf,
    folders: SharedFolderRegistry,
    clipboard_guard: ClipboardGuard,
    discovery: DiscoveryService,
    events: mpsc::Sender<CoreEvent>,
) -> Result<SocketAddr> {
    fs::create_dir_all(&receive_dir)?;
    let listener = TcpListener::bind(bind_addr).await?;
    let local_addr = listener.local_addr()?;
    let listen_port = local_addr.port();

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    let id = identity.clone();
                    let rx_dir = receive_dir.clone();
                    let reg = folders.clone();
                    let cb = clipboard_guard.clone();
                    let disc = discovery.clone();
                    let ev = events.clone();
                    tokio::spawn(async move {
                        if let Err(e) =
                            handle_connection(stream, peer_addr, listen_port, id, rx_dir, reg, cb, disc, ev.clone())
                                .await
                        {
                            let _ = ev.send(CoreEvent::Error(e.to_string())).await;
                        }
                    });
                }
                Err(e) => {
                    tracing::warn!("P2P accept error: {e}");
                }
            }
        }
    });

    Ok(local_addr)
}

#[allow(clippy::too_many_arguments)]
async fn handle_connection(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    listen_port: u16,
    identity: DeviceIdentity,
    receive_dir: PathBuf,
    folders: SharedFolderRegistry,
    clipboard_guard: ClipboardGuard,
    discovery: DiscoveryService,
    events: mpsc::Sender<CoreEvent>,
) -> Result<()> {
    let mut peer_name = "Unknown".to_string();
    let mut active_transfers: HashMap<String, (PathBuf, Option<String>, String, u64, std::fs::File)> =
        HashMap::new();

    while let Some(frame) = read_frame(&mut stream).await? {
        match frame {
            Frame::Hello {
                device_id,
                device_name,
                os,
                p2p_port,
            } => {
                peer_name = device_name.clone();
                if let Some(port) = p2p_port {
                    discovery
                        .register_peer(PeerInfo {
                            device_id,
                            device_name,
                            os,
                            addr: SocketAddr::new(peer_addr.ip(), port),
                            last_seen: std::time::Instant::now(),
                        })
                        .await;
                }
                write_frame(
                    &mut stream,
                    &Frame::Hello {
                        device_id: identity.device_id.clone(),
                        device_name: identity.device_name.clone(),
                        os: identity.os.clone(),
                        p2p_port: Some(listen_port),
                    },
                )
                .await?;
            }
            Frame::QuickSendOffer {
                transfer_id,
                file_name,
                rel_path,
                file_size,
                sha256,
            } => {
                let rel_for_event = rel_path.clone();
                let dest_path = if let Some(rel) = rel_path {
                    let candidate = safe_join(&receive_dir, &rel)?;
                    if let Some(parent) = candidate.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    if candidate.exists() {
                        let parent = candidate.parent().unwrap_or(&receive_dir);
                        let name = candidate
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("received_file");
                        unique_dest_path(parent, name)
                    } else {
                        candidate
                    }
                } else {
                    let safe_name = Path::new(&file_name)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("received_file")
                        .to_string();
                    unique_dest_path(&receive_dir, &safe_name)
                };
                let file = File::create(&dest_path)?;
                active_transfers.insert(
                    transfer_id.clone(),
                    (dest_path, rel_for_event, sha256, file_size, file),
                );
                write_frame(
                    &mut stream,
                    &Frame::QuickSendAccept {
                        transfer_id,
                        accepted: true,
                    },
                )
                .await?;
            }
            Frame::FileChunk {
                transfer_id,
                offset: _,
                data,
            } => {
                if let Some((_, _, _, _, file)) = active_transfers.get_mut(&transfer_id) {
                    use std::io::Write;
                    file.write_all(&data)?;
                }
            }
            Frame::QuickSendDone { transfer_id } => {
                if let Some((dest_path, rel_path, expected_sha, expected_size, mut file)) =
                    active_transfers.remove(&transfer_id)
                {
                    use std::io::Write;
                    file.flush()?;
                    drop(file);
                    let actual_sha = hash_file(&dest_path)?;
                    if !expected_sha.is_empty() && actual_sha != expected_sha {
                        return Err(LierError::IntegrityMismatch(
                            dest_path.display().to_string(),
                        ));
                    }
                    write_frame(
                        &mut stream,
                        &Frame::Ack {
                            message: "File received".into(),
                        },
                    )
                    .await?;
                    let _ = events
                        .send(CoreEvent::IncomingFile {
                            from_peer: peer_name.clone(),
                            file_name: dest_path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("file")
                                .to_string(),
                            rel_path,
                            saved_path: dest_path,
                            bytes: expected_size,
                        })
                        .await;
                }
            }
            Frame::SyncTreeRequest { folder_id } => {
                if let Some(dir) = folders.get(&folder_id).await {
                    let tree = build_merkle_tree(&dir)?;
                    write_frame(&mut stream, &Frame::SyncTreeResponse { folder_id, tree }).await?;
                } else {
                    write_frame(
                        &mut stream,
                        &Frame::Error {
                            message: format!("Unknown shared folder: {folder_id}"),
                        },
                    )
                    .await?;
                }
            }
            Frame::SyncChunkRequest {
                folder_id,
                rel_path,
                chunk_index,
            } => {
                if let Some(dir) = folders.get(&folder_id).await {
                    let full_path = safe_join(&dir, &rel_path)?;
                    let data = if full_path.exists() {
                        read_chunk(&full_path, chunk_index)?
                    } else {
                        Vec::new()
                    };
                    let file_size = fs::metadata(&full_path).map(|m| m.len()).unwrap_or(0);
                    let total_chunks = ((file_size as usize) + CHUNK_SIZE - 1) / CHUNK_SIZE.max(1);
                    write_frame(
                        &mut stream,
                        &Frame::SyncChunkData {
                            folder_id,
                            rel_path,
                            chunk_index,
                            total_chunks: total_chunks.max(1),
                            file_mtime: 0,
                            data,
                        },
                    )
                    .await?;
                }
            }
            Frame::SyncChunkData {
                folder_id,
                rel_path,
                chunk_index,
                data,
                ..
            } => {
                if let Some(dir) = folders.get(&folder_id).await {
                    let dest = safe_join(&dir, &rel_path)?;
                    write_chunk(&dest, chunk_index, &data)?;
                    write_frame(
                        &mut stream,
                        &Frame::Ack {
                            message: "Chunk stored".into(),
                        },
                    )
                    .await?;
                } else {
                    write_frame(
                        &mut stream,
                        &Frame::Error {
                            message: format!("Unknown shared folder: {folder_id}"),
                        },
                    )
                    .await?;
                }
            }
            Frame::ClipboardPush {
                sender_name,
                sha256,
                text,
                ..
            } => {
                if clipboard_guard.accept_remote(&sender_name, &text, &sha256) {
                    let _ = set_clipboard_text(&text);
                    let _ = events
                        .send(CoreEvent::ClipboardReceived {
                            from_peer: sender_name,
                            preview: text.chars().take(60).collect(),
                            full_text: text,
                        })
                        .await;
                }
                write_frame(
                    &mut stream,
                    &Frame::Ack {
                        message: "Clipboard synced".into(),
                    },
                )
                .await?;
            }
            Frame::ClipboardPullRequest => {
                let text = get_clipboard_text().unwrap_or_else(|_| {
                    clipboard_guard
                        .history()
                        .first()
                        .map(|e| e.full_text.clone())
                        .unwrap_or_default()
                });
                let sha256 = hash_bytes(text.as_bytes());
                write_frame(
                    &mut stream,
                    &Frame::ClipboardPullResponse {
                        sender_name: identity.device_name.clone(),
                        sha256,
                        text,
                    },
                )
                .await?;
            }
            _ => {}
        }
    }

    let _ = stream.shutdown().await;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct BatchItem {
    pub abs_path: PathBuf,
    pub rel_path: String,
    pub size: u64,
}

/// Expand any mix of files and folders into a flat list of `BatchItem`s for unlimited batch sending.
pub fn expand_paths_for_send(paths: &[PathBuf]) -> Result<Vec<BatchItem>> {
    let mut items = Vec::new();
    for path in paths {
        if path.is_file() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
                .to_string();
            let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            items.push(BatchItem {
                abs_path: path.clone(),
                rel_path: name,
                size,
            });
        } else if path.is_dir() {
            let root_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("folder");
            for entry in walkdir::WalkDir::new(path)
                .follow_links(false)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if !entry.file_type().is_file() {
                    continue;
                }
                if let Ok(rel) = entry.path().strip_prefix(path) {
                    let rel_str = format!(
                        "{}/{}",
                        root_name,
                        rel.to_string_lossy().replace('\\', "/")
                    );
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    items.push(BatchItem {
                        abs_path: entry.path().to_path_buf(),
                        rel_path: rel_str,
                        size,
                    });
                }
            }
        }
    }
    Ok(items)
}

/// Send an unlimited number of files/folders over a single P2P connection (Quick Send batch).
pub async fn send_quick_batch<F>(
    addr: SocketAddr,
    identity: &DeviceIdentity,
    paths: &[PathBuf],
    mut on_progress: F,
) -> Result<usize>
where
    F: FnMut(usize, usize, u64, u64, &str),
{
    let items = expand_paths_for_send(paths)?;
    if items.is_empty() {
        return Ok(0);
    }
    let total_bytes: u64 = items.iter().map(|i| i.size).sum();
    let total_files = items.len();

    let mut stream = TcpStream::connect(addr).await?;
    write_frame(
        &mut stream,
        &Frame::Hello {
            device_id: identity.device_id.clone(),
            device_name: identity.device_name.clone(),
            os: identity.os.clone(),
            p2p_port: Some(DEFAULT_P2P_PORT),
        },
    )
    .await?;
    let _ = read_frame(&mut stream).await?;

    let mut sent_bytes_all = 0u64;
    let mut buf = vec![0u8; CHUNK_SIZE];

    for (idx, item) in items.iter().enumerate() {
        let sha256 = hash_file(&item.abs_path)?;
        let transfer_id = Uuid::new_v4().to_string();
        let file_name = item
            .abs_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();

        write_frame(
            &mut stream,
            &Frame::QuickSendOffer {
                transfer_id: transfer_id.clone(),
                file_name: file_name.clone(),
                rel_path: Some(item.rel_path.clone()),
                file_size: item.size,
                sha256,
            },
        )
        .await?;

        match read_frame(&mut stream).await? {
            Some(Frame::QuickSendAccept { accepted: true, .. }) => {}
            Some(Frame::QuickSendAccept {
                accepted: false, ..
            }) => return Err(LierError::Protocol("Peer rejected transfer".into())),
            _ => return Err(LierError::Protocol("Unexpected response to offer".into())),
        }

        let mut file = File::open(&item.abs_path)?;
        let mut offset = 0u64;
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            write_frame(
                &mut stream,
                &Frame::FileChunk {
                    transfer_id: transfer_id.clone(),
                    offset,
                    data: buf[..n].to_vec(),
                },
            )
            .await?;
            offset += n as u64;
            sent_bytes_all += n as u64;
            on_progress(idx + 1, total_files, sent_bytes_all, total_bytes, &item.rel_path);
        }

        write_frame(&mut stream, &Frame::QuickSendDone { transfer_id }).await?;
        let _ = read_frame(&mut stream).await?;
    }

    Ok(total_files)
}

/// Send a single file directly to a peer (Quick Send mode).
pub async fn send_quick_file<F>(
    addr: SocketAddr,
    identity: &DeviceIdentity,
    file_path: &Path,
    mut on_progress: F,
) -> Result<()>
where
    F: FnMut(u64, u64),
{
    let meta = fs::metadata(file_path)?;
    let file_size = meta.len();
    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| LierError::Protocol("Invalid file name".into()))?
        .to_string();
    let sha256 = hash_file(file_path)?;
    let transfer_id = Uuid::new_v4().to_string();

    let mut stream = TcpStream::connect(addr).await?;
    write_frame(
        &mut stream,
        &Frame::Hello {
            device_id: identity.device_id.clone(),
            device_name: identity.device_name.clone(),
            os: identity.os.clone(),
            p2p_port: Some(DEFAULT_P2P_PORT),
        },
    )
    .await?;
    let _ = read_frame(&mut stream).await?;

    write_frame(
        &mut stream,
        &Frame::QuickSendOffer {
            transfer_id: transfer_id.clone(),
            file_name,
            rel_path: None,
            file_size,
            sha256,
        },
    )
    .await?;

    match read_frame(&mut stream).await? {
        Some(Frame::QuickSendAccept { accepted: true, .. }) => {}
        Some(Frame::QuickSendAccept {
            accepted: false, ..
        }) => return Err(LierError::Protocol("Peer rejected transfer".into())),
        _ => return Err(LierError::Protocol("Unexpected response to offer".into())),
    }

    let mut file = File::open(file_path)?;
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut offset = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        write_frame(
            &mut stream,
            &Frame::FileChunk {
                transfer_id: transfer_id.clone(),
                offset,
                data: buf[..n].to_vec(),
            },
        )
        .await?;
        offset += n as u64;
        on_progress(offset, file_size);
    }

    write_frame(&mut stream, &Frame::QuickSendDone { transfer_id }).await?;
    let _ = read_frame(&mut stream).await?;
    Ok(())
}

/// Compare local folder against a remote peer's shared folder before syncing.
pub async fn compare_folder_with_peer(
    addr: SocketAddr,
    identity: &DeviceIdentity,
    folder_id: &str,
    local_dir: &Path,
) -> Result<SyncPreview> {
    let mut stream = TcpStream::connect(addr).await?;
    write_frame(
        &mut stream,
        &Frame::Hello {
            device_id: identity.device_id.clone(),
            device_name: identity.device_name.clone(),
            os: identity.os.clone(),
            p2p_port: Some(DEFAULT_P2P_PORT),
        },
    )
    .await?;
    let _ = read_frame(&mut stream).await?;

    write_frame(
        &mut stream,
        &Frame::SyncTreeRequest {
            folder_id: folder_id.to_string(),
        },
    )
    .await?;

    let remote_tree = match read_frame(&mut stream).await? {
        Some(Frame::SyncTreeResponse { tree, .. }) => tree,
        Some(Frame::Error { message }) => return Err(LierError::Protocol(message)),
        _ => return Err(LierError::Protocol("Expected SyncTreeResponse".into())),
    };

    let local_tree = build_merkle_tree(local_dir)?;
    Ok(preview_sync(&remote_tree, &local_tree))
}

/// Pull changed chunks from a remote peer's shared folder into `local_dir` using Merkle diff.
pub async fn sync_folder_from_peer(
    addr: SocketAddr,
    identity: &DeviceIdentity,
    folder_id: &str,
    local_dir: &Path,
) -> Result<usize> {
    fs::create_dir_all(local_dir)?;
    let mut stream = TcpStream::connect(addr).await?;

    write_frame(
        &mut stream,
        &Frame::Hello {
            device_id: identity.device_id.clone(),
            device_name: identity.device_name.clone(),
            os: identity.os.clone(),
            p2p_port: Some(DEFAULT_P2P_PORT),
        },
    )
    .await?;
    let _ = read_frame(&mut stream).await?;

    write_frame(
        &mut stream,
        &Frame::SyncTreeRequest {
            folder_id: folder_id.to_string(),
        },
    )
    .await?;

    let remote_tree = match read_frame(&mut stream).await? {
        Some(Frame::SyncTreeResponse { tree, .. }) => tree,
        Some(Frame::Error { message }) => return Err(LierError::Protocol(message)),
        _ => return Err(LierError::Protocol("Expected SyncTreeResponse".into())),
    };

    let local_tree = build_merkle_tree(local_dir)?;
    let diff = diff_trees(&remote_tree, &local_tree);

    for conflict_rel in &diff.conflicts {
        if let Ok(path) = safe_join(local_dir, conflict_rel) {
            let _ = create_conflict_copy(&path);
        }
    }

    let mut fetched = 0usize;
    for (rel_path, chunk_index) in &diff.chunks_to_fetch {
        write_frame(
            &mut stream,
            &Frame::SyncChunkRequest {
                folder_id: folder_id.to_string(),
                rel_path: rel_path.clone(),
                chunk_index: *chunk_index,
            },
        )
        .await?;

        if let Some(Frame::SyncChunkData { data, .. }) = read_frame(&mut stream).await? {
            let dest = safe_join(local_dir, rel_path)?;
            write_chunk(&dest, *chunk_index, &data)?;
            fetched += 1;
        }
    }

    Ok(fetched)
}

/// Push changed chunks from `local_dir` to a remote peer's shared folder.
pub async fn push_folder_to_peer(
    addr: SocketAddr,
    identity: &DeviceIdentity,
    folder_id: &str,
    local_dir: &Path,
) -> Result<usize> {
    if !local_dir.exists() {
        return Ok(0);
    }
    let mut stream = TcpStream::connect(addr).await?;
    write_frame(
        &mut stream,
        &Frame::Hello {
            device_id: identity.device_id.clone(),
            device_name: identity.device_name.clone(),
            os: identity.os.clone(),
            p2p_port: Some(DEFAULT_P2P_PORT),
        },
    )
    .await?;
    let _ = read_frame(&mut stream).await?;

    write_frame(
        &mut stream,
        &Frame::SyncTreeRequest {
            folder_id: folder_id.to_string(),
        },
    )
    .await?;

    let remote_tree = match read_frame(&mut stream).await? {
        Some(Frame::SyncTreeResponse { tree, .. }) => tree,
        Some(Frame::Error { message }) => return Err(LierError::Protocol(message)),
        _ => return Err(LierError::Protocol("Expected SyncTreeResponse".into())),
    };

    let local_tree = build_merkle_tree(local_dir)?;
    let diff = diff_trees(&local_tree, &remote_tree);
    let mut pushed = 0usize;

    for (rel_path, chunk_index) in &diff.chunks_to_fetch {
        let full_path = safe_join(local_dir, rel_path)?;
        if !full_path.exists() {
            continue;
        }
        let data = read_chunk(&full_path, *chunk_index)?;
        let file_size = fs::metadata(&full_path).map(|m| m.len()).unwrap_or(0);
        let total_chunks = (((file_size as usize) + CHUNK_SIZE - 1) / CHUNK_SIZE.max(1)).max(1);
        write_frame(
            &mut stream,
            &Frame::SyncChunkData {
                folder_id: folder_id.to_string(),
                rel_path: rel_path.clone(),
                chunk_index: *chunk_index,
                total_chunks,
                file_mtime: 0,
                data,
            },
        )
        .await?;
        match read_frame(&mut stream).await? {
            Some(Frame::Ack { .. }) => pushed += 1,
            Some(Frame::Error { message }) => return Err(LierError::Protocol(message)),
            _ => {}
        }
    }

    Ok(pushed)
}

/// Synchronize a folder according to its configured `SyncDirection`.
pub async fn sync_folder_by_direction(
    addr: SocketAddr,
    identity: &DeviceIdentity,
    folder_id: &str,
    local_dir: &Path,
    direction: SyncDirection,
) -> Result<usize> {
    match direction {
        SyncDirection::ReceiveOnly => sync_folder_from_peer(addr, identity, folder_id, local_dir).await,
        SyncDirection::SendOnly => push_folder_to_peer(addr, identity, folder_id, local_dir).await,
        SyncDirection::Bidirectional => {
            let pulled = sync_folder_from_peer(addr, identity, folder_id, local_dir).await?;
            let pushed = push_folder_to_peer(addr, identity, folder_id, local_dir).await?;
            Ok(pulled + pushed)
        }
    }
}

/// Measure round-trip connectivity and retrieve remote peer name/OS via `Hello` handshake.
pub async fn ping_peer(addr: SocketAddr, identity: &DeviceIdentity) -> Result<(String, String, u128)> {
    let started = std::time::Instant::now();
    let mut stream = tokio::time::timeout(
        std::time::Duration::from_secs(4),
        TcpStream::connect(addr),
    )
    .await
    .map_err(|_| LierError::Protocol(format!("Таймаут подключения к {addr}")))??;

    write_frame(
        &mut stream,
        &Frame::Hello {
            device_id: identity.device_id.clone(),
            device_name: identity.device_name.clone(),
            os: identity.os.clone(),
            p2p_port: Some(DEFAULT_P2P_PORT),
        },
    )
    .await?;

    match tokio::time::timeout(std::time::Duration::from_secs(4), read_frame(&mut stream)).await {
        Ok(Ok(Some(Frame::Hello { device_name, os, .. }))) => {
            let ms = started.elapsed().as_millis();
            Ok((device_name, os, ms))
        }
        Ok(Ok(_)) => Err(LierError::Protocol("Неожиданный ответ от узла".into())),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(LierError::Protocol("Таймаут ответа от узла".into())),
    }
}

/// Pull the current clipboard text from a remote peer on demand.
pub async fn fetch_clipboard_from_peer(
    addr: SocketAddr,
    identity: &DeviceIdentity,
) -> Result<(String, String, String)> {
    let mut stream = tokio::time::timeout(
        std::time::Duration::from_secs(4),
        TcpStream::connect(addr),
    )
    .await
    .map_err(|_| LierError::Protocol(format!("Таймаут подключения к {addr}")))??;

    write_frame(
        &mut stream,
        &Frame::Hello {
            device_id: identity.device_id.clone(),
            device_name: identity.device_name.clone(),
            os: identity.os.clone(),
            p2p_port: Some(DEFAULT_P2P_PORT),
        },
    )
    .await?;
    let _ = read_frame(&mut stream).await?;

    write_frame(&mut stream, &Frame::ClipboardPullRequest).await?;
    match read_frame(&mut stream).await? {
        Some(Frame::ClipboardPullResponse {
            sender_name,
            sha256,
            text,
        }) => Ok((sender_name, sha256, text)),
        Some(Frame::Error { message }) => Err(LierError::Protocol(message)),
        _ => Err(LierError::Protocol("Неожиданный ответ на запрос буфера".into())),
    }
}

/// Push clipboard text to a peer.
pub async fn send_clipboard_text(
    addr: SocketAddr,
    identity: &DeviceIdentity,
    text: &str,
) -> Result<()> {
    let mut stream = TcpStream::connect(addr).await?;
    let sha256 = hash_bytes(text.as_bytes());
    write_frame(
        &mut stream,
        &Frame::ClipboardPush {
            sender_id: identity.device_id.clone(),
            sender_name: identity.device_name.clone(),
            sha256,
            text: text.to_string(),
        },
    )
    .await?;
    let _ = read_frame(&mut stream).await?;
    Ok(())
}

fn unique_dest_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = Path::new(name).extension().and_then(|e| e.to_str());
    for i in 1..1000 {
        let numbered = match ext {
            Some(e) => format!("{stem}_{i}.{e}"),
            None => format!("{stem}_{i}"),
        };
        let p = dir.join(numbered);
        if !p.exists() {
            return p;
        }
    }
    dir.join(format!("{}_{}", Uuid::new_v4(), name))
}

fn safe_join(root: &Path, rel: &str) -> Result<PathBuf> {
    let clean = Path::new(rel);
    for comp in clean.components() {
        if matches!(comp, std::path::Component::ParentDir | std::path::Component::RootDir) {
            return Err(LierError::Protocol(format!("Unsafe relative path: {rel}")));
        }
    }
    Ok(root.join(clean))
}
