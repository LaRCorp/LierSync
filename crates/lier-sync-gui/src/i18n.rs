// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

use lier_sync_core::{AppLanguage, ChannelPolicy, SyncDirection};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirChoice(pub SyncDirection, pub AppLanguage);

impl fmt::Display for DirChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.label(self.1))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyChoice(pub ChannelPolicy, pub AppLanguage);

impl fmt::Display for PolicyChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.label(self.1))
    }
}

pub fn dir_choices(lang: AppLanguage) -> [DirChoice; 3] {
    [
        DirChoice(SyncDirection::Bidirectional, lang),
        DirChoice(SyncDirection::SendOnly, lang),
        DirChoice(SyncDirection::ReceiveOnly, lang),
    ]
}

pub fn policy_choices(lang: AppLanguage) -> [PolicyChoice; 5] {
    [
        PolicyChoice(ChannelPolicy::Auto, lang),
        PolicyChoice(ChannelPolicy::WifiLanOnly, lang),
        PolicyChoice(ChannelPolicy::WifiDirectUsbOnly, lang),
        PolicyChoice(ChannelPolicy::BluetoothPreferred, lang),
        PolicyChoice(ChannelPolicy::InternetRelay, lang),
    ]
}

pub fn tr<'a>(lang: AppLanguage, ru: &'a str, en: &'a str) -> &'a str {
    match lang {
        AppLanguage::Ru => ru,
        AppLanguage::En => en,
    }
}

pub fn folder_status_label(auto_enabled: bool, dir: SyncDirection, lang: AppLanguage) -> String {
    let mode = if auto_enabled {
        tr(lang, "Авто", "Auto")
    } else {
        tr(lang, "Ручной", "Manual")
    };
    format!("{mode} · {}", dir.label(lang))
}
