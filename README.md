<div align="center">
  <img src="crates/lier-sync-gui/assets/logo_128.png" width="96" height="96" alt="LierSync Logo" />
  <h1>⚡🔄 LierSync v1.2 — Быстрая P2P-синхронизация файлов и буфера</h1>
  <p><strong>Нативный кроссплатформенный клиент для ПК (Windows / Linux), CLI и Android для передачи файлов, синхронизации папок и общего буфера обмена на Rust</strong></p>

  <p>
    <a href="README.md"><img src="https://img.shields.io/badge/Язык-Русский-8B0000?style=flat-square" alt="Русский" /></a>
    <a href="README_EN.md"><img src="https://img.shields.io/badge/Language-English-39C5BB?style=flat-square" alt="English" /></a>
    <a href="https://github.com/LaRCorp/LierSync/releases"><img src="https://img.shields.io/badge/release-v1.2-39C5BB?style=flat-square" alt="Release v1.2" /></a>
    <img src="https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20Android-8B0000?style=flat-square" alt="Windows | Linux | Android" />
    <img src="https://img.shields.io/badge/Rust-2024%20%7C%20iced%200.14%20%7C%20Jetpack%20Compose-39C5BB?style=flat-square" alt="Rust + iced + Jetpack Compose" />
    <a href="LICENSE"><img src="https://img.shields.io/badge/License-GPLv3-10b981?style=flat-square" alt="GPLv3 License" /></a>
  </p>
</div>

**LierSync** — лёгкое нативное P2P-приложение для мгновенного обмена файлами, инкрементальной синхронизации папок и общего буфера обмена между **ПК и Android** без облака, аккаунтов и WebView.

Единое P2P-ядро написано на **Rust** (`tokio`), десктопный GUI — на **iced** (GPU-рендеринг + системный трей), мобильный клиент — на **Kotlin + Jetpack Compose** через нативный JNI-мост (`libliersync_android.so`). Принятые файлы сохраняются напрямую в папку загрузок (`Downloads\LierSync` на ПК и `Download/LierSync` на Android).

---

## Новое в v1.1

- 🔐 **Безопасность и шифрование:** обмен ключами **X25519** + шифрование трафика **ChaCha20-Poly1305**, сопряжение по **6-значному PIN** и постоянный **белый список доверенных устройств** (`whitelist_only`).
- 📷 **QR-код подключения:** мгновенная генерация QR-кода (`liersync://pair?...`) на ПК и Android + встроенный сканер на `Camera2` + `ZXing` в мобильном приложении.
- 📶 **Передача по Bluetooth (RFCOMM SPP):** прямой обмен файлами и буфером по Bluetooth между Windows (Winsock `AF_BTH`) и Android без Wi-Fi-роутера.
- 🖼️ **Картинки в буфере обмена:** отправка и получение PNG-изображений между буфером обмена ПК (`arboard` + `image`) и Android (`FileProvider`), с SHA-256 анти-эхо защитой.
- 🔔 **Учёт изменений в папках:** отслеживание добавлений, изменений и удалений файлов (`FolderChangeEvent`) с подробными логами и системными уведомлениями на Android (`liersync_alerts`).
- 📤 **Системное «Поделиться»:** приём файлов и текста из меню «Поделиться» на Android (`ShareReceiverActivity` для `ACTION_SEND` / `ACTION_SEND_MULTIPLE`) и запуск отправки через аргумент `--send` или ярлык Windows **SendTo**.

---

## Все возможности

### 📤 Быстрая отправка файлов и папок (Quick Send)
- Отправка произвольного количества файлов и целых каталогов, **Drag-and-Drop** в окно ПК и системная интеграция «Поделиться» / Windows `SendTo`.
- Проверка целостности каждого переданного файла по **SHA-256** и сохранение истории передач с быстрыми кнопками **«Открыть»** и **«Папка»**.
- Автообнаружение устройств в локальной сети по UDP multicast (`239.255.77.88:47788`) и broadcast, ручное подключение по `IP:port`, проверка пингом и сопряжение по QR-коду / PIN.

