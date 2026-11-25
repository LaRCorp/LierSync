// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

use jni::JNIEnv;
use jni::objects::{JByteArray, JClass, JString};
use jni::sys::{jint, jstring};
use lier_sync_core::{
    ClipboardGuard, CoreEvent, DEFAULT_P2P_PORT, DeviceIdentity, DiscoveryService,
    RemoteClipboardPayload, SecurityManager, SecurityPolicy, SharedFolderRegistry, SyncDirection,
    TrustedPeer, build_pairing_uri, compare_folder_with_peer, detect_local_ipv4s,
    fetch_clipboard_payload_from_peer, generate_qr_png, pair_with_peer, parse_pairing_uri,
    ping_peer, send_clipboard_image_png, send_clipboard_text, send_quick_batch,
    start_p2p_server_secure, sync_folder_by_direction,
};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

struct AndroidNodeState {
    rt: Runtime,
    identity: DeviceIdentity,
    p2p_port: u16,
    receive_dir: PathBuf,
    discovery: DiscoveryService,
    folders: SharedFolderRegistry,
    clipboard_guard: ClipboardGuard,
    security: SecurityManager,
    events: Arc<Mutex<Vec<MobileEventDto>>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
enum MobileEventDto {
    IncomingFile {
        from_peer: String,
        file_name: String,
        rel_path: Option<String>,
        saved_path: String,
        bytes: u64,
    },
    ClipboardReceived {
        from_peer: String,
        text_preview: String,
        full_text: String,
    },
    ClipboardImageReceived {
        from_peer: String,
        width: u32,
        height: u32,
        png_path: String,
    },
    SyncActivity {
        folder_id: String,
        message: String,
    },
    PeerPaired {
        device_id: String,
        device_name: String,
        fingerprint: String,
        added_at: String,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize)]
struct NodeInfoDto {
    device_id: String,
    device_name: String,
    fingerprint: String,
    os: String,
    p2p_port: u16,
    receive_dir: String,
    local_ips: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PeerDto {
    device_id: String,
    device_name: String,
    os: String,
    addr: String,
}

#[derive(Debug, Clone, Serialize)]
struct ChannelDto {
    medium: String,
    available: bool,
    endpoint_hint: String,
    bandwidth_note: String,
}

static NODE: OnceLock<Arc<AndroidNodeState>> = OnceLock::new();
static INIT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn jstr(env: &mut JNIEnv<'_>, s: &JString<'_>) -> String {
    env.get_string(s)
        .map(|v| v.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn ret_jstr(env: &mut JNIEnv<'_>, val: &str) -> jstring {
    env.new_string(val)
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_larevole_liersync_nativebridge_LierSyncNative_initNode(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    app_files_dir: JString<'_>,
    receive_dir: JString<'_>,
    device_name: JString<'_>,
) -> jstring {
    let files_dir = PathBuf::from(jstr(&mut env, &app_files_dir));
    let rx_dir = PathBuf::from(jstr(&mut env, &receive_dir));
    let dev_name = jstr(&mut env, &device_name);

    let _init_guard = INIT_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    if let Some(existing) = NODE.get() {
        let info = NodeInfoDto {
            device_id: existing.identity.device_id.clone(),
            device_name: existing.identity.device_name.clone(),
            fingerprint: existing.identity.fingerprint.clone(),
            os: existing.identity.os.clone(),
            p2p_port: existing.p2p_port,
            receive_dir: existing.receive_dir.display().to_string(),
            local_ips: detect_local_ipv4s()
                .into_iter()
                .map(|ip| ip.to_string())
                .collect(),
        };
        return ret_jstr(&mut env, &serde_json::to_string(&info).unwrap_or_default());
    }

    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => return ret_jstr(&mut env, &format!("{{\"error\":\"{e}\"}}")),
    };

    let identity_path = files_dir.join("identity.json");
    let identity = match DeviceIdentity::load_or_create_in(&identity_path, Some(dev_name)) {
        Ok(id) => id,
        Err(e) => return ret_jstr(&mut env, &format!("{{\"error\":\"{e}\"}}")),
    };

    let folders = SharedFolderRegistry::new();
    let clipboard_guard = ClipboardGuard::new(60);
    let discovery = DiscoveryService::new();
    let security = SecurityManager::new(SecurityPolicy::default());
    let (core_tx, mut core_rx) = mpsc::channel::<CoreEvent>(128);

    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), DEFAULT_P2P_PORT);
    let bound_port = rt.block_on(async {
        match start_p2p_server_secure(
            bind_addr,
            identity.clone(),
            rx_dir.clone(),
            folders.clone(),
            clipboard_guard.clone(),
            discovery.clone(),
            security.clone(),
            core_tx.clone(),
        )
        .await
        {
            Ok(a) => a.port(),
            Err(_) => {
                let fallback = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
                start_p2p_server_secure(
                    fallback,
                    identity.clone(),
                    rx_dir.clone(),
                    folders.clone(),
                    clipboard_guard.clone(),
                    discovery.clone(),
                    security.clone(),
                    core_tx,
                )
                .await
                .map(|a| a.port())
                .unwrap_or(DEFAULT_P2P_PORT)
            }
        }
    });

    {
        let _enter = rt.enter();
        let _ = discovery.start(identity.clone(), bound_port);
    }

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_sink = Arc::clone(&events);
    let rx_dir_ev = rx_dir.clone();
    rt.spawn(async move {
        while let Some(ev) = core_rx.recv().await {
            let dto = match ev {
                CoreEvent::IncomingFile {
                    from_peer,
                    file_name,
                    rel_path,
                    saved_path,
                    bytes,
                } => MobileEventDto::IncomingFile {
                    from_peer,
                    file_name,
                    rel_path,
                    saved_path: saved_path.display().to_string(),
                    bytes,
                },
                CoreEvent::ClipboardReceived {
                    from_peer,
                    preview,
                    full_text,
                } => MobileEventDto::ClipboardReceived {
                    from_peer,
                    text_preview: preview,
                    full_text,
                },
                CoreEvent::ClipboardImageReceived {
                    from_peer,
                    width,
                    height,
                    png_bytes,
                    ..
                } => {
                    let fname = format!(
                        "clip_{}.png",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_nanos())
                            .unwrap_or(0)
                    );
                    let path = rx_dir_ev.join(fname);
                    let _ = std::fs::write(&path, &png_bytes);
                    MobileEventDto::ClipboardImageReceived {
                        from_peer,
                        width,
                        height,
                        png_path: path.display().to_string(),
                    }
                }
                CoreEvent::SyncActivity { folder_id, message } => {
                    MobileEventDto::SyncActivity { folder_id, message }
                }
                CoreEvent::PeerPaired(peer) => MobileEventDto::PeerPaired {
                    device_id: peer.device_id,
                    device_name: peer.device_name,
                    fingerprint: peer.fingerprint,
                    added_at: peer.added_at,
                },
                CoreEvent::Error(message) => MobileEventDto::Error { message },
            };
            if let Ok(mut list) = events_sink.lock() {
                list.push(dto);
            }
        }
    });

