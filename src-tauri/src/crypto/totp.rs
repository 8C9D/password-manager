//! RFC 6238 time-based one-time passwords (TOTP), plus the RFC 4648 base32
//! decoding and `otpauth://` URI parsing needed to ingest secrets from
//! authenticator app exports.
//!
//! The secret is the sensitive part and is only ever persisted encrypted at
//! rest (see `commands::entries`). This module is pure and side-effect free so
//! it can be exhaustively tested against the published test vectors.

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Sha256, Sha512};

use crate::error::AppError;

pub const DEFAULT_DIGITS: u32 = 6;
pub const DEFAULT_PERIOD: u64 = 30;
const MIN_DIGITS: u32 = 6;
const MAX_DIGITS: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TotpAlgorithm {
    #[serde(rename = "SHA1")]
    Sha1,
    #[serde(rename = "SHA256")]
    Sha256,
    #[serde(rename = "SHA512")]
    Sha512,
}

/// A canonical, storable TOTP configuration. The secret is kept base32-encoded
/// (uppercase, unpadded) so the serialized form is compact and text-safe; the
/// serialized bytes are only ever written to the database encrypted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotpConfig {
    pub secret_base32: String,
    pub algorithm: TotpAlgorithm,
    pub digits: u32,
    pub period: u64,
}

impl TotpConfig {
    /// Enforce the invariants every stored config must hold. Deserialized
    /// configs (e.g. from an import file) bypass `build_config`, and an
    /// out-of-range `digits` would make `hotp`'s `10u32.pow(digits)` overflow.
    pub fn validate(&self) -> Result<(), AppError> {
        let decoded = base32_decode(&self.secret_base32)
            .ok_or(AppError::Validation("invalid TOTP secret"))?;
        if decoded.is_empty() {
            return Err(AppError::Validation("TOTP secret is empty"));
        }
        if !(MIN_DIGITS..=MAX_DIGITS).contains(&self.digits) {
            return Err(AppError::Validation("TOTP digits must be between 6 and 8"));
        }
        if self.period == 0 {
            return Err(AppError::Validation("TOTP period must be positive"));
        }
        Ok(())
    }
}

/// A generated code together with the time metadata a live countdown needs.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedTotp {
    pub code: String,
    pub period: u64,
    pub seconds_remaining: u64,
}

