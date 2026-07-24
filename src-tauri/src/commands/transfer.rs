use std::path::Path;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use tauri::State;
use zeroize::Zeroizing;

use crate::crypto;
use crate::crypto::totp::TotpConfig;
use crate::db::now_iso8601;
use crate::error::AppError;
use crate::state::{with_state, with_unlocked, AppState};

use super::vault::{read_vault_crypto_row, verify_password};

const EXPORT_FORMAT: &str = "password-manager-export";
const EXPORT_FORMAT_VERSION: u32 = 1;

/// On-disk export file: a cleartext header identifying the format and KDF
/// inputs, plus the AES-256-GCM-encrypted payload.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportFile {
    format: String,
    format_version: u32,
    kdf_algorithm: String,
    salt: String,
    nonce: String,
    ciphertext: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportPayload {
    categories: Vec<String>,
    entries: Vec<ExportEntry>,
    settings: ExportSettings,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportEntry {
    title: String,
    username: String,
    url_or_app_name: String,
    password: String,
    notes: Option<String>,
    category: Option<String>,
    created_at: String,
    updated_at: String,
    last_used_at: Option<String>,
    /// Canonical TOTP config, present only for entries with a 2FA secret.
    /// `#[serde(default)]` keeps version-1 export files (which lacked this
    /// field) readable.
    #[serde(default)]
    totp: Option<TotpConfig>,
    /// `#[serde(default)]` on both keeps older export files (which lacked
    /// favorites and tags) readable.
    #[serde(default)]
    is_favorite: bool,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportSettings {
    auto_lock_secs: Option<u64>,
    /// `#[serde(default)]` keeps older export files (which lacked this field)
    /// readable.
    #[serde(default)]
    clipboard_clear_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub entries_imported: usize,
    pub categories_created: usize,
}

fn gather_payload(
    s: &mut crate::state::AppStateInner,
    key: &[u8; 32],
) -> Result<ExportPayload, AppError> {
    let mut stmt = s.conn.prepare("SELECT name FROM categories ORDER BY name")?;
    let categories: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    let mut stmt = s.conn.prepare(
        "SELECT e.title, e.username, e.url_or_app_name,
                e.encrypted_password, e.password_nonce,
                e.encrypted_notes, e.notes_nonce,
                c.name, e.created_at, e.updated_at, e.last_used_at,
                e.encrypted_totp, e.totp_nonce, e.is_favorite, e.tags
         FROM password_entries e
         LEFT JOIN categories c ON c.id = e.category_id
         ORDER BY e.id",
    )?;
    struct Row {
        title: String,
        username: String,
        url_or_app_name: String,
        enc_pw: Vec<u8>,
        pw_nonce: Vec<u8>,
        enc_notes: Option<Vec<u8>>,
        notes_nonce: Option<Vec<u8>>,
        category: Option<String>,
        created_at: String,
        updated_at: String,
        last_used_at: Option<String>,
        enc_totp: Option<Vec<u8>>,
        totp_nonce: Option<Vec<u8>>,
        is_favorite: bool,
        tags: String,
    }
    let rows: Vec<Row> = stmt
        .query_map([], |r| {
            Ok(Row {
                title: r.get(0)?,
                username: r.get(1)?,
                url_or_app_name: r.get(2)?,
                enc_pw: r.get(3)?,
                pw_nonce: r.get(4)?,
                enc_notes: r.get(5)?,
                notes_nonce: r.get(6)?,
                category: r.get(7)?,
                created_at: r.get(8)?,
                updated_at: r.get(9)?,
                last_used_at: r.get(10)?,
                enc_totp: r.get(11)?,
                totp_nonce: r.get(12)?,
                is_favorite: r.get(13)?,
                tags: r.get(14)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);

    let mut entries = Vec::with_capacity(rows.len());
    for row in rows {
        let password = String::from_utf8(crypto::decrypt(key, &row.enc_pw, &row.pw_nonce)?)
            .map_err(|_| AppError::Crypto("password is not valid utf-8"))?;
        let notes = match (row.enc_notes, row.notes_nonce) {
            (Some(c), Some(n)) => Some(
                String::from_utf8(crypto::decrypt(key, &c, &n)?)
                    .map_err(|_| AppError::Crypto("notes are not valid utf-8"))?,
            ),
            _ => None,
        };
        let totp = match (row.enc_totp, row.totp_nonce) {
            (Some(c), Some(n)) => {
                let json = Zeroizing::new(crypto::decrypt(key, &c, &n)?);
                Some(
                    serde_json::from_slice::<TotpConfig>(&json)
                        .map_err(|_| AppError::Crypto("stored TOTP config is invalid"))?,
                )
            }
            _ => None,
        };
        entries.push(ExportEntry {
            title: row.title,
            username: row.username,
            url_or_app_name: row.url_or_app_name,
            password,
            notes,
            category: row.category,
            created_at: row.created_at,
            updated_at: row.updated_at,
            last_used_at: row.last_used_at,
            totp,
            is_favorite: row.is_favorite,
            tags: crate::commands::entries::tags_from_json(&row.tags),
        });
    }

    let read_setting = |key: &str| -> Option<u64> {
        s.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get::<_, String>(0)
            })
            .ok()
            .and_then(|v| v.parse().ok())
    };
    let auto_lock_secs = read_setting("auto_lock_secs");
    let clipboard_clear_secs = read_setting("clipboard_clear_secs");

    Ok(ExportPayload {
        categories,
        entries,
        settings: ExportSettings {
            auto_lock_secs,
            clipboard_clear_secs,
        },
    })
}

fn export_vault_impl(
    state: &AppState,
    master_password: String,
    path: &Path,
) -> Result<(), AppError> {
    let master_password = Zeroizing::new(master_password);

    let (salt, encrypted_test, test_nonce) = with_state(state, |s| {
        if s.key.is_none() {
            return Err(AppError::Locked);
        }
        read_vault_crypto_row(&s.conn)
    })?;

    // Confirms the caller knows the master password; the verified key is the
    // vault key, so it also decrypts every entry.
    let vault_key = verify_password(&salt, &encrypted_test, &test_nonce, &master_password)?;

    let payload = with_state(state, |s| gather_payload(s, &vault_key))?;
    let plaintext = Zeroizing::new(
        serde_json::to_vec(&payload)
            .map_err(|e| AppError::Internal(format!("export serialization failed: {e}")))?,
    );

    // The export gets its own salt so the file never shares KDF inputs with
    // the vault, even when exported with the vault's master password.
    let export_salt = crypto::generate_salt();
    let export_key = crypto::derive_key(&master_password, &export_salt)?;
    let ct = crypto::encrypt(&export_key, &plaintext)?;

    let file = ExportFile {
        format: EXPORT_FORMAT.into(),
        format_version: EXPORT_FORMAT_VERSION,
        kdf_algorithm: "argon2id".into(),
        salt: B64.encode(export_salt),
        nonce: B64.encode(ct.nonce),
        ciphertext: B64.encode(&ct.bytes),
    };
    let json = serde_json::to_string_pretty(&file)
        .map_err(|e| AppError::Internal(format!("export serialization failed: {e}")))?;
    std::fs::write(path, json)?;
    Ok(())
}

#[tauri::command]
pub fn export_vault(
    state: State<'_, AppState>,
    master_password: String,
    path: String,
) -> Result<(), AppError> {
    export_vault_impl(&state, master_password, Path::new(&path))
}

fn parse_export_file(bytes: &[u8]) -> Result<ExportFile, AppError> {
    let file: ExportFile = serde_json::from_slice(bytes)
        .map_err(|_| AppError::Validation("not a valid export file"))?;
    if file.format != EXPORT_FORMAT {
        return Err(AppError::Validation("not a valid export file"));
    }
    if file.format_version > EXPORT_FORMAT_VERSION {
        return Err(AppError::Validation(
            "export file was created by a newer version of this app",
        ));
    }
    if file.kdf_algorithm != "argon2id" {
        return Err(AppError::Validation("unsupported export key derivation"));
    }
    Ok(file)
}

fn decrypt_payload(file: &ExportFile, password: &str) -> Result<ExportPayload, AppError> {
    let decode = |field: &str| {
        B64.decode(field)
            .map_err(|_| AppError::Validation("export file is corrupted"))
    };
    let salt = decode(&file.salt)?;
    let nonce = decode(&file.nonce)?;
    let ciphertext = decode(&file.ciphertext)?;

    let key = crypto::derive_key(password, &salt)?;
    let plaintext = Zeroizing::new(
        crypto::decrypt(&key, &ciphertext, &nonce).map_err(|_| AppError::WrongPassword)?,
    );
    serde_json::from_slice(&plaintext).map_err(|_| AppError::Validation("export file is corrupted"))
}

fn import_vault_impl(
    state: &AppState,
    path: &Path,
    password: String,
) -> Result<ImportSummary, AppError> {
    let password = Zeroizing::new(password);
    let bytes = std::fs::read(path)?;
    let file = parse_export_file(&bytes)?;

    // Slow KDF happens before taking the state lock.
    let payload = decrypt_payload(&file, &password)?;

    with_unlocked(state, |s, key| {
        let tx = s.conn.transaction()?;
        let now = now_iso8601();
        let mut categories_created = 0usize;

        // Existing categories are reused by name (the column is UNIQUE);
        // entries are always inserted as new rows, never overwriting.
        let mut category_ids = std::collections::HashMap::new();
        let names: std::collections::BTreeSet<&str> = payload
            .categories
            .iter()
            .map(|n| n.as_str())
            .chain(payload.entries.iter().filter_map(|e| e.category.as_deref()))
            .collect();
        for name in names {
            let existing: Option<i64> = tx
                .query_row("SELECT id FROM categories WHERE name = ?1", [name], |r| {
                    r.get(0)
                })
                .ok();
            let id = match existing {
                Some(id) => id,
                None => {
                    tx.execute(
                        "INSERT INTO categories (name, created_at, updated_at) VALUES (?1, ?2, ?2)",
                        rusqlite::params![name, now],
                    )?;
                    categories_created += 1;
                    tx.last_insert_rowid()
                }
            };
            category_ids.insert(name.to_string(), id);
        }

        for entry in &payload.entries {
            let pw_ct = crypto::encrypt(key, entry.password.as_bytes())?;
            let (notes_bytes, notes_nonce) = match entry.notes.as_deref() {
                Some(n) if !n.is_empty() => {
                    let ct = crypto::encrypt(key, n.as_bytes())?;
                    (Some(ct.bytes), Some(ct.nonce.to_vec()))
                }
                _ => (None, None),
            };
            let (totp_bytes, totp_nonce) = match &entry.totp {
                Some(config) => {
                    let json = Zeroizing::new(serde_json::to_vec(config).map_err(|_| {
                        AppError::Internal("failed to serialize TOTP config".into())
                    })?);
                    let ct = crypto::encrypt(key, &json)?;
                    (Some(ct.bytes), Some(ct.nonce.to_vec()))
                }
                None => (None, None),
            };
            let category_id = entry
                .category
                .as_deref()
                .and_then(|n| category_ids.get(n))
                .copied();
            // Tags are re-normalized on the way in: export files can be
            // hand-edited, and older files carry no tags at all.
            let tags = crate::commands::entries::tags_to_json(
                &crate::commands::entries::normalize_tags(&entry.tags),
            );
            tx.execute(
                "INSERT INTO password_entries
                    (category_id, title, username, url_or_app_name,
                     encrypted_password, password_nonce,
                     encrypted_notes, notes_nonce,
                     encrypted_totp, totp_nonce,
                     created_at, updated_at, last_used_at, is_favorite, tags)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                rusqlite::params![
                    category_id,
                    entry.title,
                    entry.username,
                    entry.url_or_app_name,
                    pw_ct.bytes,
                    pw_ct.nonce.as_slice(),
                    notes_bytes,
                    notes_nonce,
                    totp_bytes,
                    totp_nonce,
                    entry.created_at,
                    entry.updated_at,
                    entry.last_used_at,
                    entry.is_favorite,
                    tags,
                ],
            )?;
        }

        tx.commit()?;
        Ok(ImportSummary {
            entries_imported: payload.entries.len(),
            categories_created,
        })
    })
}

#[tauri::command]
pub fn import_vault(
    state: State<'_, AppState>,
    path: String,
    password: String,
) -> Result<ImportSummary, AppError> {
    import_vault_impl(&state, Path::new(&path), password)
}

// --- Plaintext CSV import (Bitwarden / 1Password / Chrome exports) ---

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvImportSummary {
    pub imported: usize,
    pub skipped: usize,
    pub categories_created: usize,
}

/// Parse RFC 4180 CSV text into records of fields. Handles quoted fields,
/// escaped quotes (`""`), and commas/newlines embedded inside quotes. Both LF
/// and CRLF line endings are accepted.
fn parse_csv(input: &str) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut dirty = false; // any content seen for the current record
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' => {
                // A quote only opens a quoted field at the field's start. A
                // stray quote inside an unquoted field is treated as a literal
                // character (matching lenient real-world parsers) rather than
                // re-entering quote mode and swallowing the rest of the file.
                if field.is_empty() {
                    in_quotes = true;
                } else {
                    field.push('"');
                }
                dirty = true;
            }
            ',' => {
                record.push(std::mem::take(&mut field));
                dirty = true;
            }
            '\r' => {}
            '\n' => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                dirty = false;
            }
            _ => {
                field.push(c);
                dirty = true;
            }
        }
    }
    if dirty || !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records
}

