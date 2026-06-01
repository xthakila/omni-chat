use cef::*;
use log::{debug, info};

use crate::app::SharedState;

/// Prefix marking a console.log line as an OmniChat IPC message from a service
/// page. The shim emits `console.log(SENTINEL + json)` instead of navigating a
/// hidden iframe to `omnichat-ipc://`: Chromium blocks subframe navigations to
/// the unregistered scheme at the renderer, so they never reach
/// `on_before_browse` and every badge/title/notification from a service page
/// was silently dropped. console.log runs in the page's main world (where the
/// shim is injected), fires `on_console_message` for any frame, and needs no
/// navigation, scheme registration, or CSP allowance.
const IPC_SENTINEL: &str = "__OMNICHAT_IPC__";

wrap_display_handler! {
    pub struct ServiceDisplayHandler {
        state: SharedState,
    }

    impl DisplayHandler {
        fn on_title_change(&self, _browser: Option<&mut Browser>, title: Option<&CefString>) {
            debug_assert_ne!(currently_on(ThreadId::UI), 0);
            let title_str = title.map(CefString::to_string).unwrap_or_default();
            debug!("Title changed: {title_str}");
        }

        fn on_favicon_urlchange(
            &self,
            _browser: Option<&mut Browser>,
            _icon_urls: Option<&mut CefStringList>,
        ) {
            debug!("Favicon changed");
        }

        fn on_console_message(
            &self,
            browser: Option<&mut Browser>,
            _level: LogSeverity,
            message: Option<&CefString>,
            source: Option<&CefString>,
            _line: i32,
        ) -> i32 {
            let msg = message.map(CefString::to_string).unwrap_or_default();

            // IPC channel: a service page emits `console.log(SENTINEL + json)`.
            // Capture the sender's browser id for the trust boundary, then defer
            // dispatch via post_task — handle_message touches CEF host/view ops
            // (activate service, set title, push sidebar JS) which must not run
            // re-entrantly inside this callback. Suppress the line so the IPC
            // payload is never echoed to the console/log.
            if let Some(json) = msg.strip_prefix(IPC_SENTINEL) {
                let sender_id = browser.as_deref().map(|b| b.identifier());
                let mut task = ConsoleIpcTask::new(self.state.clone(), json.to_string(), sender_id);
                post_task(ThreadId::UI, Some(&mut task));
                return 1; // Suppress: consumed as IPC.
            }

            let src = source.map(CefString::to_string).unwrap_or_default();
            if msg.contains("[Sidebar]") || msg.contains("[OmniChat]") || msg.contains("cefQuery") || msg.contains("Error") || msg.contains("error") || src.contains("omnichat") || src.contains("data:") {
                info!("JS: {msg} ({src})");
            }
            0 // Don't suppress
        }
    }
}

// Deferred IPC task — runs handle_message on the UI thread AFTER
// on_console_message returns, to avoid re-entrant CEF host/view operations.
// Mirrors request.rs's IpcTask (the omnichat-ipc:// path), kept separate so the
// two transports stay independently removable.
wrap_task! {
    struct ConsoleIpcTask {
        state: SharedState,
        json: String,
        browser_id: Option<i32>,
    }

    impl Task {
        fn execute(&self) {
            crate::ipc::handler::handle_message(&self.state, &self.json, self.browser_id);
        }
    }
}
