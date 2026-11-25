<div align="center">
  <img src="crates/lier-sync-gui/assets/logo_128.png" width="96" height="96" alt="LierSync Logo" />
  <h1>⚡🔄 LierSync v1.0 — Быстрая P2P-синхронизация файлов и буфера</h1>
  <p><strong>Нативный кроссплатформенный клиент и CLI для передачи файлов, синхронизации папок и общего буфера обмена на Rust и iced</strong></p>

  <p>
    <a href="https://github.com/LaRCorp/LierSync/releases"><img src="https://img.shields.io/badge/release-v1.0-39C5BB?style=flat-square" alt="Release v1.0" /></a>
    <img src="https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20Android-8B0000?style=flat-square" alt="Windows | Linux | Android" />
    <img src="https://img.shields.io/badge/Rust-2024%20%7C%20iced%200.14%20%7C%20tokio-39C5BB?style=flat-square" alt="Rust 2024 + iced + tokio" />
    <a href="LICENSE"><img src="https://img.shields.io/badge/License-GPLv3-10b981?style=flat-square" alt="GPLv3 License" /></a>
    <a href="https://github.com/LaRCorp/LierTags"><img src="https://img.shields.io/badge/Companion-LierTags-6366f1?style=flat-square" alt="LierTags" /></a>
  </p>
</div>

**LierSync** — лёгкое нативное P2P-приложение и CLI-инструмент для мгновенного обмена файлами, инкрементальной синхронизации папок и общего буфера обмена между ПК и Android без облака и WebView. Написано на **Rust** с GPU-ускоренным интерфейсом на **iced**, асинхронным сетевым движком на **tokio** и нативным JNI-мостом для Android (Kotlin + Jetpack Compose).

Приложение работает полностью локально в сети **Wi-Fi / LAN**, через **точку доступа / Wi-Fi Direct / USB (ADB forward)** без роутера или через настраиваемый **Relay**, не требует регистрации аккаунтов и сохраняет принятые файлы напрямую в системную папку загрузок (`Downloads\LierSync` на ПК, `Download/LierSync` на Android).

---

## Возможности

### 📤 Быстрая отправка файлов и папок (Quick Send)
- Отправка произвольного количества файлов и целых каталогов в очереди, включая **Drag-and-Drop** прямо в окно приложения.
- Проверка целостности каждого переданного файла по **SHA-256** и сохранение истории передач с кнопками **«Открыть»** и **«Папка»**.
- Автоматическое обнаружение устройств в локальной сети через UDP multicast (`239.255.77.88:47788`) с broadcast-фолбэком и возможностью ручного ввода `IP:port` + проверки пингом.

### 🔄 Синхронизация общих папок (Merkle Diff)
- Построение деревьев Меркла с блоками фиксированного размера (**128 КБ**) для быстрого сравнения и дозагрузки только изменившихся частей файлов.
- Поддержка режимов синхронизации для каждой папки: **Общая (двусторонняя)**, **Отправитель (отдавать)** и **Получатель (принимать)**.
- Предпросмотр расхождений перед синхронизацией (*скачать / отдать / конфликты*), непрерывное отслеживание изменений в реальном времени и создание безопасных копий при конфликтах (`.sync-conflict-<date>`).
- На Android поддерживается выбор `Download/LierSync`, папки приложения или любой пользовательской папки через `OpenDocumentTree` (прямой путь или SAF-зеркалирование).

### 📋 Общий буфер обмена
- Фоновая двусторонняя синхронизация текста буфера обмена между устройствами с **SHA-256 анти-эхо защитой**, предотвращающей зацикливание.
- Ручная отправка и получение буфера одной кнопкой, история операций, на Android — управление прямо из шторки уведомлений без открытия главного окна и быстрая плитка Quick Settings.

### 🌐 Каналы связи и фоновая работа
- Приоритетная маршрутизация: `Wi-Fi / LAN` -> `Wi-Fi Direct / USB` -> `Bluetooth (резерв)` -> `Internet Relay`.
- Интеграция с **системным треем** на десктопе: при закрытии окна LierSync продолжает приём файлов, автосинхронизацию папок и буфера в фоне.
- Встроенное переключение языка интерфейса **Русский / English** в разделе **Настройки / Settings** с сохранением конфигурации, истории и подключённых папок.

---

## CLI-утилита (`lier-sync-cli`)

Для серверов, скриптов и терминальной работы проект включает консольную утилиту `lier-sync-cli`:

### 1. Запуск фонового узла приёма и раздачи
```bash
cargo run -p lier-sync-cli -- serve --port 47789 --out ./LierSync_Inbox --share default=./Shared
```

### 2. Поиск устройств в локальной сети
```bash
cargo run -p lier-sync-cli -- discover --seconds 5
```

### 3. Прямая отправка файлов или папок (Quick Send)
```bash
cargo run -p lier-sync-cli -- send "C:\Docs\archive.zip" "C:\Photos" --to 192.168.1.50:47789
```

