use cef::*;
use log::{debug, error, info};

use crate::app::SharedState;
use crate::recipe;
use crate::service::state::ServiceLifecycleState;

// --- Service load handler: injects recipe JS on page load ---

wrap_load_handler! {
    pub struct ServiceLoadHandler {
        state: SharedState,
    }

    impl LoadHandler {
        fn on_load_end(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: i32,
        ) {
            let Some(frame) = frame else { return };
            if frame.is_main() != 1 {
                return;
            }

            let browser = match browser {
                Some(b) => b,
                None => return,
            };

            let state = self.state.lock().unwrap();

            // Find which service this browser belongs to.
            let service_id = state
                .browsers
                .iter()
                .find(|(_, b)| {
                    let b_clone = (*b).clone();
                    b_clone.is_same(Some(browser)) != 0
                })
                .map(|(k, _)| k.clone());

            let Some(service_id) = service_id else {
                return;
            };

            let service = state
                .service_manager
                .services()
                .iter()
                .find(|s| s.id == service_id)
                .cloned();

            let Some(service) = service else {
                return;
            };

            let recipe_opt = state.recipes.get(&service.recipe_id).cloned();
            drop(state);

            let Some(recipe_data) = recipe_opt else {
                debug!("No recipe found for service {service_id}");
                return;
            };

            // Build and inject the composite JS.
            let composite_js =
                recipe::injector::build_injection_js(&service_id, &service, &recipe_data);

            info!(
                "Injecting recipe '{}' into service '{}'",
                recipe_data.id, service_id
            );

            let js = CefString::from(composite_js.as_str());
            let url = CefString::from("omnichat://injection");
            frame.execute_java_script(Some(&js), Some(&url), 0);

            // Start the lifecycle-aware poll timer for this service.
            start_poll_timer(browser, &service_id, self.state.clone());
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            error_code: Errorcode,
            _error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            let error_code_raw = sys::cef_errorcode_t::from(error_code);
            if error_code_raw == sys::cef_errorcode_t::ERR_ABORTED {
                return;
            }

            let failed_url = failed_url.map(CefString::to_string).unwrap_or_default();
            error!("Load error for URL: {failed_url}");

            if let Some(frame) = frame {
                if frame.is_main() == 1 {
                    let html = format!(
                        r#"<html><body style="font-family:system-ui;padding:40px;background:#1a1a2e;color:#eee">
                        <h2>Failed to load</h2><p>{failed_url}</p>
                        <p>Check your network connection and try again.</p>
                        </body></html>"#
                    );
                    let data_uri = format!(
                        "data:text/html;base64,{}",
                        crate::app::base64_encode_str(&html)
                    );
                    let url = CefString::from(data_uri.as_str());
                    frame.load_url(Some(&url));
                }
            }
        }
    }
}

/// Start a lifecycle-aware poll timer for a service.
/// - Active: polls every 2s
/// - Backgrounded: polls every 5s (still need badge counts)
/// - Frozen: no polling
/// - Hibernated: no polling (browser destroyed)
fn start_poll_timer(browser: &mut Browser, service_id: &str, state: SharedState) {
    let browser_clone = browser.clone();
    let service_id = service_id.to_string();

    let mut task = PollTask::new(browser_clone, service_id, state);
    post_delayed_task(ThreadId::UI, Some(&mut task), 2000);
}

wrap_task! {
    struct PollTask {
        browser: Browser,
        service_id: String,
        state: SharedState,
    }

    impl Task {
        fn execute(&self) {
            debug_assert_ne!(currently_on(ThreadId::UI), 0);

            // Check the service's lifecycle state to determine poll behavior.
            let (lifecycle, still_exists) = {
                let s = self.state.lock().unwrap();
                let exists = s.browsers.contains_key(&self.service_id);
                let lifecycle = s
                    .service_manager
                    .get_runtime(&self.service_id)
                    .map(|r| r.lifecycle)
                    .unwrap_or(ServiceLifecycleState::Hibernated);
                (lifecycle, exists)
            };

            // If the browser no longer exists (hibernated/removed), stop polling.
            if !still_exists {
                debug!("Poll timer stopped for {} (browser removed)", self.service_id);
                return;
            }

            match lifecycle {
                ServiceLifecycleState::Active => {
                    // Full-speed polling: execute recipe loop function.
                    self.execute_poll();
                    // Reschedule at 2s.
                    self.reschedule(2000);
                }
                ServiceLifecycleState::Backgrounded => {
                    // Background polling: still execute loop for badge counts and notifications.
                    self.execute_poll();
                    // Reschedule at 5s (slower).
                    self.reschedule(5000);
                }
                ServiceLifecycleState::Frozen => {
                    // No polling, but keep the timer alive to check for state changes.
                    // Check again in 10s in case the service gets unfrozen.
                    self.reschedule(10000);
                }
                ServiceLifecycleState::Hibernated => {
                    // Browser is destroyed, stop polling.
                    debug!("Poll timer stopped for {} (hibernated)", self.service_id);
                    return;
                }
            }
        }
    }
}

impl PollTask {
    fn execute_poll(&self) {
        if let Some(frame) = self.browser.main_frame() {
            let js = CefString::from(
                "if(window.__omnichat_ferdium && window.__omnichat_ferdium._loopFn) { try { window.__omnichat_ferdium._loopFn(); } catch(e) { console.error('[OmniChat poll]', e); } }"
            );
            let url = CefString::from("omnichat://poll");
            frame.execute_java_script(Some(&js), Some(&url), 0);
        }
    }

    fn reschedule(&self, delay_ms: i64) {
        let mut next = PollTask::new(
            self.browser.clone(),
            self.service_id.clone(),
            self.state.clone(),
        );
        post_delayed_task(ThreadId::UI, Some(&mut next), delay_ms);
    }
}

// --- Sidebar load handler ---

wrap_load_handler! {
    pub struct SidebarLoadHandler {
        state: SharedState,
    }

    impl LoadHandler {
        fn on_load_end(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: i32,
        ) {
            let Some(frame) = frame else { return };
            if frame.is_main() != 1 {
                return;
            }

            // Push initial service state + available recipes to the sidebar.
            let state = self.state.lock().unwrap();
            let services = state.service_manager.services().clone();
            let active_id = state.active_service_id.clone();

            // Build a compact recipe catalog: [{id, name, url}, ...]
            let recipe_catalog: Vec<serde_json::Value> = state
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
            drop(state);

            let services_json = serde_json::to_string(&services).unwrap_or_else(|_| "[]".into());
            let active_json = active_id
                .map(|id| format!("\"{}\"", id))
                .unwrap_or_else(|| "null".into());
            let recipes_json = serde_json::to_string(&recipe_catalog).unwrap_or_else(|_| "[]".into());

            let js = format!(
                "if(window.__omnichat_sidebar) {{ \
                    window.__omnichat_sidebar.updateServices({services_json}, {active_json}); \
                    window.__omnichat_sidebar.setRecipes({recipes_json}); \
                }}"
            );
            let js = CefString::from(js.as_str());
            let url = CefString::from("omnichat://sidebar-init");
            frame.execute_java_script(Some(&js), Some(&url), 0);

            info!("Sidebar state pushed");
        }
    }
}