const TITLE_ALIASES: &[&str] = &["name", "title"];
const USERNAME_ALIASES: &[&str] = &["username", "login_username", "user", "email"];
const PASSWORD_ALIASES: &[&str] = &["password", "login_password"];
const URL_ALIASES: &[&str] = &["url", "urls", "website", "login_uri", "uri"];
const NOTES_ALIASES: &[&str] = &["notes", "note"];
const CATEGORY_ALIASES: &[&str] = &["folder", "category", "grouping"];
const TOTP_ALIASES: &[&str] = &["login_totp", "otpauth", "totp", "one-time password", "otp"];
const FAVORITE_ALIASES: &[&str] = &["favorite", "favourite", "fav"];

fn find_col(headers: &[String], aliases: &[&str]) -> Option<usize> {
    headers.iter().position(|h| aliases.contains(&h.as_str()))
}

fn cell(row: &[String], col: Option<usize>) -> &str {
    col.and_then(|i| row.get(i)).map(String::as_str).unwrap_or("")
}

/// Bitwarden exports `1` for favorites; other tools use `true`/`yes`.
fn is_truthy(cell: &str) -> bool {
    matches!(cell.trim().to_lowercase().as_str(), "1" | "true" | "yes")
}

fn import_csv_content_impl(state: &AppState, csv: &str) -> Result<CsvImportSummary, AppError> {
    // Strip a leading UTF-8 BOM that Excel/Chrome exports sometimes prepend.
    let csv = csv.strip_prefix('\u{feff}').unwrap_or(csv);
    let records = parse_csv(csv);
    let mut rows = records.iter();
    let headers: Vec<String> = match rows.next() {
        Some(h) => h.iter().map(|c| c.trim().to_lowercase()).collect(),
        None => return Err(AppError::Validation("CSV file is empty")),
    };
    let c_title = find_col(&headers, TITLE_ALIASES);
    let c_user = find_col(&headers, USERNAME_ALIASES);
    let c_pass = find_col(&headers, PASSWORD_ALIASES)
        .ok_or(AppError::Validation("CSV has no recognizable password column"))?;
    let c_url = find_col(&headers, URL_ALIASES);
    let c_notes = find_col(&headers, NOTES_ALIASES);
    let c_cat = find_col(&headers, CATEGORY_ALIASES);
    let c_totp = find_col(&headers, TOTP_ALIASES);
    let c_fav = find_col(&headers, FAVORITE_ALIASES);

    let data: Vec<&Vec<String>> = rows.collect();

    with_unlocked(state, |s, key| {
        let tx = s.conn.transaction()?;
        let now = now_iso8601();

        // Categories are created lazily inside the loop, only for rows that are
        // actually imported, so a skipped (e.g. password-less) row never leaves
        // behind an empty orphan category.
        let mut category_ids: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();
        let mut categories_created = 0usize;

        let mut imported = 0usize;
        let mut skipped = 0usize;
        for row in &data {
            if row.iter().all(|f| f.trim().is_empty()) {
                continue; // blank line
            }
            // Password is not trimmed - leading/trailing spaces can be significant.
            let password = cell(row, Some(c_pass));
            if password.is_empty() {
                skipped += 1;
                continue;
            }
            let title_raw = cell(row, c_title).trim();
            let url = cell(row, c_url).trim();
            let title = if !title_raw.is_empty() {
                title_raw
            } else if !url.is_empty() {
                url
            } else {
                "(untitled)"
            };
            let username = cell(row, c_user).trim();
            let notes = cell(row, c_notes);
            let category = cell(row, c_cat).trim();
            let is_favorite = is_truthy(cell(row, c_fav));

            let pw_ct = crypto::encrypt(key, password.as_bytes())?;
            let (notes_bytes, notes_nonce) = if notes.trim().is_empty() {
                (None, None)
            } else {
                let ct = crypto::encrypt(key, notes.as_bytes())?;
                (Some(ct.bytes), Some(ct.nonce.to_vec()))
            };
            // TOTP is best-effort: an unparseable secret is dropped, not fatal.
            let totp_val = cell(row, c_totp).trim();
            let (totp_bytes, totp_nonce) = if totp_val.is_empty() {
                (None, None)
            } else {
                match crate::commands::entries::encrypt_totp(key, totp_val) {
                    Ok((b, n)) => (Some(b), Some(n)),
                    Err(_) => (None, None),
                }
            };
            let category_id = if category.is_empty() {
                None
            } else {
                let id = match category_ids.get(category) {
                    Some(&id) => id,
                    None => {
                        let existing: Option<i64> = tx
                            .query_row(
                                "SELECT id FROM categories WHERE name = ?1",
                                [category],
                                |r| r.get(0),
                            )
                            .ok();
                        let id = match existing {
                            Some(id) => id,
                            None => {
                                tx.execute(
                                    "INSERT INTO categories (name, created_at, updated_at)
                                     VALUES (?1, ?2, ?2)",
                                    rusqlite::params![category, now],
                                )?;
                                categories_created += 1;
                                tx.last_insert_rowid()
                            }
                        };
                        category_ids.insert(category.to_string(), id);
                        id
                    }
                };
                Some(id)
            };

            tx.execute(
                "INSERT INTO password_entries
                    (category_id, title, username, url_or_app_name,
                     encrypted_password, password_nonce,
                     encrypted_notes, notes_nonce,
                     encrypted_totp, totp_nonce,
                     created_at, updated_at, last_used_at, is_favorite)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, NULL, ?12)",
                rusqlite::params![
                    category_id,
                    title,
                    username,
                    url,
                    pw_ct.bytes,
                    pw_ct.nonce.as_slice(),
                    notes_bytes,
                    notes_nonce,
                    totp_bytes,
                    totp_nonce,
                    now,
                    is_favorite,
                ],
            )?;
            imported += 1;
        }

        tx.commit()?;
        Ok(CsvImportSummary {
            imported,
            skipped,
            categories_created,
        })
    })
}

