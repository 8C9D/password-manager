use rand::{rngs::OsRng, Rng};
use serde::{Deserialize, Serialize};

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

// --- Passphrase generation ---

/// Bundled wordlist, 4096 entries so each word contributes exactly 12 bits.
///
/// Built from the system dictionary, restricted to 4-6 lowercase ASCII letters
/// and de-duplicated against simple inflections. It leans on an unabridged
/// dictionary, so a few entries are obscure; that costs memorability but not
/// strength, since the entropy comes from the size of the list rather than from
/// how familiar any particular word is.
const WORDLIST: &str = include_str!("wordlist.txt");

pub(crate) const MIN_WORDS: usize = 3;
pub(crate) const MAX_WORDS: usize = 12;
/// Long enough for " - " style separators, short enough that the field cannot
/// become a second password.
pub(crate) const MAX_SEPARATOR_CHARS: usize = 4;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PassphraseOptions {
    pub word_count: usize,
    /// Text placed between words. Empty is allowed (one run-on phrase).
    pub separator: String,
    pub capitalize: bool,
    /// Append a digit to one randomly chosen word, for sites that demand one.
    pub include_number: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedPassphrase {
    pub passphrase: String,
    /// Exact entropy of the choices made, not an estimate from the text. A
    /// character-based strength model reads a passphrase as a long lowercase
    /// string and badly misjudges it in both directions.
    pub entropy_bits: f64,
}

fn words() -> Vec<&'static str> {
    WORDLIST.lines().filter(|w| !w.is_empty()).collect()
}

pub(crate) fn build_passphrase(
    opts: &PassphraseOptions,
) -> Result<GeneratedPassphrase, AppError> {
    if opts.word_count < MIN_WORDS || opts.word_count > MAX_WORDS {
        return Err(AppError::Validation("passphrase must be 3 to 12 words"));
    }
    if opts.separator.chars().count() > MAX_SEPARATOR_CHARS {
        return Err(AppError::Validation(
            "separator must be 4 characters or fewer",
        ));
    }
    let pool = words();
    if pool.is_empty() {
        return Err(AppError::Internal("wordlist is empty".into()));
    }
    let mut rng = OsRng;

    let mut chosen: Vec<String> = (0..opts.word_count)
        .map(|_| {
            let w = pool[rng.gen_range(0..pool.len())];
            if opts.capitalize {
                let mut c = w.chars();
                match c.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            } else {
                w.to_string()
            }
        })
        .collect();

    // log2(pool)^word_count, computed as word_count * log2(pool).
    let mut entropy_bits = opts.word_count as f64 * (pool.len() as f64).log2();

    if opts.include_number {
        let target = rng.gen_range(0..chosen.len());
        let digit = rng.gen_range(0..10u32);
        chosen[target].push_str(&digit.to_string());
        // Both the digit and which word carries it are random choices.
        entropy_bits += 10f64.log2() + (chosen.len() as f64).log2();
    }

    Ok(GeneratedPassphrase {
        passphrase: chosen.join(&opts.separator),
        entropy_bits,
    })
}

