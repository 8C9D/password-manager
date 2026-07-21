use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::crypto;
use crate::crypto::totp::{self, GeneratedTotp, TotpConfig};
use crate::db::{now_iso8601, now_unix};
use crate::error::AppError;
use crate::state::{with_authorized, with_unlocked, AppState};

/// How an entry write should treat the stored TOTP secret.
#[derive(Deserialize, Default)]
#[serde(tag = "action", rename_all = "lowercase")]
pub enum TotpUpdate {
    /// Leave any existing secret untouched (also the create-time "no TOTP" case).
    #[default]
    Keep,
    /// Remove any existing secret.
    Clear,
    /// Replace with a secret parsed from a base32 string or an otpauth:// URI.
    Set { value: String },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryInput {
    pub category_id: Option<i64>,
    pub title: String,
    pub username: String,
    pub url_or_app_name: String,
    pub password: String,
    pub notes: Option<String>,
    #[serde(default)]
    pub totp: TotpUpdate,
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
    pub has_totp: bool,
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

/// Encrypted bytes paired with their nonce for an optional field; both are
/// `None` when the field is absent or empty.
type OptionalCiphertext = (Option<Vec<u8>>, Option<Vec<u8>>);

fn encrypt_optional(
    key: &[u8; 32],
    plaintext: Option<&str>,
) -> Result<OptionalCiphertext, AppError> {
    match plaintext {
        Some(s) if !s.is_empty() => {
            let ct = crypto::encrypt(key, s.as_bytes())?;
            Ok((Some(ct.bytes), Some(ct.nonce.to_vec())))
        }
        _ => Ok((None, None)),
    }
}

/// Parse a TOTP secret/URI, serialize the canonical config to JSON, and encrypt
/// it with the vault key. The secret is only ever stored as ciphertext.
pub(crate) fn encrypt_totp(key: &[u8; 32], value: &str) -> Result<(Vec<u8>, Vec<u8>), AppError> {
    let config = totp::parse_totp_input(value)?;
    let json = serde_json::to_vec(&config)
        .map_err(|_| AppError::Internal("failed to serialize TOTP config".into()))?;
    let ct = crypto::encrypt(key, &json)?;
    Ok((ct.bytes, ct.nonce.to_vec()))
}

fn create_entry_impl(state: &AppState, input: EntryInput) -> Result<i64, AppError> {
    validate_input(&input)?;

    with_unlocked(state, |s, key| {
        let pw_ct = crypto::encrypt(key, input.password.as_bytes())?;
        let (notes_bytes, notes_nonce) = encrypt_optional(key, input.notes.as_deref())?;
        let (totp_bytes, totp_nonce) = match &input.totp {
            TotpUpdate::Set { value } => {
                let (b, n) = encrypt_totp(key, value)?;
                (Some(b), Some(n))
            }
            // On create, Keep and Clear both mean "no TOTP".
            _ => (None, None),
        };
        let now = now_iso8601();
        s.conn.execute(
            "INSERT INTO password_entries
                (category_id, title, username, url_or_app_name,
                 encrypted_password, password_nonce,
                 encrypted_notes, notes_nonce,
                 encrypted_totp, totp_nonce,
                 created_at, updated_at, last_used_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, NULL)",
            rusqlite::params![
                input.category_id,
                input.title.trim(),
                input.username,
                input.url_or_app_name,
                pw_ct.bytes,
                pw_ct.nonce.as_slice(),
                notes_bytes,
                notes_nonce,
                totp_bytes,
                totp_nonce,
                now,
            ],
        )?;
        Ok(s.conn.last_insert_rowid())
    })
}

fn list_entries_impl(state: &AppState) -> Result<Vec<EntrySummary>, AppError> {
    with_authorized(state, |s| {
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
pub fn create_entry(
    state: State<'_, AppState>,
    input: EntryInput,
) -> Result<i64, AppError> {
    create_entry_impl(&state, input)
}

#[tauri::command]
pub fn list_entries(state: State<'_, AppState>) -> Result<Vec<EntrySummary>, AppError> {
    list_entries_impl(&state)
}

struct EntryRow {
    id: i64,
    category_id: Option<i64>,
    title: String,
    username: String,
    url_or_app_name: String,
    encrypted_password: Vec<u8>,
    password_nonce: Vec<u8>,
    encrypted_notes: Option<Vec<u8>>,
    notes_nonce: Option<Vec<u8>>,
    created_at: String,
    updated_at: String,
    has_totp: bool,
}

fn get_entry_impl(state: &AppState, id: i64) -> Result<EntryFull, AppError> {
    with_unlocked(state, |s, key| {
        let row: Option<EntryRow> = s
            .conn
            .query_row(
                "SELECT id, category_id, title, username, url_or_app_name,
                        encrypted_password, password_nonce,
                        encrypted_notes, notes_nonce,
                        created_at, updated_at,
                        encrypted_totp IS NOT NULL
                 FROM password_entries WHERE id = ?1",
                [id],
                |r| {
                    Ok(EntryRow {
                        id: r.get(0)?,
                        category_id: r.get(1)?,
                        title: r.get(2)?,
                        username: r.get(3)?,
                        url_or_app_name: r.get(4)?,
                        encrypted_password: r.get(5)?,
                        password_nonce: r.get(6)?,
                        encrypted_notes: r.get(7)?,
                        notes_nonce: r.get(8)?,
                        created_at: r.get(9)?,
                        updated_at: r.get(10)?,
                        has_totp: r.get(11)?,
                    })
                },
            )
            .optional()?;

        let row = row.ok_or(AppError::EntryNotFound)?;

        let password_bytes = crypto::decrypt(key, &row.encrypted_password, &row.password_nonce)?;
        let password = String::from_utf8(password_bytes)
            .map_err(|_| AppError::Crypto("password is not valid utf-8"))?;

        let notes = match (row.encrypted_notes, row.notes_nonce) {
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
            rusqlite::params![now, row.id],
        )?;

        Ok(EntryFull {
            id: row.id,
            category_id: row.category_id,
            title: row.title,
            username: row.username,
            url_or_app_name: row.url_or_app_name,
            password,
            notes,
            created_at: row.created_at,
            updated_at: row.updated_at,
            last_used_at: Some(now),
            has_totp: row.has_totp,
        })
    })
}

fn update_entry_impl(state: &AppState, id: i64, input: EntryInput) -> Result<(), AppError> {
    validate_input(&input)?;

    with_unlocked(state, |s, key| {
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
        match &input.totp {
            TotpUpdate::Keep => {}
            TotpUpdate::Clear => {
                s.conn.execute(
                    "UPDATE password_entries SET encrypted_totp = NULL, totp_nonce = NULL
                     WHERE id = ?1",
                    rusqlite::params![id],
                )?;
            }
            TotpUpdate::Set { value } => {
                let (b, n) = encrypt_totp(key, value)?;
                s.conn.execute(
                    "UPDATE password_entries SET encrypted_totp = ?1, totp_nonce = ?2
                     WHERE id = ?3",
                    rusqlite::params![b, n, id],
                )?;
            }
        }
        Ok(())
    })
}

fn generate_totp_at(
    state: &AppState,
    id: i64,
    unix_seconds: u64,
) -> Result<GeneratedTotp, AppError> {
    with_unlocked(state, |s, key| {
        let row: Option<OptionalCiphertext> = s
            .conn
            .query_row(
                "SELECT encrypted_totp, totp_nonce FROM password_entries WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (enc, nonce) = match row {
            None => return Err(AppError::EntryNotFound),
            Some((Some(e), Some(n))) => (e, n),
            Some(_) => return Err(AppError::Validation("this entry has no TOTP configured")),
        };
        let json = crypto::decrypt(key, &enc, &nonce)?;
        let config: TotpConfig = serde_json::from_slice(&json)
            .map_err(|_| AppError::Crypto("stored TOTP config is invalid"))?;
        totp::generate(&config, unix_seconds)
    })
}

fn generate_totp_impl(state: &AppState, id: i64) -> Result<GeneratedTotp, AppError> {
    generate_totp_at(state, id, now_unix())
}

fn delete_entry_impl(state: &AppState, id: i64) -> Result<(), AppError> {
    with_unlocked(state, |s, _key| {
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

#[tauri::command]
pub fn get_entry(state: State<'_, AppState>, id: i64) -> Result<EntryFull, AppError> {
    get_entry_impl(&state, id)
}

#[tauri::command]
pub fn update_entry(
    state: State<'_, AppState>,
    id: i64,
    input: EntryInput,
) -> Result<(), AppError> {
    update_entry_impl(&state, id, input)
}

#[tauri::command]
pub fn delete_entry(state: State<'_, AppState>, id: i64) -> Result<(), AppError> {
    delete_entry_impl(&state, id)
}

#[tauri::command]
pub fn generate_totp(state: State<'_, AppState>, id: i64) -> Result<GeneratedTotp, AppError> {
    generate_totp_impl(&state, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use zeroize::Zeroizing;

    fn fixed_key() -> [u8; 32] {
        let mut k = [0u8; 32];
        for (i, b) in k.iter_mut().enumerate() {
            *b = i as u8;
        }
        k
    }

    fn locked_state() -> AppState {
        AppState::new(db::open_in_memory().unwrap())
    }

    fn unlocked_state() -> AppState {
        let state = locked_state();
        state.inner.lock().unwrap().key = Some(Zeroizing::new(fixed_key()));
        state
    }

    fn sample_input() -> EntryInput {
        EntryInput {
            category_id: None,
            title: "GitHub".into(),
            username: "alice".into(),
            url_or_app_name: "github.com".into(),
            password: "hunter2".into(),
            notes: Some("the cake is a lie".into()),
            totp: TotpUpdate::Keep,
        }
    }

    // RFC 6238 SHA1 seed, base32-encoded; code at t=59 is 287082 (6-digit).
    const RFC_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

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

    #[test]
    fn create_then_list_returns_summary() {
        let state = unlocked_state();
        let id = create_entry_impl(&state, sample_input()).unwrap();
        let list = list_entries_impl(&state).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, id);
        assert_eq!(list[0].title, "GitHub");
        assert_eq!(list[0].username, "alice");
        assert_eq!(list[0].url_or_app_name, "github.com");
        assert!(list[0].last_used_at.is_none());
    }

    #[test]
    fn list_sorts_case_insensitively() {
        let state = unlocked_state();
        for title in ["banana", "Apple", "cherry"] {
            create_entry_impl(
                &state,
                EntryInput {
                    title: title.into(),
                    ..sample_input()
                },
            )
            .unwrap();
        }
        let titles: Vec<_> = list_entries_impl(&state)
            .unwrap()
            .into_iter()
            .map(|e| e.title)
            .collect();
        assert_eq!(titles, vec!["Apple", "banana", "cherry"]);
    }

    #[test]
    fn get_round_trips_password_and_notes() {
        let state = unlocked_state();
        let id = create_entry_impl(&state, sample_input()).unwrap();
        let full = get_entry_impl(&state, id).unwrap();
        assert_eq!(full.id, id);
        assert_eq!(full.title, "GitHub");
        assert_eq!(full.password, "hunter2");
        assert_eq!(full.notes.as_deref(), Some("the cake is a lie"));
        assert!(full.last_used_at.is_some());
    }

    #[test]
    fn get_returns_none_notes_when_notes_missing() {
        let state = unlocked_state();
        let id = create_entry_impl(
            &state,
            EntryInput {
                notes: None,
                ..sample_input()
            },
        )
        .unwrap();
        let full = get_entry_impl(&state, id).unwrap();
        assert!(full.notes.is_none());
    }

    #[test]
    fn get_returns_none_notes_when_notes_empty_string() {
        let state = unlocked_state();
        let id = create_entry_impl(
            &state,
            EntryInput {
                notes: Some(String::new()),
                ..sample_input()
            },
        )
        .unwrap();
        let full = get_entry_impl(&state, id).unwrap();
        assert!(full.notes.is_none());
    }

    #[test]
    fn get_returns_entry_not_found_for_missing_id() {
        let state = unlocked_state();
        assert!(matches!(
            get_entry_impl(&state, 9999),
            Err(AppError::EntryNotFound)
        ));
    }

    #[test]
    fn update_changes_fields_and_round_trips_new_password() {
        let state = unlocked_state();
        let id = create_entry_impl(&state, sample_input()).unwrap();
        update_entry_impl(
            &state,
            id,
            EntryInput {
                title: "GitHub Pro".into(),
                username: "alice2".into(),
                url_or_app_name: "github.com".into(),
                password: "newpass".into(),
                notes: Some("rotated".into()),
                category_id: None,
                totp: TotpUpdate::Keep,
            },
        )
        .unwrap();
        let full = get_entry_impl(&state, id).unwrap();
        assert_eq!(full.title, "GitHub Pro");
        assert_eq!(full.username, "alice2");
        assert_eq!(full.password, "newpass");
        assert_eq!(full.notes.as_deref(), Some("rotated"));
    }

    #[test]
    fn update_clears_notes_when_set_to_none() {
        let state = unlocked_state();
        let id = create_entry_impl(&state, sample_input()).unwrap();
        update_entry_impl(
            &state,
            id,
            EntryInput {
                notes: None,
                ..sample_input()
            },
        )
        .unwrap();
        let full = get_entry_impl(&state, id).unwrap();
        assert!(full.notes.is_none());
    }

    #[test]
    fn update_returns_entry_not_found_for_missing_id() {
        let state = unlocked_state();
        let err = update_entry_impl(&state, 9999, sample_input()).unwrap_err();
        assert!(matches!(err, AppError::EntryNotFound));
    }

    #[test]
    fn delete_removes_existing_entry() {
        let state = unlocked_state();
        let id = create_entry_impl(&state, sample_input()).unwrap();
        delete_entry_impl(&state, id).unwrap();
        assert!(list_entries_impl(&state).unwrap().is_empty());
        assert!(matches!(
            get_entry_impl(&state, id),
            Err(AppError::EntryNotFound)
        ));
    }

    #[test]
    fn delete_returns_entry_not_found_for_missing_id() {
        let state = unlocked_state();
        let err = delete_entry_impl(&state, 9999).unwrap_err();
        assert!(matches!(err, AppError::EntryNotFound));
    }

    #[test]
    fn create_rejects_blank_title() {
        let state = unlocked_state();
        let err = create_entry_impl(
            &state,
            EntryInput {
                title: "   ".into(),
                ..sample_input()
            },
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn create_rejects_empty_password() {
        let state = unlocked_state();
        let err = create_entry_impl(
            &state,
            EntryInput {
                password: String::new(),
                ..sample_input()
            },
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn get_entry_rejects_when_locked() {
        let state = locked_state();
        assert!(matches!(
            get_entry_impl(&state, 1),
            Err(AppError::Locked)
        ));
    }

    #[test]
    fn get_entry_surfaces_db_error_rather_than_not_found() {
        let state = unlocked_state();
        let id = create_entry_impl(&state, sample_input()).unwrap();
        // Force a genuine DB-layer failure on the row read: with the table
        // gone, query_row errors instead of returning "no rows". The old
        // `.ok()` collapsed that into EntryNotFound, masking real corruption;
        // `.optional()?` must surface it as a Database error.
        state
            .inner
            .lock()
            .unwrap()
            .conn
            .execute("DROP TABLE password_entries", [])
            .unwrap();
        // Matched directly (not via unwrap_err) so we don't need Debug on
        // EntryFull, which carries the decrypted password.
        assert!(matches!(
            get_entry_impl(&state, id),
            Err(AppError::Database(_))
        ));
    }

    #[test]
    fn create_with_totp_sets_flag_and_generates_rfc_code() {
        let state = unlocked_state();
        let id = create_entry_impl(
            &state,
            EntryInput {
                totp: TotpUpdate::Set {
                    value: RFC_SECRET.into(),
                },
                ..sample_input()
            },
        )
        .unwrap();
        assert!(get_entry_impl(&state, id).unwrap().has_totp);
        // The stored secret must decrypt and reproduce the RFC 6238 vector.
        let code = generate_totp_at(&state, id, 59).unwrap();
        assert_eq!(code.code, "287082");
        assert_eq!(code.period, 30);
        // And a different time step yields the next RFC vector.
        assert_eq!(generate_totp_at(&state, id, 1111111109).unwrap().code, "081804");
    }

    #[test]
    fn entry_without_totp_reports_false_and_refuses_generation() {
        let state = unlocked_state();
        let id = create_entry_impl(&state, sample_input()).unwrap();
        assert!(!get_entry_impl(&state, id).unwrap().has_totp);
        assert!(matches!(
            generate_totp_at(&state, id, 59),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn generate_totp_reports_not_found_for_missing_entry() {
        let state = unlocked_state();
        assert!(matches!(
            generate_totp_at(&state, 9999, 59),
            Err(AppError::EntryNotFound)
        ));
    }

    #[test]
    fn update_can_add_keep_and_clear_totp() {
        let state = unlocked_state();
        let id = create_entry_impl(&state, sample_input()).unwrap();

        // Add a secret.
        update_entry_impl(
            &state,
            id,
            EntryInput {
                totp: TotpUpdate::Set {
                    value: RFC_SECRET.into(),
                },
                ..sample_input()
            },
        )
        .unwrap();
        assert!(get_entry_impl(&state, id).unwrap().has_totp);
        assert_eq!(generate_totp_at(&state, id, 59).unwrap().code, "287082");

        // Keep leaves it untouched (default action on an ordinary edit).
        update_entry_impl(
            &state,
            id,
            EntryInput {
                totp: TotpUpdate::Keep,
                ..sample_input()
            },
        )
        .unwrap();
        assert!(get_entry_impl(&state, id).unwrap().has_totp);

        // Clear removes it.
        update_entry_impl(
            &state,
            id,
            EntryInput {
                totp: TotpUpdate::Clear,
                ..sample_input()
            },
        )
        .unwrap();
        assert!(!get_entry_impl(&state, id).unwrap().has_totp);
        assert!(matches!(
            generate_totp_at(&state, id, 59),
            Err(AppError::Validation(_))
        ));
    }
}
