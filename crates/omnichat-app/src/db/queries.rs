use rusqlite::{params, Connection};

use crate::service::config::ServiceConfig;
use crate::settings::AppSettings;

/// Load all services from the database.
pub fn load_services(conn: &Connection) -> Result<Vec<ServiceConfig>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, recipe_id, name, team, custom_url, sort_order,
                is_enabled, is_notification_enabled, is_badge_enabled,
                is_muted, is_dark_mode_enabled, is_hibernation_enabled, proxy
         FROM services ORDER BY sort_order",
    )?;

    let services = stmt.query_map([], |row| {
        Ok(ServiceConfig {
            id: row.get(0)?,
            recipe_id: row.get(1)?,
            name: row.get(2)?,
            team: row.get(3)?,
            custom_url: row.get(4)?,
            sort_order: row.get(5)?,
            is_enabled: row.get::<_, i32>(6)? != 0,
            is_notification_enabled: row.get::<_, i32>(7)? != 0,
            is_badge_enabled: row.get::<_, i32>(8)? != 0,
            is_muted: row.get::<_, i32>(9)? != 0,
            is_dark_mode_enabled: row.get::<_, i32>(10)? != 0,
            is_hibernation_enabled: row.get::<_, i32>(11)? != 0,
            proxy: row.get(12)?,
        })
    })?;

    services.collect()
}

/// Insert or update a service.
pub fn save_service(conn: &Connection, svc: &ServiceConfig) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO services (id, recipe_id, name, team, custom_url, sort_order,
            is_enabled, is_notification_enabled, is_badge_enabled, is_muted,
            is_dark_mode_enabled, is_hibernation_enabled, proxy, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, datetime('now'))
         ON CONFLICT(id) DO UPDATE SET
            recipe_id=excluded.recipe_id, name=excluded.name, team=excluded.team,
            custom_url=excluded.custom_url, sort_order=excluded.sort_order,
            is_enabled=excluded.is_enabled,
            is_notification_enabled=excluded.is_notification_enabled,
            is_badge_enabled=excluded.is_badge_enabled, is_muted=excluded.is_muted,
            is_dark_mode_enabled=excluded.is_dark_mode_enabled,
            is_hibernation_enabled=excluded.is_hibernation_enabled,
            proxy=excluded.proxy, updated_at=datetime('now')",
        params![
            svc.id,
            svc.recipe_id,
            svc.name,
            svc.team,
            svc.custom_url,
            svc.sort_order,
            svc.is_enabled as i32,
            svc.is_notification_enabled as i32,
            svc.is_badge_enabled as i32,
            svc.is_muted as i32,
            svc.is_dark_mode_enabled as i32,
            svc.is_hibernation_enabled as i32,
            svc.proxy,
        ],
    )?;
    Ok(())
}

/// Delete a service.
pub fn delete_service(conn: &Connection, id: &str) -> Result<(), rusqlite::Error> {
    conn.execute("DELETE FROM services WHERE id = ?1", params![id])?;
    Ok(())
}

/// Load a setting value.
pub fn load_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .ok()
}

/// Save a setting value.
pub fn save_setting(conn: &Connection, key: &str, value: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// Load all app settings into an AppSettings struct.
pub fn load_all_settings(conn: &Connection) -> AppSettings {
    let mut settings = AppSettings::default();

    if let Some(v) = load_setting(conn, "show_tray_icon") {
        settings.show_tray_icon = v == "true";
    }
    if let Some(v) = load_setting(conn, "enable_system_tray") {
        settings.enable_system_tray = v == "true";
    }
    if let Some(v) = load_setting(conn, "minimize_to_tray") {
        settings.minimize_to_tray = v == "true";
    }
    if let Some(v) = load_setting(conn, "close_to_tray") {
        settings.close_to_tray = v == "true";
    }
    if let Some(v) = load_setting(conn, "start_minimized") {
        settings.start_minimized = v == "true";
    }
    if let Some(v) = load_setting(conn, "enable_dnd") {
        settings.enable_dnd = v == "true";
    }
    if let Some(v) = load_setting(conn, "global_hibernation_enabled") {
        settings.global_hibernation_enabled = v == "true";
    }

    settings
}
