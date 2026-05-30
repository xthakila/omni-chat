use cef::wrapper::message_router::*;
use cef::{Browser, Frame, ImplBrowser, ImplBrowserHost, ImplFrame};
use log::{debug, info, warn};
use serde::Deserialize;
use std::sync::{Arc, Mutex};

use crate::app::{IpcRole, SharedState};
use crate::notification;
use crate::service::config::ServiceConfig;
use crate::settings::AppSettings;

/// Whether a URL is safe to hand to the OS handler via `open::that`.
/// Only web + mail schemes; everything else (file://, custom protocol handlers,
/// javascript:, data:, etc.) is rejected so a remote service page cannot abuse
/// `Ferdium.openNewWindow` to disclose local files or invoke system handlers.
pub fn is_allowed_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:")
}

/// CEF MessageRouter handler that receives cefQuery messages from JS.
pub struct OmniChatQueryHandler {
    state: SharedState,
}

impl OmniChatQueryHandler {
    pub fn new(state: SharedState) -> Self {
        Self { state }
    }
}

impl BrowserSideHandler for OmniChatQueryHandler {
    fn on_query_str(
        &self,
        browser: Option<Browser>,
        _frame: Option<Frame>,
        _query_id: i64,
        request: &str,
        _persistent: bool,
        callback: Arc<Mutex<dyn BrowserSideCallback>>,
    ) -> bool {
        info!("IPC received: {}", &request[..request.len().min(100)]);
        let sender_id = browser.as_ref().map(|b| b.identifier());
        handle_message(&self.state, request, sender_id);
        if let Ok(cb) = callback.lock() {
            cb.success_str("");
        }
        true
    }

    fn on_query_canceled(&self, _browser: Option<Browser>, _frame: Option<Frame>, _query_id: i64) {}
}

/// Messages received from JavaScript via cefQuery.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum IpcMessage {
    #[serde(rename = "badge")]
    Badge {
        #[serde(rename = "serviceId")]
        service_id: String,
        direct: u32,
        indirect: u32,
    },

    #[serde(rename = "notification")]
    Notification {
        #[serde(default, rename = "serviceId")]
        service_id: String,
        title: String,
        #[serde(default)]
        body: String,
        #[serde(default)]
        icon: String,
        #[serde(default)]
        tag: String,
        #[serde(default)]
        silent: bool,
    },

    #[serde(rename = "dialog_title")]
    DialogTitle {
        #[serde(rename = "serviceId")]
        service_id: String,
        title: String,
    },

    #[serde(rename = "avatar")]
    Avatar {
        #[serde(rename = "serviceId")]
        service_id: String,
        url: String,
    },

    #[serde(rename = "open_url")]
    OpenUrl { url: String },

    #[serde(rename = "activate_service")]
    ActivateService {
        #[serde(rename = "serviceId")]
        service_id: String,
    },

    #[serde(rename = "add_service")]
    AddService {
        #[serde(rename = "recipeId")]
        recipe_id: String,
        name: String,
        #[serde(default)]
        url: Option<String>,
        #[serde(default)]
        team: Option<String>,
    },

    #[serde(rename = "remove_service")]
    RemoveService {
        #[serde(rename = "serviceId")]
        service_id: String,
    },

    #[serde(rename = "reorder_services")]
    ReorderServices {
        #[serde(rename = "serviceIds")]
        service_ids: Vec<String>,
    },

    #[serde(rename = "open_picker")]
    OpenPicker {},

    #[serde(rename = "open_settings")]
    OpenSettings {},

    #[serde(rename = "update_settings")]
    UpdateSettings { settings: AppSettings },

    #[serde(rename = "set_service_flag")]
    SetServiceFlag {
        #[serde(rename = "serviceId")]
        service_id: String,
        flag: String,
        value: bool,
    },

    #[serde(rename = "rename_service")]
    RenameService {
        #[serde(rename = "serviceId")]
        service_id: String,
        name: String,
    },
}

