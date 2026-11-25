// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

use crate::clipboard::{
    get_clipboard_image_png, get_clipboard_text, set_clipboard_image_png, set_clipboard_text,
    ClipboardGuard,
};
use crate::error::{LierError, Result};
use crate::identity::DeviceIdentity;
use crate::merkle::{hash_bytes, hash_file, CHUNK_SIZE};
use crate::protocol::Frame;
use crate::security::SecurityManager;
use crate::transport::{expand_paths_for_send, CoreEvent, DEFAULT_P2P_PORT};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use uuid::Uuid;

/// LierSync RFCOMM Serial Port Profile (SPP) UUID shared across Windows & Android:
/// `7a3f21b0-4c8e-4e9b-9e12-a5b6c7d8e901`
pub const LIERSYNC_BT_UUID_STR: &str = "7a3f21b0-4c8e-4e9b-9e12-a5b6c7d8e901";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BluetoothPeerInfo {
    pub name: String,
    pub mac: String,
    pub connected: bool,
}

pub fn parse_bt_mac(mac: &str) -> Option<u64> {
    let hex: String = mac.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 12 {
        return None;
    }
    u64::from_str_radix(&hex, 16).ok()
}

pub fn format_bt_mac(addr: u64) -> String {
    let b = addr.to_be_bytes();
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        b[2], b[3], b[4], b[5], b[6], b[7]
    )
}

const MAX_BT_FRAME_LEN: usize = 16 * 1024 * 1024;

fn write_sync_frame<W: Write>(w: &mut W, frame: &Frame) -> Result<()> {
    let raw = serde_json::to_vec(frame)?;
    let len = u32::try_from(raw.len())
        .map_err(|_| LierError::Protocol("Bluetooth frame too large".into()))?;
    w.write_all(&len.to_be_bytes())?;
    w.write_all(&raw)?;
    w.flush()?;
    Ok(())
}

fn read_sync_frame<R: Read>(r: &mut R) -> Result<Option<Frame>> {
    let mut len_buf = [0u8; 4];
    match r.read_exact(&mut len_buf) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_BT_FRAME_LEN {
        return Err(LierError::Protocol(format!(
            "Bluetooth frame too large ({len} bytes)"
        )));
    }
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload)?;
    let frame = serde_json::from_slice(&payload)?;
    Ok(Some(frame))
}

#[cfg(windows)]
mod win_bt {
    use super::*;
    use windows_sys::core::GUID;
    use windows_sys::Win32::Devices::Bluetooth::{
        BluetoothFindDeviceClose, BluetoothFindFirstDevice, BluetoothFindFirstRadio,
        BluetoothFindNextDevice, BluetoothFindRadioClose, BLUETOOTH_DEVICE_INFO,
        BLUETOOTH_DEVICE_SEARCH_PARAMS, BLUETOOTH_FIND_RADIO_PARAMS,
    };
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Networking::WinSock::{
        accept, bind, closesocket, connect, getsockname, listen, recv, send, socket,
        WSACleanup, WSASetServiceW, WSAStartup, CSADDR_INFO, RNRSERVICE_REGISTER, SOCKET,
        SOCKET_ADDRESS, SOCKET_ERROR, SOCK_STREAM, WSADATA, WSAQUERYSETW,
    };

    const AF_BTH: u16 = 32;
    const BTHPROTO_RFCOMM: u32 = 3;

    const LIERSYNC_SPP_GUID: GUID = GUID {
        data1: 0x7a3f_21b0,
        data2: 0x4c8e,
        data3: 0x4e9b,
        data4: [0x9e, 0x12, 0xa5, 0xb6, 0xc7, 0xd8, 0xe9, 0x01],
    };

    #[repr(C, packed)]
    #[derive(Clone, Copy)]
    struct SockAddrBth {
        address_family: u16,
        bt_addr: u64,
        service_class_id: GUID,
        port: u32,
    }

    pub struct BthSocketStream {
        sock: SOCKET,
    }

    impl Drop for BthSocketStream {
        fn drop(&mut self) {
            unsafe {
                let _ = closesocket(self.sock);
            }
        }
    }

