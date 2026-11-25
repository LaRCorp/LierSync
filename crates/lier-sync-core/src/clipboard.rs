// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

use crate::error::{LierError, Result};
use crate::merkle::hash_bytes;
use chrono::{DateTime, Local};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardDirection {
    Sent,
    Received,
}

#[derive(Debug, Clone)]
pub struct ClipboardEntry {
    pub timestamp: DateTime<Local>,
    pub peer_name: String,
    pub direction: ClipboardDirection,
    pub preview: String,
    pub sha256: String,
    pub full_text: String,
    pub is_image: bool,
    pub image_png: Option<Vec<u8>>,
}

/// Anti-echo guard that tracks the last sent/received clipboard content hash (text & PNG images)
/// to prevent feedback loops across peers.
#[derive(Clone)]
pub struct ClipboardGuard {
    last_hash: Arc<Mutex<Option<String>>>,
    history: Arc<Mutex<VecDeque<ClipboardEntry>>>,
    max_history: usize,
}

impl ClipboardGuard {
    pub fn new(max_history: usize) -> Self {
        Self {
            last_hash: Arc::new(Mutex::new(None)),
            history: Arc::new(Mutex::new(VecDeque::with_capacity(max_history))),
            max_history,
        }
    }

    /// Returns `Some(hash)` if the text is new and not an echo of a recent remote/local sync.
    pub fn should_emit_local(&self, text: &str) -> Option<String> {
        if text.is_empty() {
            return None;
        }
        let hash = hash_bytes(text.as_bytes());
        let mut guard = self.last_hash.lock().ok()?;
        if guard.as_deref() == Some(hash.as_str()) {
            return None;
        }
        *guard = Some(hash.clone());
        Some(hash)
    }

    /// Returns `Some(hash)` if PNG image bytes are new and not an echo.
    pub fn should_emit_local_image(&self, png_bytes: &[u8]) -> Option<String> {
        if png_bytes.is_empty() {
            return None;
        }
        let hash = hash_bytes(png_bytes);
        let mut guard = self.last_hash.lock().ok()?;
        if guard.as_deref() == Some(hash.as_str()) {
            return None;
        }
        *guard = Some(hash.clone());
        Some(hash)
    }

    /// Record a remotely received clipboard payload and update the anti-echo hash.
    pub fn accept_remote(&self, peer_name: &str, text: &str, expected_hash: &str) -> bool {
        let actual_hash = hash_bytes(text.as_bytes());
        if !expected_hash.is_empty() && actual_hash != expected_hash {
            return false;
        }
        if let Ok(mut guard) = self.last_hash.lock() {
            if guard.as_deref() == Some(actual_hash.as_str()) {
                return false;
            }
            *guard = Some(actual_hash.clone());
        }
        self.push_history(ClipboardEntry {
            timestamp: Local::now(),
            peer_name: peer_name.to_string(),
            direction: ClipboardDirection::Received,
            preview: make_preview(text),
            sha256: actual_hash,
            full_text: text.to_string(),
            is_image: false,
            image_png: None,
        });
        true
    }

    /// Record a remotely received PNG clipboard image and update anti-echo hash.
    pub fn accept_remote_image(
        &self,
        peer_name: &str,
        width: u32,
        height: u32,
        png_bytes: &[u8],
        expected_hash: &str,
    ) -> bool {
        if png_bytes.is_empty() {
            return false;
        }
        let actual_hash = hash_bytes(png_bytes);
        if !expected_hash.is_empty() && actual_hash != expected_hash {
            return false;
        }
        if let Ok(mut guard) = self.last_hash.lock() {
            if guard.as_deref() == Some(actual_hash.as_str()) {
                return false;
            }
            *guard = Some(actual_hash.clone());
        }
        self.push_history(ClipboardEntry {
            timestamp: Local::now(),
            peer_name: peer_name.to_string(),
            direction: ClipboardDirection::Received,
            preview: format!("[Image {width}x{height}]"),
            sha256: actual_hash,
            full_text: String::new(),
            is_image: true,
            image_png: Some(png_bytes.to_vec()),
        });
        true
    }

    pub fn record_sent(&self, peer_name: &str, text: &str, hash: &str) {
        self.push_history(ClipboardEntry {
            timestamp: Local::now(),
            peer_name: peer_name.to_string(),
            direction: ClipboardDirection::Sent,
            preview: make_preview(text),
            sha256: hash.to_string(),
            full_text: text.to_string(),
            is_image: false,
            image_png: None,
        });
    }

