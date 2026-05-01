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

#[tauri::command]
pub fn vault_status(state: State<'_, AppState>) -> Result<VaultStatus, AppError> {
    with_state(&state, |s| {
        Ok(VaultStatus {
            exists: vault_row_exists(&s.conn)?,
            unlocked: s.key.is_some(),
        })
    })
}

#[tauri::command]
pub fn create_vault(
    state: State<'_, AppState>,
    master_password: String,
    vault_name: Option<String>,
) -> Result<(), AppError> {
    if master_password.is_empty() {
        return Err(AppError::Validation("master password must not be empty"));
    }
    if master_password.len() < 8 {
        return Err(AppError::Validation(
            "master password must be at least 8 characters",
        ));
    }

    let password = Zeroizing::new(master_password);
    let vault_name = vault_name.unwrap_or_else(|| "My Vault".to_string());

    let salt = crypto::generate_salt();
    let key = crypto::derive_key(&password, &salt)?;
    let test_ct = crypto::encrypt(&key, TEST_VALUE_PLAINTEXT)?;
    let now = now_iso8601();

    with_state(&state, |s| {
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
pub fn unlock_vault(
    state: State<'_, AppState>,
    master_password: String,
) -> Result<(), AppError> {
    let password = Zeroizing::new(master_password);

    let (salt, encrypted_test, test_nonce) = with_state(&state, |s| {
        if !vault_row_exists(&s.conn)? {
            return Err(AppError::VaultNotFound);
        }
        let row: (Vec<u8>, Vec<u8>, Vec<u8>) = s.conn.query_row(
            "SELECT kdf_salt, encrypted_test_value, test_value_nonce
             FROM vault_metadata WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        Ok(row)
    })?;

    let key = crypto::derive_key(&password, &salt)?;
    let decrypted = crypto::decrypt(&key, &encrypted_test, &test_nonce)
        .map_err(|_| AppError::WrongPassword)?;
    if decrypted != TEST_VALUE_PLAINTEXT {
        return Err(AppError::WrongPassword);
    }

    with_state(&state, |s| {
        s.key = Some(key);
        Ok(())
    })
}

#[tauri::command]
pub fn lock_vault(state: State<'_, AppState>) -> Result<(), AppError> {
    with_state(&state, |s| {
        s.key = None;
        s.clipboard_token = None;
        Ok(())
    })
}
