use cef::wrapper::message_router::*;
use cef::*;
use log::info;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::client::OmniChatClient;
use crate::recipe::model::Recipe;
use crate::service::config::ServiceConfig;
use crate::service::manager::ServiceManager;
use crate::settings::AppSettings;

/// Shared application state accessible from CEF callbacks.
pub struct AppState {
    pub service_manager: ServiceManager,
    pub db: rusqlite::Connection,
    pub settings: AppSettings,
    pub recipes: HashMap<String, Recipe>,
    /// CEF browsers keyed by service ID.
    pub browsers: HashMap<String, Browser>,
    /// BrowserViews keyed by service ID (for Views framework).
    pub browser_views: HashMap<String, BrowserView>,
    /// The sidebar CEF browser.
    pub sidebar_browser: Option<Browser>,
    /// The main window reference.
    pub main_window: Option<Window>,
    /// The box layout for adding/removing content views.
    pub box_layout: Option<BoxLayout>,
    /// Currently active service ID (has its BrowserView visible in the window).
    pub active_service_id: Option<String>,
    /// The currently displayed content BrowserView's service ID.
    pub displayed_service_id: Option<String>,
    /// Ordered service IDs for creation tracking.
    pub pending_service_ids: Vec<String>,
    /// CEF MessageRouter (browser side) for JS→Rust IPC via cefQuery.
    pub message_router: Option<Arc<BrowserSideRouter>>,
}

impl AppState {
    pub fn new(
        db: rusqlite::Connection,
        settings: AppSettings,
        recipes: Vec<Recipe>,
        services: Vec<ServiceConfig>,
    ) -> Self {
        let recipe_map: HashMap<String, Recipe> =
            recipes.into_iter().map(|r| (r.id.clone(), r)).collect();
        let service_manager = ServiceManager::new(services);

        Self {
            service_manager,
            db,
            settings,
            recipes: recipe_map,
            browsers: HashMap::new(),
            browser_views: HashMap::new(),
            sidebar_browser: None,
            main_window: None,
            box_layout: None,
            active_service_id: None,
            displayed_service_id: None,
            pending_service_ids: Vec::new(),
            message_router: None,
        }
    }

    pub fn active_browser(&self) -> Option<&Browser> {
        self.active_service_id
            .as_ref()
            .and_then(|id| self.browsers.get(id))
    }
}

pub type SharedState = Arc<Mutex<AppState>>;

wrap_app! {
    pub struct OmniChatApp;

    impl App {
        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(OmniChatBrowserProcessHandler::new(
                RefCell::new(None),
                RefCell::new(None),
            ))
        }
    }
}

static SHARED_STATE: OnceLock<SharedState> = OnceLock::new();

/// Initialize shared state and return a CEF App instance.
pub fn create_app(
    db: rusqlite::Connection,
    settings: AppSettings,
    recipes: Vec<Recipe>,
    services: Vec<ServiceConfig>,
) -> App {
    let state = Arc::new(Mutex::new(AppState::new(db, settings, recipes, services)));
    let _ = SHARED_STATE.set(state);
    OmniChatApp::new()
}

pub fn shared_state() -> SharedState {
    SHARED_STATE.get().expect("AppState not initialized").clone()
}

// --- Window delegate for the single main window ---

