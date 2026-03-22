use log::{debug, warn};

/// Show an OS notification.
pub fn show(service_name: &str, title: &str, body: &str) {
    debug!("Notification: [{service_name}] {title}: {body}");

    let summary = if service_name.is_empty() {
        title.to_string()
    } else {
        format!("{service_name}: {title}")
    };

    match notify_rust::Notification::new()
        .summary(&summary)
        .body(body)
        .appname("OmniChat")
        .timeout(notify_rust::Timeout::Milliseconds(5000))
        .show()
    {
        Ok(_) => debug!("Notification sent"),
        Err(e) => warn!("Failed to send notification: {e}"),
    }
}
