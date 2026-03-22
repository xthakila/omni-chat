# OmniChat

A lightweight messaging aggregator built with Rust and CEF (Chromium Embedded Framework). Replaces [Ferdium](https://ferdium.org) at 1/60th the binary size with the same 409-recipe ecosystem.

![OmniChat](https://img.shields.io/badge/binary-6.3MB-brightgreen) ![Recipes](https://img.shields.io/badge/recipes-409-blue) ![Platform](https://img.shields.io/badge/platform-Linux%20x86__64-lightgrey)

## Why?

Ferdium uses Electron + React + MobX + AdonisJS + SQLite ORM + 15 simultaneous Chromium webviews = **3-4 GB RAM**. OmniChat replaces it with a native Rust shell using CEF for webviews only, with aggressive lifecycle management.

| | Ferdium | OmniChat |
|---|---|---|
| Binary size | ~400 MB | **6.7 MB** |
| Runtime | Electron (Node.js + Chromium) | Rust + CEF |
| RAM (5 services) | 3-4 GB | ~400-600 MB |
| Startup | 5-15s | <2s |
| Recipes | 409 | 409 (same) |

## Features

- **409 Ferdium-compatible recipes** — WhatsApp, Slack, Telegram, Discord, Gmail, and hundreds more
- **Searchable service picker** with Popular section — click `+`, search, click to add
- **Service switching** — click sidebar icons, each service gets its own isolated browser session
- **Session isolation** — separate cookies, localStorage, IndexedDB per service via CEF RequestContext
- **Background notifications** — lifecycle-aware polling (2s active, 5s background)
- **Recipe injection** — full Ferdium API shim (setBadge, loop, onNotify, injectCSS, etc.)
- **System tray** icon with unread badge
- **Frameless window** with custom title bar
- **Do Not Disturb** mode
- **SQLite persistence** — services and settings survive restarts
- **Single instance** enforcement
- **Catppuccin Mocha** dark theme sidebar with Discord-style pill indicators

## Install

### Prerequisites

```bash
# CEF runtime (~300MB, downloaded once)
cargo install export-cef-dir
mkdir -p ~/.local/share/cef
export-cef-dir --force ~/.local/share/cef

# System libraries
sudo apt install libgtk-3-dev libxdo-dev cmake
```

### From Release

```bash
# Download from GitHub Releases
tar xzf omnichat-v0.1.0-linux-x86_64.tar.gz
cd omnichat-release
bash install.sh
```

### From Source

```bash
git clone https://github.com/xthakila/omni-chat.git
cd omni-chat

# Download Ferdium recipes (optional, 409 recipes)
# Place them in ./recipes/ directory

# Build
export CEF_PATH=~/.local/share/cef
export LD_LIBRARY_PATH=$CEF_PATH
cargo build --release -j1  # -j1 to avoid OOM on <32GB RAM

# Install
bash install.sh
```

### Run

```bash
omnichat
```

Or find **OmniChat** in your application launcher.

## Architecture

```
Rust Application (Browser Process)
+--------------------------------------------------+
|  Sidebar    |    Active Service CEF Browser       |
|  (CEF       |    (WhatsApp / Slack / etc.)        |
|  browser)   |                                     |
|             |  JS shim injected on load:          |
|  * Slack    |  - Ferdium API (setBadge, etc.)     |
|  o WA       |  - Notification monkey-patch        |
|  o Disc     |  - Recipe webview.js executed        |
+-----------+-------------------------------------+
|  ServiceManager   RecipeLoader    SQLite DB       |
|  LifecycleManager TrayIcon        Settings        |
+--------------------------------------------------+
```

### Key Design Decisions

- **CEF Views framework** with Alloy runtime for single-window multi-BrowserView layout
- **IPC via URL scheme** (`omnichat-ipc://`) — JS navigates to custom URL, Rust's `RequestHandler.on_before_browse` intercepts and defers processing via `post_task` to avoid deadlock
- **No `format!()`** for JS code generation — raw string concatenation because recipe JS contains `{}` braces
- **State Mutex discipline** — never hold lock during CEF view operations (add/remove child views) to prevent deadlock
- **Wayland app_id proxy** — Python script that intercepts CEF's empty `xdg_toplevel.set_app_id("")` (opcode 3) and replaces it with `"omnichat"` for correct GNOME taskbar icon

### Project Structure

```
omnichat/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── omnichat-app/             # Main binary (6.3 MB release)
│   │   └── src/
│   │       ├── main.rs           # CEF init, single instance, message loop
│   │       ├── app.rs            # CefApp, window delegate, BrowserView management
│   │       ├── client.rs         # CefClient with MessageRouter forwarding
│   │       ├── handlers/         # CEF event handlers (life_span, load, display, request)
│   │       ├── service/          # ServiceManager, lifecycle, config, state
│   │       ├── recipe/           # Loader, injector, model, shim.js
│   │       ├── ipc/              # cefQuery handler, IPC message routing
│   │       ├── db/               # SQLite schema, queries
│   │       ├── notification.rs   # OS notifications via notify-rust
│   │       ├── tray.rs           # System tray via tray-icon
│   │       └── settings.rs       # App settings model
│   └── omnichat-helper/          # CEF subprocess (427 KB release)
│       └── src/main.rs           # RendererSideRouter for cefQuery
├── resources/
│   ├── sidebar.html              # Sidebar UI (Catppuccin theme)
│   └── settings.html             # Settings page
├── wayland-app-id-proxy.py       # Fixes CEF's empty Wayland app_id
└── install.sh                    # Installer (desktop entry, icon, launcher)
```

## Recipe Compatibility

OmniChat uses the same recipe format as Ferdium. Each recipe is a directory with:

- `package.json` — service metadata (URL, capabilities)
- `webview.js` — badge counting, notification handling

The Ferdium API shim provides: `setBadge()`, `setDialogTitle()`, `loop()`, `onNotify()`, `injectCSS()`, `handleDarkMode()`, `openNewWindow()`, `safeParseInt()`, `isImage()`, `setAvatarImage()`, `initialize()`, `injectJSUnsafe()`, `clearStorageData()`, `releaseServiceWorkers()`

CommonJS polyfills: `require('path')`, `require('fs')` (from file cache), `__dirname`, `_interopRequireDefault`

## Service Lifecycle

| State | Rendering | JS | WebSocket | Polling | Transition |
|---|---|---|---|---|---|
| **Active** | Full | Full | Yes | 2s | Switch away |
| **Backgrounded** | Hidden | Throttled | Yes | 5s | Idle 5 min |
| **Frozen** | Hidden | Muted | Yes | None | Idle 15 min |
| **Hibernated** | Destroyed | None | No | None | Manual |

## Platform Support

| Platform | Status | Notes |
|---|---|---|
| Linux x86_64 | **Supported** | Tested on Ubuntu 25.10 / GNOME / Wayland |
| Linux ARM64 | Untested | CEF binaries available, should work |
| macOS | Code ready | Needs CEF macOS framework + app bundle |
| Windows | Code ready | Needs CEF Windows DLLs + manifest |

## Known Limitations

- **GNOME Wayland taskbar icon**: CEF Alloy runtime sends empty `xdg_toplevel.set_app_id("")`. Worked around with a Wayland protocol proxy (`wayland-app-id-proxy.py`). Root cause is a missing property propagation in Chromium's `ConvertWidgetInitParamsToInitProperties()` in `ui/views/widget/desktop_aura/desktop_window_tree_host_platform.cc`.
- **CEF runtime required**: ~300MB CEF binary distribution must be installed separately.
- **Recipe compatibility**: Some Ferdium recipes using Node.js-specific APIs beyond `require('path')` and `require('fs')` may not work. The top 20 messaging services are tested.

## License

MIT
