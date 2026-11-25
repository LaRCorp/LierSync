// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

use crate::identity::DeviceIdentity;
use crate::transport::ping_peer;
use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::RwLock;

pub const DISCOVERY_PORT: u16 = 47788;
pub const MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(239, 255, 77, 88);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerAnnouncement {
    pub device_id: String,
    pub device_name: String,
    pub os: String,
    pub p2p_port: u16,
}

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub device_id: String,
    pub device_name: String,
    pub os: String,
    pub addr: SocketAddr,
    pub last_seen: Instant,
}

#[derive(Clone)]
pub struct DiscoveryService {
    peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
    extra_ips: Arc<RwLock<Vec<Ipv4Addr>>>,
}

impl DiscoveryService {
    pub fn new() -> Self {
        Self {
            peers: Arc::new(RwLock::new(HashMap::new())),
            extra_ips: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn set_extra_ips(&self, ips: Vec<Ipv4Addr>) {
        *self.extra_ips.write().await = ips;
    }

    pub async fn register_peer(&self, peer: PeerInfo) {
        let mut map = self.peers.write().await;
        map.insert(peer.device_id.clone(), peer);
    }

    pub async fn peers(&self) -> Vec<PeerInfo> {
        let mut map = self.peers.write().await;
        let now = Instant::now();
        map.retain(|_, p| now.duration_since(p.last_seen) < Duration::from_secs(20));
        let mut list: Vec<PeerInfo> = map.values().cloned().collect();
        list.sort_by(|a, b| a.device_name.cmp(&b.device_name));
        list
    }

    /// Spawn background UDP multicast/broadcast/unicast-reply announcer, listener, and hotspot gateway probe.
    pub fn start(&self, identity: DeviceIdentity, p2p_port: u16) -> tokio::task::JoinHandle<()> {
        let peers = Arc::clone(&self.peers);
        let extra_ips_ref = Arc::clone(&self.extra_ips);
        let svc_for_probe = self.clone();
        tokio::spawn(async move {
            let announcement = PeerAnnouncement {
                device_id: identity.device_id.clone(),
                device_name: identity.device_name.clone(),
                os: identity.os.clone(),
                p2p_port,
            };
            let payload = serde_json::to_vec(&announcement).unwrap_or_default();

            // Active TCP gateway (.1) probe for Wi-Fi Hotspot / Router scenarios (energy-friendly cadence)
            let probe_id = identity.clone();
            let probe_svc = svc_for_probe.clone();
            tokio::spawn(async move {
                loop {
                    for ip in detect_local_ipv4s() {
                        let o = ip.octets();
                        let gw_ip = Ipv4Addr::new(o[0], o[1], o[2], 1);
                        if gw_ip != ip {
                            let gw_id = format!("gw-{gw_ip}");
                            let already_fresh = probe_svc
                                .peers
                                .read()
                                .await
                                .get(&gw_id)
                                .map(|p| p.last_seen.elapsed() < Duration::from_secs(12))
                                .unwrap_or(false);
                            if !already_fresh {
                                let gw_addr = SocketAddr::V4(SocketAddrV4::new(
                                    gw_ip,
                                    crate::DEFAULT_P2P_PORT,
                                ));
                                if let Ok((name, os, _ms)) = ping_peer(gw_addr, &probe_id).await {
                                    probe_svc
                                        .register_peer(PeerInfo {
                                            device_id: gw_id,
                                            device_name: name,
                                            os,
                                            addr: gw_addr,
                                            last_seen: Instant::now(),
                                        })
                                        .await;
                                }
                            }
                        }
                    }
                    tokio::time::sleep(Duration::from_secs(6)).await;
                }
            });

            let Ok(socket) = bind_discovery_socket() else {
                tracing::warn!("Failed to bind discovery UDP socket on port {DISCOVERY_PORT}");
                return;
            };
            let socket = Arc::new(socket);
            let recv_socket = Arc::clone(&socket);
            let reply_socket = Arc::clone(&socket);
            let reply_payload = payload.clone();
            let own_id = identity.device_id.clone();

            // Listener task (replies directly via unicast UDP + TCP Hello so stateful firewalls allow it back)
            let peers_recv = Arc::clone(&peers);
            let tcp_reply_id = identity.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 2048];
                let mut last_reply: HashMap<String, Instant> = HashMap::new();
                loop {
                    match recv_socket.recv_from(&mut buf).await {
                        Ok((len, src)) => {
                            if let Ok(ann) = serde_json::from_slice::<PeerAnnouncement>(&buf[..len])
                            {
                                if ann.device_id == own_id {
                                    continue;
                                }
                                let p2p_addr = SocketAddr::new(src.ip(), ann.p2p_port);
                                {
                                    let mut map = peers_recv.write().await;
                                    map.insert(
                                        ann.device_id.clone(),
                                        PeerInfo {
                                            device_id: ann.device_id.clone(),
                                            device_name: ann.device_name,
                                            os: ann.os,
                                            addr: p2p_addr,
                                            last_seen: Instant::now(),
                                        },
                                    );
                                }
                                let should_reply = last_reply
                                    .get(&ann.device_id)
                                    .map(|t| t.elapsed() >= Duration::from_secs(3))
                                    .unwrap_or(true);
                                if should_reply {
                                    last_reply.insert(ann.device_id, Instant::now());
                                    let reply_addr = SocketAddr::new(src.ip(), DISCOVERY_PORT);
                                    let _ = reply_socket.send_to(&reply_payload, reply_addr).await;
                                    let _ = reply_socket.send_to(&reply_payload, src).await;
                                    let id_clone = tcp_reply_id.clone();
                                    tokio::spawn(async move {
                                        let _ = ping_peer(p2p_addr, &id_clone).await;
                                    });
                                }
                            }
                        }
                        Err(e) => {
                            tracing::debug!("Discovery recv error: {e}");
                            tokio::time::sleep(Duration::from_millis(250)).await;
                        }
                    }
                }
            });

            // Announcer loop
            let multicast_dest = SocketAddr::V4(SocketAddrV4::new(MULTICAST_ADDR, DISCOVERY_PORT));
            let broadcast_dest =
                SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::BROADCAST, DISCOVERY_PORT));

