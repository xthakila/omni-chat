use log::{debug, info, warn};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder};

static BADGE_COUNT: AtomicU32 = AtomicU32::new(0);

/// Channel to send badge updates to the tray icon thread.
static BADGE_SENDER: std::sync::OnceLock<mpsc::Sender<u32>> = std::sync::OnceLock::new();

/// Generate a programmatic tray icon with optional badge dot.
fn generate_icon(badge: u32) -> Icon {
    let size = 32u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];

    let cx = size as f32 / 2.0;
    let cy = size as f32 / 2.0;
    let outer_r = 14.0f32;
    let inner_r = 7.0f32;

    for y in 0..size {
        for x in 0..size {
            let idx = ((y * size + x) * 4) as usize;
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();

            // Outer circle
            if dist <= outer_r && dist >= inner_r {
                // Gradient from mauve (#cba6f7) to blue (#89b4fa)
                let t = (dist - inner_r) / (outer_r - inner_r);
                rgba[idx] = (137.0 + t * 66.0) as u8;     // 137 → 203
                rgba[idx + 1] = (180.0 - t * 14.0) as u8;  // 180 → 166
                rgba[idx + 2] = (250.0 - t * 3.0) as u8;   // 250 → 247
                rgba[idx + 3] = 255;
            } else if dist > outer_r && dist <= outer_r + 0.8 {
                // Anti-alias
                let a = ((outer_r + 0.8 - dist) / 0.8 * 255.0) as u8;
                rgba[idx] = 203; rgba[idx + 1] = 166; rgba[idx + 2] = 247; rgba[idx + 3] = a;
            } else if dist < inner_r && dist >= inner_r - 0.8 {
                let a = ((dist - (inner_r - 0.8)) / 0.8 * 255.0) as u8;
                rgba[idx] = 137; rgba[idx + 1] = 180; rgba[idx + 2] = 250; rgba[idx + 3] = a;
            }
        }
    }

    // Red badge dot for unread messages
    if badge > 0 {
        let bcx = size as f32 - 7.0;
        let bcy = 7.0f32;
        let br = 5.5f32;
        let border = 1.5f32;

        for y in 0..size {
            for x in 0..size {
                let idx = ((y * size + x) * 4) as usize;
                let dx = x as f32 - bcx;
                let dy = y as f32 - bcy;
                let d = (dx * dx + dy * dy).sqrt();

                if d <= br + border {
                    // White border ring
                    rgba[idx] = 30; rgba[idx + 1] = 30; rgba[idx + 2] = 46; rgba[idx + 3] = 255;
                }
                if d <= br {
                    // Red fill (#f38ba8)
                    rgba[idx] = 243; rgba[idx + 1] = 139; rgba[idx + 2] = 168; rgba[idx + 3] = 255;
                }
            }
        }
    }

    Icon::from_rgba(rgba, size, size).expect("Failed to create tray icon")
}

/// Initialize the system tray icon with menu.
/// Must be called from the main thread before entering CEF message loop.
pub fn init() {
    // GTK must be initialized before tray-icon can create menus.
    #[cfg(target_os = "linux")]
    {
        if let Err(e) = gtk::init() {
            warn!("GTK init failed: {e} — tray icon may not work");
        }
    }
    let menu = Menu::new();
    let show_item = MenuItem::new("Show OmniChat", true, None);
    let quit_item = MenuItem::new("Quit", true, None);
    let _ = menu.append(&show_item);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&quit_item);

    let icon = generate_icon(0);

    let tray = match TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("OmniChat")
        .with_icon(icon)
        .build()
    {
        Ok(t) => {
            info!("Tray icon created");
            t
        }
        Err(e) => {
            warn!("Failed to create tray icon: {e}");
            return;
        }
    };

    // Set up a channel so other threads can request badge updates.
    let (tx, _rx) = mpsc::channel::<u32>();
    let _ = BADGE_SENDER.set(tx);

    let quit_id = quit_item.id().clone();

    // Menu event handler thread.
    std::thread::spawn(move || {
        loop {
            if let Ok(event) = MenuEvent::receiver().recv() {
                if event.id == quit_id {
                    info!("Quit requested from tray");
                    std::process::exit(0);
                }
            }
        }
    });

    // Badge update handler thread.
    // TrayIcon is !Send, so we keep it on this spawned thread after moving it.
    // We use a trick: spawn a thread, move tray into it, and listen for badge updates.
    // But actually TrayIcon is !Send... so we need to keep it on the current thread.
    // We'll use a leak approach and process badge updates via a periodic CEF task instead.
    Box::leak(Box::new(tray));
    // Badge updates will be handled by the update_badge_on_main_thread function
    // called from the CEF UI thread.
}

/// Update the tray icon badge count.
/// Called from any thread; the actual icon update needs to happen on main thread.
pub fn update_badge(total: u32) {
    let prev = BADGE_COUNT.swap(total, Ordering::Relaxed);
    if prev != total {
        debug!("Tray badge: {prev} -> {total}");
    }
}

/// Get the current badge count.
pub fn badge_count() -> u32 {
    BADGE_COUNT.load(Ordering::Relaxed)
}
