use serde::{Serialize, Serializer};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("vault is locked")]
    Locked,

    #[error("vault already exists")]
    VaultAlreadyExists,

    #[error("vault does not exist")]
    VaultNotFound,

    #[error("incorrect master password")]
    WrongPassword,

    #[error("too many attempts: wait {0} seconds before trying again")]
    TooManyUnlockAttempts(u64),

    #[error("entry not found")]
    EntryNotFound,

    #[error("category not found")]
    CategoryNotFound,

    #[error("validation: {0}")]
    Validation(&'static str),

    #[error("crypto: {0}")]
    Crypto(&'static str),

    #[error("database error")]
    Database(#[from] rusqlite::Error),

    #[error("io error")]
    Io(#[from] std::io::Error),

    #[error("internal error")]
    Internal(String),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct Wire<'a> {
            kind: &'a str,
            message: String,
        }
        let kind = match self {
            AppError::Locked => "locked",
            AppError::VaultAlreadyExists => "vault_already_exists",
            AppError::VaultNotFound => "vault_not_found",
            AppError::WrongPassword => "wrong_password",
            AppError::TooManyUnlockAttempts(_) => "too_many_unlock_attempts",
            AppError::EntryNotFound => "entry_not_found",
            AppError::CategoryNotFound => "category_not_found",
            AppError::Validation(_) => "validation",
            AppError::Crypto(_) => "crypto",
            AppError::Database(_) => "database",
            AppError::Io(_) => "io",
            AppError::Internal(_) => "internal",
        };
        Wire {
            kind,
            message: self.to_string(),
        }
        .serialize(serializer)
    }
}

impl From<tauri::Error> for AppError {
    fn from(value: tauri::Error) -> Self {
        AppError::Internal(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialize an error the way Tauri sends it to the frontend and assert on
    /// the `{ kind, message }` wire fields that `formatBackendError` consumes.
    fn assert_wire(e: AppError, kind: &str, message: &str) {
        let v = serde_json::to_value(&e).expect("AppError serializes");
        assert_eq!(v["kind"].as_str(), Some(kind), "kind for {e:?}");
        assert_eq!(v["message"].as_str(), Some(message), "message for {e:?}");
    }

    #[test]
    fn unit_variants_map_to_expected_kind_and_message() {
        // These kind strings are a contract: the frontend switches on them in
        // `formatBackendError` (see tauri-invoke.spec.ts). Changing one here
        // without updating the frontend silently breaks error handling.
        assert_wire(AppError::Locked, "locked", "vault is locked");
        assert_wire(
            AppError::VaultAlreadyExists,
            "vault_already_exists",
            "vault already exists",
        );
        assert_wire(
            AppError::VaultNotFound,
            "vault_not_found",
            "vault does not exist",
        );
        assert_wire(
            AppError::WrongPassword,
            "wrong_password",
            "incorrect master password",
        );
        assert_wire(AppError::EntryNotFound, "entry_not_found", "entry not found");
        assert_wire(
            AppError::CategoryNotFound,
            "category_not_found",
            "category not found",
        );
    }

    #[test]
    fn too_many_unlock_attempts_carries_the_wait_seconds() {
        assert_wire(
            AppError::TooManyUnlockAttempts(3),
            "too_many_unlock_attempts",
            "too many attempts: wait 3 seconds before trying again",
        );
    }

    #[test]
    fn validation_message_keeps_the_prefix_the_frontend_strips() {
        // `formatBackendError` strips a leading "validation: " from the message,
        // so the wire message must carry that prefix.
        assert_wire(
            AppError::Validation("title is required"),
            "validation",
            "validation: title is required",
        );
    }

    #[test]
    fn crypto_message_carries_crypto_prefix() {
        assert_wire(
            AppError::Crypto("decryption failed"),
            "crypto",
            "crypto: decryption failed",
        );
    }

    #[test]
    fn database_and_io_errors_serialize_to_opaque_kinds() {
        assert_wire(
            AppError::Database(rusqlite::Error::QueryReturnedNoRows),
            "database",
            "database error",
        );
        assert_wire(
            AppError::Io(std::io::Error::other("disk gone")),
            "io",
            "io error",
        );
    }

    #[test]
    fn internal_error_does_not_leak_detail_to_the_wire() {
        // The inner string can hold sensitive context (paths, backend messages);
        // it must not reach the frontend.
        let e = AppError::Internal("db path /Users/secret/vault.db".into());
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"].as_str(), Some("internal"));
        let msg = v["message"].as_str().unwrap();
        assert_eq!(msg, "internal error");
        assert!(!msg.contains("secret"), "internal detail leaked: {msg}");
    }
}
