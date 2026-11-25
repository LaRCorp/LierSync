// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

use crate::config::AppLanguage;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Physical / logical network medium used by LierSync to transfer files and clipboard data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransportMedium {
    /// Local Wi-Fi or Ethernet LAN (TCP/QUIC + UDP multicast/broadcast discovery).
    LanWifi,
    /// Direct IP link over Wi-Fi Direct, Mobile Hotspot, or USB Tethering (RNDIS / ADB forward).
    WifiDirectUsb,
    /// Bluetooth RFCOMM / BLE GATT fallback (ideal for clipboard & small files without Wi-Fi).
    Bluetooth,
    /// Encrypted Internet Relay / NAT-traversal endpoint for remote devices outside LAN.
    InternetRelay,
}

impl fmt::Display for TransportMedium {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportMedium::LanWifi => write!(f, "Wi-Fi / LAN"),
            TransportMedium::WifiDirectUsb => write!(f, "Wi-Fi Direct / USB"),
            TransportMedium::Bluetooth => write!(f, "Bluetooth"),
            TransportMedium::InternetRelay => write!(f, "Internet Relay"),
        }
    }
}

/// User-selectable routing policy for choosing the active sync channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChannelPolicy {
    /// Automatically pick the fastest available channel (Wi-Fi/LAN -> Wi-Fi Direct/USB -> Bluetooth -> Relay).
    #[default]
    Auto,
    /// Restrict synchronization to local Wi-Fi / Ethernet LAN only.
    WifiLanOnly,
    /// Prefer direct point-to-point Wi-Fi Direct / Hotspot / USB-tethered link.
    WifiDirectUsbOnly,
    /// Prefer Bluetooth RFCOMM/BLE when pairing nearby devices without Wi-Fi.
    BluetoothPreferred,
    /// Route through Internet Relay when devices are on separate networks.
    InternetRelay,
}

impl fmt::Display for ChannelPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChannelPolicy::Auto => write!(f, "Авто (Wi-Fi -> USB/Direct -> Bluetooth -> Relay)"),
            ChannelPolicy::WifiLanOnly => write!(f, "Только Wi-Fi / LAN"),
            ChannelPolicy::WifiDirectUsbOnly => write!(f, "Wi-Fi Direct / USB-модем"),
            ChannelPolicy::BluetoothPreferred => write!(f, "Bluetooth (резервный канал)"),
            ChannelPolicy::InternetRelay => write!(f, "Удалённый Relay (Интернет)"),
        }
    }
}

impl ChannelPolicy {
    pub const ALL: [ChannelPolicy; 5] = [
        ChannelPolicy::Auto,
        ChannelPolicy::WifiLanOnly,
        ChannelPolicy::WifiDirectUsbOnly,
        ChannelPolicy::BluetoothPreferred,
        ChannelPolicy::InternetRelay,
    ];

