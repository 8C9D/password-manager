use std::path::Path;

use rusqlite::Connection;

use crate::error::AppError;

/// Baseline schema, frozen at user_version 0. Never edit it for schema
/// changes; add a numbered entry to `MIGRATIONS` instead.
const SCHEMA: &str = include_str!("schema.sql");

struct Migration {
    version: i32,
    sql: &'static str,
}

/// Ordered, append-only list of schema migrations. Each entry runs in its own
/// transaction and bumps user_version on success.
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: "ALTER TABLE password_entries ADD COLUMN encrypted_totp BLOB;
              ALTER TABLE password_entries ADD COLUMN totp_nonce BLOB;",
    },
    Migration {
        version: 2,
        sql: "ALTER TABLE password_entries ADD COLUMN is_favorite INTEGER NOT NULL DEFAULT 0;
              ALTER TABLE password_entries ADD COLUMN tags TEXT NOT NULL DEFAULT '[]';",
    },
    Migration {
        version: 3,
        // Stale-password auditing needs the password's own age; updated_at is
        // bumped by every metadata edit (rename, tags, notes) and so hides an
        // old password behind any edit. Backfill with updated_at - the best
        // approximation available for existing rows.
        sql: "ALTER TABLE password_entries ADD COLUMN password_changed_at TEXT;
              UPDATE password_entries SET password_changed_at = updated_at;",
    },
    Migration {
        version: 4,
        // Previous passwords, encrypted with the vault key exactly like
        // password_entries.encrypted_password. ON DELETE CASCADE relies on the
        // per-connection foreign_keys pragma set in enable_foreign_keys.
        sql: "CREATE TABLE password_history (
                  id INTEGER PRIMARY KEY AUTOINCREMENT,
                  entry_id INTEGER NOT NULL
                      REFERENCES password_entries(id) ON DELETE CASCADE,
                  encrypted_password BLOB NOT NULL,
                  password_nonce BLOB NOT NULL,
                  changed_at TEXT NOT NULL
              );
              CREATE INDEX idx_history_entry ON password_history(entry_id, id DESC);",
    },
    Migration {
        version: 5,
        // Soft delete. NULL means live; a timestamp means the entry is in the
        // trash, still fully encrypted and restorable until it is purged.
        sql: "ALTER TABLE password_entries ADD COLUMN deleted_at TEXT;
              CREATE INDEX idx_entries_deleted ON password_entries(deleted_at);",
    },
    Migration {
        version: 6,
        // Per-entry rotation reminder, in days from password_changed_at. NULL
        // means no reminder, which is the default and what every existing row
        // gets. Distinct from the health scan's fixed 365-day staleness rule:
        // this one is the user's own cadence for one account.
        sql: "ALTER TABLE password_entries ADD COLUMN password_expiry_days INTEGER;",
    },
    Migration {
        version: 7,
        // User-defined extra fields on an entry. The value is encrypted with the
        // vault key exactly like a password; the label is not, matching how
        // title, username, and url_or_app_name are already stored in the clear.
        // ON DELETE CASCADE relies on the per-connection foreign_keys pragma set
        // in enable_foreign_keys, like password_history.
        sql: "CREATE TABLE entry_fields (
                  id INTEGER PRIMARY KEY AUTOINCREMENT,
                  entry_id INTEGER NOT NULL
                      REFERENCES password_entries(id) ON DELETE CASCADE,
                  label TEXT NOT NULL,
                  encrypted_value BLOB NOT NULL,
                  value_nonce BLOB NOT NULL,
                  is_secret INTEGER NOT NULL DEFAULT 1,
                  position INTEGER NOT NULL DEFAULT 0
              );
              CREATE INDEX idx_fields_entry ON entry_fields(entry_id, position);",
    },
];

#[cfg(test)]
const LATEST_VERSION: i32 = if MIGRATIONS.is_empty() {
    0
} else {
    MIGRATIONS[MIGRATIONS.len() - 1].version
};

fn user_version(conn: &Connection) -> Result<i32, AppError> {
    let v: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    Ok(v)
}

fn migrate(conn: &mut Connection, migrations: &[Migration]) -> Result<(), AppError> {
    let latest = migrations.last().map(|m| m.version).unwrap_or(0);
    let mut version = user_version(conn)?;
    if version > latest {
        return Err(AppError::Internal(format!(
            "vault database version {version} is newer than this app supports ({latest})"
        )));
    }

    // Version 0 covers both a fresh database and a pre-migration vault whose
    // tables already exist: the baseline is idempotent (CREATE IF NOT EXISTS),
    // so existing vaults are adopted without change.
    if version == 0 {
        conn.execute_batch(SCHEMA)?;
    }

    for m in migrations {
        if m.version <= version {
            continue;
        }
        if m.version != version + 1 {
            return Err(AppError::Internal(format!(
                "non-sequential migration: have version {version}, next is {}",
                m.version
            )));
        }
        let tx = conn.transaction()?;
        tx.execute_batch(m.sql)?;
        tx.pragma_update(None, "user_version", m.version)?;
        tx.commit()?;
        version = m.version;
    }
    Ok(())
}

