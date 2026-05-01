use std::sync::Mutex;

use rusqlite::Connection;

use crate::crypto::VaultKey;
use crate::error::AppError;

pub struct AppState {
    pub inner: Mutex<AppStateInner>,
}

pub struct AppStateInner {
    pub conn: Connection,
    pub key: Option<VaultKey>,
    pub clipboard_token: Option<String>,
}

impl AppState {
    pub fn new(conn: Connection) -> Self {
        Self {
            inner: Mutex::new(AppStateInner {
                conn,
                key: None,
                clipboard_token: None,
            }),
        }
    }
}

pub fn with_unlocked<R>(
    state: &AppState,
    f: impl FnOnce(&mut AppStateInner, &[u8; 32]) -> Result<R, AppError>,
) -> Result<R, AppError> {
    let mut guard = state
        .inner
        .lock()
        .map_err(|_| AppError::Internal("state lock poisoned".into()))?;
    let key_bytes: [u8; 32] = match guard.key.as_ref() {
        Some(k) => **k,
        None => return Err(AppError::Locked),
    };
    f(&mut guard, &key_bytes)
}

pub fn with_state<R>(
    state: &AppState,
    f: impl FnOnce(&mut AppStateInner) -> Result<R, AppError>,
) -> Result<R, AppError> {
    let mut guard = state
        .inner
        .lock()
        .map_err(|_| AppError::Internal("state lock poisoned".into()))?;
    f(&mut guard)
}
