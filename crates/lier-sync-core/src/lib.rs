// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

pub mod bluetooth;
pub mod channel;
pub mod clipboard;
pub mod config;
pub mod discovery;
pub mod error;
pub mod identity;
pub mod merkle;
pub mod protocol;
pub mod security;
pub mod transport;
pub mod watcher;

pub use bluetooth::{
    has_bluetooth_radio, list_paired_bluetooth_devices, send_bluetooth_clipboard_image,
    send_bluetooth_clipboard_text, send_bluetooth_quick_batch, start_bluetooth_server,
    BluetoothPeerInfo, LIERSYNC_BT_UUID_STR,
};
pub use channel::{
    probe_channels, probe_channels_for_lang, select_active_medium, ChannelDescriptor,
    ChannelPolicy, TransportMedium,
};

pub use clipboard::{
    get_clipboard_image_png, get_clipboard_text, set_clipboard_image_png, set_clipboard_text,
    ClipboardDirection, ClipboardEntry, ClipboardGuard,
};
pub use config::{
    AppConfig, AppLanguage, SharedFolderConfig, SyncDirection, TransferDirection, TransferRecord,
    TrustedPeer,
};
pub use discovery::{detect_local_ipv4s, DiscoveryService, PeerInfo, DISCOVERY_PORT};
pub use error::{LierError, Result};
pub use identity::DeviceIdentity;
pub use merkle::{
    build_merkle_tree, diff_trees, hash_bytes, hash_file, preview_sync, MerkleTree, SyncPreview,
    CHUNK_SIZE,
};
pub use security::{
    build_pairing_uri, generate_pairing_pin, generate_qr_png, parse_pairing_uri, PairingPayload,
    SecurityManager, SecurityPolicy, SessionCipher, StreamRole,
};
pub use transport::{
    compare_folder_with_peer, expand_paths_for_send, fetch_clipboard_from_peer,
    fetch_clipboard_payload_from_peer, pair_with_peer, ping_peer, push_folder_to_peer,
    send_clipboard_image_png, send_clipboard_text, send_quick_batch, send_quick_file,
    start_p2p_server, start_p2p_server_secure, sync_folder_by_direction, sync_folder_from_peer,
    BatchItem, CoreEvent, RemoteClipboardPayload, SharedFolderRegistry, DEFAULT_P2P_PORT,
};
pub use watcher::{diff_snapshots, spawn_folder_watcher, FolderChangeEvent};