    pub fn label(self, lang: AppLanguage) -> &'static str {
        match (self, lang) {
            (ChannelPolicy::Auto, AppLanguage::Ru) => "Авто (Wi-Fi -> USB/Direct -> Bluetooth -> Relay)",
            (ChannelPolicy::WifiLanOnly, AppLanguage::Ru) => "Только Wi-Fi / LAN",
            (ChannelPolicy::WifiDirectUsbOnly, AppLanguage::Ru) => "Wi-Fi Direct / USB-модем",
            (ChannelPolicy::BluetoothPreferred, AppLanguage::Ru) => "Bluetooth (резервный канал)",
            (ChannelPolicy::InternetRelay, AppLanguage::Ru) => "Удалённый Relay (Интернет)",
            (ChannelPolicy::Auto, AppLanguage::En) => "Auto (Wi-Fi -> USB/Direct -> Bluetooth -> Relay)",
            (ChannelPolicy::WifiLanOnly, AppLanguage::En) => "Wi-Fi / LAN only",
            (ChannelPolicy::WifiDirectUsbOnly, AppLanguage::En) => "Wi-Fi Direct / USB Tether",
            (ChannelPolicy::BluetoothPreferred, AppLanguage::En) => "Bluetooth (fallback)",
            (ChannelPolicy::InternetRelay, AppLanguage::En) => "Internet Relay (remote)",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChannelDescriptor {
    pub medium: TransportMedium,
    pub available: bool,
    pub endpoint_hint: String,
    pub bandwidth_note: &'static str,
}

/// Inspect available transport channels on the current host (Russian default).
pub fn probe_channels(p2p_port: u16, relay_endpoint: Option<&str>) -> Vec<ChannelDescriptor> {
    probe_channels_for_lang(p2p_port, relay_endpoint, AppLanguage::Ru)
}

/// Inspect available transport channels localized for `lang`.
pub fn probe_channels_for_lang(
    p2p_port: u16,
    relay_endpoint: Option<&str>,
    lang: AppLanguage,
) -> Vec<ChannelDescriptor> {
    match lang {
        AppLanguage::Ru => vec![
            ChannelDescriptor {
                medium: TransportMedium::LanWifi,
                available: true,
                endpoint_hint: format!("TCP/UDP :{p2p_port} (multicast 239.255.77.88)"),
                bandwidth_note: "Высокая скорость · папки, пакеты файлов и буфер",
            },
            ChannelDescriptor {
                medium: TransportMedium::WifiDirectUsb,
                available: true,
                endpoint_hint: format!("Direct IP :{p2p_port} / USB ADB forward"),
                bandwidth_note: "Прямой канал без роутера · высокая скорость",
            },
            ChannelDescriptor {
                medium: TransportMedium::Bluetooth,
                available: false,
                endpoint_hint: "RFCOMM / BLE (требуется сопряжение и нативный адаптер)".to_string(),
                bandwidth_note: "Низкое энергопотребление · буфер обмена и мелкие файлы",
            },
            ChannelDescriptor {
                medium: TransportMedium::InternetRelay,
                available: relay_endpoint.map(|s| !s.trim().is_empty()).unwrap_or(false),
                endpoint_hint: relay_endpoint
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or("Не настроен (укажите адрес relay/IP)")
                    .to_string(),
                bandwidth_note: "Удалённый доступ вне локальной сети",
            },
        ],
        AppLanguage::En => vec![
            ChannelDescriptor {
                medium: TransportMedium::LanWifi,
                available: true,
                endpoint_hint: format!("TCP/UDP :{p2p_port} (multicast 239.255.77.88)"),
                bandwidth_note: "High speed · folders, file batches & clipboard",
            },
            ChannelDescriptor {
                medium: TransportMedium::WifiDirectUsb,
                available: true,
                endpoint_hint: format!("Direct IP :{p2p_port} / USB ADB forward"),
                bandwidth_note: "Direct link without router · high speed",
            },
            ChannelDescriptor {
                medium: TransportMedium::Bluetooth,
                available: false,
                endpoint_hint: "RFCOMM / BLE (requires pairing & native adapter)".to_string(),
                bandwidth_note: "Low power · clipboard & small files",
            },
            ChannelDescriptor {
                medium: TransportMedium::InternetRelay,
                available: relay_endpoint.map(|s| !s.trim().is_empty()).unwrap_or(false),
                endpoint_hint: relay_endpoint
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or("Not configured (enter relay host/IP)")
                    .to_string(),
                bandwidth_note: "Remote access outside local network",
            },
        ],
    }
}

/// Select the preferred active transport medium according to `policy` and `channels`.
pub fn select_active_medium(
    policy: ChannelPolicy,
    channels: &[ChannelDescriptor],
) -> Option<TransportMedium> {
    let is_up = |m: TransportMedium| channels.iter().any(|c| c.medium == m && c.available);

    match policy {
        ChannelPolicy::Auto => [
            TransportMedium::LanWifi,
            TransportMedium::WifiDirectUsb,
            TransportMedium::Bluetooth,
            TransportMedium::InternetRelay,
        ]
        .into_iter()
        .find(|&m| is_up(m)),
        ChannelPolicy::WifiLanOnly => is_up(TransportMedium::LanWifi).then_some(TransportMedium::LanWifi),
        ChannelPolicy::WifiDirectUsbOnly => {
            is_up(TransportMedium::WifiDirectUsb).then_some(TransportMedium::WifiDirectUsb)
        }
        ChannelPolicy::BluetoothPreferred => is_up(TransportMedium::Bluetooth)
            .then_some(TransportMedium::Bluetooth)
            .or_else(|| is_up(TransportMedium::LanWifi).then_some(TransportMedium::LanWifi)),
        ChannelPolicy::InternetRelay => {
            is_up(TransportMedium::InternetRelay).then_some(TransportMedium::InternetRelay)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_policy_selects_lan_wifi_first() {
        let channels = probe_channels(47789, None);
        assert_eq!(
            select_active_medium(ChannelPolicy::Auto, &channels),
            Some(TransportMedium::LanWifi)
        );
        assert_eq!(
            select_active_medium(ChannelPolicy::InternetRelay, &channels),
            None
        );
    }
}
