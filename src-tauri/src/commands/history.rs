use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use tauri::State;

use crate::crypto;
use crate::error::AppError;
use crate::state::{with_authorized, with_unlocked, AppState};

use super::settings::password_history_limit;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordHistoryItem {
    pub id: i64,
    pub password: String,
    /// When this password stopped being the entry's current one.
    pub changed_at: String,
}

/// Record `old_password` as a superseded password of `entry_id`, then trim the
/// entry's history to the configured retention limit.
///
/// Takes the plaintext rather than the stored ciphertext because the caller has
/// already decrypted it to decide whether the password actually changed, and
/// re-encrypting under a fresh nonce keeps a history row indistinguishable from
/// any other encrypted column.
pub(crate) fn record_password_change(
    conn: &Connection,
    key: &[u8; 32],
    entry_id: i64,
    old_password: &[u8],
    changed_at: &str,
) -> Result<(), AppError> {
    let limit = password_history_limit(conn);
    if limit == 0 {
        // History is off: discard anything retained from when it was on rather
        // than letting superseded secrets linger.
        return prune_history(conn, entry_id, 0);
    }
    let ct = crypto::encrypt(key, old_password)?;
    conn.execute(
        "INSERT INTO password_history
            (entry_id, encrypted_password, password_nonce, changed_at)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![entry_id, ct.bytes, ct.nonce.as_slice(), changed_at],
    )?;
    prune_history(conn, entry_id, limit)
}

/// Keep only the `limit` most recently recorded history rows for one entry.
///
/// Recency is `id`, not `changed_at`: ids are assigned in insertion order, while
/// `changed_at` comes from the clock (ties within a second) or, after an import,
/// straight out of the file.
pub(crate) fn prune_history(
    conn: &Connection,
    entry_id: i64,
    limit: u64,
) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM password_history
         WHERE entry_id = ?1
           AND id NOT IN (
               SELECT id FROM password_history
               WHERE entry_id = ?1
               ORDER BY id DESC
               LIMIT ?2
           )",
        rusqlite::params![entry_id, limit],
    )?;
    Ok(())
}

/// Trim every entry's history to `limit`, for when the retention setting is
/// lowered and the new limit has to apply to already-stored history.
pub(crate) fn prune_all_history(conn: &Connection, limit: u64) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM password_history
         WHERE id NOT IN (
             SELECT id FROM (
                 SELECT id, ROW_NUMBER() OVER (
                     PARTITION BY entry_id ORDER BY id DESC
                 ) AS rn
                 FROM password_history
             )
             WHERE rn <= ?1
         )",
        rusqlite::params![limit],
    )?;
    Ok(())
}

fn list_password_history_impl(
    state: &AppState,
    id: i64,
) -> Result<Vec<PasswordHistoryItem>, AppError> {
    with_unlocked(state, |s, key| {
        // An unknown entry is reported as such; an entry that simply has no
        // recorded history returns an empty list.
        let exists: Option<i64> = s
            .conn
            .query_row("SELECT id FROM password_entries WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        if exists.is_none() {
            return Err(AppError::EntryNotFound);
        }

        let mut stmt = s.conn.prepare(
            "SELECT id, encrypted_password, password_nonce, changed_at
             FROM password_history
             WHERE entry_id = ?1
             ORDER BY id DESC",
        )?;
        type HistoryRow = (i64, Vec<u8>, Vec<u8>, String);
        let rows: Vec<HistoryRow> = stmt
            .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let mut items = Vec::with_capacity(rows.len());
        for (history_id, ciphertext, nonce, changed_at) in rows {
            let plain = crypto::decrypt(key, &ciphertext, &nonce)?;
            let password = String::from_utf8(plain)
                .map_err(|_| AppError::Crypto("stored password is not valid utf-8"))?;
            items.push(PasswordHistoryItem {
                id: history_id,
                password,
                changed_at,
            });
        }
        Ok(items)
    })
}

/// Drop every retained previous password for one entry. Idempotent: clearing an
/// entry with no history (or no such entry) reports zero rows removed.
fn clear_password_history_impl(state: &AppState, id: i64) -> Result<usize, AppError> {
    with_authorized(state, |s| {
        let n = s
            .conn
            .execute("DELETE FROM password_history WHERE entry_id = ?1", [id])?;
        Ok(n)
    })
}