wrap_window_delegate! {
    pub struct OmniChatWindowDelegate {
        sidebar_view: RefCell<Option<BrowserView>>,
        content_view: RefCell<Option<BrowserView>>,
    }

    impl ViewDelegate {
        fn preferred_size(&self, _view: Option<&mut View>) -> Size {
            Size {
                width: 1200,
                height: 800,
            }
        }
    }

    impl PanelDelegate {}

    impl WindowDelegate {
        fn on_window_created(&self, window: Option<&mut Window>) {
            let Some(window) = window else { return };

            // Set window title.
            let title = CefString::from("OmniChat");
            window.set_title(Some(&title));

            // Set window icon (purple O ring).
            if let Some(mut icon_image) = image_create() {
                let png_data = generate_app_icon_png(64);
                icon_image.add_png(1.0, Some(&png_data));
                window.set_window_icon(Some(&mut icon_image));
                window.set_window_app_icon(Some(&mut icon_image));
            }

            // Set up horizontal box layout on the window (which is a Panel).
            let layout_settings = BoxLayoutSettings {
                horizontal: 1,
                ..Default::default()
            };
            let box_layout = window.set_to_box_layout(Some(&layout_settings));

            // Add sidebar view (left panel, fixed width).
            if let Some(sidebar) = self.sidebar_view.borrow().as_ref() {
                let mut sidebar_view = View::from(sidebar);
                sidebar_view.set_size(Some(&Size { width: 68, height: 800 }));
                window.add_child_view(Some(&mut sidebar_view));

                if let Some(ref layout) = box_layout {
                    layout.set_flex_for_view(Some(&mut sidebar_view), 0);
                }
            }

            // Add content view (main panel, fills remaining space).
            if let Some(content) = self.content_view.borrow().as_ref() {
                let mut content_view = View::from(content);
                window.add_child_view(Some(&mut content_view));

                if let Some(ref layout) = box_layout {
                    layout.set_flex_for_view(Some(&mut content_view), 1);
                }
            }

            // Store window + layout references for dynamic view management.
            let state = shared_state();
            let mut s = state.lock().unwrap();
            s.main_window = Some(window.clone());
            s.box_layout = box_layout.clone();
            drop(s);

            window.show();

            // NOTE: CEF Alloy sends xdg_toplevel.set_app_id("") on Wayland,
            // which prevents GNOME from matching the window to omnichat.desktop.
            // This is a known CEF limitation. The in-window icon and tray icon work.
        }

        fn on_window_destroyed(&self, _window: Option<&mut Window>) {
            let mut sidebar = self.sidebar_view.borrow_mut();
            *sidebar = None;
            let mut content = self.content_view.borrow_mut();
            *content = None;

            let state = shared_state();
            let mut s = state.lock().unwrap();
            s.main_window = None;
        }

        fn can_close(&self, _window: Option<&mut Window>) -> i32 {
            let state = shared_state();
            let s = state.lock().unwrap();
            let browser_ids: Vec<String> = s.browsers.keys().cloned().collect();
            drop(s);

            for id in &browser_ids {
                let state = shared_state();
                let s = state.lock().unwrap();
                if let Some(browser) = s.browsers.get(id) {
                    if let Some(host) = browser.host() {
                        host.try_close_browser();
                    }
                }
            }
            1
        }

        fn initial_show_state(&self, _window: Option<&mut Window>) -> ShowState {
            ShowState::NORMAL
        }

        fn window_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }

        fn is_frameless(&self, _window: Option<&mut Window>) -> i32 {
            1 // Frameless — we draw our own title bar region
        }

        fn with_standard_window_buttons(&self, _window: Option<&mut Window>) -> i32 {
            1 // Keep native close/min/max buttons overlaid on the frameless window
        }

        fn titlebar_height(
            &self,
            _window: Option<&mut Window>,
            titlebar_height: Option<&mut f32>,
        ) -> i32 {
            // Set the draggable title bar region height (top of content area).
            if let Some(h) = titlebar_height {
                *h = 38.0;
            }
            1
        }

        fn can_resize(&self, _window: Option<&mut Window>) -> i32 {
            1
        }

        fn can_maximize(&self, _window: Option<&mut Window>) -> i32 {
            1
        }

        fn can_minimize(&self, _window: Option<&mut Window>) -> i32 {
            1
        }

        fn linux_window_properties(
            &self,
            _window: Option<&mut Window>,
            properties: Option<&mut LinuxWindowProperties>,
        ) -> i32 {
            if let Some(props) = properties {
                props.wayland_app_id = CefString::from("omnichat");
                props.wm_class_class = CefString::from("OmniChat");
                props.wm_class_name = CefString::from("omnichat");
            }
            1
        }
    }
}

// NOTE: Wayland taskbar icon is a known CEF Alloy limitation.
// CEF sends xdg_toplevel.set_app_id("") ignoring linux_window_properties.
// The in-window icon (title bar) and tray icon work correctly.

// Layout is handled by CEF's BoxLayout in on_window_created.

// --- BrowserView delegates ---

wrap_browser_view_delegate! {
    pub struct SidebarBrowserViewDelegate {
    }

    impl ViewDelegate {
    }
    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }
    }
}

wrap_browser_view_delegate! {
    pub struct ContentBrowserViewDelegate {
    }

    impl ViewDelegate {
    }
    impl BrowserViewDelegate {
        fn browser_runtime_style(&self) -> RuntimeStyle {
            RuntimeStyle::ALLOY
        }
    }
}

// --- Browser process handler ---

