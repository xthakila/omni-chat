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
        }
    }
}

impl AppSettings {
    /// Load settings from the database.
    pub fn load(conn: &rusqlite::Connection) -> Self {
        crate::db::queries::load_all_settings(conn)
    }

    /// Save all settings to the database.
    pub fn save(&self, conn: &rusqlite::Connection) {
        let _ = crate::db::queries::save_setting(conn, "show_tray_icon", &self.show_tray_icon.to_string());
        let _ = crate::db::queries::save_setting(conn, "enable_system_tray", &self.enable_system_tray.to_string());
        let _ = crate::db::queries::save_setting(conn, "minimize_to_tray", &self.minimize_to_tray.to_string());
        let _ = crate::db::queries::save_setting(conn, "close_to_tray", &self.close_to_tray.to_string());
        let _ = crate::db::queries::save_setting(conn, "start_minimized", &self.start_minimized.to_string());
        let _ = crate::db::queries::save_setting(conn, "enable_dnd", &self.enable_dnd.to_string());
        let _ = crate::db::queries::save_setting(conn, "global_hibernation_enabled", &self.global_hibernation_enabled.to_string());
    }
}
