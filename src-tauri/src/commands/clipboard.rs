use std::time::Duration;

use tauri::{AppHandle, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::error::AppError;
use crate::state::{with_state, AppState};

const DEFAULT_CLEAR_SECS: u64 = 15;

#[tauri::command]
pub fn copy_to_clipboard(
    app: AppHandle,
    state: State<'_, AppState>,
    value: String,
    clear_after_secs: Option<u64>,
) -> Result<u64, AppError> {
    let secs = clear_after_secs.unwrap_or(DEFAULT_CLEAR_SECS).clamp(1, 600);

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
                let matches = guard
                    .clipboard_token
                    .as_deref()
                    .map(|t| t == token.as_str())
                    .unwrap_or(false);
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
        if current.as_deref() == Some(token.as_str()) {
            let _ = app_for_task.clipboard().clear();
        }
    });

    Ok(secs)
}
