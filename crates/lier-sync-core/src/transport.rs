// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

use crate::clipboard::{
    ClipboardGuard, get_clipboard_image_png, get_clipboard_text, set_clipboard_image_png,
    set_clipboard_text,
};
use crate::config::{SyncDirection, TrustedPeer};
use crate::discovery::{DiscoveryService, PeerInfo};
use crate::error::{LierError, Result};
use crate::identity::DeviceIdentity;
use crate::merkle::{
    CHUNK_SIZE, SyncPreview, build_merkle_tree, create_conflict_copy, diff_trees, hash_bytes,
    hash_file, preview_sync, read_chunk, write_chunk,
};
use crate::protocol::{Frame, read_frame_with, write_frame_with};
use crate::security::{
    SecurityManager, SecurityPolicy, SessionCipher, StreamRole, derive_shared_secret,
    generate_ephemeral_keypair,
};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{RwLock, mpsc};
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
    ClipboardImageReceived {
        from_peer: String,
        width: u32,
        height: u32,
        png_bytes: Vec<u8>,
        sha256: String,
    },
    SyncActivity {
        folder_id: String,
        message: String,
    },
    PeerPaired(TrustedPeer),
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

/// Start the P2P TCP server with default permissive security policy.
pub async fn start_p2p_server(
    bind_addr: SocketAddr,
    identity: DeviceIdentity,
    receive_dir: PathBuf,
    folders: SharedFolderRegistry,
    clipboard_guard: ClipboardGuard,
    discovery: DiscoveryService,
    events: mpsc::Sender<CoreEvent>,
) -> Result<SocketAddr> {
    start_p2p_server_secure(
        bind_addr,
        identity,
        receive_dir,
        folders,
        clipboard_guard,
        discovery,
        SecurityManager::new(SecurityPolicy::default()),
        events,
    )
    .await
}

