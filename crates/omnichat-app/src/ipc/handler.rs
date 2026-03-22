use cef::wrapper::message_router::*;
use cef::{Browser, Frame, ImplBrowser, ImplBrowserHost, ImplFrame};
use log::{debug, info, warn};
use serde::Deserialize;
use std::sync::{Arc, Mutex};

use crate::app::SharedState;
use crate::notification;

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
        _browser: Option<Browser>,
        _frame: Option<Frame>,
        _query_id: i64,
        request: &str,
        _persistent: bool,
        callback: Arc<Mutex<dyn BrowserSideCallback>>,
    ) -> bool {
        info!("IPC received: {}", &request[..request.len().min(100)]);
        handle_message(&self.state, request);
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
}

/// Handle an IPC message from a service webview.
pub fn handle_message(state: &SharedState, raw: &str) {
    let msg: IpcMessage = match serde_json::from_str(raw) {
        Ok(m) => m,
        Err(e) => {
            warn!("Invalid IPC message: {e} — raw: {raw}");
            return;
        }
    };

    match msg {
        IpcMessage::Badge {
            service_id,
            direct,
            indirect,
        } => {
            debug!("Badge update: {service_id} direct={direct} indirect={indirect}");
            let mut s = state.lock().unwrap();
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
            let s = state.lock().unwrap();

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
            let mut s = state.lock().unwrap();
            let title_opt = if title.is_empty() {
                None
            } else {
                Some(title)
            };
            s.service_manager.set_dialog_title(&service_id, title_opt);
        }

        IpcMessage::Avatar { service_id, url } => {
            debug!("Avatar update: {service_id} = {url}");
            // Could cache the avatar for sidebar display.
        }

        IpcMessage::OpenUrl { url } => {
            info!("Opening URL in system browser: {url}");
            let _ = open::that(&url);
        }

        IpcMessage::ActivateService { service_id } => {
            info!("Activating service: {service_id}");

            // Background the previously active service.
            {
                let mut s = state.lock().unwrap();
                if let Some(ref prev_id) = s.active_service_id.clone() {
                    if prev_id != &service_id {
                        s.service_manager
                            .set_lifecycle_state(prev_id, crate::service::state::ServiceLifecycleState::Backgrounded);
                    }
                }
            }

            // If the service doesn't have a BrowserView yet, create one.
            let needs_creation = {
                let s = state.lock().unwrap();
                !s.browser_views.contains_key(&service_id)
            };
            if needs_creation {
                crate::app::create_service_browser_view(state, &service_id);
            }

            // Swap the displayed view.
            crate::app::swap_content_view(state, &service_id);

            let s = state.lock().unwrap();
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
                let mut s = state.lock().unwrap();
                let sort_order = s.service_manager.services().len() as i32;
                config.sort_order = sort_order;
                s.service_manager.add_service(config.clone());
                let _ = crate::db::queries::save_service(&s.db, &config);
            }

            // Create a BrowserView for the new service.
            crate::app::create_service_browser_view(state, &id);

            // Switch to the new service immediately.
            crate::app::swap_content_view(state, &id);

            let s = state.lock().unwrap();
            push_sidebar_state(&s);
        }

        IpcMessage::RemoveService { service_id } => {
            info!("Removing service: {service_id}");
            let mut s = state.lock().unwrap();

            // Close the browser if open.
            if let Some(browser) = s.browsers.get(&service_id).cloned() {
                if let Some(host) = browser.host() {
                    host.close_browser(1);
                }
            }
            s.browsers.remove(&service_id);
            s.service_manager.remove_service(&service_id);

            // Persist deletion.
            let _ = crate::db::queries::delete_service(&s.db, &service_id);
            push_sidebar_state(&s);
        }

        IpcMessage::OpenSettings {} => {
            info!("Opening settings");
            let s = state.lock().unwrap();
            let services_json = serde_json::to_string(s.service_manager.services()).unwrap_or_else(|_| "[]".into());
            let settings_json = serde_json::to_string(&s.settings).unwrap_or_else(|_| "{}".into());
            let browser = s
                .displayed_service_id
                .as_ref()
                .and_then(|id| s.browsers.get(id))
                .cloned();
            drop(s);

            if let Some(browser) = browser {
                if let Some(frame) = browser.main_frame() {
                    let html = build_settings_html(&services_json, &settings_json);
                    let data_uri = format!("data:text/html;base64,{}", crate::app::base64_encode_str(&html));
                    let url = cef::CefString::from(data_uri.as_str());
                    frame.load_url(Some(&url));
                }
            }
        }

        IpcMessage::OpenPicker {} => {
            info!("Opening service picker");
            // Load the picker page in the content area by navigating the current
            // content browser to a data: URI with the picker HTML.
            let s = state.lock().unwrap();
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
            let recipes_json = serde_json::to_string(&recipe_catalog).unwrap_or_else(|_| "[]".into());

            // Find the active content browser and navigate it to the picker.
            let browser = s
                .displayed_service_id
                .as_ref()
                .and_then(|id| s.browsers.get(id))
                .cloned();
            drop(s);

            if let Some(browser) = browser {
                if let Some(frame) = browser.main_frame() {
                    let picker_html = build_picker_html(&recipes_json);
                    let data_uri = format!(
                        "data:text/html;base64,{}",
                        crate::app::base64_encode_str(&picker_html)
                    );
                    let url = cef::CefString::from(data_uri.as_str());
                    frame.load_url(Some(&url));
                }
            }
        }

        IpcMessage::ReorderServices { service_ids } => {
            debug!("Reordering services: {service_ids:?}");
            let mut s = state.lock().unwrap();
            for (i, id) in service_ids.iter().enumerate() {
                if let Some(config) = s.service_manager.get_config_mut(id) {
                    config.sort_order = i as i32;
                }
            }
            // Persist updated order.
            for svc in s.service_manager.services() {
                let _ = crate::db::queries::save_service(&s.db, svc);
            }
            push_sidebar_state(&s);
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

    let services_json =
        serde_json::to_string(state.service_manager.services()).unwrap_or_else(|_| "[]".into());
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
<html><head><meta charset="UTF-8">
<style>
:root {{ --bg:#1e1e2e; --sf:#313244; --hv:#45475a; --ac:#cba6f7; --tx:#cdd6f4; --dm:#6c7086; --rd:#f38ba8; }}
* {{ margin:0; padding:0; box-sizing:border-box; }}
body {{ font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif; background:var(--bg); color:var(--tx); padding:32px 40px; }}
h1 {{ font-size:22px; font-weight:700; margin-bottom:24px; }}
h2 {{ font-size:15px; font-weight:600; color:var(--ac); margin:24px 0 12px; }}
.svc-row {{ display:flex; align-items:center; padding:10px 14px; background:var(--sf); border-radius:8px; margin-bottom:6px; gap:12px; }}
.svc-name {{ flex:1; font-size:13px; font-weight:500; }}
.svc-recipe {{ color:var(--dm); font-size:11px; }}
.rm-btn {{ background:none; border:1px solid rgba(243,139,168,.3); color:var(--rd); border-radius:6px; padding:4px 10px; font-size:11px; cursor:pointer; }}
.rm-btn:hover {{ background:rgba(243,139,168,.1); }}
.setting {{ display:flex; align-items:center; justify-content:space-between; padding:10px 14px; background:var(--sf); border-radius:8px; margin-bottom:6px; }}
.setting-label {{ font-size:13px; }}
.toggle {{ width:40px; height:22px; background:var(--hv); border-radius:11px; cursor:pointer; position:relative; transition:background .2s; }}
.toggle.on {{ background:var(--ac); }}
.toggle::after {{ content:''; width:18px; height:18px; background:#fff; border-radius:50%; position:absolute; top:2px; left:2px; transition:transform .2s; }}
.toggle.on::after {{ transform:translateX(18px); }}
</style></head>
<body>
<h1>Settings</h1>
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
    services.forEach(function(s) {{
        var row = document.createElement('div'); row.className = 'svc-row';
        var name = document.createElement('span'); name.className = 'svc-name'; name.textContent = s.name;
        var recipe = document.createElement('span'); recipe.className = 'svc-recipe'; recipe.textContent = s.recipe_id;
        var btn = document.createElement('button'); btn.className = 'rm-btn'; btn.textContent = 'Remove';
        btn.addEventListener('click', function() {{ sendIPC({{ type:'remove_service', serviceId:s.id }}); services = services.filter(function(x){{return x.id!==s.id}}); renderServices(); }});
        row.appendChild(name); row.appendChild(recipe); row.appendChild(btn);
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
        toggle.addEventListener('click', function() {{ toggle.classList.toggle('on'); }});
        row.appendChild(label); row.appendChild(toggle);
        el.appendChild(row);
    }});
}}
renderServices(); renderSettings();
</script></body></html>"#;
    template
        .replace("{{", "{")
        .replace("}}", "}")
        .replace("{services}", services_json)
        .replace("{settings}", settings_json)
}

/// Build the service picker HTML page.
fn build_picker_html(recipes_json: &str) -> String {
    let template = r#"<!DOCTYPE html>
<html><head><meta charset="UTF-8">
<style>
:root {{ --bg:#1e1e2e; --sf:#313244; --hv:#45475a; --ac:#cba6f7; --tx:#cdd6f4; --dm:#6c7086; --rd:#f38ba8; --gn:#a6e3a1; }}
* {{ margin:0; padding:0; box-sizing:border-box; }}
body {{ font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif; background:var(--bg); color:var(--tx); height:100vh; display:flex; flex-direction:column; }}
.hdr {{ padding:32px 40px 16px; flex-shrink:0; }}
h1 {{ font-size:22px; font-weight:700; margin-bottom:4px; }}
.sub {{ color:var(--dm); font-size:13px; margin-bottom:16px; }}
.search {{ width:100%; max-width:480px; padding:10px 16px; border:1px solid var(--hv); border-radius:10px; background:var(--sf); color:var(--tx); font-size:14px; outline:none; }}
.search:focus {{ border-color:var(--ac); }}
.search::placeholder {{ color:var(--dm); }}
.grid {{ flex:1; overflow-y:auto; padding:8px 40px 40px; align-content:start; }}
.section-title {{ font-size:12px; font-weight:700; color:var(--dm); text-transform:uppercase; letter-spacing:.5px; padding:12px 0 6px; }}
.cards {{ display:grid; grid-template-columns:repeat(auto-fill,minmax(220px,1fr)); gap:8px; margin-bottom:8px; }}
.card {{ display:flex; align-items:center; gap:12px; padding:12px 14px; border-radius:10px; cursor:pointer; transition:background .1s; }}
.card:hover {{ background:var(--sf); }}
.card-icon {{ width:36px; height:36px; border-radius:10px; background:var(--hv); display:flex; align-items:center; justify-content:center; font-size:16px; font-weight:700; flex-shrink:0; }}
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
    template
        .replace("{{", "{")
        .replace("}}", "}")
        .replace("{recipes}", recipes_json)
        .replace("{count}", &count.to_string())
}
