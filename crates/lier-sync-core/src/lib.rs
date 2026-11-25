// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

pub mod channel;
pub mod clipboard;
pub mod config;
pub mod discovery;
pub mod error;
pub mod identity;
pub mod merkle;
pub mod protocol;
pub mod transport;
pub mod watcher;

pub use channel::{
    probe_channels, probe_channels_for_lang, select_active_medium, ChannelDescriptor,
    ChannelPolicy, TransportMedium,
};

pub use clipboard::{
    get_clipboard_text, set_clipboard_text, ClipboardDirection, ClipboardEntry, ClipboardGuard,
};
pub use config::{
    AppConfig, AppLanguage, SharedFolderConfig, SyncDirection, TransferDirection, TransferRecord,
};
pub use discovery::{detect_local_ipv4s, DiscoveryService, PeerInfo, DISCOVERY_PORT};
pub use error::{LierError, Result};
pub use identity::DeviceIdentity;
pub use merkle::{
    build_merkle_tree, diff_trees, hash_bytes, hash_file, preview_sync, MerkleTree, SyncPreview,
    CHUNK_SIZE,
};
pub use transport::{
    compare_folder_with_peer, expand_paths_for_send, fetch_clipboard_from_peer, ping_peer,
    push_folder_to_peer, send_clipboard_text, send_quick_batch, send_quick_file,
    start_p2p_server, sync_folder_by_direction, sync_folder_from_peer, BatchItem, CoreEvent,
    SharedFolderRegistry, DEFAULT_P2P_PORT,
};
pub use watcher::{spawn_folder_watcher, FolderChangeEvent};
