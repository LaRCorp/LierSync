// LierSync — Copyright (C) 2026 LaRevole (LaRCorp) — GPL-3.0-only

use anyhow::Result;
use clap::{Parser, Subcommand};
use lier_sync_core::{
    ClipboardGuard, CoreEvent, DEFAULT_P2P_PORT, DeviceIdentity, DiscoveryService,
    SharedFolderRegistry, compare_folder_with_peer, get_clipboard_text, send_clipboard_text,
    send_quick_batch, start_p2p_server, sync_folder_from_peer,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc;

#[derive(Parser)]
#[command(
    name = "lier-sync-cli",
    version,
    about = "LierSync CLI — P2P file & clipboard sync"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Listen for incoming Quick Send files, Sync requests, and Clipboard pushes
    Serve {
        #[arg(short, long, default_value_t = DEFAULT_P2P_PORT)]
        port: u16,
        #[arg(short, long)]
        out: Option<PathBuf>,
        /// Optional shared folder mapping in the form id=path
        #[arg(long)]
        share: Vec<String>,
    },
    /// Discover LierSync devices on the local network
    Discover {
        #[arg(short, long, default_value_t = 5)]
        seconds: u64,
    },
    /// Send one or more files/folders directly to a peer (Quick Send mode)
    Send {
        #[arg(required = true)]
        files: Vec<PathBuf>,
        #[arg(long)]
        to: SocketAddr,
    },
    /// Compare local folder against a remote peer's shared folder
    Compare {
        #[arg(long)]
        from: SocketAddr,
        #[arg(long)]
        folder_id: String,
        #[arg(long)]
        dir: PathBuf,
    },
    /// Sync a local folder from a remote peer's shared folder ID (Merkle mode)
    Sync {
        #[arg(long)]
        from: SocketAddr,
        #[arg(long)]
        folder_id: String,
        #[arg(long)]
        dir: PathBuf,
    },
    /// Push current clipboard text to a peer
    Clipboard {
        #[arg(long)]
        to: SocketAddr,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let identity = DeviceIdentity::load_or_create()?;

    match cli.command {
        Commands::Serve { port, out, share } => {
            let out = out.unwrap_or_else(DeviceIdentity::default_download_dir);
            let folders = SharedFolderRegistry::new();
            for item in share {
                if let Some((id, path)) = item.split_once('=') {
                    folders.register(id.to_string(), PathBuf::from(path)).await;
                    println!("Shared folder '{id}' -> {path}");
                }
            }

            let (tx, mut rx) = mpsc::channel::<CoreEvent>(64);
            let clipboard_guard = ClipboardGuard::new(32);
            let discovery = DiscoveryService::new();
            let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
            let bound = start_p2p_server(
                bind_addr,
                identity.clone(),
                out.clone(),
                folders,
                clipboard_guard,
                discovery.clone(),
                tx,
            )
            .await?;

            let _disc_handle = discovery.start(identity.clone(), bound.port());

            println!(
                "LierSync Serve running as '{}' ({}) on {} | Inbox: {}",
                identity.device_name,
                identity.fingerprint,
                bound,
                out.display()
            );

            while let Some(ev) = rx.recv().await {
                match ev {
                    CoreEvent::IncomingFile {
                        from_peer,
                        file_name,
                        saved_path,
                        bytes,
                        ..
                    } => {
                        println!(
                            "[QuickSend] Received '{file_name}' ({bytes} B) from {from_peer} -> {}",
                            saved_path.display()
                        );
                    }
                    CoreEvent::ClipboardReceived {
                        from_peer, preview, ..
                    } => {
                        println!("[Clipboard] From {from_peer}: {preview}");
                    }
                    CoreEvent::ClipboardImageReceived {
                        from_peer,
                        width,
                        height,
                        png_bytes,
                        ..
                    } => {
                        println!(
                            "[ClipboardImage] From {from_peer}: {width}x{height} ({} bytes)",
                            png_bytes.len()
                        );
                    }
                    CoreEvent::PeerPaired(peer) => {
                        println!(
                            "[Security] Paired with {} ({})",
                            peer.device_name, peer.fingerprint
                        );
                    }
                    CoreEvent::SyncActivity { folder_id, message } => {
                        println!("[Sync:{folder_id}] {message}");
                    }
                    CoreEvent::Error(err) => {
                        eprintln!("[Error] {err}");
                    }
                }
            }
        }
        Commands::Discover { seconds } => {
            let discovery = DiscoveryService::new();
            let _handle = discovery.start(identity, DEFAULT_P2P_PORT);
            println!("Scanning LAN for LierSync peers ({seconds}s)...");
            tokio::time::sleep(Duration::from_secs(seconds)).await;
            let peers = discovery.peers().await;
            if peers.is_empty() {
                println!("No peers discovered.");
            } else {
                for p in peers {
                    println!(
                        "• {} ({}) — {} [{}]",
                        p.device_name, p.os, p.addr, p.device_id
                    );
                }
            }
        }
        Commands::Send { files, to } => {
            println!("Sending {} item(s) to {to}...", files.len());
            let count = send_quick_batch(
                to,
                &identity,
                &files,
                |idx, total_files, sent, total, name| {
                    let pct = if total > 0 { (sent * 100) / total } else { 100 };
                    print!("\r[{idx}/{total_files}] {name} — {sent}/{total} bytes ({pct}%)   ");
                },
            )
            .await?;
            println!("\nDone! Sent {count} file(s).");
        }
        Commands::Compare {
            from,
            folder_id,
            dir,
        } => {
            let preview = compare_folder_with_peer(from, &identity, &folder_id, &dir).await?;
            println!("Compare '{folder_id}' ({from} <-> {}):", dir.display());
            println!("  To download : {}", preview.to_download.len());
            println!("  To upload   : {}", preview.to_upload.len());
            println!("  Conflicts   : {}", preview.conflicts.len());
        }
        Commands::Sync {
            from,
            folder_id,
            dir,
        } => {
            println!("Syncing '{folder_id}' from {from} -> {}...", dir.display());
            let chunks = sync_folder_from_peer(from, &identity, &folder_id, &dir).await?;
            println!("Sync complete. Updated {chunks} chunk(s).");
        }
        Commands::Clipboard { to } => {
            let text = get_clipboard_text()?;
            send_clipboard_text(to, &identity, &text).await?;
            println!("Clipboard sent ({} chars) to {to}.", text.len());
        }
    }

    Ok(())
}