#[tauri::command]
pub fn list_password_history(
    state: State<'_, AppState>,
    id: i64,
) -> Result<Vec<PasswordHistoryItem>, AppError> {
    list_password_history_impl(&state, id)
}

#[tauri::command]
pub fn clear_password_history(
    state: State<'_, AppState>,
    id: i64,
) -> Result<usize, AppError> {
    clear_password_history_impl(&state, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::entries::{
        create_entry_for_test, update_entry_for_test, EntryInput, TotpUpdate,
    };
    use crate::db;
    use zeroize::Zeroizing;

    fn fixed_key() -> [u8; 32] {
        let mut k = [0u8; 32];
        for (i, b) in k.iter_mut().enumerate() {
            *b = i as u8;
        }
        k
    }

    fn unlocked_state() -> AppState {
        let state = AppState::new(db::open_in_memory().unwrap());
        state.inner.lock().unwrap().key = Some(Zeroizing::new(fixed_key()));
        state
    }

    fn sample_input(password: &str) -> EntryInput {
        EntryInput {
            category_id: None,
            title: "GitHub".into(),
            username: "alice".into(),
            url_or_app_name: "github.com".into(),
            password: password.into(),
            notes: None,
            totp: TotpUpdate::Keep,
            favorite: false,
            tags: vec![],
        }
    }

    fn set_limit(state: &AppState, limit: u64) {
        let guard = state.inner.lock().unwrap();
        guard
            .conn
            .execute(
                "INSERT INTO settings (key, value, updated_at)
                 VALUES ('password_history_limit', ?1, '2026-01-01T00:00:00Z')
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [limit.to_string()],
            )
            .unwrap();
    }

    fn passwords(state: &AppState, id: i64) -> Vec<String> {
        list_password_history_impl(state, id)
            .unwrap()
            .into_iter()
            .map(|h| h.password)
            .collect()
    }

    #[test]
    fn a_new_entry_has_no_history() {
        let state = unlocked_state();
        let id = create_entry_for_test(&state, sample_input("first")).unwrap();
        assert!(list_password_history_impl(&state, id).unwrap().is_empty());
    }

    #[test]
    fn rotating_a_password_records_the_superseded_one_newest_first() {
        let state = unlocked_state();
        let id = create_entry_for_test(&state, sample_input("first")).unwrap();
        update_entry_for_test(&state, id, sample_input("second")).unwrap();
        update_entry_for_test(&state, id, sample_input("third")).unwrap();

        // The current password is never in history; the superseded ones are,
        // most recently replaced first.
        assert_eq!(passwords(&state, id), vec!["second", "first"]);
    }

    #[test]
    fn a_metadata_only_edit_records_nothing() {
        let state = unlocked_state();
        let id = create_entry_for_test(&state, sample_input("first")).unwrap();
        let mut input = sample_input("first");
        input.title = "GitHub (work)".into();
        update_entry_for_test(&state, id, input).unwrap();
        assert!(list_password_history_impl(&state, id).unwrap().is_empty());
    }

    #[test]
    fn history_is_trimmed_to_the_retention_limit() {
        let state = unlocked_state();
        set_limit(&state, 2);
        let id = create_entry_for_test(&state, sample_input("p0")).unwrap();
        for i in 1..=5 {
            update_entry_for_test(&state, id, sample_input(&format!("p{i}"))).unwrap();
        }
        // p5 is current; only the two most recently superseded are retained.
        assert_eq!(passwords(&state, id), vec!["p4", "p3"]);
    }

    #[test]
    fn a_zero_limit_records_nothing_and_discards_existing_history() {
        let state = unlocked_state();
        let id = create_entry_for_test(&state, sample_input("p0")).unwrap();
        update_entry_for_test(&state, id, sample_input("p1")).unwrap();
        assert_eq!(passwords(&state, id), vec!["p0"]);

        // Turning history off must not leave the already-recorded secret behind.
        set_limit(&state, 0);
        update_entry_for_test(&state, id, sample_input("p2")).unwrap();
        assert!(passwords(&state, id).is_empty());
    }

    #[test]
    fn prune_all_history_applies_a_lowered_limit_to_every_entry() {
        let state = unlocked_state();
        let a = create_entry_for_test(&state, sample_input("a0")).unwrap();
        let b = create_entry_for_test(&state, sample_input("b0")).unwrap();
        for i in 1..=3 {
            update_entry_for_test(&state, a, sample_input(&format!("a{i}"))).unwrap();
            update_entry_for_test(&state, b, sample_input(&format!("b{i}"))).unwrap();
        }
        assert_eq!(passwords(&state, a).len(), 3);
        assert_eq!(passwords(&state, b).len(), 3);

        {
            let guard = state.inner.lock().unwrap();
            prune_all_history(&guard.conn, 1).unwrap();
        }
        // Every entry is trimmed independently, keeping its own newest row.
        assert_eq!(passwords(&state, a), vec!["a2"]);
        assert_eq!(passwords(&state, b), vec!["b2"]);
    }

    #[test]
    fn prune_all_history_with_zero_empties_the_table() {
        let state = unlocked_state();
        let id = create_entry_for_test(&state, sample_input("p0")).unwrap();
        update_entry_for_test(&state, id, sample_input("p1")).unwrap();
        {
            let guard = state.inner.lock().unwrap();
            prune_all_history(&guard.conn, 0).unwrap();
        }
        assert!(passwords(&state, id).is_empty());
    }

    #[test]
    fn clear_removes_only_the_requested_entrys_history() {
        let state = unlocked_state();
        let a = create_entry_for_test(&state, sample_input("a0")).unwrap();
        let b = create_entry_for_test(&state, sample_input("b0")).unwrap();
        update_entry_for_test(&state, a, sample_input("a1")).unwrap();
        update_entry_for_test(&state, b, sample_input("b1")).unwrap();

        assert_eq!(clear_password_history_impl(&state, a).unwrap(), 1);
        assert!(passwords(&state, a).is_empty());
        assert_eq!(passwords(&state, b), vec!["b0"]);
    }

    #[test]
    fn clearing_an_entry_without_history_is_a_no_op() {
        let state = unlocked_state();
        let id = create_entry_for_test(&state, sample_input("p0")).unwrap();
        assert_eq!(clear_password_history_impl(&state, id).unwrap(), 0);
    }

    #[test]
    fn deleting_an_entry_cascades_its_history_away() {
        let state = unlocked_state();
        let id = create_entry_for_test(&state, sample_input("p0")).unwrap();
        update_entry_for_test(&state, id, sample_input("p1")).unwrap();

        let remaining: i64 = {
            let guard = state.inner.lock().unwrap();
            guard
                .conn
                .execute("DELETE FROM password_entries WHERE id = ?1", [id])
                .unwrap();
            guard
                .conn
                .query_row("SELECT COUNT(*) FROM password_history", [], |r| r.get(0))
                .unwrap()
        };
        // Retained secrets must not outlive the entry they belong to.
        assert_eq!(remaining, 0);
    }

    #[test]
    fn history_reports_not_found_for_a_missing_entry() {
        let state = unlocked_state();
        assert!(matches!(
            list_password_history_impl(&state, 9999),
            Err(AppError::EntryNotFound)
        ));
    }

    #[test]
    fn history_is_refused_while_locked() {
        let state = AppState::new(db::open_in_memory().unwrap());
        assert!(matches!(
            list_password_history_impl(&state, 1),
            Err(AppError::Locked)
        ));
        assert!(matches!(
            clear_password_history_impl(&state, 1),
            Err(AppError::Locked)
        ));
    }

    #[test]
    fn recorded_passwords_are_stored_encrypted() {
        let state = unlocked_state();
        let id = create_entry_for_test(&state, sample_input("plaintext-canary")).unwrap();
        update_entry_for_test(&state, id, sample_input("rotated")).unwrap();

        let guard = state.inner.lock().unwrap();
        let stored: Vec<u8> = guard
            .conn
            .query_row(
                "SELECT encrypted_password FROM password_history WHERE entry_id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            !stored
                .windows(b"plaintext-canary".len())
                .any(|w| w == b"plaintext-canary"),
            "history row must not contain the password in the clear"
        );
    }
}
