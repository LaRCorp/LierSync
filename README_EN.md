<div align="center">
  <img src="crates/lier-sync-gui/assets/logo_128.png" width="96" height="96" alt="LierSync Logo" />
  <h1>⚡🔄 LierSync v1.2 — Fast P2P File & Clipboard Sync</h1>
  <p><strong>Native cross-platform Desktop (Windows / Linux), CLI, and Android utility for fast file transfers, folder synchronization, and shared clipboard powered by Rust</strong></p>

  <p>
    <a href="README.md"><img src="https://img.shields.io/badge/Язык-Русский-8B0000?style=flat-square" alt="Русский" /></a>
    <a href="README_EN.md"><img src="https://img.shields.io/badge/Language-English-39C5BB?style=flat-square" alt="English" /></a>
    <a href="https://github.com/LaRCorp/LierSync/releases"><img src="https://img.shields.io/badge/release-v1.2-39C5BB?style=flat-square" alt="Release v1.2" /></a>
    <img src="https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20Android-8B0000?style=flat-square" alt="Windows | Linux | Android" />
    <img src="https://img.shields.io/badge/Rust-2024%20%7C%20iced%200.14%20%7C%20Jetpack%20Compose-39C5BB?style=flat-square" alt="Rust + iced + Jetpack Compose" />
    <a href="LICENSE"><img src="https://img.shields.io/badge/License-GPLv3-10b981?style=flat-square" alt="GPLv3 License" /></a>
  </p>
</div>

**LierSync** is a lightweight native peer-to-peer utility for instant file transfers, incremental folder synchronization, and shared clipboard between **PC and Android** — with zero cloud accounts and no WebView overhead.

Its shared P2P core is written in **Rust** (`tokio`), the desktop GUI runs on **iced** (GPU-accelerated + system tray), and the Android client is built with **Kotlin + Jetpack Compose** over a native JNI bridge (`libliersync_android.so`). Received files are saved directly into your Downloads directory (`Downloads\LierSync` on PC, `Download/LierSync` on Android).

---

## What's New in v1.1

- 🔐 **Security & Encryption:** **X25519** key exchange + **ChaCha20-Poly1305** authenticated wire encryption, **6-digit PIN** pairing, and a persistent **trusted peer whitelist** (`whitelist_only`).
- 📷 **QR Code Pairing:** Instant `liersync://pair?...` QR code generation on both PC and Android, plus an integrated `Camera2` + `ZXing` QR scanner on Android.
- 📶 **Bluetooth RFCOMM SPP:** Direct file & clipboard transfer over Bluetooth between Windows (Winsock `AF_BTH`) and Android without needing a Wi-Fi router.
- 🖼️ **Images in Clipboard:** Send and pull PNG clipboard images between PC (`arboard` + `image`) and Android (`FileProvider`) with SHA-256 anti-echo protection.
- 🔔 **Folder Change Alerts:** Real-time tracking of added, modified, and removed files (`FolderChangeEvent`) with detailed logs and Android notifications (`liersync_alerts`).
- 📤 **System Share Integration:** Receive files and text directly from Android's system Share sheet (`ShareReceiverActivity` for `ACTION_SEND` / `ACTION_SEND_MULTIPLE`), plus `--send` CLI launch arguments and a Windows **SendTo** shortcut installer on PC.

---

## Core Features

### 📤 Quick Send (Files & Folders)
- Queue any number of files and nested directories, **Drag & Drop** into the desktop window, or trigger transfers straight from Android Share / Windows `SendTo`.
- **SHA-256** integrity verification on every transfer and persistent transfer history with **Open** and **Folder** actions.
- Automatic LAN peer discovery via UDP multicast (`239.255.77.88:47788`) with broadcast fallback, manual `IP:port` entry with ping checks, and QR/PIN pairing.

### 🔄 Folder Synchronization (Merkle Diff) & Change Tracking
- Fixed **128 KiB** Merkle block hashing for fast diff comparison and delta syncing of only modified chunks.
- Per-folder direction modes: **Bidirectional (Shared)**, **Send Only**, and **Receive Only**.
- Diff preview (*download / upload / conflicts*), conflict preservation (`.sync-conflict-<date>`), and live change alerts when files are added, edited, or deleted.
- Android supports `Download/LierSync`, app storage, and arbitrary custom folders via **SAF (`OpenDocumentTree`)**.

### 📋 Shared Clipboard (Text + Images)
- Background two-way sync for both **text and PNG images** guarded by a **SHA-256 anti-echo cache**.
- Manual **Send** and **Pull** buttons, clipboard history, Android notification quick actions (works without opening the app), and a **Quick Settings** tile.

### 🌐 Channels, Background Tray & Localization
- Routing pipeline: `Wi-Fi / LAN` → `Wi-Fi Direct / USB (ADB forward)` → `Bluetooth (RFCOMM SPP)` → `Internet Relay`.
- **System tray** integration on Windows/Linux: closing the window keeps incoming transfers, folder sync, and clipboard sync active in the background.
- Instant **Russian / English** language switching in **Settings** with full persistence for preferences, trusted peers, shared folders, and transfer history.

---

## CLI Tool (`lier-sync-cli`)

