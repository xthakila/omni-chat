pub mod queries;
pub mod schema;

use rusqlite::Connection;
use std::path::Path;

/// Initialize the database, creating tables if they don't exist.
pub fn init(path: &Path) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open(path)?;

    // Enable WAL mode for better concurrent read performance.
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;

    schema::create_tables(&conn)?;

    Ok(conn)
}
