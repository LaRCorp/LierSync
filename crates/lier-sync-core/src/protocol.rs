// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

use crate::error::{LierError, Result};
use crate::merkle::MerkleTree;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Frame {
    Hello {
        device_id: String,
        device_name: String,
        os: String,
        #[serde(default)]
        p2p_port: Option<u16>,
    },
    QuickSendOffer {
        transfer_id: String,
        file_name: String,
        #[serde(default)]
        rel_path: Option<String>,
        file_size: u64,
        sha256: String,
    },
    QuickSendAccept {
        transfer_id: String,
        accepted: bool,
    },
    FileChunk {
        transfer_id: String,
        offset: u64,
        data: Vec<u8>,
    },
    QuickSendDone {
        transfer_id: String,
    },
    SyncTreeRequest {
        folder_id: String,
    },
    SyncTreeResponse {
        folder_id: String,
        tree: MerkleTree,
    },
    SyncChunkRequest {
        folder_id: String,
        rel_path: String,
        chunk_index: usize,
    },
    SyncChunkData {
        folder_id: String,
        rel_path: String,
        chunk_index: usize,
        total_chunks: usize,
        file_mtime: i64,
        data: Vec<u8>,
    },
    ClipboardPush {
        sender_id: String,
        sender_name: String,
        sha256: String,
        text: String,
    },
    ClipboardPullRequest,
    ClipboardPullResponse {
        sender_name: String,
        sha256: String,
        text: String,
    },
    Ack {
        message: String,
    },
    Error {
        message: String,
    },
}

const MAX_FRAME_LEN: usize = 16 * 1024 * 1024; // 16 MiB safety cap

pub async fn write_frame<W: AsyncWriteExt + Unpin>(writer: &mut W, frame: &Frame) -> Result<()> {
    let payload = serde_json::to_vec(frame)?;
    let len = u32::try_from(payload.len())
        .map_err(|_| LierError::Protocol("Frame too large".into()))?;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn read_frame<R: AsyncReadExt + Unpin>(reader: &mut R) -> Result<Option<Frame>> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_LEN {
        return Err(LierError::Protocol(format!(
            "Incoming frame exceeds max size ({len} bytes)"
        )));
    }
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).await?;
    let frame = serde_json::from_slice(&payload)?;
    Ok(Some(frame))
}