    pub fn record_sent_image(
        &self,
        peer_name: &str,
        width: u32,
        height: u32,
        png_bytes: &[u8],
        hash: &str,
    ) {
        self.push_history(ClipboardEntry {
            timestamp: Local::now(),
            peer_name: peer_name.to_string(),
            direction: ClipboardDirection::Sent,
            preview: format!("[Image {width}x{height}]"),
            sha256: hash.to_string(),
            full_text: String::new(),
            is_image: true,
            image_png: Some(png_bytes.to_vec()),
        });
    }

    pub fn history(&self) -> Vec<ClipboardEntry> {
        self.history
            .lock()
            .map(|h| h.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn push_history(&self, entry: ClipboardEntry) {
        if let Ok(mut h) = self.history.lock() {
            if h.len() >= self.max_history {
                h.pop_back();
            }
            h.push_front(entry);
        }
    }
}

fn make_preview(text: &str) -> String {
    let single_line = text.replace(['\r', '\n'], " ");
    if single_line.chars().count() > 80 {
        let prefix: String = single_line.chars().take(77).collect();
        format!("{prefix}...")
    } else {
        single_line
    }
}

#[cfg(not(target_os = "android"))]
pub fn get_clipboard_text() -> Result<String> {
    let mut cb = arboard::Clipboard::new().map_err(|e| LierError::Clipboard(e.to_string()))?;
    cb.get_text()
        .map_err(|e| LierError::Clipboard(e.to_string()))
}

#[cfg(not(target_os = "android"))]
pub fn set_clipboard_text(text: &str) -> Result<()> {
    let mut cb = arboard::Clipboard::new().map_err(|e| LierError::Clipboard(e.to_string()))?;
    cb.set_text(text.to_string())
        .map_err(|e| LierError::Clipboard(e.to_string()))
}

#[cfg(not(target_os = "android"))]
pub fn get_clipboard_image_png() -> Result<(u32, u32, Vec<u8>)> {
    use image::codecs::png::PngEncoder;
    use image::ImageEncoder;

    let mut cb = arboard::Clipboard::new().map_err(|e| LierError::Clipboard(e.to_string()))?;
    let img = cb.get_image().map_err(|e| LierError::Clipboard(e.to_string()))?;
    let w = img.width as u32;
    let h = img.height as u32;
    let mut png = Vec::new();
    let encoder = PngEncoder::new(&mut png);
    encoder
        .write_image(&img.bytes, w, h, image::ExtendedColorType::Rgba8)
        .map_err(|e| LierError::Clipboard(format!("PNG encode failed: {e}")))?;
    Ok((w, h, png))
}

#[cfg(not(target_os = "android"))]
pub fn set_clipboard_image_png(png_bytes: &[u8]) -> Result<(u32, u32)> {
    let dyn_img = image::load_from_memory(png_bytes)
        .map_err(|e| LierError::Clipboard(format!("PNG decode failed: {e}")))?;
    let rgba = dyn_img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut cb = arboard::Clipboard::new().map_err(|e| LierError::Clipboard(e.to_string()))?;
    cb.set_image(arboard::ImageData {
        width: w as usize,
        height: h as usize,
        bytes: std::borrow::Cow::Owned(rgba.into_raw()),
    })
    .map_err(|e| LierError::Clipboard(e.to_string()))?;
    Ok((w, h))
}

#[cfg(target_os = "android")]
pub fn get_clipboard_text() -> Result<String> {
    Err(LierError::Clipboard(
        "Android clipboard must be bridged via JNI ClipboardManager".into(),
    ))
}

#[cfg(target_os = "android")]
pub fn set_clipboard_text(_text: &str) -> Result<()> {
    Err(LierError::Clipboard(
        "Android clipboard must be bridged via JNI ClipboardManager".into(),
    ))
}

#[cfg(target_os = "android")]
pub fn get_clipboard_image_png() -> Result<(u32, u32, Vec<u8>)> {
    Err(LierError::Clipboard(
        "Android image clipboard must be bridged via JNI ClipboardManager".into(),
    ))
}

#[cfg(target_os = "android")]
pub fn set_clipboard_image_png(_png_bytes: &[u8]) -> Result<(u32, u32)> {
    Err(LierError::Clipboard(
        "Android image clipboard must be bridged via JNI ClipboardManager".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anti_echo_blocks_duplicate_remote_and_local() {
        let guard = ClipboardGuard::new(10);
        let text = "hello lier-sync";
        let hash = hash_bytes(text.as_bytes());

        assert!(guard.accept_remote("PeerA", text, &hash));
        // Local watcher sees the same text we just wrote -> must be ignored
        assert!(guard.should_emit_local(text).is_none());
        // Different text -> must emit
        assert!(guard.should_emit_local("new text").is_some());
    }
}
