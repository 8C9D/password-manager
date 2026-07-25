use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::now_iso8601;
use crate::error::AppError;
use crate::state::{with_authorized, with_state, AppState};

const DEFAULT_AUTO_LOCK_SECS: u64 = 300;
const MIN_AUTO_LOCK_SECS: u64 = 30;
const MAX_AUTO_LOCK_SECS: u64 = 86_400;

pub(crate) const DEFAULT_CLIPBOARD_CLEAR_SECS: u64 = 15;
pub(crate) const MIN_CLIPBOARD_CLEAR_SECS: u64 = 1;
pub(crate) const MAX_CLIPBOARD_CLEAR_SECS: u64 = 600;

pub(crate) const DEFAULT_PASSWORD_HISTORY_LIMIT: u64 = 10;
/// Zero is a supported value: it turns password history off entirely and
/// discards whatever was already retained.
pub(crate) const MIN_PASSWORD_HISTORY_LIMIT: u64 = 0;
pub(crate) const MAX_PASSWORD_HISTORY_LIMIT: u64 = 50;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub auto_lock_secs: u64,
    pub clipboard_clear_secs: u64,
    pub password_history_limit: u64,
}

fn read_u64_setting(conn: &rusqlite::Connection, key: &str, default: u64) -> u64 {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        [key],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .and_then(|s| s.parse::<u64>().ok())
    .unwrap_or(default)
}

/// Stored clipboard auto-clear delay, used by copy_to_clipboard when the
/// caller does not pass an explicit value.
pub(crate) fn clipboard_clear_secs(conn: &rusqlite::Connection) -> u64 {
    read_u64_setting(
        conn,
        "clipboard_clear_secs",
        DEFAULT_CLIPBOARD_CLEAR_SECS,
    )
    .clamp(MIN_CLIPBOARD_CLEAR_SECS, MAX_CLIPBOARD_CLEAR_SECS)
}

/// Stored auto-lock timeout, clamped on read for the same reason as the
/// clipboard delay: a hand-edited row (e.g. 0 or an absurdly large value)
/// must not be able to weaken or disable auto-lock.
fn auto_lock_secs(conn: &rusqlite::Connection) -> u64 {
    read_u64_setting(conn, "auto_lock_secs", DEFAULT_AUTO_LOCK_SECS)
        .clamp(MIN_AUTO_LOCK_SECS, MAX_AUTO_LOCK_SECS)
}

/// How many previous passwords to retain per entry, clamped on read for the
/// same reason as the other stored numbers.
pub(crate) fn password_history_limit(conn: &rusqlite::Connection) -> u64 {
    read_u64_setting(
        conn,
        "password_history_limit",
        DEFAULT_PASSWORD_HISTORY_LIMIT,
    )
    .clamp(MIN_PASSWORD_HISTORY_LIMIT, MAX_PASSWORD_HISTORY_LIMIT)
}

/// The accepted range and default for one numeric setting.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bound {
    pub min: u64,
    pub max: u64,
    pub default: u64,
}

/// Every bound `update_settings` enforces, so the settings form can validate
/// against the same numbers instead of repeating them. A bound changed here but
/// not in the template used to produce a form that rejected values the backend
/// accepts, or offered values it refuses.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsBounds {
    pub auto_lock_secs: Bound,
    pub clipboard_clear_secs: Bound,
    pub password_history_limit: Bound,
}

fn settings_bounds() -> SettingsBounds {
    SettingsBounds {
        auto_lock_secs: Bound {
            min: MIN_AUTO_LOCK_SECS,
            max: MAX_AUTO_LOCK_SECS,
            default: DEFAULT_AUTO_LOCK_SECS,
        },
        clipboard_clear_secs: Bound {
            min: MIN_CLIPBOARD_CLEAR_SECS,
            max: MAX_CLIPBOARD_CLEAR_SECS,
            default: DEFAULT_CLIPBOARD_CLEAR_SECS,
        },
        password_history_limit: Bound {
            min: MIN_PASSWORD_HISTORY_LIMIT,
            max: MAX_PASSWORD_HISTORY_LIMIT,
            default: DEFAULT_PASSWORD_HISTORY_LIMIT,
        },
    }
}

#[tauri::command]
pub fn get_settings_bounds() -> SettingsBounds {
    settings_bounds()
}

fn get_settings_impl(state: &AppState) -> Result<Settings, AppError> {
    with_state(state, |s| {
        Ok(Settings {
            auto_lock_secs: auto_lock_secs(&s.conn),
            clipboard_clear_secs: clipboard_clear_secs(&s.conn),
            password_history_limit: password_history_limit(&s.conn),
        })
    })
}

