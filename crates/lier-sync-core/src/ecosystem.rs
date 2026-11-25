// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

use crate::clipboard::{ClipboardGuard, set_clipboard_image_png, set_clipboard_text};
use crate::discovery::DiscoveryService;
use crate::error::{LierError, Result};
use crate::identity::DeviceIdentity;
use crate::merkle::hash_bytes;
use crate::transport::{send_clipboard_image_png, send_clipboard_text, send_quick_batch};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

pub const LIERKOUR_EXT_PORT: u16 = 8765;
pub const LIERTAGS_EXT_PORT: u16 = 8766;
pub const LIERKOUR_SYNC_PORT: u16 = 8767;
pub const LIERSYNC_BRIDGE_PORT: u16 = 8768;

const MAX_HTTP_BODY_BYTES: usize = 24 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcosystemAppStatus {
    pub id: String,
    pub name: String,
    pub port: u16,
    pub online: bool,
    pub version: Option<String>,
    pub platform: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone)]
pub enum EcosystemBridgeEvent {
    Log(String),
    IncomingBridgeFiles {
        source: String,
        target: SocketAddr,
        count: usize,
        bytes: u64,
    },
}

#[derive(Clone)]
pub struct EcosystemBridgeContext {
    pub identity: DeviceIdentity,
    pub p2p_port: u16,
    pub targets: Arc<Mutex<Vec<SocketAddr>>>,
    pub discovery: DiscoveryService,
    pub clipboard_guard: ClipboardGuard,
    pub events: mpsc::Sender<EcosystemBridgeEvent>,
}

#[derive(Debug, Deserialize)]
struct SendFilesRequest {
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ClipboardBridgeRequest {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    image_base64: Option<String>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

pub fn decode_base64(input: &str) -> Result<Vec<u8>> {
    let trimmed = input.trim();
    let payload = match trimmed.find(";base64,") {
        Some(idx) => &trimmed[idx + 8..],
        None => trimmed,
    };

    let mut out = Vec::with_capacity((payload.len() / 4) * 3);
    let mut buf = [0u8; 4];
    let mut len = 0usize;
    let mut pad = 0usize;

    for b in payload.bytes() {
        if b.is_ascii_whitespace() {
            continue;
        }
        if b == b'=' {
            buf[len] = 0;
            len += 1;
            pad += 1;
        } else {
            let val = match b {
                b'A'..=b'Z' => b - b'A',
                b'a'..=b'z' => b - b'a' + 26,
                b'0'..=b'9' => b - b'0' + 52,
                b'+' | b'-' => 62,
                b'/' | b'_' => 63,
                _ => return Err(LierError::Protocol(format!("Invalid base64 byte: {b}"))),
            };
            if pad > 0 {
                return Err(LierError::Protocol(
                    "Unexpected data after base64 padding".into(),
                ));
            }
            buf[len] = val;
            len += 1;
        }

        if len == 4 {
            out.push((buf[0] << 2) | (buf[1] >> 4));
            if pad < 2 {
                out.push((buf[1] << 4) | (buf[2] >> 2));
            }
            if pad == 0 {
                out.push((buf[2] << 6) | buf[3]);
            }
            len = 0;
            pad = 0;
        }
    }

    if len != 0 {
        return Err(LierError::Protocol("Truncated base64 input".into()));
    }
    Ok(out)
}

pub async fn start_ecosystem_bridge(port: u16, ctx: EcosystemBridgeContext) -> Result<SocketAddr> {
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let listener = TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let c = ctx.clone();
                    tokio::spawn(async move {
                        let _ = handle_bridge_conn(stream, bound.port(), c).await;
                    });
                }
                Err(_) => break,
            }
        }
    });

    Ok(bound)
}