    impl Read for BthSocketStream {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = unsafe { recv(self.sock, buf.as_mut_ptr(), buf.len() as i32, 0) };
            if n == SOCKET_ERROR {
                return Err(std::io::Error::last_os_error());
            }
            Ok(n as usize)
        }
    }

    impl Write for BthSocketStream {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let n = unsafe { send(self.sock, buf.as_ptr(), buf.len() as i32, 0) };
            if n == SOCKET_ERROR {
                return Err(std::io::Error::last_os_error());
            }
            Ok(n as usize)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn ensure_wsa() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| unsafe {
            let mut data: WSADATA = std::mem::zeroed();
            let _ = WSAStartup(0x0202, &mut data);
            // Process keeps Winsock active; no teardown needed until exit.
            let _ = WSACleanup as *const ();
        });
    }

    pub fn has_radio() -> bool {
        unsafe {
            let params = BLUETOOTH_FIND_RADIO_PARAMS {
                dwSize: std::mem::size_of::<BLUETOOTH_FIND_RADIO_PARAMS>() as u32,
            };
            let mut radio: HANDLE = std::ptr::null_mut();
            let h_find = BluetoothFindFirstRadio(&params, &mut radio);
            if h_find == std::ptr::null_mut() || h_find == INVALID_HANDLE_VALUE {
                return false;
            }
            if !radio.is_null() && radio != INVALID_HANDLE_VALUE {
                let _ = CloseHandle(radio);
            }
            let _ = BluetoothFindRadioClose(h_find);
            true
        }
    }

    pub fn list_paired() -> Vec<BluetoothPeerInfo> {
        let mut out = Vec::new();
        unsafe {
            let params = BLUETOOTH_DEVICE_SEARCH_PARAMS {
                dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32,
                fReturnAuthenticated: 1,
                fReturnRemembered: 1,
                fReturnUnknown: 0,
                fReturnConnected: 1,
                fIssueInquiry: 0,
                cTimeoutMultiplier: 0,
                hRadio: std::ptr::null_mut(),
            };
            let mut info: BLUETOOTH_DEVICE_INFO = std::mem::zeroed();
            info.dwSize = std::mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32;
            let h_find = BluetoothFindFirstDevice(&params, &mut info);
            if h_find.is_null() || h_find == INVALID_HANDLE_VALUE {
                return out;
            }
            loop {
                let nul = info
                    .szName
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(info.szName.len());
                let name = String::from_utf16_lossy(&info.szName[..nul]).trim().to_string();
                let addr_raw = info.Address.Anonymous.ullLong;
                out.push(BluetoothPeerInfo {
                    name: if name.is_empty() {
                        format_bt_mac(addr_raw)
                    } else {
                        name
                    },
                    mac: format_bt_mac(addr_raw),
                    connected: info.fConnected != 0,
                });
                info = std::mem::zeroed();
                info.dwSize = std::mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32;
                if BluetoothFindNextDevice(h_find, &mut info) == 0 {
                    break;
                }
            }
            let _ = BluetoothFindDeviceClose(h_find);
        }
        out
    }

    pub fn connect_rfcomm(mac: &str) -> Result<BthSocketStream> {
        let bt_addr = parse_bt_mac(mac)
            .ok_or_else(|| LierError::Protocol(format!("Invalid Bluetooth MAC: {mac}")))?;
        ensure_wsa();
        unsafe {
            let sock = socket(AF_BTH as i32, SOCK_STREAM as i32, BTHPROTO_RFCOMM as i32);
            if sock == usize::MAX {
                return Err(LierError::Io(std::io::Error::last_os_error()));
            }
            let sa = SockAddrBth {
                address_family: AF_BTH,
                bt_addr,
                service_class_id: LIERSYNC_SPP_GUID,
                port: 0,
            };
            let rc = connect(
                sock,
                (&sa as *const SockAddrBth).cast(),
                std::mem::size_of::<SockAddrBth>() as i32,
            );
            if rc == SOCKET_ERROR {
                let err = std::io::Error::last_os_error();
                let _ = closesocket(sock);
                return Err(LierError::Io(err));
            }
            Ok(BthSocketStream { sock })
        }
    }

    pub fn start_rfcomm_server(
        identity: DeviceIdentity,
        receive_dir: PathBuf,
        clipboard_guard: ClipboardGuard,
        security: SecurityManager,
        events: mpsc::Sender<CoreEvent>,
    ) -> Result<()> {
        if !has_radio() {
            return Ok(());
        }
        ensure_wsa();
        std::thread::Builder::new()
            .name("liersync-bt-server".into())
            .spawn(move || unsafe {
                let server = socket(AF_BTH as i32, SOCK_STREAM as i32, BTHPROTO_RFCOMM as i32);
                if server == usize::MAX {
                    return;
                }
                let mut sa = SockAddrBth {
                    address_family: AF_BTH,
                    bt_addr: 0,
                    service_class_id: GUID::from_u128(0),
                    port: u32::MAX, // BT_PORT_ANY
                };
                if bind(
                    server,
                    (&sa as *const SockAddrBth).cast(),
                    std::mem::size_of::<SockAddrBth>() as i32,
                ) == SOCKET_ERROR
                {
                    let _ = closesocket(server);
                    return;
                }
                let mut sa_len = std::mem::size_of::<SockAddrBth>() as i32;
                let _ = getsockname(
                    server,
                    (&mut sa as *mut SockAddrBth).cast(),
                    &mut sa_len,
                );
                if listen(server, 4) == SOCKET_ERROR {
                    let _ = closesocket(server);
                    return;
                }

                // Register SDP service record so Android peers resolve LierSync UUID -> channel
                let mut svc_name: Vec<u16> = "LierSync Bluetooth\0".encode_utf16().collect();
                let mut svc_comment: Vec<u16> =
                    "LierSync P2P RFCOMM Sync\0".encode_utf16().collect();
                let mut guid = LIERSYNC_SPP_GUID;
                let mut cs_addr = CSADDR_INFO {
                    LocalAddr: SOCKET_ADDRESS {
                        lpSockaddr: (&mut sa as *mut SockAddrBth).cast(),
                        iSockaddrLength: std::mem::size_of::<SockAddrBth>() as i32,
                    },
                    RemoteAddr: SOCKET_ADDRESS {
                        lpSockaddr: std::ptr::null_mut(),
                        iSockaddrLength: 0,
                    },
                    iSocketType: SOCK_STREAM as i32,
                    iProtocol: BTHPROTO_RFCOMM as i32,
                };
                let query_set = WSAQUERYSETW {
                    dwSize: std::mem::size_of::<WSAQUERYSETW>() as u32,
                    lpszServiceInstanceName: svc_name.as_mut_ptr(),
                    lpServiceClassId: &mut guid,
                    lpVersion: std::ptr::null_mut(),
                    lpszComment: svc_comment.as_mut_ptr(),
                    dwNameSpace: 16, // NS_BTH
                    lpNSProviderId: std::ptr::null_mut(),
                    lpszContext: std::ptr::null_mut(),
                    dwNumberOfProtocols: 0,
                    lpafpProtocols: std::ptr::null_mut(),
                    lpszQueryString: std::ptr::null_mut(),
                    dwNumberOfCsAddrs: 1,
                    lpcsaBuffer: &mut cs_addr,
                    dwOutputFlags: 0,
                    lpBlob: std::ptr::null_mut(),
                };
                let _ = WSASetServiceW(&query_set, RNRSERVICE_REGISTER, 0);

                loop {
                    let client = accept(server, std::ptr::null_mut(), std::ptr::null_mut());
                    if client == usize::MAX {
                        std::thread::sleep(std::time::Duration::from_millis(250));
                        continue;
                    }
                    let id = identity.clone();
                    let rx_dir = receive_dir.clone();
                    let cb = clipboard_guard.clone();
                    let sec = security.clone();
                    let ev = events.clone();
                    std::thread::spawn(move || {
                        let stream = BthSocketStream { sock: client };
                        let _ = handle_bt_client(stream, id, rx_dir, cb, sec, ev);
                    });
                }
            })
            .map(|_| ())
            .map_err(LierError::Io)
    }
}

