use serde::{Deserialize, Serialize};

/// A Ferdium-compatible recipe, parsed from package.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// Path to the recipe directory on disk.
    #[serde(skip)]
    pub path: String,

    // --- config section (from package.json "config") ---
    #[serde(default)]
    pub service_url: String,
    #[serde(default = "default_true")]
    pub has_direct_messages: bool,
    #[serde(default)]
    pub has_indirect_messages: bool,
    #[serde(default)]
    pub has_notification_sound: bool,
    #[serde(default)]
    pub has_team_id: bool,
    #[serde(default)]
    pub has_custom_url: bool,
    #[serde(default)]
    pub has_hosted_option: bool,
    #[serde(default)]
    pub url_input_prefix: String,
    #[serde(default)]
    pub url_input_suffix: String,
    #[serde(default)]
    pub disable_web_security: bool,
    #[serde(default)]
    pub message: String,

    /// Contents of webview.js (loaded on first use).
    #[serde(skip)]
    pub webview_js: Option<String>,
    /// Contents of darkmode.css if it exists.
    #[serde(skip)]
    pub darkmode_css: Option<String>,
}

fn default_true() -> bool {
    true
}

impl Recipe {
    pub fn effective_url(&self, custom_url: Option<&str>, team: Option<&str>) -> String {
        if let Some(url) = custom_url {
            if !url.is_empty() {
                return url.to_string();
            }
        }

        let mut url = self.service_url.clone();

        // Replace {teamId} placeholder if present.
        if let Some(team) = team {
            url = url.replace("{teamId}", team);
        }

        url
    }
}