async fn handle_bridge_conn(
    mut stream: TcpStream,
    bridge_port: u16,
    ctx: EcosystemBridgeContext,
) -> Result<()> {
    let mut raw = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let header_end;

    loop {
        let n = tokio::time::timeout(Duration::from_secs(10), stream.read(&mut chunk))
            .await
            .map_err(|_| LierError::Protocol("HTTP read timeout".into()))??;
        if n == 0 {
            return Ok(());
        }
        raw.extend_from_slice(&chunk[..n]);
        if let Some(pos) = find_subslice(&raw, b"\r\n\r\n") {
            header_end = pos + 4;
            break;
        }
        if raw.len() > 64 * 1024 {
            send_http_json(
                &mut stream,
                431,
                serde_json::json!({ "error": "Headers too large" }),
            )
            .await?;
            return Ok(());
        }
    }

    let (method, path, content_length) = {
        let header_text = String::from_utf8_lossy(&raw[..header_end]);
        let mut lines = header_text.lines();
        let request_line = lines.next().unwrap_or("");
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or("GET").to_ascii_uppercase();
        let full_path = parts.next().unwrap_or("/");
        let path = full_path.split('?').next().unwrap_or("/").to_string();

        let mut content_length = 0usize;
        for line in lines {
            if let Some((k, v)) = line.split_once(':') {
                if k.trim().eq_ignore_ascii_case("content-length") {
                    content_length = v.trim().parse::<usize>().unwrap_or(0);
                }
            }
        }
        (method, path, content_length)
    };

    if content_length > MAX_HTTP_BODY_BYTES {
        send_http_json(
            &mut stream,
            413,
            serde_json::json!({ "error": "Payload too large" }),
        )
        .await?;
        return Ok(());
    }

    while raw.len() < header_end + content_length {
        let n = tokio::time::timeout(Duration::from_secs(15), stream.read(&mut chunk))
            .await
            .map_err(|_| LierError::Protocol("HTTP body read timeout".into()))??;
        if n == 0 {
            break;
        }
        raw.extend_from_slice(&chunk[..n]);
    }

    let body = &raw[header_end..(header_end + content_length).min(raw.len())];

    if method == "OPTIONS" {
        send_http_empty(&mut stream, 204).await?;
        return Ok(());
    }

    match (method.as_str(), path.as_str()) {
        ("GET", "/api/lier/info") | ("GET", "/api/health") | ("GET", "/api/status") => {
            let peers: Vec<serde_json::Value> = ctx
                .discovery
                .peers()
                .await
                .into_iter()
                .map(|p| {
                    serde_json::json!({
                        "device_id": p.device_id,
                        "device_name": p.device_name,
                        "os": p.os,
                        "addr": p.addr.to_string(),
                    })
                })
                .collect();
            let targets: Vec<String> = ctx
                .targets
                .lock()
                .map(|g| g.iter().map(|a| a.to_string()).collect())
                .unwrap_or_default();
            send_http_json(
                &mut stream,
                200,
                serde_json::json!({
                    "ecosystem": "LIER",
                    "app": "LierSync",
                    "status": "running",
                    "version": env!("CARGO_PKG_VERSION"),
                    "port": bridge_port,
                    "p2p_port": ctx.p2p_port,
                    "platform": std::env::consts::OS,
                    "license": "GPL-3.0-only",
                    "device_name": ctx.identity.device_name,
                    "device_id": ctx.identity.device_id,
                    "fingerprint": ctx.identity.fingerprint,
                    "peers": peers,
                    "targets": targets,
                    "capabilities": ["quick_send", "clipboard_text", "clipboard_image", "sync_folders"]
                }),
            )
            .await?;
        }
        ("POST", "/api/ecosystem/send-files") => {
            let req: SendFilesRequest = match serde_json::from_slice(body) {
                Ok(v) => v,
                Err(e) => {
                    send_http_json(
                        &mut stream,
                        400,
                        serde_json::json!({ "success": false, "error": format!("Invalid JSON: {e}") }),
                    )
                    .await?;
                    return Ok(());
                }
            };
            let source = req.source.unwrap_or_else(|| "Ecosystem".to_string());
            let valid_paths: Vec<PathBuf> = req
                .paths
                .into_iter()
                .map(PathBuf::from)
                .filter(|p| p.exists())
                .collect();
            if valid_paths.is_empty() {
                send_http_json(
                    &mut stream,
                    400,
                    serde_json::json!({ "success": false, "error": "No existing paths supplied" }),
                )
                .await?;
                return Ok(());
            }
            let target = match resolve_target(req.target.as_deref(), &ctx).await {
                Some(t) => t,
                None => {
                    send_http_json(
                        &mut stream,
                        409,
                        serde_json::json!({ "success": false, "error": "No target LierSync peer configured or discovered" }),
                    )
                    .await?;
                    return Ok(());
                }
            };

            let mut total_bytes = 0u64;
            match send_quick_batch(
                target,
                &ctx.identity,
                &valid_paths,
                |_done, _total, _sb, tb, _name| {
                    total_bytes = tb;
                },
            )
            .await
            {
                Ok(count) => {
                    let _ = ctx
                        .events
                        .send(EcosystemBridgeEvent::IncomingBridgeFiles {
                            source: source.clone(),
                            target,
                            count,
                            bytes: total_bytes,
                        })
                        .await;
                    send_http_json(
                        &mut stream,
                        200,
                        serde_json::json!({
                            "success": true,
                            "source": source,
                            "target": target.to_string(),
                            "sent_files": count,
                            "bytes": total_bytes
                        }),
                    )
                    .await?;
                }
                Err(e) => {
                    let _ = ctx
                        .events
                        .send(EcosystemBridgeEvent::Log(format!(
                            "Ecosystem send error ({source} -> {target}): {e}"
                        )))
                        .await;
                    send_http_json(
                        &mut stream,
                        502,
                        serde_json::json!({ "success": false, "error": e.to_string() }),
                    )
                    .await?;
                }
            }
        }
        ("POST", "/api/ecosystem/clipboard") => {
            let req: ClipboardBridgeRequest = match serde_json::from_slice(body) {
                Ok(v) => v,
                Err(e) => {
                    send_http_json(
                        &mut stream,
                        400,
                        serde_json::json!({ "success": false, "error": format!("Invalid JSON: {e}") }),
                    )
                    .await?;
                    return Ok(());
                }
            };
            let source = req.source.unwrap_or_else(|| "Ecosystem".to_string());
            let targets = resolve_all_targets(req.target.as_deref(), &ctx).await;

            if let Some(img_b64) = req.image_base64.filter(|s| !s.trim().is_empty()) {
                let raw_img = match decode_base64(&img_b64) {
                    Ok(v) => v,
                    Err(e) => {
                        send_http_json(
                            &mut stream,
                            400,
                            serde_json::json!({ "success": false, "error": e.to_string() }),
                        )
                        .await?;
                        return Ok(());
                    }
                };
                let (w, h) = match image::load_from_memory_with_format(
                    &raw_img,
                    image::ImageFormat::Png,
                ) {
                    Ok(img) => (img.width(), img.height()),
                    Err(_) => {
                        send_http_json(
                            &mut stream,
                            400,
                            serde_json::json!({ "success": false, "error": "Clipboard bridge image must be PNG" }),
                        )
                        .await?;
                        return Ok(());
                    }
                };
                let _ = set_clipboard_image_png(&raw_img);
                let hash = hash_bytes(&raw_img);
                let mut pushed = 0usize;
                for addr in &targets {
                    ctx.clipboard_guard
                        .record_sent_image(&addr.to_string(), w, h, &raw_img, &hash);
                    if send_clipboard_image_png(*addr, &ctx.identity, w, h, raw_img.clone())
                        .await
                        .is_ok()
                    {
                        pushed += 1;
                    }
                }
                let _ = ctx
                    .events
                    .send(EcosystemBridgeEvent::Log(format!(
                        "[{source}] Clipboard PNG ({w}x{h}) -> {pushed} peer(s)"
                    )))
                    .await;
                send_http_json(
                    &mut stream,
                    200,
                    serde_json::json!({
                        "success": true,
                        "kind": "image",
                        "width": w,
                        "height": h,
                        "pushed_peers": pushed
                    }),
                )
                .await?;
                return Ok(());
            }

            let text_payload = req
                .text
                .filter(|s| !s.is_empty())
                .or_else(|| req.url.filter(|s| !s.is_empty()));

            let Some(text) = text_payload else {
                send_http_json(
                    &mut stream,
                    400,
                    serde_json::json!({ "success": false, "error": "Missing text, url or image_base64" }),
                )
                .await?;
                return Ok(());
            };

            let _ = set_clipboard_text(&text);
            let hash = hash_bytes(text.as_bytes());
            let mut pushed = 0usize;
            for addr in &targets {
                ctx.clipboard_guard
                    .record_sent(&addr.to_string(), &text, &hash);
                if send_clipboard_text(*addr, &ctx.identity, &text)
                    .await
                    .is_ok()
                {
                    pushed += 1;
                }
            }
            let _ = ctx
                .events
                .send(EcosystemBridgeEvent::Log(format!(
                    "[{source}] Clipboard text -> {pushed} peer(s)"
                )))
                .await;
            send_http_json(
                &mut stream,
                200,
                serde_json::json!({
                    "success": true,
                    "kind": "text",
                    "pushed_peers": pushed
                }),
            )
            .await?;
        }
        _ => {
            send_http_json(
                &mut stream,
                404,
                serde_json::json!({ "error": "Not found" }),
            )
            .await?;
        }
    }

    Ok(())
}

