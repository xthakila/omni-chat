use cef::wrapper::message_router::*;
use cef::*;
use log::info;

use crate::app::SharedState;

// --- Service browser life span handler ---

wrap_life_span_handler! {
    pub struct ServiceLifeSpanHandler {
        state: SharedState,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            debug_assert_ne!(currently_on(ThreadId::UI), 0);

            let browser = browser.cloned().expect("Browser is None");
            let mut state = self.state.lock().unwrap();

            // Match browser to service by pending_service_ids order.
            // Each browser_host_create_browser call fires on_after_created in order.
            let service_id = if !state.pending_service_ids.is_empty() {
                Some(state.pending_service_ids.remove(0))
            } else {
                // Fallback: try URL matching.
                let main_frame = browser.main_frame();
                let url = main_frame
                    .as_ref()
                    .map(|f| CefString::from(&f.url()).to_string())
                    .unwrap_or_default();

                state
                    .service_manager
                    .services()
                    .iter()
                    .find(|s| {
                        let svc_url = s.effective_url();
                        !state.browsers.contains_key(&s.id)
                            && (url.contains(&svc_url) || svc_url.contains(&url))
                    })
                    .map(|s| s.id.clone())
            };

            if let Some(id) = &service_id {
                info!("Browser created for service: {id}");
                state.browsers.insert(id.clone(), browser.clone());

                // If not the active service, hide it.
                if state.active_service_id.as_ref() != Some(id) {
                    if let Some(host) = browser.host() {
                        host.was_hidden(1);
                    }
                }
            } else {
                let main_frame = browser.main_frame();
                let url = main_frame
                    .as_ref()
                    .map(|f| CefString::from(&f.url()).to_string())
                    .unwrap_or_default();
                info!("Browser created (unmatched): {url}");
            }
        }

        fn do_close(&self, _browser: Option<&mut Browser>) -> i32 {
            0 // Allow close
        }

        fn on_before_close(&self, browser: Option<&mut Browser>) {
            debug_assert_ne!(currently_on(ThreadId::UI), 0);

            let mut browser = browser.cloned().expect("Browser is None");
            let mut state = self.state.lock().unwrap();

            // Notify the MessageRouter that this browser is closing.
            if let Some(ref router) = state.message_router {
                router.on_before_close(Some(browser.clone()));
            }

            // Remove from browsers map.
            let to_remove: Option<String> = state
                .browsers
                .iter()
                .find(|(_, b)| {
                    let b_clone = (*b).clone();
                    b_clone.is_same(Some(&mut browser)) != 0
                })
                .map(|(k, _)| k.clone());

            if let Some(id) = to_remove {
                state.browsers.remove(&id);
                info!("Browser closed for service: {id}");
            }

            if state.browsers.is_empty() && state.sidebar_browser.is_none() {
                quit_message_loop();
            }
        }
    }
}

// --- Sidebar browser life span handler ---

wrap_life_span_handler! {
    pub struct SidebarLifeSpanHandler {
        state: SharedState,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut Browser>) {
            debug_assert_ne!(currently_on(ThreadId::UI), 0);

            let browser = browser.cloned().expect("Browser is None");
            let mut state = self.state.lock().unwrap();
            state.sidebar_browser = Some(browser);
            info!("Sidebar browser created");
        }

        fn do_close(&self, _browser: Option<&mut Browser>) -> i32 {
            0
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            debug_assert_ne!(currently_on(ThreadId::UI), 0);

            let mut state = self.state.lock().unwrap();
            state.sidebar_browser = None;
            info!("Sidebar browser closed");

            if state.browsers.is_empty() {
                quit_message_loop();
            }
        }
    }
}