fn handle_bt_client<S: Read + Write>(
    mut stream: S,
    identity: DeviceIdentity,
    receive_dir: PathBuf,
    clipboard_guard: ClipboardGuard,
    security: SecurityManager,
    events: mpsc::Sender<CoreEvent>,
) -> Result<()> {
    let mut peer_name = "Bluetooth-Peer".to_string();
    let mut peer_trusted = !security.whitelist_only();
    let mut active_transfers: HashMap<String, (PathBuf, Option<String>, String, u64, File)> =
        HashMap::new();

    while let Some(frame) = read_sync_frame(&mut stream)? {
        match frame {
            Frame::Hello {
                device_id,
                device_name,
                fingerprint,
                ..
            } => {
                peer_name = device_name;
                peer_trusted = if security.whitelist_only() {
                    security.is_trusted(&device_id, &fingerprint)
                } else {
                    true
                };
                write_sync_frame(
                    &mut stream,
                    &Frame::Hello {
                        device_id: identity.device_id.clone(),
                        device_name: identity.device_name.clone(),
                        os: identity.os.clone(),
                        fingerprint: identity.fingerprint.clone(),
                        p2p_port: Some(DEFAULT_P2P_PORT),
                    },
                )?;
            }
            Frame::ClipboardPush {
                sender_name,
                sha256,
                text,
                ..
            } => {
                if security.whitelist_only() && !peer_trusted {
                    write_sync_frame(
                        &mut stream,
                        &Frame::Error {
                            message: "Bluetooth peer not in whitelist".into(),
                        },
                    )?;
                    continue;
                }
                if clipboard_guard.accept_remote(&sender_name, &text, &sha256) {
                    let _ = set_clipboard_text(&text);
                    let _ = events.blocking_send(CoreEvent::ClipboardReceived {
                        from_peer: format!("{sender_name} (BT)"),
                        preview: text.chars().take(60).collect(),
                        full_text: text,
                    });
                }
                write_sync_frame(
                    &mut stream,
                    &Frame::Ack {
                        message: "BT Clipboard synced".into(),
                    },
                )?;
            }
            Frame::ClipboardImagePush {
                sender_name,
                sha256,
                width,
                height,
                png_bytes,
                ..
            } => {
                if security.whitelist_only() && !peer_trusted {
                    write_sync_frame(
                        &mut stream,
                        &Frame::Error {
                            message: "Bluetooth peer not in whitelist".into(),
                        },
                    )?;
                    continue;
                }
                if clipboard_guard.accept_remote_image(&sender_name, width, height, &png_bytes, &sha256) {
                    let _ = set_clipboard_image_png(&png_bytes);
                    let _ = events.blocking_send(CoreEvent::ClipboardImageReceived {
                        from_peer: format!("{sender_name} (BT)"),
                        width,
                        height,
                        png_bytes,
                        sha256,
                    });
                }
                write_sync_frame(
                    &mut stream,
                    &Frame::Ack {
                        message: "BT Image synced".into(),
                    },
                )?;
            }
            Frame::ClipboardPullRequest => {
                if let Some(text) = get_clipboard_text().ok().filter(|t| !t.is_empty()) {
                    let sha256 = hash_bytes(text.as_bytes());
                    write_sync_frame(
                        &mut stream,
                        &Frame::ClipboardPullResponse {
                            sender_name: identity.device_name.clone(),
                            sha256,
                            text,
                        },
                    )?;
                } else if let Ok((w, h, png)) = get_clipboard_image_png() {
                    let sha256 = hash_bytes(&png);
                    write_sync_frame(
                        &mut stream,
                        &Frame::ClipboardImageResponse {
                            sender_name: identity.device_name.clone(),
                            sha256,
                            width: w,
                            height: h,
                            png_bytes: png,
                        },
                    )?;
                } else {
                    write_sync_frame(
                        &mut stream,
                        &Frame::ClipboardPullResponse {
                            sender_name: identity.device_name.clone(),
                            sha256: hash_bytes(b""),
                            text: String::new(),
                        },
                    )?;
                }
            }
            Frame::QuickSendOffer {
                transfer_id,
                file_name,
                rel_path,
                file_size,
                sha256,
            } => {
                if security.whitelist_only() && !peer_trusted {
                    write_sync_frame(
                        &mut stream,
                        &Frame::QuickSendAccept {
                            transfer_id,
                            accepted: false,
                        },
                    )?;
                    continue;
                }
                fs::create_dir_all(&receive_dir)?;
                let safe_name = Path::new(&file_name)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("bt_file");
                let mut dest = receive_dir.join(safe_name);
                if dest.exists() {
                    dest = receive_dir.join(format!("{}_{safe_name}", Uuid::new_v4()));
                }
                let file = File::create(&dest)?;
                active_transfers.insert(
                    transfer_id.clone(),
                    (dest, rel_path, sha256, file_size, file),
                );
                write_sync_frame(
                    &mut stream,
                    &Frame::QuickSendAccept {
                        transfer_id,
                        accepted: true,
                    },
                )?;
            }
            Frame::FileChunk {
                transfer_id,
                data,
                ..
            } => {
                if let Some((_, _, _, _, file)) = active_transfers.get_mut(&transfer_id) {
                    file.write_all(&data)?;
                }
            }
            Frame::QuickSendDone { transfer_id } => {
                if let Some((dest, rel_path, expected_sha, expected_size, mut file)) =
                    active_transfers.remove(&transfer_id)
                {
                    file.flush()?;
                    drop(file);
                    let actual_sha = hash_file(&dest)?;
                    if !expected_sha.is_empty() && actual_sha != expected_sha {
                        return Err(LierError::IntegrityMismatch(dest.display().to_string()));
                    }
                    write_sync_frame(
                        &mut stream,
                        &Frame::Ack {
                            message: "BT file received".into(),
                        },
                    )?;
                    let _ = events.blocking_send(CoreEvent::IncomingFile {
                        from_peer: format!("{peer_name} (BT)"),
                        file_name: dest
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("file")
                            .to_string(),
                        rel_path,
                        saved_path: dest,
                        bytes: expected_size,
                    });
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn has_bluetooth_radio() -> bool {
    #[cfg(windows)]
    {
        win_bt::has_radio()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub fn list_paired_bluetooth_devices() -> Vec<BluetoothPeerInfo> {
    #[cfg(windows)]
    {
        win_bt::list_paired()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

pub fn start_bluetooth_server(
    identity: DeviceIdentity,
    receive_dir: PathBuf,
    clipboard_guard: ClipboardGuard,
    security: SecurityManager,
    events: mpsc::Sender<CoreEvent>,
) -> Result<()> {
    #[cfg(windows)]
    {
        win_bt::start_rfcomm_server(identity, receive_dir, clipboard_guard, security, events)
    }
    #[cfg(not(windows))]
    {
        let _ = (identity, receive_dir, clipboard_guard, security, events);
        Ok(())
    }
}

pub async fn send_bluetooth_clipboard_text(
    mac: String,
    identity: DeviceIdentity,
    text: String,
) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        #[cfg(windows)]
        {
            let mut stream = win_bt::connect_rfcomm(&mac)?;
            write_sync_frame(
                &mut stream,
                &Frame::Hello {
                    device_id: identity.device_id.clone(),
                    device_name: identity.device_name.clone(),
                    os: identity.os.clone(),
                    fingerprint: identity.fingerprint.clone(),
                    p2p_port: Some(DEFAULT_P2P_PORT),
                },
            )?;
            let _ = read_sync_frame(&mut stream)?;
            let sha256 = hash_bytes(text.as_bytes());
            write_sync_frame(
                &mut stream,
                &Frame::ClipboardPush {
                    sender_id: identity.device_id,
                    sender_name: identity.device_name,
                    sha256,
                    text,
                },
            )?;
            match read_sync_frame(&mut stream)? {
                Some(Frame::Error { message }) => Err(LierError::Protocol(message)),
                _ => Ok(()),
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (mac, identity, text);
            Err(LierError::Protocol("Bluetooth RFCOMM requires Android or Windows".into()))
        }
    })
    .await
    .map_err(|e| LierError::Protocol(e.to_string()))?
}

pub async fn send_bluetooth_clipboard_image(
    mac: String,
    identity: DeviceIdentity,
    width: u32,
    height: u32,
    png_bytes: Vec<u8>,
) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        #[cfg(windows)]
        {
            let mut stream = win_bt::connect_rfcomm(&mac)?;
            write_sync_frame(
                &mut stream,
                &Frame::Hello {
                    device_id: identity.device_id.clone(),
                    device_name: identity.device_name.clone(),
                    os: identity.os.clone(),
                    fingerprint: identity.fingerprint.clone(),
                    p2p_port: Some(DEFAULT_P2P_PORT),
                },
            )?;
            let _ = read_sync_frame(&mut stream)?;
            let sha256 = hash_bytes(&png_bytes);
            write_sync_frame(
                &mut stream,
                &Frame::ClipboardImagePush {
                    sender_id: identity.device_id,
                    sender_name: identity.device_name,
                    sha256,
                    width,
                    height,
                    png_bytes,
                },
            )?;
            match read_sync_frame(&mut stream)? {
                Some(Frame::Error { message }) => Err(LierError::Protocol(message)),
                _ => Ok(()),
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (mac, identity, width, height, png_bytes);
            Err(LierError::Protocol("Bluetooth RFCOMM requires Android or Windows".into()))
        }
    })
    .await
    .map_err(|e| LierError::Protocol(e.to_string()))?
}

pub async fn send_bluetooth_quick_batch(
    mac: String,
    identity: DeviceIdentity,
    paths: Vec<PathBuf>,
) -> Result<usize> {
    tokio::task::spawn_blocking(move || {
        #[cfg(windows)]
        {
            let items = expand_paths_for_send(&paths)?;
            if items.is_empty() {
                return Ok(0);
            }
            let mut stream = win_bt::connect_rfcomm(&mac)?;
            write_sync_frame(
                &mut stream,
                &Frame::Hello {
                    device_id: identity.device_id.clone(),
                    device_name: identity.device_name.clone(),
                    os: identity.os.clone(),
                    fingerprint: identity.fingerprint.clone(),
                    p2p_port: Some(DEFAULT_P2P_PORT),
                },
            )?;
            let _ = read_sync_frame(&mut stream)?;
            let mut buf = vec![0u8; CHUNK_SIZE];
            for item in &items {
                let sha256 = hash_file(&item.abs_path)?;
                let transfer_id = Uuid::new_v4().to_string();
                let file_name = item
                    .abs_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file")
                    .to_string();
                write_sync_frame(
                    &mut stream,
                    &Frame::QuickSendOffer {
                        transfer_id: transfer_id.clone(),
                        file_name,
                        rel_path: Some(item.rel_path.clone()),
                        file_size: item.size,
                        sha256,
                    },
                )?;
                match read_sync_frame(&mut stream)? {
                    Some(Frame::QuickSendAccept { accepted: true, .. }) => {}
                    _ => return Err(LierError::Protocol("Bluetooth peer rejected file".into())),
                }
                let mut file = File::open(&item.abs_path)?;
                let mut offset = 0u64;
                loop {
                    let n = file.read(&mut buf)?;
                    if n == 0 {
                        break;
                    }
                    write_sync_frame(
                        &mut stream,
                        &Frame::FileChunk {
                            transfer_id: transfer_id.clone(),
                            offset,
                            data: buf[..n].to_vec(),
                        },
                    )?;
                    offset += n as u64;
                }
                write_sync_frame(&mut stream, &Frame::QuickSendDone { transfer_id })?;
                let _ = read_sync_frame(&mut stream)?;
            }
            Ok(items.len())
        }
        #[cfg(not(windows))]
        {
            let _ = (mac, identity, paths);
            Err(LierError::Protocol("Bluetooth RFCOMM requires Android or Windows".into()))
        }
    })
    .await
    .map_err(|e| LierError::Protocol(e.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_format_bt_mac_roundtrip() {
        let mac = parse_bt_mac("aa:bb:cc:11:22:33").unwrap();
        assert_eq!(format_bt_mac(mac), "AA:BB:CC:11:22:33");
    }
}