async fn resolve_target(
    explicit: Option<&str>,
    ctx: &EcosystemBridgeContext,
) -> Option<SocketAddr> {
    if let Some(raw) = explicit.map(str::trim).filter(|s| !s.is_empty()) {
        if let Ok(addr) = raw.parse::<SocketAddr>() {
            return Some(addr);
        }
    }
    if let Ok(g) = ctx.targets.lock() {
        if let Some(first) = g.first() {
            return Some(*first);
        }
    }
    ctx.discovery.peers().await.first().map(|p| p.addr)
}

async fn resolve_all_targets(
    explicit: Option<&str>,
    ctx: &EcosystemBridgeContext,
) -> Vec<SocketAddr> {
    if let Some(raw) = explicit.map(str::trim).filter(|s| !s.is_empty()) {
        if let Ok(addr) = raw.parse::<SocketAddr>() {
            return vec![addr];
        }
    }
    if let Ok(g) = ctx.targets.lock() {
        if !g.is_empty() {
            return g.clone();
        }
    }
    ctx.discovery
        .peers()
        .await
        .into_iter()
        .map(|p| p.addr)
        .collect()
}

pub async fn probe_ecosystem_apps(liersync_bridge_port: u16) -> Vec<EcosystemAppStatus> {
    let specs = [
        (
            "liersync",
            "LierSync Bridge",
            liersync_bridge_port,
            &["/api/lier/info"][..],
        ),
        (
            "lierkour",
            "Lierkour (Ext)",
            LIERKOUR_EXT_PORT,
            &["/api/lier/info", "/api/health"][..],
        ),
        (
            "liertags",
            "LierTags",
            LIERTAGS_EXT_PORT,
            &["/api/lier/info", "/api/status"][..],
        ),
        (
            "lierkour_sync",
            "Lierkour Sync",
            LIERKOUR_SYNC_PORT,
            &["/api/lier/info", "/api/v1/sync/ping"][..],
        ),
    ];

    let mut out = Vec::with_capacity(specs.len());
    for (id, name, port, paths) in specs {
        let mut status = EcosystemAppStatus {
            id: id.to_string(),
            name: name.to_string(),
            port,
            online: false,
            version: None,
            platform: None,
            detail: "Offline".to_string(),
        };
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        if let Ok(Ok(_)) =
            tokio::time::timeout(Duration::from_millis(350), TcpStream::connect(addr)).await
        {
            status.online = true;
            status.detail = "Online".to_string();
            for p in paths {
                if let Some(json) = http_get_json_localhost(port, p).await {
                    status.version = json
                        .get("version")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    status.platform = json
                        .get("platform")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    if let Some(ver) = &status.version {
                        status.detail = format!("v{ver}");
                    }
                    break;
                }
            }
        }
        out.push(status);
    }
    out
}