/// Handle an IPC message from a service webview.
pub fn handle_message(state: &SharedState, raw: &str, sender_id: Option<i32>) {
    let msg: IpcMessage = match serde_json::from_str(raw) {
        Ok(m) => m,
        Err(e) => {
            warn!("Invalid IPC message: {e} — raw: {raw}");
            return;
        }
    };

    // Classify the sender (trust boundary). The lock is released before the
    // match arms re-lock (parking_lot is not reentrant).
    let role = state.lock().ipc_role(sender_id);
    debug!("IPC from {role:?}: {}", &raw[..raw.len().min(60)]);

    // Capability gating: privileged commands may only come from our own trusted
    // UI (sidebar / picker-settings overlay), never from a loaded third-party
    // service page (which could be malicious or XSS'd).
    let trusted = matches!(role, IpcRole::Sidebar | IpcRole::TrustedOverlay);
    let privileged = matches!(
        msg,
        IpcMessage::AddService { .. }
            | IpcMessage::RemoveService { .. }
            | IpcMessage::ActivateService { .. }
            | IpcMessage::ReorderServices { .. }
            | IpcMessage::OpenPicker { .. }
            | IpcMessage::OpenSettings { .. }
            | IpcMessage::UpdateSettings { .. }
            | IpcMessage::SetServiceFlag { .. }
            | IpcMessage::RenameService { .. }
    );
    if privileged && !trusted {
        warn!(
            "Rejected privileged IPC from untrusted sender ({role:?}): {}",
            &raw[..raw.len().min(80)]
        );
        return;
    }

    // A loaded service page may only report events about ITS OWN serviceId, so
    // it cannot spoof another service's badge/notification/title/avatar.
    if let IpcRole::Service(sender_sid) = &role {
        let claimed = match &msg {
            IpcMessage::Badge { service_id, .. }
            | IpcMessage::DialogTitle { service_id, .. }
            | IpcMessage::Avatar { service_id, .. }
            | IpcMessage::Notification { service_id, .. } => Some(service_id.as_str()),
            _ => None,
        };
        if let Some(c) = claimed {
            if !c.is_empty() && c != sender_sid {
                warn!("Service '{sender_sid}' tried to report for '{c}'; rejected");
                return;
            }
        }
    }

    match msg {
        IpcMessage::Badge {
            service_id,
            direct,
            indirect,
        } => {
            debug!("Badge update: {service_id} direct={direct} indirect={indirect}");
            let mut s = state.lock();
            s.service_manager
                .update_badge(&service_id, direct, indirect);

            // Update the sidebar.
            push_sidebar_state(&s);

            // Update the tray icon badge.
            let total = s.service_manager.total_unread();
            drop(s);
            crate::tray::update_badge(total);
        }

        IpcMessage::Notification {
            service_id,
            title,
            body,
            icon: _,
            tag: _,
            silent,
        } => {
            let s = state.lock();

            // Look up the service that sent this notification.
            // Falls back to active service if service_id is empty.
            let lookup_id = if service_id.is_empty() {
                s.active_service_id.as_deref().unwrap_or("")
            } else {
                &service_id
            };

            let config = s.service_manager.get_config(lookup_id);
            let service_name = config.map(|c| c.name.clone()).unwrap_or_default();
            let enabled = config.map(|c| c.is_notification_enabled).unwrap_or(true);
            let muted = config.map(|c| c.is_muted).unwrap_or(false);

            // Check DND mode.
            let dnd = s.settings.enable_dnd;
            drop(s);

            if enabled && !muted && !silent && !dnd {
                notification::show(&service_name, &title, &body);
            }
        }

        IpcMessage::DialogTitle { service_id, title } => {
            debug!("Dialog title: {service_id} = {title}");
            let mut s = state.lock();
            let title_opt = if title.is_empty() { None } else { Some(title) };
            s.service_manager.set_dialog_title(&service_id, title_opt);
        }

        IpcMessage::Avatar { service_id, url } => {
            debug!("Avatar update: {service_id} = {url}");
            // Could cache the avatar for sidebar display.
        }

        IpcMessage::OpenUrl { url } => {
            // A loaded remote page can reach this via Ferdium.openNewWindow(url).
            // Only hand safe schemes to the OS handler — never file:// or custom
            // protocol handlers (local-file disclosure / handler-abuse / RCE).
            if is_allowed_url(&url) {
                info!("Opening URL in system browser: {url}");
                let _ = open::that(&url);
            } else {
                warn!("Refused to open URL with disallowed scheme: {url}");
            }
        }

        IpcMessage::ActivateService { service_id } => {
            info!("Activating service: {service_id}");

            // Background the previously active service.
            {
                let mut s = state.lock();
                if let Some(ref prev_id) = s.active_service_id.clone() {
                    if prev_id != &service_id {
                        s.service_manager.set_lifecycle_state(
                            prev_id,
                            crate::service::state::ServiceLifecycleState::Backgrounded,
                        );
                    }
                }
            }

            // If the service doesn't have a BrowserView yet, create one.
            let needs_creation = {
                let s = state.lock();
                !s.browser_views.contains_key(&service_id)
            };
            if needs_creation {
                crate::app::create_service_browser_view(state, &service_id);
            }

            // Swap the displayed view.
            crate::app::swap_content_view(state, &service_id);

            let s = state.lock();
            push_sidebar_state(&s);
        }

        IpcMessage::AddService {
            recipe_id,
            name,
            url,
            team,
        } => {
            info!("Adding service: {name} ({recipe_id})");
            let id = uuid::Uuid::new_v4().to_string();
            let mut config =
                crate::service::config::ServiceConfig::new(id.clone(), recipe_id, name);
            config.custom_url = url;
            config.team = team;

            {
                let mut s = state.lock();
                let sort_order = s.service_manager.services().len() as i32;
                config.sort_order = sort_order;
                s.service_manager.add_service(config.clone());
                crate::db::warn_on_err(
                    "save_service",
                    crate::db::queries::save_service(&s.db, &config),
                );
            }

            // Create a BrowserView for the new service.
            crate::app::create_service_browser_view(state, &id);

            // Switch to the new service immediately.
            crate::app::swap_content_view(state, &id);

            let s = state.lock();
            push_sidebar_state(&s);
        }

        IpcMessage::RemoveService { service_id } => {
            info!("Removing service: {service_id}");
            let mut s = state.lock();

            // Close the browser if open.
            if let Some(browser) = s.browsers.get(&service_id).cloned() {
                if let Some(host) = browser.host() {
                    host.close_browser(1);
                }
            }
            s.browsers.remove(&service_id);
            s.service_manager.remove_service(&service_id);

            // Persist deletion.
            crate::db::warn_on_err(
                "delete_service",
                crate::db::queries::delete_service(&s.db, &service_id),
            );
            push_sidebar_state(&s);
        }

        IpcMessage::OpenSettings {} => {
            info!("Opening settings");
            let (services_json, settings_json) = {
                let s = state.lock();
                let sorted = s.service_manager.sorted_services();
                (
                    serde_json::to_string(&sorted).unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(&s.settings).unwrap_or_else(|_| "{}".into()),
                )
            };
            // Render in the trusted overlay so the active service's session is
            // preserved (no logout) and the page is origin-classified as trusted.
            let html = build_settings_html(&services_json, &settings_json);
            crate::app::show_overlay(state, &html);
        }

        IpcMessage::OpenPicker {} => {
            info!("Opening service picker");
            let recipes_json = {
                let s = state.lock();
                let recipe_catalog: Vec<serde_json::Value> = s
                    .recipes
                    .values()
                    .map(|r| {
                        serde_json::json!({
                            "id": r.id,
                            "name": r.name,
                            "url": r.service_url,
                            "hasTeamId": r.has_team_id,
                        })
                    })
                    .collect();
                serde_json::to_string(&recipe_catalog).unwrap_or_else(|_| "[]".into())
            };
            // Render in the trusted overlay (preserves the active service session).
            let picker_html = build_picker_html(&recipes_json);
            crate::app::show_overlay(state, &picker_html);
        }

        IpcMessage::ReorderServices { service_ids } => {
            debug!("Reordering services: {service_ids:?}");
            let mut s = state.lock();
            for (i, id) in service_ids.iter().enumerate() {
                if let Some(config) = s.service_manager.get_config_mut(id) {
                    config.sort_order = i as i32;
                }
            }
            // Persist updated order.
            for svc in s.service_manager.services() {
                crate::db::warn_on_err(
                    "save_service",
                    crate::db::queries::save_service(&s.db, svc),
                );
            }
            push_sidebar_state(&s);
        }

        IpcMessage::UpdateSettings { settings } => {
            info!("Updating settings via IPC");
            let mut s = state.lock();
            s.settings = settings;
            let snapshot = s.settings.clone();
            snapshot.save(&s.db);
        }

        IpcMessage::SetServiceFlag {
            service_id,
            flag,
            value,
        } => {
            info!("Set service flag {service_id}.{flag} = {value}");
            let mut s = state.lock();
            let mut saved: Option<ServiceConfig> = None;
            if let Some(cfg) = s.service_manager.get_config_mut(&service_id) {
                let ok = match flag.as_str() {
                    "muted" => {
                        cfg.is_muted = value;
                        true
                    }
                    "notifications" => {
                        cfg.is_notification_enabled = value;
                        true
                    }
                    "badge" => {
                        cfg.is_badge_enabled = value;
                        true
                    }
                    "darkMode" => {
                        cfg.is_dark_mode_enabled = value;
                        true
                    }
                    "hibernation" => {
                        cfg.is_hibernation_enabled = value;
                        true
                    }
                    "enabled" => {
                        cfg.is_enabled = value;
                        true
                    }
                    _ => false,
                };
                if ok {
                    saved = Some(cfg.clone());
                }
            }
            match saved {
                Some(cfg) => {
                    crate::db::warn_on_err(
                        "save_service",
                        crate::db::queries::save_service(&s.db, &cfg),
                    );
                    push_sidebar_state(&s);
                }
                None => {
                    warn!("set_service_flag: unknown service '{service_id}' or flag '{flag}'")
                }
            }
        }

        IpcMessage::RenameService { service_id, name } => {
            info!("Rename service {service_id} -> {name}");
            let mut s = state.lock();
            let mut saved: Option<ServiceConfig> = None;
            if let Some(cfg) = s.service_manager.get_config_mut(&service_id) {
                cfg.name = name;
                saved = Some(cfg.clone());
            }
            match saved {
                Some(cfg) => {
                    crate::db::warn_on_err(
                        "save_service",
                        crate::db::queries::save_service(&s.db, &cfg),
                    );
                    push_sidebar_state(&s);
                }
                None => warn!("rename_service: unknown service '{service_id}'"),
            }
        }
    }
}

