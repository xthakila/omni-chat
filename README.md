# OmniChat

A lightweight messaging aggregator built with Rust and CEF (Chromium Embedded Framework). A native, ~7 MB shell that runs [Ferdium](https://ferdium.org)-compatible recipes — far smaller than Ferdium's Electron app. (Recipes are not bundled; see [Recipes](#recipe-compatibility).)

![OmniChat](https://img.shields.io/badge/binary-6.8MB-brightgreen) ![Recipes](https://img.shields.io/badge/recipes-Ferdium--compatible-blue) ![Platform](https://img.shields.io/badge/platform-Linux%20x86__64-lightgrey)

## Why?

Ferdium uses Electron + React + MobX + AdonisJS + SQLite ORM + many simultaneous Chromium webviews. OmniChat replaces the shell with a small native Rust process using CEF for webviews only, with lifecycle management (freeze/hibernate idle services).

| | Ferdium | OmniChat |
|---|---|---|
| Binary size | ~400 MB | **6.8 MB** (release, stripped) |
| Runtime | Electron (Node.js + Chromium) | Rust + CEF |
| Startup (to first paint) | 5–15 s | **~0.4–0.9 s** (measured, Wayland/GNOME) |
| RAM | 3–4 GB | **~0.7–1.0 GB PSS / ~1.5–1.9 GB RSS** for 3 services (measured; one Chromium renderer each, so it scales like that many browser tabs) |
| Recipes | 409 bundled | Ferdium-compatible (bring your own) |

> Performance figures are measured on the dev machine (Ubuntu/GNOME/Wayland, software-rendered) via `scripts/rss-sampler.sh` and the `OMNICHAT_TIMING` startup timer — not synthetic. RAM is CEF/Chromium-dominated (one renderer process per service), so it scales with the number of live services. Backgrounded services are hidden (`was_hidden`) so Chromium throttles them, and after a longer idle they are **discarded** (`about:blank`) to free the page heap — see the lifecycle table + [Known Limitations](#known-limitations).

## Features

- **409 Ferdium-compatible recipes** — WhatsApp, Slack, Telegram, Discord, Gmail, and hundreds more
- **Searchable service picker** with Popular section — click `+`, search, click to add
- **Service switching** — click sidebar icons, each service gets its own isolated browser session
- **Per-origin session isolation** — cookies, localStorage, and IndexedDB are isolated per origin within a shared on-disk profile, so distinct services (different domains) keep separate logins that persist across restarts. (Two services on the *same* domain — e.g. two accounts of one service — would share; see [Known Limitations](#known-limitations).)
- **Background notifications** — lifecycle-aware polling (2s active, 5s background); on Linux, clicking a notification switches to the originating service
- **Recipe injection** — full Ferdium API shim (setBadge, loop, onNotify, injectCSS, etc.)
- **System tray** icon with unread badge + a quick-switch menu (one entry per service, snapshot at launch)
- **Frameless window** with custom title bar
- **Do Not Disturb** mode
- **SQLite persistence** — services and settings survive restarts
- **Single instance** enforcement
- **"Aurora" design system** — dark activity rail with accent active-pill, empty state, light/dark-ready tokens (`resources/theme.css`)
- **Origin-gated IPC** — privileged actions (add/remove/switch/settings) are accepted only from the app's own trusted UI, never from a loaded service page; outbound link-opens are restricted to `http(s)`/`mailto` (see [Security](#security))
- **Session-preserving picker/settings** — opening the picker or settings renders in a trusted overlay and does **not** reload (log out) the active service
- **Drag-to-reorder the rail** — drag a service icon to reorder; the order persists across restarts
- **Per-service controls in Settings** — inline rename, per-service Mute / Notifications / Dark-mode / **Sleep** toggles, ↑/↓ reorder, and Remove. **Sleep** controls the RAM↔notifications trade-off per service: ON = hibernate when idle (frees the page heap, but notifications pause until you reopen it); OFF = stay loaded so it keeps notifying. Turn it OFF for the services you must never miss
- **Reload a service** — right-click a rail icon → Reload (Chromium's own Ctrl+R also works when a service is focused)
- **Unread count in the window title** — the title shows `OmniChat (N)` when services have unread messages
- **Light / dark / auto appearance** — Settings → Appearance themes the app's own pages (picker/settings); the rail stays dark chrome. *Auto* follows your OS

## Install

### Quick setup (recommended)

From a source checkout, one script does everything — system libraries, Rust, the
CEF runtime, build, and install:

```bash
git clone https://github.com/xthakila/omni-chat.git
cd omni-chat
./setup.sh            # idempotent; JOBS=4 ./setup.sh to cap build parallelism on low-RAM machines
```

To upgrade later (pull + rebuild + reinstall):

```bash
./update.sh
```

The manual steps below are equivalent, if you prefer to run them yourself.

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

### Graphics (GPU vs software rendering)

OmniChat **renders in software by default** on Linux. This is deliberate: on a
Wayland session CEF/Chromium picks the Wayland ozone backend, which is
incompatible with its Vulkan GPU backend on common Intel (i915) drivers — the
GPU process can hang and **freeze the whole machine** (reproduced on an Intel
i3-1215U; Intel iGPU + Wayland is a very common laptop config). Software
rendering cannot hang the GPU on any hardware and is plenty fast for chat UIs.

To opt into hardware acceleration (Vulkan is still disabled to avoid the hang):

```bash
OMNICHAT_ENABLE_GPU=1 omnichat
```

If you enable the GPU and hit graphical glitches or a freeze, just unset it to
return to the safe default.

### Debugging

Set `OMNICHAT_REMOTE_DEBUG_PORT` to expose CEF's DevTools protocol for the app's
own pages (off by default). Pair it with `--remote-allow-origins` (required by
recent Chromium for the WebSocket upgrade):

```bash
OMNICHAT_REMOTE_DEBUG_PORT=9222 omnichat --remote-allow-origins='*'
# then: curl http://127.0.0.1:9222/json   (or attach chrome://inspect)
```

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
- **Wayland app_id** — CEF Alloy emits an empty `xdg_toplevel.set_app_id("")` and ignores the in-code hints; `wayland-app-id-proxy.py` rewrites it to `omnichat` and is wired into the launcher on Wayland (with a direct-exec fallback). Verified under a wlroots compositor: `app_id` goes from `''` to `omnichat`

### Project Structure

```
omnichat/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── omnichat-app/             # Main binary (6.8 MB release)
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
| **Frozen** | Hidden | Throttled | Yes | None | Idle 15 min |
| **Hibernated** | Page discarded (`about:blank`) | None | No | None | Switch back → reload |

> The lifecycle tick (env-tunable via `OMNICHAT_FREEZE_SECS` / `OMNICHAT_HIBERNATE_SECS` / `OMNICHAT_TICK_MS`) freezes idle services (audio muted) and then **discards** them to `about:blank` to free the page heap (see [Known Limitations](#known-limitations) for the trade-off). Switching back reloads the real URL; the persistent profile keeps you logged in.

## Platform Support

| Platform | Status | Notes |
|---|---|---|
| Linux x86_64 | **Supported** | Developed/tested on Ubuntu + GNOME/Wayland; CI builds on `ubuntu-latest` |
| Linux ARM64 | Untested | CEF binaries exist; not built or tested |
| macOS / Windows | **Planned / Untested** | The CEF webview code is cross-platform, but tray, window integration, and the installer are Linux-only today. Not built in CI. |

## Security

- **Origin-gated IPC.** Each CEF browser is classified at creation as the trusted sidebar, a trusted picker/settings overlay, or an untrusted service page. Privileged commands (add/remove/switch/reorder services, open/change settings) are honored **only** from trusted surfaces; a loaded (possibly compromised) service page cannot drive them. Service pages may report only their **own** badge/notification/title/avatar.
- **URL-scheme allowlist.** `Ferdium.openNewWindow(url)` / link-opens are restricted to `http(s)` and `mailto`; `file://` and custom protocol handlers are refused.
- **Recipe trust model.** Recipes are tagged **trusted** (bundled next to the binary / shipped with the app) or **untrusted** (dropped into your data dir or a Ferdium install). Only trusted recipes may call `injectJSUnsafe` (which injects arbitrary JS from the recipe's files) — for an untrusted recipe it's a logged no-op. All other shim APIs and `webview.js` still run for every recipe. Per-service RequestContext isolation separates cookies/storage. (CEF runs with `no_sandbox` today; a vendored+signed bundled set is a future enhancement.)
- **Crash resilience.** State is guarded by a non-poisoning mutex and CEF callbacks degrade gracefully, so one error can't cascade-crash the app.

## Known Limitations

- **GNOME Wayland taskbar icon.** CEF's Alloy runtime sends an empty `xdg_toplevel.set_app_id("")` and ignores the in-code `LinuxWindowProperties` / `--class` hints. The bundled `wayland-app-id-proxy.py` rewrites it to `omnichat` and **is wired into the launcher** on Wayland (with a direct-exec fallback); verified under a wlroots compositor (`app_id` `''` → `omnichat`). The in-window and tray icons are correct.
- **Shared profile (not per-service profiles).** Services render through CEF's **global** on-disk request context. A separate per-service `RequestContext` with its own `cache_path` *initializes its profile on disk but never spawns a renderer* — the service page stays blank — so OmniChat uses one shared profile. Logins persist and cookies/storage are isolated **per origin**; the only loss vs true per-service profiles is running two accounts of the *same* service. (Restoring true per-service profiles needs a fix to the CEF created-context renderer issue.)
- **RAM scales per service.** Each service runs in its own Chromium renderer (one per site), so RAM grows with the number of live services. The lever is the lifecycle (freeze/hibernate) via the per-service **Sleep** toggle — which trades a service's notifications for its RAM.
- **Hibernation = page discard.** Idle services (after `OMNICHAT_HIBERNATE_SECS`) discard their page by navigating to `about:blank`, freeing the page's DOM/JS heap (**~60–130 MB/service reclaimed, measured**) while keeping the browser + view alive (so switching stays instant, no dead pane). The renderer **process** persists, so this is a partial reclaim — not the service's full footprint — and the freed memory returns to Chromium's allocator (so RSS may not shrink immediately). Reactivation reloads the real URL; the persistent per-service profile restores the login. (`close_browser()` is NOT used — it can't tear down a CEF-Views `BrowserView`-backed browser. Trade-off: unsaved page state, e.g. a half-typed message, is lost once a service has been idle long enough to discard.)
- **CEF runtime required.** A ~300 MB CEF binary distribution must be installed separately (`export-cef-dir`).
- **Recipes not bundled** (see above) and **no runtime `fs`** for recipes.
- **Testing.** 20 unit tests cover the pure logic (URL allowlist, IPC parsing, URL decode, service-URL resolution, DB round-trips), run in CI. Service-by-service compatibility is spot-checked manually, not automated.

## License

MIT
