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

/// Whether a delayed clear task still owns the clipboard: it must be the most
/// recent copy (generation match) AND the stored token must still be its
/// value. Generation alone distinguishes two copies of the same secret; the
/// earlier task must not clear at its (now superseded) deadline.
pub(crate) fn clear_task_owns_clipboard(
    state_generation: u64,
    task_generation: u64,
    state_token: Option<&str>,
    task_token: &str,
) -> bool {
    state_generation == task_generation && is_our_clipboard_value(state_token, task_token)
}

/// Write to the clipboard and record what we wrote, as one atomic step under
/// the state lock, returning the new generation.
///
/// The two halves must not be separated. With the write outside the lock, two
/// copies racing can land in the order write(A), write(B), record(B),
/// record(A): the state then claims A while the OS clipboard holds B, so B's
/// clear task is superseded by generation and A's task refuses to clear a
/// value that is not the one it recorded. The result is a secret sitting on
/// the clipboard that nothing ever wipes - the exact outcome auto-clear exists
/// to prevent.
pub(crate) fn write_and_record<F>(
    state: &AppState,
    value: String,
    write: F,
) -> Result<u64, AppError>
where
    F: FnOnce(&str) -> Result<(), AppError>,
{
    with_state(state, |s| {
        write(&value)?;
        s.clipboard_token = Some(value);
        s.clipboard_generation += 1;
        Ok(s.clipboard_generation)
    })
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

    let token = value.clone();
    let generation = write_and_record(&state, value, |v| {
        app.clipboard()
            .write_text(v.to_string())
            .map_err(|e| AppError::Internal(format!("clipboard write failed: {e}")))
    })?;

    let app_for_task = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(secs)).await;
        let should_clear = match app_for_task.try_state::<AppState>() {
            Some(state) => {
                let mut guard = match state.inner.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                let matches = clear_task_owns_clipboard(
                    guard.clipboard_generation,
                    generation,
                    guard.clipboard_token.as_deref(),
                    &token,
                );
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
    fn clear_task_yields_to_a_newer_copy_of_the_same_value() {
        // Copy A at gen 1, copy A again at gen 2: the first task's deadline
        // must no-op so the second copy gets the full delay it returned.
        assert!(!clear_task_owns_clipboard(2, 1, Some("s3cret"), "s3cret"));
        // The newest task still clears.
        assert!(clear_task_owns_clipboard(2, 2, Some("s3cret"), "s3cret"));
        // Generation match alone is not enough; the token must still be ours.
        assert!(!clear_task_owns_clipboard(2, 2, Some("other"), "s3cret"));
        assert!(!clear_task_owns_clipboard(2, 2, None, "s3cret"));
    }

    #[test]
    fn a_copy_cannot_start_writing_while_another_is_mid_write() {
        use crate::db;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use std::sync::Arc;

        // The write and the record have to be one indivisible step. If a second
        // copy can write while the first is still between its write and its
        // record, the two can land as write(A), write(B), record(B), record(A):
        // the state then names A while the clipboard holds B, and neither
        // clear task will touch it - the secret stays on the clipboard forever.
        //
        // Overlap is the thing to assert; a plain two-thread race is timing
        // dependent and passes even when the ordering is wrong.
        let state = Arc::new(AppState::new(db::open_in_memory().unwrap()));
        let in_write = Arc::new(AtomicBool::new(false));
        let overlaps = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = ["secret-A", "secret-B", "secret-C"]
            .into_iter()
            .map(|value| {
                let state = Arc::clone(&state);
                let in_write = Arc::clone(&in_write);
                let overlaps = Arc::clone(&overlaps);
                std::thread::spawn(move || {
                    write_and_record(&state, value.to_string(), |_| {
                        if in_write.swap(true, Ordering::SeqCst) {
                            overlaps.fetch_add(1, Ordering::SeqCst);
                        }
                        // Hold the "clipboard" long enough that any thread
                        // allowed to proceed concurrently will be seen.
                        std::thread::sleep(std::time::Duration::from_millis(30));
                        in_write.store(false, Ordering::SeqCst);
                        Ok(())
                    })
                    .unwrap()
                })
            })
            .collect();
        let generations: Vec<u64> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        assert_eq!(
            overlaps.load(Ordering::SeqCst),
            0,
            "two copies were writing at once, so the recorded token can diverge from the clipboard"
        );
        // Each copy got its own generation, and the last one recorded wins.
        let mut sorted = generations.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![1, 2, 3]);
        let guard = state.inner.lock().unwrap();
        assert_eq!(guard.clipboard_generation, 3);
    }

    #[test]
    fn a_failed_clipboard_write_records_nothing() {
        use crate::db;

        // If the OS write fails there is no secret on the clipboard, so the
        // token must not claim one - a stale token would make the next lock
        // try to wipe a clipboard that was never ours.
        let state = AppState::new(db::open_in_memory().unwrap());
        let result = write_and_record(&state, "s3cret".into(), |_| {
            Err(AppError::Internal("clipboard write failed".into()))
        });
        assert!(result.is_err());
        let guard = state.inner.lock().unwrap();
        assert!(guard.clipboard_token.is_none());
        assert_eq!(guard.clipboard_generation, 0);
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
