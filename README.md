# OmniChat

A lightweight messaging aggregator built with Rust and CEF (Chromium Embedded Framework). A native, ~6 MB shell that runs [Ferdium](https://ferdium.org)-compatible recipes — far smaller than Ferdium's Electron app. (Recipes are not bundled; see [Recipes](#recipe-compatibility).)

![OmniChat](https://img.shields.io/badge/binary-6.3MB-brightgreen) ![Recipes](https://img.shields.io/badge/recipes-Ferdium--compatible-blue) ![Platform](https://img.shields.io/badge/platform-Linux%20x86__64-lightgrey)

## Why?

Ferdium uses Electron + React + MobX + AdonisJS + SQLite ORM + many simultaneous Chromium webviews. OmniChat replaces the shell with a small native Rust process using CEF for webviews only, with lifecycle management (freeze/hibernate idle services).

| | Ferdium | OmniChat |
|---|---|---|
| Binary size | ~400 MB | **6.3 MB** (release, stripped) |
| Runtime | Electron (Node.js + Chromium) | Rust + CEF |
| Startup (to first paint) | 5–15 s | **~0.4–0.9 s** (measured, Wayland/GNOME) |
| RAM | 3–4 GB | **~0.8 GB PSS / ~1.5 GB RSS** for 3 services (measured) |
| Recipes | 409 bundled | Ferdium-compatible (bring your own) |

> Performance figures are measured on the dev machine (Ubuntu/GNOME/Wayland, software-rendered) via `scripts/rss-sampler.sh` and the `OMNICHAT_TIMING` startup timer — not synthetic. RAM is CEF/Chromium-dominated (one renderer process per active service), so it scales with the number of live services; hibernation reclaims it.

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
- **"Aurora" design system** — dark activity rail with accent active-pill, empty state, light/dark-ready tokens (`resources/theme.css`)
- **Origin-gated IPC** — privileged actions (add/remove/switch/settings) are accepted only from the app's own trusted UI, never from a loaded service page; outbound link-opens are restricted to `http(s)`/`mailto` (see [Security](#security))
- **Session-preserving picker/settings** — opening the picker or settings renders in a trusted overlay and does **not** reload (log out) the active service

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

# (Optional) add recipes so services can be added out of the box — see
# "Recipe Compatibility" below. Place recipe directories in ./recipes/.

# Build
export CEF_PATH=~/.local/share/cef
export LD_LIBRARY_PATH=$CEF_PATH
cargo build --release   # ~3 min; builds fine at default -j on a 16 GB+ machine

# Install (installs exactly one launcher entry; ships an uninstaller)
bash install.sh

# Uninstall later:  ~/.local/lib/omnichat/uninstall.sh   (add --purge to drop data)
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
- **State Mutex discipline** — a non-poisoning `parking_lot::Mutex`; never held during CEF view operations (add/remove child views) to prevent deadlock
- **IPC trust boundary** — every browser is classified (sidebar / overlay / service) by `Browser::identifier()`; privileged commands are gated to trusted surfaces (see [Security](#security))
- **Trusted overlay** — picker/settings render in a dedicated overlay BrowserView over the content area, so the active service's session is preserved (not navigated away)
- **Wayland app_id** — CEF Alloy emits an empty `xdg_toplevel.set_app_id("")` and ignores the in-code hints; `wayland-app-id-proxy.py` is a protocol-proxy attempt but is not yet wired into the launcher (see [Known Limitations](#known-limitations))

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
│   ├── theme.css                 # "Aurora" design system (shared tokens)
│   └── sidebar.html              # Sidebar UI (Aurora dark rail)
├── scripts/rss-sampler.sh        # Read-only PSS/RSS sampler (perf measurement)
├── wayland-app-id-proxy.py       # Wayland app_id proxy (not yet wired in)
├── install.sh                    # Installer (single desktop entry, icon, launcher)
└── uninstall.sh                  # Uninstaller (removes all artifacts; --purge for data)
```

> Note: the service picker and settings pages are generated in `ipc/handler.rs`
> (data: URIs with the shared theme injected), not as standalone files.

## Recipe Compatibility

> **Recipes are not bundled.** OmniChat reads [Ferdium](https://github.com/ferdium/ferdium-recipes)-format recipes but ships none in this repo. At first run the service picker is empty until recipe directories are present. OmniChat scans, in order: next to the binary, `~/.local/share/omnichat/recipes/`, `~/.local/share/Ferdium/recipes/`, and `./recipes/`. To populate it, copy a Ferdium `recipes/` set (or an existing Ferdium install's) into one of those locations.

OmniChat uses the same recipe format as Ferdium. Each recipe is a directory with:

- `package.json` — service metadata (URL, capabilities)
- `webview.js` — badge counting, notification handling

The Ferdium API shim provides: `setBadge()`, `setDialogTitle()`, `loop()`, `onNotify()`, `injectCSS()`, `handleDarkMode()`, `openNewWindow()`, `safeParseInt()`, `isImage()`, `setAvatarImage()`, `initialize()`, `injectJSUnsafe()`. Intentional no-ops (not needed under CEF's RequestContext isolation): `clearStorageData()`, `releaseServiceWorkers()`.

CommonJS polyfills: `require('path')`, `require('fs')` (reads from a small per-recipe file cache only — runtime file I/O is **not** supported; `readFileSync` of an uncached path returns `""`), `__dirname`, `_interopRequireDefault`.

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
| Linux x86_64 | **Supported** | Developed/tested on Ubuntu + GNOME/Wayland; CI builds on `ubuntu-latest` |
| Linux ARM64 | Untested | CEF binaries exist; not built or tested |
| macOS / Windows | **Planned / Untested** | The CEF webview code is cross-platform, but tray, window integration, and the installer are Linux-only today. Not built in CI. |

## Security

- **Origin-gated IPC.** Each CEF browser is classified at creation as the trusted sidebar, a trusted picker/settings overlay, or an untrusted service page. Privileged commands (add/remove/switch/reorder services, open/change settings) are honored **only** from trusted surfaces; a loaded (possibly compromised) service page cannot drive them. Service pages may report only their **own** badge/notification/title/avatar.
- **URL-scheme allowlist.** `Ferdium.openNewWindow(url)` / link-opens are restricted to `http(s)` and `mailto`; `file://` and custom protocol handlers are refused.
- **Recipe trust (note).** Recipe `webview.js` runs as trusted JavaScript inside each service. Only load recipes you trust. CEF runs with `no_sandbox` today; per-service RequestContext isolation separates cookies/storage. Recipe signing + a curated trusted set are planned.
- **Crash resilience.** State is guarded by a non-poisoning mutex and CEF callbacks degrade gracefully, so one error can't cascade-crash the app.

## Known Limitations

- **GNOME Wayland taskbar icon.** CEF's Alloy runtime sends an empty `xdg_toplevel.set_app_id("")` and ignores the in-code `LinuxWindowProperties` / `--class` hints, so GNOME can't match the window to `omnichat.desktop` (generic icon). The bundled `wayland-app-id-proxy.py` is a protocol-proxy approach to this but is **not yet wired into the launcher**. The in-window and tray icons are correct.
- **CEF runtime required.** A ~300 MB CEF binary distribution must be installed separately (`export-cef-dir`).
- **Recipes not bundled** (see above) and **no runtime `fs`** for recipes.
- **Testing.** 20 unit tests cover the pure logic (URL allowlist, IPC parsing, URL decode, service-URL resolution, DB round-trips), run in CI. Service-by-service compatibility is spot-checked manually, not automated.

## License

MIT
