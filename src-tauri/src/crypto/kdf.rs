use argon2::{Algorithm, Argon2, Params, Version};
use rand::{rngs::OsRng, RngCore};
use zeroize::Zeroizing;

use crate::error::AppError;

pub const SALT_LEN: usize = 16;
pub const KEY_LEN: usize = 32;

pub type VaultKey = Zeroizing<[u8; KEY_LEN]>;

const M_COST_KIB: u32 = 65_536;
const T_COST: u32 = 3;
const P_COST: u32 = 1;

pub fn generate_salt() -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    OsRng.fill_bytes(&mut salt);
    salt
}

pub fn derive_key(password: &str, salt: &[u8]) -> Result<VaultKey, AppError> {
    if salt.len() != SALT_LEN {
        return Err(AppError::Crypto("invalid salt length"));
    }
    let params = Params::new(M_COST_KIB, T_COST, P_COST, Some(KEY_LEN))
        .map_err(|_| AppError::Crypto("invalid argon2 params"))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = Zeroizing::new([0u8; KEY_LEN]);
    argon2
        .hash_password_into(password.as_bytes(), salt, key.as_mut_slice())
        .map_err(|_| AppError::Crypto("key derivation failed"))?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_deterministic_key_for_same_inputs() {
        let salt = generate_salt();
        let k1 = derive_key("correct horse battery staple", &salt).unwrap();
        let k2 = derive_key("correct horse battery staple", &salt).unwrap();
        assert_eq!(k1.as_slice(), k2.as_slice());
    }

    #[test]
    fn different_passwords_yield_different_keys() {
        let salt = generate_salt();
        let k1 = derive_key("password-one", &salt).unwrap();
        let k2 = derive_key("password-two", &salt).unwrap();
        assert_ne!(k1.as_slice(), k2.as_slice());
    }

    #[test]
    fn different_salts_yield_different_keys() {
        let s1 = generate_salt();
        let s2 = generate_salt();
        assert_ne!(s1, s2);
        let k1 = derive_key("same-password", &s1).unwrap();
        let k2 = derive_key("same-password", &s2).unwrap();
        assert_ne!(k1.as_slice(), k2.as_slice());
    }

    #[test]
    fn rejects_wrong_salt_length() {
        assert!(derive_key("anything", &[0u8; 8]).is_err());
    }
}
