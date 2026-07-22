use std::time::Instant;

use serde::Serialize;
use tauri::State;
use zeroize::Zeroizing;

use crate::crypto::{self, TEST_VALUE_PLAINTEXT};
use crate::db::now_iso8601;
use crate::error::AppError;
use crate::state::{with_state, AppState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    pub exists: bool,
    pub unlocked: bool,
    pub vault_name: Option<String>,
}

fn vault_row_exists(conn: &rusqlite::Connection) -> Result<bool, AppError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM vault_metadata WHERE id = 1",
        [],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

fn vault_status_impl(state: &AppState) -> Result<VaultStatus, AppError> {
    with_state(state, |s| {
        let vault_name: Option<String> = s
            .conn
            .query_row(
                "SELECT vault_name FROM vault_metadata WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .ok();
        Ok(VaultStatus {
            exists: vault_name.is_some(),
            unlocked: s.key.is_some(),
            vault_name,
        })
    })
}

#[tauri::command]
pub fn vault_status(state: State<'_, AppState>) -> Result<VaultStatus, AppError> {
    vault_status_impl(&state)
}

fn validate_master_password(password: &str) -> Result<(), AppError> {
    if password.is_empty() {
        return Err(AppError::Validation("master password must not be empty"));
    }
    if password.len() < 8 {
        return Err(AppError::Validation(
            "master password must be at least 8 characters",
        ));
    }
    Ok(())
}

fn create_vault_impl(
    state: &AppState,
    master_password: String,
    vault_name: Option<String>,
) -> Result<(), AppError> {
    validate_master_password(&master_password)?;

    let password = Zeroizing::new(master_password);
    let vault_name = vault_name.unwrap_or_else(|| "My Vault".to_string());

    let salt = crypto::generate_salt();
    let key = crypto::derive_key(&password, &salt)?;
    let test_ct = crypto::encrypt(&key, TEST_VALUE_PLAINTEXT)?;
    let now = now_iso8601();

    with_state(state, |s| {
        if vault_row_exists(&s.conn)? {
            return Err(AppError::VaultAlreadyExists);
        }
        s.conn.execute(
            "INSERT INTO vault_metadata
                (id, vault_name, kdf_algorithm, kdf_salt,
                 encrypted_test_value, test_value_nonce,
                 created_at, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            rusqlite::params![
                vault_name,
                "argon2id",
                salt.as_slice(),
                test_ct.bytes,
                test_ct.nonce.as_slice(),
                now,
            ],
        )?;
        s.key = Some(key);
        Ok(())
    })
}

#[tauri::command]
pub fn create_vault(
    state: State<'_, AppState>,
    master_password: String,
    vault_name: Option<String>,
) -> Result<(), AppError> {
    create_vault_impl(&state, master_password, vault_name)
}

/// (kdf_salt, encrypted_test_value, test_value_nonce)
type VaultCryptoRow = (Vec<u8>, Vec<u8>, Vec<u8>);

pub(crate) fn read_vault_crypto_row(
    conn: &rusqlite::Connection,
) -> Result<VaultCryptoRow, AppError> {
    if !vault_row_exists(conn)? {
        return Err(AppError::VaultNotFound);
    }
    let row = conn.query_row(
        "SELECT kdf_salt, encrypted_test_value, test_value_nonce
         FROM vault_metadata WHERE id = 1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    Ok(row)
}

/// Derive a key from `password` with the vault's stored salt and verify it
/// against the stored test value. Returns the key and the salt it was
/// derived from.
pub(crate) fn verify_password(
    salt: &[u8],
    encrypted_test: &[u8],
    test_nonce: &[u8],
    password: &str,
) -> Result<crypto::VaultKey, AppError> {
    let key = crypto::derive_key(password, salt)?;
    let decrypted = crypto::decrypt(&key, encrypted_test, test_nonce)
        .map_err(|_| AppError::WrongPassword)?;
    if decrypted != TEST_VALUE_PLAINTEXT {
        return Err(AppError::WrongPassword);
    }
    Ok(key)
}

/// Unlock attempts allowed with no delay before backoff starts.
pub(crate) const FREE_UNLOCK_ATTEMPTS: u32 = 5;
/// Upper bound on the escalating unlock backoff.
pub(crate) const MAX_UNLOCK_BACKOFF_SECS: u64 = 300;

/// Required wait (seconds) before the next unlock attempt after
/// `consecutive_failures` wrong master passwords: zero for the first
/// `FREE_UNLOCK_ATTEMPTS`, then 1s doubling per further failure, capped at
/// `MAX_UNLOCK_BACKOFF_SECS`.
///
/// This only slows *interactive* guessing through the app. It is not a defense
/// against offline brute force of a copied vault file - the Argon2id KDF cost
/// is what protects against that.
pub(crate) fn unlock_backoff_secs(consecutive_failures: u32) -> u64 {
    if consecutive_failures <= FREE_UNLOCK_ATTEMPTS {
        return 0;
    }
    let over = consecutive_failures - FREE_UNLOCK_ATTEMPTS; // >= 1
    let secs = 1u64.checked_shl(over - 1).unwrap_or(u64::MAX);
    secs.min(MAX_UNLOCK_BACKOFF_SECS)
}

/// Remaining wait given the failure count and seconds elapsed since the last
/// failed attempt. Zero means an attempt is allowed now.
pub(crate) fn remaining_unlock_backoff_secs(consecutive_failures: u32, elapsed_secs: u64) -> u64 {
    unlock_backoff_secs(consecutive_failures).saturating_sub(elapsed_secs)
}

fn unlock_vault_impl(state: &AppState, master_password: String) -> Result<(), AppError> {
    let password = Zeroizing::new(master_password);

    // Enforce interactive-guessing backoff BEFORE the expensive Argon2id
    // derivation, so a locked-out attempt costs no CPU.
    let (salt, encrypted_test, test_nonce) = with_state(state, |s| {
        let elapsed = s
            .last_failed_unlock
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(u64::MAX);
        let wait = remaining_unlock_backoff_secs(s.failed_unlocks, elapsed);
        if wait > 0 {
            return Err(AppError::TooManyUnlockAttempts(wait));
        }
        read_vault_crypto_row(&s.conn)
    })?;

    match verify_password(&salt, &encrypted_test, &test_nonce, &password) {
        Ok(key) => with_state(state, |s| {
            s.key = Some(key);
            s.failed_unlocks = 0;
            s.last_failed_unlock = None;
            Ok(())
        }),
        Err(e) => {
            with_state(state, |s| {
                s.failed_unlocks = s.failed_unlocks.saturating_add(1);
                s.last_failed_unlock = Some(Instant::now());
                Ok(())
            })?;
            Err(e)
        }
    }
}

#[tauri::command]
pub fn unlock_vault(
    state: State<'_, AppState>,
    master_password: String,
) -> Result<(), AppError> {
    unlock_vault_impl(&state, master_password)
}

/// Locks the vault and hands back the clipboard token (if any) so the caller
/// can clear the OS clipboard when the last copy came from us.
fn lock_vault_impl(state: &AppState) -> Result<Option<String>, AppError> {
    with_state(state, |s| {
        s.key = None;
        Ok(s.clipboard_token.take())
    })
}

#[tauri::command]
pub fn lock_vault(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    use tauri_plugin_clipboard_manager::ClipboardExt;

    let token = lock_vault_impl(&state)?;
    // Same safety check as the delayed clear in commands/clipboard.rs: only
    // wipe the clipboard if it still holds the value we put there.
    if let Some(token) = token {
        let current = app.clipboard().read_text().ok();
        if crate::commands::clipboard::is_our_clipboard_value(current.as_deref(), &token) {
            if let Err(e) = app.clipboard().clear() {
                log::warn!("failed to clear clipboard on lock: {e}");
            }
        }
    }
    Ok(())
}

/// An optional encrypted field re-encoded under a new key, as `(ciphertext, nonce)`.
type ReencryptedField = (Option<Vec<u8>>, Option<Vec<u8>>);

/// Re-encrypt an optional encrypted column from `old_key` to `new_key`,
/// preserving `NULL` when the field is absent. Used by the master-password
/// change to rotate every encrypted column (notes and the TOTP secret alike).
fn reencrypt_optional(
    old_key: &[u8; 32],
    new_key: &[u8; 32],
    ciphertext: Option<Vec<u8>>,
    nonce: Option<Vec<u8>>,
) -> Result<ReencryptedField, AppError> {
    match (ciphertext, nonce) {
        (Some(c), Some(n)) => {
            let plain = Zeroizing::new(crypto::decrypt(old_key, &c, &n)?);
            let ct = crypto::encrypt(new_key, &plain)?;
            Ok((Some(ct.bytes), Some(ct.nonce.to_vec())))
        }
        _ => Ok((None, None)),
    }
}

fn change_master_password_impl(
    state: &AppState,
    current_password: String,
    new_password: String,
) -> Result<(), AppError> {
    validate_master_password(&new_password)?;

    let current_password = Zeroizing::new(current_password);
    let new_password = Zeroizing::new(new_password);

    let (salt, encrypted_test, test_nonce) = with_state(state, |s| {
        if s.key.is_none() {
            return Err(AppError::Locked);
        }
        read_vault_crypto_row(&s.conn)
    })?;

    // Both derivations happen outside the state lock; they are slow.
    let old_key = verify_password(&salt, &encrypted_test, &test_nonce, &current_password)?;

    let new_salt = crypto::generate_salt();
    let new_key = crypto::derive_key(&new_password, &new_salt)?;
    let new_test_ct = crypto::encrypt(&new_key, TEST_VALUE_PLAINTEXT)?;

    with_state(state, |s| {
        if s.key.is_none() {
            return Err(AppError::Locked);
        }
        let tx = s.conn.transaction()?;

        // Guard against the vault having been re-keyed between the unlocked
        // verification above and this transaction.
        let current_salt: Vec<u8> =
            tx.query_row("SELECT kdf_salt FROM vault_metadata WHERE id = 1", [], |r| {
                r.get(0)
            })?;
        if current_salt != salt {
            return Err(AppError::Internal(
                "vault key changed during password change".into(),
            ));
        }

        let now = now_iso8601();
        tx.execute(
            "UPDATE vault_metadata SET
                kdf_salt = ?1,
                encrypted_test_value = ?2,
                test_value_nonce = ?3,
                updated_at = ?4
             WHERE id = 1",
            rusqlite::params![
                new_salt.as_slice(),
                new_test_ct.bytes,
                new_test_ct.nonce.as_slice(),
                now,
            ],
        )?;

        type EntryCryptoRow = (
            i64,
            Vec<u8>,
            Vec<u8>,
            Option<Vec<u8>>,
            Option<Vec<u8>>,
            Option<Vec<u8>>,
            Option<Vec<u8>>,
        );
        let mut stmt = tx.prepare(
            "SELECT id, encrypted_password, password_nonce,
                    encrypted_notes, notes_nonce,
                    encrypted_totp, totp_nonce
             FROM password_entries",
        )?;
        let rows: Vec<EntryCryptoRow> = stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        for (id, enc_pw, pw_nonce, enc_notes, notes_nonce, enc_totp, totp_nonce) in rows {
            let pw_plain = Zeroizing::new(crypto::decrypt(&old_key, &enc_pw, &pw_nonce)?);
            let pw_ct = crypto::encrypt(&new_key, &pw_plain)?;

            let (notes_bytes, notes_nonce_new) =
                reencrypt_optional(&old_key, &new_key, enc_notes, notes_nonce)?;
            let (totp_bytes, totp_nonce_new) =
                reencrypt_optional(&old_key, &new_key, enc_totp, totp_nonce)?;

            tx.execute(
                "UPDATE password_entries SET
                    encrypted_password = ?1,
                    password_nonce = ?2,
                    encrypted_notes = ?3,
                    notes_nonce = ?4,
                    encrypted_totp = ?5,
                    totp_nonce = ?6
                 WHERE id = ?7",
                rusqlite::params![
                    pw_ct.bytes,
                    pw_ct.nonce.as_slice(),
                    notes_bytes,
                    notes_nonce_new,
                    totp_bytes,
                    totp_nonce_new,
                    id,
                ],
            )?;
        }

        tx.commit()?;

        // Only after the commit is the new key authoritative; the old key
        // (Zeroizing) is wiped when it drops.
        s.key = Some(new_key);
        Ok(())
    })
}

#[tauri::command]
pub fn change_master_password(
    state: State<'_, AppState>,
    current_password: String,
    new_password: String,
) -> Result<(), AppError> {
    change_master_password_impl(&state, current_password, new_password)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    const PW_OLD: &str = "old-password-123";
    const PW_NEW: &str = "new-password-456";

    fn state_with_vault() -> AppState {
        let state = AppState::new(db::open_in_memory().unwrap());
        create_vault_impl(&state, PW_OLD.into(), None).unwrap();
        state
    }

    fn current_key(state: &AppState) -> [u8; 32] {
        **state.inner.lock().unwrap().key.as_ref().unwrap()
    }

    /// Insert an entry encrypted with the vault's current in-memory key,
    /// bypassing the entries module (its impl fns are private to it).
    fn insert_entry(state: &AppState, title: &str, password: &str, notes: Option<&str>) -> i64 {
        let key = current_key(state);
        let pw_ct = crypto::encrypt(&key, password.as_bytes()).unwrap();
        let (notes_bytes, notes_nonce) = match notes {
            Some(n) => {
                let ct = crypto::encrypt(&key, n.as_bytes()).unwrap();
                (Some(ct.bytes), Some(ct.nonce.to_vec()))
            }
            None => (None, None),
        };
        let guard = state.inner.lock().unwrap();
        guard
            .conn
            .execute(
                "INSERT INTO password_entries
                    (title, username, url_or_app_name,
                     encrypted_password, password_nonce,
                     encrypted_notes, notes_nonce, created_at, updated_at)
                 VALUES (?1, 'user', 'example.com', ?2, ?3, ?4, ?5,
                         '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                rusqlite::params![
                    title,
                    pw_ct.bytes,
                    pw_ct.nonce.as_slice(),
                    notes_bytes,
                    notes_nonce,
                ],
            )
            .unwrap();
        guard.conn.last_insert_rowid()
    }

    fn decrypt_entry(state: &AppState, id: i64) -> (String, Option<String>) {
        let key = current_key(state);
        let guard = state.inner.lock().unwrap();
        type StoredEntryCiphertext = (Vec<u8>, Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>);
        let (enc_pw, pw_nonce, enc_notes, notes_nonce): StoredEntryCiphertext = guard
            .conn
            .query_row(
                "SELECT encrypted_password, password_nonce, encrypted_notes, notes_nonce
                 FROM password_entries WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        drop(guard);
        let pw = String::from_utf8(crypto::decrypt(&key, &enc_pw, &pw_nonce).unwrap()).unwrap();
        let notes = match (enc_notes, notes_nonce) {
            (Some(c), Some(n)) => {
                Some(String::from_utf8(crypto::decrypt(&key, &c, &n).unwrap()).unwrap())
            }
            _ => None,
        };
        (pw, notes)
    }

    fn kdf_salt(state: &AppState) -> Vec<u8> {
        state
            .inner
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT kdf_salt FROM vault_metadata WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap()
    }

    #[test]
    fn create_then_status_reports_exists_and_unlocked() {
        let state = state_with_vault();
        let status = vault_status_impl(&state).unwrap();
        assert!(status.exists);
        assert!(status.unlocked);
        assert_eq!(status.vault_name.as_deref(), Some("My Vault"));
    }

    #[test]
    fn status_reports_custom_vault_name() {
        let state = AppState::new(db::open_in_memory().unwrap());
        create_vault_impl(&state, PW_OLD.into(), Some("Family Passwords".into())).unwrap();
        let status = vault_status_impl(&state).unwrap();
        assert_eq!(status.vault_name.as_deref(), Some("Family Passwords"));
    }

    #[test]
    fn status_reports_no_name_before_vault_exists() {
        let state = AppState::new(db::open_in_memory().unwrap());
        let status = vault_status_impl(&state).unwrap();
        assert!(!status.exists);
        assert!(status.vault_name.is_none());
    }

    #[test]
    fn unlock_with_correct_password_succeeds() {
        let state = state_with_vault();
        lock_vault_impl(&state).unwrap();
        unlock_vault_impl(&state, PW_OLD.into()).unwrap();
        assert!(vault_status_impl(&state).unwrap().unlocked);
    }

    #[test]
    fn unlock_with_wrong_password_fails() {
        let state = state_with_vault();
        lock_vault_impl(&state).unwrap();
        assert!(matches!(
            unlock_vault_impl(&state, "wrong-password".into()),
            Err(AppError::WrongPassword)
        ));
    }

    #[test]
    fn backoff_is_zero_until_free_attempts_are_used() {
        for f in 0..=FREE_UNLOCK_ATTEMPTS {
            assert_eq!(unlock_backoff_secs(f), 0, "no backoff for {f} failures");
        }
        // Then 1s doubling per further failure.
        assert_eq!(unlock_backoff_secs(FREE_UNLOCK_ATTEMPTS + 1), 1);
        assert_eq!(unlock_backoff_secs(FREE_UNLOCK_ATTEMPTS + 2), 2);
        assert_eq!(unlock_backoff_secs(FREE_UNLOCK_ATTEMPTS + 3), 4);
        assert_eq!(unlock_backoff_secs(FREE_UNLOCK_ATTEMPTS + 4), 8);
        // Escalation is capped and never overflows.
        assert_eq!(unlock_backoff_secs(1000), MAX_UNLOCK_BACKOFF_SECS);
        assert_eq!(unlock_backoff_secs(u32::MAX), MAX_UNLOCK_BACKOFF_SECS);
    }

    #[test]
    fn remaining_backoff_counts_down_with_elapsed_time() {
        let failures = FREE_UNLOCK_ATTEMPTS + 3; // 4s backoff
        assert_eq!(remaining_unlock_backoff_secs(failures, 0), 4);
        assert_eq!(remaining_unlock_backoff_secs(failures, 3), 1);
        assert_eq!(remaining_unlock_backoff_secs(failures, 4), 0);
        assert_eq!(remaining_unlock_backoff_secs(failures, 99), 0);
        // Inside the free window there is never a wait.
        assert_eq!(remaining_unlock_backoff_secs(1, 0), 0);
    }

    #[test]
    fn repeated_wrong_passwords_lock_out_even_the_correct_one() {
        let state = state_with_vault();
        lock_vault_impl(&state).unwrap();
        // Use up the free attempts plus one more to arm the backoff.
        for _ in 0..=FREE_UNLOCK_ATTEMPTS {
            assert!(matches!(
                unlock_vault_impl(&state, "wrong".into()),
                Err(AppError::WrongPassword)
            ));
        }
        // The next attempt is refused by backoff before any password check -
        // so even the correct password is rejected until the wait elapses.
        assert!(matches!(
            unlock_vault_impl(&state, PW_OLD.into()),
            Err(AppError::TooManyUnlockAttempts(_))
        ));
    }

    #[test]
    fn successful_unlock_resets_the_failure_counter() {
        let state = state_with_vault();
        lock_vault_impl(&state).unwrap();
        // A few failures, staying within the free window.
        for _ in 0..3 {
            let _ = unlock_vault_impl(&state, "wrong".into());
        }
        unlock_vault_impl(&state, PW_OLD.into()).unwrap();
        let guard = state.inner.lock().unwrap();
        assert_eq!(guard.failed_unlocks, 0);
        assert!(guard.last_failed_unlock.is_none());
    }

    #[test]
    fn preexisting_v0_vault_file_unlocks_and_decrypts_after_upgrade() {
        // Reproduce on disk exactly what the pre-migration build wrote:
        // schema.sql applied directly, user_version left at 0, rows encrypted
        // with the unchanged Argon2id/AES-GCM scheme. Opening it through the
        // current open_and_migrate must adopt it losslessly.
        let path = std::env::temp_dir().join(format!(
            "pm-test-{}-preexisting-vault.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let salt = crypto::generate_salt();
        let key = crypto::derive_key(PW_OLD, &salt).unwrap();
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch(include_str!("../db/schema.sql")).unwrap();
            let test_ct = crypto::encrypt(&key, TEST_VALUE_PLAINTEXT).unwrap();
            conn.execute(
                "INSERT INTO vault_metadata
                    (id, vault_name, kdf_algorithm, kdf_salt,
                     encrypted_test_value, test_value_nonce, created_at, updated_at)
                 VALUES (1, 'Old Vault', 'argon2id', ?1, ?2, ?3,
                         '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                rusqlite::params![salt.as_slice(), test_ct.bytes, test_ct.nonce.as_slice()],
            )
            .unwrap();
            let pw_ct = crypto::encrypt(&key, b"hunter2").unwrap();
            let notes_ct = crypto::encrypt(&key, b"old note").unwrap();
            conn.execute(
                "INSERT INTO password_entries
                    (title, username, url_or_app_name,
                     encrypted_password, password_nonce,
                     encrypted_notes, notes_nonce, created_at, updated_at)
                 VALUES ('GitHub', 'alice', 'github.com', ?1, ?2, ?3, ?4,
                         '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                rusqlite::params![
                    pw_ct.bytes,
                    pw_ct.nonce.as_slice(),
                    notes_ct.bytes,
                    notes_ct.nonce.as_slice(),
                ],
            )
            .unwrap();
        }

        let conn = crate::db::open_and_migrate(&path).unwrap();
        let state = AppState::new(conn);
        unlock_vault_impl(&state, PW_OLD.into()).unwrap();
        let status = vault_status_impl(&state).unwrap();
        assert!(status.unlocked);
        assert_eq!(status.vault_name.as_deref(), Some("Old Vault"));
        assert_eq!(
            decrypt_entry(&state, 1),
            ("hunter2".to_string(), Some("old note".to_string()))
        );

        drop(state);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(path.with_extension(format!("db{suffix}")));
        }
    }

    #[test]
    fn lock_returns_clipboard_token_and_clears_state() {
        let state = state_with_vault();
        state.inner.lock().unwrap().clipboard_token = Some("copied-secret".into());
        let token = lock_vault_impl(&state).unwrap();
        assert_eq!(token.as_deref(), Some("copied-secret"));
        let guard = state.inner.lock().unwrap();
        assert!(guard.key.is_none());
        assert!(guard.clipboard_token.is_none());
    }

    #[test]
    fn lock_returns_no_token_when_nothing_was_copied() {
        let state = state_with_vault();
        assert!(lock_vault_impl(&state).unwrap().is_none());
    }

    #[test]
    fn change_rejects_wrong_current_password() {
        let state = state_with_vault();
        let err = change_master_password_impl(&state, "not-the-password".into(), PW_NEW.into())
            .unwrap_err();
        assert!(matches!(err, AppError::WrongPassword));

        // Old password must still work.
        lock_vault_impl(&state).unwrap();
        unlock_vault_impl(&state, PW_OLD.into()).unwrap();
    }

    #[test]
    fn change_rejects_short_new_password() {
        let state = state_with_vault();
        let err =
            change_master_password_impl(&state, PW_OLD.into(), "short".into()).unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn change_rejects_when_locked() {
        let state = state_with_vault();
        lock_vault_impl(&state).unwrap();
        assert!(matches!(
            change_master_password_impl(&state, PW_OLD.into(), PW_NEW.into()),
            Err(AppError::Locked)
        ));
    }

    #[test]
    fn change_reencrypts_entries_and_swaps_password() {
        let state = state_with_vault();
        let id1 = insert_entry(&state, "GitHub", "hunter2", Some("note one"));
        let id2 = insert_entry(&state, "Bank", "s3cret!", None);
        let old_salt = kdf_salt(&state);

        change_master_password_impl(&state, PW_OLD.into(), PW_NEW.into()).unwrap();

        assert_ne!(kdf_salt(&state), old_salt, "salt must be rotated");

        // Entries decrypt to identical plaintext under the new in-memory key.
        assert_eq!(
            decrypt_entry(&state, id1),
            ("hunter2".to_string(), Some("note one".to_string()))
        );
        assert_eq!(decrypt_entry(&state, id2), ("s3cret!".to_string(), None));

        // Old password no longer unlocks; new one does, and entries still
        // decrypt after a fresh unlock.
        lock_vault_impl(&state).unwrap();
        assert!(matches!(
            unlock_vault_impl(&state, PW_OLD.into()),
            Err(AppError::WrongPassword)
        ));
        unlock_vault_impl(&state, PW_NEW.into()).unwrap();
        assert_eq!(
            decrypt_entry(&state, id1),
            ("hunter2".to_string(), Some("note one".to_string()))
        );
    }

    #[test]
    fn change_reencrypts_totp_secret_so_it_survives_a_rekey() {
        use crate::commands::entries::encrypt_totp;
        use crate::crypto::totp::TotpConfig;

        // RFC 6238 SHA1 seed, base32-encoded.
        const RFC_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

        let state = state_with_vault();
        let id = insert_entry(&state, "GitHub", "hunter2", None);

        // Attach a TOTP secret encrypted under the current (old) vault key.
        let old_key = current_key(&state);
        let (totp_bytes, totp_nonce) = encrypt_totp(&old_key, RFC_SECRET).unwrap();
        {
            let guard = state.inner.lock().unwrap();
            guard
                .conn
                .execute(
                    "UPDATE password_entries SET encrypted_totp = ?1, totp_nonce = ?2
                     WHERE id = ?3",
                    rusqlite::params![totp_bytes, totp_nonce, id],
                )
                .unwrap();
        }

        change_master_password_impl(&state, PW_OLD.into(), PW_NEW.into()).unwrap();

        // After the rekey the TOTP blob must decrypt under the NEW key and still
        // hold the original secret. Before the fix it was left encrypted under
        // the old key, whose salt is gone, making it permanently unrecoverable.
        let new_key = current_key(&state);
        let (enc, nonce): (Vec<u8>, Vec<u8>) = {
            let guard = state.inner.lock().unwrap();
            guard
                .conn
                .query_row(
                    "SELECT encrypted_totp, totp_nonce FROM password_entries WHERE id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap()
        };
        let json = crypto::decrypt(&new_key, &enc, &nonce).unwrap();
        let config: TotpConfig = serde_json::from_slice(&json).unwrap();
        assert_eq!(config.secret_base32, RFC_SECRET);

        // And it still decrypts after a fresh lock/unlock with the new password.
        lock_vault_impl(&state).unwrap();
        unlock_vault_impl(&state, PW_NEW.into()).unwrap();
        let reunlocked_key = current_key(&state);
        let json2 = crypto::decrypt(&reunlocked_key, &enc, &nonce).unwrap();
        let config2: TotpConfig = serde_json::from_slice(&json2).unwrap();
        assert_eq!(config2.secret_base32, RFC_SECRET);
    }

    #[test]
    fn failed_change_rolls_back_and_old_password_still_works() {
        let state = state_with_vault();
        let good_id = insert_entry(&state, "GitHub", "hunter2", Some("note"));
        let old_salt = kdf_salt(&state);

        // An entry whose ciphertext cannot be decrypted forces a failure
        // mid-transaction, after vault_metadata has already been updated.
        {
            let guard = state.inner.lock().unwrap();
            guard
                .conn
                .execute(
                    "INSERT INTO password_entries
                        (title, username, url_or_app_name,
                         encrypted_password, password_nonce, created_at, updated_at)
                     VALUES ('Corrupt', 'u', 'x', X'00112233', X'000102030405060708090A0B',
                             '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                    [],
                )
                .unwrap();
        }

        let err =
            change_master_password_impl(&state, PW_OLD.into(), PW_NEW.into()).unwrap_err();
        assert!(matches!(err, AppError::Crypto(_)));

        // Everything rolled back: same salt, old password unlocks, entry
        // decrypts with the old key.
        assert_eq!(kdf_salt(&state), old_salt);
        lock_vault_impl(&state).unwrap();
        unlock_vault_impl(&state, PW_OLD.into()).unwrap();
        assert_eq!(
            decrypt_entry(&state, good_id),
            ("hunter2".to_string(), Some("note".to_string()))
        );
    }
}
