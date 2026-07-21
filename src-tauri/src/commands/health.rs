//! Vault health audit: a read-only scan that decrypts every entry's password
//! in-process and reports weak, reused, and stale passwords. Plaintext never
//! leaves the backend - only per-entry issue flags (id, title, which problems)
//! are returned.
//!
//! The "weak" policy here is the audit's own (length + character-class
//! diversity); it is intentionally simpler and more conservative than the live
//! advisory strength meter in the UI, which uses an entropy model.

use std::collections::HashMap;

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::State;
use zeroize::Zeroizing;

use crate::crypto;
use crate::error::AppError;
use crate::state::{with_unlocked, AppState};

/// A password shorter than this with few character classes is flagged weak.
const WEAK_MIN_LENGTH: usize = 12;
/// Distinct classes (lower/upper/digit/other) below this, when also short, is weak.
const WEAK_MIN_CLASSES: u32 = 3;
/// Anything below this length is weak regardless of composition.
const HARD_MIN_LENGTH: usize = 8;
/// Passwords not changed in this long are flagged stale.
const STALE_AFTER_DAYS: i64 = 365;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryIssue {
    pub id: i64,
    pub title: String,
    pub weak: bool,
    pub reused: bool,
    pub stale: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultHealth {
    pub total: usize,
    pub weak_count: usize,
    pub reused_count: usize,
    pub stale_count: usize,
    /// Only entries with at least one problem, most-affected first.
    pub issues: Vec<EntryIssue>,
}

/// Count of distinct character classes present: lowercase, uppercase, digit, other.
pub(crate) fn char_classes(pw: &str) -> u32 {
    let (mut lower, mut upper, mut digit, mut other) = (false, false, false, false);
    for c in pw.chars() {
        if c.is_ascii_lowercase() {
            lower = true;
        } else if c.is_ascii_uppercase() {
            upper = true;
        } else if c.is_ascii_digit() {
            digit = true;
        } else {
            other = true;
        }
    }
    u32::from(lower) + u32::from(upper) + u32::from(digit) + u32::from(other)
}

/// The audit's weakness policy: too short outright, or short with low diversity.
pub(crate) fn is_weak(pw: &str) -> bool {
    let len = pw.chars().count();
    len < HARD_MIN_LENGTH || (len < WEAK_MIN_LENGTH && char_classes(pw) < WEAK_MIN_CLASSES)
}

/// Whether an RFC3339 `updated_at` is older than the staleness threshold as of
/// `now`. Unparseable timestamps are treated as not-stale (never a false alarm).
pub(crate) fn is_stale(updated_at: &str, now: chrono::DateTime<chrono::Utc>) -> bool {
    match chrono::DateTime::parse_from_rfc3339(updated_at) {
        Ok(dt) => (now - dt.with_timezone(&chrono::Utc)).num_days() > STALE_AFTER_DAYS,
        Err(_) => false,
    }
}

struct ScannedEntry {
    id: i64,
    title: String,
    password_hash: [u8; 32],
    weak: bool,
    stale: bool,
}

fn audit_vault_impl(state: &AppState) -> Result<VaultHealth, AppError> {
    let now = chrono::Utc::now();
    with_unlocked(state, |s, key| {
        let mut stmt = s.conn.prepare(
            "SELECT id, title, encrypted_password, password_nonce, updated_at
             FROM password_entries
             ORDER BY title COLLATE NOCASE ASC",
        )?;
        type Raw = (i64, String, Vec<u8>, Vec<u8>, String);
        let raw: Vec<Raw> = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let total = raw.len();
        let mut hash_counts: HashMap<[u8; 32], usize> = HashMap::new();
        let mut scanned = Vec::with_capacity(total);
        for (id, title, enc, nonce, updated_at) in raw {
            let pw = Zeroizing::new(crypto::decrypt(key, &enc, &nonce)?);
            let password_hash: [u8; 32] = Sha256::digest(pw.as_slice()).into();
            let pw_str = String::from_utf8_lossy(&pw);
            let weak = is_weak(&pw_str);
            let stale = is_stale(&updated_at, now);
            *hash_counts.entry(password_hash).or_insert(0) += 1;
            scanned.push(ScannedEntry {
                id,
                title,
                password_hash,
                weak,
                stale,
            });
        }

        let mut issues: Vec<EntryIssue> = scanned
            .into_iter()
            .filter_map(|e| {
                let reused = hash_counts.get(&e.password_hash).copied().unwrap_or(0) > 1;
                (e.weak || reused || e.stale).then_some(EntryIssue {
                    id: e.id,
                    title: e.title,
                    weak: e.weak,
                    reused,
                    stale: e.stale,
                })
            })
            .collect();

        // Most-affected first, then alphabetical for stable ordering.
        issues.sort_by(|a, b| {
            let sev = |i: &EntryIssue| u32::from(i.weak) + u32::from(i.reused) + u32::from(i.stale);
            sev(b)
                .cmp(&sev(a))
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });

        Ok(VaultHealth {
            total,
            weak_count: issues.iter().filter(|i| i.weak).count(),
            reused_count: issues.iter().filter(|i| i.reused).count(),
            stale_count: issues.iter().filter(|i| i.stale).count(),
            issues,
        })
    })
}

#[tauri::command]
pub fn audit_vault(state: State<'_, AppState>) -> Result<VaultHealth, AppError> {
    audit_vault_impl(&state)
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

    fn unlocked_state() -> AppState {
        let state = AppState::new(db::open_in_memory().unwrap());
        state.inner.lock().unwrap().key = Some(Zeroizing::new(fixed_key()));
        state
    }

    fn insert_entry(state: &AppState, title: &str, password: &str, updated_at: &str) {
        let key = fixed_key();
        let ct = crypto::encrypt(&key, password.as_bytes()).unwrap();
        let guard = state.inner.lock().unwrap();
        guard
            .conn
            .execute(
                "INSERT INTO password_entries
                    (title, username, url_or_app_name,
                     encrypted_password, password_nonce, created_at, updated_at)
                 VALUES (?1, 'u', 'x', ?2, ?3, ?4, ?4)",
                rusqlite::params![title, ct.bytes, ct.nonce.as_slice(), updated_at],
            )
            .unwrap();
    }

    #[test]
    fn char_classes_counts_distinct_kinds() {
        assert_eq!(char_classes("abc"), 1);
        assert_eq!(char_classes("abcABC"), 2);
        assert_eq!(char_classes("abcABC123"), 3);
        assert_eq!(char_classes("abcABC123!@#"), 4);
        assert_eq!(char_classes(""), 0);
    }

    #[test]
    fn weakness_policy_flags_short_or_low_diversity() {
        assert!(is_weak("short")); // < 8
        assert!(is_weak("abcdefgh")); // 8 chars, 1 class
        assert!(is_weak("abcdefghij1")); // 11 chars, 2 classes
        assert!(!is_weak("abcABC12!def")); // 12 chars, 4 classes
        assert!(!is_weak("abcdefghijkl")); // 12 chars: length carries it
        assert!(!is_weak("Ab1!Ab1!")); // 8 chars but 4 classes and >= 8
    }

    #[test]
    fn staleness_compares_against_threshold() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert!(is_stale("2024-06-01T00:00:00Z", now)); // > 365 days
        assert!(!is_stale("2025-06-01T00:00:00Z", now)); // < 365 days
        assert!(!is_stale("not-a-date", now)); // unparseable -> not stale
    }

    #[test]
    fn audit_flags_weak_reused_and_counts_totals() {
        let state = unlocked_state();
        let recent = crate::db::now_iso8601();
        // Two entries share a password -> both reused. One is also weak.
        insert_entry(&state, "Alpha", "abc", &recent); // weak (short) + reused
        insert_entry(&state, "Beta", "abc", &recent); // reused (strong-ish? no, also weak & short)
        // A strong, unique password -> no issues.
        insert_entry(&state, "Gamma", "Zx9!qWer_p2@Lm", &recent);

        let health = audit_vault_impl(&state).unwrap();
        assert_eq!(health.total, 3);
        assert_eq!(health.reused_count, 2);
        // Gamma has no issues, so it is absent from the list.
        assert!(!health.issues.iter().any(|i| i.title == "Gamma"));
        let alpha = health.issues.iter().find(|i| i.title == "Alpha").unwrap();
        assert!(alpha.weak && alpha.reused);
    }

    #[test]
    fn audit_flags_stale_entries() {
        let state = unlocked_state();
        insert_entry(&state, "Old", "Zx9!qWer_p2@Lm", "2020-01-01T00:00:00Z");
        let health = audit_vault_impl(&state).unwrap();
        let old = health.issues.iter().find(|i| i.title == "Old").unwrap();
        assert!(old.stale);
        assert_eq!(health.stale_count, 1);
    }

    #[test]
    fn empty_vault_is_healthy() {
        let state = unlocked_state();
        let health = audit_vault_impl(&state).unwrap();
        assert_eq!(health.total, 0);
        assert!(health.issues.is_empty());
    }

    #[test]
    fn audit_requires_unlocked_vault() {
        let state = AppState::new(db::open_in_memory().unwrap());
        assert!(matches!(audit_vault_impl(&state), Err(AppError::Locked)));
    }
}
