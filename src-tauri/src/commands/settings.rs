use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::now_iso8601;
use crate::error::AppError;
use crate::state::{with_state, AppState};

const DEFAULT_AUTO_LOCK_SECS: u64 = 300;
const MIN_AUTO_LOCK_SECS: u64 = 30;
const MAX_AUTO_LOCK_SECS: u64 = 86_400;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub auto_lock_secs: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsInput {
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

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, AppError> {
    with_state(&state, |s| {
        let secs = read_secs(&s.conn)?;
        Ok(Settings { auto_lock_secs: secs })
    })
}

#[tauri::command]
pub fn update_settings(
    state: State<'_, AppState>,
    input: SettingsInput,
) -> Result<Settings, AppError> {
    if input.auto_lock_secs < MIN_AUTO_LOCK_SECS || input.auto_lock_secs > MAX_AUTO_LOCK_SECS {
        return Err(AppError::Validation(
            "auto-lock timeout must be between 30 seconds and 24 hours",
        ));
    }
    with_state(&state, |s| {
        if s.key.is_none() {
            return Err(AppError::Locked);
        }
        let now = now_iso8601();
        s.conn.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES ('auto_lock_secs', ?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            rusqlite::params![input.auto_lock_secs.to_string(), now],
        )?;
        Ok(Settings { auto_lock_secs: input.auto_lock_secs })
    })
}