### 🔄 Синхронизация общих папок (Merkle Diff) и оповещения
- Блочное хеширование **128 КБ** (Merkle Diff) для быстрой дозагрузки только изменившихся частей файлов.
- Режимы для каждой папки: **Общая (двусторонняя)**, **Отправитель (отдавать)** и **Получатель (принимать)**.
- Предпросмотр расхождений (*скачать / отдать / конфликты*), создание безопасных копий при конфликтах (`.sync-conflict-<date>`), мониторинг добавлений/изменений/удалений с оповещениями.
- На Android поддерживаются `Download/LierSync`, каталог приложения и выбор любой папки через **SAF (`OpenDocumentTree`)**.

### 📋 Общий буфер обмена (текст + изображения)
- Двусторонняя фоновая синхронизация текста и **PNG-изображений** буфера обмена с **SHA-256 анти-эхо фильтром**.
- Ручные кнопки **«Отправить»** и **«Получить»**, история операций, на Android — быстрые кнопки прямо в уведомлении без открытия главного окна и плитка **Quick Settings**.

### 🌐 Каналы связи, трей и локализация
- Маршрутизация: `Wi-Fi / LAN` → `Wi-Fi Direct / USB (ADB forward)` → `Bluetooth (RFCOMM SPP)` → `Internet Relay`.
- Сворачивание в **системный трей** на Windows/Linux: при закрытии окна LierSync продолжает приём файлов, автосинхронизацию папок и буфера в фоне.
- Переключение языка **Русский / English** в разделе **Настройки / Settings** с полным сохранением конфигурации, доверенных узлов, папок и истории.

---

## CLI-утилита (`lier-sync-cli`)

```bash
# Запуск фонового узла приёма и раздачи
cargo run -p lier-sync-cli -- serve --port 47789 --out ./LierSync_Inbox --share default=./Shared

# Поиск устройств в локальной сети
cargo run -p lier-sync-cli -- discover --seconds 5

# Прямая отправка файлов или папок (Quick Send)
cargo run -p lier-sync-cli -- send "C:\Docs\archive.zip" "C:\Photos" --to 192.168.1.50:47789

# Сравнение и синхронизация общей папки (Merkle)
cargo run -p lier-sync-cli -- compare --from 192.168.1.50:47789 --folder-id default --dir ./Shared
cargo run -p lier-sync-cli -- sync --from 192.168.1.50:47789 --folder-id default --dir ./Shared

# Отправка текущего буфера обмена
cargo run -p lier-sync-cli -- clipboard --to 192.168.1.50:47789
```

---

## Архитектура репозитория

Репозиторий объединяет общее ядро на Rust, десктопное приложение, CLI и мобильный клиент для Android:

```text
LierSync/
├── crates/
│   ├── lier-sync-core/             # Общее кроссплатформенное P2P-ядро
│   │   └── src/
│   │       ├── bluetooth.rs        # Bluetooth RFCOMM SPP (Winsock AF_BTH)
│   │       ├── channel.rs          # Транспортные каналы и политики маршрутизации
│   │       ├── clipboard.rs        # Буфер обмена (текст + PNG) и SHA-256 анти-эхо
│   │       ├── config.rs           # Конфигурация AppConfig, TrustedPeer, история, папки
│   │       ├── discovery.rs        # UDP multicast/broadcast обнаружение пиров в LAN
│   │       ├── identity.rs         # Идентификатор узла и путь Downloads\LierSync
│   │       ├── merkle.rs           # Блочное хеширование 128 КБ и расчёт diff
│   │       ├── protocol.rs         # Бинарный протокол сообщений, шифрования и сопряжения
│   │       ├── security.rs         # X25519 + ChaCha20-Poly1305, PIN-сопряжение, QR URI/PNG
│   │       ├── transport.rs        # TCP P2P-сервер, Quick Send, буфер и Merkle-синхронизация
│   │       └── watcher.rs          # Наблюдатель изменений файлов (добавлено/изменено/удалено)
│   ├── lier-sync-cli/              # Консольный клиент и headless-сервер
│   ├── lier-sync-gui/              # Нативное десктоп-приложение (iced + трей + i18n RU/EN)
│   └── lier-sync-android/          # Мост JNI для Android-приложения
├── mobile/                         # Android-клиент (Kotlin + Jetpack Compose)
│   ├── app/src/main/
│   │   ├── java/org/larevole/liersync/
│   │   │   ├── nativebridge/       # LierSyncNative.kt (JNI-вызовы в Rust)
│   │   │   ├── service/            # LierSyncService, BluetoothSyncManager, TileService
│   │   │   └── ui/                 # MainActivity, QrScannerActivity, ShareReceiverActivity, ClipboardSyncActivity
│   │   └── AndroidManifest.xml
│   ├── build_rust_android.ps1      # Сборка .so библиотек через cargo-ndk
│   └── build.gradle.kts
├── Cargo.toml
├── LICENSE                         # GNU GPLv3
├── README.md                       # Русская документация
└── README_EN.md                    # English documentation
```

