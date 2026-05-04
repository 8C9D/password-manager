use serde::{Deserialize, Serialize};
use tauri::State;

use crate::crypto;
use crate::db::now_iso8601;
use crate::error::AppError;
use crate::state::{with_state, with_unlocked, AppState};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryInput {
    pub category_id: Option<i64>,
    pub title: String,
    pub username: String,
    pub url_or_app_name: String,
    pub password: String,
    pub notes: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntrySummary {
    pub id: i64,
    pub category_id: Option<i64>,
    pub title: String,
    pub username: String,
    pub url_or_app_name: String,
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryFull {
    pub id: i64,
    pub category_id: Option<i64>,
    pub title: String,
    pub username: String,
    pub url_or_app_name: String,
    pub password: String,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
}

fn validate_input(input: &EntryInput) -> Result<(), AppError> {
    if input.title.trim().is_empty() {
        return Err(AppError::Validation("title is required"));
    }
    if input.password.is_empty() {
        return Err(AppError::Validation("password is required"));
    }
    Ok(())
}

fn encrypt_optional(
    key: &[u8; 32],
    plaintext: Option<&str>,
) -> Result<(Option<Vec<u8>>, Option<Vec<u8>>), AppError> {
    match plaintext {
        Some(s) if !s.is_empty() => {
            let ct = crypto::encrypt(key, s.as_bytes())?;
            Ok((Some(ct.bytes), Some(ct.nonce.to_vec())))
        }
        _ => Ok((None, None)),
    }
}

#[tauri::command]
pub fn create_entry(
    state: State<'_, AppState>,
    input: EntryInput,
) -> Result<i64, AppError> {
    validate_input(&input)?;

    with_unlocked(&state, |s, key| {
        let pw_ct = crypto::encrypt(key, input.password.as_bytes())?;
        let (notes_bytes, notes_nonce) = encrypt_optional(key, input.notes.as_deref())?;
        let now = now_iso8601();
        s.conn.execute(
            "INSERT INTO password_entries
                (category_id, title, username, url_or_app_name,
                 encrypted_password, password_nonce,
                 encrypted_notes, notes_nonce,
                 created_at, updated_at, last_used_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, NULL)",
            rusqlite::params![
                input.category_id,
                input.title.trim(),
                input.username,
                input.url_or_app_name,
                pw_ct.bytes,
                pw_ct.nonce.as_slice(),
                notes_bytes,
                notes_nonce,
                now,
            ],
        )?;
        Ok(s.conn.last_insert_rowid())
    })
}

#[tauri::command]
pub fn list_entries(state: State<'_, AppState>) -> Result<Vec<EntrySummary>, AppError> {
    with_state(&state, |s| {
        if s.key.is_none() {
            return Err(AppError::Locked);
        }
        let mut stmt = s.conn.prepare(
            "SELECT id, category_id, title, username, url_or_app_name,
                    created_at, updated_at, last_used_at
             FROM password_entries
             ORDER BY title COLLATE NOCASE ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(EntrySummary {
                    id: r.get(0)?,
                    category_id: r.get(1)?,
                    title: r.get(2)?,
                    username: r.get(3)?,
                    url_or_app_name: r.get(4)?,
                    created_at: r.get(5)?,
                    updated_at: r.get(6)?,
                    last_used_at: r.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

#[tauri::command]
pub fn get_entry(state: State<'_, AppState>, id: i64) -> Result<EntryFull, AppError> {
    with_unlocked(&state, |s, key| {
        let row: Option<(
            i64,
            Option<i64>,
            String,
            String,
            String,
            Vec<u8>,
            Vec<u8>,
            Option<Vec<u8>>,
            Option<Vec<u8>>,
            String,
            String,
            Option<String>,
        )> = s
            .conn
            .query_row(
                "SELECT id, category_id, title, username, url_or_app_name,
                        encrypted_password, password_nonce,
                        encrypted_notes, notes_nonce,
                        created_at, updated_at, last_used_at
                 FROM password_entries WHERE id = ?1",
                [id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                        r.get(9)?,
                        r.get(10)?,
                        r.get(11)?,
                    ))
                },
            )
            .ok();

        let (
            id,
            category_id,
            title,
            username,
            url_or_app_name,
            enc_pw,
            pw_nonce,
            enc_notes,
            notes_nonce,
            created_at,
            updated_at,
            _last_used_at,
        ) = row.ok_or(AppError::EntryNotFound)?;

        let password_bytes = crypto::decrypt(key, &enc_pw, &pw_nonce)?;
        let password = String::from_utf8(password_bytes)
            .map_err(|_| AppError::Crypto("password is not valid utf-8"))?;

        let notes = match (enc_notes, notes_nonce) {
            (Some(c), Some(n)) => {
                let pt = crypto::decrypt(key, &c, &n)?;
                Some(
                    String::from_utf8(pt)
                        .map_err(|_| AppError::Crypto("notes are not valid utf-8"))?,
                )
            }
            _ => None,
        };

        let now = now_iso8601();
        s.conn.execute(
            "UPDATE password_entries SET last_used_at = ?1 WHERE id = ?2",
            rusqlite::params![now, id],
        )?;

        Ok(EntryFull {
            id,
            category_id,
            title,
            username,
            url_or_app_name,
            password,
            notes,
            created_at,
            updated_at,
            last_used_at: Some(now),
        })
    })
}

#[tauri::command]
pub fn update_entry(
    state: State<'_, AppState>,
    id: i64,
    input: EntryInput,
) -> Result<(), AppError> {
    validate_input(&input)?;

    with_unlocked(&state, |s, key| {
        let pw_ct = crypto::encrypt(key, input.password.as_bytes())?;
        let (notes_bytes, notes_nonce) = encrypt_optional(key, input.notes.as_deref())?;
        let now = now_iso8601();
        let n = s.conn.execute(
            "UPDATE password_entries SET
                category_id = ?1,
                title = ?2,
                username = ?3,
                url_or_app_name = ?4,
                encrypted_password = ?5,
                password_nonce = ?6,
                encrypted_notes = ?7,
                notes_nonce = ?8,
                updated_at = ?9
             WHERE id = ?10",
            rusqlite::params![
                input.category_id,
                input.title.trim(),
                input.username,
                input.url_or_app_name,
                pw_ct.bytes,
                pw_ct.nonce.as_slice(),
                notes_bytes,
                notes_nonce,
                now,
                id,
            ],
        )?;
        if n == 0 {
            return Err(AppError::EntryNotFound);
        }
        Ok(())
    })
}

#[tauri::command]
pub fn delete_entry(state: State<'_, AppState>, id: i64) -> Result<(), AppError> {
    with_unlocked(&state, |s, _key| {
        let n = s.conn.execute(
            "DELETE FROM password_entries WHERE id = ?1",
            rusqlite::params![id],
        )?;
        if n == 0 {
            return Err(AppError::EntryNotFound);
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_key() -> [u8; 32] {
        let mut k = [0u8; 32];
        for (i, b) in k.iter_mut().enumerate() {
            *b = i as u8;
        }
        k
    }

    #[test]
    fn encrypt_optional_returns_none_for_none() {
        let (bytes, nonce) = encrypt_optional(&fixed_key(), None).unwrap();
        assert!(bytes.is_none());
        assert!(nonce.is_none());
    }

    #[test]
    fn encrypt_optional_returns_none_for_empty_string() {
        let (bytes, nonce) = encrypt_optional(&fixed_key(), Some("")).unwrap();
        assert!(bytes.is_none());
        assert!(nonce.is_none());
    }

    #[test]
    fn encrypt_optional_round_trips_non_empty_value() {
        let key = fixed_key();
        let (bytes, nonce) = encrypt_optional(&key, Some("secret note")).unwrap();
        let ct = bytes.expect("ciphertext present");
        let n = nonce.expect("nonce present");
        let recovered = crypto::decrypt(&key, &ct, &n).unwrap();
        assert_eq!(recovered, b"secret note");
    }
}