/// Decode an RFC 4648 base32 string. Whitespace, `=` padding and `-` grouping
/// separators are ignored, and lowercase is accepted. Returns `None` on any
/// character outside the base32 alphabet.
pub fn base32_decode(input: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut buffer: u64 = 0;
    let mut bits: u32 = 0;
    let mut out = Vec::new();
    for c in input.chars() {
        if c.is_whitespace() || c == '=' || c == '-' {
            continue;
        }
        let up = c.to_ascii_uppercase() as u8;
        let val = ALPHABET.iter().position(|&x| x == up)? as u64;
        buffer = (buffer << 5) | val;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

fn hmac_sign(algorithm: TotpAlgorithm, key: &[u8], msg: &[u8]) -> Vec<u8> {
    match algorithm {
        TotpAlgorithm::Sha1 => {
            let mut mac = Hmac::<Sha1>::new_from_slice(key).expect("HMAC accepts any key length");
            mac.update(msg);
            mac.finalize().into_bytes().to_vec()
        }
        TotpAlgorithm::Sha256 => {
            let mut mac =
                Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
            mac.update(msg);
            mac.finalize().into_bytes().to_vec()
        }
        TotpAlgorithm::Sha512 => {
            let mut mac =
                Hmac::<Sha512>::new_from_slice(key).expect("HMAC accepts any key length");
            mac.update(msg);
            mac.finalize().into_bytes().to_vec()
        }
    }
}

/// RFC 4226 HOTP: the dynamically-truncated, zero-padded decimal code for a
/// given counter.
fn hotp(algorithm: TotpAlgorithm, secret: &[u8], counter: u64, digits: u32) -> String {
    let hs = hmac_sign(algorithm, secret, &counter.to_be_bytes());
    // Dynamic truncation: the low nibble of the last byte picks a 4-byte window.
    let offset = (hs[hs.len() - 1] & 0x0f) as usize;
    let bin = (u32::from(hs[offset] & 0x7f) << 24)
        | (u32::from(hs[offset + 1]) << 16)
        | (u32::from(hs[offset + 2]) << 8)
        | u32::from(hs[offset + 3]);
    let modulo = 10u32.pow(digits);
    format!("{:0width$}", bin % modulo, width = digits as usize)
}

/// Generate the TOTP code for the given wall-clock time (Unix seconds).
pub fn generate(config: &TotpConfig, unix_seconds: u64) -> Result<GeneratedTotp, AppError> {
    // Re-validate here too: rows stored before validation existed (or by a
    // buggy path) must fail cleanly instead of overflowing in `hotp`.
    config.validate()?;
    let secret = base32_decode(&config.secret_base32)
        .filter(|s| !s.is_empty())
        .ok_or(AppError::Validation("invalid TOTP secret"))?;
    let counter = unix_seconds / config.period;
    let code = hotp(config.algorithm, &secret, counter, config.digits);
    let seconds_remaining = config.period - (unix_seconds % config.period);
    Ok(GeneratedTotp {
        code,
        period: config.period,
        seconds_remaining,
    })
}

fn normalize_secret(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_whitespace() && *c != '=' && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

fn parse_algorithm(value: &str) -> Result<TotpAlgorithm, AppError> {
    match value.to_ascii_uppercase().as_str() {
        "SHA1" => Ok(TotpAlgorithm::Sha1),
        "SHA256" => Ok(TotpAlgorithm::Sha256),
        "SHA512" => Ok(TotpAlgorithm::Sha512),
        _ => Err(AppError::Validation("unsupported TOTP algorithm")),
    }
}

fn build_config(
    secret_raw: &str,
    algorithm: TotpAlgorithm,
    digits: u32,
    period: u64,
) -> Result<TotpConfig, AppError> {
    let config = TotpConfig {
        secret_base32: normalize_secret(secret_raw),
        algorithm,
        digits,
        period,
    };
    config.validate()?;
    Ok(config)
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push((h * 16 + l) as char);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn parse_otpauth(uri: &str) -> Result<TotpConfig, AppError> {
    let rest = uri
        .strip_prefix("otpauth://")
        .or_else(|| uri.strip_prefix("OTPAUTH://"))
        .ok_or(AppError::Validation("not an otpauth URI"))?;
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let kind = path.split('/').next().unwrap_or("");
    if !kind.eq_ignore_ascii_case("totp") {
        return Err(AppError::Validation("only TOTP otpauth URIs are supported"));
    }

    let mut secret: Option<String> = None;
    let mut algorithm = TotpAlgorithm::Sha1;
    let mut digits = DEFAULT_DIGITS;
    let mut period = DEFAULT_PERIOD;
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let value = percent_decode(value);
        match key.to_ascii_lowercase().as_str() {
            "secret" => secret = Some(value),
            "algorithm" => algorithm = parse_algorithm(&value)?,
            "digits" => {
                digits = value
                    .parse()
                    .map_err(|_| AppError::Validation("invalid TOTP digits"))?
            }
            "period" => {
                period = value
                    .parse()
                    .map_err(|_| AppError::Validation("invalid TOTP period"))?
            }
            _ => {}
        }
    }
    let secret = secret.ok_or(AppError::Validation("otpauth URI has no secret"))?;
    build_config(&secret, algorithm, digits, period)
}

/// Parse user input into a storable config. Accepts either a full
/// `otpauth://totp/...` URI (as exported by authenticator apps) or a bare
/// base32 secret, in which case RFC-default parameters (SHA1, 6 digits, 30s)
/// are used.
pub fn parse_totp_input(input: &str) -> Result<TotpConfig, AppError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("TOTP secret is empty"));
    }
    let is_uri = trimmed.len() >= 10 && trimmed.as_bytes()[..10].eq_ignore_ascii_case(b"otpauth://");
    if is_uri {
        parse_otpauth(trimmed)
    } else {
        build_config(trimmed, TotpAlgorithm::Sha1, DEFAULT_DIGITS, DEFAULT_PERIOD)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // RFC 4648 base32 test vectors.
    #[test]
    fn base32_decodes_rfc4648_vectors() {
        assert_eq!(base32_decode("").unwrap(), b"");
        assert_eq!(base32_decode("MY======").unwrap(), b"f");
        assert_eq!(base32_decode("MZXQ====").unwrap(), b"fo");
        assert_eq!(base32_decode("MZXW6===").unwrap(), b"foo");
        assert_eq!(base32_decode("MZXW6YQ=").unwrap(), b"foob");
        assert_eq!(base32_decode("MZXW6YTB").unwrap(), b"fooba");
        assert_eq!(base32_decode("MZXW6YTBOI======").unwrap(), b"foobar");
    }

    #[test]
    fn base32_is_lenient_about_padding_case_and_separators() {
        assert_eq!(base32_decode("mzxw6ytboi").unwrap(), b"foobar");
        assert_eq!(base32_decode("MZXW 6YTB OI").unwrap(), b"foobar");
        assert_eq!(base32_decode("MZXW-6YTB-OI").unwrap(), b"foobar");
    }

    #[test]
    fn base32_rejects_out_of_alphabet_characters() {
        assert!(base32_decode("MZXW0189").is_none()); // 0, 1, 8, 9 are not base32
    }

    fn config(secret_base32: &str, algorithm: TotpAlgorithm, digits: u32) -> TotpConfig {
        TotpConfig {
            secret_base32: secret_base32.to_string(),
            algorithm,
            digits,
            period: 30,
        }
    }

    // Base32 of the RFC 6238 Appendix B seeds.
    // SHA1 seed = ASCII "12345678901234567890" (20 bytes).
    const SHA1_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";
    // SHA256 seed = ASCII "12345678901234567890123456789012" (32 bytes).
    const SHA256_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZA";
    // SHA512 seed = ASCII "1234567890" x6 + "1234" (64 bytes).
    const SHA512_SECRET: &str =
        "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQGEZDGNA";

    #[test]
    fn generate_matches_rfc6238_sha1_vectors_8_digits() {
        let c = config(SHA1_SECRET, TotpAlgorithm::Sha1, 8);
        assert_eq!(generate(&c, 59).unwrap().code, "94287082");
        assert_eq!(generate(&c, 1111111109).unwrap().code, "07081804");
        assert_eq!(generate(&c, 1234567890).unwrap().code, "89005924");
        assert_eq!(generate(&c, 2000000000).unwrap().code, "69279037");
        assert_eq!(generate(&c, 20000000000).unwrap().code, "65353130");
    }

    #[test]
    fn generate_matches_rfc6238_sha256_and_sha512_vectors() {
        let c256 = config(SHA256_SECRET, TotpAlgorithm::Sha256, 8);
        assert_eq!(generate(&c256, 59).unwrap().code, "46119246");
        let c512 = config(SHA512_SECRET, TotpAlgorithm::Sha512, 8);
        assert_eq!(generate(&c512, 59).unwrap().code, "90693936");
    }

    #[test]
    fn generate_rejects_out_of_range_digits() {
        // A deserialized config (import file) bypasses build_config; without
        // validation 10u32.pow(12) overflows inside hotp.
        for digits in [0u32, 5, 9, 12] {
            let c = config(SHA1_SECRET, TotpAlgorithm::Sha1, digits);
            assert!(
                matches!(generate(&c, 59), Err(AppError::Validation(_))),
                "digits {digits} must be rejected"
            );
        }
    }

    #[test]
    fn generate_reports_seconds_remaining_in_period() {
        let c = config(SHA1_SECRET, TotpAlgorithm::Sha1, 6);
        // 30s period: at t=0 a fresh window has the full 30s left.
        assert_eq!(generate(&c, 0).unwrap().seconds_remaining, 30);
        assert_eq!(generate(&c, 1).unwrap().seconds_remaining, 29);
        assert_eq!(generate(&c, 29).unwrap().seconds_remaining, 1);
        assert_eq!(generate(&c, 30).unwrap().seconds_remaining, 30);
    }

    #[test]
    fn generate_rejects_a_zero_period() {
        let mut c = config(SHA1_SECRET, TotpAlgorithm::Sha1, 6);
        c.period = 0;
        assert!(matches!(generate(&c, 0), Err(AppError::Validation(_))));
    }

    #[test]
    fn parse_bare_secret_uses_rfc_defaults() {
        let c = parse_totp_input("JBSWY3DPEHPK3PXP").unwrap();
        assert_eq!(c.secret_base32, "JBSWY3DPEHPK3PXP");
        assert_eq!(c.algorithm, TotpAlgorithm::Sha1);
        assert_eq!(c.digits, DEFAULT_DIGITS);
        assert_eq!(c.period, DEFAULT_PERIOD);
    }

    #[test]
    fn parse_normalizes_spacing_and_case() {
        let c = parse_totp_input("jbsw y3dp ehpk 3pxp").unwrap();
        assert_eq!(c.secret_base32, "JBSWY3DPEHPK3PXP");
    }

    #[test]
    fn parse_full_otpauth_uri_honors_parameters() {
        let c = parse_totp_input(
            "otpauth://totp/ACME:alice@acme.com?secret=JBSWY3DPEHPK3PXP&issuer=ACME&algorithm=SHA256&digits=8&period=60",
        )
        .unwrap();
        assert_eq!(c.secret_base32, "JBSWY3DPEHPK3PXP");
        assert_eq!(c.algorithm, TotpAlgorithm::Sha256);
        assert_eq!(c.digits, 8);
        assert_eq!(c.period, 60);
    }

    #[test]
    fn parse_otpauth_uri_defaults_missing_parameters() {
        let c = parse_totp_input("otpauth://totp/Label?secret=JBSWY3DPEHPK3PXP").unwrap();
        assert_eq!(c.algorithm, TotpAlgorithm::Sha1);
        assert_eq!(c.digits, DEFAULT_DIGITS);
        assert_eq!(c.period, DEFAULT_PERIOD);
    }

    #[test]
    fn parse_rejects_empty_blank_and_invalid_input() {
        assert!(matches!(parse_totp_input(""), Err(AppError::Validation(_))));
        assert!(matches!(parse_totp_input("   "), Err(AppError::Validation(_))));
        // Not valid base32.
        assert!(matches!(parse_totp_input("not base32!!"), Err(AppError::Validation(_))));
    }

    #[test]
    fn parse_rejects_non_totp_uri_and_missing_secret() {
        assert!(matches!(
            parse_totp_input("otpauth://hotp/Label?secret=JBSWY3DPEHPK3PXP"),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            parse_totp_input("otpauth://totp/Label?issuer=ACME"),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn parse_rejects_out_of_range_digits() {
        assert!(matches!(
            parse_totp_input("otpauth://totp/L?secret=JBSWY3DPEHPK3PXP&digits=9"),
            Err(AppError::Validation(_))
        ));
        assert!(matches!(
            parse_totp_input("otpauth://totp/L?secret=JBSWY3DPEHPK3PXP&digits=4"),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn config_survives_a_json_round_trip() {
        let c = parse_totp_input(
            "otpauth://totp/L?secret=JBSWY3DPEHPK3PXP&algorithm=SHA512&digits=7&period=45",
        )
        .unwrap();
        let json = serde_json::to_vec(&c).unwrap();
        let back: TotpConfig = serde_json::from_slice(&json).unwrap();
        assert_eq!(back.secret_base32, c.secret_base32);
        assert_eq!(back.algorithm, TotpAlgorithm::Sha512);
        assert_eq!(back.digits, 7);
        assert_eq!(back.period, 45);
    }
}