/// Start the P2P TCP server with explicit `SecurityManager` (X25519+ChaCha20 & trusted whitelist).
#[allow(clippy::too_many_arguments)]
pub async fn start_p2p_server_secure(
    bind_addr: SocketAddr,
    identity: DeviceIdentity,
    receive_dir: PathBuf,
    folders: SharedFolderRegistry,
    clipboard_guard: ClipboardGuard,
    discovery: DiscoveryService,
    security: SecurityManager,
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
                    let sec = security.clone();
                    let ev = events.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(
                            stream,
                            peer_addr,
                            listen_port,
                            id,
                            rx_dir,
                            reg,
                            cb,
                            disc,
                            sec,
                            ev.clone(),
                        )
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
    security: SecurityManager,
    events: mpsc::Sender<CoreEvent>,
) -> Result<()> {
    let mut peer_name = "Unknown".to_string();
    let mut peer_trusted = !security.whitelist_only();
    let mut cipher = SessionCipher::Plain;
    let mut active_transfers: HashMap<
        String,
        (PathBuf, Option<String>, String, u64, std::fs::File),
    > = HashMap::new();

    while let Some(frame) = read_frame_with(&mut stream, &mut cipher).await? {
        match frame {
            Frame::KeyExchange { public_key } => {
                if security.encrypt_enabled() {
                    let (sec_key, pub_key) = generate_ephemeral_keypair();
                    write_frame_with(
                        &mut stream,
                        &Frame::KeyExchange {
                            public_key: pub_key,
                        },
                        &mut cipher,
                    )
                    .await?;
                    let shared = derive_shared_secret(sec_key, &public_key);
                    cipher = SessionCipher::from_shared_secret(
                        &shared,
                        &public_key,
                        &pub_key,
                        StreamRole::Server,
                    );
                } else {
                    write_frame_with(
                        &mut stream,
                        &Frame::Ack {
                            message: "plain".into(),
                        },
                        &mut cipher,
                    )
                    .await?;
                }
            }
            Frame::Hello {
                device_id,
                device_name,
                os,
                fingerprint,
                p2p_port,
            } => {
                peer_name = device_name.clone();
                if security.whitelist_only() {
                    peer_trusted = security.is_trusted(&device_id, &fingerprint);
                } else {
                    peer_trusted = true;
                }
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
                write_frame_with(
                    &mut stream,
                    &Frame::Hello {
                        device_id: identity.device_id.clone(),
                        device_name: identity.device_name.clone(),
                        os: identity.os.clone(),
                        fingerprint: identity.fingerprint.clone(),
                        p2p_port: Some(listen_port),
                    },
                    &mut cipher,
                )
                .await?;
            }
            Frame::PairRequest {
                device_id,
                device_name,
                fingerprint,
                pin,
            } => {
                if security.verify_pin(&pin) {
                    let paired =
                        security.add_or_update_trusted(&device_id, &device_name, &fingerprint);
                    peer_trusted = true;
                    peer_name = device_name;
                    let _ = events.send(CoreEvent::PeerPaired(paired)).await;
                    write_frame_with(
                        &mut stream,
                        &Frame::PairResponse {
                            accepted: true,
                            device_id: identity.device_id.clone(),
                            device_name: identity.device_name.clone(),
                            fingerprint: identity.fingerprint.clone(),
                            reason: "Paired".into(),
                        },
                        &mut cipher,
                    )
                    .await?;
                } else {
                    write_frame_with(
                        &mut stream,
                        &Frame::PairResponse {
                            accepted: false,
                            device_id: identity.device_id.clone(),
                            device_name: identity.device_name.clone(),
                            fingerprint: identity.fingerprint.clone(),
                            reason: "Invalid PIN".into(),
                        },
                        &mut cipher,
                    )
                    .await?;
                }
            }
            Frame::QuickSendOffer {
                transfer_id,
                file_name,
                rel_path,
                file_size,
                sha256,
            } => {
                if security.whitelist_only() && !peer_trusted {
                    write_frame_with(
                        &mut stream,
                        &Frame::QuickSendAccept {
                            transfer_id,
                            accepted: false,
                        },
                        &mut cipher,
                    )
                    .await?;
                    continue;
                }
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
                write_frame_with(
                    &mut stream,
                    &Frame::QuickSendAccept {
                        transfer_id,
                        accepted: true,
                    },
                    &mut cipher,
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
                    write_frame_with(
                        &mut stream,
                        &Frame::Ack {
                            message: "File received".into(),
                        },
                        &mut cipher,
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
                if security.whitelist_only() && !peer_trusted {
                    write_frame_with(
                        &mut stream,
                        &Frame::Error {
                            message: "Device not paired / untrusted".into(),
                        },
                        &mut cipher,
                    )
                    .await?;
                    continue;
                }
                if let Some(dir) = folders.get(&folder_id).await {
                    let tree = build_merkle_tree(&dir)?;
                    write_frame_with(
                        &mut stream,
                        &Frame::SyncTreeResponse { folder_id, tree },
                        &mut cipher,
                    )
                    .await?;
                } else {
                    write_frame_with(
                        &mut stream,
                        &Frame::Error {
                            message: format!("Unknown shared folder: {folder_id}"),
                        },
                        &mut cipher,
                    )
                    .await?;
                }
            }
            Frame::SyncChunkRequest {
                folder_id,
                rel_path,
                chunk_index,
            } => {
                if security.whitelist_only() && !peer_trusted {
                    write_frame_with(
                        &mut stream,
                        &Frame::Error {
                            message: "Device not paired / untrusted".into(),
                        },
                        &mut cipher,
                    )
                    .await?;
                    continue;
                }
                if let Some(dir) = folders.get(&folder_id).await {
                    let full_path = safe_join(&dir, &rel_path)?;
                    let data = if full_path.exists() {
                        read_chunk(&full_path, chunk_index)?
                    } else {
                        Vec::new()
                    };
                    let file_size = fs::metadata(&full_path).map(|m| m.len()).unwrap_or(0);
                    let total_chunks = ((file_size as usize) + CHUNK_SIZE - 1) / CHUNK_SIZE.max(1);
                    write_frame_with(
                        &mut stream,
                        &Frame::SyncChunkData {
                            folder_id,
                            rel_path,
                            chunk_index,
                            total_chunks: total_chunks.max(1),
                            file_mtime: 0,
                            data,
                        },
                        &mut cipher,
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
                if security.whitelist_only() && !peer_trusted {
                    write_frame_with(
                        &mut stream,
                        &Frame::Error {
                            message: "Device not paired / untrusted".into(),
                        },
                        &mut cipher,
                    )
                    .await?;
                    continue;
                }
                if let Some(dir) = folders.get(&folder_id).await {
                    let dest = safe_join(&dir, &rel_path)?;
                    write_chunk(&dest, chunk_index, &data)?;
                    write_frame_with(
                        &mut stream,
                        &Frame::Ack {
                            message: "Chunk stored".into(),
                        },
                        &mut cipher,
                    )
                    .await?;
                } else {
                    write_frame_with(
                        &mut stream,
                        &Frame::Error {
                            message: format!("Unknown shared folder: {folder_id}"),
                        },
                        &mut cipher,
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
                if security.whitelist_only() && !peer_trusted {
                    write_frame_with(
                        &mut stream,
                        &Frame::Error {
                            message: "Device not paired / untrusted".into(),
                        },
                        &mut cipher,
                    )
                    .await?;
                    continue;
                }
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
                write_frame_with(
                    &mut stream,
                    &Frame::Ack {
                        message: "Clipboard synced".into(),
                    },
                    &mut cipher,
                )
                .await?;
            }
            Frame::ClipboardImagePush {
                sender_name,
                sha256,
                width,
                height,
                png_bytes,
                ..
            } => {
                if security.whitelist_only() && !peer_trusted {
                    write_frame_with(
                        &mut stream,
                        &Frame::Error {
                            message: "Device not paired / untrusted".into(),
                        },
                        &mut cipher,
                    )
                    .await?;
                    continue;
                }
                if clipboard_guard.accept_remote_image(
                    &sender_name,
                    width,
                    height,
                    &png_bytes,
                    &sha256,
                ) {
                    let _ = set_clipboard_image_png(&png_bytes);
                    let _ = events
                        .send(CoreEvent::ClipboardImageReceived {
                            from_peer: sender_name,
                            width,
                            height,
                            png_bytes,
                            sha256,
                        })
                        .await;
                }
                write_frame_with(
                    &mut stream,
                    &Frame::Ack {
                        message: "Clipboard image synced".into(),
                    },
                    &mut cipher,
                )
                .await?;
            }
            Frame::ClipboardPullRequest => {
                if security.whitelist_only() && !peer_trusted {
                    write_frame_with(
                        &mut stream,
                        &Frame::Error {
                            message: "Device not paired / untrusted".into(),
                        },
                        &mut cipher,
                    )
                    .await?;
                    continue;
                }
                if let Some(text) = get_clipboard_text().ok().filter(|t| !t.is_empty()) {
                    let sha256 = hash_bytes(text.as_bytes());
                    write_frame_with(
                        &mut stream,
                        &Frame::ClipboardPullResponse {
                            sender_name: identity.device_name.clone(),
                            sha256,
                            text,
                        },
                        &mut cipher,
                    )
                    .await?;
                } else if let Ok((width, height, png_bytes)) = get_clipboard_image_png() {
                    let sha256 = hash_bytes(&png_bytes);
                    write_frame_with(
                        &mut stream,
                        &Frame::ClipboardImageResponse {
                            sender_name: identity.device_name.clone(),
                            sha256,
                            width,
                            height,
                            png_bytes,
                        },
                        &mut cipher,
                    )
                    .await?;
                } else if let Some(last) = clipboard_guard.history().into_iter().next() {
                    if last.is_image {
                        if let Some(png_bytes) = last.image_png {
                            write_frame_with(
                                &mut stream,
                                &Frame::ClipboardImageResponse {
                                    sender_name: identity.device_name.clone(),
                                    sha256: last.sha256,
                                    width: 0,
                                    height: 0,
                                    png_bytes,
                                },
                                &mut cipher,
                            )
                            .await?;
                            continue;
                        }
                    }
                    let sha256 = hash_bytes(last.full_text.as_bytes());
                    write_frame_with(
                        &mut stream,
                        &Frame::ClipboardPullResponse {
                            sender_name: identity.device_name.clone(),
                            sha256,
                            text: last.full_text,
                        },
                        &mut cipher,
                    )
                    .await?;
                } else {
                    write_frame_with(
                        &mut stream,
                        &Frame::ClipboardPullResponse {
                            sender_name: identity.device_name.clone(),
                            sha256: hash_bytes(b""),
                            text: String::new(),
                        },
                        &mut cipher,
                    )
                    .await?;
                }
            }
            _ => {}
        }
    }

    let _ = stream.shutdown().await;
    Ok(())
}

/// Shared client helper: connects, negotiates X25519+ChaCha20 encryption, and exchanges `Hello`.
async fn connect_secure(
    addr: SocketAddr,
    identity: &DeviceIdentity,
    timeout_secs: Option<u64>,
) -> Result<(TcpStream, SessionCipher, Option<Frame>)> {
    let mut stream = if let Some(secs) = timeout_secs {
        tokio::time::timeout(
            std::time::Duration::from_secs(secs),
            TcpStream::connect(addr),
        )
        .await
        .map_err(|_| LierError::Protocol(format!("Таймаут подключения к {addr}")))??
    } else {
        TcpStream::connect(addr).await?
    };

    let mut cipher = SessionCipher::Plain;
    let (c_sec, c_pub) = generate_ephemeral_keypair();
    write_frame_with(
        &mut stream,
        &Frame::KeyExchange { public_key: c_pub },
        &mut cipher,
    )
    .await?;

    let kex_reply = if let Some(secs) = timeout_secs {
        tokio::time::timeout(
            std::time::Duration::from_secs(secs),
            read_frame_with(&mut stream, &mut cipher),
        )
        .await
        .map_err(|_| LierError::Protocol("Таймаут ответа от узла".into()))??
    } else {
        read_frame_with(&mut stream, &mut cipher).await?
    };

    if let Some(Frame::KeyExchange { public_key: s_pub }) = kex_reply {
        let shared = derive_shared_secret(c_sec, &s_pub);
        cipher = SessionCipher::from_shared_secret(&shared, &c_pub, &s_pub, StreamRole::Client);
    }

    write_frame_with(
        &mut stream,
        &Frame::Hello {
            device_id: identity.device_id.clone(),
            device_name: identity.device_name.clone(),
            os: identity.os.clone(),
            fingerprint: identity.fingerprint.clone(),
            p2p_port: Some(DEFAULT_P2P_PORT),
        },
        &mut cipher,
    )
    .await?;

    let hello_reply = if let Some(secs) = timeout_secs {
        tokio::time::timeout(
            std::time::Duration::from_secs(secs),
            read_frame_with(&mut stream, &mut cipher),
        )
        .await
        .map_err(|_| LierError::Protocol("Таймаут ответа от узла".into()))??
    } else {
        read_frame_with(&mut stream, &mut cipher).await?
    };

    Ok((stream, cipher, hello_reply))
}

/// Pair with a remote peer using its 6-digit PIN (or QR PIN), returning the peer's trusted identity.
pub async fn pair_with_peer(
    addr: SocketAddr,
    identity: &DeviceIdentity,
    pin: &str,
) -> Result<TrustedPeer> {
    let (mut stream, mut cipher, _) = connect_secure(addr, identity, Some(5)).await?;
    write_frame_with(
        &mut stream,
        &Frame::PairRequest {
            device_id: identity.device_id.clone(),
            device_name: identity.device_name.clone(),
            fingerprint: identity.fingerprint.clone(),
            pin: pin.trim().to_string(),
        },
        &mut cipher,
    )
    .await?;

    match read_frame_with(&mut stream, &mut cipher).await? {
        Some(Frame::PairResponse {
            accepted: true,
            device_id,
            device_name,
            fingerprint,
            ..
        }) => Ok(TrustedPeer {
            device_id,
            device_name,
            fingerprint,
            added_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }),
        Some(Frame::PairResponse {
            accepted: false,
            reason,
            ..
        }) => Err(LierError::Protocol(reason)),
        Some(Frame::Error { message }) => Err(LierError::Protocol(message)),
        _ => Err(LierError::Protocol("Unexpected pairing response".into())),
    }
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
                    let rel_str =
                        format!("{}/{}", root_name, rel.to_string_lossy().replace('\\', "/"));
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

    let (mut stream, mut cipher, _) = connect_secure(addr, identity, None).await?;

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

        write_frame_with(
            &mut stream,
            &Frame::QuickSendOffer {
                transfer_id: transfer_id.clone(),
                file_name: file_name.clone(),
                rel_path: Some(item.rel_path.clone()),
                file_size: item.size,
                sha256,
            },
            &mut cipher,
        )
        .await?;

        match read_frame_with(&mut stream, &mut cipher).await? {
            Some(Frame::QuickSendAccept { accepted: true, .. }) => {}
            Some(Frame::QuickSendAccept {
                accepted: false, ..
            }) => {
                return Err(LierError::Protocol(
                    "Peer rejected transfer (check pairing/whitelist)".into(),
                ));
            }
            Some(Frame::Error { message }) => return Err(LierError::Protocol(message)),
            _ => return Err(LierError::Protocol("Unexpected response to offer".into())),
        }

        let mut file = File::open(&item.abs_path)?;
        let mut offset = 0u64;
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            write_frame_with(
                &mut stream,
                &Frame::FileChunk {
                    transfer_id: transfer_id.clone(),
                    offset,
                    data: buf[..n].to_vec(),
                },
                &mut cipher,
            )
            .await?;
            offset += n as u64;
            sent_bytes_all += n as u64;
            on_progress(
                idx + 1,
                total_files,
                sent_bytes_all,
                total_bytes,
                &item.rel_path,
            );
        }

        write_frame_with(
            &mut stream,
            &Frame::QuickSendDone { transfer_id },
            &mut cipher,
        )
        .await?;
        let _ = read_frame_with(&mut stream, &mut cipher).await?;
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

    let (mut stream, mut cipher, _) = connect_secure(addr, identity, None).await?;

    write_frame_with(
        &mut stream,
        &Frame::QuickSendOffer {
            transfer_id: transfer_id.clone(),
            file_name,
            rel_path: None,
            file_size,
            sha256,
        },
        &mut cipher,
    )
    .await?;

    match read_frame_with(&mut stream, &mut cipher).await? {
        Some(Frame::QuickSendAccept { accepted: true, .. }) => {}
        Some(Frame::QuickSendAccept {
            accepted: false, ..
        }) => {
            return Err(LierError::Protocol(
                "Peer rejected transfer (check pairing/whitelist)".into(),
            ));
        }
        Some(Frame::Error { message }) => return Err(LierError::Protocol(message)),
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
        write_frame_with(
            &mut stream,
            &Frame::FileChunk {
                transfer_id: transfer_id.clone(),
                offset,
                data: buf[..n].to_vec(),
            },
            &mut cipher,
        )
        .await?;
        offset += n as u64;
        on_progress(offset, file_size);
    }

    write_frame_with(
        &mut stream,
        &Frame::QuickSendDone { transfer_id },
        &mut cipher,
    )
    .await?;
    let _ = read_frame_with(&mut stream, &mut cipher).await?;
    Ok(())
}

/// Compare local folder against a remote peer's shared folder before syncing.
pub async fn compare_folder_with_peer(
    addr: SocketAddr,
    identity: &DeviceIdentity,
    folder_id: &str,
    local_dir: &Path,
) -> Result<SyncPreview> {
    let (mut stream, mut cipher, _) = connect_secure(addr, identity, Some(5)).await?;

    write_frame_with(
        &mut stream,
        &Frame::SyncTreeRequest {
            folder_id: folder_id.to_string(),
        },
        &mut cipher,
    )
    .await?;

    let remote_tree = match read_frame_with(&mut stream, &mut cipher).await? {
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
    let (mut stream, mut cipher, _) = connect_secure(addr, identity, Some(8)).await?;

    write_frame_with(
        &mut stream,
        &Frame::SyncTreeRequest {
            folder_id: folder_id.to_string(),
        },
        &mut cipher,
    )
    .await?;

    let remote_tree = match read_frame_with(&mut stream, &mut cipher).await? {
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
        write_frame_with(
            &mut stream,
            &Frame::SyncChunkRequest {
                folder_id: folder_id.to_string(),
                rel_path: rel_path.clone(),
                chunk_index: *chunk_index,
            },
            &mut cipher,
        )
        .await?;

        if let Some(Frame::SyncChunkData { data, .. }) =
            read_frame_with(&mut stream, &mut cipher).await?
        {
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
    let (mut stream, mut cipher, _) = connect_secure(addr, identity, Some(8)).await?;

    write_frame_with(
        &mut stream,
        &Frame::SyncTreeRequest {
            folder_id: folder_id.to_string(),
        },
        &mut cipher,
    )
    .await?;

    let remote_tree = match read_frame_with(&mut stream, &mut cipher).await? {
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
        write_frame_with(
            &mut stream,
            &Frame::SyncChunkData {
                folder_id: folder_id.to_string(),
                rel_path: rel_path.clone(),
                chunk_index: *chunk_index,
                total_chunks,
                file_mtime: 0,
                data,
            },
            &mut cipher,
        )
        .await?;
        match read_frame_with(&mut stream, &mut cipher).await? {
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
        SyncDirection::ReceiveOnly => {
            sync_folder_from_peer(addr, identity, folder_id, local_dir).await
        }
        SyncDirection::SendOnly => push_folder_to_peer(addr, identity, folder_id, local_dir).await,
        SyncDirection::Bidirectional => {
            let pulled = sync_folder_from_peer(addr, identity, folder_id, local_dir).await?;
            let pushed = push_folder_to_peer(addr, identity, folder_id, local_dir).await?;
            Ok(pulled + pushed)
        }
    }
}

/// Measure round-trip connectivity and retrieve remote peer name/OS via `Hello` handshake.
pub async fn ping_peer(
    addr: SocketAddr,
    identity: &DeviceIdentity,
) -> Result<(String, String, u128)> {
    let started = std::time::Instant::now();
    let (_, _, hello) = connect_secure(addr, identity, Some(4)).await?;
    match hello {
        Some(Frame::Hello {
            device_name, os, ..
        }) => {
            let ms = started.elapsed().as_millis();
            Ok((device_name, os, ms))
        }
        _ => Err(LierError::Protocol("Неожиданный ответ от узла".into())),
    }
}

#[derive(Debug, Clone)]
pub enum RemoteClipboardPayload {
    Text {
        sender_name: String,
        sha256: String,
        text: String,
    },
    Image {
        sender_name: String,
        sha256: String,
        width: u32,
        height: u32,
        png_bytes: Vec<u8>,
    },
}

/// Pull either text or PNG image from a remote peer's clipboard.
pub async fn fetch_clipboard_payload_from_peer(
    addr: SocketAddr,
    identity: &DeviceIdentity,
) -> Result<RemoteClipboardPayload> {
    let (mut stream, mut cipher, _) = connect_secure(addr, identity, Some(6)).await?;
    write_frame_with(&mut stream, &Frame::ClipboardPullRequest, &mut cipher).await?;
    match read_frame_with(&mut stream, &mut cipher).await? {
        Some(Frame::ClipboardPullResponse {
            sender_name,
            sha256,
            text,
        }) => Ok(RemoteClipboardPayload::Text {
            sender_name,
            sha256,
            text,
        }),
        Some(Frame::ClipboardImageResponse {
            sender_name,
            sha256,
            width,
            height,
            png_bytes,
        }) => Ok(RemoteClipboardPayload::Image {
            sender_name,
            sha256,
            width,
            height,
            png_bytes,
        }),
        Some(Frame::Error { message }) => Err(LierError::Protocol(message)),
        _ => Err(LierError::Protocol(
            "Неожиданный ответ на запрос буфера".into(),
        )),
    }
}

/// Pull the current clipboard text from a remote peer on demand.
pub async fn fetch_clipboard_from_peer(
    addr: SocketAddr,
    identity: &DeviceIdentity,
) -> Result<(String, String, String)> {
    match fetch_clipboard_payload_from_peer(addr, identity).await? {
        RemoteClipboardPayload::Text {
            sender_name,
            sha256,
            text,
        } => Ok((sender_name, sha256, text)),
        RemoteClipboardPayload::Image { .. } => Ok((String::new(), String::new(), String::new())),
    }
}

/// Push clipboard PNG image to a peer.
pub async fn send_clipboard_image_png(
    addr: SocketAddr,
    identity: &DeviceIdentity,
    width: u32,
    height: u32,
    png_bytes: Vec<u8>,
) -> Result<()> {
    let (mut stream, mut cipher, _) = connect_secure(addr, identity, Some(8)).await?;
    let sha256 = hash_bytes(&png_bytes);
    write_frame_with(
        &mut stream,
        &Frame::ClipboardImagePush {
            sender_id: identity.device_id.clone(),
            sender_name: identity.device_name.clone(),
            sha256,
            width,
            height,
            png_bytes,
        },
        &mut cipher,
    )
    .await?;
    match read_frame_with(&mut stream, &mut cipher).await? {
        Some(Frame::Error { message }) => Err(LierError::Protocol(message)),
        _ => Ok(()),
    }
}

/// Push clipboard text to a peer.
pub async fn send_clipboard_text(
    addr: SocketAddr,
    identity: &DeviceIdentity,
    text: &str,
) -> Result<()> {
    let (mut stream, mut cipher, _) = connect_secure(addr, identity, Some(5)).await?;
    let sha256 = hash_bytes(text.as_bytes());
    write_frame_with(
        &mut stream,
        &Frame::ClipboardPush {
            sender_id: identity.device_id.clone(),
            sender_name: identity.device_name.clone(),
            sha256,
            text: text.to_string(),
        },
        &mut cipher,
    )
    .await?;
    match read_frame_with(&mut stream, &mut cipher).await? {
        Some(Frame::Error { message }) => Err(LierError::Protocol(message)),
        _ => Ok(()),
    }
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
        if matches!(
            comp,
            std::path::Component::ParentDir | std::path::Component::RootDir
        ) {
            return Err(LierError::Protocol(format!("Unsafe relative path: {rel}")));
        }
    }
    Ok(root.join(clean))
}