#[tauri::command]
pub fn generate_passphrase(
    options: PassphraseOptions,
) -> Result<GeneratedPassphrase, AppError> {
    build_passphrase(&options)
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

    // --- Passphrase ---

    fn default_phrase_opts() -> PassphraseOptions {
        PassphraseOptions {
            word_count: 5,
            separator: "-".into(),
            capitalize: false,
            include_number: false,
        }
    }

    #[test]
    fn the_wordlist_is_the_size_its_entropy_claim_depends_on() {
        // Every word is asserted to be 12 bits, which is only true at 4096
        // entries; a duplicate or a stray blank line would silently overstate
        // the strength shown to the user.
        let pool = words();
        assert_eq!(pool.len(), 4096);
        let unique: std::collections::HashSet<&str> = pool.iter().copied().collect();
        assert_eq!(unique.len(), pool.len(), "wordlist has duplicates");
        for w in &pool {
            assert!(
                w.chars().all(|c| c.is_ascii_lowercase()),
                "unexpected characters in {w:?}"
            );
            assert!((4..=6).contains(&w.len()), "unexpected length in {w:?}");
        }
    }

    #[test]
    fn passphrase_has_the_requested_number_of_words() {
        for count in [MIN_WORDS, 5, MAX_WORDS] {
            let opts = PassphraseOptions {
                word_count: count,
                ..default_phrase_opts()
            };
            let out = build_passphrase(&opts).unwrap();
            assert_eq!(out.passphrase.split('-').count(), count);
        }
    }

    #[test]
    fn word_count_outside_the_supported_range_is_refused() {
        for count in [0, MIN_WORDS - 1, MAX_WORDS + 1, usize::MAX] {
            let opts = PassphraseOptions {
                word_count: count,
                ..default_phrase_opts()
            };
            assert!(matches!(
                build_passphrase(&opts),
                Err(AppError::Validation(_))
            ));
        }
    }

    #[test]
    fn an_over_long_separator_is_refused() {
        let opts = PassphraseOptions {
            separator: "-----".into(),
            ..default_phrase_opts()
        };
        assert!(matches!(
            build_passphrase(&opts),
            Err(AppError::Validation(_))
        ));
        // Exactly at the limit is fine, counted in characters like every other
        // length bound in this app.
        let ok = PassphraseOptions {
            separator: "😀😀😀😀".into(),
            ..default_phrase_opts()
        };
        assert!(build_passphrase(&ok).is_ok());
    }

    #[test]
    fn an_empty_separator_runs_the_words_together() {
        let opts = PassphraseOptions {
            separator: String::new(),
            ..default_phrase_opts()
        };
        let out = build_passphrase(&opts).unwrap();
        assert!(out.passphrase.chars().all(|c| c.is_ascii_lowercase()));
    }

    #[test]
    fn capitalize_uppercases_the_first_letter_of_every_word() {
        let opts = PassphraseOptions {
            capitalize: true,
            ..default_phrase_opts()
        };
        let out = build_passphrase(&opts).unwrap();
        for word in out.passphrase.split('-') {
            let first = word.chars().next().unwrap();
            assert!(first.is_ascii_uppercase(), "not capitalized: {word:?}");
        }
    }

    #[test]
    fn a_requested_digit_lands_on_exactly_one_word() {
        let opts = PassphraseOptions {
            include_number: true,
            ..default_phrase_opts()
        };
        for _ in 0..20 {
            let out = build_passphrase(&opts).unwrap();
            let with_digit = out
                .passphrase
                .split('-')
                .filter(|w| w.chars().any(|c| c.is_ascii_digit()))
                .count();
            assert_eq!(with_digit, 1, "in {:?}", out.passphrase);
            // The word count is unchanged; the digit rides along on a word.
            assert_eq!(out.passphrase.split('-').count(), 5);
        }
    }

    #[test]
    fn reported_entropy_matches_the_choices_actually_made() {
        // 4096 words is 12 bits each.
        let out = build_passphrase(&default_phrase_opts()).unwrap();
        assert!((out.entropy_bits - 60.0).abs() < 1e-9, "got {}", out.entropy_bits);

        // Capitalizing every word adds no choice, so it must not add bits.
        let capitalized = build_passphrase(&PassphraseOptions {
            capitalize: true,
            ..default_phrase_opts()
        })
        .unwrap();
        assert!((capitalized.entropy_bits - 60.0).abs() < 1e-9);

        // A digit adds its own value plus which word carries it.
        let numbered = build_passphrase(&PassphraseOptions {
            include_number: true,
            ..default_phrase_opts()
        })
        .unwrap();
        let expected = 60.0 + 10f64.log2() + 5f64.log2();
        assert!((numbered.entropy_bits - expected).abs() < 1e-9);
    }

    #[test]
    fn two_calls_yield_different_passphrases() {
        let opts = default_phrase_opts();
        let a = build_passphrase(&opts).unwrap().passphrase;
        let b = build_passphrase(&opts).unwrap().passphrase;
        assert_ne!(a, b);
    }

    #[test]
    fn generated_words_all_come_from_the_bundled_list() {
        let pool: std::collections::HashSet<&str> = words().into_iter().collect();
        let out = build_passphrase(&PassphraseOptions {
            word_count: MAX_WORDS,
            ..default_phrase_opts()
        })
        .unwrap();
        for w in out.passphrase.split('-') {
            assert!(pool.contains(w), "{w:?} is not in the wordlist");
        }
    }
}