fn update_settings_impl(state: &AppState, input: Settings) -> Result<Settings, AppError> {
    if input.auto_lock_secs < MIN_AUTO_LOCK_SECS || input.auto_lock_secs > MAX_AUTO_LOCK_SECS {
        return Err(AppError::Validation(
            "auto-lock timeout must be between 30 seconds and 24 hours",
        ));
    }
    if input.clipboard_clear_secs < MIN_CLIPBOARD_CLEAR_SECS
        || input.clipboard_clear_secs > MAX_CLIPBOARD_CLEAR_SECS
    {
        return Err(AppError::Validation(
            "clipboard clear delay must be between 1 and 600 seconds",
        ));
    }
    if input.password_history_limit > MAX_PASSWORD_HISTORY_LIMIT {
        return Err(AppError::Validation(
            "password history limit must be between 0 and 50",
        ));
    }
    with_authorized(state, |s| {
        let now = now_iso8601();
        // One transaction so the retention trim below can never outlive a
        // failed settings write (it deletes secrets that the old limit kept).
        let tx = s.conn.transaction()?;
        for (key, value) in [
            ("auto_lock_secs", input.auto_lock_secs),
            ("clipboard_clear_secs", input.clipboard_clear_secs),
            ("password_history_limit", input.password_history_limit),
        ] {
            tx.execute(
                "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                rusqlite::params![key, value.to_string(), now],
            )?;
        }
        // Apply a lowered limit to history that already exists, rather than
        // only to future password changes - otherwise turning history down (or
        // off) would leave the old secrets sitting in the database.
        super::history::prune_all_history(&tx, input.password_history_limit)?;
        tx.commit()?;
        Ok(input)
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

    fn settings(auto_lock_secs: u64, clipboard_clear_secs: u64) -> Settings {
        Settings {
            auto_lock_secs,
            clipboard_clear_secs,
            password_history_limit: DEFAULT_PASSWORD_HISTORY_LIMIT,
        }
    }

    #[test]
    fn get_returns_defaults_when_no_rows_exist() {
        let state = unlocked_state();
        let s = get_settings_impl(&state).unwrap();
        assert_eq!(s.auto_lock_secs, DEFAULT_AUTO_LOCK_SECS);
        assert_eq!(s.clipboard_clear_secs, DEFAULT_CLIPBOARD_CLEAR_SECS);
    }

    #[test]
    fn get_clamps_hand_edited_auto_lock_rows() {
        let state = unlocked_state();
        let write = |value: &str| {
            state
                .inner
                .lock()
                .unwrap()
                .conn
                .execute(
                    "INSERT INTO settings (key, value, updated_at)
                     VALUES ('auto_lock_secs', ?1, '2026-01-01T00:00:00Z')
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    [value],
                )
                .unwrap();
        };
        // A row edited to 0 (or anything under the minimum) must not disable
        // auto-lock; an absurdly large value must not effectively disable it.
        write("0");
        assert_eq!(get_settings_impl(&state).unwrap().auto_lock_secs, MIN_AUTO_LOCK_SECS);
        write("99999999");
        assert_eq!(get_settings_impl(&state).unwrap().auto_lock_secs, MAX_AUTO_LOCK_SECS);
    }

    #[test]
    fn update_then_get_returns_saved_values() {
        let state = unlocked_state();
        let saved = update_settings_impl(&state, settings(600, 30)).unwrap();
        assert_eq!(saved.auto_lock_secs, 600);
        assert_eq!(saved.clipboard_clear_secs, 30);
        let fetched = get_settings_impl(&state).unwrap();
        assert_eq!(fetched.auto_lock_secs, 600);
        assert_eq!(fetched.clipboard_clear_secs, 30);
    }

    #[test]
    fn update_overwrites_previous_values() {
        let state = unlocked_state();
        update_settings_impl(&state, settings(600, 20)).unwrap();
        update_settings_impl(&state, settings(900, 45)).unwrap();
        let fetched = get_settings_impl(&state).unwrap();
        assert_eq!(fetched.auto_lock_secs, 900);
        assert_eq!(fetched.clipboard_clear_secs, 45);
    }

    /// The published bounds are only useful if `update_settings` actually
    /// enforces them: drive each edge through the real validator rather than
    /// comparing the struct to the same constants it was built from.
    #[test]
    fn published_bounds_match_what_update_settings_enforces() {
        let b = settings_bounds();
        let state = unlocked_state();

        let auto_lock = |v: u64| Settings {
            auto_lock_secs: v,
            clipboard_clear_secs: DEFAULT_CLIPBOARD_CLEAR_SECS,
            password_history_limit: DEFAULT_PASSWORD_HISTORY_LIMIT,
        };
        assert!(update_settings_impl(&state, auto_lock(b.auto_lock_secs.min)).is_ok());
        assert!(update_settings_impl(&state, auto_lock(b.auto_lock_secs.max)).is_ok());
        assert!(update_settings_impl(&state, auto_lock(b.auto_lock_secs.min - 1)).is_err());
        assert!(update_settings_impl(&state, auto_lock(b.auto_lock_secs.max + 1)).is_err());

        let clipboard = |v: u64| Settings {
            auto_lock_secs: DEFAULT_AUTO_LOCK_SECS,
            clipboard_clear_secs: v,
            password_history_limit: DEFAULT_PASSWORD_HISTORY_LIMIT,
        };
        assert!(update_settings_impl(&state, clipboard(b.clipboard_clear_secs.min)).is_ok());
        assert!(update_settings_impl(&state, clipboard(b.clipboard_clear_secs.max)).is_ok());
        assert!(update_settings_impl(&state, clipboard(b.clipboard_clear_secs.min - 1)).is_err());
        assert!(update_settings_impl(&state, clipboard(b.clipboard_clear_secs.max + 1)).is_err());

        let history = |v: u64| Settings {
            auto_lock_secs: DEFAULT_AUTO_LOCK_SECS,
            clipboard_clear_secs: DEFAULT_CLIPBOARD_CLEAR_SECS,
            password_history_limit: v,
        };
        assert!(update_settings_impl(&state, history(b.password_history_limit.min)).is_ok());
        assert!(update_settings_impl(&state, history(b.password_history_limit.max)).is_ok());
        assert!(update_settings_impl(&state, history(b.password_history_limit.max + 1)).is_err());

        // Each published default must itself be inside its published range.
        for bound in [
            &b.auto_lock_secs,
            &b.clipboard_clear_secs,
            &b.password_history_limit,
        ] {
            assert!(bound.min <= bound.default && bound.default <= bound.max);
        }
    }

    #[test]
    fn update_rejects_auto_lock_below_minimum() {
        let state = unlocked_state();
        assert!(matches!(
            update_settings_impl(&state, settings(29, 15)),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn update_rejects_auto_lock_above_maximum() {
        let state = unlocked_state();
        assert!(matches!(
            update_settings_impl(&state, settings(86_401, 15)),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn update_accepts_auto_lock_at_minimum_boundary() {
        // The guard is `< MIN_AUTO_LOCK_SECS`, so the minimum itself must be
        // accepted. The reject test above only proves MIN - 1 fails; tightening
        // the comparison to `<=` would reject the documented minimum while every
        // existing test stayed green. Assert it is accepted and round-trips.
        let state = unlocked_state();
        let saved =
            update_settings_impl(&state, settings(MIN_AUTO_LOCK_SECS, 15)).unwrap();
        assert_eq!(saved.auto_lock_secs, MIN_AUTO_LOCK_SECS);
        assert_eq!(
            get_settings_impl(&state).unwrap().auto_lock_secs,
            MIN_AUTO_LOCK_SECS
        );
    }

    #[test]
    fn update_accepts_auto_lock_at_maximum_boundary() {
        // Mirror of the minimum case for the upper bound: the guard is
        // `> MAX_AUTO_LOCK_SECS`, so the maximum (24h) must be accepted and
        // round-trip. Guards against a `>=` regression on the upper bound.
        let state = unlocked_state();
        let saved =
            update_settings_impl(&state, settings(MAX_AUTO_LOCK_SECS, 15)).unwrap();
        assert_eq!(saved.auto_lock_secs, MAX_AUTO_LOCK_SECS);
        assert_eq!(
            get_settings_impl(&state).unwrap().auto_lock_secs,
            MAX_AUTO_LOCK_SECS
        );
    }

    #[test]
    fn update_rejects_clipboard_clear_below_minimum() {
        let state = unlocked_state();
        assert!(matches!(
            update_settings_impl(&state, settings(300, MIN_CLIPBOARD_CLEAR_SECS - 1)),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn update_rejects_clipboard_clear_above_maximum() {
        let state = unlocked_state();
        assert!(matches!(
            update_settings_impl(&state, settings(300, MAX_CLIPBOARD_CLEAR_SECS + 1)),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn update_accepts_clipboard_clear_at_minimum_boundary() {
        // Same boundary contract as auto-lock: 1 second is documented as valid
        // and must round-trip, so a `<=` regression cannot slip through.
        let state = unlocked_state();
        let saved =
            update_settings_impl(&state, settings(300, MIN_CLIPBOARD_CLEAR_SECS)).unwrap();
        assert_eq!(saved.clipboard_clear_secs, MIN_CLIPBOARD_CLEAR_SECS);
        assert_eq!(
            get_settings_impl(&state).unwrap().clipboard_clear_secs,
            MIN_CLIPBOARD_CLEAR_SECS
        );
    }

    #[test]
    fn update_accepts_clipboard_clear_at_maximum_boundary() {
        let state = unlocked_state();
        let saved =
            update_settings_impl(&state, settings(300, MAX_CLIPBOARD_CLEAR_SECS)).unwrap();
        assert_eq!(saved.clipboard_clear_secs, MAX_CLIPBOARD_CLEAR_SECS);
        assert_eq!(
            get_settings_impl(&state).unwrap().clipboard_clear_secs,
            MAX_CLIPBOARD_CLEAR_SECS
        );
    }

    #[test]
    fn update_rejects_when_locked() {
        let state = locked_state();
        assert!(matches!(
            update_settings_impl(&state, settings(600, 15)),
            Err(AppError::Locked)
        ));
    }

    #[test]
    fn get_allowed_while_locked() {
        let state = locked_state();
        let s = get_settings_impl(&state).unwrap();
        assert_eq!(s.auto_lock_secs, DEFAULT_AUTO_LOCK_SECS);
        assert_eq!(s.clipboard_clear_secs, DEFAULT_CLIPBOARD_CLEAR_SECS);
    }

    #[test]
    fn get_falls_back_to_default_when_stored_value_is_not_a_number() {
        let state = unlocked_state();
        // Simulate a corrupt or hand-edited row whose value can't parse as u64.
        // The read must fall back to the default rather than panic or yield
        // a garbage timeout that would weaken the auto-lock guarantee.
        {
            let guard = state.inner.lock().unwrap();
            guard
                .conn
                .execute(
                    "INSERT INTO settings (key, value, updated_at)
                     VALUES ('auto_lock_secs', 'not-a-number', '2026-05-28T00:00:00Z')",
                    [],
                )
                .unwrap();
        }
        let s = get_settings_impl(&state).unwrap();
        assert_eq!(s.auto_lock_secs, DEFAULT_AUTO_LOCK_SECS);
    }

    #[test]
    fn history_limit_round_trips_and_is_clamped_on_read() {
        let state = unlocked_state();
        assert_eq!(
            get_settings_impl(&state).unwrap().password_history_limit,
            DEFAULT_PASSWORD_HISTORY_LIMIT
        );

        let saved = update_settings_impl(
            &state,
            Settings {
                password_history_limit: 3,
                ..settings(300, 15)
            },
        )
        .unwrap();
        assert_eq!(saved.password_history_limit, 3);
        assert_eq!(get_settings_impl(&state).unwrap().password_history_limit, 3);

        // A hand-edited row above the maximum must not let history grow without
        // bound.
        state
            .inner
            .lock()
            .unwrap()
            .conn
            .execute(
                "UPDATE settings SET value = '9999' WHERE key = 'password_history_limit'",
                [],
            )
            .unwrap();
        assert_eq!(
            get_settings_impl(&state).unwrap().password_history_limit,
            MAX_PASSWORD_HISTORY_LIMIT
        );
    }

    #[test]
    fn update_rejects_history_limit_above_maximum() {
        let state = unlocked_state();
        assert!(matches!(
            update_settings_impl(
                &state,
                Settings {
                    password_history_limit: MAX_PASSWORD_HISTORY_LIMIT + 1,
                    ..settings(300, 15)
                }
            ),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn update_accepts_zero_history_limit_and_discards_retained_history() {
        let state = unlocked_state();
        {
            let guard = state.inner.lock().unwrap();
            guard
                .conn
                .execute(
                    "INSERT INTO password_entries
                        (id, title, username, url_or_app_name,
                         encrypted_password, password_nonce, created_at, updated_at)
                     VALUES (1, 'E', 'u', 'x', X'00', X'00',
                             '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                    [],
                )
                .unwrap();
            guard
                .conn
                .execute(
                    "INSERT INTO password_history
                        (entry_id, encrypted_password, password_nonce, changed_at)
                     VALUES (1, X'AA', X'BB', '2026-01-01T00:00:00Z')",
                    [],
                )
                .unwrap();
        }

        let saved = update_settings_impl(
            &state,
            Settings {
                password_history_limit: 0,
                ..settings(300, 15)
            },
        )
        .unwrap();
        assert_eq!(saved.password_history_limit, 0);

        // Turning history off must delete what was already retained, not just
        // stop recording new rows.
        let remaining: i64 = state
            .inner
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT COUNT(*) FROM password_history", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 0);
    }

    #[test]
    fn stored_clipboard_clear_outside_range_is_clamped_on_read() {
        // A hand-edited row must not produce a delay outside the documented
        // range when consumed by copy_to_clipboard's default path.
        let state = unlocked_state();
        {
            let guard = state.inner.lock().unwrap();
            guard
                .conn
                .execute(
                    "INSERT INTO settings (key, value, updated_at)
                     VALUES ('clipboard_clear_secs', '9999', '2026-05-28T00:00:00Z')",
                    [],
                )
                .unwrap();
        }
        let s = get_settings_impl(&state).unwrap();
        assert_eq!(s.clipboard_clear_secs, MAX_CLIPBOARD_CLEAR_SECS);
    }
}
