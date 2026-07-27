# Refactor Opportunities

_Last verified 2026-07-25 on `main`, after the passphrase / bulk-action / custom-field pass. Supersedes the 2026-05-28/29 autopilot reports, whose findings are all resolved._

## 1. Repository Overview

A local-only desktop password manager built with **Tauri 2** (Rust backend) and **Angular 21** (TypeScript frontend, standalone components + signals).

- `src-tauri/src/` - Rust backend (~10,500 lines including tests). Commands under `commands/` (vault, entries, history, categories, generator, settings, clipboard, health, transfer), with `crypto/` (Argon2id KDF, AES-256-GCM AEAD, RFC 6238 TOTP), `db/` (rusqlite + bundled SQLite, a baseline `schema.sql` plus a numbered migration list, currently at v7), `state.rs` (mutex-guarded `AppState` with `with_state` / `with_authorized` / `with_unlocked` gates), and `error.rs` (one `AppError` enum serialized to a `{kind, message}` wire shape).
- `src/app/` - Angular frontend (~7,200 lines including stylesheets). `core/` holds models, one service per backend domain plus `tauri-invoke` / `confirm` / `clipboard` / `auto-lock`, and the `unlockedGuard`. `features/` holds 14 standalone components across 13 directories. The shared design system lives in the global `src/styles.css` (CSS variables plus the `.btn` family).

## 2. Current Quality

- **Tests:** 274 Rust unit tests, 139 Vitest tests, all green. `cargo clippy --all-targets -D warnings` is clean and `ng build` produces no warnings.
- **No** `TODO` / `FIXME` / `HACK` markers, **no** stray debug output, **no** `#[allow(...)]` suppressions, **no** dead imports.
- **Consistent patterns:** command `_impl` functions behind thin `#[tauri::command]` shims; services follow a uniform `call<T>(…)` + signal shape; components follow a uniform `busy` / `errorMsg` / `formatBackendError` shape; all 14 components use external `templateUrl` / `styleUrl` (no inline templates remain).

The codebase is tidy. The strength-meter banding became a single source of truth this pass (`scoreForBits`), so the exact entropy the passphrase generator reports and the estimate the meter computes are described with the same words.
Beyond the standing test-fixture cleanup, one new duplication is worth noting below.

## 3. Open Opportunities

### Opportunity 1 - Consolidate duplicated Rust test fixtures

- **Location:** `commands/{entries,categories,settings,history}.rs`, `state.rs`, `crypto/aead.rs`
- **Problem:** `locked_state` / `unlocked_state` / `fixed_key` are re-declared per test module. They differ slightly on purpose (`entries.rs` seeds a non-zero key; others use `[0u8; 32]`), which is part of why this has not been unified.
- **Suggested refactor:** A shared `#[cfg(test)]` support module exposing both key variants.
- **Risk level:** Low, but multi-file test-only churn for modest gain.
- **Status:** Open, low priority. Self-contained test modules remain a defensible status quo.

### Opportunity 2 - Bounds are declared in Rust and re-declared in TypeScript

- **Location:** `commands/entries.rs` (`MAX_EXPIRY_DAYS`, `MAX_FIELDS_PER_ENTRY`, `MAX_FIELD_LABEL_CHARS`), `commands/generator.rs` (`MIN_WORDS` / `MAX_WORDS` / `MAX_SEPARATOR_CHARS`) against `core/models/generator.model.ts` and `core/services/password-entry.service.ts`.
- **Problem:** the settings bounds are published by the backend (`get_settings_bounds`) precisely so they are not written twice - but the entry, generator, and custom-field bounds still are, as constants and as `maxlength` / `min` / `max` attributes.
- **Suggested refactor:** extend `get_settings_bounds` into a general `get_bounds`, or add a second command, and drive the templates from it the way the settings form already is.
- **Risk level:** Low. The failure mode is mild (a form that refuses a value the backend accepts, or offers one it refuses) and the backend is authoritative either way.
- **Status:** Open. Worth doing before a fourth set of bounds is added.

## 4. Resolved

- **Auto-lock, clipboard, and history bounds were declared in four places** - the backend now publishes them (2026-07-25).
  A `get_settings_bounds` command returns `{min, max, default}` per setting.
  `SettingsService.bounds` feeds both the form's `min` / `max` attributes and `validateSettingsForm`, leaving Rust the single source of truth.
  A Rust test drives each published edge through the real validator rather than comparing the struct to the constants it was built from.
- **Export payload types held plaintext in unzeroized `String`s** - replaced with a `SecretString` newtype (2026-07-25).
  It wraps `Zeroizing<String>`, serializes as a plain JSON string so the export format is unchanged, and redacts itself in `Debug`.
  A test pins the wire compatibility in both directions.

- **`.btn.danger` duplicated across component stylesheets** - promoted to global `src/styles.css` (2026-07-24). Three components shared an identical outline variant; the confirm dialog keeps a local filled override, since there the destructive action is the primary one, and component-scoped styles out-specify the global rule. Verified by comparing computed styles at all four sites.
- **`get_entry_impl` swallowed DB errors** - now uses `OptionalExtension::optional()?`, so a genuine failure surfaces as `Database` instead of a false `EntryNotFound`. The same fix was later applied to `vault_status_impl`, where swallowing an error had shown the user a create-vault screen over an unreadable vault.
- **Unique-name constraint mapping duplicated** in `create_category_impl` / `update_category_impl` - extracted to `map_category_write_error`.
- **clippy `type_complexity` in `encrypt_optional`** - named `OptionalCiphertext` alias.
- **Dead `last_used_at` read in `get_entry_impl`** - removed.
- **clippy `io_other_error`** in `error.rs` test code - fixed.
- **Inline component templates and styles** - all 12 components now use `templateUrl` / `styleUrl`.

## 5. Deliberately Not Changed

- **Custom field labels are stored in the clear.** Values are encrypted like passwords, but labels sit next to `title`, `username`, and `url_or_app_name`, which this vault has always stored unencrypted. Encrypting only the labels would be inconsistent and would still leave those three readable.
- **The bundled wordlist contains some obscure words.** It is drawn from an unabridged dictionary, which costs memorability but not strength - entropy comes from the size of the list, not from how familiar a word is. A curated list would be better and is a bigger job than it looks.

- **`last_used_at` is written on every `get_entry`.** It records "viewed", not "used". Rather than change the write, the UI now says "Last viewed" and the sort option is labelled "Recently viewed". Making it mean "used" would need a separate signal from the copy action.
- **CSV import stays lenient.** Bad rows are skipped rather than failing the file, which is the point of the format. The one invariant it now enforces is clipping over-long folder names, because an over-long category imported fine and then could not be renamed.
- **The frontend strength meter and the backend health audit use different definitions of "weak".** The meter is an entropy estimate for live feedback; the audit is a conservative length-plus-character-class policy. They answer different questions and are intentionally not unified.
