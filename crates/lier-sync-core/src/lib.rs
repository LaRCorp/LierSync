// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

pub mod bluetooth;
pub mod channel;
pub mod clipboard;
pub mod config;
pub mod discovery;
pub mod ecosystem;
pub mod error;
pub mod identity;
pub mod merkle;
pub mod protocol;
pub mod security;
pub mod transport;
pub mod watcher;

pub use bluetooth::{
    BluetoothPeerInfo, LIERSYNC_BT_UUID_STR, has_bluetooth_radio, list_paired_bluetooth_devices,
    send_bluetooth_clipboard_image, send_bluetooth_clipboard_text, send_bluetooth_quick_batch,
    start_bluetooth_server,
};
pub use channel::{
    ChannelDescriptor, ChannelPolicy, TransportMedium, probe_channels, probe_channels_for_lang,
    select_active_medium,
};

pub use clipboard::{
    ClipboardDirection, ClipboardEntry, ClipboardGuard, get_clipboard_image_png,
    get_clipboard_text, set_clipboard_image_png, set_clipboard_text,
};
pub use config::{
    AppConfig, AppLanguage, SharedFolderConfig, SyncDirection, TransferDirection, TransferRecord,
    TrustedPeer,
};
pub use discovery::{DISCOVERY_PORT, DiscoveryService, PeerInfo, detect_local_ipv4s};
pub use ecosystem::{
    EcosystemAppStatus, EcosystemBridgeContext, EcosystemBridgeEvent, LIERKOUR_EXT_PORT,
    LIERKOUR_SYNC_PORT, LIERSYNC_BRIDGE_PORT, LIERTAGS_EXT_PORT, probe_ecosystem_apps,
    start_ecosystem_bridge,
};
pub use error::{LierError, Result};
pub use identity::DeviceIdentity;
pub use merkle::{
    CHUNK_SIZE, MerkleTree, SyncPreview, build_merkle_tree, diff_trees, hash_bytes, hash_file,
    preview_sync,
};
pub use security::{
    PairingPayload, SecurityManager, SecurityPolicy, SessionCipher, StreamRole, build_pairing_uri,
    generate_pairing_pin, generate_qr_png, parse_pairing_uri,
};
pub use transport::{
    BatchItem, CoreEvent, DEFAULT_P2P_PORT, RemoteClipboardPayload, SharedFolderRegistry,
    compare_folder_with_peer, expand_paths_for_send, fetch_clipboard_from_peer,
    fetch_clipboard_payload_from_peer, pair_with_peer, ping_peer, push_folder_to_peer,
    send_clipboard_image_png, send_clipboard_text, send_quick_batch, send_quick_file,
    start_p2p_server, start_p2p_server_secure, sync_folder_by_direction, sync_folder_from_peer,
};
pub use watcher::{FolderChangeEvent, diff_snapshots, spawn_folder_watcher};
