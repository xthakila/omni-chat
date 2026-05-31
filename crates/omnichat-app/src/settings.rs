use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub show_tray_icon: bool,
    pub enable_system_tray: bool,
    pub minimize_to_tray: bool,
    pub close_to_tray: bool,
    pub start_minimized: bool,
    pub enable_dnd: bool,
    pub global_hibernation_enabled: bool,
    /// Content-area theme: "auto" (follow OS), "light", or "dark". The sidebar
    /// rail is always dark chrome regardless of this setting.
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_theme() -> String {
    "auto".to_string()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            show_tray_icon: true,
            enable_system_tray: true,
            minimize_to_tray: false,
            close_to_tray: false,
            start_minimized: false,
            enable_dnd: false,
            global_hibernation_enabled: true,
            theme: default_theme(),
        }
    }
}

impl AppSettings {
    /// Load settings from the database.
    pub fn load(conn: &rusqlite::Connection) -> Self {
        crate::db::queries::load_all_settings(conn)
    }

    /// Save all settings to the database. Errors are logged, not silently dropped.
    pub fn save(&self, conn: &rusqlite::Connection) {
        let kv: [(&str, String); 8] = [
            ("show_tray_icon", self.show_tray_icon.to_string()),
            ("enable_system_tray", self.enable_system_tray.to_string()),
            ("minimize_to_tray", self.minimize_to_tray.to_string()),
            ("close_to_tray", self.close_to_tray.to_string()),
            ("start_minimized", self.start_minimized.to_string()),
            ("enable_dnd", self.enable_dnd.to_string()),
            (
                "global_hibernation_enabled",
                self.global_hibernation_enabled.to_string(),
            ),
            ("theme", self.theme.clone()),
        ];
        for (key, value) in &kv {
            crate::db::warn_on_err(
                "save_setting",
                crate::db::queries::save_setting(conn, key, value),
            );
        }
    }
}
