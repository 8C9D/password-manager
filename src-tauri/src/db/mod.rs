use std::path::Path;

use rusqlite::Connection;

use crate::error::AppError;

const SCHEMA: &str = include_str!("schema.sql");

pub fn open_and_migrate(path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

#[cfg(test)]
pub fn open_in_memory() -> Result<Connection, AppError> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

pub fn now_iso8601() -> String {
    chrono::Utc::now().to_rfc3339()
}
