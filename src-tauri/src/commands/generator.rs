use rand::{rngs::OsRng, Rng};
use serde::Deserialize;

use crate::error::AppError;

const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const NUMBERS: &str = "0123456789";
const SYMBOLS: &str = "!@#$%^&*()_-+=[]{}<>?/.,;:";
const AMBIGUOUS: &[char] = &['0', 'O', 'o', 'l', '1', 'I', '|', '`', '\''];

const MIN_LEN: usize = 4;
const MAX_LEN: usize = 256;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratorOptions {
    pub length: usize,
    pub include_lowercase: bool,
    pub include_uppercase: bool,
    pub include_numbers: bool,
    pub include_symbols: bool,
    pub exclude_ambiguous: bool,
}

pub fn build_pool(opts: &GeneratorOptions) -> Result<Vec<char>, AppError> {
    let mut pool: Vec<char> = Vec::new();
    if opts.include_lowercase {
        pool.extend(LOWER.chars());
    }
    if opts.include_uppercase {
        pool.extend(UPPER.chars());
    }
    if opts.include_numbers {
        pool.extend(NUMBERS.chars());
    }
    if opts.include_symbols {
        pool.extend(SYMBOLS.chars());
    }
    if opts.exclude_ambiguous {
        pool.retain(|c| !AMBIGUOUS.contains(c));
    }
    if pool.is_empty() {
        return Err(AppError::Validation(
            "select at least one character class",
        ));
    }
    Ok(pool)
}

/// One pool per selected class, with ambiguous characters already removed.
/// A class emptied by the ambiguous filter is dropped rather than required.
fn class_pools(opts: &GeneratorOptions) -> Vec<Vec<char>> {
    let selected: [(bool, &str); 4] = [
        (opts.include_lowercase, LOWER),
        (opts.include_uppercase, UPPER),
        (opts.include_numbers, NUMBERS),
        (opts.include_symbols, SYMBOLS),
    ];
    selected
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, chars)| {
            let mut pool: Vec<char> = chars.chars().collect();
            if opts.exclude_ambiguous {
                pool.retain(|c| !AMBIGUOUS.contains(c));
            }
            pool
        })
        .filter(|p| !p.is_empty())
        .collect()
}

pub fn generate(opts: &GeneratorOptions) -> Result<String, AppError> {
    if opts.length < MIN_LEN || opts.length > MAX_LEN {
        return Err(AppError::Validation("length must be between 4 and 256"));
    }
    let pool = build_pool(opts)?;
    let required = class_pools(opts);
    let mut rng = OsRng;

    // Rejection-sample so every selected class appears at least once while the
    // distribution stays uniform over the accepted set. MIN_LEN (4) >= the
    // number of classes, so a satisfying password always exists; the attempt
    // cap is a safety net, not an expected path.
    for _ in 0..10_000 {
        let mut out = String::with_capacity(opts.length);
        for _ in 0..opts.length {
            let idx = rng.gen_range(0..pool.len());
            out.push(pool[idx]);
        }
        if required
            .iter()
            .all(|p| out.chars().any(|c| p.contains(&c)))
        {
            return Ok(out);
        }
    }
    Err(AppError::Internal(
        "could not generate a password satisfying all character classes".into(),
    ))
}