fn import_csv_impl(state: &AppState, path: &Path) -> Result<CsvImportSummary, AppError> {
    let bytes = std::fs::read(path)?;
    let text =
        String::from_utf8(bytes).map_err(|_| AppError::Validation("CSV file is not valid UTF-8"))?;
    import_csv_content_impl(state, &text)
}

#[tauri::command]
pub fn import_csv(
    state: State<'_, AppState>,
    path: String,
) -> Result<CsvImportSummary, AppError> {
    import_csv_impl(&state, Path::new(&path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use std::path::PathBuf;

    const PW: &str = "vault-password-1";

    struct TempFile(PathBuf);

    impl TempFile {
        fn new(name: &str) -> Self {
            Self(std::env::temp_dir().join(format!("pm-test-{}-{name}", std::process::id())))
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn state_with_vault(password: &str) -> AppState {
        let state = AppState::new(db::open_in_memory().unwrap());
        let salt = crypto::generate_salt();
        let key = crypto::derive_key(password, &salt).unwrap();
        let test_ct = crypto::encrypt(&key, crypto::TEST_VALUE_PLAINTEXT).unwrap();
        {
            let guard = state.inner.lock().unwrap();
            guard
                .conn
                .execute(
                    "INSERT INTO vault_metadata
                        (id, vault_name, kdf_algorithm, kdf_salt,
                         encrypted_test_value, test_value_nonce, created_at, updated_at)
                     VALUES (1, 'My Vault', 'argon2id', ?1, ?2, ?3,
                             '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                    rusqlite::params![salt.as_slice(), test_ct.bytes, test_ct.nonce.as_slice()],
                )
                .unwrap();
        }
        state.inner.lock().unwrap().key = Some(key);
        state
    }

    fn add_category(state: &AppState, name: &str) -> i64 {
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

    fn add_entry(
        state: &AppState,
        title: &str,
        password: &str,
        notes: Option<&str>,
        category_id: Option<i64>,
    ) {
        let key = **state.inner.lock().unwrap().key.as_ref().unwrap();
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
                    (category_id, title, username, url_or_app_name,
                     encrypted_password, password_nonce, encrypted_notes, notes_nonce,
                     created_at, updated_at)
                 VALUES (?1, ?2, 'user', 'example.com', ?3, ?4, ?5, ?6,
                         '2026-02-03T04:05:06Z', '2026-02-03T04:05:06Z')",
                rusqlite::params![
                    category_id,
                    title,
                    pw_ct.bytes,
                    pw_ct.nonce.as_slice(),
                    notes_bytes,
                    notes_nonce,
                ],
            )
            .unwrap();
    }

    fn decrypted_entries(state: &AppState) -> Vec<(String, String, Option<String>, Option<String>)> {
        let key = **state.inner.lock().unwrap().key.as_ref().unwrap();
        let guard = state.inner.lock().unwrap();
        let mut stmt = guard
            .conn
            .prepare(
                "SELECT e.title, e.encrypted_password, e.password_nonce,
                        e.encrypted_notes, e.notes_nonce, c.name
                 FROM password_entries e
                 LEFT JOIN categories c ON c.id = e.category_id
                 ORDER BY e.title, e.id",
            )
            .unwrap();
        type Row = (String, Vec<u8>, Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>, Option<String>);
        let rows: Vec<Row> = stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows.into_iter()
            .map(|(title, enc_pw, pw_nonce, enc_notes, notes_nonce, cat)| {
                let pw =
                    String::from_utf8(crypto::decrypt(&key, &enc_pw, &pw_nonce).unwrap()).unwrap();
                let notes = match (enc_notes, notes_nonce) {
                    (Some(c), Some(n)) => Some(
                        String::from_utf8(crypto::decrypt(&key, &c, &n).unwrap()).unwrap(),
                    ),
                    _ => None,
                };
                (title, pw, notes, cat)
            })
            .collect()
    }

    #[test]
    fn export_import_preserves_totp_secrets() {
        use crate::commands::entries::encrypt_totp;

        // RFC 6238 SHA1 seed, base32-encoded.
        const RFC_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

        let source = state_with_vault(PW);
        add_entry(&source, "GitHub", "hunter2", None, None);
        {
            let key = **source.inner.lock().unwrap().key.as_ref().unwrap();
            let (b, n) = encrypt_totp(&key, RFC_SECRET).unwrap();
            let guard = source.inner.lock().unwrap();
            guard
                .conn
                .execute(
                    "UPDATE password_entries SET encrypted_totp = ?1, totp_nonce = ?2
                     WHERE title = 'GitHub'",
                    rusqlite::params![b, n],
                )
                .unwrap();
        }

        let file = TempFile::new("totp-roundtrip.json");
        export_vault_impl(&source, PW.into(), &file.0).unwrap();

        let target = state_with_vault(PW);
        import_vault_impl(&target, &file.0, PW.into()).unwrap();

        // The imported entry must carry the TOTP secret, decryptable under the
        // target vault key and reproducing the original secret. Before the fix
        // the export dropped it and the imported entry had no TOTP.
        let (enc, nonce): (Vec<u8>, Vec<u8>) = {
            let guard = target.inner.lock().unwrap();
            guard
                .conn
                .query_row(
                    "SELECT encrypted_totp, totp_nonce FROM password_entries WHERE title = 'GitHub'",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap()
        };
        let key = **target.inner.lock().unwrap().key.as_ref().unwrap();
        let json = crypto::decrypt(&key, &enc, &nonce).unwrap();
        let config: TotpConfig = serde_json::from_slice(&json).unwrap();
        assert_eq!(config.secret_base32, RFC_SECRET);
    }

    #[test]
    fn export_import_preserves_favorites_and_tags() {
        let source = state_with_vault(PW);
        add_entry(&source, "GitHub", "hunter2", None, None);
        add_entry(&source, "Bank", "s3cret!", None, None);
        source
            .inner
            .lock()
            .unwrap()
            .conn
            .execute(
                "UPDATE password_entries
                 SET is_favorite = 1, tags = '[\"work\",\"email\"]'
                 WHERE title = 'GitHub'",
                [],
            )
            .unwrap();

        let file = TempFile::new("fav-tags-roundtrip.json");
        export_vault_impl(&source, PW.into(), &file.0).unwrap();

        let target = state_with_vault(PW);
        import_vault_impl(&target, &file.0, PW.into()).unwrap();

        let rows: Vec<(String, bool, String)> = {
            let guard = target.inner.lock().unwrap();
            let mut stmt = guard
                .conn
                .prepare("SELECT title, is_favorite, tags FROM password_entries ORDER BY title")
                .unwrap();
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            rows
        };
        assert_eq!(
            rows,
            vec![
                ("Bank".into(), false, "[]".into()),
                ("GitHub".into(), true, "[\"work\",\"email\"]".into()),
            ]
        );
    }

    #[test]
    fn export_entry_without_favorite_and_tags_fields_deserializes_with_defaults() {
        // Export files written before favorites/tags existed lack both fields;
        // serde defaults must keep them importable.
        let entry: ExportEntry = serde_json::from_str(
            r#"{"title":"Old","username":"u","urlOrAppName":"x",
                "password":"pw","notes":null,"category":null,
                "createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-01T00:00:00Z",
                "lastUsedAt":null}"#,
        )
        .unwrap();
        assert!(!entry.is_favorite);
        assert!(entry.tags.is_empty());
        assert!(entry.totp.is_none());
    }

    #[test]
    fn export_import_round_trips_entries_categories_and_fields() {
        let file = TempFile::new("round-trip.json");
        let source = state_with_vault(PW);
        let work = add_category(&source, "Work");
        add_entry(&source, "GitHub", "hunter2", Some("note one"), Some(work));
        add_entry(&source, "Bank", "s3cret!", None, None);

        export_vault_impl(&source, PW.into(), &file.0).unwrap();

        // Import into a fresh vault with a different master password.
        let target = state_with_vault("другой-пароль-2");
        let summary = import_vault_impl(&target, &file.0, PW.into()).unwrap();
        assert_eq!(summary.entries_imported, 2);
        assert_eq!(summary.categories_created, 1);

        assert_eq!(
            decrypted_entries(&target),
            vec![
                ("Bank".into(), "s3cret!".into(), None, None),
                (
                    "GitHub".into(),
                    "hunter2".into(),
                    Some("note one".into()),
                    Some("Work".into())
                ),
            ]
        );

        // Original timestamps survive the round trip.
        let created: String = target
            .inner
            .lock()
            .unwrap()
            .conn
            .query_row(
                "SELECT created_at FROM password_entries WHERE title = 'GitHub'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(created, "2026-02-03T04:05:06Z");
    }

    #[test]
    fn export_carries_both_settings() {
        let source = state_with_vault(PW);
        {
            let guard = source.inner.lock().unwrap();
            guard
                .conn
                .execute_batch(
                    "INSERT INTO settings (key, value, updated_at)
                     VALUES ('auto_lock_secs', '600', '2026-01-01T00:00:00Z');
                     INSERT INTO settings (key, value, updated_at)
                     VALUES ('clipboard_clear_secs', '45', '2026-01-01T00:00:00Z');",
                )
                .unwrap();
        }

        let file = TempFile::new("settings-export.json");
        export_vault_impl(&source, PW.into(), &file.0).unwrap();

        let parsed = parse_export_file(&std::fs::read(&file.0).unwrap()).unwrap();
        let payload = decrypt_payload(&parsed, PW).unwrap();
        assert_eq!(payload.settings.auto_lock_secs, Some(600));
        assert_eq!(payload.settings.clipboard_clear_secs, Some(45));
    }

    #[test]
    fn export_rejects_wrong_master_password_and_writes_nothing() {
        let file = TempFile::new("wrong-pw-export.json");
        let state = state_with_vault(PW);
        let err = export_vault_impl(&state, "wrong-password".into(), &file.0).unwrap_err();
        assert!(matches!(err, AppError::WrongPassword));
        assert!(!file.0.exists(), "no file must be written on failure");
    }

    #[test]
    fn export_rejects_when_locked() {
        let file = TempFile::new("locked-export.json");
        let state = state_with_vault(PW);
        state.inner.lock().unwrap().key = None;
        assert!(matches!(
            export_vault_impl(&state, PW.into(), &file.0),
            Err(AppError::Locked)
        ));
    }

    #[test]
    fn import_with_wrong_password_fails_cleanly() {
        let file = TempFile::new("wrong-pw-import.json");
        let source = state_with_vault(PW);
        add_entry(&source, "GitHub", "hunter2", None, None);
        export_vault_impl(&source, PW.into(), &file.0).unwrap();

        let target = state_with_vault("target-password");
        let err = import_vault_impl(&target, &file.0, "not-the-password".into()).unwrap_err();
        assert!(matches!(err, AppError::WrongPassword));
        assert!(decrypted_entries(&target).is_empty(), "nothing imported");
    }

    #[test]
    fn import_reuses_existing_category_and_duplicates_entries() {
        let file = TempFile::new("merge.json");
        let source = state_with_vault(PW);
        let work = add_category(&source, "Work");
        add_entry(&source, "GitHub", "hunter2", None, Some(work));
        export_vault_impl(&source, PW.into(), &file.0).unwrap();

        // Target already has the category and an entry with the same title.
        let target = state_with_vault(PW);
        let existing = add_category(&target, "Work");
        add_entry(&target, "GitHub", "old-password", None, Some(existing));

        let summary = import_vault_impl(&target, &file.0, PW.into()).unwrap();
        assert_eq!(summary.entries_imported, 1);
        assert_eq!(summary.categories_created, 0);

        // Both entries exist: the import adds a duplicate, never overwrites.
        assert_eq!(
            decrypted_entries(&target),
            vec![
                ("GitHub".into(), "old-password".into(), None, Some("Work".into())),
                ("GitHub".into(), "hunter2".into(), None, Some("Work".into())),
            ]
        );
    }

    #[test]
    fn import_rejects_malformed_file() {
        let file = TempFile::new("malformed.json");
        std::fs::write(&file.0, b"{\"not\": \"an export\"}").unwrap();
        let state = state_with_vault(PW);
        assert!(matches!(
            import_vault_impl(&state, &file.0, PW.into()),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn import_rejects_newer_format_version() {
        let file = TempFile::new("newer-version.json");
        let source = state_with_vault(PW);
        export_vault_impl(&source, PW.into(), &file.0).unwrap();
        let mut parsed: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file.0).unwrap()).unwrap();
        parsed["formatVersion"] = serde_json::json!(EXPORT_FORMAT_VERSION + 1);
        std::fs::write(&file.0, serde_json::to_vec(&parsed).unwrap()).unwrap();

        assert!(matches!(
            import_vault_impl(&source, &file.0, PW.into()),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn import_rejects_when_locked() {
        let file = TempFile::new("locked-import.json");
        let source = state_with_vault(PW);
        export_vault_impl(&source, PW.into(), &file.0).unwrap();
        source.inner.lock().unwrap().key = None;
        assert!(matches!(
            import_vault_impl(&source, &file.0, PW.into()),
            Err(AppError::Locked)
        ));
    }

    // --- CSV import ---

    #[test]
    fn parse_csv_handles_quotes_commas_and_embedded_newlines() {
        let input = "a,b,c\n1,\"two, and\na newline\",\"quote \"\"x\"\"\"\n";
        let records = parse_csv(input);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0], vec!["a", "b", "c"]);
        assert_eq!(records[1], vec!["1", "two, and\na newline", "quote \"x\""]);
    }

    #[test]
    fn parse_csv_handles_crlf_and_no_trailing_newline() {
        let records = parse_csv("h1,h2\r\nv1,v2");
        assert_eq!(records, vec![vec!["h1", "h2"], vec!["v1", "v2"]]);
    }

    #[test]
    fn parse_csv_treats_a_stray_quote_in_an_unquoted_field_as_literal() {
        // The note field ends with a literal `"` the exporter failed to quote.
        // It must be kept as a literal character, not re-enter quote mode and
        // swallow the following row.
        let records = parse_csv("name,note\nSite A,size is 15\"\nSite B,ok\n");
        assert_eq!(records.len(), 3);
        assert_eq!(records[0], vec!["name", "note"]);
        assert_eq!(records[1], vec!["Site A", "size is 15\""]);
        assert_eq!(records[2], vec!["Site B", "ok"]);
    }

    #[test]
    fn import_chrome_csv_creates_entries() {
        let state = state_with_vault(PW);
        let csv = "name,url,username,password,note\n\
                   GitHub,https://github.com,alice,hunter2,my note\n\
                   Bank,https://bank.example,bob,s3cret!,\n";
        let summary = import_csv_content_impl(&state, csv).unwrap();
        assert_eq!(summary.imported, 2);
        assert_eq!(summary.skipped, 0);

        let entries = decrypted_entries(&state);
        let bank = entries.iter().find(|e| e.0 == "Bank").unwrap();
        assert_eq!(bank.1, "s3cret!");
        let github = entries.iter().find(|e| e.0 == "GitHub").unwrap();
        assert_eq!(github.1, "hunter2");
        assert_eq!(github.2.as_deref(), Some("my note"));
    }

    #[test]
    fn import_bitwarden_csv_maps_folder_and_skips_passwordless_rows() {
        let state = state_with_vault(PW);
        let csv = "folder,favorite,type,name,notes,fields,login_uri,login_username,login_password,login_totp\n\
                   Work,,login,GitHub,,,,alice,hunter2,\n\
                   ,,login,No Password Row,,,,,\n";
        let summary = import_csv_content_impl(&state, csv).unwrap();
        assert_eq!(summary.imported, 1);
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.categories_created, 1);

        let entries = decrypted_entries(&state);
        let github = entries.iter().find(|e| e.0 == "GitHub").unwrap();
        assert_eq!(github.1, "hunter2");
        assert_eq!(github.3.as_deref(), Some("Work")); // category
    }

    #[test]
    fn import_csv_maps_favorite_column() {
        let state = state_with_vault(PW);
        let csv = "folder,favorite,name,login_username,login_password\n\
                   ,1,Starred,alice,pw-one-1\n\
                   ,,Plain,bob,pw-two-2\n\
                   ,0,Zero,carol,pw-three-3\n";
        assert_eq!(import_csv_content_impl(&state, csv).unwrap().imported, 3);

        let favs: Vec<(String, bool)> = {
            let guard = state.inner.lock().unwrap();
            let mut stmt = guard
                .conn
                .prepare("SELECT title, is_favorite FROM password_entries ORDER BY title")
                .unwrap();
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            rows
        };
        assert_eq!(
            favs,
            vec![
                ("Plain".into(), false),
                ("Starred".into(), true),
                ("Zero".into(), false),
            ]
        );
    }

    #[test]
    fn import_csv_does_not_create_categories_for_skipped_rows() {
        let state = state_with_vault(PW);
        // 'Archive' is referenced only by a password-less row that gets skipped,
        // so it must not be created as an empty orphan category.
        let csv = "folder,name,username,password\n\
                   Archive,Old Site,alice,\n\
                   Work,GitHub,bob,hunter2\n";
        let summary = import_csv_content_impl(&state, csv).unwrap();
        assert_eq!(summary.imported, 1);
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.categories_created, 1);

        let cat_names: Vec<String> = {
            let guard = state.inner.lock().unwrap();
            let mut stmt = guard
                .conn
                .prepare("SELECT name FROM categories ORDER BY name")
                .unwrap();
            let names = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            names
        };
        assert_eq!(cat_names, vec!["Work".to_string()]);
    }

    #[test]
    fn import_csv_falls_back_to_url_for_missing_title() {
        let state = state_with_vault(PW);
        let csv = "name,url,username,password\n\
                   ,https://example.com,alice,pw12345\n";
        assert_eq!(import_csv_content_impl(&state, csv).unwrap().imported, 1);
        let entries = decrypted_entries(&state);
        assert_eq!(entries[0].0, "https://example.com");
    }

    #[test]
    fn import_csv_rejects_file_without_password_column() {
        let state = state_with_vault(PW);
        let csv = "name,url,username\nGitHub,https://github.com,alice\n";
        assert!(matches!(
            import_csv_content_impl(&state, csv),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn import_csv_rejects_when_locked() {
        let state = state_with_vault(PW);
        state.inner.lock().unwrap().key = None;
        let csv = "name,password\nX,pw\n";
        assert!(matches!(
            import_csv_content_impl(&state, csv),
            Err(AppError::Locked)
        ));
    }
}
