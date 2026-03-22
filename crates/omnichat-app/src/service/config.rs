use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub id: String,
    pub recipe_id: String,
    pub name: String,
    #[serde(default)]
    pub team: Option<String>,
    #[serde(default)]
    pub custom_url: Option<String>,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default = "default_true")]
    pub is_enabled: bool,
    #[serde(default = "default_true")]
    pub is_notification_enabled: bool,
    #[serde(default = "default_true")]
    pub is_badge_enabled: bool,
    #[serde(default)]
    pub is_muted: bool,
    #[serde(default)]
    pub is_dark_mode_enabled: bool,
    #[serde(default = "default_true")]
    pub is_hibernation_enabled: bool,
    #[serde(default)]
    pub proxy: Option<String>,
}

fn default_true() -> bool {
    true
}

impl ServiceConfig {
    pub fn new(id: String, recipe_id: String, name: String) -> Self {
        Self {
            id,
            recipe_id,
            name,
            team: None,
            custom_url: None,
            sort_order: 0,
            is_enabled: true,
            is_notification_enabled: true,
            is_badge_enabled: true,
            is_muted: false,
            is_dark_mode_enabled: false,
            is_hibernation_enabled: true,
            proxy: None,
        }
    }

    /// Returns the URL to load for this service.
    /// Uses custom_url if set, otherwise falls back to the recipe's serviceURL.
    pub fn effective_url(&self) -> String {
        if let Some(ref url) = self.custom_url {
            if !url.is_empty() {
                return url.clone();
            }
        }
        // Fallback: generic URL from recipe ID.
        format!("https://{}.com", self.recipe_id)
    }

    /// Returns the URL resolved against a recipe's service_url with {teamId} substitution.
    pub fn effective_url_with_recipe(&self, recipe_service_url: &str) -> String {
        // 1. Custom URL always wins.
        if let Some(ref url) = self.custom_url {
            if !url.is_empty() {
                return url.clone();
            }
        }

        // 2. Recipe service URL with {teamId} substitution.
        if !recipe_service_url.is_empty() {
            let mut url = recipe_service_url.to_string();
            if let Some(ref team) = self.team {
                url = url.replace("{teamId}", team);
            }
            // If {teamId} wasn't substituted (no team set), strip it.
            url = url.replace("{teamId}", "");
            return url;
        }

        // 3. Final fallback.
        self.effective_url()
    }
}