            loop {
                let _ = socket.send_to(&payload, multicast_dest).await;
                let _ = socket.send_to(&payload, broadcast_dest).await;
                let mut all_ips = detect_local_ipv4s();
                for extra in extra_ips_ref.read().await.iter() {
                    if !all_ips.contains(extra) {
                        all_ips.push(*extra);
                    }
                }
                for ip in all_ips {
                    let octets = ip.octets();
                    let subnet_bcast = SocketAddr::V4(SocketAddrV4::new(
                        Ipv4Addr::new(octets[0], octets[1], octets[2], 255),
                        DISCOVERY_PORT,
                    ));
                    let _ = socket.send_to(&payload, subnet_bcast).await;
                    let gw_unicast = SocketAddr::V4(SocketAddrV4::new(
                        Ipv4Addr::new(octets[0], octets[1], octets[2], 1),
                        DISCOVERY_PORT,
                    ));
                    if gw_unicast != SocketAddr::V4(SocketAddrV4::new(ip, DISCOVERY_PORT)) {
                        let _ = socket.send_to(&payload, gw_unicast).await;
                    }
                }
                tokio::time::sleep(Duration::from_secs(4)).await;
            }
        })
    }
}

impl Default for DiscoveryService {
    fn default() -> Self {
        Self::new()
    }
}

/// Detect the primary local LAN/Hotspot IPv4 address(es) of this machine.
pub fn detect_local_ipv4s() -> Vec<Ipv4Addr> {
    let mut ips = Vec::new();
    for probe in ["192.168.43.1:80", "192.168.1.1:80", "8.8.8.8:80"] {
        if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if sock.connect(probe).is_ok() {
                if let Ok(SocketAddr::V4(v4)) = sock.local_addr() {
                    let ip = *v4.ip();
                    if !ip.is_loopback() && !ip.is_unspecified() && !ips.contains(&ip) {
                        ips.push(ip);
                    }
                }
            }
        }
    }
    ips
}

fn bind_discovery_socket() -> std::io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    socket.set_broadcast(true)?;
    let addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT);
    socket.bind(&addr.into())?;
    let _ = socket.join_multicast_v4(&MULTICAST_ADDR, &Ipv4Addr::UNSPECIFIED);
    socket.set_nonblocking(true)?;
    UdpSocket::from_std(socket.into())
}
