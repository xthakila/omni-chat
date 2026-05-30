pub mod queries;
pub mod schema;

use rusqlite::Connection;
use std::path::Path;

/// Log (instead of silently discarding) a DB write whose result we don't
/// propagate. Used at CEF callback edges where we cannot return an error but
/// must never unwind. Replaces the old `let _ = save_…(…)` pattern.
pub fn warn_on_err<T>(ctx: &str, r: rusqlite::Result<T>) {
    if let Err(e) = r {
        log::warn!("db {ctx} failed: {e}");
    }
}

/// Initialize the database, creating tables if they don't exist.
pub fn init(path: &Path) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open(path)?;

    // Enable WAL mode for better concurrent read performance.
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;

    schema::create_tables(&conn)?;

    Ok(conn)
}