```bash
# Start a headless background node
cargo run -p lier-sync-cli -- serve --port 47789 --out ./LierSync_Inbox --share default=./Shared

# Discover peers on the local network
cargo run -p lier-sync-cli -- discover --seconds 5

# Direct file/folder Quick Send
cargo run -p lier-sync-cli -- send "C:\Docs\archive.zip" "C:\Photos" --to 192.168.1.50:47789

# Compare and synchronize a shared folder (Merkle)
cargo run -p lier-sync-cli -- compare --from 192.168.1.50:47789 --folder-id default --dir ./Shared
cargo run -p lier-sync-cli -- sync --from 192.168.1.50:47789 --folder-id default --dir ./Shared

# Push current clipboard contents
cargo run -p lier-sync-cli -- clipboard --to 192.168.1.50:47789
```

---

## Repository Structure

One unified repository holds the shared Rust core, desktop app, CLI, and Android client:

```text
LierSync/
├── crates/
│   ├── lier-sync-core/             # Shared cross-platform P2P core
│   │   └── src/
│   │       ├── bluetooth.rs        # Bluetooth RFCOMM SPP (Winsock AF_BTH)
│   │       ├── channel.rs          # Transport channels & routing policies
│   │       ├── clipboard.rs        # Clipboard (text + PNG) & SHA-256 anti-echo guard
│   │       ├── config.rs           # AppConfig, TrustedPeer, transfer history, folders
│   │       ├── discovery.rs        # UDP multicast/broadcast LAN peer discovery
│   │       ├── identity.rs         # Node identity & Downloads\LierSync target path
│   │       ├── merkle.rs           # 128 KiB block hashing & diff computation
│   │       ├── protocol.rs         # Binary wire protocol, encryption & pairing frames
│   │       ├── security.rs         # X25519 + ChaCha20-Poly1305, PIN pairing, QR URI/PNG
│   │       ├── transport.rs        # TCP P2P server, Quick Send, clipboard & Merkle sync
│   │       └── watcher.rs          # Folder watcher with add/edit/delete change events
│   ├── lier-sync-cli/              # Console client & headless node
│   ├── lier-sync-gui/              # Native desktop GUI (iced + system tray + RU/EN i18n)
│   └── lier-sync-android/          # JNI bridge for the Android client
├── mobile/                         # Android client (Kotlin + Jetpack Compose)
│   ├── app/src/main/
│   │   ├── java/org/larevole/liersync/
│   │   │   ├── nativebridge/       # LierSyncNative.kt (JNI bindings to Rust)
│   │   │   ├── service/            # LierSyncService, BluetoothSyncManager, TileService
│   │   │   └── ui/                 # MainActivity, QrScannerActivity, ShareReceiverActivity, ClipboardSyncActivity
│   │   └── AndroidManifest.xml
│   ├── build_rust_android.ps1      # cargo-ndk build script for .so libraries
│   └── build.gradle.kts
├── Cargo.toml
├── LICENSE                         # GNU GPLv3
├── README.md                       # Russian documentation
└── README_EN.md                    # English documentation
```

---

## Tech Stack

| Component | Technology |
| --- | --- |
| P2P Core & CLI | Rust 2024 (`tokio`, `socket2`, `serde`, `walkdir`) |
| Crypto & QR | `x25519-dalek`, `chacha20poly1305`, `sha2`, `qrcode` |
| Desktop GUI | `iced` 0.14, `tray-icon` 0.25, `rfd` 0.17, `arboard` 3.7, `image` |
| Android Client | Kotlin 2.0, Jetpack Compose (Material 3), `Camera2`, `com.google.zxing:core`, JNI |
| Bluetooth | Windows Winsock (`AF_BTH` / `BTHPROTO_RFCOMM`) ↔ Android `BluetoothSocket` (SPP) |

---

## Building from Source

### Desktop & CLI (Windows / Linux)
Requires **Rust stable** (edition 2024):

```bash
git clone https://github.com/LaRCorp/LierSync.git
cd LierSync

# Run workspace unit tests
cargo test --workspace

# Run desktop GUI
cargo run -p lier-sync-gui

# Build optimized release binaries (LTO + strip)
cargo build --release -p lier-sync-gui -p lier-sync-cli
```

Release binaries will be output to `target/release/lier-sync-gui.exe` and `target/release/lier-sync-cli.exe` (without `.exe` on Linux).

### Android APK (`mobile/`)
Requires **Android SDK (API 35)**, **Android NDK**, **JDK 17/21**, **Gradle 8.11+**, and **`cargo-ndk`**:

```powershell
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk

cd mobile
.\build_rust_android.ps1
gradle assembleDebug
```

The APK will be generated at `mobile/app/build/outputs/apk/debug/app-debug.apk`.

---

## Data Locations

- **Received Files (Windows):** `%USERPROFILE%\Downloads\LierSync\`
- **Received Files (Android):** `Download/LierSync`
- **Config, Trusted Peers & History (Windows):** `%APPDATA%\LaRevole\LierSync\config.json`

---

## License

Released under the **GNU General Public License v3.0 (GPLv3)** — see [LICENSE](LICENSE).

Copyright (c) 2026 LaRevole.

---

### Related Ecosystem Projects
- 🎵🏷️ **[LaRCorp/LierTags](https://github.com/LaRCorp/LierTags)** — fast local audio tag & cover art editor built with Rust and Tauri 2.
- 🌐 **[LaRCorp/LierEX-Q](https://github.com/LaRCorp/LierEX-Q)** — 1-click browser metadata extractor extension.
- 🎮🎬 **[LaRCorp/Lierkour](https://github.com/LaRCorp/Lierkour)** — local cross-platform catalog for movies, series, anime, and video games.