async fn http_get_json_localhost(port: u16, path: &str) -> Option<serde_json::Value> {
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let mut stream = tokio::time::timeout(Duration::from_millis(450), TcpStream::connect(addr))
        .await
        .ok()?
        .ok()?;
    let req = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    tokio::time::timeout(Duration::from_millis(450), stream.write_all(req.as_bytes()))
        .await
        .ok()?
        .ok()?;
    let mut buf = Vec::new();
    let _ = tokio::time::timeout(Duration::from_millis(500), stream.read_to_end(&mut buf)).await;
    let body_idx = find_subslice(&buf, b"\r\n\r\n").map(|p| p + 4)?;
    serde_json::from_slice(&buf[body_idx..]).ok()
}

async fn send_http_json(stream: &mut TcpStream, code: u16, value: serde_json::Value) -> Result<()> {
    let body = serde_json::to_vec(&value)?;
    let reason = match code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        409 => "Conflict",
        413 => "Payload Too Large",
        431 => "Request Header Fields Too Large",
        502 => "Bad Gateway",
        _ => "Error",
    };
    let headers = format!(
        "HTTP/1.1 {code} {reason}\r\n\
         Content-Type: application/json; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type, X-Lier-Token, X-LierEX-Token\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(headers.as_bytes()).await?;
    stream.write_all(&body).await?;
    stream.flush().await?;
    Ok(())
}

async fn send_http_empty(stream: &mut TcpStream, code: u16) -> Result<()> {
    let headers = format!(
        "HTTP/1.1 {code} No Content\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
         Access-Control-Allow-Headers: Content-Type, X-Lier-Token, X-LierEX-Token\r\n\
         Content-Length: 0\r\n\
         Connection: close\r\n\r\n"
    );
    stream.write_all(headers.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_decode_strips_data_url_and_decodes() {
        let bytes = decode_base64("data:image/png;base64,SGVsbG8h").unwrap();
        assert_eq!(bytes, b"Hello!");
    }

    #[tokio::test]
    async fn bridge_serves_lier_info_json() {
        let (tx, _rx) = mpsc::channel(4);
        let ctx = EcosystemBridgeContext {
            identity: DeviceIdentity {
                device_id: "test-node".into(),
                device_name: "TestPC".into(),
                os: "windows".into(),
                fingerprint: "abcd1234".into(),
            },
            p2p_port: 47789,
            targets: Arc::new(Mutex::new(Vec::new())),
            discovery: DiscoveryService::new(),
            clipboard_guard: ClipboardGuard::new(50),
            events: tx,
        };
        let bound = start_ecosystem_bridge(0, ctx).await.unwrap();
        let json = http_get_json_localhost(bound.port(), "/api/lier/info")
            .await
            .expect("bridge info response");
        assert_eq!(json["ecosystem"], "LIER");
        assert_eq!(json["app"], "LierSync");
        assert_eq!(json["license"], "GPL-3.0-only");
    }
}