pub fn open_and_migrate(path: &Path) -> Result<Connection, AppError> {
    let mut conn = Connection::open(path)?;
    enable_foreign_keys(&conn)?;
    migrate(&mut conn, MIGRATIONS)?;
    Ok(conn)
}

/// `foreign_keys` is a per-connection pragma that SQLite defaults to OFF, so it
/// must be set on every open - not just at schema creation - for the
/// `ON DELETE SET NULL` on `password_entries.category_id` to actually fire.
fn enable_foreign_keys(conn: &Connection) -> Result<(), AppError> {
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(())
}

#[cfg(test)]
pub fn open_in_memory() -> Result<Connection, AppError> {
    let mut conn = Connection::open_in_memory()?;
    enable_foreign_keys(&conn)?;
    migrate(&mut conn, MIGRATIONS)?;
    Ok(conn)
}

pub fn now_iso8601() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Current Unix time in whole seconds, for TOTP time-step counting.
pub fn now_unix() -> u64 {
    chrono::Utc::now().timestamp().max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_conn() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    fn table_names(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<String>, _>>()
            .unwrap()
    }

    #[test]
    fn fresh_db_reaches_latest_version() {
        let mut conn = fresh_conn();
        migrate(&mut conn, MIGRATIONS).unwrap();
        assert_eq!(user_version(&conn).unwrap(), LATEST_VERSION);
        let tables = table_names(&conn);
        for t in [
            "vault_metadata",
            "categories",
            "password_entries",
            "settings",
            "password_history",
        ] {
            assert!(tables.iter().any(|n| n == t), "missing table {t}");
        }
    }

    #[test]
    fn deleting_a_category_nulls_entry_refs_when_foreign_keys_are_enforced() {
        // open_in_memory enables foreign_keys, so ON DELETE SET NULL must fire.
        let conn = open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO categories (id, name, created_at, updated_at)
             VALUES (1, 'Work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO password_entries
                (id, category_id, title, username, url_or_app_name,
                 encrypted_password, password_nonce, created_at, updated_at)
             VALUES (1, 1, 'E', 'u', 'x', X'00', X'00',
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM categories WHERE id = 1", []).unwrap();
        let category_id: Option<i64> = conn
            .query_row(
                "SELECT category_id FROM password_entries WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(category_id, None);
    }

    #[test]
    fn migrate_is_idempotent() {
        let mut conn = fresh_conn();
        migrate(&mut conn, MIGRATIONS).unwrap();
        migrate(&mut conn, MIGRATIONS).unwrap();
        assert_eq!(user_version(&conn).unwrap(), LATEST_VERSION);
    }

    #[test]
    fn v0_db_with_data_migrates_losslessly() {
        // Simulate a vault created by the pre-migration build: baseline schema,
        // user_version 0, real rows in every table.
        let mut conn = fresh_conn();
        conn.execute_batch(SCHEMA).unwrap();
        conn.execute(
            "INSERT INTO vault_metadata
                (id, vault_name, kdf_algorithm, kdf_salt,
                 encrypted_test_value, test_value_nonce, created_at, updated_at)
             VALUES (1, 'My Vault', 'argon2id', X'00112233445566778899AABBCCDDEEFF',
                     X'DEADBEEF', X'000102030405060708090A0B',
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO categories (name, created_at, updated_at)
             VALUES ('Work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO password_entries
                (category_id, title, username, url_or_app_name,
                 encrypted_password, password_nonce, created_at, updated_at)
             VALUES (1, 'GitHub', 'alice', 'github.com',
                     X'CAFEBABE', X'000102030405060708090A0B',
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO settings (key, value, updated_at)
             VALUES ('auto_lock_secs', '600', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        assert_eq!(user_version(&conn).unwrap(), 0);

        migrate(&mut conn, MIGRATIONS).unwrap();

        assert_eq!(user_version(&conn).unwrap(), LATEST_VERSION);
        let (name, salt): (String, Vec<u8>) = conn
            .query_row(
                "SELECT vault_name, kdf_salt FROM vault_metadata WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(name, "My Vault");
        assert_eq!(salt.len(), 16);
        let title: String = conn
            .query_row("SELECT title FROM password_entries WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(title, "GitHub");
        let value: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'auto_lock_secs'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(value, "600");
        let cat: String = conn
            .query_row("SELECT name FROM categories WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(cat, "Work");
        // Migration 3 backfills the password-change time from updated_at.
        let changed: Option<String> = conn
            .query_row(
                "SELECT password_changed_at FROM password_entries WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(changed.as_deref(), Some("2026-01-01T00:00:00Z"));
    }

    /// A database as a build shipping migrations 1..=`version` would have left
    /// it: the baseline schema plus exactly that prefix of the real migration
    /// list, with `user_version` set accordingly.
    fn conn_at_version(version: i32) -> Connection {
        let mut conn = fresh_conn();
        let prefix: Vec<Migration> = MIGRATIONS
            .iter()
            .filter(|m| m.version <= version)
            .map(|m| Migration {
                version: m.version,
                sql: m.sql,
            })
            .collect();
        migrate(&mut conn, &prefix).unwrap();
        assert_eq!(user_version(&conn).unwrap(), version);
        conn
    }

    #[test]
    fn a_vault_from_any_shipped_version_upgrades_straight_to_the_latest() {
        // Not every user upgrades one release at a time. A vault last opened at
        // v2 has to survive the whole remaining chain in one run, and the rows
        // it already holds have to come through it intact - a later migration
        // that assumes a column an earlier one only backfills would lose data
        // exactly here.
        for start in 0..=LATEST_VERSION {
            let mut conn = conn_at_version(start);
            conn.execute(
                "INSERT INTO categories (id, name, created_at, updated_at)
                 VALUES (1, 'Work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO password_entries
                    (id, category_id, title, username, url_or_app_name,
                     encrypted_password, password_nonce, created_at, updated_at)
                 VALUES (1, 1, 'GitHub', 'alice', 'github.com', X'CAFEBABE',
                         X'000102030405060708090A0B',
                         '2026-01-01T00:00:00Z', '2026-02-01T00:00:00Z')",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO settings (key, value, updated_at)
                 VALUES ('auto_lock_secs', '600', '2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();

            migrate(&mut conn, MIGRATIONS).unwrap();

            assert_eq!(
                user_version(&conn).unwrap(),
                LATEST_VERSION,
                "starting from v{start}"
            );
            let (title, ciphertext, favorite, tags, deleted, expiry): (
                String,
                Vec<u8>,
                bool,
                String,
                Option<String>,
                Option<u32>,
            ) = conn
                .query_row(
                    "SELECT title, encrypted_password, is_favorite, tags,
                            deleted_at, password_expiry_days
                     FROM password_entries WHERE id = 1",
                    [],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                        ))
                    },
                )
                .unwrap();
            assert_eq!(title, "GitHub", "starting from v{start}");
            assert_eq!(ciphertext, vec![0xCA, 0xFE, 0xBA, 0xBE]);
            // The defaults every pre-existing row must land on.
            assert!(!favorite, "starting from v{start}");
            assert_eq!(tags, "[]", "starting from v{start}");
            assert_eq!(deleted, None, "a migrated row must not arrive trashed");
            assert_eq!(expiry, None, "a migrated row must not arrive with a reminder");

            let value: String = conn
                .query_row(
                    "SELECT value FROM settings WHERE key = 'auto_lock_secs'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(value, "600", "starting from v{start}");
            let cat: String = conn
                .query_row("SELECT name FROM categories WHERE id = 1", [], |r| r.get(0))
                .unwrap();
            assert_eq!(cat, "Work", "starting from v{start}");
        }
    }

    #[test]
    fn the_password_change_time_is_backfilled_only_for_rows_that_predate_it() {
        // Migration 3 backfills from updated_at, so a vault that starts at v2
        // or earlier gets the approximation; one already at v3+ keeps whatever
        // it recorded. Staleness auditing reads this column, so a backfill that
        // silently overwrote a real value would reset every password's age.
        let mut before = conn_at_version(2);
        before
            .execute(
                "INSERT INTO password_entries
                    (id, title, username, url_or_app_name,
                     encrypted_password, password_nonce, created_at, updated_at)
                 VALUES (1, 'Old', 'u', 'x', X'00', X'00',
                         '2020-01-01T00:00:00Z', '2026-02-01T00:00:00Z')",
                [],
            )
            .unwrap();
        migrate(&mut before, MIGRATIONS).unwrap();
        let changed: Option<String> = before
            .query_row(
                "SELECT password_changed_at FROM password_entries WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(changed.as_deref(), Some("2026-02-01T00:00:00Z"));

        let mut after = conn_at_version(3);
        after
            .execute(
                "INSERT INTO password_entries
                    (id, title, username, url_or_app_name,
                     encrypted_password, password_nonce, created_at, updated_at,
                     password_changed_at)
                 VALUES (1, 'Rotated', 'u', 'x', X'00', X'00',
                         '2020-01-01T00:00:00Z', '2026-02-01T00:00:00Z',
                         '2026-07-01T00:00:00Z')",
                [],
            )
            .unwrap();
        migrate(&mut after, MIGRATIONS).unwrap();
        let kept: Option<String> = after
            .query_row(
                "SELECT password_changed_at FROM password_entries WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(kept.as_deref(), Some("2026-07-01T00:00:00Z"));
    }

    #[test]
    fn history_rows_survive_the_migrations_added_after_them() {
        // password_history arrived at v4; v5 and v6 both alter
        // password_entries, and the cascade between the two tables has to still
        // be intact afterwards.
        let mut conn = conn_at_version(4);
        conn.execute(
            "INSERT INTO password_entries
                (id, title, username, url_or_app_name,
                 encrypted_password, password_nonce, created_at, updated_at)
             VALUES (1, 'E', 'u', 'x', X'00', X'00',
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO password_history
                (entry_id, encrypted_password, password_nonce, changed_at)
             VALUES (1, X'DEADBEEF', X'000102030405060708090A0B',
                     '2026-01-02T00:00:00Z')",
            [],
        )
        .unwrap();

        migrate(&mut conn, MIGRATIONS).unwrap();

        let kept: Vec<u8> = conn
            .query_row(
                "SELECT encrypted_password FROM password_history WHERE entry_id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(kept, vec![0xDE, 0xAD, 0xBE, 0xEF]);

        enable_foreign_keys(&conn).unwrap();
        conn.execute("DELETE FROM password_entries WHERE id = 1", []).unwrap();
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM password_history", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 0, "history must still cascade with its entry");
    }

    #[test]
    fn newer_db_version_is_refused() {
        let mut conn = fresh_conn();
        conn.pragma_update(None, "user_version", LATEST_VERSION + 1)
            .unwrap();
        let err = migrate(&mut conn, MIGRATIONS).unwrap_err();
        assert!(matches!(err, AppError::Internal(_)));
    }

    #[test]
    fn steps_apply_sequentially_and_set_version() {
        let steps: &[Migration] = &[
            Migration {
                version: 1,
                sql: "CREATE TABLE step_one (id INTEGER PRIMARY KEY);",
            },
            Migration {
                version: 2,
                sql: "ALTER TABLE step_one ADD COLUMN note TEXT;",
            },
        ];
        let mut conn = fresh_conn();
        migrate(&mut conn, steps).unwrap();
        assert_eq!(user_version(&conn).unwrap(), 2);
        conn.execute("INSERT INTO step_one (id, note) VALUES (1, 'ok')", [])
            .unwrap();
    }

    #[test]
    fn already_applied_steps_are_skipped() {
        let steps: &[Migration] = &[Migration {
            version: 1,
            sql: "CREATE TABLE only_once (id INTEGER PRIMARY KEY);",
        }];
        let mut conn = fresh_conn();
        migrate(&mut conn, steps).unwrap();
        // A second run must skip version 1; re-running the CREATE would fail.
        migrate(&mut conn, steps).unwrap();
        assert_eq!(user_version(&conn).unwrap(), 1);
    }

    #[test]
    fn failed_step_rolls_back_and_leaves_version_unchanged() {
        let steps: &[Migration] = &[Migration {
            version: 1,
            sql: "CREATE TABLE half_done (id INTEGER PRIMARY KEY);
                  CREATE TABLE half_done (id INTEGER PRIMARY KEY);",
        }];
        let mut conn = fresh_conn();
        assert!(migrate(&mut conn, steps).is_err());
        assert_eq!(user_version(&conn).unwrap(), 0);
        assert!(
            !table_names(&conn).iter().any(|n| n == "half_done"),
            "partial migration must roll back"
        );
    }

    #[test]
    fn non_sequential_migration_list_is_refused() {
        let steps: &[Migration] = &[Migration {
            version: 2,
            sql: "CREATE TABLE skipped (id INTEGER PRIMARY KEY);",
        }];
        let mut conn = fresh_conn();
        let err = migrate(&mut conn, steps).unwrap_err();
        assert!(matches!(err, AppError::Internal(_)));
        assert_eq!(user_version(&conn).unwrap(), 0);
    }
}
