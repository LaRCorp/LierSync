// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod i18n;

use chrono::Local;
use i18n::{dir_choices, folder_status_label, policy_choices, tr, DirChoice, PolicyChoice};
use iced::theme::Palette;
use iced::widget::{
    button, checkbox, column, container, image as iced_image, pick_list, progress_bar, row, rule,
    scrollable, svg, text, text_input, Space,
};
use iced::{
    event, time, window, Alignment, Background, Border, Color, Element, Fill, Size, Subscription,
    Task, Theme,
};
use lier_sync_core::{
    build_pairing_uri, compare_folder_with_peer, detect_local_ipv4s,
    fetch_clipboard_payload_from_peer, generate_qr_png, get_clipboard_image_png,
    get_clipboard_text, list_paired_bluetooth_devices, pair_with_peer, ping_peer,
    probe_channels_for_lang, select_active_medium, send_bluetooth_clipboard_image,
    send_bluetooth_clipboard_text, send_bluetooth_quick_batch, send_clipboard_image_png,
    send_clipboard_text, send_quick_batch, set_clipboard_image_png, set_clipboard_text,
    spawn_folder_watcher, start_bluetooth_server, start_p2p_server_secure,
    sync_folder_by_direction, AppConfig, AppLanguage, BluetoothPeerInfo, ChannelDescriptor,
    ChannelPolicy, ClipboardDirection, ClipboardGuard, CoreEvent, DeviceIdentity, DiscoveryService,
    FolderChangeEvent, PeerInfo, RemoteClipboardPayload, SecurityManager, SecurityPolicy,
    SharedFolderConfig, SharedFolderRegistry, SyncDirection, SyncPreview, TransferDirection,
    TransferRecord, DEFAULT_P2P_PORT,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

// LierTags-inspired Dark Crimson + Teal palette
const BG_BASE: Color = Color::from_rgb(0x12 as f32 / 255.0, 0x13 as f32 / 255.0, 0x16 as f32 / 255.0);
const BG_SURFACE: Color = Color::from_rgb(0x1A as f32 / 255.0, 0x1B as f32 / 255.0, 0x20 as f32 / 255.0);
const BG_ELEVATED: Color = Color::from_rgb(0x24 as f32 / 255.0, 0x26 as f32 / 255.0, 0x2E as f32 / 255.0);
const BORDER_COLOR: Color = Color::from_rgb(0x33 as f32 / 255.0, 0x36 as f32 / 255.0, 0x42 as f32 / 255.0);
const ACCENT_TEAL: Color = Color::from_rgb(0x39 as f32 / 255.0, 0xC5 as f32 / 255.0, 0xBB as f32 / 255.0);
const ACCENT_RED: Color = Color::from_rgb(0x8B as f32 / 255.0, 0x00 as f32 / 255.0, 0x00 as f32 / 255.0);
const ACCENT_RED_HOVER: Color = Color::from_rgb(0xA8 as f32 / 255.0, 0x13 as f32 / 255.0, 0x13 as f32 / 255.0);
const TEXT_MAIN: Color = Color::from_rgb(0xE0 as f32 / 255.0, 0xE2 as f32 / 255.0, 0xEC as f32 / 255.0);
const TEXT_MUTED: Color = Color::from_rgb(0x9E as f32 / 255.0, 0xA2 as f32 / 255.0, 0xB0 as f32 / 255.0);

const LOGO_PNG: &[u8] = include_bytes!("../assets/logo_128.png");
const ICON_SEND: &[u8] = include_bytes!("../assets/icons/send.svg");
const ICON_SYNC: &[u8] = include_bytes!("../assets/icons/sync.svg");
const ICON_CLIPBOARD: &[u8] = include_bytes!("../assets/icons/clipboard.svg");
const ICON_CHANNELS: &[u8] = include_bytes!("../assets/icons/channels.svg");
const ICON_FILE: &[u8] = include_bytes!("../assets/icons/file.svg");
const ICON_FOLDER: &[u8] = include_bytes!("../assets/icons/folder.svg");
const ICON_TRASH: &[u8] = include_bytes!("../assets/icons/trash.svg");
const ICON_SEARCH: &[u8] = include_bytes!("../assets/icons/search.svg");
const ICON_PLUS: &[u8] = include_bytes!("../assets/icons/plus.svg");
const ICON_CLOSE: &[u8] = include_bytes!("../assets/icons/close.svg");
const ICON_COPY: &[u8] = include_bytes!("../assets/icons/copy.svg");
const ICON_INFO: &[u8] = include_bytes!("../assets/icons/info.svg");
const ICON_ARROW_UP: &[u8] = include_bytes!("../assets/icons/arrow_up.svg");
const ICON_ARROW_DOWN: &[u8] = include_bytes!("../assets/icons/arrow_down.svg");
const ICON_SETTINGS: &[u8] = include_bytes!("../assets/icons/settings.svg");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveTab {
    QuickSend,
    SyncFolders,
    Clipboard,
    Channels,
    Settings,
    About,
}

#[derive(Debug, Clone)]
struct SharedFolderItem {
    id: String,
    path: PathBuf,
    status: String,
    remote_addr: String,
    auto_sync: Arc<AtomicBool>,
    direction: SyncDirection,
    last_preview: Option<SyncPreview>,
}

struct TrayState {
    tray: TrayIcon,
    id_open: MenuId,
    id_push_clip: MenuId,
    id_pull_clip: MenuId,
    id_open_downloads: MenuId,
    id_exit: MenuId,
    item_open: MenuItem,
    item_push_clip: MenuItem,
    item_pull_clip: MenuItem,
    item_open_dl: MenuItem,
    item_exit: MenuItem,
}

impl TrayState {
    fn apply_language(&self, lang: AppLanguage) {
        self.item_open.set_text(tr(lang, "Открыть LierSync", "Open LierSync"));
        self.item_push_clip.set_text(tr(lang, "Отправить буфер", "Send clipboard"));
        self.item_pull_clip.set_text(tr(lang, "Получить буфер", "Pull clipboard"));
        self.item_open_dl.set_text(tr(lang, "Открыть Загрузки\\LierSync", "Open Downloads\\LierSync"));
        self.item_exit.set_text(tr(lang, "Выход", "Exit"));
        let _ = self.tray.set_tooltip(Some(tr(
            lang,
            "LierSync — работает в фоне",
            "LierSync — running in background",
        )));
    }
}

#[derive(Debug, Clone)]
enum UiUpdate {
    Peers(Vec<PeerInfo>),
    QuickSendProgress(f32, String),
    QuickSendFinished(String),
    SentFilesRecorded(Vec<TransferRecord>),
    Log(String),
    FolderStatus(String, String),
    FolderChanged(FolderChangeEvent),
    FolderPreview(String, SyncPreview),
    PingResult(String),
    PairFinished(String),
}

#[derive(Debug, Clone)]
enum Message {
    Tick,
    WindowCloseRequested(window::Id),
    ShowOrFocusWindow,
    HideToTray,
    ExitApp,
    SelectTab(ActiveTab),
    SelectPeer(usize),
    ManualAddrChanged(String),
    CopyMyAddress,
    PingTarget,
    OpenDownloadsFolder,
    OpenPath(PathBuf),
    RevealPath(PathBuf),

    // Quick Send
    PickFiles,
    PickFolderForSend,
    FilesPicked(Vec<PathBuf>),
    FileDropped(PathBuf),
    RemoveQueuedItem(usize),
    ClearQueue,
    ClearTransferHistory,
    SendBatch,

    // Folder Sync
    NewFolderIdChanged(String),
    NewFolderRemoteChanged(String),
    NewFolderDirectionSelected(SyncDirection),
    PickSyncFolder,
    SyncFolderPicked(Option<PathBuf>),
    ToggleFolderAuto(usize, bool),
    ChangeFolderDirection(usize, SyncDirection),
    RemoveFolder(usize),
    CompareFolder(usize),
    SyncFolder(usize),

    // Clipboard
    ToggleAutoClipboard(bool),
    SendClipboardNow,
    PullClipboardNow,

    // Channels & Settings
    SelectChannelPolicy(ChannelPolicy),
    RelayEndpointChanged(String),
    SelectLanguage(AppLanguage),

    // Security & Pairing
    ToggleEncryptTransport(bool),
    ToggleWhitelistOnly(bool),
    RotatePairingPin,
    PairPinInputChanged(String),
    PairTargetPeer,
    RemoveTrustedPeer(String),

    // Bluetooth & System Share
    RefreshBtDevices,
    SelectBtDevice(String),
    SendClipboardBluetooth,
    SendQueueBluetooth,
    InstallSendToShortcut,
}

struct LierSyncApp {
    rt: Arc<tokio::runtime::Runtime>,
    main_window: Option<window::Id>,
    tray: Option<TrayState>,
    logo_handle: iced_image::Handle,
    qr_handle: iced_image::Handle,
    language: AppLanguage,
    identity: DeviceIdentity,
    security: SecurityManager,
    pair_pin_input: String,
    pair_status: String,
    bt_devices: Vec<BluetoothPeerInfo>,
    selected_bt_mac: String,
    p2p_port: u16,
    local_addr_label: String,
    receive_dir: PathBuf,
    tab: ActiveTab,

    peers: Vec<PeerInfo>,
    selected_peer_idx: Option<usize>,
    manual_addr: String,
    ping_status: String,

    send_queue: Vec<PathBuf>,
    transfer_history: Vec<TransferRecord>,
    send_progress: f32,
    send_status: String,

    shared_folders: Vec<SharedFolderItem>,
    new_folder_id: String,
    new_folder_remote: String,
    new_folder_direction: SyncDirection,
    folder_registry: SharedFolderRegistry,

    auto_clipboard: bool,
    auto_clipboard_flag: Arc<AtomicBool>,
    clipboard_targets: Arc<Mutex<Vec<SocketAddr>>>,
    clipboard_guard: ClipboardGuard,

    channel_policy: ChannelPolicy,
    relay_endpoint: String,
    channels: Vec<ChannelDescriptor>,

    logs: Vec<String>,
    ui_rx: Arc<Mutex<mpsc::Receiver<UiUpdate>>>,
    ui_tx: mpsc::Sender<UiUpdate>,
    core_rx: Arc<Mutex<mpsc::Receiver<CoreEvent>>>,
}

impl LierSyncApp {
    fn new() -> (Self, Task<Message>) {
        let rt = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Failed to create Tokio runtime"),
        );

        let identity = DeviceIdentity::load_or_create().expect("Failed to load device identity");
        let receive_dir = DeviceIdentity::default_download_dir();
        let folder_registry = SharedFolderRegistry::new();
        let clipboard_guard = ClipboardGuard::new(50);

        let saved_cfg = AppConfig::load_from(&AppConfig::default_path());
        let lang = saved_cfg.language;
        let need_pin_save = saved_cfg.pairing_pin.trim().is_empty();
        let security = SecurityManager::new(SecurityPolicy {
            encrypt_transport: saved_cfg.encrypt_transport,
            whitelist_only: saved_cfg.whitelist_only,
            pairing_pin: saved_cfg.pairing_pin.clone(),
            trusted_peers: saved_cfg.trusted_peers.clone(),
        });

        let (core_tx, core_rx) = mpsc::channel::<CoreEvent>(128);
        let (ui_tx, ui_rx) = mpsc::channel::<UiUpdate>(128);

        let discovery = DiscoveryService::new();
        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), DEFAULT_P2P_PORT);
        let bound_port = rt.block_on(async {
            match start_p2p_server_secure(
                bind_addr,
                identity.clone(),
                receive_dir.clone(),
                folder_registry.clone(),
                clipboard_guard.clone(),
                discovery.clone(),
                security.clone(),
                core_tx.clone(),
            )
            .await
            {
                Ok(addr) => addr.port(),
                Err(_) => {
                    let fallback = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
                    start_p2p_server_secure(
                        fallback,
                        identity.clone(),
                        receive_dir.clone(),
                        folder_registry.clone(),
                        clipboard_guard.clone(),
                        discovery.clone(),
                        security.clone(),
                        core_tx.clone(),
                    )
                    .await
                    .map(|a| a.port())
                    .unwrap_or(DEFAULT_P2P_PORT)
                }
            }
        });

        let local_ips = detect_local_ipv4s();
        let local_addr_label = local_ips
            .first()
            .map(|ip| format!("{ip}:{bound_port}"))
            .unwrap_or_else(|| format!("127.0.0.1:{bound_port}"));

        {
            let _guard = rt.enter();
            let _ = discovery.start(identity.clone(), bound_port);
        }
        let _ = start_bluetooth_server(
            identity.clone(),
            receive_dir.clone(),
            clipboard_guard.clone(),
            security.clone(),
            core_tx.clone(),
        );
        let bt_devices = list_paired_bluetooth_devices();
        let selected_bt_mac = bt_devices.first().map(|d| d.mac.clone()).unwrap_or_default();

        let ui_tx_peers = ui_tx.clone();
        let disc_poll = discovery.clone();
        rt.spawn(async move {
            loop {
                let list = disc_poll.peers().await;
                let _ = ui_tx_peers.send(UiUpdate::Peers(list)).await;
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });

        let auto_clipboard_flag = Arc::new(AtomicBool::new(saved_cfg.auto_clipboard));
        let clipboard_targets: Arc<Mutex<Vec<SocketAddr>>> = Arc::new(Mutex::new(Vec::new()));
        {
            let flag = Arc::clone(&auto_clipboard_flag);
            let targets = Arc::clone(&clipboard_targets);
            let guard = clipboard_guard.clone();
            let id = identity.clone();
            let ui_tx_cb = ui_tx.clone();
            rt.spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    if !flag.load(Ordering::Relaxed) {
                        continue;
                    }
                    let addrs = targets.lock().map(|g| g.clone()).unwrap_or_default();
                    if addrs.is_empty() {
                        continue;
                    }
                    if let Ok(text) = get_clipboard_text() {
                        if let Some(hash) = guard.should_emit_local(&text) {
                            for addr in addrs {
                                guard.record_sent(&addr.to_string(), &text, &hash);
                                if let Err(e) = send_clipboard_text(addr, &id, &text).await {
                                    let _ = ui_tx_cb
                                        .send(UiUpdate::Log(format!("Ошибка автобуфера ({addr}): {e}")))
                                        .await;
                                }
                            }
                        }
                    } else if let Ok((w, h, png)) = get_clipboard_image_png() {
                        if let Some(hash) = guard.should_emit_local_image(&png) {
                            for addr in addrs {
                                guard.record_sent_image(&addr.to_string(), w, h, &png, &hash);
                                if let Err(e) = send_clipboard_image_png(addr, &id, w, h, png.clone()).await {
                                    let _ = ui_tx_cb
                                        .send(UiUpdate::Log(format!("Ошибка автобуфера ({addr}): {e}")))
                                        .await;
                                }
                            }
                        }
                    }
                }
            });
        }

        let channels = probe_channels_for_lang(
            bound_port,
            Some(saved_cfg.relay_endpoint.as_str()).filter(|s| !s.is_empty()),
            lang,
        );
        let default_manual = if !saved_cfg.manual_addr.trim().is_empty() {
            saved_cfg.manual_addr.clone()
        } else {
            local_ips
                .first()
                .map(|ip| {
                    let o = ip.octets();
                    format!("{}.{}.{}.1:{DEFAULT_P2P_PORT}", o[0], o[1], o[2])
                })
                .unwrap_or_else(|| format!("127.0.0.1:{DEFAULT_P2P_PORT}"))
        };

        let (win_id, open_task) = window::open(main_window_settings());
        let tray = create_tray_icon(lang);
        let logo_handle = iced_image::Handle::from_bytes(LOGO_PNG);
        let initial_uri = build_pairing_uri(
            &local_addr_label,
            &identity.device_id,
            &identity.device_name,
            &identity.fingerprint,
            &security.pairing_pin(),
        );
        let qr_png = generate_qr_png(&initial_uri, 200).unwrap_or_default();
        let qr_handle = iced_image::Handle::from_bytes(qr_png);

        let cli_send_queue: Vec<PathBuf> = std::env::args()
            .skip(1)
            .filter(|a| a != "--send")
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .collect();

        let mut app = Self {
            rt,
            main_window: Some(win_id),
            tray,
            logo_handle,
            qr_handle,
            language: lang,
            identity,
            security,
            pair_pin_input: String::new(),
            pair_status: String::new(),
            bt_devices,
            selected_bt_mac,
            p2p_port: bound_port,
            local_addr_label: local_addr_label.clone(),
            receive_dir,
            tab: ActiveTab::QuickSend,
            peers: Vec::new(),
            selected_peer_idx: None,
            manual_addr: default_manual.clone(),
            ping_status: tr(lang, "Не проверялось", "Not checked").to_string(),
            send_queue: cli_send_queue,
            transfer_history: saved_cfg.transfer_history,
            send_progress: 0.0,
            send_status: tr(
                lang,
                "Перетащите файлы или выберите их кнопками",
                "Drag & drop files here or pick them below",
            )
            .to_string(),
            shared_folders: Vec::new(),
            new_folder_id: "default".to_string(),
            new_folder_remote: default_manual,
            new_folder_direction: SyncDirection::Bidirectional,
            folder_registry,
            auto_clipboard: saved_cfg.auto_clipboard,
            auto_clipboard_flag,
            clipboard_targets,
            clipboard_guard,
            channel_policy: saved_cfg.channel_policy,
            relay_endpoint: saved_cfg.relay_endpoint,
            channels,
            logs: vec![format!(
                "[{}] {}: {local_addr_label}",
                Local::now().format("%H:%M:%S"),
                tr(lang, "LierSync готов. Мой адрес", "LierSync ready. My address")
            )],
            ui_rx: Arc::new(Mutex::new(ui_rx)),
            ui_tx,
            core_rx: Arc::new(Mutex::new(core_rx)),
        };

        for f in saved_cfg.shared_folders {
            let dir = PathBuf::from(&f.path);
            app.mount_shared_folder(f.id, dir, f.remote_addr, f.auto_sync, f.direction, false);
        }
        app.sync_clipboard_targets();
        if need_pin_save {
            app.save_config();
        }

        (app, open_task.discard())
    }

    fn title(&self, _window: window::Id) -> String {
        format!("LierSync — {}", self.local_addr_label)
    }

    fn theme(&self, _window: window::Id) -> Theme {
        Theme::custom(
            "LierSync".to_string(),
            Palette {
                background: BG_BASE,
                text: TEXT_MAIN,
                primary: ACCENT_TEAL,
                success: ACCENT_TEAL,
                warning: Color::from_rgb(0.92, 0.68, 0.22),
                danger: ACCENT_RED,
            },
        )
    }

    fn subscription(&self) -> Subscription<Message> {
        let ticks = time::every(Duration::from_millis(250)).map(|_| Message::Tick);
        let closes = window::close_requests().map(Message::WindowCloseRequested);
        let drops = event::listen_with(|ev, _status, _id| match ev {
            iced::Event::Window(window::Event::FileDropped(path)) => {
                Some(Message::FileDropped(path))
            }
            _ => None,
        });
        Subscription::batch([ticks, closes, drops])
    }

    fn target_addr(&self) -> Option<SocketAddr> {
        if let Some(idx) = self.selected_peer_idx {
            if let Some(peer) = self.peers.get(idx) {
                return Some(peer.addr);
            }
        }
        self.manual_addr.trim().parse::<SocketAddr>().ok()
    }

    fn active_recipient_label(&self) -> String {
        if let Some(idx) = self.selected_peer_idx {
            if let Some(peer) = self.peers.get(idx) {
                return format!("{} ({})", peer.device_name, peer.addr);
            }
        }
        if let Ok(addr) = self.manual_addr.trim().parse::<SocketAddr>() {
            format!("{}: {addr}", tr(self.language, "Вручную", "Manual"))
        } else {
            tr(self.language, "Не выбран", "Not selected").to_string()
        }
    }

    fn sync_clipboard_targets(&self) {
        let mut addrs: Vec<SocketAddr> = self.peers.iter().map(|p| p.addr).collect();
        if addrs.is_empty() {
            if let Some(manual) = self.target_addr() {
                addrs.push(manual);
            }
        }
        if let Ok(mut guard) = self.clipboard_targets.lock() {
            *guard = addrs;
        }
    }

    fn save_config(&self) {
        let sec = self.security.snapshot();
        let cfg = AppConfig {
            manual_addr: self.manual_addr.clone(),
            auto_clipboard: self.auto_clipboard,
            channel_policy: self.channel_policy,
            relay_endpoint: self.relay_endpoint.clone(),
            shared_folders: self
                .shared_folders
                .iter()
                .map(|f| SharedFolderConfig {
                    id: f.id.clone(),
                    path: f.path.display().to_string(),
                    remote_addr: f.remote_addr.clone(),
                    auto_sync: f.auto_sync.load(Ordering::Relaxed),
                    direction: f.direction,
                })
                .collect(),
            transfer_history: self.transfer_history.iter().take(60).cloned().collect(),
            language: self.language,
            encrypt_transport: sec.encrypt_transport,
            whitelist_only: sec.whitelist_only,
            pairing_pin: sec.pairing_pin,
            trusted_peers: sec.trusted_peers,
        };
        let _ = cfg.save_to(&AppConfig::default_path());
    }

    fn mount_shared_folder(
        &mut self,
        id: String,
        dir: PathBuf,
        remote_addr: String,
        auto_enabled: bool,
        direction: SyncDirection,
        persist: bool,
    ) {
        let reg = self.folder_registry.clone();
        let dir_reg = dir.clone();
        let id_reg = id.clone();
        self.rt.spawn(async move {
            reg.register(id_reg, dir_reg).await;
        });

        let auto_flag = Arc::new(AtomicBool::new(auto_enabled));
        let dir_flag = Arc::new(std::sync::Mutex::new(direction));
        let (watch_tx, mut watch_rx) = mpsc::channel(16);
        {
            let _guard = self.rt.enter();
            spawn_folder_watcher(id.clone(), dir.clone(), Duration::from_secs(3), watch_tx);
        }

        let ui_tx = self.ui_tx.clone();
        let auto_clone = Arc::clone(&auto_flag);
        let dir_flag_clone = Arc::clone(&dir_flag);
        let remote_str = remote_addr.clone();
        let identity_clone = self.identity.clone();
        let fid_clone = id.clone();
        let dir_clone = dir.clone();
        self.rt.spawn(async move {
            while let Some(ev) = watch_rx.recv().await {
                let fid_ev = ev.folder_id.clone();
                let _ = ui_tx.send(UiUpdate::FolderChanged(ev)).await;
                if auto_clone.load(Ordering::Relaxed) {
                    if let Ok(addr) = remote_str.trim().parse::<SocketAddr>() {
                        let d_mode = *dir_flag_clone.lock().unwrap();
                        if let Ok(chunks) = sync_folder_by_direction(
                            addr,
                            &identity_clone,
                            &fid_clone,
                            &dir_clone,
                            d_mode,
                        )
                        .await
                        {
                            if chunks > 0 {
                                let _ = ui_tx
                                    .send(UiUpdate::FolderStatus(
                                        fid_ev,
                                        format!("Авто-синхр. ({chunks} бл.)"),
                                    ))
                                    .await;
                            }
                        }
                    }
                }
            }
        });

        self.shared_folders.push(SharedFolderItem {
            id: id.clone(),
            path: dir.clone(),
            status: folder_status_label(auto_enabled, direction, self.language),
            remote_addr,
            auto_sync: auto_flag,
            direction,
            last_preview: None,
        });
        if persist {
            self.save_config();
            self.push_log(format!("Добавлена общая папка '{id}': {}", dir.display()));
        }
    }

    fn push_log(&mut self, msg: impl Into<String>) {
        let line = format!("[{}] {}", Local::now().format("%H:%M:%S"), msg.into());
        self.logs.insert(0, line);
        if self.logs.len() > 200 {
            self.logs.truncate(200);
        }
    }

    fn drain_async_events(&mut self) {
        let mut core_events = Vec::new();
        if let Ok(mut rx) = self.core_rx.lock() {
            while let Ok(ev) = rx.try_recv() {
                core_events.push(ev);
            }
        }
        for ev in core_events {
            match ev {
                CoreEvent::IncomingFile {
                    from_peer,
                    file_name,
                    saved_path,
                    bytes,
                    ..
                } => {
                    self.transfer_history.insert(
                        0,
                        TransferRecord {
                            file_name: file_name.clone(),
                            peer: from_peer.clone(),
                            path: saved_path.display().to_string(),
                            content_uri: None,
                            bytes,
                            time: Local::now().format("%H:%M:%S").to_string(),
                            direction: TransferDirection::Received,
                        },
                    );
                    if self.transfer_history.len() > 60 {
                        self.transfer_history.truncate(60);
                    }
                    self.save_config();
                    self.push_log(format!(
                        "{} '{file_name}' ({bytes} {}) <- {from_peer} -> {}",
                        tr(self.language, "Получен файл", "Received file"),
                        tr(self.language, "Б", "B"),
                        saved_path.display()
                    ));
                }
                CoreEvent::ClipboardReceived {
                    from_peer,
                    preview,
                    ..
                } => {
                    self.push_log(format!(
                        "{} {from_peer}: {preview}",
                        tr(self.language, "Буфер от", "Clipboard from")
                    ));
                }
                CoreEvent::ClipboardImageReceived {
                    from_peer,
                    width,
                    height,
                    ..
                } => {
                    self.push_log(format!(
                        "{} {from_peer}: [{} {width}x{height}]",
                        tr(self.language, "Картинка в буфере от", "Clipboard image from"),
                        tr(self.language, "Изображение", "Image")
                    ));
                }
                CoreEvent::SyncActivity { folder_id, message } => {
                    self.push_log(format!("[{folder_id}] {message}"));
                }
                CoreEvent::PeerPaired(peer) => {
                    self.save_config();
                    self.push_log(format!(
                        "{}: {} ({})",
                        tr(self.language, "Сопряжено устройство", "Device paired"),
                        peer.device_name,
                        peer.fingerprint
                    ));
                }
                CoreEvent::Error(e) => {
                    self.push_log(format!("{}: {e}", tr(self.language, "Ошибка", "Error")));
                }
            }
        }

        let mut updates = Vec::new();
        if let Ok(mut rx) = self.ui_rx.lock() {
            while let Ok(upd) = rx.try_recv() {
                updates.push(upd);
            }
        }

        for upd in updates {
            match upd {
                UiUpdate::Peers(list) => {
                    self.peers = list;
                    self.sync_clipboard_targets();
                }
                UiUpdate::QuickSendProgress(p, msg) => {
                    self.send_progress = p;
                    self.send_status = msg;
                }
                UiUpdate::QuickSendFinished(msg) => {
                    self.send_progress = 1.0;
                    self.send_status = msg.clone();
                    self.push_log(msg);
                }
                UiUpdate::SentFilesRecorded(records) => {
                    for rec in records.into_iter().rev() {
                        self.transfer_history.insert(0, rec);
                    }
                    if self.transfer_history.len() > 60 {
                        self.transfer_history.truncate(60);
                    }
                    self.save_config();
                }
                UiUpdate::Log(msg) => self.push_log(msg),
                UiUpdate::FolderStatus(id, status) => {
                    if let Some(f) = self.shared_folders.iter_mut().find(|f| f.id == id) {
                        f.status = status;
                    }
                }
                UiUpdate::FolderChanged(ev) => {
                    let summary = ev.format_alert(self.language);
                    if let Some(f) = self.shared_folders.iter_mut().find(|f| f.id == ev.folder_id) {
                        f.status = format!(
                            "{}: {summary}",
                            tr(self.language, "Изменено", "Changed")
                        );
                    }
                    self.push_log(format!(
                        "[{}] {}: {summary}",
                        ev.folder_id,
                        tr(self.language, "Обнаружены изменения", "Folder changes detected")
                    ));
                }
                UiUpdate::FolderPreview(id, preview) => {
                    if let Some(f) = self.shared_folders.iter_mut().find(|f| f.id == id) {
                        f.last_preview = Some(preview);
                    }
                }
                UiUpdate::PingResult(msg) => {
                    self.ping_status = msg.clone();
                    self.push_log(msg);
                }
                UiUpdate::PairFinished(msg) => {
                    self.pair_status = msg.clone();
                    self.push_log(msg);
                    self.save_config();
                }
            }
        }
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => {
                self.drain_async_events();
                let mut tasks = Vec::new();
                while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
                    match ev {
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                        | TrayIconEvent::DoubleClick {
                            button: MouseButton::Left,
                            ..
                        } => {
                            tasks.push(Task::done(Message::ShowOrFocusWindow));
                        }
                        _ => {}
                    }
                }
                while let Ok(ev) = MenuEvent::receiver().try_recv() {
                    if let Some(tray) = &self.tray {
                        if ev.id == tray.id_open {
                            tasks.push(Task::done(Message::ShowOrFocusWindow));
                        } else if ev.id == tray.id_push_clip {
                            tasks.push(Task::done(Message::SendClipboardNow));
                        } else if ev.id == tray.id_pull_clip {
                            tasks.push(Task::done(Message::PullClipboardNow));
                        } else if ev.id == tray.id_open_downloads {
                            tasks.push(Task::done(Message::OpenDownloadsFolder));
                        } else if ev.id == tray.id_exit {
                            tasks.push(Task::done(Message::ExitApp));
                        }
                    }
                }
                Task::batch(tasks)
            }
            Message::WindowCloseRequested(id) => {
                if self.tray.is_some() {
                    if self.main_window == Some(id) {
                        self.main_window = None;
                    }
                    window::close(id)
                } else {
                    iced::exit()
                }
            }
            Message::HideToTray => {
                if let Some(id) = self.main_window.take() {
                    window::close(id)
                } else {
                    Task::none()
                }
            }
            Message::ShowOrFocusWindow => {
                if let Some(id) = self.main_window {
                    window::gain_focus(id)
                } else {
                    let (id, open_task) = window::open(main_window_settings());
                    self.main_window = Some(id);
                    open_task.discard()
                }
            }
            Message::ExitApp => iced::exit(),
            Message::SelectTab(tab) => {
                self.tab = tab;
                Task::none()
            }
            Message::SelectPeer(idx) => {
                self.selected_peer_idx = Some(idx);
                if let Some(peer) = self.peers.get(idx) {
                    self.manual_addr = peer.addr.to_string();
                    self.new_folder_remote = peer.addr.to_string();
                    self.save_config();
                }
                self.sync_clipboard_targets();
                Task::none()
            }
            Message::ManualAddrChanged(val) => {
                self.manual_addr = val;
                self.selected_peer_idx = None;
                self.sync_clipboard_targets();
                self.save_config();
                Task::none()
            }
            Message::CopyMyAddress => {
                let _ = self.clipboard_guard.should_emit_local(&self.local_addr_label);
                match set_clipboard_text(&self.local_addr_label) {
                    Ok(()) => self.push_log(format!(
                        "{}: {}",
                        tr(self.language, "Адрес скопирован", "Address copied"),
                        self.local_addr_label
                    )),
                    Err(e) => self.push_log(format!(
                        "{}: {e}",
                        tr(self.language, "Не удалось скопировать", "Copy failed")
                    )),
                }
                Task::none()
            }
            Message::PingTarget => {
                let l = self.language;
                if let Some(addr) = self.target_addr() {
                    self.ping_status = format!("{} {addr}...", tr(l, "Проверка", "Checking"));
                    let id = self.identity.clone();
                    let ui_tx = self.ui_tx.clone();
                    self.rt.spawn(async move {
                        let msg = match ping_peer(addr, &id).await {
                            Ok((name, os, ms)) => format!(
                                "{}: {name} ({os}) · {ms} {}",
                                tr(l, "Связь OK", "Link OK"),
                                tr(l, "мс", "ms")
                            ),
                            Err(e) => format!(
                                "{} {addr}: {e}",
                                tr(l, "Нет связи с", "Unreachable")
                            ),
                        };
                        let _ = ui_tx.send(UiUpdate::PingResult(msg)).await;
                    });
                } else {
                    self.ping_status = tr(l, "Введите корректный ip:port", "Enter valid ip:port").into();
                }
                Task::none()
            }
            Message::OpenDownloadsFolder => {
                let _ = std::fs::create_dir_all(&self.receive_dir);
                open_in_os(&self.receive_dir);
                Task::none()
            }
            Message::OpenPath(path) => {
                open_in_os(&path);
                Task::none()
            }
            Message::RevealPath(path) => {
                reveal_in_os(&path);
                Task::none()
            }
            Message::PickFiles => Task::perform(
                async { rfd::FileDialog::new().pick_files().unwrap_or_default() },
                Message::FilesPicked,
            ),
            Message::PickFolderForSend => Task::perform(
                async { rfd::FileDialog::new().pick_folder().into_iter().collect() },
                Message::FilesPicked,
            ),
            Message::FilesPicked(paths) => {
                let mut added = 0;
                for p in paths {
                    if !self.send_queue.contains(&p) {
                        self.send_queue.push(p);
                        added += 1;
                    }
                }
                if added > 0 {
                    self.push_log(format!(
                        "{}: {added}",
                        tr(self.language, "В очередь добавлено объектов", "Queued items")
                    ));
                }
                Task::none()
            }
            Message::FileDropped(path) => {
                if !self.send_queue.contains(&path) {
                    self.send_queue.push(path);
                    self.push_log(tr(
                        self.language,
                        "Добавлен объект перетаскиванием",
                        "Added dropped item",
                    ));
                }
                Task::none()
            }
            Message::RemoveQueuedItem(idx) => {
                if idx < self.send_queue.len() {
                    self.send_queue.remove(idx);
                }
                Task::none()
            }
            Message::ClearQueue => {
                self.send_queue.clear();
                Task::none()
            }
            Message::ClearTransferHistory => {
                self.transfer_history.clear();
                self.save_config();
                Task::none()
            }
            Message::SendBatch => {
                let l = self.language;
                if let Some(addr) = self.target_addr() {
                    if !self.send_queue.is_empty() {
                        let paths = self.send_queue.clone();
                        let id = self.identity.clone();
                        let ui_tx = self.ui_tx.clone();
                        self.send_progress = 0.0;
                        self.send_status = format!(
                            "{} {} -> {addr}...",
                            tr(l, "Отправка", "Sending"),
                            paths.len()
                        );
                        self.rt.spawn(async move {
                            let unit_b = tr(l, "Б", "B");
                            let res = send_quick_batch(
                                addr,
                                &id,
                                &paths,
                                |idx, total_files, sent, total_bytes, name| {
                                    let p = if total_bytes > 0 {
                                        sent as f32 / total_bytes as f32
                                    } else {
                                        1.0
                                    };
                                    let _ = ui_tx.try_send(UiUpdate::QuickSendProgress(
                                        p,
                                        format!("[{idx}/{total_files}] {name} ({sent}/{total_bytes} {unit_b})"),
                                    ));
                                },
                            )
                            .await;
                            match res {
                                Ok(count) => {
                                    let now = Local::now().format("%H:%M:%S").to_string();
                                    let sent_recs: Vec<TransferRecord> = paths
                                        .iter()
                                        .map(|p| TransferRecord {
                                            file_name: p
                                                .file_name()
                                                .and_then(|n| n.to_str())
                                                .unwrap_or("item")
                                                .to_string(),
                                            peer: addr.to_string(),
                                            path: p.display().to_string(),
                                            content_uri: None,
                                            bytes: std::fs::metadata(p).map(|m| m.len()).unwrap_or(0),
                                            time: now.clone(),
                                            direction: TransferDirection::Sent,
                                        })
                                        .collect();
                                    let _ = ui_tx.send(UiUpdate::SentFilesRecorded(sent_recs)).await;
                                    let _ = ui_tx
                                        .send(UiUpdate::QuickSendFinished(format!(
                                            "{}: {count} -> {addr}",
                                            tr(l, "Отправлено файлов", "Sent files")
                                        )))
                                        .await;
                                }
                                Err(e) => {
                                    let _ = ui_tx
                                        .send(UiUpdate::QuickSendFinished(format!(
                                            "{}: {e}",
                                            tr(l, "Ошибка", "Error")
                                        )))
                                        .await;
                                }
                            }
                        });
                    }
                }
                Task::none()
            }
            Message::NewFolderIdChanged(s) => {
                self.new_folder_id = s;
                Task::none()
            }
            Message::NewFolderRemoteChanged(s) => {
                self.new_folder_remote = s;
                Task::none()
            }
            Message::NewFolderDirectionSelected(dir_mode) => {
                self.new_folder_direction = dir_mode;
                Task::none()
            }
            Message::PickSyncFolder => Task::perform(
                async { rfd::FileDialog::new().pick_folder() },
                Message::SyncFolderPicked,
            ),
            Message::SyncFolderPicked(Some(dir)) => {
                let id = self.new_folder_id.trim().to_string();
                if !id.is_empty() {
                    let remote = self.new_folder_remote.clone();
                    let dir_mode = self.new_folder_direction;
                    self.mount_shared_folder(id, dir, remote, true, dir_mode, true);
                }
                Task::none()
            }
            Message::SyncFolderPicked(None) => Task::none(),
            Message::ToggleFolderAuto(idx, enabled) => {
                let lang = self.language;
                if let Some(folder) = self.shared_folders.get_mut(idx) {
                    folder.auto_sync.store(enabled, Ordering::Relaxed);
                    folder.status = folder_status_label(enabled, folder.direction, lang);
                }
                self.save_config();
                Task::none()
            }
            Message::ChangeFolderDirection(idx, dir_mode) => {
                let lang = self.language;
                if let Some(folder) = self.shared_folders.get_mut(idx) {
                    folder.direction = dir_mode;
                    let auto_on = folder.auto_sync.load(Ordering::Relaxed);
                    folder.status = folder_status_label(auto_on, dir_mode, lang);
                }
                self.save_config();
                Task::none()
            }
            Message::RemoveFolder(idx) => {
                if idx < self.shared_folders.len() {
                    let removed = self.shared_folders.remove(idx);
                    let reg = self.folder_registry.clone();
                    let fid = removed.id.clone();
                    self.rt.spawn(async move {
                        reg.unregister(&fid).await;
                    });
                    self.save_config();
                    self.push_log(format!(
                        "{} '{}'",
                        tr(self.language, "Удалена общая папка", "Removed shared folder"),
                        removed.id
                    ));
                }
                Task::none()
            }
            Message::CompareFolder(idx) => {
                let l = self.language;
                if let Some(folder) = self.shared_folders.get(idx) {
                    if let Ok(addr) = folder.remote_addr.trim().parse::<SocketAddr>() {
                        let fid = folder.id.clone();
                        let dir = folder.path.clone();
                        let id = self.identity.clone();
                        let ui_tx = self.ui_tx.clone();
                        self.rt.spawn(async move {
                            match compare_folder_with_peer(addr, &id, &fid, &dir).await {
                                Ok(preview) => {
                                    let _ = ui_tx.send(UiUpdate::FolderPreview(fid, preview)).await;
                                }
                                Err(e) => {
                                    let _ = ui_tx
                                        .send(UiUpdate::FolderStatus(
                                            fid,
                                            format!("{}: {e}", tr(l, "Ошибка сравнения", "Compare error")),
                                        ))
                                        .await;
                                }
                            }
                        });
                    }
                }
                Task::none()
            }
            Message::SyncFolder(idx) => {
                let l = self.language;
                if let Some(folder) = self.shared_folders.get(idx) {
                    if let Ok(addr) = folder.remote_addr.trim().parse::<SocketAddr>() {
                        let fid = folder.id.clone();
                        let dir = folder.path.clone();
                        let dir_mode = folder.direction;
                        let id = self.identity.clone();
                        let ui_tx = self.ui_tx.clone();
                        self.rt.spawn(async move {
                            let _ = ui_tx
                                .send(UiUpdate::FolderStatus(
                                    fid.clone(),
                                    tr(l, "Синхронизация...", "Syncing...").into(),
                                ))
                                .await;
                            match sync_folder_by_direction(addr, &id, &fid, &dir, dir_mode).await {
                                Ok(chunks) => {
                                    let _ = ui_tx
                                        .send(UiUpdate::FolderStatus(
                                            fid.clone(),
                                            format!(
                                                "{} ({chunks} {})",
                                                tr(l, "Синхронизировано", "Synced"),
                                                tr(l, "бл.", "chunks")
                                            ),
                                        ))
                                        .await;
                                    let _ = ui_tx
                                        .send(UiUpdate::Log(format!(
                                            "{} '{fid}' ({}): {} {chunks}",
                                            tr(l, "Папка", "Folder"),
                                            dir_mode.label(l),
                                            tr(l, "обновлено блоков:", "updated chunks:")
                                        )))
                                        .await;
                                }
                                Err(e) => {
                                    let _ = ui_tx
                                        .send(UiUpdate::FolderStatus(
                                            fid,
                                            format!("{}: {e}", tr(l, "Ошибка", "Error")),
                                        ))
                                        .await;
                                }
                            }
                        });
                    }
                }
                Task::none()
            }
            Message::ToggleAutoClipboard(enabled) => {
                self.auto_clipboard = enabled;
                self.sync_clipboard_targets();
                self.auto_clipboard_flag.store(enabled, Ordering::Relaxed);
                self.save_config();
                self.push_log(if enabled {
                    tr(
                        self.language,
                        "Автосинхронизация буфера включена",
                        "Auto-clipboard enabled",
                    )
                } else {
                    tr(
                        self.language,
                        "Автосинхронизация буфера выключена",
                        "Auto-clipboard disabled",
                    )
                });
                Task::none()
            }
            Message::SendClipboardNow => {
                let l = self.language;
                if let Some(addr) = self.target_addr() {
                    if let Some(text) = get_clipboard_text().ok().filter(|t| !t.is_empty()) {
                        let id = self.identity.clone();
                        let guard = self.clipboard_guard.clone();
                        let ui_tx = self.ui_tx.clone();
                        let hash = lier_sync_core::hash_bytes(text.as_bytes());
                        guard.record_sent(&addr.to_string(), &text, &hash);
                        self.rt.spawn(async move {
                            if let Err(e) = send_clipboard_text(addr, &id, &text).await {
                                let _ = ui_tx
                                    .send(UiUpdate::Log(format!(
                                        "{}: {e}",
                                        tr(l, "Ошибка отправки буфера", "Clipboard send error")
                                    )))
                                    .await;
                            }
                        });
                        self.push_log(format!(
                            "{} -> {addr}",
                            tr(l, "Буфер отправлен", "Clipboard sent")
                        ));
                    } else if let Ok((w, h, png)) = get_clipboard_image_png() {
                        let id = self.identity.clone();
                        let guard = self.clipboard_guard.clone();
                        let ui_tx = self.ui_tx.clone();
                        let hash = lier_sync_core::hash_bytes(&png);
                        guard.record_sent_image(&addr.to_string(), w, h, &png, &hash);
                        self.rt.spawn(async move {
                            if let Err(e) = send_clipboard_image_png(addr, &id, w, h, png).await {
                                let _ = ui_tx
                                    .send(UiUpdate::Log(format!(
                                        "{}: {e}",
                                        tr(l, "Ошибка отправки картинки", "Image send error")
                                    )))
                                    .await;
                            }
                        });
                        self.push_log(format!(
                            "{} ({w}x{h}) -> {addr}",
                            tr(l, "Картинка буфера отправлена", "Clipboard image sent")
                        ));
                    } else {
                        self.push_log(tr(
                            l,
                            "Буфер обмена пуст (нет текста и картинки)",
                            "Clipboard is empty (no text or image)",
                        ));
                    }
                } else {
                    self.push_log(tr(
                        l,
                        "Укажите получателя во вкладке «Отправка»",
                        "Select a recipient in Quick Send tab",
                    ));
                }
                Task::none()
            }
            Message::PullClipboardNow => {
                let l = self.language;
                if let Some(addr) = self.target_addr() {
                    let id = self.identity.clone();
                    let guard = self.clipboard_guard.clone();
                    let ui_tx = self.ui_tx.clone();
                    self.rt.spawn(async move {
                        match fetch_clipboard_payload_from_peer(addr, &id).await {
                            Ok(RemoteClipboardPayload::Text {
                                sender_name,
                                sha256,
                                text,
                            }) => {
                                if text.is_empty() {
                                    let _ = ui_tx
                                        .send(UiUpdate::Log(
                                            tr(
                                                l,
                                                "Буфер обмена на удалённом устройстве пуст",
                                                "Remote device clipboard is empty",
                                            )
                                            .into(),
                                        ))
                                        .await;
                                } else if guard.accept_remote(&sender_name, &text, &sha256) {
                                    let _ = set_clipboard_text(&text);
                                    let preview: String = text.chars().take(60).collect();
                                    let _ = ui_tx
                                        .send(UiUpdate::Log(format!(
                                            "{} {sender_name}: {preview}",
                                            tr(l, "Получен буфер от", "Clipboard pulled from")
                                        )))
                                        .await;
                                }
                            }
                            Ok(RemoteClipboardPayload::Image {
                                sender_name,
                                sha256,
                                width,
                                height,
                                png_bytes,
                            }) => {
                                if guard.accept_remote_image(
                                    &sender_name,
                                    width,
                                    height,
                                    &png_bytes,
                                    &sha256,
                                ) {
                                    let _ = set_clipboard_image_png(&png_bytes);
                                    let _ = ui_tx
                                        .send(UiUpdate::Log(format!(
                                            "{} {sender_name}: [{width}x{height}]",
                                            tr(l, "Получена картинка в буфер от", "Pulled clipboard image from")
                                        )))
                                        .await;
                                }
                            }
                            Err(e) => {
                                let _ = ui_tx
                                    .send(UiUpdate::Log(format!(
                                        "{}: {e}",
                                        tr(l, "Ошибка получения буфера", "Clipboard pull error")
                                    )))
                                    .await;
                            }
                        }
                    });
                } else {
                    self.push_log(tr(
                        l,
                        "Укажите получателя во вкладке «Отправка»",
                        "Select a recipient in Quick Send tab",
                    ));
                }
                Task::none()
            }
            Message::SelectChannelPolicy(policy) => {
                self.channel_policy = policy;
                self.save_config();
                self.push_log(format!(
                    "{}: {}",
                    tr(self.language, "Выбран режим каналов", "Channel policy selected"),
                    policy.label(self.language)
                ));
                Task::none()
            }
            Message::RelayEndpointChanged(val) => {
                self.relay_endpoint = val;
                self.channels = probe_channels_for_lang(
                    self.p2p_port,
                    Some(&self.relay_endpoint),
                    self.language,
                );
                self.save_config();
                Task::none()
            }
            Message::SelectLanguage(lang) => {
                self.language = lang;
                self.channels = probe_channels_for_lang(
                    self.p2p_port,
                    Some(&self.relay_endpoint),
                    lang,
                );
                for f in &mut self.shared_folders {
                    f.status = folder_status_label(
                        f.auto_sync.load(Ordering::Relaxed),
                        f.direction,
                        lang,
                    );
                }
                if let Some(tray) = &self.tray {
                    tray.apply_language(lang);
                }
                self.ping_status = tr(lang, "Не проверялось", "Not checked").to_string();
                self.send_status = tr(
                    lang,
                    "Перетащите файлы или выберите их кнопками",
                    "Drag & drop files here or pick them below",
                )
                .to_string();
                self.save_config();
                self.push_log(tr(lang, "Язык: Русский", "Language: English"));
                Task::none()
            }
            Message::ToggleEncryptTransport(enabled) => {
                self.security.set_encrypt_transport(enabled);
                self.save_config();
                self.push_log(if enabled {
                    tr(
                        self.language,
                        "Шифрование X25519 + ChaCha20 включено",
                        "X25519 + ChaCha20 encryption enabled",
                    )
                } else {
                    tr(
                        self.language,
                        "Шифрование выключено",
                        "Transport encryption disabled",
                    )
                });
                Task::none()
            }
            Message::ToggleWhitelistOnly(enabled) => {
                self.security.set_whitelist_only(enabled);
                self.save_config();
                self.push_log(if enabled {
                    tr(
                        self.language,
                        "Включён белый список доверенных устройств",
                        "Trusted whitelist mode enabled",
                    )
                } else {
                    tr(
                        self.language,
                        "Белый список отключён",
                        "Trusted whitelist mode disabled",
                    )
                });
                Task::none()
            }
            Message::RotatePairingPin => {
                let pin = self.security.rotate_pin();
                let uri = build_pairing_uri(
                    &self.local_addr_label,
                    &self.identity.device_id,
                    &self.identity.device_name,
                    &self.identity.fingerprint,
                    &pin,
                );
                if let Ok(png) = generate_qr_png(&uri, 200) {
                    self.qr_handle = iced_image::Handle::from_bytes(png);
                }
                self.save_config();
                self.push_log(format!(
                    "{}: {pin}",
                    tr(self.language, "Обновлён PIN сопряжения", "New pairing PIN")
                ));
                Task::none()
            }
            Message::PairPinInputChanged(v) => {
                self.pair_pin_input = v.chars().filter(|c| c.is_ascii_digit()).take(8).collect();
                Task::none()
            }
            Message::PairTargetPeer => {
                let l = self.language;
                if let Some(addr) = self.target_addr() {
                    let pin = self.pair_pin_input.trim().to_string();
                    if pin.is_empty() {
                        self.pair_status = tr(l, "Введите PIN другого устройства", "Enter peer PIN").into();
                        return Task::none();
                    }
                    self.pair_status = tr(l, "Сопряжение...", "Pairing...").into();
                    let id = self.identity.clone();
                    let sec = self.security.clone();
                    let ui_tx = self.ui_tx.clone();
                    self.rt.spawn(async move {
                        let msg = match pair_with_peer(addr, &id, &pin).await {
                            Ok(peer) => {
                                sec.add_or_update_trusted(
                                    &peer.device_id,
                                    &peer.device_name,
                                    &peer.fingerprint,
                                );
                                format!(
                                    "{}: {} ({})",
                                    tr(l, "Сопряжено", "Paired"),
                                    peer.device_name,
                                    peer.fingerprint
                                )
                            }
                            Err(e) => format!("{}: {e}", tr(l, "Ошибка сопряжения", "Pairing error")),
                        };
                        let _ = ui_tx.send(UiUpdate::PairFinished(msg)).await;
                    });
                } else {
                    self.pair_status = tr(l, "Укажите IP:порт пира", "Enter peer IP:port").into();
                }
                Task::none()
            }
            Message::RemoveTrustedPeer(id_or_fp) => {
                if self.security.remove_trusted(&id_or_fp) {
                    self.save_config();
                    self.push_log(tr(
                        self.language,
                        "Устройство удалено из белого списка",
                        "Removed device from whitelist",
                    ));
                }
                Task::none()
            }
            Message::RefreshBtDevices => {
                self.bt_devices = list_paired_bluetooth_devices();
                if self.selected_bt_mac.is_empty() {
                    if let Some(first) = self.bt_devices.first() {
                        self.selected_bt_mac = first.mac.clone();
                    }
                }
                self.channels = probe_channels_for_lang(
                    self.p2p_port,
                    Some(&self.relay_endpoint),
                    self.language,
                );
                Task::none()
            }
            Message::SelectBtDevice(mac) => {
                self.selected_bt_mac = mac;
                Task::none()
            }
            Message::SendClipboardBluetooth => {
                let l = self.language;
                if self.selected_bt_mac.is_empty() {
                    self.push_log(tr(l, "Выберите Bluetooth-устройство", "Select a Bluetooth device"));
                    return Task::none();
                }
                let mac = self.selected_bt_mac.clone();
                let id = self.identity.clone();
                let ui_tx = self.ui_tx.clone();
                if let Some(text) = get_clipboard_text().ok().filter(|t| !t.is_empty()) {
                    let guard = self.clipboard_guard.clone();
                    let hash = lier_sync_core::hash_bytes(text.as_bytes());
                    guard.record_sent(&format!("BT {mac}"), &text, &hash);
                    self.rt.spawn(async move {
                        let msg = match send_bluetooth_clipboard_text(mac, id, text).await {
                            Ok(()) => format!("{}: BT", tr(l, "Буфер отправлен через", "Clipboard sent via")),
                            Err(e) => format!("{}: {e}", tr(l, "Ошибка Bluetooth", "Bluetooth error")),
                        };
                        let _ = ui_tx.send(UiUpdate::Log(msg)).await;
                    });
                } else if let Ok((w, h, png)) = get_clipboard_image_png() {
                    let guard = self.clipboard_guard.clone();
                    let hash = lier_sync_core::hash_bytes(&png);
                    guard.record_sent_image(&format!("BT {mac}"), w, h, &png, &hash);
                    self.rt.spawn(async move {
                        let msg = match send_bluetooth_clipboard_image(mac, id, w, h, png).await {
                            Ok(()) => format!("{}: BT ({w}x{h})", tr(l, "Картинка отправлена через", "Image sent via")),
                            Err(e) => format!("{}: {e}", tr(l, "Ошибка Bluetooth", "Bluetooth error")),
                        };
                        let _ = ui_tx.send(UiUpdate::Log(msg)).await;
                    });
                } else {
                    self.push_log(tr(l, "Буфер обмена пуст", "Clipboard is empty"));
                }
                Task::none()
            }
            Message::SendQueueBluetooth => {
                let l = self.language;
                if self.selected_bt_mac.is_empty() || self.send_queue.is_empty() {
                    self.push_log(tr(
                        l,
                        "Выберите Bluetooth-устройство и добавьте файлы в очередь",
                        "Select a Bluetooth device and queue files first",
                    ));
                    return Task::none();
                }
                let mac = self.selected_bt_mac.clone();
                let id = self.identity.clone();
                let paths = self.send_queue.clone();
                let ui_tx = self.ui_tx.clone();
                self.push_log(tr(l, "Отправка файлов по Bluetooth...", "Sending files via Bluetooth..."));
                self.rt.spawn(async move {
                    let msg = match send_bluetooth_quick_batch(mac, id, paths).await {
                        Ok(n) => format!("{}: {n}", tr(l, "Отправлено файлов по Bluetooth", "Sent files via Bluetooth")),
                        Err(e) => format!("{}: {e}", tr(l, "Ошибка Bluetooth", "Bluetooth error")),
                    };
                    let _ = ui_tx.send(UiUpdate::Log(msg)).await;
                });
                Task::none()
            }
            Message::InstallSendToShortcut => {
                let l = self.language;
                match install_windows_sendto() {
                    Ok(path) => {
                        self.push_log(format!(
                            "{}: {}",
                            tr(l, "Ярлык SendTo создан", "SendTo shortcut created"),
                            path.display()
                        ));
                    }
                    Err(e) => {
                        self.push_log(format!(
                            "{}: {e}",
                            tr(l, "Ошибка создания ярлыка SendTo", "SendTo shortcut error")
                        ));
                    }
                }
                Task::none()
            }
        }
    }

    fn view(&self, _window: window::Id) -> Element<'_, Message> {
        let l = self.language;
        let active_medium = select_active_medium(self.channel_policy, &self.channels)
            .map(|m| m.to_string())
            .unwrap_or_else(|| tr(l, "Нет канала", "No channel").to_string());

        let logo_widget = iced_image(self.logo_handle.clone())
            .width(28)
            .height(28);

        let version_badge = container(text("v1.1").size(11).color(ACCENT_TEAL))
            .padding([3, 8])
            .style(|_| container::Style {
                background: Some(Background::Color(BG_ELEVATED)),
                border: Border {
                    width: 1.0,
                    color: Color {
                        a: 0.35,
                        ..ACCENT_TEAL
                    },
                    radius: 4.0.into(),
                },
                ..container::Style::default()
            });

        let header = container(
            row![
                row![logo_widget, text("LierSync").size(20), version_badge]
                    .spacing(8)
                    .align_y(Alignment::Center),
                Space::new().width(Fill),
                row![
                    column![
                        text(format!("{}: {}", tr(l, "Мой адрес", "My address"), self.local_addr_label))
                            .size(13)
                            .color(ACCENT_TEAL),
                        text(format!(
                            "{} · {}: {} · {active_medium}",
                            self.identity.device_name,
                            tr(l, "пиров", "peers"),
                            self.peers.len()
                        ))
                        .size(11)
                        .color(TEXT_MUTED),
                    ]
                    .align_x(Alignment::End)
                    .spacing(2),
                    button(
                        row![icon(ICON_COPY, 14.0), text(tr(l, "Копировать", "Copy")).size(12)]
                            .spacing(5)
                            .align_y(Alignment::Center)
                    )
                    .on_press(Message::CopyMyAddress)
                    .style(secondary_btn_style),
                    button(text(tr(l, "В трей", "To tray")).size(12))
                        .on_press(Message::HideToTray)
                        .style(secondary_btn_style),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            ]
            .align_y(Alignment::Center),
        )
        .padding([10, 14])
        .style(surface_card_style);

        let tabs = row![
            tab_button(tr(l, "Отправка", "Quick Send"), ICON_SEND, ActiveTab::QuickSend, self.tab),
            tab_button(tr(l, "Синхронизация", "Sync"), ICON_SYNC, ActiveTab::SyncFolders, self.tab),
            tab_button(tr(l, "Буфер обмена", "Clipboard"), ICON_CLIPBOARD, ActiveTab::Clipboard, self.tab),
            tab_button(tr(l, "Каналы связи", "Channels"), ICON_CHANNELS, ActiveTab::Channels, self.tab),
            Space::new().width(Fill),
            tab_button(tr(l, "Настройки", "Settings"), ICON_SETTINGS, ActiveTab::Settings, self.tab),
            tab_button(tr(l, "О программе", "About"), ICON_INFO, ActiveTab::About, self.tab),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let body: Element<'_, Message> = match self.tab {
            ActiveTab::QuickSend => self.view_quick_send(),
            ActiveTab::SyncFolders => self.view_sync_folders(),
            ActiveTab::Clipboard => self.view_clipboard(),
            ActiveTab::Channels => self.view_channels(),
            ActiveTab::Settings => self.view_settings(),
            ActiveTab::About => self.view_about(),
        };

        let footer = container(
            row![
                text(format!("{}: {}", tr(l, "Сохранение", "Save folder"), self.receive_dir.display()))
                    .size(11)
                    .color(TEXT_MUTED),
                button(
                    row![icon(ICON_FOLDER, 13.0), text(tr(l, "Открыть папку", "Open folder")).size(11)]
                        .spacing(5)
                        .align_y(Alignment::Center)
                )
                .on_press(Message::OpenDownloadsFolder)
                .style(secondary_btn_style),
                Space::new().width(Fill),
                text(format!("ID: {}", self.identity.fingerprint))
                    .size(11)
                    .color(TEXT_MUTED),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding([7, 12])
        .style(surface_card_style);

        container(
            column![header, tabs, container(body).height(Fill), footer]
                .spacing(10)
                .padding(14),
        )
        .width(Fill)
        .height(Fill)
        .into()
    }

    fn view_quick_send(&self) -> Element<'_, Message> {
        let l = self.language;
        let mut peers_col = column![
            row![
                text(tr(l, "1. Получатель", "1. Recipient")).size(15),
                Space::new().width(Fill),
                text(self.active_recipient_label())
                    .size(12)
                    .color(ACCENT_TEAL),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(6);

        if self.peers.is_empty() {
            peers_col = peers_col.push(
                text(tr(
                    l,
                    "Устройства в сети появятся автоматически. Либо введите адрес с другого устройства («Мой адрес»).",
                    "Devices on your network appear automatically, or enter the address shown on the other device.",
                ))
                .size(12)
                .color(TEXT_MUTED),
            );
        } else {
            for (i, peer) in self.peers.iter().enumerate() {
                let selected = self.selected_peer_idx == Some(i);
                let label = format!("{} ({}) — {}", peer.device_name, peer.os, peer.addr);
                let btn = button(text(label).size(13))
                    .on_press(Message::SelectPeer(i))
                    .style(if selected {
                        teal_btn_style
                    } else {
                        secondary_btn_style
                    });
                peers_col = peers_col.push(btn);
            }
        }

        peers_col = peers_col.push(
            row![
                text(tr(l, "IP:порт:", "IP:port:")).size(13).color(TEXT_MUTED),
                text_input("192.168.43.1:47789", &self.manual_addr)
                    .on_input(Message::ManualAddrChanged)
                    .width(210),
                button(
                    row![icon(ICON_SEARCH, 14.0), text(tr(l, "Проверить", "Ping")).size(12)]
                        .spacing(5)
                        .align_y(Alignment::Center)
                )
                .on_press(Message::PingTarget)
                .style(secondary_btn_style),
                text(&self.ping_status).size(12).color(TEXT_MUTED),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );

        let peers_card = container(peers_col).padding(12).width(Fill).style(surface_card_style);

        let actions = row![
            text(tr(l, "2. Файлы и папки", "2. Files & Folders")).size(15),
            Space::new().width(Fill),
            button(
                row![icon(ICON_FILE, 14.0), text(tr(l, "Выбрать файлы", "Pick files")).size(12)]
                    .spacing(5)
                    .align_y(Alignment::Center)
            )
            .on_press(Message::PickFiles)
            .style(secondary_btn_style),
            button(
                row![icon(ICON_FOLDER, 14.0), text(tr(l, "Добавить папку", "Add folder")).size(12)]
                    .spacing(5)
                    .align_y(Alignment::Center)
            )
            .on_press(Message::PickFolderForSend)
            .style(secondary_btn_style),
            button(
                row![icon(ICON_TRASH, 14.0), text(tr(l, "Очистить", "Clear")).size(12)]
                    .spacing(5)
                    .align_y(Alignment::Center)
            )
            .on_press(Message::ClearQueue)
            .style(crimson_btn_style),
            text(format!("{}: {}", tr(l, "В очереди", "Queued"), self.send_queue.len()))
                .size(12)
                .color(TEXT_MUTED),
        ]
        .spacing(8)
        .align_y(Alignment::Center);

        let mut queue_col = column![].spacing(4);
        if self.send_queue.is_empty() {
            queue_col = queue_col.push(
                text(tr(
                    l,
                    "Перетащите сюда любое количество файлов или папок",
                    "Drag & drop any files or folders here",
                ))
                .size(12)
                .color(TEXT_MUTED),
            );
        } else {
            for (i, path) in self.send_queue.iter().enumerate() {
                let kind_icon = if path.is_dir() { ICON_FOLDER } else { ICON_FILE };
                queue_col = queue_col.push(
                    row![
                        icon(kind_icon, 14.0),
                        text(path.display().to_string()).size(12).width(Fill),
                        button(icon(ICON_CLOSE, 13.0))
                            .on_press(Message::RemoveQueuedItem(i))
                            .style(button::text),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                );
            }
        }

        let send_btn = button(
            row![icon(ICON_SEND, 15.0), text(tr(l, "Отправить всё", "Send all")).size(13)]
                .spacing(6)
                .align_y(Alignment::Center),
        )
        .on_press_maybe(
            (!self.send_queue.is_empty() && self.target_addr().is_some())
                .then_some(Message::SendBatch),
        )
        .style(teal_btn_style);

        let transfer_card = container(
            column![
                actions,
                scrollable(queue_col).height(100),
                row![send_btn, progress_bar(0.0..=1.0, self.send_progress)]
                    .spacing(12)
                    .align_y(Alignment::Center),
                text(&self.send_status).size(12).color(TEXT_MUTED),
            ]
            .spacing(8),
        )
        .padding(12)
        .width(Fill)
        .style(surface_card_style);

        let mut hist_col = column![
            row![
                text(tr(l, "История передачи файлов:", "Transfer history:")).size(13),
                Space::new().width(Fill),
                button(text(tr(l, "Очистить", "Clear")).size(11))
                    .on_press(Message::ClearTransferHistory)
                    .style(secondary_btn_style),
            ]
            .align_y(Alignment::Center)
        ]
        .spacing(4);
        if self.transfer_history.is_empty() {
            hist_col = hist_col.push(
                text(tr(
                    l,
                    "Отправленные и полученные файлы будут сохраняться здесь",
                    "Sent and received files will appear here",
                ))
                .size(12)
                .color(TEXT_MUTED),
            );
        } else {
            let unit_b = tr(l, "Б", "B");
            for item in self.transfer_history.iter().take(20) {
                let (dir_icon, dir_txt) = match item.direction {
                    TransferDirection::Sent => (ICON_ARROW_UP, tr(l, "Отпр.", "Sent")),
                    TransferDirection::Received => (ICON_ARROW_DOWN, tr(l, "Получ.", "Recv")),
                };
                let p = PathBuf::from(&item.path);
                hist_col = hist_col.push(
                    row![
                        icon(dir_icon, 13.0),
                        text(format!(
                            "{} · {dir_txt} · {} ({} {unit_b}) · {}",
                            item.time, item.file_name, item.bytes, item.peer
                        ))
                        .size(12)
                        .width(Fill),
                        button(text(tr(l, "Открыть", "Open")).size(11))
                            .on_press(Message::OpenPath(p.clone()))
                            .style(teal_btn_style),
                        button(text(tr(l, "Папка", "Folder")).size(11))
                            .on_press(Message::RevealPath(p))
                            .style(secondary_btn_style),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                );
            }
        }
        let received_card = container(scrollable(hist_col).height(105))
            .padding(10)
            .width(Fill)
            .style(surface_card_style);

        column![peers_card, transfer_card, received_card, self.view_logs_card(85.0)]
            .spacing(8)
            .into()
    }

    fn view_sync_folders(&self) -> Element<'_, Message> {
        let l = self.language;
        let dir_opts = dir_choices(l);
        let add_bar = container(
            row![
                text("ID:").size(13).color(TEXT_MUTED),
                text_input("default", &self.new_folder_id)
                    .on_input(Message::NewFolderIdChanged)
                    .width(95),
                text(tr(l, "Пир:", "Peer:")).size(13).color(TEXT_MUTED),
                text_input("192.168.43.1:47789", &self.new_folder_remote)
                    .on_input(Message::NewFolderRemoteChanged)
                    .width(155),
                pick_list(
                    dir_opts,
                    Some(DirChoice(self.new_folder_direction, l)),
                    |c| Message::NewFolderDirectionSelected(c.0),
                )
                .width(195),
                button(
                    row![icon(ICON_PLUS, 14.0), text(tr(l, "Выбрать папку", "Choose folder")).size(12)]
                        .spacing(5)
                        .align_y(Alignment::Center)
                )
                .on_press(Message::PickSyncFolder)
                .style(teal_btn_style),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding(12)
        .width(Fill)
        .style(surface_card_style);

        let mut list_col = column![].spacing(8);
        if self.shared_folders.is_empty() {
            list_col = list_col.push(
                container(
                    text(tr(
                        l,
                        "Общих папок пока нет. Укажите одинаковый ID папки, выберите направление и папку на диске.",
                        "No shared folders yet. Enter matching folder ID, pick direction, and choose a folder.",
                    ))
                    .size(13)
                    .color(TEXT_MUTED),
                )
                .padding(12)
                .width(Fill)
                .style(surface_card_style),
            );
        } else {
            for (i, folder) in self.shared_folders.iter().enumerate() {
                let auto_on = folder.auto_sync.load(Ordering::Relaxed);
                let mut card_col = column![row![
                    icon(ICON_FOLDER, 18.0),
                    column![
                        text(&folder.id).size(15),
                        text(folder.path.display().to_string())
                            .size(12)
                            .color(TEXT_MUTED),
                        text(&folder.status).size(12).color(ACCENT_TEAL),
                    ]
                    .spacing(2)
                    .width(Fill),
                    pick_list(
                        dir_opts,
                        Some(DirChoice(folder.direction, l)),
                        move |c| Message::ChangeFolderDirection(i, c.0),
                    )
                    .width(175),
                    checkbox(auto_on)
                        .label(tr(l, "Авто", "Auto"))
                        .on_toggle(move |v| Message::ToggleFolderAuto(i, v)),
                    button(
                        row![icon(ICON_FOLDER, 14.0), text(tr(l, "Открыть", "Open")).size(12)]
                            .spacing(4)
                            .align_y(Alignment::Center)
                    )
                    .on_press(Message::OpenPath(folder.path.clone()))
                    .style(secondary_btn_style),
                    button(
                        row![icon(ICON_SEARCH, 14.0), text(tr(l, "Сравнить", "Compare")).size(12)]
                            .spacing(4)
                            .align_y(Alignment::Center)
                    )
                    .on_press(Message::CompareFolder(i))
                    .style(secondary_btn_style),
                    button(
                        row![icon(ICON_SYNC, 14.0), text(tr(l, "Синхр.", "Sync")).size(12)]
                            .spacing(4)
                            .align_y(Alignment::Center)
                    )
                    .on_press(Message::SyncFolder(i))
                    .style(teal_btn_style),
                    button(icon(ICON_TRASH, 14.0))
                        .on_press(Message::RemoveFolder(i))
                        .style(crimson_btn_style),
                ]
                .spacing(6)
                .align_y(Alignment::Center)]
                .spacing(6);

                if let Some(prev) = &folder.last_preview {
                    card_col = card_col.push(
                        text(format!(
                            "{}: {} {} | {} {} | {} {}",
                            tr(l, "Сравнение", "Diff"),
                            tr(l, "скачать", "download"),
                            prev.to_download.len(),
                            tr(l, "отдать", "upload"),
                            prev.to_upload.len(),
                            tr(l, "конфликты", "conflicts"),
                            prev.conflicts.len()
                        ))
                        .size(12)
                        .color(TEXT_MUTED),
                    );
                }

                list_col = list_col.push(
                    container(card_col)
                        .padding(12)
                        .width(Fill)
                        .style(surface_card_style),
                );
            }
        }

        column![
            add_bar,
            scrollable(list_col).height(Fill),
            self.view_logs_card(120.0),
        ]
        .spacing(10)
        .into()
    }

    fn view_clipboard(&self) -> Element<'_, Message> {
        let l = self.language;
        let controls = container(
            row![
                checkbox(self.auto_clipboard)
                    .label(tr(
                        l,
                        "Автосинхронизация буфера обмена (в фоне, анти-эхо)",
                        "Auto-sync clipboard (background, anti-echo)",
                    ))
                    .on_toggle(Message::ToggleAutoClipboard),
                Space::new().width(Fill),
                button(
                    row![
                        icon(ICON_ARROW_DOWN, 14.0),
                        text(tr(l, "Получить с устройства", "Pull from peer")).size(12)
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center)
                )
                .on_press(Message::PullClipboardNow)
                .style(secondary_btn_style),
                button(
                    row![
                        icon(ICON_CLIPBOARD, 14.0),
                        text(tr(l, "Отправить сейчас", "Send now")).size(12)
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center)
                )
                .on_press(Message::SendClipboardNow)
                .style(teal_btn_style),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .padding(12)
        .width(Fill)
        .style(surface_card_style);

        let entries = self.clipboard_guard.history();
        let mut hist_col =
            column![text(tr(l, "История буфера обмена:", "Clipboard history:")).size(15)].spacing(6);
        if entries.is_empty() {
            hist_col = hist_col.push(
                text(tr(l, "История пуста", "History is empty"))
                    .size(13)
                    .color(TEXT_MUTED),
            );
        } else {
            for entry in entries {
                let (dir_icon, dir_label) = match entry.direction {
                    ClipboardDirection::Sent => (ICON_ARROW_UP, tr(l, "Отправлено", "Sent")),
                    ClipboardDirection::Received => (ICON_ARROW_DOWN, tr(l, "Получено", "Received")),
                };
                hist_col = hist_col.push(
                    container(
                        column![
                            row![
                                icon(dir_icon, 14.0),
                                text(format!(
                                    "{dir_label} · {} · {}",
                                    entry.timestamp.format("%H:%M:%S"),
                                    entry.peer_name
                                ))
                                .size(12)
                                .color(TEXT_MUTED),
                            ]
                            .spacing(6)
                            .align_y(Alignment::Center),
                            text(entry.preview).size(13),
                        ]
                        .spacing(3),
                    )
                    .padding(10)
                    .width(Fill)
                    .style(surface_card_style),
                );
            }
        }

        column![controls, scrollable(hist_col).height(Fill)]
            .spacing(10)
            .into()
    }

    fn view_channels(&self) -> Element<'_, Message> {
        let l = self.language;
        let policy_opts = policy_choices(l);
        let sec_pin = self.security.pairing_pin();
        let my_info_card = container(
            row![
                column![
                    text(tr(l, "Подключение, QR и проверка связи", "Connection, QR & Diagnostics")).size(15),
                    row![
                        text(format!("{}: {}", tr(l, "Мой адрес", "My address"), self.local_addr_label))
                            .size(13)
                            .color(ACCENT_TEAL),
                        button(
                            row![icon(ICON_COPY, 14.0), text(tr(l, "Копировать", "Copy")).size(12)]
                                .spacing(5)
                                .align_y(Alignment::Center)
                        )
                        .on_press(Message::CopyMyAddress)
                        .style(secondary_btn_style),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                    text(format!(
                        "PIN: {sec_pin} · ADB: adb reverse tcp:{} tcp:{} -> 127.0.0.1:{}",
                        self.p2p_port, self.p2p_port, self.p2p_port
                    ))
                    .size(12)
                    .color(TEXT_MUTED),
                    row![
                        text(tr(l, "Проверить пир:", "Ping peer:")).size(13).color(TEXT_MUTED),
                        text_input("192.168.43.1:47789", &self.manual_addr)
                            .on_input(Message::ManualAddrChanged)
                            .width(200),
                        button(
                            row![icon(ICON_SEARCH, 14.0), text(tr(l, "Проверить связь", "Ping peer")).size(12)]
                                .spacing(5)
                                .align_y(Alignment::Center)
                        )
                        .on_press(Message::PingTarget)
                        .style(teal_btn_style),
                        text(&self.ping_status).size(12).color(TEXT_MUTED),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                ]
                .spacing(8)
                .width(Fill),
                column![
                    iced_image(self.qr_handle.clone()).width(116).height(116),
                    text(tr(l, "Сканируйте с телефона", "Scan from phone"))
                        .size(11)
                        .color(TEXT_MUTED),
                ]
                .spacing(4)
                .align_x(Alignment::Center),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .padding(12)
        .width(Fill)
        .style(surface_card_style);

        let policy_card = container(
            column![
                row![
                    text(tr(l, "Приоритет маршрутизации:", "Routing policy:")).size(13).color(TEXT_MUTED),
                    pick_list(
                        policy_opts,
                        Some(PolicyChoice(self.channel_policy, l)),
                        |c| Message::SelectChannelPolicy(c.0),
                    )
                    .width(340),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                row![
                    text(tr(l, "Relay / Внешний IP:", "Relay / External IP:")).size(13).color(TEXT_MUTED),
                    text_input("relay.larevole.org:47790", &self.relay_endpoint)
                        .on_input(Message::RelayEndpointChanged)
                        .width(280),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            ]
            .spacing(8),
        )
        .padding(12)
        .width(Fill)
        .style(surface_card_style);

        let mut cards = column![].spacing(8);
        for ch in &self.channels {
            let badge_color = if ch.available { ACCENT_TEAL } else { TEXT_MUTED };
            let badge = if ch.available {
                tr(l, "Активен", "Active")
            } else {
                tr(l, "Ожидание", "Standby")
            };
            cards = cards.push(
                container(
                    column![
                        row![
                            icon(ICON_CHANNELS, 15.0),
                            text(ch.medium.to_string()).size(14).width(Fill),
                            text(badge).size(12).color(badge_color),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                        text(&ch.endpoint_hint).size(12).color(TEXT_MUTED),
                        text(ch.bandwidth_note).size(12).color(TEXT_MUTED),
                    ]
                    .spacing(3),
                )
                .padding(10)
                .width(Fill)
                .style(surface_card_style),
            );
        }

        let mut bt_rows = column![
            row![
                text("Bluetooth RFCOMM SPP").size(15),
                Space::new().width(Fill),
                button(text(tr(l, "Обновить", "Refresh")).size(12))
                    .on_press(Message::RefreshBtDevices)
                    .style(secondary_btn_style),
                button(text(tr(l, "Буфер по BT", "Clipboard BT")).size(12))
                    .on_press(Message::SendClipboardBluetooth)
                    .style(teal_btn_style),
                button(text(tr(l, "Очередь по BT", "Files BT")).size(12))
                    .on_press(Message::SendQueueBluetooth)
                    .style(secondary_btn_style),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(6);
        if self.bt_devices.is_empty() {
            bt_rows = bt_rows.push(
                text(tr(
                    l,
                    "Сопряжённых Bluetooth-устройств не найдено (выполните сопряжение в Windows).",
                    "No paired Bluetooth devices found (pair in Windows settings first).",
                ))
                .size(12)
                .color(TEXT_MUTED),
            );
        } else {
            for dev in &self.bt_devices {
                let selected = dev.mac == self.selected_bt_mac;
                bt_rows = bt_rows.push(
                    button(text(format!("{} — {}", dev.name, dev.mac)).size(12))
                        .on_press(Message::SelectBtDevice(dev.mac.clone()))
                        .style(if selected {
                            teal_btn_style
                        } else {
                            secondary_btn_style
                        }),
                );
            }
        }
        let bt_card = container(bt_rows).padding(12).width(Fill).style(surface_card_style);

        column![my_info_card, policy_card, bt_card, scrollable(cards).height(Fill)]
            .spacing(10)
            .into()
    }

    fn view_settings(&self) -> Element<'_, Message> {
        let l = self.language;
        let policy_opts = policy_choices(l);

        let lang_card = container(
            column![
                text(tr(l, "Язык интерфейса", "Interface Language")).size(15),
                text(tr(
                    l,
                    "Переключение применяется мгновенно и сохраняется между запусками.",
                    "Switches immediately across tabs and tray menu, saved automatically.",
                ))
                .size(12)
                .color(TEXT_MUTED),
                row![
                    button(text("Русский").size(13))
                        .on_press(Message::SelectLanguage(AppLanguage::Ru))
                        .style(if l == AppLanguage::Ru {
                            teal_btn_style
                        } else {
                            secondary_btn_style
                        }),
                    button(text("English").size(13))
                        .on_press(Message::SelectLanguage(AppLanguage::En))
                        .style(if l == AppLanguage::En {
                            teal_btn_style
                        } else {
                            secondary_btn_style
                        }),
                ]
                .spacing(10),
            ]
            .spacing(8),
        )
        .padding(14)
        .width(Fill)
        .style(surface_card_style);

        let behavior_card = container(
            column![
                text(tr(l, "Поведение и фоновый режим", "Behavior & Background")).size(15),
                checkbox(self.auto_clipboard)
                    .label(tr(
                        l,
                        "Автосинхронизация буфера обмена (в фоне, анти-эхо)",
                        "Auto-sync clipboard (background, anti-echo)",
                    ))
                    .on_toggle(Message::ToggleAutoClipboard),
                row![
                    text(format!(
                        "{}: {}",
                        tr(l, "Папка приёма файлов", "Receive directory"),
                        self.receive_dir.display()
                    ))
                    .size(12)
                    .color(TEXT_MUTED)
                    .width(Fill),
                    button(
                        row![icon(ICON_FOLDER, 14.0), text(tr(l, "Открыть папку", "Open folder")).size(12)]
                            .spacing(5)
                            .align_y(Alignment::Center)
                    )
                    .on_press(Message::OpenDownloadsFolder)
                    .style(secondary_btn_style),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                row![
                    text(tr(
                        l,
                        "При закрытии окна LierSync продолжает работать в трее.",
                        "Closing the window keeps LierSync running in the system tray.",
                    ))
                    .size(12)
                    .color(TEXT_MUTED)
                    .width(Fill),
                    button(text(tr(l, "Свернуть в трей", "Minimize to tray")).size(12))
                        .on_press(Message::HideToTray)
                        .style(secondary_btn_style),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                row![
                    text(tr(
                        l,
                        "Интеграция с проводником: ПКМ -> Отправить -> LierSync.",
                        "Windows Explorer integration: Right-click -> Send to -> LierSync.",
                    ))
                    .size(12)
                    .color(TEXT_MUTED)
                    .width(Fill),
                    button(text(tr(l, "Добавить в «Отправить»", "Add to SendTo")).size(12))
                        .on_press(Message::InstallSendToShortcut)
                        .style(secondary_btn_style),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            ]
            .spacing(10),
        )
        .padding(14)
        .width(Fill)
        .style(surface_card_style);

        let network_card = container(
            column![
                text(tr(l, "Сеть и каналы по умолчанию", "Default Network & Routing")).size(15),
                row![
                    text(tr(l, "Адрес пира по умолчанию:", "Default peer address:")).size(13).color(TEXT_MUTED),
                    text_input("192.168.43.1:47789", &self.manual_addr)
                        .on_input(Message::ManualAddrChanged)
                        .width(220),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                row![
                    text(tr(l, "Приоритет маршрутизации:", "Routing policy:")).size(13).color(TEXT_MUTED),
                    pick_list(
                        policy_opts,
                        Some(PolicyChoice(self.channel_policy, l)),
                        |c| Message::SelectChannelPolicy(c.0),
                    )
                    .width(340),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                row![
                    text(tr(l, "Relay / Внешний IP:", "Relay / External IP:")).size(13).color(TEXT_MUTED),
                    text_input("relay.larevole.org:47790", &self.relay_endpoint)
                        .on_input(Message::RelayEndpointChanged)
                        .width(280),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            ]
            .spacing(10),
        )
        .padding(14)
        .width(Fill)
        .style(surface_card_style);

        let sec = self.security.snapshot();
        let mut trusted_col = column![].spacing(6);
        if sec.trusted_peers.is_empty() {
            trusted_col = trusted_col.push(
                text(tr(
                    l,
                    "Доверенных устройств пока нет. Выполните сопряжение по PIN или QR.",
                    "No trusted devices yet. Pair using PIN or QR.",
                ))
                .size(12)
                .color(TEXT_MUTED),
            );
        } else {
            for tp in &sec.trusted_peers {
                let key = tp.device_id.clone();
                trusted_col = trusted_col.push(
                    row![
                        text(format!("{} · {} ({})", tp.device_name, tp.fingerprint, tp.added_at))
                            .size(12)
                            .width(Fill),
                        button(icon(ICON_TRASH, 13.0))
                            .on_press(Message::RemoveTrustedPeer(key))
                            .style(crimson_btn_style),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }
        }

        let security_card = container(
            column![
                text(tr(l, "Безопасность и сопряжение", "Security & Pairing")).size(15),
                checkbox(sec.encrypt_transport)
                    .label(tr(
                        l,
                        "Шифровать соединения (X25519 + ChaCha20-Poly1305)",
                        "Encrypt connections (X25519 + ChaCha20-Poly1305)",
                    ))
                    .on_toggle(Message::ToggleEncryptTransport),
                checkbox(sec.whitelist_only)
                    .label(tr(
                        l,
                        "Только доверенные устройства (белый список)",
                        "Trusted devices only (whitelist)",
                    ))
                    .on_toggle(Message::ToggleWhitelistOnly),
                row![
                    text(format!(
                        "{}: {}",
                        tr(l, "PIN этого ПК", "This PC pairing PIN"),
                        sec.pairing_pin
                    ))
                    .size(13)
                    .color(ACCENT_TEAL),
                    button(text(tr(l, "Новый PIN", "Rotate PIN")).size(12))
                        .on_press(Message::RotatePairingPin)
                        .style(secondary_btn_style),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                row![
                    text(tr(l, "Сопрячь с пиром (IP выше):", "Pair target peer (PIN):"))
                        .size(12)
                        .color(TEXT_MUTED),
                    text_input("PIN", &self.pair_pin_input)
                        .on_input(Message::PairPinInputChanged)
                        .width(100),
                    button(text(tr(l, "Сопрячь", "Pair")).size(12))
                        .on_press(Message::PairTargetPeer)
                        .style(teal_btn_style),
                    text(&self.pair_status).size(12).color(ACCENT_TEAL),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                rule::horizontal(1),
                text(tr(l, "Доверенные устройства:", "Trusted devices:")).size(13),
                trusted_col,
            ]
            .spacing(8),
        )
        .padding(14)
        .width(Fill)
        .style(surface_card_style);

        scrollable(
            column![lang_card, security_card, behavior_card, network_card].spacing(10),
        )
        .height(Fill)
        .into()
    }

    fn view_about(&self) -> Element<'_, Message> {
        let l = self.language;
        let logo_large = iced_image(self.logo_handle.clone())
            .width(56)
            .height(56);
        container(
            column![
                row![
                    logo_large,
                    column![
                        row![
                            text("LierSync").size(22),
                            text("v1.1").size(13).color(ACCENT_TEAL),
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center),
                        text(tr(
                            l,
                            "Фоновая работа в системном трее при закрытии окна",
                            "Background system tray operation when window is closed",
                        ))
                        .size(12)
                        .color(TEXT_MUTED),
                    ]
                    .spacing(3),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
                rule::horizontal(1),
                text(tr(
                    l,
                    "Лёгкий нативный P2P-клиент для передачи файлов, синхронизации папок и общего буфера обмена.",
                    "Lightweight native P2P client for fast file transfers, folder sync and shared clipboard.",
                ))
                .size(13),
                text(tr(
                    l,
                    "Разработчик: LaRevole (LaRCorp)",
                    "Developer: LaRevole (LaRCorp)",
                ))
                .size(13)
                .color(TEXT_MUTED),
                text(tr(
                    l,
                    "Лицензия: GNU General Public License v3.0 (GPLv3)",
                    "License: GNU General Public License v3.0 (GPLv3)",
                ))
                .size(13)
                .color(TEXT_MUTED),
                rule::horizontal(1),
                text(format!("{}: {}", tr(l, "Имя узла", "Node name"), self.identity.device_name)).size(12),
                text(format!("{}: {}", tr(l, "Отпечаток", "Fingerprint"), self.identity.fingerprint)).size(12),
                text(format!("{}: {}", tr(l, "Папка загрузок", "Downloads folder"), self.receive_dir.display())).size(12),
            ]
            .spacing(10),
        )
        .padding(18)
        .width(Fill)
        .style(surface_card_style)
        .into()
    }

    fn view_logs_card(&self, height: f32) -> Element<'_, Message> {
        let mut log_col = column![text(tr(self.language, "Журнал событий:", "Event log:")).size(13)].spacing(3);
        for line in &self.logs {
            log_col = log_col.push(text(line).size(12).color(TEXT_MUTED));
        }
        container(
            column![rule::horizontal(1), scrollable(log_col).height(height)].spacing(6),
        )
        .padding(10)
        .width(Fill)
        .style(surface_card_style)
        .into()
    }
}

fn surface_card_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_SURFACE)),
        border: Border {
            width: 1.0,
            color: BORDER_COLOR,
            radius: 8.0.into(),
        },
        text_color: Some(TEXT_MAIN),
        ..container::Style::default()
    }
}

fn teal_btn_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0x48 as f32 / 255.0, 0xD8 as f32 / 255.0, 0xCE as f32 / 255.0),
        button::Status::Disabled => BG_ELEVATED,
        _ => ACCENT_TEAL,
    };
    let fg = match status {
        button::Status::Disabled => TEXT_MUTED,
        _ => BG_BASE,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: fg,
        border: Border {
            radius: 6.0.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}

fn crimson_btn_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => ACCENT_RED_HOVER,
        _ => ACCENT_RED,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: TEXT_MAIN,
        border: Border {
            radius: 6.0.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}

fn secondary_btn_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0x2E as f32 / 255.0, 0x30 as f32 / 255.0, 0x3A as f32 / 255.0),
        _ => BG_ELEVATED,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: TEXT_MAIN,
        border: Border {
            width: 1.0,
            color: BORDER_COLOR,
            radius: 6.0.into(),
        },
        ..button::Style::default()
    }
}

#[cfg(target_os = "windows")]
fn install_windows_sendto() -> std::result::Result<std::path::PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let appdata = std::env::var_os("APPDATA").ok_or_else(|| "APPDATA is not set".to_string())?;
    let sendto_dir = std::path::PathBuf::from(appdata).join("Microsoft\\Windows\\SendTo");
    std::fs::create_dir_all(&sendto_dir).map_err(|e| e.to_string())?;
    let lnk_path = sendto_dir.join("LierSync.lnk");

    let esc = |s: &str| s.replace('\'', "''");
    let script = format!(
        "$ws = New-Object -ComObject WScript.Shell; \
         $s = $ws.CreateShortcut('{}'); \
         $s.TargetPath = '{}'; \
         $s.Arguments = '--send'; \
         $s.WorkingDirectory = '{}'; \
         $s.Description = 'Send via LierSync'; \
         $s.Save()",
        esc(&lnk_path.to_string_lossy()),
        esc(&exe.to_string_lossy()),
        esc(&exe.parent().unwrap_or(std::path::Path::new(".")).to_string_lossy()),
    );

    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(stderr.trim().to_string());
    }
    Ok(lnk_path)
}

#[cfg(not(target_os = "windows"))]
fn install_windows_sendto() -> std::result::Result<std::path::PathBuf, String> {
    Err("SendTo shortcut is only supported on Windows".to_string())
}

fn icon<'a>(bytes: &'static [u8], size: f32) -> svg::Svg<'a> {
    svg(svg::Handle::from_memory(bytes))
        .width(size)
        .height(size)
}

fn tab_button<'a>(
    label: &'a str,
    icon_bytes: &'static [u8],
    tab: ActiveTab,
    current: ActiveTab,
) -> Element<'a, Message> {
    button(
        row![icon(icon_bytes, 15.0), text(label).size(13)]
            .spacing(6)
            .align_y(Alignment::Center),
    )
    .on_press(Message::SelectTab(tab))
    .style(if tab == current {
        teal_btn_style
    } else {
        secondary_btn_style
    })
    .into()
}

fn open_in_os(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", &path.to_string_lossy()])
            .spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}

fn reveal_in_os(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let target = path.parent().unwrap_or(path);
        let _ = std::process::Command::new("xdg-open").arg(target).spawn();
    }
}

fn main_window_settings() -> window::Settings {
    window::Settings {
        size: Size::new(980.0, 660.0),
        min_size: Some(Size::new(720.0, 480.0)),
        icon: load_window_icon(),
        exit_on_close_request: false,
        ..window::Settings::default()
    }
}

fn load_window_icon() -> Option<window::Icon> {
    let img = image::load_from_memory(LOGO_PNG).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    window::icon::from_rgba(rgba.into_raw(), w, h).ok()
}

fn create_tray_icon(lang: AppLanguage) -> Option<TrayState> {
    let img = image::load_from_memory(LOGO_PNG).ok()?;
    let small = img.resize_exact(32, 32, image::imageops::FilterType::Lanczos3);
    let rgba = small.to_rgba8();
    let (w, h) = rgba.dimensions();
    let icon = tray_icon::Icon::from_rgba(rgba.into_raw(), w, h).ok()?;

    let item_open = MenuItem::new(tr(lang, "Открыть LierSync", "Open LierSync"), true, None);
    let item_push_clip = MenuItem::new(tr(lang, "Отправить буфер", "Send clipboard"), true, None);
    let item_pull_clip = MenuItem::new(tr(lang, "Получить буфер", "Pull clipboard"), true, None);
    let item_open_dl = MenuItem::new(
        tr(lang, "Открыть Загрузки\\LierSync", "Open Downloads\\LierSync"),
        true,
        None,
    );
    let item_exit = MenuItem::new(tr(lang, "Выход", "Exit"), true, None);

    let id_open = item_open.id().clone();
    let id_push_clip = item_push_clip.id().clone();
    let id_pull_clip = item_pull_clip.id().clone();
    let id_open_downloads = item_open_dl.id().clone();
    let id_exit = item_exit.id().clone();

    let menu = Menu::new();
    let _ = menu.append(&item_open);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&item_push_clip);
    let _ = menu.append(&item_pull_clip);
    let _ = menu.append(&item_open_dl);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&item_exit);

    let tray = TrayIconBuilder::new()
        .with_tooltip(tr(
            lang,
            "LierSync — работает в фоне",
            "LierSync — running in background",
        ))
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .build()
        .ok()?;

    Some(TrayState {
        tray,
        id_open,
        id_push_clip,
        id_pull_clip,
        id_open_downloads,
        id_exit,
        item_open,
        item_push_clip,
        item_pull_clip,
        item_open_dl,
        item_exit,
    })
}

fn main() -> iced::Result {
    iced::daemon(LierSyncApp::new, LierSyncApp::update, LierSyncApp::view)
        .title(LierSyncApp::title)
        .theme(LierSyncApp::theme)
        .subscription(LierSyncApp::subscription)
        .run()
}
