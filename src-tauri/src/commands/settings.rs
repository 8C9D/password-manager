use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::now_iso8601;
use crate::error::AppError;
use crate::state::{with_authorized, with_state, AppState};

const DEFAULT_AUTO_LOCK_SECS: u64 = 300;
const MIN_AUTO_LOCK_SECS: u64 = 30;
const MAX_AUTO_LOCK_SECS: u64 = 86_400;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub auto_lock_secs: u64,
}

fn read_secs(conn: &rusqlite::Connection) -> Result<u64, AppError> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'auto_lock_secs'",
            [],
            |r| r.get::<_, String>(0),
        )
        .ok();
    Ok(raw
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_AUTO_LOCK_SECS))
}

fn get_settings_impl(state: &AppState) -> Result<Settings, AppError> {
    with_state(state, |s| {
        let secs = read_secs(&s.conn)?;
        Ok(Settings { auto_lock_secs: secs })
    })
}

fn update_settings_impl(state: &AppState, input: Settings) -> Result<Settings, AppError> {
    if input.auto_lock_secs < MIN_AUTO_LOCK_SECS || input.auto_lock_secs > MAX_AUTO_LOCK_SECS {
        return Err(AppError::Validation(
            "auto-lock timeout must be between 30 seconds and 24 hours",
        ));
    }
    with_authorized(state, |s| {
        let now = now_iso8601();
        s.conn.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES ('auto_lock_secs', ?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            rusqlite::params![input.auto_lock_secs.to_string(), now],
        )?;
        Ok(Settings { auto_lock_secs: input.auto_lock_secs })
    })
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, AppError> {
    get_settings_impl(&state)
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, AppState>,
    input: Settings,
) -> Result<Settings, AppError> {
    update_settings_impl(&state, input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use zeroize::Zeroizing;

    fn locked_state() -> AppState {
        AppState::new(db::open_in_memory().unwrap())
    }

    fn unlocked_state() -> AppState {
        let state = locked_state();
        state.inner.lock().unwrap().key = Some(Zeroizing::new([0u8; 32]));
        state
    }

    #[test]
    fn get_returns_default_when_no_row_exists() {
        let state = unlocked_state();
        let settings = get_settings_impl(&state).unwrap();
        assert_eq!(settings.auto_lock_secs, DEFAULT_AUTO_LOCK_SECS);
    }

    #[test]
    fn update_then_get_returns_saved_value() {
        let state = unlocked_state();
        let saved = update_settings_impl(&state, Settings { auto_lock_secs: 600 }).unwrap();
        assert_eq!(saved.auto_lock_secs, 600);
        let fetched = get_settings_impl(&state).unwrap();
        assert_eq!(fetched.auto_lock_secs, 600);
    }

    #[test]
    fn update_overwrites_previous_value() {
        let state = unlocked_state();
        update_settings_impl(&state, Settings { auto_lock_secs: 600 }).unwrap();
        update_settings_impl(&state, Settings { auto_lock_secs: 900 }).unwrap();
        let fetched = get_settings_impl(&state).unwrap();
        assert_eq!(fetched.auto_lock_secs, 900);
    }

    #[test]
    fn update_rejects_value_below_minimum() {
        let state = unlocked_state();
        assert!(matches!(
            update_settings_impl(&state, Settings { auto_lock_secs: 29 }),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn update_rejects_value_above_maximum() {
        let state = unlocked_state();
        assert!(matches!(
            update_settings_impl(&state, Settings { auto_lock_secs: 86_401 }),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn update_rejects_when_locked() {
        let state = locked_state();
        assert!(matches!(
            update_settings_impl(&state, Settings { auto_lock_secs: 600 }),
            Err(AppError::Locked)
        ));
    }

    #[test]
    fn get_allowed_while_locked() {
        let state = locked_state();
        let settings = get_settings_impl(&state).unwrap();
        assert_eq!(settings.auto_lock_secs, DEFAULT_AUTO_LOCK_SECS);
    }
}