/// Push current service state to the sidebar browser.
fn push_sidebar_state(state: &crate::app::AppState) {
    let sidebar = match state.sidebar_browser.as_ref().cloned() {
        Some(s) => s,
        None => return,
    };
    let frame: cef::Frame = match sidebar.main_frame() {
        Some(f) => f,
        None => return,
    };

    // Serialize in display order (sort_order), so reorder is reflected live
    // without a restart.
    let sorted = state.service_manager.sorted_services();
    let services_json = serde_json::to_string(&sorted).unwrap_or_else(|_| "[]".into());
    let active_json = state
        .active_service_id
        .as_ref()
        .map(|id| format!("\"{}\"", id))
        .unwrap_or_else(|| "null".into());

    let mut badges = serde_json::Map::new();
    for svc in state.service_manager.services() {
        if let Some(rt) = state.service_manager.get_runtime(&svc.id) {
            badges.insert(
                svc.id.clone(),
                serde_json::json!({
                    "direct": rt.direct_count,
                    "indirect": rt.indirect_count,
                }),
            );
        }
    }
    let badges_json = serde_json::to_string(&badges).unwrap_or_else(|_| "{}".into());

    let js = format!(
        "if(window.__omnichat_sidebar) {{ window.__omnichat_sidebar.updateServices({services_json}, {active_json}, {badges_json}); }}"
    );
    let js = cef::CefString::from(js.as_str());
    let url = cef::CefString::from("omnichat://sidebar-update");
    frame.execute_java_script(Some(&js), Some(&url), 0);
}

