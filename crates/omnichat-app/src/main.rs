#![allow(dead_code)]

mod app;
mod client;
mod db;
mod handlers;
mod ipc;
mod notification;
mod recipe;
mod service;
mod settings;
mod tray;

use cef::*;
use log::{error, info};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // --- Single instance check ---
    let data_dir = dirs_data_dir();
    std::fs::create_dir_all(&data_dir).ok();
    let pid_file = data_dir.join("omnichat.pid");
    if let Ok(old_pid) = std::fs::read_to_string(&pid_file) {
        if let Ok(pid) = old_pid.trim().parse::<u32>() {
            if std::path::Path::new(&format!("/proc/{pid}")).exists() {
                eprintln!("OmniChat is already running (PID {pid}).");
                // Try to focus the existing window via wmctrl
                let _ = std::process::Command::new("wmctrl")
                    .args(["-a", "OmniChat"])
                    .status();
                std::process::exit(0);
            }
        }
    }
    std::fs::write(&pid_file, std::process::id().to_string()).ok();

    // Set program name — GNOME uses this to match .desktop files for taskbar icons.
    gtk::glib::set_prgname(Some("omnichat"));
    gtk::glib::set_application_name("OmniChat");

    // Set the default window icon for all GTK windows.
    #[cfg(target_os = "linux")]
    {
        if gtk::init().is_ok() {
            gtk::Window::set_default_icon_name("omnichat");
        }
    }

    info!("OmniChat starting");

    // Initialize CEF API version.
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    let args = cef::args::Args::new();
    let Some(cmd_line) = args.as_cmd_line() else {
        error!("Failed to parse command line arguments");
        return Err("Failed to parse command line arguments".into());
    };

    // Check if this is a subprocess (renderer, GPU, etc.).
    let type_switch = CefString::from("type");
    let is_browser_process = cmd_line.has_switch(Some(&type_switch)) != 1;

    let ret = execute_process(Some(args.as_main_args()), None, std::ptr::null_mut());

    if is_browser_process {
        info!("Launching browser process");
        assert_eq!(ret, -1, "Cannot execute browser process");
    } else {
        let process_type = CefString::from(&cmd_line.switch_value(Some(&type_switch)));
        info!("Launching subprocess: {process_type}");
        assert!(ret >= 0, "Cannot execute non-browser process");
        return Ok(());
    }

    // Initialize the database.
    let data_dir = dirs_data_dir();
    std::fs::create_dir_all(&data_dir)?;
    let db_path = data_dir.join("omnichat.db");
    let conn = db::init(&db_path)?;
    info!("Database initialized at {}", db_path.display());

    // Load settings.
    let app_settings = settings::AppSettings::load(&conn);
    info!("Settings loaded");

    // Load recipes.
    let recipe_dirs = recipe::loader::default_recipe_dirs();
    let recipes = recipe::loader::scan_recipes(&recipe_dirs);
    info!("Loaded {} recipes", recipes.len());

    // Load services from DB.
    let services = db::queries::load_services(&conn)?;
    info!("Loaded {} services", services.len());

    // Create the CEF app with all state.
    let mut cef_app = app::create_app(conn, app_settings, recipes, services);

    // Set up CEF cache/data paths in our app data directory.
    let cache_dir = data_dir.join("cef_cache");
    std::fs::create_dir_all(&cache_dir)?;

    // Clean up stale singleton locks from previous crashes.
    // Check if the lock file exists but no process is using it.
    let lock_path = cache_dir.join("SingletonLock");
    if lock_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&lock_path) {
            // Lock file contains "hostname-pid". Check if that PID is still alive.
            let parts: Vec<&str> = content.split('-').collect();
            if let Some(pid_str) = parts.last() {
                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                    let proc_path = format!("/proc/{}", pid);
                    if !std::path::Path::new(&proc_path).exists() {
                        info!("Removing stale CEF singleton lock (PID {pid} dead)");
                        let _ = std::fs::remove_file(&lock_path);
                        let _ = std::fs::remove_file(cache_dir.join("SingletonSocket"));
                        let _ = std::fs::remove_file(cache_dir.join("SingletonCookie"));
                    }
                }
            }
        }
    }
    let root_cache = CefString::from(cache_dir.to_string_lossy().as_ref());

    // Find the helper binary (subprocess for renderer/GPU/utility processes).
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_default();
    let helper_path = exe_dir.join("omnichat_helper");
    let helper_path_str = CefString::from(helper_path.to_string_lossy().as_ref());
    info!("Helper binary: {}", helper_path.display());

    // Append --class=omnichat to CEF's global command line.
    // Chromium maps --class to Wayland xdg_toplevel app_id and X11 WM_CLASS.
    if let Some(global_cmd) = command_line_get_global() {
        let class_switch = CefString::from("class");
        let class_value = CefString::from("omnichat");
        global_cmd.append_switch_with_value(Some(&class_switch), Some(&class_value));
    }

    let settings = Settings {
        no_sandbox: 1,
        root_cache_path: root_cache,
        browser_subprocess_path: helper_path_str,
        ..Default::default()
    };

    assert_eq!(
        initialize(
            Some(args.as_main_args()),
            Some(&settings),
            Some(&mut cef_app),
            std::ptr::null_mut(),
        ),
        1,
        "CEF initialization failed"
    );

    // Re-set prgname after CEF init (CEF may override it during initialize).
    gtk::glib::set_prgname(Some("omnichat"));

    // Initialize the system tray icon.
    tray::init();

    info!("CEF initialized, entering message loop");
    run_message_loop();

    info!("Shutting down");
    shutdown();

    // Clean up PID file on exit.
    let _ = std::fs::remove_file(dirs_data_dir().join("omnichat.pid"));

    Ok(())
}

fn dirs_data_dir() -> std::path::PathBuf {
    if let Some(dir) = dirs_next::data_dir() {
        dir.join("omnichat")
    } else {
        std::path::PathBuf::from(".omnichat")
    }
}