    let info = NodeInfoDto {
        device_id: identity.device_id.clone(),
        device_name: identity.device_name.clone(),
        fingerprint: identity.fingerprint.clone(),
        os: identity.os.clone(),
        p2p_port: bound_port,
        receive_dir: rx_dir.display().to_string(),
        local_ips: detect_local_ipv4s()
            .into_iter()
            .map(|ip| ip.to_string())
            .collect(),
    };

    let _ = NODE.set(Arc::new(AndroidNodeState {
        rt,
        identity,
        p2p_port: bound_port,
        receive_dir: rx_dir,
        discovery,
        folders,
        clipboard_guard,
        security,
        events,
    }));

    ret_jstr(&mut env, &serde_json::to_string(&info).unwrap_or_default())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_larevole_liersync_nativebridge_LierSyncNative_setExtraLanIpsJson(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    ips_json: JString<'_>,
) {
    let raw = jstr(&mut env, &ips_json);
    let parsed: Vec<std::net::Ipv4Addr> = serde_json::from_str::<Vec<String>>(&raw)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|s| s.parse().ok())
        .collect();
    if let Some(state) = NODE.get() {
        state.rt.block_on(state.discovery.set_extra_ips(parsed));
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_larevole_liersync_nativebridge_LierSyncNative_getPeersJson(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jstring {
    let Some(state) = NODE.get() else {
        return ret_jstr(&mut env, "[]");
    };
    let peers = state.rt.block_on(state.discovery.peers());
    let dtos: Vec<PeerDto> = peers
        .into_iter()
        .map(|p| PeerDto {
            device_id: p.device_id,
            device_name: p.device_name,
            os: p.os,
            addr: p.addr.to_string(),
        })
        .collect();
    ret_jstr(
        &mut env,
        &serde_json::to_string(&dtos).unwrap_or_else(|_| "[]".into()),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_larevole_liersync_nativebridge_LierSyncNative_pollEventsJson(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
) -> jstring {
    let Some(state) = NODE.get() else {
        return ret_jstr(&mut env, "[]");
    };
    let drained: Vec<MobileEventDto> = state
        .events
        .lock()
        .map(|mut list| std::mem::take(&mut *list))
        .unwrap_or_default();
    ret_jstr(
        &mut env,
        &serde_json::to_string(&drained).unwrap_or_else(|_| "[]".into()),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_larevole_liersync_nativebridge_LierSyncNative_sendBatchJson(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    target_addr: JString<'_>,
    paths_json: JString<'_>,
) -> jstring {
    let addr_str = jstr(&mut env, &target_addr);
    let paths_raw = jstr(&mut env, &paths_json);

    let Ok(addr) = addr_str.trim().parse::<SocketAddr>() else {
        return ret_jstr(
            &mut env,
            "{\"ok\":false,\"error\":\"Invalid target address\"}",
        );
    };
    let paths: Vec<PathBuf> = serde_json::from_str::<Vec<String>>(&paths_raw)
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect();

    let Some(state) = NODE.get() else {
        return ret_jstr(
            &mut env,
            "{\"ok\":false,\"error\":\"Node not initialized\"}",
        );
    };
    let id = state.identity.clone();
    let res = state
        .rt
        .block_on(send_quick_batch(addr, &id, &paths, |_, _, _, _, _| {}));

    match res {
        Ok(count) => ret_jstr(&mut env, &format!("{{\"ok\":true,\"sent_files\":{count}}}")),
        Err(e) => ret_jstr(
            &mut env,
            &serde_json::json!({ "ok": false, "error": e.to_string() }).to_string(),
        ),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_larevole_liersync_nativebridge_LierSyncNative_registerSharedFolder(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    folder_id: JString<'_>,
    dir_path: JString<'_>,
) {
    let fid = jstr(&mut env, &folder_id);
    let dir = PathBuf::from(jstr(&mut env, &dir_path));
    if let Some(state) = NODE.get() {
        state.rt.block_on(state.folders.register(fid, dir));
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_larevole_liersync_nativebridge_LierSyncNative_compareFolderJson(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    target_addr: JString<'_>,
    folder_id: JString<'_>,
    dir_path: JString<'_>,
) -> jstring {
    let addr_str = jstr(&mut env, &target_addr);
    let fid = jstr(&mut env, &folder_id);
    let dir = PathBuf::from(jstr(&mut env, &dir_path));

    let Ok(addr) = addr_str.trim().parse::<SocketAddr>() else {
        return ret_jstr(&mut env, "{\"error\":\"Invalid address\"}");
    };
    let Some(state) = NODE.get() else {
        return ret_jstr(&mut env, "{\"error\":\"Node not initialized\"}");
    };
    let id = state.identity.clone();
    match state
        .rt
        .block_on(compare_folder_with_peer(addr, &id, &fid, &dir))
    {
        Ok(preview) => ret_jstr(
            &mut env,
            &serde_json::to_string(&preview).unwrap_or_default(),
        ),
        Err(e) => ret_jstr(
            &mut env,
            &serde_json::json!({ "error": e.to_string() }).to_string(),
        ),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_larevole_liersync_nativebridge_LierSyncNative_syncFolderJson(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    target_addr: JString<'_>,
    folder_id: JString<'_>,
    dir_path: JString<'_>,
    direction_code: JString<'_>,
) -> jstring {
    let addr_str = jstr(&mut env, &target_addr);
    let fid = jstr(&mut env, &folder_id);
    let dir = PathBuf::from(jstr(&mut env, &dir_path));
    let dir_mode = SyncDirection::from_code(&jstr(&mut env, &direction_code));

    let Ok(addr) = addr_str.trim().parse::<SocketAddr>() else {
        return ret_jstr(&mut env, "{\"ok\":false,\"error\":\"Invalid address\"}");
    };
    let Some(state) = NODE.get() else {
        return ret_jstr(
            &mut env,
            "{\"ok\":false,\"error\":\"Node not initialized\"}",
        );
    };
    let id = state.identity.clone();
    match state
        .rt
        .block_on(sync_folder_by_direction(addr, &id, &fid, &dir, dir_mode))
    {
        Ok(chunks) => ret_jstr(&mut env, &format!("{{\"ok\":true,\"chunks\":{chunks}}}")),
        Err(e) => ret_jstr(
            &mut env,
            &serde_json::json!({ "ok": false, "error": e.to_string() }).to_string(),
        ),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_larevole_liersync_nativebridge_LierSyncNative_pushClipboardJson(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    target_addr: JString<'_>,
    text: JString<'_>,
    force: jni::sys::jboolean,
) -> jstring {
    let addr_str = jstr(&mut env, &target_addr);
    let clip_text = jstr(&mut env, &text);
    let Ok(addr) = addr_str.trim().parse::<SocketAddr>() else {
        return ret_jstr(&mut env, "{\"ok\":false,\"error\":\"Invalid address\"}");
    };
    let Some(state) = NODE.get() else {
        return ret_jstr(
            &mut env,
            "{\"ok\":false,\"error\":\"Node not initialized\"}",
        );
    };
    let hash = match state.clipboard_guard.should_emit_local(&clip_text) {
        Some(h) => h,
        None if force != 0 => lier_sync_core::hash_bytes(clip_text.as_bytes()),
        None => return ret_jstr(&mut env, "{\"ok\":true,\"skipped\":\"anti_echo\"}"),
    };
    state
        .clipboard_guard
        .record_sent(&addr.to_string(), &clip_text, &hash);
    let id = state.identity.clone();
    match state
        .rt
        .block_on(send_clipboard_text(addr, &id, &clip_text))
    {
        Ok(()) => ret_jstr(&mut env, "{\"ok\":true}"),
        Err(e) => ret_jstr(
            &mut env,
            &serde_json::json!({ "ok": false, "error": e.to_string() }).to_string(),
        ),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_larevole_liersync_nativebridge_LierSyncNative_pushClipboardImageBytes(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    target_addr: JString<'_>,
    width: jint,
    height: jint,
    png_bytes: JByteArray<'_>,
    force: jni::sys::jboolean,
) -> jstring {
    let addr_str = jstr(&mut env, &target_addr);
    let Ok(addr) = addr_str.trim().parse::<SocketAddr>() else {
        return ret_jstr(&mut env, "{\"ok\":false,\"error\":\"Invalid address\"}");
    };
    let Ok(bytes) = env.convert_byte_array(&png_bytes) else {
        return ret_jstr(&mut env, "{\"ok\":false,\"error\":\"Invalid image bytes\"}");
    };
    let Some(state) = NODE.get() else {
        return ret_jstr(
            &mut env,
            "{\"ok\":false,\"error\":\"Node not initialized\"}",
        );
    };
    let w = width.max(0) as u32;
    let h = height.max(0) as u32;
    let hash = match state.clipboard_guard.should_emit_local_image(&bytes) {
        Some(h) => h,
        None if force != 0 => lier_sync_core::hash_bytes(&bytes),
        None => return ret_jstr(&mut env, "{\"ok\":true,\"skipped\":\"anti_echo\"}"),
    };
    state
        .clipboard_guard
        .record_sent_image(&addr.to_string(), w, h, &bytes, &hash);
    let id = state.identity.clone();
    match state
        .rt
        .block_on(send_clipboard_image_png(addr, &id, w, h, bytes))
    {
        Ok(()) => ret_jstr(&mut env, "{\"ok\":true}"),
        Err(e) => ret_jstr(
            &mut env,
            &serde_json::json!({ "ok": false, "error": e.to_string() }).to_string(),
        ),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_larevole_liersync_nativebridge_LierSyncNative_pullClipboardJson(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    target_addr: JString<'_>,
) -> jstring {
    let addr_str = jstr(&mut env, &target_addr);
    let Ok(addr) = addr_str.trim().parse::<SocketAddr>() else {
        return ret_jstr(
            &mut env,
            "{\"ok\":false,\"error\":\"Invalid peer address\"}",
        );
    };
    let Some(state) = NODE.get() else {
        return ret_jstr(&mut env, "{\"ok\":false,\"error\":\"Node not running\"}");
    };
    let id = state.identity.clone();
    match state
        .rt
        .block_on(fetch_clipboard_payload_from_peer(addr, &id))
    {
        Ok(RemoteClipboardPayload::Text {
            sender_name,
            sha256,
            text,
        }) => {
            if text.is_empty() {
                return ret_jstr(
                    &mut env,
                    "{\"ok\":false,\"error\":\"Remote clipboard is empty\"}",
                );
            }
            let _ = state
                .clipboard_guard
                .accept_remote(&sender_name, &text, &sha256);
            ret_jstr(
                &mut env,
                &serde_json::json!({
                    "ok": true,
                    "kind": "text",
                    "sender_name": sender_name,
                    "text": text
                })
                .to_string(),
            )
        }
        Ok(RemoteClipboardPayload::Image {
            sender_name,
            sha256,
            width,
            height,
            png_bytes,
        }) => {
            let _ = state.clipboard_guard.accept_remote_image(
                &sender_name,
                width,
                height,
                &png_bytes,
                &sha256,
            );
            let fname = format!(
                "clip_pull_{}.png",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            );
            let path = state.receive_dir.join(fname);
            if let Err(e) = std::fs::write(&path, &png_bytes) {
                return ret_jstr(
                    &mut env,
                    &serde_json::json!({ "ok": false, "error": e.to_string() }).to_string(),
                );
            }
            ret_jstr(
                &mut env,
                &serde_json::json!({
                    "ok": true,
                    "kind": "image",
                    "sender_name": sender_name,
                    "width": width,
                    "height": height,
                    "png_path": path.display().to_string()
                })
                .to_string(),
            )
        }
        Err(e) => ret_jstr(
            &mut env,
            &serde_json::json!({ "ok": false, "error": e.to_string() }).to_string(),
        ),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_larevole_liersync_nativebridge_LierSyncNative_pingPeerJson(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    target_addr: JString<'_>,
) -> jstring {
    let addr_str = jstr(&mut env, &target_addr);
    let Ok(addr) = addr_str.trim().parse::<SocketAddr>() else {
        return ret_jstr(
            &mut env,
            "{\"ok\":false,\"error\":\"Некорректный адрес (нужен ip:port)\"}",
        );
    };
    let Some(state) = NODE.get() else {
        return ret_jstr(&mut env, "{\"ok\":false,\"error\":\"Узел не запущен\"}");
    };
    let id = state.identity.clone();
    match state.rt.block_on(ping_peer(addr, &id)) {
        Ok((name, os, ms)) => ret_jstr(
            &mut env,
            &serde_json::json!({
                "ok": true,
                "device_name": name,
                "os": os,
                "latency_ms": ms
            })
            .to_string(),
        ),
        Err(e) => ret_jstr(
            &mut env,
            &serde_json::json!({ "ok": false, "error": e.to_string() }).to_string(),
        ),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_larevole_liersync_nativebridge_LierSyncNative_getChannelsJson(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    relay_endpoint: JString<'_>,
    lang: JString<'_>,
) -> jstring {
    let relay = jstr(&mut env, &relay_endpoint);
    let lang_code = jstr(&mut env, &lang);
    let app_lang = lier_sync_core::AppLanguage::from_code(&lang_code);
    let port = NODE.get().map(|s| s.p2p_port).unwrap_or(DEFAULT_P2P_PORT);
    let list: Vec<ChannelDto> =
        lier_sync_core::probe_channels_for_lang(port, Some(&relay), app_lang)
            .into_iter()
            .map(|c| ChannelDto {
                medium: c.medium.to_string(),
                available: c.available,
                endpoint_hint: c.endpoint_hint,
                bandwidth_note: c.bandwidth_note.to_string(),
            })
            .collect();
    ret_jstr(
        &mut env,
        &serde_json::to_string(&list).unwrap_or_else(|_| "[]".into()),
    )
}

#[derive(Deserialize)]
struct SecurityConfigInput {
    encrypt_transport: Option<bool>,
    whitelist_only: Option<bool>,
    pairing_pin: Option<String>,
    trusted_peers: Option<Vec<TrustedPeer>>,
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_larevole_liersync_nativebridge_LierSyncNative_setSecurityConfigJson(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    config_json: JString<'_>,
) -> jstring {
    let raw = jstr(&mut env, &config_json);
    let Some(state) = NODE.get() else {
        return ret_jstr(&mut env, "{\"ok\":false}");
    };
    if let Ok(cfg) = serde_json::from_str::<SecurityConfigInput>(&raw) {
        if let Some(enc) = cfg.encrypt_transport {
            state.security.set_encrypt_transport(enc);
        }
        if let Some(wl) = cfg.whitelist_only {
            state.security.set_whitelist_only(wl);
        }
        if let Some(pin) = cfg.pairing_pin.filter(|s| !s.trim().is_empty()) {
            state.security.set_pin(&pin);
        }
        if let Some(list) = cfg.trusted_peers {
            state.security.set_trusted_peers(list);
        }
    }
    let snap = state.security.snapshot();
    ret_jstr(
        &mut env,
        &serde_json::json!({
            "ok": true,
            "encrypt_transport": snap.encrypt_transport,
            "whitelist_only": snap.whitelist_only,
            "pairing_pin": snap.pairing_pin,
            "trusted_peers": snap.trusted_peers,
        })
        .to_string(),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_larevole_liersync_nativebridge_LierSyncNative_pairWithPeerJson(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    target_addr: JString<'_>,
    pin: JString<'_>,
) -> jstring {
    let addr_str = jstr(&mut env, &target_addr);
    let pin_str = jstr(&mut env, &pin);
    let Ok(addr) = addr_str.trim().parse::<SocketAddr>() else {
        return ret_jstr(&mut env, "{\"ok\":false,\"error\":\"Invalid ip:port\"}");
    };
    let Some(state) = NODE.get() else {
        return ret_jstr(&mut env, "{\"ok\":false,\"error\":\"Node not running\"}");
    };
    let id = state.identity.clone();
    match state.rt.block_on(pair_with_peer(addr, &id, &pin_str)) {
        Ok(peer) => {
            state.security.add_or_update_trusted(
                &peer.device_id,
                &peer.device_name,
                &peer.fingerprint,
            );
            ret_jstr(
                &mut env,
                &serde_json::json!({
                    "ok": true,
                    "device_id": peer.device_id,
                    "device_name": peer.device_name,
                    "fingerprint": peer.fingerprint,
                    "added_at": peer.added_at,
                })
                .to_string(),
            )
        }
        Err(e) => ret_jstr(
            &mut env,
            &serde_json::json!({ "ok": false, "error": e.to_string() }).to_string(),
        ),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_larevole_liersync_nativebridge_LierSyncNative_generatePairingQrPng(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    addr_str: JString<'_>,
    pin_str: JString<'_>,
    size_px: jint,
) -> jni::sys::jbyteArray {
    let addr = jstr(&mut env, &addr_str);
    let pin = jstr(&mut env, &pin_str);
    let (id_str, name_str, fp_str) = NODE
        .get()
        .map(|s| {
            (
                s.identity.device_id.clone(),
                s.identity.device_name.clone(),
                s.identity.fingerprint.clone(),
            )
        })
        .unwrap_or_default();
    let uri = build_pairing_uri(&addr, &id_str, &name_str, &fp_str, &pin);
    let png = generate_qr_png(&uri, size_px.max(128) as u32).unwrap_or_default();
    env.byte_array_from_slice(&png)
        .map(|a| a.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_org_larevole_liersync_nativebridge_LierSyncNative_parsePairingUriJson(
    mut env: JNIEnv<'_>,
    _class: JClass<'_>,
    raw_uri: JString<'_>,
) -> jstring {
    let raw = jstr(&mut env, &raw_uri);
    match parse_pairing_uri(&raw) {
        Some(p) => ret_jstr(
            &mut env,
            &serde_json::json!({
                "ok": true,
                "addr": p.addr,
                "device_id": p.device_id,
                "device_name": p.device_name,
                "fingerprint": p.fingerprint,
                "pin": p.pin
            })
            .to_string(),
        ),
        None => ret_jstr(
            &mut env,
            "{\"ok\":false,\"error\":\"Invalid LierSync QR code\"}",
        ),
    }
}