wrap_browser_process_handler! {
    struct OmniChatBrowserProcessHandler {
        client: RefCell<Option<Client>>,
        sidebar_client: RefCell<Option<Client>>,
    }

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            debug_assert_ne!(currently_on(ThreadId::UI), 0);
            info!("CEF context initialized");

            let state = shared_state();

            // Create the MessageRouter for JS→Rust IPC (cefQuery).
            let config = MessageRouterConfig::default();
            let router = BrowserSideRouter::new(config);

            // Add our IPC handler.
            let ipc_handler = Arc::new(crate::ipc::handler::OmniChatQueryHandler::new(state.clone()));
            router.add_handler(ipc_handler, false);

            {
                let mut s = state.lock().unwrap();
                s.message_router = Some(router.clone());
            }

            let state_guard = state.lock().unwrap();

            // Create clients (pass router for process message forwarding).
            let client = OmniChatClient::new_client(state.clone(), router.clone());
            *self.client.borrow_mut() = Some(client);

            let sidebar_client = OmniChatClient::new_sidebar_client(state.clone(), router.clone());
            *self.sidebar_client.borrow_mut() = Some(sidebar_client);

            let services: Vec<ServiceConfig> = state_guard
                .service_manager
                .services()
                .iter()
                .filter(|s| s.is_enabled)
                .cloned()
                .collect();

            // Track only the first service ID for browser-to-service matching.
            // Additional services are created lazily on activation.
            let first_id: Vec<String> = services.iter().take(1).map(|s| s.id.clone()).collect();
            drop(state_guard);

            {
                let mut s = state.lock().unwrap();
                s.pending_service_ids = first_id;
            }

            // Build the sidebar URL.
            let sidebar_html = include_str!("../../../resources/sidebar.html");
            let sidebar_url = format!(
                "data:text/html;base64,{}",
                base64_encode_str(sidebar_html)
            );
            let sidebar_url = CefString::from(sidebar_url.as_str());

            // Create the sidebar BrowserView.
            let mut sidebar_client = self.sidebar_client.borrow().clone();
            let mut sidebar_delegate = SidebarBrowserViewDelegate::new();
            let sidebar_view = browser_view_create(
                sidebar_client.as_mut(),
                Some(&sidebar_url),
                Some(&BrowserSettings::default()),
                None,
                None,
                Some(&mut sidebar_delegate),
            );

            // Create the content BrowserView (first/active service or welcome page).
            let content_url = if !services.is_empty() {
                let first_svc = &services[0];
                let s = state.lock().unwrap();
                let recipe_url = s
                    .recipes
                    .get(&first_svc.recipe_id)
                    .map(|r| r.service_url.as_str())
                    .unwrap_or("");
                let url = first_svc.effective_url_with_recipe(recipe_url);
                drop(s);
                let mut s = state.lock().unwrap();
                s.active_service_id = Some(first_svc.id.clone());
                drop(s);
                url
            } else {
                let welcome = r#"<html><head><title>OmniChat</title></head>
                    <body style="font-family:system-ui;display:flex;align-items:center;justify-content:center;height:100vh;margin:0;background:#1a1a2e;color:#eee">
                    <div style="text-align:center"><h1>Welcome to OmniChat</h1><p>Add a service to get started.</p></div>
                    </body></html>"#;
                format!("data:text/html;base64,{}", base64_encode_str(welcome))
            };
            let content_url = CefString::from(content_url.as_str());

            // Create isolated request context for service isolation.
            let rc_settings = RequestContextSettings::default();
            let mut request_context = request_context_create_context(Some(&rc_settings), None);

            let mut content_client = self.client.borrow().clone();
            let mut content_delegate = ContentBrowserViewDelegate::new();
            let content_view = browser_view_create(
                content_client.as_mut(),
                Some(&content_url),
                Some(&BrowserSettings::default()),
                None,
                request_context.as_mut(),
                Some(&mut content_delegate),
            );

            // Store the content BrowserView for the first service.
            if !services.is_empty() {
                let first_id = services[0].id.clone();
                if let Some(ref cv) = content_view {
                    let mut s = state.lock().unwrap();
                    s.browser_views.insert(first_id.clone(), cv.clone());
                    s.displayed_service_id = Some(first_id);
                    drop(s);
                }
            }

            // Create the single top-level window with both views.
            let mut window_delegate = OmniChatWindowDelegate::new(
                RefCell::new(sidebar_view),
                RefCell::new(content_view),
            );
            window_create_top_level(Some(&mut window_delegate));

            info!("Main window created with sidebar + content");

            // Start the lifecycle tick timer (checks for freeze/hibernate transitions).
            let mut lifecycle_task = crate::service::lifecycle::LifecycleTickTask::new(state.clone());
            post_delayed_task(ThreadId::UI, Some(&mut lifecycle_task), 30_000);
        }

        fn default_client(&self) -> Option<Client> {
            self.client.borrow().clone()
        }
    }
}