/// Build the settings HTML page.
fn build_settings_html(services_json: &str, settings_json: &str) -> String {
    let template = r#"<!DOCTYPE html>
<html data-theme="dark"><head><meta charset="UTF-8">
<style>
/*__THEME__*/
:root {{ --bg:var(--surface-base); --sf:var(--surface-raised); --hv:var(--surface-hover); --ac:var(--accent); --tx:var(--text-strong); --dm:var(--text-muted); --rd:var(--danger); }}
body {{ font-family:var(--font); background:var(--bg); color:var(--tx); padding:36px 40px; max-width:760px; margin:0 auto; -webkit-font-smoothing:antialiased; }}
h1 {{ font-size:24px; font-weight:700; margin-bottom:4px; letter-spacing:-.01em; }}
.sub {{ color:var(--dm); font-size:13px; margin-bottom:28px; }}
h2 {{ font-size:12px; font-weight:700; color:var(--dm); text-transform:uppercase; letter-spacing:.06em; margin:26px 0 10px; }}
.svc-row {{ display:flex; align-items:center; padding:12px 14px; background:var(--sf); border:1px solid var(--border-subtle); border-radius:var(--r-md); margin-bottom:7px; gap:12px; }}
.svc-ico {{ width:30px; height:30px; border-radius:8px; background:var(--hv); display:flex; align-items:center; justify-content:center; font-size:13px; font-weight:700; color:var(--ac); flex-shrink:0; }}
.svc-name {{ flex:1; font-size:14px; font-weight:600; }}
.svc-recipe {{ color:var(--dm); font-size:11px; }}
.rm-btn {{ background:none; border:1px solid var(--border-strong); color:var(--dm); border-radius:var(--r-sm); padding:5px 12px; font-size:12px; font-weight:600; cursor:pointer; transition:all .12s; }}
.rm-btn:hover {{ border-color:var(--rd); color:var(--rd); }}
.rm-btn.armed {{ background:var(--rd); border-color:var(--rd); color:#fff; }}
.setting {{ display:flex; align-items:center; justify-content:space-between; padding:12px 14px; background:var(--sf); border:1px solid var(--border-subtle); border-radius:var(--r-md); margin-bottom:7px; }}
.setting-label {{ font-size:14px; font-weight:500; }}
.toggle {{ width:42px; height:24px; background:var(--hv); border-radius:var(--r-pill); cursor:pointer; position:relative; transition:background .2s; flex-shrink:0; }}
.toggle.on {{ background:var(--ac); }}
.toggle::after {{ content:''; width:18px; height:18px; background:#fff; border-radius:50%; position:absolute; top:3px; left:3px; transition:transform .2s; }}
.toggle.on::after {{ transform:translateX(18px); }}
.empty {{ color:var(--dm); font-size:13px; padding:8px 2px; }}
</style></head>
<body>
<h1>Settings</h1>
<p class="sub">Manage your services and preferences</p>
<h2>Services</h2>
<div id="svcs"></div>
<h2>General</h2>
<div id="settings"></div>
<script>
var services = {services};
var settings = {settings};
function sendIPC(msg) {{ window.location.href = 'omnichat-ipc://' + encodeURIComponent(JSON.stringify(msg)); }}
function renderServices() {{
    var el = document.getElementById('svcs');
    while(el.firstChild) el.removeChild(el.firstChild);
    if (!services.length) {{
        var e = document.createElement('div'); e.className = 'empty';
        e.textContent = 'No services yet. Click + in the sidebar to add one.';
        el.appendChild(e); return;
    }}
    services.forEach(function(s) {{
        var row = document.createElement('div'); row.className = 'svc-row';
        var ico = document.createElement('div'); ico.className = 'svc-ico';
        ico.textContent = (s.name || '?').charAt(0).toUpperCase();
        var name = document.createElement('span'); name.className = 'svc-name'; name.textContent = s.name;
        var recipe = document.createElement('span'); recipe.className = 'svc-recipe'; recipe.textContent = s.recipe_id;
        var btn = document.createElement('button'); btn.className = 'rm-btn'; btn.textContent = 'Remove';
        var armed = false;
        btn.addEventListener('click', function() {{
            if (!armed) {{
                armed = true; btn.textContent = 'Confirm?'; btn.classList.add('armed');
                setTimeout(function() {{ armed = false; btn.textContent = 'Remove'; btn.classList.remove('armed'); }}, 3000);
                return;
            }}
            sendIPC({{ type:'remove_service', serviceId:s.id }});
            services = services.filter(function(x){{ return x.id!==s.id; }}); renderServices();
        }});
        row.appendChild(ico); row.appendChild(name); row.appendChild(recipe); row.appendChild(btn);
        el.appendChild(row);
    }});
}}
function renderSettings() {{
    var el = document.getElementById('settings');
    var items = [
        ['Do Not Disturb', 'enable_dnd', settings.enable_dnd],
        ['Enable Hibernation', 'global_hibernation_enabled', settings.global_hibernation_enabled],
        ['Show Tray Icon', 'show_tray_icon', settings.show_tray_icon],
    ];
    items.forEach(function(item) {{
        var row = document.createElement('div'); row.className = 'setting';
        var label = document.createElement('span'); label.className = 'setting-label'; label.textContent = item[0];
        var toggle = document.createElement('div'); toggle.className = 'toggle' + (item[2] ? ' on' : '');
        toggle.addEventListener('click', function() {{
            toggle.classList.toggle('on');
            settings[item[1]] = toggle.classList.contains('on');
            sendIPC({{ type: 'update_settings', settings: settings }});
        }});
        row.appendChild(label); row.appendChild(toggle);
        el.appendChild(row);
    }});
}}
renderServices(); renderSettings();
</script></body></html>"#;
    // Collapse the {{ }} brace-escaping + fill placeholders FIRST, then inject
    // the shared theme (theme.css uses single braces, so it must go in after the
    // collapse). The /*__THEME__*/ sentinel has no braces, so it survives.
    let html = template
        .replace("{{", "{")
        .replace("}}", "}")
        .replace("{services}", services_json)
        .replace("{settings}", settings_json);
    crate::app::with_theme(&html)
}

/// Build the service picker HTML page.
fn build_picker_html(recipes_json: &str) -> String {
    let template = r#"<!DOCTYPE html>
<html data-theme="dark"><head><meta charset="UTF-8">
<style>
/*__THEME__*/
:root {{ --bg:var(--surface-base); --sf:var(--surface-raised); --hv:var(--surface-hover); --ac:var(--accent); --tx:var(--text-strong); --dm:var(--text-muted); --rd:var(--danger); --gn:var(--success); }}
body {{ font-family:var(--font); background:var(--bg); color:var(--tx); height:100vh; display:flex; flex-direction:column; -webkit-font-smoothing:antialiased; }}
.hdr {{ padding:32px 40px 16px; flex-shrink:0; }}
h1 {{ font-size:24px; font-weight:700; margin-bottom:4px; letter-spacing:-.01em; }}
.sub {{ color:var(--dm); font-size:13px; margin-bottom:16px; }}
.search {{ width:100%; max-width:480px; padding:11px 16px; border:1px solid var(--border-strong); border-radius:var(--r-md); background:var(--surface-sunken); color:var(--tx); font-size:14px; outline:none; transition:border-color .12s; }}
.search:focus {{ border-color:var(--ac); }}
.search::placeholder {{ color:var(--dm); }}
.grid {{ flex:1; overflow-y:auto; padding:8px 40px 40px; align-content:start; }}
.section-title {{ font-size:12px; font-weight:700; color:var(--dm); text-transform:uppercase; letter-spacing:.5px; padding:14px 0 6px; }}
.cards {{ display:grid; grid-template-columns:repeat(auto-fill,minmax(220px,1fr)); gap:6px; margin-bottom:8px; }}
.card {{ display:flex; align-items:center; gap:12px; padding:11px 12px; border-radius:var(--r-md); cursor:pointer; transition:background .1s; border:1px solid transparent; }}
.card:hover {{ background:var(--sf); border-color:var(--border-subtle); }}
.card-icon {{ width:38px; height:38px; border-radius:var(--r-md); background:var(--accent-soft); color:var(--ac); display:flex; align-items:center; justify-content:center; font-size:16px; font-weight:700; flex-shrink:0; }}
.card-name {{ font-size:13px; font-weight:600; }}
.card-url {{ font-size:11px; color:var(--dm); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; max-width:140px; }}
.team-modal {{ display:none; position:fixed; inset:0; background:rgba(0,0,0,.6); z-index:10; align-items:center; justify-content:center; }}
.team-modal.open {{ display:flex; }}
.team-box {{ background:var(--sf); padding:24px; border-radius:12px; width:340px; }}
.team-box h3 {{ font-size:15px; margin-bottom:12px; }}
.team-box input {{ width:100%; padding:8px 12px; border:1px solid var(--hv); border-radius:8px; background:var(--bg); color:var(--tx); font-size:13px; outline:none; margin-bottom:12px; }}
.team-box input:focus {{ border-color:var(--ac); }}
.btn-row {{ display:flex; gap:8px; }}
.btn {{ flex:1; padding:8px; border:none; border-radius:8px; cursor:pointer; font-size:13px; font-weight:600; }}
.btn-primary {{ background:var(--ac); color:var(--bg); }}
.btn-cancel {{ background:var(--hv); color:var(--tx); }}
</style></head>
<body>
<div class="hdr">
<h1>Add a service</h1>
<p class="sub">Choose from {count} available services</p>
<input class="search" id="q" placeholder="Search services..." autofocus>
</div>
<div class="grid" id="grid"></div>
<div class="team-modal" id="modal">
<div class="team-box">
<h3 id="modalTitle">Workspace name</h3>
<input id="modalInput" placeholder="e.g. mycompany">
<div class="btn-row">
<button class="btn btn-cancel" id="modalCancel">Cancel</button>
<button class="btn btn-primary" id="modalAdd">Add</button>
</div>
</div>
</div>
<script>
var recipes = {recipes};
var pending = null;
var POPULAR = ['whatsapp','slack','telegram','discord','messenger','gmail','instagram',
    'linkedin','skype','microsoft-teams','google-chat','signal','element','mattermost',
    'zoom','notion','github','chatgpt','twitter','reddit','twitch'];
function sendIPC(msg) {{
    window.location.href = 'omnichat-ipc://' + encodeURIComponent(JSON.stringify(msg));
}}
function makeCard(r) {{
        var c = document.createElement('div');
        c.className = 'card';
        var ic = document.createElement('div');
        ic.className = 'card-icon';
        ic.textContent = r.name.charAt(0).toUpperCase();
        c.appendChild(ic);
        var info = document.createElement('div');
        var n = document.createElement('div');
        n.className = 'card-name';
        n.textContent = r.name;
        info.appendChild(n);
        if (r.url) {{
            var u = document.createElement('div');
            u.className = 'card-url';
            u.textContent = r.url.replace('https://','').replace('http://','');
            info.appendChild(u);
        }}
        c.appendChild(info);
        c.addEventListener('click', function() {{
            if (r.hasTeamId) {{
                pending = r;
                document.getElementById('modalTitle').textContent = r.name + ' — workspace name';
                document.getElementById('modalInput').value = '';
                document.getElementById('modal').classList.add('open');
                document.getElementById('modalInput').focus();
            }} else {{
                sendIPC({{ type:'add_service', recipeId:r.id, name:r.name }});
            }}
        }});
        return c;
}}
function render(q) {{
    var g = document.getElementById('grid');
    while(g.firstChild) g.removeChild(g.firstChild);
    var all = recipes.filter(function(r) {{
        return !q || r.name.toLowerCase().indexOf(q)>-1 || r.id.toLowerCase().indexOf(q)>-1;
    }}).sort(function(a,b){{ return a.name.localeCompare(b.name); }});

    if (!q) {{
        // Show popular section first
        var pop = POPULAR.map(function(pid){{ return recipes.find(function(r){{return r.id===pid}}); }}).filter(Boolean);
        if (pop.length) {{
            var title = document.createElement('div'); title.className='section-title'; title.textContent='Popular'; g.appendChild(title);
            var cards = document.createElement('div'); cards.className='cards';
            pop.forEach(function(r){{ cards.appendChild(makeCard(r)); }});
            g.appendChild(cards);
        }}
        var title2 = document.createElement('div'); title2.className='section-title'; title2.textContent='All services'; g.appendChild(title2);
        var cards2 = document.createElement('div'); cards2.className='cards';
        all.slice(0,100).forEach(function(r){{ cards2.appendChild(makeCard(r)); }});
        g.appendChild(cards2);
    }} else {{
        var cards = document.createElement('div'); cards.className='cards';
        all.slice(0,80).forEach(function(r){{ cards.appendChild(makeCard(r)); }});
        g.appendChild(cards);
    }}
}}
document.getElementById('q').addEventListener('input', function() {{ render(this.value.toLowerCase()); }});
document.getElementById('modalCancel').addEventListener('click', function() {{ document.getElementById('modal').classList.remove('open'); }});
document.getElementById('modalAdd').addEventListener('click', function() {{
    if(pending) sendIPC({{ type:'add_service', recipeId:pending.id, name:pending.name, team:document.getElementById('modalInput').value.trim()||undefined }});
    document.getElementById('modal').classList.remove('open');
}});
document.getElementById('modalInput').addEventListener('keydown', function(e) {{ if(e.key==='Enter') document.getElementById('modalAdd').click(); }});
render('');
</script></body></html>"#;
    // Can't use format!() because recipes_json may contain {teamId} which breaks format strings.
    // The template uses {{ and }} for JS braces (format! escaping). Since we're now using
    // .replace() instead, we need to unescape them first.
    let count = recipes_json.matches("\"id\"").count();
    let html = template
        .replace("{{", "{")
        .replace("}}", "}")
        .replace("{recipes}", recipes_json)
        .replace("{count}", &count.to_string());
    // Inject the shared theme AFTER the brace-collapse (theme.css uses single
    // braces); the /*__THEME__*/ sentinel has no braces so it survives.
    crate::app::with_theme(&html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_allowlist_accepts_web_and_mail() {
        assert!(is_allowed_url("https://example.com/x"));
        assert!(is_allowed_url("http://example.com"));
        assert!(is_allowed_url("mailto:a@b.com"));
        assert!(is_allowed_url("  HTTPS://EXAMPLE.com ")); // trimmed + case-insensitive
    }

    #[test]
    fn url_allowlist_rejects_dangerous_schemes() {
        assert!(!is_allowed_url("file:///etc/passwd"));
        assert!(!is_allowed_url("javascript:alert(1)"));
        assert!(!is_allowed_url("data:text/html,x"));
        assert!(!is_allowed_url("sudo://x"));
        assert!(!is_allowed_url(""));
    }

    #[test]
    fn parses_badge_from_service() {
        let m: IpcMessage =
            serde_json::from_str(r#"{"type":"badge","serviceId":"s1","direct":3,"indirect":0}"#)
                .unwrap();
        match m {
            IpcMessage::Badge {
                service_id, direct, ..
            } => {
                assert_eq!(service_id, "s1");
                assert_eq!(direct, 3);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_add_service_with_optional_team() {
        let m: IpcMessage = serde_json::from_str(
            r#"{"type":"add_service","recipeId":"slack","name":"Work","team":"acme"}"#,
        )
        .unwrap();
        match m {
            IpcMessage::AddService {
                recipe_id,
                name,
                team,
                ..
            } => {
                assert_eq!(recipe_id, "slack");
                assert_eq!(name, "Work");
                assert_eq!(team.as_deref(), Some("acme"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_new_persistence_variants() {
        assert!(matches!(
            serde_json::from_str::<IpcMessage>(
                r#"{"type":"set_service_flag","serviceId":"s1","flag":"muted","value":true}"#
            )
            .unwrap(),
            IpcMessage::SetServiceFlag { .. }
        ));
        assert!(matches!(
            serde_json::from_str::<IpcMessage>(
                r#"{"type":"rename_service","serviceId":"s1","name":"X"}"#
            )
            .unwrap(),
            IpcMessage::RenameService { .. }
        ));
        assert!(matches!(
            serde_json::from_str::<IpcMessage>(r#"{"type":"open_picker"}"#).unwrap(),
            IpcMessage::OpenPicker {}
        ));
    }

    #[test]
    fn rejects_unknown_message_type() {
        assert!(serde_json::from_str::<IpcMessage>(r#"{"type":"definitely_not_real"}"#).is_err());
    }
}
