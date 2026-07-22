use std::sync::Mutex;
use std::time::Instant;

use rusqlite::Connection;
use zeroize::Zeroizing;

use crate::crypto::VaultKey;
use crate::error::AppError;

pub struct AppState {
    pub inner: Mutex<AppStateInner>,
}

pub struct AppStateInner {
    pub conn: Connection,
    pub key: Option<VaultKey>,
    pub clipboard_token: Option<String>,
    /// Consecutive failed unlock attempts, reset on a successful unlock.
    pub failed_unlocks: u32,
    /// When the last failed unlock happened, for computing the backoff window.
    pub last_failed_unlock: Option<Instant>,
}

impl AppState {
    pub fn new(conn: Connection) -> Self {
        Self {
            inner: Mutex::new(AppStateInner {
                conn,
                key: None,
                clipboard_token: None,
                failed_unlocks: 0,
                last_failed_unlock: None,
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
    // Copy the key into a Zeroizing wrapper so this stack copy is wiped on
    // drop; a bare `[u8; 32]` has no Drop and would leave the master key in
    // stack memory after every unlocked command, defeating zeroize-on-lock.
    let key_bytes = match guard.key.as_ref() {
        Some(k) => Zeroizing::new(**k),
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

pub fn with_authorized<R>(
    state: &AppState,
    f: impl FnOnce(&mut AppStateInner) -> Result<R, AppError>,
) -> Result<R, AppError> {
    let mut guard = state
        .inner
        .lock()
        .map_err(|_| AppError::Internal("state lock poisoned".into()))?;
    if guard.key.is_none() {
        return Err(AppError::Locked);
    }
    f(&mut guard)
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
    fn with_state_runs_closure_when_locked() {
        let state = locked_state();
        let result: Result<i32, AppError> = with_state(&state, |s| {
            assert!(s.key.is_none());
            Ok(7)
        });
        assert_eq!(result.unwrap(), 7);
    }

    #[test]
    fn with_state_runs_closure_when_unlocked() {
        let state = unlocked_state();
        let result: Result<i32, AppError> = with_state(&state, |s| {
            assert!(s.key.is_some());
            Ok(7)
        });
        assert_eq!(result.unwrap(), 7);
    }

    #[test]
    fn with_authorized_rejects_when_locked() {
        let state = locked_state();
        let result: Result<(), AppError> = with_authorized(&state, |_| {
            panic!("closure must not run when locked");
        });
        assert!(matches!(result, Err(AppError::Locked)));
    }

    #[test]
    fn with_authorized_runs_closure_when_unlocked() {
        let state = unlocked_state();
        let result: Result<i32, AppError> = with_authorized(&state, |s| {
            assert!(s.key.is_some());
            Ok(42)
        });
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn with_unlocked_rejects_when_locked() {
        let state = locked_state();
        let result: Result<(), AppError> = with_unlocked(&state, |_, _| {
            panic!("closure must not run when locked");
        });
        assert!(matches!(result, Err(AppError::Locked)));
    }

    #[test]
    fn with_unlocked_exposes_key_when_unlocked() {
        let state = unlocked_state();
        let result: Result<[u8; 32], AppError> = with_unlocked(&state, |_, key| Ok(*key));
        assert_eq!(result.unwrap(), [0u8; 32]);
    }
}
