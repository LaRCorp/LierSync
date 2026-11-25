// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

use crate::error::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceIdentity {
    pub device_id: String,
    pub device_name: String,
    pub os: String,
    pub fingerprint: String,
}

impl DeviceIdentity {
    /// Load existing identity from OS config dir or generate & persist a new one.
    pub fn load_or_create() -> Result<Self> {
        Self::load_or_create_in(&Self::config_path(), None)
    }

    /// Load existing identity from an explicit file path (used on Android via `filesDir`).
    pub fn load_or_create_in(path: &Path, custom_name: Option<String>) -> Result<Self> {
        if path.exists() {
            if let Ok(raw) = fs::read_to_string(path) {
                if let Ok(id) = serde_json::from_str::<DeviceIdentity>(&raw) {
                    return Ok(id);
                }
            }
        }

        let device_id = Uuid::new_v4().to_string();
        let device_name = custom_name
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(default_hostname);
        let os = std::env::consts::OS.to_string();
        let mut hasher = Sha256::new();
        hasher.update(device_id.as_bytes());
        hasher.update(device_name.as_bytes());
        let fingerprint = format!("{:x}", hasher.finalize())[..16].to_string();

        let identity = Self {
            device_id,
            device_name,
            os,
            fingerprint,
        };

        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, serde_json::to_string_pretty(&identity)?);
        Ok(identity)
    }

    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LaRevole")
            .join("LierSync")
            .join("identity.json")
    }

    /// Default folder for received files on desktop (`Downloads/LierSync`).
    pub fn default_download_dir() -> PathBuf {
        dirs::download_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LierSync")
    }
}

fn default_hostname() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "LierSync-Node".to_string())
}
