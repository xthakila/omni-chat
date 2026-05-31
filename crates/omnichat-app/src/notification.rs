use log::{debug, warn};

/// Show an OS notification. On Linux, clicking the notification switches OmniChat
/// to the originating service (`service_id`).
///
/// The show + action-wait runs on a dedicated thread: `wait_for_action` blocks
/// until the notification is actioned or closed, so it must never run on the CEF
/// UI thread. If the desktop's notification daemon doesn't support actions, the
/// click simply dismisses the notification (graceful degradation). The thread is
/// bounded by the notification timeout, so it can't leak indefinitely.
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
                    if service_id.is_empty() {
                        return;
                    }
                    // Blocks this (dedicated) thread until the notification is
                    // actioned or auto-closes; then exits.
                    handle.wait_for_action(|action| {
                        if action == "default" || action == "__clicked" {
                            crate::app::post_activate_service(service_id.clone());
                        }
                    });
                }
                #[cfg(not(target_os = "linux"))]
                let _ = (&handle, &service_id);
            }
            Err(e) => warn!("Failed to send notification: {e}"),
        }
    });
}