#[tauri::command]
pub fn generate_password(options: GeneratorOptions) -> Result<String, AppError> {
    generate(&options)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_opts() -> GeneratorOptions {
        GeneratorOptions {
            length: 24,
            include_lowercase: true,
            include_uppercase: true,
            include_numbers: true,
            include_symbols: true,
            exclude_ambiguous: false,
        }
    }

    #[test]
    fn returns_password_of_requested_length() {
        for len in [4usize, 8, 16, 24, 64, 128] {
            let opts = GeneratorOptions {
                length: len,
                ..default_opts()
            };
            let pw = generate(&opts).unwrap();
            assert_eq!(pw.chars().count(), len);
        }
    }

    #[test]
    fn rejects_length_outside_range() {
        let opts = GeneratorOptions {
            length: 3,
            ..default_opts()
        };
        assert!(generate(&opts).is_err());
        let opts = GeneratorOptions {
            length: 257,
            ..default_opts()
        };
        assert!(generate(&opts).is_err());
    }

    #[test]
    fn accepts_length_at_both_boundaries() {
        // Symmetric with `rejects_length_outside_range`, which pins MIN_LEN - 1
        // (3) and MAX_LEN + 1 (257) as rejected. This pins the inclusive
        // endpoints MIN_LEN (4) and MAX_LEN (256) as accepted: the guard is
        // `length < MIN_LEN || length > MAX_LEN`, so a regression to `>= MAX_LEN`
        // would reject the documented maximum while every other test stayed
        // green (the existing length loop only goes up to 128).
        for len in [MIN_LEN, MAX_LEN] {
            let opts = GeneratorOptions {
                length: len,
                ..default_opts()
            };
            let pw = generate(&opts).unwrap();
            assert_eq!(pw.chars().count(), len);
        }
    }

    #[test]
    fn rejects_when_no_classes_selected() {
        let opts = GeneratorOptions {
            length: 16,
            include_lowercase: false,
            include_uppercase: false,
            include_numbers: false,
            include_symbols: false,
            exclude_ambiguous: false,
        };
        assert!(generate(&opts).is_err());
    }

    #[test]
    fn only_includes_selected_classes() {
        let opts = GeneratorOptions {
            length: 64,
            include_lowercase: true,
            include_uppercase: false,
            include_numbers: false,
            include_symbols: false,
            exclude_ambiguous: false,
        };
        let pw = generate(&opts).unwrap();
        assert!(pw.chars().all(|c| c.is_ascii_lowercase()));
    }

    #[test]
    fn exclude_ambiguous_removes_ambiguous_chars() {
        let opts = GeneratorOptions {
            length: 200,
            include_lowercase: true,
            include_uppercase: true,
            include_numbers: true,
            include_symbols: true,
            exclude_ambiguous: true,
        };
        let pw = generate(&opts).unwrap();
        for c in pw.chars() {
            assert!(!AMBIGUOUS.contains(&c), "found ambiguous char {c:?}");
        }
    }

    #[test]
    fn every_selected_class_appears_at_least_once() {
        // Repeat at the minimum length, where random omission of a class is
        // by far the most likely; without the guarantee this fails almost
        // always within 50 iterations.
        for _ in 0..50 {
            let opts = GeneratorOptions {
                length: 4,
                ..default_opts()
            };
            let pw = generate(&opts).unwrap();
            assert!(pw.chars().any(|c| c.is_ascii_lowercase()), "no lowercase in {pw:?}");
            assert!(pw.chars().any(|c| c.is_ascii_uppercase()), "no uppercase in {pw:?}");
            assert!(pw.chars().any(|c| c.is_ascii_digit()), "no digit in {pw:?}");
            assert!(
                pw.chars().any(|c| SYMBOLS.contains(c)),
                "no symbol in {pw:?}"
            );
        }
    }

    #[test]
    fn class_guarantee_respects_exclude_ambiguous() {
        for _ in 0..20 {
            let opts = GeneratorOptions {
                length: 4,
                exclude_ambiguous: true,
                ..default_opts()
            };
            let pw = generate(&opts).unwrap();
            assert!(pw.chars().all(|c| !AMBIGUOUS.contains(&c)));
            assert!(pw.chars().any(|c| c.is_ascii_digit()), "no digit in {pw:?}");
        }
    }

    #[test]
    fn two_calls_yield_different_passwords() {
        let opts = default_opts();
        let a = generate(&opts).unwrap();
        let b = generate(&opts).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn pool_size_with_all_classes_no_ambiguous() {
        let opts = GeneratorOptions {
            length: 24,
            include_lowercase: true,
            include_uppercase: true,
            include_numbers: true,
            include_symbols: true,
            exclude_ambiguous: true,
        };
        let pool = build_pool(&opts).unwrap();
        for c in AMBIGUOUS {
            assert!(!pool.contains(c));
        }
    }
}
