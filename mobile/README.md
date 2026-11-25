# LierSync Mobile (Android) — v1.1

**[🇷🇺 Русский README](../README.md) | [🇺🇸 English README](../README_EN.md)**

Android client (`org.larevole.liersync`) built with **Kotlin + Jetpack Compose** and backed by the shared Rust P2P engine (`crates/lier-sync-android` + `crates/lier-sync-core` via JNI).

## Features (v1.1)
- **Quick Send & System Share:** Multi-file selection, `ShareReceiverActivity` (`ACTION_SEND` / `ACTION_SEND_MULTIPLE`), SHA-256 verification, and persistent transfer history (`Download/LierSync`).
- **Folder Sync (Merkle Diff) & Change Alerts:** 128 KiB block-level diff, bidirectional / send-only / receive-only modes, SAF custom folder selection (`OpenDocumentTree`), and real-time folder change notifications (`liersync_alerts`).
- **Clipboard Text & Images:** Background text and PNG image sync via `FileProvider` with SHA-256 anti-echo guard, notification quick actions (**Send** / **Pull**), and Quick Settings tile.
- **Security & QR Pairing:** `X25519` + `ChaCha20-Poly1305` encrypted sessions, 6-digit PIN pairing, trusted peer whitelist, QR code generation, and built-in `Camera2` + ZXing QR scanner.
- **Transport Channels:** Wi-Fi / LAN (`MulticastLock`), Wi-Fi Direct / USB (`adb forward`), Bluetooth RFCOMM SPP (`BluetoothSyncManager`), and Internet Relay.

## Build Native Core & APK
```powershell
# From the mobile/ directory
.\build_rust_android.ps1
gradle assembleDebug
```
> **Note:** Open the `mobile/` folder in **Android Studio** or run `gradle assembleDebug` with JDK 17/21 and Android SDK 35 (`ANDROID_HOME` configured).

Licensed under **GNU GPLv3** — Copyright (C) 2026 LaRevole.
