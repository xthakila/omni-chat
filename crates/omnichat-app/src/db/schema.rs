use rusqlite::Connection;

pub fn create_tables(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS services (
            id TEXT PRIMARY KEY,
            recipe_id TEXT NOT NULL,
            name TEXT NOT NULL,
            team TEXT,
            custom_url TEXT,
            sort_order INTEGER DEFAULT 0,
            is_enabled INTEGER DEFAULT 1,
            is_notification_enabled INTEGER DEFAULT 1,
            is_badge_enabled INTEGER DEFAULT 1,
            is_muted INTEGER DEFAULT 0,
            is_dark_mode_enabled INTEGER DEFAULT 0,
            is_hibernation_enabled INTEGER DEFAULT 1,
            proxy TEXT,
            created_at TEXT DEFAULT (datetime('now')),
            updated_at TEXT DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS workspaces (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            sort_order INTEGER DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS workspace_services (
            workspace_id TEXT REFERENCES workspaces(id) ON DELETE CASCADE,
            service_id TEXT REFERENCES services(id) ON DELETE CASCADE,
            PRIMARY KEY (workspace_id, service_id)
        );
        ",
    )
}
