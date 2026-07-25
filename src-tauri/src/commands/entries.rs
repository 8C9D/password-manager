use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use tauri::State;
use zeroize::Zeroizing;

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

/// A user-defined extra field on an entry.
///
/// The value is encrypted with the vault key like a password; the label is
/// stored in the clear, the same choice already made for title, username, and
/// url_or_app_name. `secret` only controls whether the UI masks the value - it
/// is not a second encryption tier, and both kinds are encrypted at rest.
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomField {
    pub label: String,
    pub value: String,
    #[serde(default = "default_true")]
    pub secret: bool,
}

fn default_true() -> bool {
    true
}

/// Bounds on custom fields, enforced server-side like every other write path.
pub(crate) const MAX_FIELDS_PER_ENTRY: usize = 32;
pub(crate) const MAX_FIELD_LABEL_CHARS: usize = 64;
pub(crate) const MAX_FIELD_VALUE_CHARS: usize = 4096;

/// Drop blank rows, then hold what is left to the documented bounds.
///
/// A blank row is what an untouched "add field" line in the form looks like, so
/// discarding it is normal input handling rather than an error.
pub(crate) fn normalize_fields(fields: &[CustomField]) -> Result<Vec<CustomField>, AppError> {
    let kept: Vec<CustomField> = fields
        .iter()
        .filter(|f| !(f.label.trim().is_empty() && f.value.is_empty()))
        .cloned()
        .collect();
    if kept.len() > MAX_FIELDS_PER_ENTRY {
        return Err(AppError::Validation("too many custom fields on one entry"));
    }
    for f in &kept {
        if f.label.trim().is_empty() {
            return Err(AppError::Validation("a custom field needs a label"));
        }
        if f.label.chars().count() > MAX_FIELD_LABEL_CHARS {
            return Err(AppError::Validation(
                "a custom field label must be 64 characters or fewer",
            ));
        }
        if f.value.chars().count() > MAX_FIELD_VALUE_CHARS {
            return Err(AppError::Validation("a custom field value is too long"));
        }
    }
    Ok(kept
        .into_iter()
        .map(|f| CustomField {
            label: f.label.trim().to_string(),
            ..f
        })
        .collect())
}

/// Replace an entry's custom fields wholesale, inside the caller's transaction.
///
/// Rewriting rather than diffing keeps the stored order equal to the order the
/// form submitted, and means a removed field leaves no row behind.
pub(crate) fn write_fields(
    conn: &rusqlite::Connection,
    key: &[u8; 32],
    entry_id: i64,
    fields: &[CustomField],
) -> Result<(), AppError> {
    conn.execute("DELETE FROM entry_fields WHERE entry_id = ?1", [entry_id])?;
    for (position, field) in fields.iter().enumerate() {
        let ct = crypto::encrypt(key, field.value.as_bytes())?;
        conn.execute(
            "INSERT INTO entry_fields
                (entry_id, label, encrypted_value, value_nonce, is_secret, position)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                entry_id,
                field.label,
                ct.bytes,
                ct.nonce.as_slice(),
                field.secret,
                position as i64,
            ],
        )?;
    }
    Ok(())
}

/// Read and decrypt an entry's custom fields, in their stored order.
pub(crate) fn read_fields(
    conn: &rusqlite::Connection,
    key: &[u8; 32],
    entry_id: i64,
) -> Result<Vec<CustomField>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT label, encrypted_value, value_nonce, is_secret
         FROM entry_fields WHERE entry_id = ?1 ORDER BY position, id",
    )?;
    type Row = (String, Vec<u8>, Vec<u8>, bool);
    let rows: Vec<Row> = stmt
        .query_map([entry_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    let mut out = Vec::with_capacity(rows.len());
    for (label, ciphertext, nonce, secret) in rows {
        let plain = crypto::decrypt(key, &ciphertext, &nonce)?;
        out.push(CustomField {
            label,
            value: String::from_utf8(plain)
                .map_err(|_| AppError::Crypto("custom field value is not valid utf-8"))?,
            secret,
        });
    }
    Ok(out)
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
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Days after a password change before this entry is due for rotation.
    /// `None` (and 0) mean no reminder.
    #[serde(default)]
    pub password_expiry_days: Option<u32>,
    /// User-defined extra fields, replacing whatever the entry had.
    #[serde(default)]
    pub fields: Vec<CustomField>,
}

/// Normalize tags: trim, drop blanks, and de-duplicate while preserving order.
pub(crate) fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    tags.iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .filter(|t| seen.insert(t.to_lowercase()))
        .map(|t| t.to_string())
        .collect()
}

pub(crate) fn tags_to_json(tags: &[String]) -> String {
    serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string())
}

pub(crate) fn tags_from_json(raw: &str) -> Vec<String> {
    serde_json::from_str(raw).unwrap_or_default()
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
    pub favorite: bool,
    pub tags: Vec<String>,
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
    pub favorite: bool,
    pub tags: Vec<String>,
    pub password_expiry_days: Option<u32>,
    /// When this password is next due for rotation, or `None` with no reminder.
    pub password_due_at: Option<String>,
    pub fields: Vec<CustomField>,
}

fn validate_input(input: &EntryInput) -> Result<(), AppError> {
    if input.title.trim().is_empty() {
        return Err(AppError::Validation("title is required"));
    }
    if input.password.is_empty() {
        return Err(AppError::Validation("password is required"));
    }
    if let Some(days) = input.password_expiry_days {
        if days > MAX_EXPIRY_DAYS {
            return Err(AppError::Validation(
                "rotation reminder must be 3650 days or fewer",
            ));
        }
    }
    normalize_fields(&input.fields)?;
    Ok(())
}

/// Ten years: past this a reminder is indistinguishable from none, and the
/// bound keeps the value in a range the date arithmetic can't overflow.
pub(crate) const MAX_EXPIRY_DAYS: u32 = 3650;

/// Normalize the wire value: 0 and `None` both mean "no reminder", stored NULL.
fn expiry_to_column(days: Option<u32>) -> Option<u32> {
    days.filter(|d| *d > 0)
}

