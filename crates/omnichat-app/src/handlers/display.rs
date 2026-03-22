use cef::*;
use log::{debug, info};

use crate::app::SharedState;

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
            _browser: Option<&mut Browser>,
            _level: LogSeverity,
            message: Option<&CefString>,
            source: Option<&CefString>,
            _line: i32,
        ) -> i32 {
            let msg = message.map(CefString::to_string).unwrap_or_default();
            let src = source.map(CefString::to_string).unwrap_or_default();
            if msg.contains("[Sidebar]") || msg.contains("[OmniChat]") || msg.contains("cefQuery") || msg.contains("Error") || msg.contains("error") || src.contains("omnichat") || src.contains("data:") {
                info!("JS: {msg} ({src})");
            }
            0 // Don't suppress
        }
    }
}