/// Create a new BrowserView for a service and optionally swap it into the window.
/// This is called from the IPC handler on the CEF UI thread.
pub fn create_service_browser_view(state: &SharedState, service_id: &str) -> Option<BrowserView> {
    let s = state.lock().unwrap();
    let svc = s.service_manager.get_config(service_id)?.clone();
    let recipe_url = s
        .recipes
        .get(&svc.recipe_id)
        .map(|r| r.service_url.as_str())
        .unwrap_or("");
    let url = svc.effective_url_with_recipe(recipe_url);
    drop(s);

    let url = CefString::from(url.as_str());
    let rc_settings = RequestContextSettings::default();
    let mut request_context = request_context_create_context(Some(&rc_settings), None);

    // Create a client for this service.
    let router = {
        let s = state.lock().unwrap();
        s.message_router.clone()
    };
    let mut client = match router {
        Some(r) => OmniChatClient::new_client(state.clone(), r),
        None => return None,
    };

    let mut delegate = ContentBrowserViewDelegate::new();
    let bv = browser_view_create(
        Some(&mut client),
        Some(&url),
        Some(&BrowserSettings::default()),
        None,
        request_context.as_mut(),
        Some(&mut delegate),
    );

    if let Some(ref view) = bv {
        let mut s = state.lock().unwrap();
        s.browser_views.insert(service_id.to_string(), view.clone());
        s.pending_service_ids.push(service_id.to_string());
    }

    bv
}

/// Swap the displayed content BrowserView in the main window.
/// Removes the old one, adds the new one with flex=1.
/// IMPORTANT: must not hold state lock during CEF view operations to avoid deadlock.
pub fn swap_content_view(state: &SharedState, new_service_id: &str) {
    // 1. Gather everything we need from state, then drop the lock.
    let (window, old_bv, new_bv, layout) = {
        let s = state.lock().unwrap();
        let window = match s.main_window.as_ref() {
            Some(w) => w.clone(),
            None => return,
        };
        let old_bv = s
            .displayed_service_id
            .as_ref()
            .and_then(|id| s.browser_views.get(id).cloned());
        let new_bv = s.browser_views.get(new_service_id).cloned();
        let layout = s.box_layout.clone();
        (window, old_bv, new_bv, layout)
    };
    // Lock is dropped here.

    // 2. Do CEF view operations without holding the lock.
    if let Some(ref old) = old_bv {
        let mut old_view = View::from(old);
        window.remove_child_view(Some(&mut old_view));
    }

    if let Some(ref new) = new_bv {
        let mut new_view = View::from(new);
        window.add_child_view(Some(&mut new_view));
        if let Some(ref lay) = layout {
            lay.set_flex_for_view(Some(&mut new_view), 1);
        }
    }

    // 3. Update state after CEF operations.
    {
        let mut s = state.lock().unwrap();
        if new_bv.is_some() {
            s.displayed_service_id = Some(new_service_id.to_string());
        }
        s.active_service_id = Some(new_service_id.to_string());
        s.service_manager
            .set_lifecycle_state(new_service_id, crate::service::state::ServiceLifecycleState::Active);
        if let Some(rt) = s.service_manager.get_runtime_mut(new_service_id) {
            rt.touch();
        }
    }

    info!("Switched to service: {new_service_id}");
}

pub fn base64_encode_str(input: &str) -> String {
    let encoded = base64_encode(Some(input.as_bytes()));
    CefString::from(&encoded).to_string()
}

/// Generate a minimal PNG icon (purple O ring on dark background).
fn generate_app_icon_png(size: u32) -> Vec<u8> {
    let cx = size as f32 / 2.0;
    let cy = size as f32 / 2.0;
    let outer_r = size as f32 * 0.44;
    let inner_r = size as f32 * 0.22;

    let mut raw = Vec::with_capacity((size * size * 4 + size) as usize);
    for y in 0..size {
        raw.push(0); // PNG filter byte: None
        for x in 0..size {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let d = (dx * dx + dy * dy).sqrt();

            if d >= inner_r && d <= outer_r {
                // Gradient: mauve (#cba6f7) to blue (#89b4fa)
                let t = (d - inner_r) / (outer_r - inner_r);
                let r = (137.0 + t * 66.0) as u8;
                let g = (180.0 - t * 14.0) as u8;
                let b = (250.0 - t * 3.0) as u8;
                raw.extend_from_slice(&[r, g, b, 255]);
            } else {
                raw.extend_from_slice(&[0, 0, 0, 0]); // Transparent
            }
        }
    }

    // Minimal PNG encoder
    use std::io::Write;
    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");

    // IHDR chunk
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
    write_png_chunk(&mut png, b"IHDR", &ihdr);

    // IDAT chunk (compressed pixel data)
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(&raw).unwrap();
    let compressed = encoder.finish().unwrap();
    write_png_chunk(&mut png, b"IDAT", &compressed);

    // IEND chunk
    write_png_chunk(&mut png, b"IEND", &[]);

    png
}

fn write_png_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(chunk_type);
    out.extend_from_slice(data);
    let mut crc_data = Vec::with_capacity(4 + data.len());
    crc_data.extend_from_slice(chunk_type);
    crc_data.extend_from_slice(data);
    let crc = crc32(&crc_data);
    out.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}
