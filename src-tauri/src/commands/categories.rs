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

pub(crate) fn validate_name(name: &str) -> Result<String, AppError> {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err(AppError::Validation("category name is required"));
    }
    if trimmed.len() > 64 {
        return Err(AppError::Validation("category name must be 64 chars or fewer"));
    }
    Ok(trimmed)
}

fn list_categories_impl(state: &AppState) -> Result<Vec<Category>, AppError> {
    with_authorized(state, |s| {
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

/// Maps a write failure on the `categories` table to an `AppError`, turning the
/// `UNIQUE(name)` constraint violation into a friendly validation message and
/// passing every other database error through unchanged.
fn map_category_write_error(e: rusqlite::Error) -> AppError {
    match e {
        rusqlite::Error::SqliteFailure(f, _)
            if f.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            AppError::Validation("a category with that name already exists")
        }
        other => AppError::Database(other),
    }
}

fn create_category_impl(state: &AppState, name: String) -> Result<i64, AppError> {
    let trimmed = validate_name(&name)?;
    with_authorized(state, |s| {
        let now = now_iso8601();
        s.conn
            .execute(
                "INSERT INTO categories (name, created_at, updated_at) VALUES (?1, ?2, ?2)",
                rusqlite::params![trimmed, now],
            )
            .map_err(map_category_write_error)?;
        Ok(s.conn.last_insert_rowid())
    })
}

fn update_category_impl(state: &AppState, id: i64, name: String) -> Result<(), AppError> {
    let trimmed = validate_name(&name)?;
    with_authorized(state, |s| {
        let now = now_iso8601();
        let n = s
            .conn
            .execute(
                "UPDATE categories SET name = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![trimmed, now, id],
            )
            .map_err(map_category_write_error)?;
        if n == 0 {
            return Err(AppError::CategoryNotFound);
        }
        Ok(())
    })
}

fn delete_category_impl(state: &AppState, id: i64) -> Result<(), AppError> {
    with_authorized(state, |s| {
        let n = s
            .conn
            .execute("DELETE FROM categories WHERE id = ?1", rusqlite::params![id])?;
        if n == 0 {
            return Err(AppError::CategoryNotFound);
        }
        Ok(())
    })
}

#[tauri::command]
pub fn list_categories(state: State<'_, AppState>) -> Result<Vec<Category>, AppError> {
    list_categories_impl(&state)
}

#[tauri::command]
pub fn create_category(state: State<'_, AppState>, name: String) -> Result<i64, AppError> {
    create_category_impl(&state, name)
}

#[tauri::command]
pub fn update_category(
    state: State<'_, AppState>,
    id: i64,
    name: String,
) -> Result<(), AppError> {
    update_category_impl(&state, id, name)
}

#[tauri::command]
pub fn delete_category(state: State<'_, AppState>, id: i64) -> Result<(), AppError> {
    delete_category_impl(&state, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use zeroize::Zeroizing;

    fn unlocked_state() -> AppState {
        let state = AppState::new(db::open_in_memory().unwrap());
        state.inner.lock().unwrap().key = Some(Zeroizing::new([0u8; 32]));
        state
    }

    #[test]
    fn create_then_list_returns_category() {
        let state = unlocked_state();
        let id = create_category_impl(&state, "Work".into()).unwrap();
        let cats = list_categories_impl(&state).unwrap();
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].id, id);
        assert_eq!(cats[0].name, "Work");
    }

    #[test]
    fn list_returns_empty_when_no_categories() {
        let state = unlocked_state();
        let cats = list_categories_impl(&state).unwrap();
        assert!(cats.is_empty());
    }

    #[test]
    fn list_sorts_case_insensitively() {
        let state = unlocked_state();
        create_category_impl(&state, "banana".into()).unwrap();
        create_category_impl(&state, "Apple".into()).unwrap();
        create_category_impl(&state, "cherry".into()).unwrap();
        let names: Vec<_> = list_categories_impl(&state)
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(names, vec!["Apple", "banana", "cherry"]);
    }

    #[test]
    fn create_rejects_blank_name() {
        let state = unlocked_state();
        let err = create_category_impl(&state, "   ".into()).unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn create_rejects_duplicate_name() {
        let state = unlocked_state();
        create_category_impl(&state, "Work".into()).unwrap();
        let err = create_category_impl(&state, "Work".into()).unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn update_renames_existing_category() {
        let state = unlocked_state();
        let id = create_category_impl(&state, "Work".into()).unwrap();
        update_category_impl(&state, id, "Personal".into()).unwrap();
        let cats = list_categories_impl(&state).unwrap();
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].name, "Personal");
    }

    #[test]
    fn update_returns_category_not_found_for_missing_id() {
        let state = unlocked_state();
        let err = update_category_impl(&state, 9999, "Anything".into()).unwrap_err();
        assert!(matches!(err, AppError::CategoryNotFound));
    }

    #[test]
    fn update_rejects_duplicate_name() {
        let state = unlocked_state();
        create_category_impl(&state, "Work".into()).unwrap();
        let id = create_category_impl(&state, "Personal".into()).unwrap();
        let err = update_category_impl(&state, id, "Work".into()).unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn delete_removes_existing_category() {
        let state = unlocked_state();
        let id = create_category_impl(&state, "Work".into()).unwrap();
        delete_category_impl(&state, id).unwrap();
        assert!(list_categories_impl(&state).unwrap().is_empty());
    }

    #[test]
    fn delete_returns_category_not_found_for_missing_id() {
        let state = unlocked_state();
        let err = delete_category_impl(&state, 9999).unwrap_err();
        assert!(matches!(err, AppError::CategoryNotFound));
    }

    #[test]
    fn create_rejects_name_longer_than_64_chars() {
        let state = unlocked_state();
        let err = create_category_impl(&state, "a".repeat(65)).unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn create_accepts_name_of_exactly_64_chars() {
        let state = unlocked_state();
        let name = "a".repeat(64);
        let id = create_category_impl(&state, name.clone()).unwrap();
        let cats = list_categories_impl(&state).unwrap();
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].id, id);
        assert_eq!(cats[0].name, name);
    }
}