### 4. Сравнение и синхронизация общей папки (Merkle)
```bash
# Предпросмотр изменений
cargo run -p lier-sync-cli -- compare --from 192.168.1.50:47789 --folder-id default --dir ./Shared

# Синхронизация
cargo run -p lier-sync-cli -- sync --from 192.168.1.50:47789 --folder-id default --dir ./Shared
```

### 5. Отправка текущего буфера обмена
```bash
cargo run -p lier-sync-cli -- clipboard --to 192.168.1.50:47789
```

---

## Архитектура проекта

Рабочее пространство Rust разделено на 4 независимых крейта:

```text
LierSync/
├── crates/
│   ├── lier-sync-core/             # Общее кроссплатформенное P2P-ядро
│   │   └── src/
│   │       ├── channel.rs          # Транспортные каналы и политики маршрутизации
│   │       ├── clipboard.rs        # Буфер обмена (arboard) + SHA-256 анти-эхо фильтр
│   │       ├── config.rs           # Конфигурация AppConfig, язык RU/EN, история, папки
│   │       ├── discovery.rs        # UDP multicast/broadcast обнаружение пиров в LAN
│   │       ├── identity.rs         # Идентификатор узла и путь Downloads\LierSync
│   │       ├── merkle.rs           # Блочное хеширование по 128 КБ и расчёт diff
│   │       ├── protocol.rs         # Бинарный протокол обмена сообщениями и блоками
│   │       ├── transport.rs        # TCP P2P-сервер, Quick Send и синхронизация папок
│   │       └── watcher.rs          # Фоновый наблюдатель изменений в общих папках
│   ├── lier-sync-cli/              # Консольный клиент и headless-сервер
│   │   └── src/main.rs
│   ├── lier-sync-gui/              # Нативное десктоп-приложение (iced + системный трей)
│   │   ├── assets/                 # Иконки SVG, логотип и ресурсы Windows (.rc / .ico)
│   │   └── src/
│   │       ├── i18n.rs             # Локализация интерфейса и меню трея (RU / EN)
│   │       └── main.rs             # UI-вкладки, трей и асинхронный оркестратор
│   └── lier-sync-android/          # Мост JNI для Android-приложения (Kotlin + Compose)
│       └── src/lib.rs
├── Cargo.toml
├── LICENSE                         # GNU GPLv3
└── README.md
```

---

## Стек технологий

| Компонент | Технология |
| --- | --- |
| Язык и ядро | Rust 2024 (`lier-sync-core`, `lier-sync-cli`, `lier-sync-android`) |
| Десктопный GUI | `iced` 0.14 (нативный GPU-рендеринг без WebView) |
| Сетевой стек и async | `tokio` 1.54, `socket2` 0.6 |
| Системный трей и диалоги | `tray-icon` 0.25, `rfd` 0.17 |
| Целостность и Merkle diff | `sha2` 0.10, `walkdir` 2.5 |
| Буфер обмена и конфиги | `arboard` 3.7, `serde` + `serde_json`, `dirs` 7.0 |
| Мобильный мост | `jni` (`libliersync_android.so` для `arm64-v8a` / `x86_64`) |

---

## Быстрый старт (Разработка)

### Требования
- **Rust stable** (edition 2024)
- **Windows / Linux** (для системного трея на Linux требуется `libayatana-appindicator` или `libxdo` / `ksni` в зависимости от окружения)

### Сборка и запуск
```bash
git clone https://github.com/LaRCorp/LierSync.git
cd LierSync

# Проверка тестов
cargo test --workspace

# Запуск нативного GUI
cargo run -p lier-sync-gui

# Сборка оптимизированного release-бинарника (LTO + strip)
cargo build --release -p lier-sync-gui -p lier-sync-cli
```

Готовые бинарники появятся в `target/release/lier-sync-gui.exe` и `target/release/lier-sync-cli.exe` (на Linux — без `.exe`).

### Сборка ядра под Android (`cargo-ndk`)
```bash
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk
cargo ndk -t arm64-v8a -t x86_64 -o ./jniLibs build -p lier-sync-android --release
```

---

## Расположение системных данных

- **Принятые файлы (Windows):** `%USERPROFILE%\Downloads\LierSync\`
- **Принятые файлы (Android):** `Download/LierSync`
- **Конфигурация, язык и история передач (Windows):** `%APPDATA%\LaRevole\LierSync\config.json`

---

## Проверка качества и тесты

```bash
cargo test --workspace
```

---

## Лицензия

Проект распространяется под свободной лицензией **GNU General Public License v3.0 (GPLv3)** — см. [LICENSE](LICENSE).

Copyright (c) 2026 LaRevole (LaRCorp).

---

### Связанные проекты экосистемы
- 🎵🏷️ **[LaRCorp/LierTags](https://github.com/LaRCorp/LierTags)** — быстрый локальный редактор аудиотегов и обложек на Rust и Tauri 2.
- 🌐 **[LaRCorp/LierEX-Q](https://github.com/LaRCorp/LierEX-Q)** — браузерное расширение для извлечения метаданных в 1 клик.
- 🎮🎬 **[LaRCorp/Lierkour](https://github.com/LaRCorp/Lierkour)** — локальный кроссплатформенный каталогизатор фильмов, сериалов, аниме и видеоигр.