/// When the password is next due for rotation: the last change plus the
/// entry's reminder interval.
///
/// `None` when there is no reminder, or when the stored timestamp cannot be
/// parsed - an unreadable date must not manufacture a due date out of nothing.
///
/// The addition is checked rather than `+`: `validate_input` caps the interval
/// at `MAX_EXPIRY_DAYS`, but the column can also come from an import file or a
/// hand-edited database, and `DateTime + TimeDelta` *panics* on overflow. A
/// panic here happens while the state mutex is held, which poisons it and takes
/// every later command down with it, so an absurd interval reads as no
/// reminder instead.
pub(crate) fn password_due_at(
    password_changed_at: Option<&str>,
    expiry_days: Option<u32>,
) -> Option<String> {
    let days = expiry_to_column(expiry_days)?;
    let changed = chrono::DateTime::parse_from_rfc3339(password_changed_at?).ok()?;
    let due = changed
        .with_timezone(&chrono::Utc)
        .checked_add_signed(chrono::TimeDelta::try_days(i64::from(days))?)?;
    Some(due.to_rfc3339())
}

/// Whether a rotation reminder has come due as of `now`.
pub(crate) fn password_is_due(
    password_changed_at: Option<&str>,
    expiry_days: Option<u32>,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    match password_due_at(password_changed_at, expiry_days) {
        Some(due) => chrono::DateTime::parse_from_rfc3339(&due)
            .map(|d| now >= d.with_timezone(&chrono::Utc))
            .unwrap_or(false),
        None => false,
    }
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
        let tags = tags_to_json(&normalize_tags(&input.tags));
        let fields = normalize_fields(&input.fields)?;
        let now = now_iso8601();
        // One transaction with the field rows: a half-created entry that lost
        // its custom fields would be silent data loss at create time.
        let tx = s.conn.transaction()?;
        tx.execute(
            "INSERT INTO password_entries
                (category_id, title, username, url_or_app_name,
                 encrypted_password, password_nonce,
                 encrypted_notes, notes_nonce,
                 encrypted_totp, totp_nonce,
                 is_favorite, tags, password_expiry_days,
                 created_at, updated_at, last_used_at, password_changed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                     ?14, ?14, NULL, ?14)",
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
                input.favorite,
                tags,
                expiry_to_column(input.password_expiry_days),
                now,
            ],
        )?;
        let id = tx.last_insert_rowid();
        write_fields(&tx, key, id, &fields)?;
        tx.commit()?;
        Ok(id)
    })
}

fn list_entries_impl(state: &AppState) -> Result<Vec<EntrySummary>, AppError> {
    with_authorized(state, |s| {
        let mut stmt = s.conn.prepare(
            "SELECT id, category_id, title, username, url_or_app_name,
                    created_at, updated_at, last_used_at, is_favorite, tags
             FROM password_entries
             WHERE deleted_at IS NULL
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
                    favorite: r.get(8)?,
                    tags: tags_from_json(&r.get::<_, String>(9)?),
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
    favorite: bool,
    tags: Vec<String>,
    password_expiry_days: Option<u32>,
    password_changed_at: Option<String>,
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
                        encrypted_totp IS NOT NULL,
                        is_favorite, tags, password_expiry_days,
                        COALESCE(password_changed_at, updated_at)
                 FROM password_entries WHERE id = ?1 AND deleted_at IS NULL",
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
                        favorite: r.get(12)?,
                        tags: tags_from_json(&r.get::<_, String>(13)?),
                        password_expiry_days: r.get(14)?,
                        password_changed_at: r.get(15)?,
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

        let fields = read_fields(&s.conn, key, row.id)?;

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
            favorite: row.favorite,
            tags: row.tags,
            password_expiry_days: row.password_expiry_days,
            password_due_at: password_due_at(
                row.password_changed_at.as_deref(),
                row.password_expiry_days,
            ),
            fields,
        })
    })
}

