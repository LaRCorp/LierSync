// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

use crate::config::TrustedPeer;
use crate::error::{LierError, Result};
use chacha20poly1305::aead::{Aead, KeyInit, OsRng, Payload, rand_core::RngCore};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use chrono::Local;
use image::{ImageEncoder, Luma, codecs::png::PngEncoder};
use qrcode::QrCode;
use sha2::{Digest, Sha256};
use std::sync::{Arc, RwLock};
use x25519_dalek::{EphemeralSecret, PublicKey};

const SESSION_AAD: &[u8] = b"LierSync-v1.1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamRole {
    Client,
    Server,
}

/// Symmetric AEAD cipher state upgraded after X25519 key exchange.
#[derive(Clone)]
pub enum SessionCipher {
    Plain,
    Encrypted {
        key: [u8; 32],
        role: StreamRole,
        tx_seq: u64,
        rx_seq: u64,
    },
}

impl SessionCipher {
    pub fn from_shared_secret(
        shared_secret: &[u8; 32],
        pub_a: &[u8; 32],
        pub_b: &[u8; 32],
        role: StreamRole,
    ) -> Self {
        let (first, second) = if pub_a <= pub_b {
            (pub_a, pub_b)
        } else {
            (pub_b, pub_a)
        };
        let mut hasher = Sha256::new();
        hasher.update(b"LierSync-X25519-ChaCha20Poly1305-v1");
        hasher.update(shared_secret);
        hasher.update(first);
        hasher.update(second);
        let key: [u8; 32] = hasher.finalize().into();
        Self::Encrypted {
            key,
            role,
            tx_seq: 0,
            rx_seq: 0,
        }
    }

    pub fn is_encrypted(&self) -> bool {
        matches!(self, Self::Encrypted { .. })
    }

    pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>> {
        match self {
            Self::Plain => Ok(plaintext.to_vec()),
            Self::Encrypted {
                key,
                role,
                tx_seq,
                ..
            } => {
                let cipher = ChaCha20Poly1305::new_from_slice(key)
                    .map_err(|_| LierError::Protocol("Invalid session key".into()))?;
                let prefix = match role {
                    StreamRole::Client => b"C2S!",
                    StreamRole::Server => b"S2C!",
                };
                let mut nonce_bytes = [0u8; 12];
                nonce_bytes[..4].copy_from_slice(prefix);
                nonce_bytes[4..12].copy_from_slice(&tx_seq.to_be_bytes());
                *tx_seq = tx_seq.wrapping_add(1);
                cipher
                    .encrypt(
                        Nonce::from_slice(&nonce_bytes),
                        Payload {
                            msg: plaintext,
                            aad: SESSION_AAD,
                        },
                    )
                    .map_err(|_| LierError::Protocol("Frame encryption failed".into()))
            }
        }
    }

    pub fn open(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        match self {
            Self::Plain => Ok(ciphertext.to_vec()),
            Self::Encrypted {
                key,
                role,
                rx_seq,
                ..
            } => {
                let cipher = ChaCha20Poly1305::new_from_slice(key)
                    .map_err(|_| LierError::Protocol("Invalid session key".into()))?;
                let prefix = match role {
                    StreamRole::Client => b"S2C!",
                    StreamRole::Server => b"C2S!",
                };
                let mut nonce_bytes = [0u8; 12];
                nonce_bytes[..4].copy_from_slice(prefix);
                nonce_bytes[4..12].copy_from_slice(&rx_seq.to_be_bytes());
                *rx_seq = rx_seq.wrapping_add(1);
                cipher
                    .decrypt(
                        Nonce::from_slice(&nonce_bytes),
                        Payload {
                            msg: ciphertext,
                            aad: SESSION_AAD,
                        },
                    )
                    .map_err(|_| LierError::Protocol("Frame authentication/decryption failed".into()))
            }
        }
    }
}

pub fn generate_ephemeral_keypair() -> (EphemeralSecret, [u8; 32]) {
    let secret = EphemeralSecret::random_from_rng(OsRng);
    let pub_bytes = PublicKey::from(&secret).to_bytes();
    (secret, pub_bytes)
}

pub fn derive_shared_secret(secret: EphemeralSecret, remote_pub: &[u8; 32]) -> [u8; 32] {
    let pk = PublicKey::from(*remote_pub);
    secret.diffie_hellman(&pk).to_bytes()
}

pub fn generate_pairing_pin() -> String {
    let n = OsRng.next_u32() % 1_000_000;
    format!("{n:06}")
}

