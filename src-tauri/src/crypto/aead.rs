use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::{rngs::OsRng, RngCore};

use crate::error::AppError;

pub const NONCE_LEN: usize = 12;

pub struct Ciphertext {
    pub bytes: Vec<u8>,
    pub nonce: [u8; NONCE_LEN],
}

pub fn generate_nonce() -> [u8; NONCE_LEN] {
    let mut nonce = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce);
    nonce
}

pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<Ciphertext, AppError> {
    let cipher = Aes256Gcm::new(key.into());
    let nonce_bytes = generate_nonce();
    let nonce = Nonce::from_slice(&nonce_bytes);
    let bytes = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| AppError::Crypto("encryption failed"))?;
    Ok(Ciphertext {
        bytes,
        nonce: nonce_bytes,
    })
}

pub fn decrypt(key: &[u8; 32], ciphertext: &[u8], nonce: &[u8]) -> Result<Vec<u8>, AppError> {
    if nonce.len() != NONCE_LEN {
        return Err(AppError::Crypto("invalid nonce length"));
    }
    let cipher = Aes256Gcm::new(key.into());
    let nonce = Nonce::from_slice(nonce);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| AppError::Crypto("decryption failed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_key() -> [u8; 32] {
        let mut k = [0u8; 32];
        for (i, b) in k.iter_mut().enumerate() {
            *b = i as u8;
        }
        k
    }

    #[test]
    fn round_trip_recovers_plaintext() {
        let key = fixed_key();
        let pt = b"a very secret string";
        let ct = encrypt(&key, pt).unwrap();
        let recovered = decrypt(&key, &ct.bytes, &ct.nonce).unwrap();
        assert_eq!(recovered, pt);
    }

    #[test]
    fn round_trip_empty_plaintext() {
        let key = fixed_key();
        let ct = encrypt(&key, b"").unwrap();
        let recovered = decrypt(&key, &ct.bytes, &ct.nonce).unwrap();
        assert_eq!(recovered, b"");
    }

    #[test]
    fn wrong_key_fails_decryption() {
        let key1 = fixed_key();
        let mut key2 = fixed_key();
        key2[0] ^= 0xFF;
        let ct = encrypt(&key1, b"hello").unwrap();
        assert!(decrypt(&key2, &ct.bytes, &ct.nonce).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails_decryption() {
        let key = fixed_key();
        let ct = encrypt(&key, b"hello world").unwrap();
        let mut bad = ct.bytes.clone();
        bad[0] ^= 0x01;
        assert!(decrypt(&key, &bad, &ct.nonce).is_err());
    }

    #[test]
    fn unique_nonce_per_encryption() {
        let key = fixed_key();
        let a = encrypt(&key, b"same").unwrap();
        let b = encrypt(&key, b"same").unwrap();
        assert_ne!(a.nonce, b.nonce);
        assert_ne!(a.bytes, b.bytes);
    }

    #[test]
    fn rejects_wrong_nonce_length() {
        let key = fixed_key();
        let ct = encrypt(&key, b"x").unwrap();
        assert!(decrypt(&key, &ct.bytes, &[0u8; 8]).is_err());
    }

    #[test]
    fn round_trips_and_authenticates_many_random_inputs() {
        use rand::{rngs::OsRng, Rng, RngCore};
        for _ in 0..200 {
            let mut key = [0u8; 32];
            OsRng.fill_bytes(&mut key);
            let len = OsRng.gen_range(0..512);
            let mut pt = vec![0u8; len];
            OsRng.fill_bytes(&mut pt);

            let ct = encrypt(&key, &pt).unwrap();
            assert_eq!(decrypt(&key, &ct.bytes, &ct.nonce).unwrap(), pt);

            // Flipping any single ciphertext bit must break authentication.
            if !ct.bytes.is_empty() {
                let mut tampered = ct.bytes.clone();
                let i = OsRng.gen_range(0..tampered.len());
                tampered[i] ^= 1;
                assert!(decrypt(&key, &tampered, &ct.nonce).is_err());
            }
        }
    }
}