---

## Стек технологий

| Компонент | Технология |
| --- | --- |
| P2P-ядро и CLI | Rust 2024 (`tokio`, `socket2`, `serde`, `walkdir`) |
| Криптография и QR | `x25519-dalek`, `chacha20poly1305`, `sha2`, `qrcode` |
| Десктопный GUI | `iced` 0.14, `tray-icon` 0.25, `rfd` 0.17, `arboard` 3.7, `image` |
| Android-клиент | Kotlin 2.0, Jetpack Compose (Material 3), `Camera2`, `com.google.zxing:core`, JNI |
| Bluetooth | Windows Winsock (`AF_BTH` / `BTHPROTO_RFCOMM`) ↔ Android `BluetoothSocket` (SPP) |

---

## Сборка из исходников

### ПК-приложение и CLI (Windows / Linux)
Требуется **Rust stable** (edition 2024):

```bash
git clone https://github.com/LaRCorp/LierSync.git
cd LierSync

# Запуск тестов ядра и воркспейса
cargo test --workspace

# Запуск GUI для разработки
cargo run -p lier-sync-gui

# Оптимизированная release-сборка (LTO + strip)
cargo build --release -p lier-sync-gui -p lier-sync-cli
```

Готовые бинарники появятся в `target/release/lier-sync-gui.exe` и `target/release/lier-sync-cli.exe` (на Linux — без `.exe`).

### Android APK (`mobile/`)
Требуются **Android SDK (API 35)**, **Android NDK**, **JDK 17/21**, **Gradle 8.11+** и **`cargo-ndk`**:

```powershell
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk

cd mobile
.\build_rust_android.ps1
gradle assembleDebug
```

Готовый APK появится в `mobile/app/build/outputs/apk/debug/app-debug.apk`.

---

## Расположение данных

- **Принятые файлы (Windows):** `%USERPROFILE%\Downloads\LierSync\`
- **Принятые файлы (Android):** `Download/LierSync`
- **Конфигурация, доверенные узлы и история (Windows):** `%APPDATA%\LaRevole\LierSync\config.json`

---

## Лицензия

Проект распространяется под свободной лицензией **GNU General Public License v3.0 (GPLv3)** — см. [LICENSE](LICENSE).

Copyright (c) 2026 LaRevole.

---

### Связанные проекты экосистемы
- 🎵🏷️ **[LaRCorp/LierTags](https://github.com/LaRCorp/LierTags)** — быстрый локальный редактор аудиотегов и обложек на Rust и Tauri 2.
- 🌐 **[LaRCorp/LierEX-Q](https://github.com/LaRCorp/LierEX-Q)** — браузерное расширение для извлечения метаданных в 1 клик.
- 🎮🎬 **[LaRCorp/Lierkour](https://github.com/LaRCorp/Lierkour)** — локальный кроссплатформенный каталогизатор фильмов, сериалов, аниме и видеоигр.
