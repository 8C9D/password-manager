use std::time::Duration;

use tauri::{AppHandle, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::error::AppError;
use crate::state::{with_state, AppState};

use super::settings::{
    clipboard_clear_secs, MAX_CLIPBOARD_CLEAR_SECS, MIN_CLIPBOARD_CLEAR_SECS,
};

/// Clamp a caller-requested auto-clear delay into the supported range. The
/// persisted-setting path is already clamped on read (`clipboard_clear_secs`).
pub(crate) fn clamp_clear_secs(secs: u64) -> u64 {
    secs.clamp(MIN_CLIPBOARD_CLEAR_SECS, MAX_CLIPBOARD_CLEAR_SECS)
}

/// Whether a clipboard value still equals the one we wrote. This guards every
/// clear: we only ever wipe the OS clipboard (or drop our stored token) while
/// it still holds the exact secret we put there, never a value the user copied
/// from somewhere else afterwards.
pub(crate) fn is_our_clipboard_value(current: Option<&str>, ours: &str) -> bool {
    current == Some(ours)
}

#[tauri::command]
pub fn copy_to_clipboard(
    app: AppHandle,
    state: State<'_, AppState>,
    value: String,
    clear_after_secs: Option<u64>,
) -> Result<u64, AppError> {
    // An explicit argument wins; otherwise use the persisted setting.
    let secs = match clear_after_secs {
        Some(v) => clamp_clear_secs(v),
        None => with_state(&state, |s| Ok(clipboard_clear_secs(&s.conn)))?,
    };

    app.clipboard()
        .write_text(value.clone())
        .map_err(|e| AppError::Internal(format!("clipboard write failed: {e}")))?;

    with_state(&state, |s| {
        s.clipboard_token = Some(value.clone());
        Ok(())
    })?;

    let app_for_task = app.clone();
    let token = value;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(secs)).await;
        let should_clear = match app_for_task.try_state::<AppState>() {
            Some(state) => {
                let mut guard = match state.inner.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                let matches = is_our_clipboard_value(guard.clipboard_token.as_deref(), &token);
                if matches {
                    guard.clipboard_token = None;
                }
                matches
            }
            None => false,
        };
        if !should_clear {
            return;
        }
        let current = app_for_task.clipboard().read_text().ok();
        if is_our_clipboard_value(current.as_deref(), &token) {
            let _ = app_for_task.clipboard().clear();
        }
    });

    Ok(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_forces_requested_delay_into_supported_range() {
        assert_eq!(clamp_clear_secs(0), MIN_CLIPBOARD_CLEAR_SECS);
        assert_eq!(clamp_clear_secs(MIN_CLIPBOARD_CLEAR_SECS), MIN_CLIPBOARD_CLEAR_SECS);
        assert_eq!(clamp_clear_secs(60), 60);
        assert_eq!(clamp_clear_secs(MAX_CLIPBOARD_CLEAR_SECS), MAX_CLIPBOARD_CLEAR_SECS);
        assert_eq!(clamp_clear_secs(MAX_CLIPBOARD_CLEAR_SECS + 1), MAX_CLIPBOARD_CLEAR_SECS);
        assert_eq!(clamp_clear_secs(u64::MAX), MAX_CLIPBOARD_CLEAR_SECS);
    }

    #[test]
    fn is_our_value_matches_only_the_exact_secret_we_wrote() {
        assert!(is_our_clipboard_value(Some("s3cret"), "s3cret"));
        // A value the user copied afterwards must not be treated as ours.
        assert!(!is_our_clipboard_value(Some("something-else"), "s3cret"));
        // Trailing whitespace differs; must not match.
        assert!(!is_our_clipboard_value(Some("s3cret "), "s3cret"));
        // An empty/cleared clipboard is not ours.
        assert!(!is_our_clipboard_value(None, "s3cret"));
        // Degenerate empty-secret case still compares by exact equality.
        assert!(is_our_clipboard_value(Some(""), ""));
    }
}
