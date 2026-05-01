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

    #[error("entry not found")]
    EntryNotFound,

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
            AppError::EntryNotFound => "entry_not_found",
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
