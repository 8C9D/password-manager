PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;

CREATE TABLE IF NOT EXISTS vault_metadata (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    vault_name TEXT NOT NULL,
    kdf_algorithm TEXT NOT NULL,
    kdf_salt BLOB NOT NULL,
    encrypted_test_value BLOB NOT NULL,
    test_value_nonce BLOB NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS categories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS password_entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    username TEXT NOT NULL,
    url_or_app_name TEXT NOT NULL,
    encrypted_password BLOB NOT NULL,
    password_nonce BLOB NOT NULL,
    encrypted_notes BLOB,
    notes_nonce BLOB,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_used_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_entries_category ON password_entries(category_id);
CREATE INDEX IF NOT EXISTS idx_entries_title ON password_entries(title);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