#[derive(Debug, Clone)]
pub struct SecurityPolicy {
    pub encrypt_transport: bool,
    pub whitelist_only: bool,
    pub pairing_pin: String,
    pub trusted_peers: Vec<TrustedPeer>,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            encrypt_transport: true,
            whitelist_only: false,
            pairing_pin: generate_pairing_pin(),
            trusted_peers: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct SecurityManager {
    inner: Arc<RwLock<SecurityPolicy>>,
}

impl SecurityManager {
    pub fn new(mut policy: SecurityPolicy) -> Self {
        if policy.pairing_pin.trim().is_empty() {
            policy.pairing_pin = generate_pairing_pin();
        }
        Self {
            inner: Arc::new(RwLock::new(policy)),
        }
    }

    pub fn snapshot(&self) -> SecurityPolicy {
        self.inner.read().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn encrypt_enabled(&self) -> bool {
        self.inner.read().map(|g| g.encrypt_transport).unwrap_or(true)
    }

    pub fn set_encrypt_transport(&self, enabled: bool) {
        if let Ok(mut g) = self.inner.write() {
            g.encrypt_transport = enabled;
        }
    }

    pub fn whitelist_only(&self) -> bool {
        self.inner.read().map(|g| g.whitelist_only).unwrap_or(false)
    }

    pub fn set_whitelist_only(&self, enabled: bool) {
        if let Ok(mut g) = self.inner.write() {
            g.whitelist_only = enabled;
        }
    }

    pub fn pairing_pin(&self) -> String {
        self.inner
            .read()
            .map(|g| g.pairing_pin.clone())
            .unwrap_or_default()
    }

    pub fn rotate_pin(&self) -> String {
        let pin = generate_pairing_pin();
        if let Ok(mut g) = self.inner.write() {
            g.pairing_pin = pin.clone();
        }
        pin
    }

    pub fn set_pin(&self, pin: &str) {
        if !pin.trim().is_empty() {
            if let Ok(mut g) = self.inner.write() {
                g.pairing_pin = pin.trim().to_string();
            }
        }
    }

    pub fn trusted_peers(&self) -> Vec<TrustedPeer> {
        self.inner
            .read()
            .map(|g| g.trusted_peers.clone())
            .unwrap_or_default()
    }

    pub fn set_trusted_peers(&self, list: Vec<TrustedPeer>) {
        if let Ok(mut g) = self.inner.write() {
            g.trusted_peers = list;
        }
    }

    pub fn is_trusted(&self, device_id: &str, fingerprint: &str) -> bool {
        let Ok(g) = self.inner.read() else {
            return false;
        };
        g.trusted_peers.iter().any(|p| {
            (!device_id.is_empty() && p.device_id == device_id)
                || (!fingerprint.is_empty() && p.fingerprint == fingerprint)
        })
    }

    pub fn add_or_update_trusted(
        &self,
        device_id: &str,
        device_name: &str,
        fingerprint: &str,
    ) -> TrustedPeer {
        let entry = TrustedPeer {
            device_id: device_id.to_string(),
            device_name: device_name.to_string(),
            fingerprint: fingerprint.to_string(),
            added_at: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        };
        if let Ok(mut g) = self.inner.write() {
            if let Some(existing) = g.trusted_peers.iter_mut().find(|p| {
                (!device_id.is_empty() && p.device_id == device_id)
                    || (!fingerprint.is_empty() && p.fingerprint == fingerprint)
            }) {
                existing.device_name = device_name.to_string();
                existing.fingerprint = fingerprint.to_string();
                return existing.clone();
            }
            g.trusted_peers.push(entry.clone());
        }
        entry
    }

    pub fn remove_trusted(&self, device_id_or_fp: &str) -> bool {
        if let Ok(mut g) = self.inner.write() {
            let before = g.trusted_peers.len();
            g.trusted_peers.retain(|p| {
                p.device_id != device_id_or_fp && p.fingerprint != device_id_or_fp
            });
            return g.trusted_peers.len() != before;
        }
        false
    }

    pub fn verify_pin(&self, pin: &str) -> bool {
        let Ok(g) = self.inner.read() else {
            return false;
        };
        !pin.trim().is_empty() && g.pairing_pin == pin.trim()
    }
}

/// Render a QR code PNG (light background `#F8FAFC`, dark modules `#121316`) cleanly in pure Rust.
pub fn generate_qr_png(payload: &str, target_px: u32) -> Result<Vec<u8>> {
    let code = QrCode::new(payload.as_bytes())
        .map_err(|e| LierError::Protocol(format!("QR encode failed: {e}")))?;
    let width = code.width() as u32;
    let quiet = 4u32;
    let total_modules = width + quiet * 2;
    let scale = (target_px / total_modules).max(4);
    let img_size = total_modules * scale;

    let mut raw = vec![248u8; (img_size * img_size) as usize];
    for y in 0..width {
        for x in 0..width {
            if code[(x as usize, y as usize)] == qrcode::Color::Dark {
                let px0 = (x + quiet) * scale;
                let py0 = (y + quiet) * scale;
                for dy in 0..scale {
                    let row = ((py0 + dy) * img_size) as usize;
                    for dx in 0..scale {
                        raw[row + (px0 + dx) as usize] = 18u8;
                    }
                }
            }
        }
    }

    let mut png = Vec::new();
    let encoder = PngEncoder::new(&mut png);
    encoder
        .write_image(&raw, img_size, img_size, image::ExtendedColorType::L8)
        .map_err(|e| LierError::Protocol(format!("QR PNG encode failed: {e}")))?;
    let _ = Luma([0u8]); // keep explicit Luma import check zero-warning
    Ok(png)
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PairingPayload {
    pub addr: String,
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
    pub pin: String,
}

fn hex_encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.as_bytes() {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn hex_decode_component(hex: &str) -> String {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .filter_map(|i| hex.get(i..i + 2).and_then(|s| u8::from_str_radix(s, 16).ok()))
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

pub fn build_pairing_uri(
    addr: &str,
    device_id: &str,
    device_name: &str,
    fingerprint: &str,
    pin: &str,
) -> String {
    format!(
        "liersync://pair?addr={}&id={}&name_hex={}&fp={}&pin={}",
        addr.trim(),
        device_id.trim(),
        hex_encode_component(device_name.trim()),
        fingerprint.trim(),
        pin.trim()
    )
}

pub fn parse_pairing_uri(raw: &str) -> Option<PairingPayload> {
    let trimmed = raw.trim();
    let query = trimmed
        .strip_prefix("liersync://pair?")
        .or_else(|| trimmed.strip_prefix("liersync://pair/?"))?;
    let mut addr = String::new();
    let mut device_id = String::new();
    let mut device_name = String::new();
    let mut fingerprint = String::new();
    let mut pin = String::new();
    for pair in query.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        match k {
            "addr" => addr = v.to_string(),
            "id" => device_id = v.to_string(),
            "name_hex" => device_name = hex_decode_component(v),
            "name" if device_name.is_empty() => device_name = v.to_string(),
            "fp" => fingerprint = v.to_string(),
            "pin" => pin = v.to_string(),
            _ => {}
        }
    }
    if addr.is_empty() || pin.is_empty() {
        return None;
    }
    Some(PairingPayload {
        addr,
        device_id,
        device_name,
        fingerprint,
        pin,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x25519_chacha20_roundtrip() {
        let (c_sec, c_pub) = generate_ephemeral_keypair();
        let (s_sec, s_pub) = generate_ephemeral_keypair();

        let ss_c = derive_shared_secret(c_sec, &s_pub);
        let ss_s = derive_shared_secret(s_sec, &c_pub);
        assert_eq!(ss_c, ss_s);

        let mut client = SessionCipher::from_shared_secret(&ss_c, &c_pub, &s_pub, StreamRole::Client);
        let mut server = SessionCipher::from_shared_secret(&ss_s, &c_pub, &s_pub, StreamRole::Server);

        let ct1 = client.seal(b"hello secure liersync").unwrap();
        let pt1 = server.open(&ct1).unwrap();
        assert_eq!(pt1, b"hello secure liersync");

        let ct2 = server.seal(b"reply from server").unwrap();
        let pt2 = client.open(&ct2).unwrap();
        assert_eq!(pt2, b"reply from server");
    }

    #[test]
    fn whitelist_and_pin_pairing_work() {
        let mgr = SecurityManager::new(SecurityPolicy {
            encrypt_transport: true,
            whitelist_only: true,
            pairing_pin: "482910".into(),
            trusted_peers: Vec::new(),
        });
        assert!(!mgr.is_trusted("dev1", "fp1"));
        assert!(mgr.verify_pin("482910"));
        assert!(!mgr.verify_pin("000000"));
        mgr.add_or_update_trusted("dev1", "Phone", "fp1");
        assert!(mgr.is_trusted("dev1", "fp1"));
        assert!(mgr.remove_trusted("fp1"));
        assert!(!mgr.is_trusted("dev1", "fp1"));
    }

    #[test]
    fn qr_png_generates_valid_header() {
        let png = generate_qr_png("liersync://pair?addr=192.168.1.5:47789&pin=123456", 240).unwrap();
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    }
}
