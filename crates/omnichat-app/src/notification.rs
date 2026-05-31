use log::{debug, warn};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Concurrent notification threads currently blocked in `wait_for_action`. A
/// service spamming notifications (e.g. a JS loop) could otherwise spawn an
/// unbounded number of ~5s-lived threads; past the cap we still SHOW the
/// notification but skip the action-wait so no extra thread lingers.
static ACTIVE_WAITERS: AtomicUsize = AtomicUsize::new(0);
const MAX_WAITERS: usize = 24;

/// Show an OS notification. On Linux, clicking the notification switches OmniChat
/// to the originating service (`service_id`).
///
/// The show + action-wait runs on a dedicated thread: `wait_for_action` blocks
/// until the notification is actioned or closed, so it must never run on the CEF
/// UI thread. If the desktop's notification daemon doesn't support actions, the
/// click simply dismisses the notification (graceful degradation). The thread is
/// bounded by the notification timeout and the MAX_WAITERS cap.
pub fn show(service_id: &str, service_name: &str, title: &str, body: &str) {
    debug!("Notification: [{service_name}] {title}: {body}");

    let summary = if service_name.is_empty() {
        title.to_string()
    } else {
        format!("{service_name}: {title}")
    };
    let body = body.to_string();
    let service_id = service_id.to_string();

    std::thread::spawn(move || {
        let mut builder = notify_rust::Notification::new();
        builder
            .summary(&summary)
            .body(&body)
            .appname("OmniChat")
            .timeout(notify_rust::Timeout::Milliseconds(5000));

        // "default" is the implicit action fired by clicking the notification
        // body — on common daemons (GNOME, KDE) it does NOT render an extra
        // button. Linux/dbus only; other platforms stay fire-and-forget.
        #[cfg(target_os = "linux")]
        builder.action("default", "Open");

        match builder.show() {
            Ok(handle) => {
                debug!("Notification sent");
                #[cfg(target_os = "linux")]
                {
                    // No service to focus -> nothing to wait for.
                    if service_id.is_empty() {
                        return;
                    }
                    // Claim a waiter slot; over the cap, show-and-exit (no wait)
                    // so a notification burst can't spawn unbounded blocked threads.
                    if ACTIVE_WAITERS.fetch_add(1, Ordering::Relaxed) >= MAX_WAITERS {
                        ACTIVE_WAITERS.fetch_sub(1, Ordering::Relaxed);
                        return;
                    }
                    // Blocks this (dedicated) thread until the notification is
                    // actioned or auto-closes; then frees the slot.
                    handle.wait_for_action(|action| {
                        if action == "default" || action == "__clicked" {
                            crate::app::post_activate_service(service_id.clone());
                        }
                    });
                    ACTIVE_WAITERS.fetch_sub(1, Ordering::Relaxed);
                }
                #[cfg(not(target_os = "linux"))]
                let _ = (&handle, &service_id);
            }
            Err(e) => warn!("Failed to send notification: {e}"),
        }
    });
}
