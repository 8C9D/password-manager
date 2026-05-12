use serde::Serialize;
use tauri::State;

use crate::db::now_iso8601;
use crate::error::AppError;
use crate::state::{with_authorized, AppState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

fn validate_name(name: &str) -> Result<String, AppError> {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err(AppError::Validation("category name is required"));
    }
    if trimmed.len() > 64 {
        return Err(AppError::Validation("category name must be 64 chars or fewer"));
    }
    Ok(trimmed)
}

#[tauri::command]
pub fn list_categories(state: State<'_, AppState>) -> Result<Vec<Category>, AppError> {
    with_authorized(&state, |s| {
        let mut stmt = s.conn.prepare(
            "SELECT id, name, created_at, updated_at
             FROM categories ORDER BY name COLLATE NOCASE ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Category {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    created_at: r.get(2)?,
                    updated_at: r.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

#[tauri::command]
pub fn create_category(state: State<'_, AppState>, name: String) -> Result<i64, AppError> {
    let trimmed = validate_name(&name)?;
    with_authorized(&state, |s| {
        let now = now_iso8601();
        match s.conn.execute(
            "INSERT INTO categories (name, created_at, updated_at) VALUES (?1, ?2, ?2)",
            rusqlite::params![trimmed, now],
        ) {
            Ok(_) => Ok(s.conn.last_insert_rowid()),
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Err(AppError::Validation("a category with that name already exists"))
            }
            Err(e) => Err(AppError::Database(e)),
        }
    })
}

#[tauri::command]
pub fn update_category(
    state: State<'_, AppState>,
    id: i64,
    name: String,
) -> Result<(), AppError> {
    let trimmed = validate_name(&name)?;
    with_authorized(&state, |s| {
        let now = now_iso8601();
        let result = s.conn.execute(
            "UPDATE categories SET name = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![trimmed, now, id],
        );
        match result {
            Ok(0) => Err(AppError::EntryNotFound),
            Ok(_) => Ok(()),
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                Err(AppError::Validation("a category with that name already exists"))
            }
            Err(e) => Err(AppError::Database(e)),
        }
    })
}

#[tauri::command]
pub fn delete_category(state: State<'_, AppState>, id: i64) -> Result<(), AppError> {
    with_authorized(&state, |s| {
        let n = s
            .conn
            .execute("DELETE FROM categories WHERE id = ?1", rusqlite::params![id])?;
        if n == 0 {
            return Err(AppError::EntryNotFound);
        }
        Ok(())
    })
}
