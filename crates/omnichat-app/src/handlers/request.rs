use cef::*;
use log::{debug, info};

use crate::app::SharedState;

wrap_request_handler! {
    pub struct ServiceRequestHandler {
        state: SharedState,
    }

    impl RequestHandler {
        fn on_before_browse(
            &self,
            browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _user_gesture: i32,
            _is_redirect: i32,
        ) -> i32 {
            let Some(request) = request else { return 0 };
            let url = CefString::from(&request.url()).to_string();

            // Intercept omnichat-ipc:// URLs.
            if url.starts_with("omnichat-ipc://") {
                let encoded = url.trim_start_matches("omnichat-ipc://");
                if let Ok(json) = urlparse_decode(encoded) {
                    info!("IPC via URL scheme: {}", &json[..json.len().min(100)]);
                    // Capture which browser sent this (trust boundary), before
                    // deferring: a service webview can navigate to omnichat-ipc://
                    // just like our own pages, so the sender must be classified.
                    let sender_id = browser.as_deref().map(|b| b.identifier());
                    // IMPORTANT: defer IPC processing via post_task to avoid
                    // re-entrant CEF UI thread operations (deadlock).
                    let state = self.state.clone();
                    let mut task = IpcTask::new(state, json, sender_id);
                    post_task(ThreadId::UI, Some(&mut task));
                }
                return 1; // Cancel navigation.
            }

            0
        }

        fn on_open_urlfrom_tab(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            target_url: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: i32,
        ) -> i32 {
            if let Some(url) = target_url {
                let url_str = CefString::to_string(url);
                debug!("Open URL from tab: {url_str}");
            }
            0
        }
    }
}

// Deferred IPC task — runs after on_before_browse returns.
wrap_task! {
    struct IpcTask {
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

/// Decode a percent-encoded string.
fn urlparse_decode(encoded: &str) -> Result<String, std::string::FromUtf8Error> {
    let mut bytes = Vec::new();
    let mut chars = encoded.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars.next().unwrap_or(b'0');
            let h2 = chars.next().unwrap_or(b'0');
            let hex = [h1, h2];
            if let Ok(s) = std::str::from_utf8(&hex) {
                if let Ok(byte) = u8::from_str_radix(s, 16) {
                    bytes.push(byte);
                    continue;
                }
            }
            bytes.push(b'%');
            bytes.push(h1);
            bytes.push(h2);
        } else if b == b'+' {
            bytes.push(b' ');
        } else {
            bytes.push(b);
        }
    }
    String::from_utf8(bytes)
}

#[cfg(test)]
mod tests {
    use super::urlparse_decode;

    #[test]
    fn decodes_percent_encoded_json() {
        // What the JS sendIPC produces: encodeURIComponent(JSON.stringify(...)).
        let encoded = "%7B%22type%22%3A%22open_picker%22%7D";
        assert_eq!(
            urlparse_decode(encoded).unwrap(),
            r#"{"type":"open_picker"}"#
        );
    }

    #[test]
    fn plus_becomes_space() {
        assert_eq!(urlparse_decode("a+b").unwrap(), "a b");
    }

    #[test]
    fn passes_through_plain_text() {
        assert_eq!(urlparse_decode("hello").unwrap(), "hello");
    }

    #[test]
    fn handles_unicode_escape() {
        assert_eq!(urlparse_decode("%E2%9C%93").unwrap(), "✓");
    }
}