fn update_entry_impl(state: &AppState, id: i64, input: EntryInput) -> Result<(), AppError> {
    validate_input(&input)?;

    with_unlocked(state, |s, key| {
        let pw_ct = crypto::encrypt(key, input.password.as_bytes())?;
        let (notes_bytes, notes_nonce) = encrypt_optional(key, input.notes.as_deref())?;
        let tags = tags_to_json(&normalize_tags(&input.tags));
        let fields = normalize_fields(&input.fields)?;
        let now = now_iso8601();

        // The field write and the TOTP write share one transaction so a failure
        // in either (e.g. an unparseable TOTP secret) rolls back the whole edit
        // instead of leaving the entry half-updated.
        let tx = s.conn.transaction()?;
        // password_changed_at must only move when the password VALUE changes;
        // the row is re-encrypted on every edit, so the ciphertext can't tell
        // a rotation from a title rename - compare plaintexts instead.
        let existing: Option<(Vec<u8>, Vec<u8>)> = tx
            .query_row(
                "SELECT encrypted_password, password_nonce FROM password_entries
                 WHERE id = ?1 AND deleted_at IS NULL",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (old_enc, old_nonce) = existing.ok_or(AppError::EntryNotFound)?;
        let old_pw = Zeroizing::new(crypto::decrypt(key, &old_enc, &old_nonce)?);
        let password_changed = old_pw.as_slice() != input.password.as_bytes();
        let n = tx.execute(
            "UPDATE password_entries SET
                category_id = ?1,
                title = ?2,
                username = ?3,
                url_or_app_name = ?4,
                encrypted_password = ?5,
                password_nonce = ?6,
                encrypted_notes = ?7,
                notes_nonce = ?8,
                is_favorite = ?9,
                tags = ?10,
                password_expiry_days = ?11,
                updated_at = ?12,
                password_changed_at = CASE WHEN ?13 THEN ?12 ELSE password_changed_at END
             WHERE id = ?14",
            rusqlite::params![
                input.category_id,
                input.title.trim(),
                input.username,
                input.url_or_app_name,
                pw_ct.bytes,
                pw_ct.nonce.as_slice(),
                notes_bytes,
                notes_nonce,
                input.favorite,
                tags,
                expiry_to_column(input.password_expiry_days),
                now,
                password_changed,
                id,
            ],
        )?;
        if n == 0 {
            return Err(AppError::EntryNotFound);
        }
        // Retain the password we just replaced, inside the same transaction, so
        // a failure here rolls the rotation back rather than losing the old
        // password with no record of it.
        if password_changed {
            super::history::record_password_change(&tx, key, id, old_pw.as_slice(), &now)?;
        }
        match &input.totp {
            TotpUpdate::Keep => {}
            TotpUpdate::Clear => {
                tx.execute(
                    "UPDATE password_entries SET encrypted_totp = NULL, totp_nonce = NULL
                     WHERE id = ?1",
                    rusqlite::params![id],
                )?;
            }
            TotpUpdate::Set { value } => {
                let (totp_bytes, totp_nonce) = encrypt_totp(key, value)?;
                tx.execute(
                    "UPDATE password_entries SET encrypted_totp = ?1, totp_nonce = ?2
                     WHERE id = ?3",
                    rusqlite::params![totp_bytes, totp_nonce, id],
                )?;
            }
        }
        write_fields(&tx, key, id, &fields)?;
        tx.commit()?;
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
                "SELECT encrypted_totp, totp_nonce FROM password_entries
                 WHERE id = ?1 AND deleted_at IS NULL",
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

/// Move an entry to the trash.
///
/// Deleting a password is unrecoverable in a way almost nothing else in this
/// app is, and a misclick in a list is easy. The row keeps its ciphertext and
/// its history and only stops appearing in the vault; `purge_entry` is what
/// actually destroys it.
fn delete_entry_impl(state: &AppState, id: i64) -> Result<(), AppError> {
    with_authorized(state, |s| {
        let n = s.conn.execute(
            "UPDATE password_entries SET deleted_at = ?1
             WHERE id = ?2 AND deleted_at IS NULL",
            rusqlite::params![now_iso8601(), id],
        )?;
        if n == 0 {
            return Err(AppError::EntryNotFound);
        }
        Ok(())
    })
}

/// Put a trashed entry back into the vault. Idempotent only in the sense that
/// restoring something that is not in the trash reports it as not found.
fn restore_entry_impl(state: &AppState, id: i64) -> Result<(), AppError> {
    with_authorized(state, |s| {
        let n = s.conn.execute(
            "UPDATE password_entries SET deleted_at = NULL
             WHERE id = ?1 AND deleted_at IS NOT NULL",
            rusqlite::params![id],
        )?;
        if n == 0 {
            return Err(AppError::EntryNotFound);
        }
        Ok(())
    })
}

/// Permanently destroy one trashed entry, and the retained previous passwords
/// that cascade from it. Refuses to touch a live entry, so the only route to
/// permanent deletion runs through the trash.
fn purge_entry_impl(state: &AppState, id: i64) -> Result<(), AppError> {
    with_authorized(state, |s| {
        let n = s.conn.execute(
            "DELETE FROM password_entries WHERE id = ?1 AND deleted_at IS NOT NULL",
            rusqlite::params![id],
        )?;
        if n == 0 {
            return Err(AppError::EntryNotFound);
        }
        Ok(())
    })
}

/// Empty the trash, returning how many entries were destroyed.
fn purge_all_entries_impl(state: &AppState) -> Result<usize, AppError> {
    with_authorized(state, |s| {
        let n = s
            .conn
            .execute("DELETE FROM password_entries WHERE deleted_at IS NOT NULL", [])?;
        Ok(n)
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedEntry {
    pub id: i64,
    pub title: String,
    pub username: String,
    pub url_or_app_name: String,
    pub deleted_at: String,
}

fn list_deleted_entries_impl(state: &AppState) -> Result<Vec<DeletedEntry>, AppError> {
    with_authorized(state, |s| {
        let mut stmt = s.conn.prepare(
            "SELECT id, title, username, url_or_app_name, deleted_at
             FROM password_entries
             WHERE deleted_at IS NOT NULL
             ORDER BY deleted_at DESC, title COLLATE NOCASE ASC",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(DeletedEntry {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    username: r.get(2)?,
                    url_or_app_name: r.get(3)?,
                    deleted_at: r.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

#[tauri::command]
pub fn restore_entry(state: State<'_, AppState>, id: i64) -> Result<(), AppError> {
    restore_entry_impl(&state, id)
}

#[tauri::command]
pub fn purge_entry(state: State<'_, AppState>, id: i64) -> Result<(), AppError> {
    purge_entry_impl(&state, id)
}

#[tauri::command]
pub fn purge_all_entries(state: State<'_, AppState>) -> Result<usize, AppError> {
    purge_all_entries_impl(&state)
}

#[tauri::command]
pub fn list_deleted_entries(
    state: State<'_, AppState>,
) -> Result<Vec<DeletedEntry>, AppError> {
    list_deleted_entries_impl(&state)
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

fn set_favorite_impl(state: &AppState, id: i64, favorite: bool) -> Result<(), AppError> {
    with_authorized(state, |s| {
        let n = s.conn.execute(
            "UPDATE password_entries SET is_favorite = ?1
             WHERE id = ?2 AND deleted_at IS NULL",
            rusqlite::params![favorite, id],
        )?;
        if n == 0 {
            return Err(AppError::EntryNotFound);
        }
        Ok(())
    })
}

#[tauri::command]
pub fn set_favorite(
    state: State<'_, AppState>,
    id: i64,
    favorite: bool,
) -> Result<(), AppError> {
    set_favorite_impl(&state, id, favorite)
}

// --- Bulk actions ---

/// Upper bound on one bulk request. Well above any realistic selection, and it
/// keeps a hand-crafted call from building an enormous statement.
pub(crate) const MAX_BULK_IDS: usize = 1000;

/// Validate a bulk id list: non-empty, within the cap, and de-duplicated so a
/// repeated id cannot inflate the reported count.
pub(crate) fn normalize_bulk_ids(ids: &[i64]) -> Result<Vec<i64>, AppError> {
    if ids.is_empty() {
        return Err(AppError::Validation("no entries selected"));
    }
    if ids.len() > MAX_BULK_IDS {
        return Err(AppError::Validation("too many entries in one request"));
    }
    let mut seen = std::collections::HashSet::new();
    Ok(ids.iter().copied().filter(|id| seen.insert(*id)).collect())
}

/// Apply one metadata UPDATE to every live entry in `ids`, in a single
/// transaction, and report how many rows it actually changed.
///
/// The count is what the caller reports to the user, so it comes from the
/// database rather than from the length of the request: ids that name a
/// trashed or already-deleted entry match nothing and must not be counted.
fn bulk_update(
    state: &AppState,
    ids: &[i64],
    sql: &str,
    bind: impl Fn(&mut rusqlite::Statement<'_>, i64) -> rusqlite::Result<usize>,
) -> Result<usize, AppError> {
    let ids = normalize_bulk_ids(ids)?;
    with_authorized(state, |s| {
        let tx = s.conn.transaction()?;
        let mut changed = 0usize;
        {
            let mut stmt = tx.prepare(sql)?;
            for id in ids {
                changed += bind(&mut stmt, id)?;
            }
        }
        tx.commit()?;
        Ok(changed)
    })
}

/// Move several entries into a category at once (or out of one, with `None`).
///
/// This writes only `category_id`, so unlike routing the change through
/// `update_entry` it never decrypts or re-encrypts a password - and therefore
/// cannot disturb `password_changed_at` or record spurious password history.
fn set_entries_category_impl(
    state: &AppState,
    ids: &[i64],
    category_id: Option<i64>,
) -> Result<usize, AppError> {
    let now = now_iso8601();
    bulk_update(
        state,
        ids,
        "UPDATE password_entries SET category_id = ?1, updated_at = ?2
         WHERE id = ?3 AND deleted_at IS NULL",
        move |stmt, id| stmt.execute(rusqlite::params![category_id, now, id]),
    )
}

fn set_entries_favorite_impl(
    state: &AppState,
    ids: &[i64],
    favorite: bool,
) -> Result<usize, AppError> {
    bulk_update(
        state,
        ids,
        "UPDATE password_entries SET is_favorite = ?1
         WHERE id = ?2 AND deleted_at IS NULL",
        move |stmt, id| stmt.execute(rusqlite::params![favorite, id]),
    )
}

/// Move several entries to the trash at once. Like `delete_entry` this is a
/// soft delete; nothing here destroys data.
fn delete_entries_impl(state: &AppState, ids: &[i64]) -> Result<usize, AppError> {
    let now = now_iso8601();
    bulk_update(
        state,
        ids,
        "UPDATE password_entries SET deleted_at = ?1
         WHERE id = ?2 AND deleted_at IS NULL",
        move |stmt, id| stmt.execute(rusqlite::params![now, id]),
    )
}

#[tauri::command]
pub fn set_entries_category(
    state: State<'_, AppState>,
    ids: Vec<i64>,
    category_id: Option<i64>,
) -> Result<usize, AppError> {
    set_entries_category_impl(&state, &ids, category_id)
}

#[tauri::command]
pub fn set_entries_favorite(
    state: State<'_, AppState>,
    ids: Vec<i64>,
    favorite: bool,
) -> Result<usize, AppError> {
    set_entries_favorite_impl(&state, &ids, favorite)
}

#[tauri::command]
pub fn delete_entries(state: State<'_, AppState>, ids: Vec<i64>) -> Result<usize, AppError> {
    delete_entries_impl(&state, &ids)
}

/// Create an entry from another module's tests. The `_impl` functions are
/// private to this module, but the history and transfer tests need to drive
/// real entry writes rather than hand-rolled INSERTs that skip this path.
#[cfg(test)]
pub(crate) fn create_entry_for_test(
    state: &AppState,
    input: EntryInput,
) -> Result<i64, AppError> {
    create_entry_impl(state, input)
}

#[cfg(test)]
pub(crate) fn update_entry_for_test(
    state: &AppState,
    id: i64,
    input: EntryInput,
) -> Result<(), AppError> {
    update_entry_impl(state, id, input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

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
            favorite: false,
            tags: vec![],
            password_expiry_days: None,
            fields: vec![],
        }
    }

    // RFC 6238 SHA1 seed, base32-encoded; code at t=59 is 287082 (6-digit).
    const RFC_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn due_date_is_the_last_password_change_plus_the_interval() {
        let due = password_due_at(Some("2026-01-01T00:00:00Z"), Some(90)).unwrap();
        assert!(due.starts_with("2026-04-01T00:00:00"), "got {due}");
    }

    #[test]
    fn no_reminder_means_no_due_date() {
        assert!(password_due_at(Some("2026-01-01T00:00:00Z"), None).is_none());
        // Zero is the "off" value the UI sends when the field is cleared.
        assert!(password_due_at(Some("2026-01-01T00:00:00Z"), Some(0)).is_none());
    }

    #[test]
    fn an_unparseable_change_time_yields_no_due_date_rather_than_a_wrong_one() {
        assert!(password_due_at(Some("not a date"), Some(30)).is_none());
        assert!(password_due_at(None, Some(30)).is_none());
        let now = chrono::Utc::now();
        assert!(!password_is_due(Some("not a date"), Some(30), now));
    }

    #[test]
    fn an_absurd_interval_yields_no_due_date_instead_of_panicking() {
        // `validate_input` caps the interval, but the column can also arrive
        // from a hand-edited database, and `DateTime + TimeDelta` panics on
        // overflow - under the state mutex, which poisons it and breaks every
        // later command until the app restarts.
        assert!(password_due_at(Some("2026-01-01T00:00:00Z"), Some(u32::MAX)).is_none());
        assert!(password_due_at(Some("2026-01-01T00:00:00Z"), Some(4_000_000_000)).is_none());
        assert!(!password_is_due(
            Some("2026-01-01T00:00:00Z"),
            Some(u32::MAX),
            chrono::Utc::now()
        ));
    }

    #[test]
    fn a_reminder_comes_due_on_its_date_and_not_before() {
        let changed = "2026-01-01T00:00:00Z";
        let at = |s: &str| {
            chrono::DateTime::parse_from_rfc3339(s)
                .unwrap()
                .with_timezone(&chrono::Utc)
        };
        assert!(!password_is_due(Some(changed), Some(30), at("2026-01-30T23:59:59Z")));
        assert!(password_is_due(Some(changed), Some(30), at("2026-01-31T00:00:00Z")));
        assert!(password_is_due(Some(changed), Some(30), at("2026-06-01T00:00:00Z")));
    }

    #[test]
    fn the_reminder_round_trips_through_create_and_update() {
        let state = unlocked_state();
        let mut input = sample_input();
        input.password_expiry_days = Some(90);
        let id = create_entry_impl(&state, input).unwrap();

        let full = get_entry_impl(&state, id).unwrap();
        assert_eq!(full.password_expiry_days, Some(90));
        assert!(full.password_due_at.is_some());

        // Clearing it with 0 stores NULL, not 0.
        let mut cleared = sample_input();
        cleared.password_expiry_days = Some(0);
        update_entry_impl(&state, id, cleared).unwrap();
        let full = get_entry_impl(&state, id).unwrap();
        assert_eq!(full.password_expiry_days, None);
        assert_eq!(full.password_due_at, None);
    }

    #[test]
    fn rotating_the_password_pushes_the_due_date_out() {
        let state = unlocked_state();
        let mut input = sample_input();
        input.password_expiry_days = Some(1);
        let id = create_entry_impl(&state, input).unwrap();
        let first_due = get_entry_impl(&state, id).unwrap().password_due_at.unwrap();

        // Backdate the change so the reminder is already overdue.
        state
            .inner
            .lock()
            .unwrap()
            .conn
            .execute(
                "UPDATE password_entries SET password_changed_at = '2020-01-01T00:00:00Z'
                 WHERE id = ?1",
                [id],
            )
            .unwrap();
        assert!(get_entry_impl(&state, id)
            .unwrap()
            .password_due_at
            .unwrap()
            .starts_with("2020-01-02"));

        // Changing the password resets the clock; a metadata-only edit must not.
        let mut renamed = sample_input();
        renamed.password_expiry_days = Some(1);
        renamed.title = "Renamed".into();
        update_entry_impl(&state, id, renamed).unwrap();
        assert!(get_entry_impl(&state, id)
            .unwrap()
            .password_due_at
            .unwrap()
            .starts_with("2020-01-02"));

        let mut rotated = sample_input();
        rotated.password_expiry_days = Some(1);
        rotated.password = "a-brand-new-password".into();
        update_entry_impl(&state, id, rotated).unwrap();
        let after = get_entry_impl(&state, id).unwrap().password_due_at.unwrap();
        assert!(after > first_due, "{after} should be later than {first_due}");
    }

    #[test]
    fn an_absurd_reminder_interval_is_rejected() {
        let state = unlocked_state();
        let mut input = sample_input();
        input.password_expiry_days = Some(MAX_EXPIRY_DAYS + 1);
        assert!(matches!(
            create_entry_impl(&state, input),
            Err(AppError::Validation(_))
        ));

        let mut ok = sample_input();
        ok.password_expiry_days = Some(MAX_EXPIRY_DAYS);
        assert!(create_entry_impl(&state, ok).is_ok());
    }

    #[test]
    fn delete_moves_an_entry_to_the_trash_instead_of_destroying_it() {
        let state = unlocked_state();
        let id = create_entry_impl(&state, sample_input()).unwrap();

        delete_entry_impl(&state, id).unwrap();

        // Gone from the vault...
        assert!(list_entries_impl(&state).unwrap().is_empty());
        assert!(matches!(
            get_entry_impl(&state, id),
            Err(AppError::EntryNotFound)
        ));
        // ...but still there, and still restorable.
        let trashed = list_deleted_entries_impl(&state).unwrap();
        assert_eq!(trashed.len(), 1);
        assert_eq!(trashed[0].id, id);
        assert_eq!(trashed[0].title, "GitHub");
        assert!(!trashed[0].deleted_at.is_empty());
    }

    #[test]
    fn restore_brings_an_entry_back_with_its_secrets_intact() {
        let state = unlocked_state();
        let id = create_entry_impl(&state, sample_input()).unwrap();
        delete_entry_impl(&state, id).unwrap();

        restore_entry_impl(&state, id).unwrap();

        assert_eq!(list_entries_impl(&state).unwrap().len(), 1);
        assert!(list_deleted_entries_impl(&state).unwrap().is_empty());
        let full = get_entry_impl(&state, id).unwrap();
        assert_eq!(full.password, "hunter2");
        assert_eq!(full.notes.as_deref(), Some("the cake is a lie"));
    }

    #[test]
    fn purge_destroys_a_trashed_entry_but_refuses_a_live_one() {
        let state = unlocked_state();
        let id = create_entry_impl(&state, sample_input()).unwrap();

        // Permanent deletion must only be reachable through the trash.
        assert!(matches!(
            purge_entry_impl(&state, id),
            Err(AppError::EntryNotFound)
        ));
        assert_eq!(list_entries_impl(&state).unwrap().len(), 1);

        delete_entry_impl(&state, id).unwrap();
        purge_entry_impl(&state, id).unwrap();
        assert!(list_deleted_entries_impl(&state).unwrap().is_empty());
        let count: i64 = state
            .inner
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT COUNT(*) FROM password_entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn purging_an_entry_takes_its_password_history_with_it() {
        let state = unlocked_state();
        let id = create_entry_impl(&state, sample_input()).unwrap();
        let mut rotated = sample_input();
        rotated.password = "brand-new-password".into();
        update_entry_impl(&state, id, rotated).unwrap();

        let history_count = |state: &AppState| -> i64 {
            state
                .inner
                .lock()
                .unwrap()
                .conn
                .query_row("SELECT COUNT(*) FROM password_history", [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(history_count(&state), 1);

        // Trashing keeps the retired secrets (the entry may come back)...
        delete_entry_impl(&state, id).unwrap();
        assert_eq!(history_count(&state), 1);
        // ...purging must not leave them behind.
        purge_entry_impl(&state, id).unwrap();
        assert_eq!(history_count(&state), 0);
    }

    #[test]
    fn empty_trash_purges_only_trashed_entries() {
        let state = unlocked_state();
        let keep = create_entry_impl(&state, sample_input()).unwrap();
        let mut second = sample_input();
        second.title = "Bank".into();
        let toss = create_entry_impl(&state, second).unwrap();
        delete_entry_impl(&state, toss).unwrap();

        assert_eq!(purge_all_entries_impl(&state).unwrap(), 1);
        assert!(list_deleted_entries_impl(&state).unwrap().is_empty());
        let remaining = list_entries_impl(&state).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, keep);
    }

    #[test]
    fn a_trashed_entry_cannot_be_edited_favorited_or_deleted_again() {
        let state = unlocked_state();
        let id = create_entry_impl(&state, sample_input()).unwrap();
        delete_entry_impl(&state, id).unwrap();

        assert!(matches!(
            update_entry_impl(&state, id, sample_input()),
            Err(AppError::EntryNotFound)
        ));
        assert!(matches!(
            set_favorite_impl(&state, id, true),
            Err(AppError::EntryNotFound)
        ));
        // Deleting twice must not silently re-stamp the deletion time.
        assert!(matches!(
            delete_entry_impl(&state, id),
            Err(AppError::EntryNotFound)
        ));
    }

    #[test]
    fn trash_operations_require_an_unlocked_vault() {
        let state = locked_state();
        assert!(matches!(restore_entry_impl(&state, 1), Err(AppError::Locked)));
        assert!(matches!(purge_entry_impl(&state, 1), Err(AppError::Locked)));
        assert!(matches!(purge_all_entries_impl(&state), Err(AppError::Locked)));
        assert!(matches!(
            list_deleted_entries_impl(&state),
            Err(AppError::Locked)
        ));
    }

    #[test]
    fn restoring_something_that_is_not_trashed_reports_not_found() {
        let state = unlocked_state();
        let id = create_entry_impl(&state, sample_input()).unwrap();
        assert!(matches!(
            restore_entry_impl(&state, id),
            Err(AppError::EntryNotFound)
        ));
        assert!(matches!(
            restore_entry_impl(&state, 9999),
            Err(AppError::EntryNotFound)
        ));
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

    #[test]
    fn update_bumps_password_changed_at_only_for_a_new_password() {
        let state = unlocked_state();
        let id = create_entry_impl(&state, sample_input()).unwrap();
        let read = |state: &AppState| -> String {
            state
                .inner
                .lock()
                .unwrap()
                .conn
                .query_row(
                    "SELECT password_changed_at FROM password_entries WHERE id = ?1",
                    [id],
                    |r| r.get(0),
                )
                .unwrap()
        };
        let original = read(&state);

        // Metadata-only edit (same password, new title): the password's own
        // timestamp must not move, or the stale audit resets on every rename.
        let mut input = sample_input();
        input.title = "GitHub (work)".into();
        update_entry_impl(&state, id, input).unwrap();
        assert_eq!(read(&state), original);

        // An actual rotation moves it.
        let mut input = sample_input();
        input.password = "brand-new-password-9".into();
        update_entry_impl(&state, id, input).unwrap();
        assert_ne!(read(&state), original);
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
                favorite: false,
                tags: vec![],
                password_expiry_days: None,
                fields: vec![],
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
    fn update_with_unparseable_totp_rolls_back_the_whole_edit() {
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

        // One edit that both rotates the password and sets a malformed TOTP
        // secret. The bad secret must abort the whole update, not leave the new
        // password committed while the TOTP write fails.
        let result = update_entry_impl(
            &state,
            id,
            EntryInput {
                password: "rotated-password".into(),
                totp: TotpUpdate::Set {
                    value: "!!! not base32 !!!".into(),
                },
                ..sample_input()
            },
        );
        assert!(matches!(result, Err(AppError::Validation(_))));

        // The entry is unchanged: original password and original working TOTP.
        let full = get_entry_impl(&state, id).unwrap();
        assert_eq!(full.password, "hunter2");
        assert!(full.has_totp);
        assert_eq!(generate_totp_at(&state, id, 59).unwrap().code, "287082");
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

    #[test]
    fn normalize_tags_trims_dedups_and_drops_blanks() {
        let out = normalize_tags(&[
            "  work ".into(),
            "Work".into(), // case-insensitive duplicate
            "".into(),
            "   ".into(),
            "personal".into(),
        ]);
        assert_eq!(out, vec!["work".to_string(), "personal".to_string()]);
    }

    #[test]
    fn create_stores_favorite_and_normalized_tags() {
        let state = unlocked_state();
        let id = create_entry_impl(
            &state,
            EntryInput {
                favorite: true,
                tags: vec!["Work".into(), "  ".into(), "work".into(), "email".into()],
                ..sample_input()
            },
        )
        .unwrap();
        let full = get_entry_impl(&state, id).unwrap();
        assert!(full.favorite);
        assert_eq!(full.tags, vec!["Work".to_string(), "email".to_string()]);

        let summary = list_entries_impl(&state).unwrap();
        assert!(summary[0].favorite);
        assert_eq!(summary[0].tags, vec!["Work".to_string(), "email".to_string()]);
    }

    #[test]
    fn set_favorite_toggles_the_flag() {
        let state = unlocked_state();
        let id = create_entry_impl(&state, sample_input()).unwrap();
        assert!(!get_entry_impl(&state, id).unwrap().favorite);
        set_favorite_impl(&state, id, true).unwrap();
        assert!(get_entry_impl(&state, id).unwrap().favorite);
        set_favorite_impl(&state, id, false).unwrap();
        assert!(!get_entry_impl(&state, id).unwrap().favorite);
    }

    #[test]
    fn set_favorite_reports_not_found_for_missing_entry() {
        let state = unlocked_state();
        assert!(matches!(
            set_favorite_impl(&state, 9999, true),
            Err(AppError::EntryNotFound)
        ));
    }

    #[test]
    fn update_changes_favorite_and_tags() {
        let state = unlocked_state();
        let id = create_entry_impl(&state, sample_input()).unwrap();
        update_entry_impl(
            &state,
            id,
            EntryInput {
                favorite: true,
                tags: vec!["banking".into()],
                ..sample_input()
            },
        )
        .unwrap();
        let full = get_entry_impl(&state, id).unwrap();
        assert!(full.favorite);
        assert_eq!(full.tags, vec!["banking".to_string()]);
    }

    // --- Custom fields ---

    fn field(label: &str, value: &str) -> CustomField {
        CustomField {
            label: label.into(),
            value: value.into(),
            secret: true,
        }
    }

    #[test]
    fn custom_fields_round_trip_through_create_and_read() {
        let state = unlocked_state();
        let id = create_entry_impl(
            &state,
            EntryInput {
                fields: vec![
                    field("Recovery code", "abc-123"),
                    CustomField {
                        secret: false,
                        ..field("Support PIN", "4242")
                    },
                ],
                ..sample_input()
            },
        )
        .unwrap();

        let full = get_entry_impl(&state, id).unwrap();
        assert_eq!(full.fields.len(), 2);
        assert_eq!(full.fields[0].label, "Recovery code");
        assert_eq!(full.fields[0].value, "abc-123");
        assert!(full.fields[0].secret);
        assert_eq!(full.fields[1].label, "Support PIN");
        assert!(!full.fields[1].secret);
    }

    #[test]
    fn field_values_are_encrypted_at_rest() {
        let state = unlocked_state();
        create_entry_impl(
            &state,
            EntryInput {
                fields: vec![field("Recovery code", "super-secret-value")],
                ..sample_input()
            },
        )
        .unwrap();

        let stored: Vec<u8> = state
            .inner
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT encrypted_value FROM entry_fields", [], |r| r.get(0))
            .unwrap();
        assert!(
            !String::from_utf8_lossy(&stored).contains("super-secret-value"),
            "the value must not be readable in the database"
        );
    }

    #[test]
    fn an_update_replaces_the_field_set_rather_than_appending_to_it() {
        let state = unlocked_state();
        let id = create_entry_impl(
            &state,
            EntryInput {
                fields: vec![field("A", "1"), field("B", "2")],
                ..sample_input()
            },
        )
        .unwrap();

        update_entry_impl(
            &state,
            id,
            EntryInput {
                fields: vec![field("B", "changed"), field("C", "3")],
                ..sample_input()
            },
        )
        .unwrap();

        let full = get_entry_impl(&state, id).unwrap();
        let labels: Vec<&str> = full.fields.iter().map(|f| f.label.as_str()).collect();
        // Order follows the submitted order, and the removed field is gone.
        assert_eq!(labels, vec!["B", "C"]);
        assert_eq!(full.fields[0].value, "changed");
        let rows: i64 = state
            .inner
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT COUNT(*) FROM entry_fields", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 2, "the replaced rows must not linger");
    }

    #[test]
    fn a_blank_field_row_is_dropped_rather_than_rejected() {
        // An untouched "add a field" line in the form arrives blank; discarding
        // it is ordinary input handling, not an error to show the user.
        let state = unlocked_state();
        let id = create_entry_impl(
            &state,
            EntryInput {
                fields: vec![field("Kept", "v"), field("", "")],
                ..sample_input()
            },
        )
        .unwrap();
        let full = get_entry_impl(&state, id).unwrap();
        assert_eq!(full.fields.len(), 1);
        assert_eq!(full.fields[0].label, "Kept");
    }

    #[test]
    fn a_field_with_a_value_but_no_label_is_refused() {
        let state = unlocked_state();
        assert!(matches!(
            create_entry_impl(
                &state,
                EntryInput {
                    fields: vec![field("   ", "orphaned value")],
                    ..sample_input()
                },
            ),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn field_bounds_are_counted_in_characters() {
        let state = unlocked_state();
        // Same reason as category names and the master password: `len()` would
        // let a multi-byte label past a limit meant to count characters.
        let long_label = "é".repeat(MAX_FIELD_LABEL_CHARS + 1);
        assert!(matches!(
            create_entry_impl(
                &state,
                EntryInput {
                    fields: vec![field(&long_label, "v")],
                    ..sample_input()
                },
            ),
            Err(AppError::Validation(_))
        ));
        let at_limit = "é".repeat(MAX_FIELD_LABEL_CHARS);
        assert!(create_entry_impl(
            &state,
            EntryInput {
                fields: vec![field(&at_limit, "v")],
                ..sample_input()
            },
        )
        .is_ok());

        let too_many: Vec<CustomField> = (0..=MAX_FIELDS_PER_ENTRY)
            .map(|i| field(&format!("f{i}"), "v"))
            .collect();
        assert!(matches!(
            create_entry_impl(
                &state,
                EntryInput {
                    fields: too_many,
                    ..sample_input()
                },
            ),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn a_rejected_field_rolls_back_the_whole_write() {
        // Validation runs before the row is written on create; on update the
        // field write shares the entry's transaction, so a late failure must
        // not leave the entry half-edited.
        let state = unlocked_state();
        let id = create_entry_impl(&state, sample_input()).unwrap();
        let before = get_entry_impl(&state, id).unwrap();

        assert!(update_entry_impl(
            &state,
            id,
            EntryInput {
                title: "Renamed".into(),
                fields: vec![field("", "orphaned")],
                ..sample_input()
            },
        )
        .is_err());

        let after = get_entry_impl(&state, id).unwrap();
        assert_eq!(after.title, before.title, "the rename must not have landed");
        assert!(after.fields.is_empty());
    }

    #[test]
    fn purging_an_entry_takes_its_custom_fields_with_it() {
        let state = unlocked_state();
        let id = create_entry_impl(
            &state,
            EntryInput {
                fields: vec![field("Recovery code", "abc-123")],
                ..sample_input()
            },
        )
        .unwrap();
        delete_entry_impl(&state, id).unwrap();
        purge_entry_impl(&state, id).unwrap();

        let rows: i64 = state
            .inner
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT COUNT(*) FROM entry_fields", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 0, "fields must cascade with the purged entry");
    }

    #[test]
    fn a_restored_entry_still_has_its_fields() {
        let state = unlocked_state();
        let id = create_entry_impl(
            &state,
            EntryInput {
                fields: vec![field("Recovery code", "abc-123")],
                ..sample_input()
            },
        )
        .unwrap();
        delete_entry_impl(&state, id).unwrap();
        restore_entry_impl(&state, id).unwrap();

        let full = get_entry_impl(&state, id).unwrap();
        assert_eq!(full.fields.len(), 1);
        assert_eq!(full.fields[0].value, "abc-123");
    }

    // --- Bulk actions ---

    fn three_entries(state: &AppState) -> Vec<i64> {
        ["Alpha", "Beta", "Gamma"]
            .into_iter()
            .map(|title| {
                create_entry_impl(
                    state,
                    EntryInput {
                        title: title.into(),
                        ..sample_input()
                    },
                )
                .unwrap()
            })
            .collect()
    }

    fn category(state: &AppState, name: &str) -> i64 {
        let guard = state.inner.lock().unwrap();
        guard
            .conn
            .execute(
                "INSERT INTO categories (name, created_at, updated_at)
                 VALUES (?1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                [name],
            )
            .unwrap();
        guard.conn.last_insert_rowid()
    }

    #[test]
    fn bulk_category_move_reports_and_applies_to_every_selected_entry() {
        let state = unlocked_state();
        let ids = three_entries(&state);
        let work = category(&state, "Work");

        let moved = set_entries_category_impl(&state, &ids[..2], Some(work)).unwrap();
        assert_eq!(moved, 2);
        assert_eq!(get_entry_impl(&state, ids[0]).unwrap().category_id, Some(work));
        assert_eq!(get_entry_impl(&state, ids[1]).unwrap().category_id, Some(work));
        assert_eq!(get_entry_impl(&state, ids[2]).unwrap().category_id, None);

        // None takes them back out of the category.
        assert_eq!(set_entries_category_impl(&state, &ids[..2], None).unwrap(), 2);
        assert_eq!(get_entry_impl(&state, ids[0]).unwrap().category_id, None);
    }

    #[test]
    fn a_bulk_move_does_not_touch_the_password_or_its_history() {
        // Routing this through update_entry would decrypt and re-encrypt every
        // password, which is where password_changed_at and password history are
        // decided. A metadata-only write must leave both alone.
        let state = unlocked_state();
        let id = create_entry_impl(&state, sample_input()).unwrap();
        let before = get_entry_impl(&state, id).unwrap();
        let changed_at: String = state
            .inner
            .lock()
            .unwrap()
            .conn
            .query_row(
                "SELECT password_changed_at FROM password_entries WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();

        let work = category(&state, "Work");
        set_entries_category_impl(&state, &[id], Some(work)).unwrap();

        let after = get_entry_impl(&state, id).unwrap();
        assert_eq!(after.password, before.password);
        let still: String = state
            .inner
            .lock()
            .unwrap()
            .conn
            .query_row(
                "SELECT password_changed_at FROM password_entries WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(still, changed_at, "a metadata move must not age the password");
        let history: i64 = state
            .inner
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT COUNT(*) FROM password_history", [], |r| r.get(0))
            .unwrap();
        assert_eq!(history, 0, "a metadata move must not record a rotation");
    }

    #[test]
    fn bulk_favorite_and_bulk_trash_apply_to_the_selection() {
        let state = unlocked_state();
        let ids = three_entries(&state);

        assert_eq!(set_entries_favorite_impl(&state, &ids, true).unwrap(), 3);
        assert!(list_entries_impl(&state).unwrap().iter().all(|e| e.favorite));
        assert_eq!(set_entries_favorite_impl(&state, &ids[..1], false).unwrap(), 1);

        assert_eq!(delete_entries_impl(&state, &ids[..2]).unwrap(), 2);
        let live = list_entries_impl(&state).unwrap();
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].id, ids[2]);
        // Soft delete: the rows are in the trash, not gone.
        assert_eq!(list_deleted_entries_impl(&state).unwrap().len(), 2);
    }

    #[test]
    fn the_reported_count_comes_from_the_rows_actually_changed() {
        // An id that names nothing, or names something already in the trash,
        // must not be counted - the number goes straight to the user as "moved
        // N entries".
        let state = unlocked_state();
        let ids = three_entries(&state);
        delete_entries_impl(&state, &ids[..1]).unwrap();

        let work = category(&state, "Work");
        let moved = set_entries_category_impl(
            &state,
            &[ids[0], ids[1], 9999],
            Some(work),
        )
        .unwrap();
        assert_eq!(moved, 1, "only the one live entry was moved");

        // A repeated id is one entry, not two.
        assert_eq!(
            set_entries_favorite_impl(&state, &[ids[2], ids[2], ids[2]], true).unwrap(),
            1
        );
    }

    #[test]
    fn an_empty_or_oversized_selection_is_refused() {
        let state = unlocked_state();
        assert!(matches!(
            set_entries_favorite_impl(&state, &[], true),
            Err(AppError::Validation(_))
        ));
        let too_many: Vec<i64> = (1..=(MAX_BULK_IDS as i64 + 1)).collect();
        assert!(matches!(
            delete_entries_impl(&state, &too_many),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn a_failed_bulk_write_leaves_nothing_half_applied() {
        // One transaction: a category id that violates the foreign key must
        // roll the whole selection back rather than move the first few.
        let state = unlocked_state();
        let ids = three_entries(&state);
        let err = set_entries_category_impl(&state, &ids, Some(999_999));
        assert!(err.is_err(), "a dangling category must be refused");
        for id in ids {
            assert_eq!(get_entry_impl(&state, id).unwrap().category_id, None);
        }
    }

    #[test]
    fn bulk_actions_require_an_unlocked_vault() {
        let state = locked_state();
        assert!(matches!(
            set_entries_favorite_impl(&state, &[1], true),
            Err(AppError::Locked)
        ));
        assert!(matches!(
            delete_entries_impl(&state, &[1]),
            Err(AppError::Locked)
        ));
        assert!(matches!(
            set_entries_category_impl(&state, &[1], None),
            Err(AppError::Locked)
        ));
    }
}
