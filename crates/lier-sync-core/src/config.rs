// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

use crate::channel::ChannelPolicy;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AppLanguage {
    #[default]
    Ru,
    En,
}

impl AppLanguage {
    pub fn as_code(self) -> &'static str {
        match self {
            AppLanguage::Ru => "ru",
            AppLanguage::En => "en",
        }
    }

    pub fn from_code(code: &str) -> Self {
        if code.eq_ignore_ascii_case("en") {
            AppLanguage::En
        } else {
            AppLanguage::Ru
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SyncDirection {
    #[default]
    Bidirectional,
    SendOnly,
    ReceiveOnly,
}

impl SyncDirection {
    pub const ALL: [SyncDirection; 3] = [
        SyncDirection::Bidirectional,
        SyncDirection::SendOnly,
        SyncDirection::ReceiveOnly,
    ];

    pub fn as_code(self) -> &'static str {
        match self {
            SyncDirection::Bidirectional => "bidirectional",
            SyncDirection::SendOnly => "send_only",
            SyncDirection::ReceiveOnly => "receive_only",
        }
    }

    pub fn from_code(code: &str) -> Self {
        match code {
            "send_only" => SyncDirection::SendOnly,
            "receive_only" => SyncDirection::ReceiveOnly,
            _ => SyncDirection::Bidirectional,
        }
    }

    pub fn label(self, lang: AppLanguage) -> &'static str {
        match (self, lang) {
            (SyncDirection::Bidirectional, AppLanguage::Ru) => "Общая (двусторонняя)",
            (SyncDirection::SendOnly, AppLanguage::Ru) => "Отправитель (отдавать)",
            (SyncDirection::ReceiveOnly, AppLanguage::Ru) => "Получатель (принимать)",
            (SyncDirection::Bidirectional, AppLanguage::En) => "Bidirectional (shared)",
            (SyncDirection::SendOnly, AppLanguage::En) => "Send only (upload)",
            (SyncDirection::ReceiveOnly, AppLanguage::En) => "Receive only (download)",
        }
    }
}

impl fmt::Display for SyncDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncDirection::Bidirectional => write!(f, "Общая (двусторонняя)"),
            SyncDirection::SendOnly => write!(f, "Отправитель (отдавать)"),
            SyncDirection::ReceiveOnly => write!(f, "Получатель (принимать)"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferDirection {
    Sent,
    Received,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRecord {
    pub file_name: String,
    pub peer: String,
    pub path: String,
    #[serde(default)]
    pub content_uri: Option<String>,
    pub bytes: u64,
    pub time: String,
    pub direction: TransferDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharedFolderConfig {
    pub id: String,
    pub path: String,
    pub remote_addr: String,
    pub auto_sync: bool,
    #[serde(default)]
    pub direction: SyncDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustedPeer {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
    #[serde(default)]
    pub added_at: String,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub manual_addr: String,
    #[serde(default)]
    pub auto_clipboard: bool,
    #[serde(default)]
    pub channel_policy: ChannelPolicy,
    #[serde(default)]
    pub relay_endpoint: String,
    #[serde(default)]
    pub shared_folders: Vec<SharedFolderConfig>,
    #[serde(default)]
    pub transfer_history: Vec<TransferRecord>,
    #[serde(default)]
    pub language: AppLanguage,
    #[serde(default = "default_true")]
    pub encrypt_transport: bool,
    #[serde(default)]
    pub whitelist_only: bool,
    #[serde(default)]
    pub pairing_pin: String,
    #[serde(default)]
    pub trusted_peers: Vec<TrustedPeer>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            manual_addr: String::new(),
            auto_clipboard: false,
            channel_policy: ChannelPolicy::Auto,
            relay_endpoint: String::new(),
            shared_folders: Vec::new(),
            transfer_history: Vec::new(),
            language: AppLanguage::Ru,
            encrypt_transport: true,
            whitelist_only: false,
            pairing_pin: String::new(),
            trusted_peers: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("LaRevole")
            .join("LierSync")
            .join("config.json")
    }

    pub fn load_from(path: &Path) -> Self {
        if path.exists() {
            if let Ok(raw) = fs::read_to_string(path) {
                if let Ok(cfg) = serde_json::from_str::<AppConfig>(&raw) {
                    return cfg;
                }
            }
        }
        Self::default()
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
}
