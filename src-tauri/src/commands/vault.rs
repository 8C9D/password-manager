use serde::Serialize;
use tauri::State;
use zeroize::Zeroizing;

use crate::crypto::{self, TEST_VALUE_PLAINTEXT};
use crate::db::now_iso8601;
use crate::error::AppError;
use crate::state::{with_state, AppState};

#[derive(Serialize)]
pub struct VaultStatus {
    pub exists: bool,
    pub unlocked: bool,
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
        Ok(VaultStatus {
            exists: vault_row_exists(&s.conn)?,
            unlocked: s.key.is_some(),
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

fn unlock_vault_impl(state: &AppState, master_password: String) -> Result<(), AppError> {
    let password = Zeroizing::new(master_password);

    let (salt, encrypted_test, test_nonce) =
        with_state(state, |s| read_vault_crypto_row(&s.conn))?;

    let key = verify_password(&salt, &encrypted_test, &test_nonce, &password)?;

    with_state(state, |s| {
        s.key = Some(key);
        Ok(())
    })
}

#[tauri::command]
pub fn unlock_vault(
    state: State<'_, AppState>,
    master_password: String,
) -> Result<(), AppError> {
    unlock_vault_impl(&state, master_password)
}

fn lock_vault_impl(state: &AppState) -> Result<(), AppError> {
    with_state(state, |s| {
        s.key = None;
        s.clipboard_token = None;
        Ok(())
    })
}

#[tauri::command]
pub fn lock_vault(state: State<'_, AppState>) -> Result<(), AppError> {
    lock_vault_impl(&state)
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

        type EntryCryptoRow = (i64, Vec<u8>, Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>);
        let mut stmt = tx.prepare(
            "SELECT id, encrypted_password, password_nonce,
                    encrypted_notes, notes_nonce
             FROM password_entries",
        )?;
        let rows: Vec<EntryCryptoRow> = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        for (id, enc_pw, pw_nonce, enc_notes, notes_nonce) in rows {
            let pw_plain = Zeroizing::new(crypto::decrypt(&old_key, &enc_pw, &pw_nonce)?);
            let pw_ct = crypto::encrypt(&new_key, &pw_plain)?;

            let (notes_bytes, notes_nonce_new) = match (enc_notes, notes_nonce) {
                (Some(c), Some(n)) => {
                    let notes_plain = Zeroizing::new(crypto::decrypt(&old_key, &c, &n)?);
                    let ct = crypto::encrypt(&new_key, &notes_plain)?;
                    (Some(ct.bytes), Some(ct.nonce.to_vec()))
                }
                _ => (None, None),
            };

            tx.execute(
                "UPDATE password_entries SET
                    encrypted_password = ?1,
                    password_nonce = ?2,
                    encrypted_notes = ?3,
                    notes_nonce = ?4
                 WHERE id = ?5",
                rusqlite::params![
                    pw_ct.bytes,
                    pw_ct.nonce.as_slice(),
                    notes_bytes,
                    notes_nonce_new,
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
