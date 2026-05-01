pub mod aead;
pub mod kdf;

pub use aead::{decrypt, encrypt};
pub use kdf::{derive_key, generate_salt, VaultKey};

pub const TEST_VALUE_PLAINTEXT: &[u8] = b"vault-ok-v1";
