# Refactor Opportunities

_Last verified 2026-07-24 on `main`. Supersedes the 2026-05-28/29 autopilot reports, whose findings are all resolved._

## 1. Repository Overview

A local-only desktop password manager built with **Tauri 2** (Rust backend) and **Angular 21** (TypeScript frontend, standalone components + signals).

- `src-tauri/src/` - Rust backend (~7,350 lines including tests). Commands under `commands/` (vault, entries, history, categories, generator, settings, clipboard, health, transfer), with `crypto/` (Argon2id KDF, AES-256-GCM AEAD, RFC 6238 TOTP), `db/` (rusqlite + bundled SQLite, a baseline `schema.sql` plus a numbered migration list), `state.rs` (mutex-guarded `AppState` with `with_state` / `with_authorized` / `with_unlocked` gates), and `error.rs` (one `AppError` enum serialized to a `{kind, message}` wire shape).
- `src/app/` - Angular frontend (~4,950 lines). `core/` holds models, one service per backend domain plus `tauri-invoke` / `confirm` / `clipboard` / `auto-lock`, and the `unlockedGuard`. `features/` holds 12 standalone components. The shared design system lives in the global `src/styles.css` (CSS variables plus the `.btn` family).

## 2. Current Quality

- **Tests:** 203 Rust unit tests, 84 Vitest tests, all green. `cargo clippy --all-targets -D warnings` is clean and `ng build` produces no warnings.
- **No** `TODO` / `FIXME` / `HACK` markers, **no** stray debug output, **no** `#[allow(...)]` suppressions, **no** dead imports.
- **Consistent patterns:** command `_impl` functions behind thin `#[tauri::command]` shims; services follow a uniform `call<T>(…)` + signal shape; components follow a uniform `busy` / `errorMsg` / `formatBackendError` shape; all 12 components use external `templateUrl` / `styleUrl` (no inline templates remain).

The codebase is tidy, so the remaining opportunities are small and mostly about single sources of truth.

## 3. Open Opportunities

### Opportunity 1 - Consolidate duplicated Rust test fixtures

- **Location:** `commands/{entries,categories,settings,history}.rs`, `state.rs`, `crypto/aead.rs`
- **Problem:** `locked_state` / `unlocked_state` / `fixed_key` are re-declared per test module. They differ slightly on purpose (`entries.rs` seeds a non-zero key; others use `[0u8; 32]`), which is part of why this has not been unified.
- **Suggested refactor:** A shared `#[cfg(test)]` support module exposing both key variants.
- **Risk level:** Low, but multi-file test-only churn for modest gain.
- **Status:** Open, low priority. Self-contained test modules remain a defensible status quo.

### Opportunity 2 - Auto-lock and clipboard bounds are declared in four places

- **Location:** `settings.component.ts` (validation), `settings.component.html` (`min` / `max` attributes), `settings.service.ts` (defaults), and authoritatively in Rust (`MIN/MAX/DEFAULT_AUTO_LOCK_SECS`, the clipboard constants, and the password-history limit).
- **Problem:** The same numbers appear on both sides of the IPC boundary. Adding the history-retention setting in 2026-07 repeated the pattern a fourth time.
- **Why it matters:** A bound changed in Rust but not in the template silently produces a form that rejects values the backend accepts, or vice versa.
- **Suggested refactor:** Have the backend expose its bounds (a `get_settings_bounds` command, or fields on the existing settings payload) and drive the form from that, leaving Rust the single source of truth.
- **Risk level:** Medium - changes the settings wire shape and the form's validation path.
- **Status:** Open. This is the highest-value item in this document.

### Opportunity 3 - Export payload types hold plaintext in unzeroized `String`s

- **Location:** `commands/transfer.rs` (`ExportEntry`, `ExportHistoryItem`)
- **Problem:** The serialized buffer is wrapped in `Zeroizing`, but the intermediate `ExportPayload` holds every password, note, and retained previous password as plain `String`s that are dropped without wiping. Adding password history to exports multiplied how many secrets sit in that structure.
- **Why it matters:** Freed heap holding vault plaintext after an export widens the window for a memory-scraping attacker. It is a hardening gap, not an active vulnerability: an attacker who can read this process's memory can already read the unlocked key.
- **Suggested refactor:** A `Zeroizing<String>` newtype with `Serialize` / `Deserialize` passthrough, applied to the secret-bearing fields.
- **Risk level:** Medium - touches the serde shape of the export format, so it needs round-trip tests against existing files.
- **Status:** Open.

## 4. Resolved

- **`.btn.danger` duplicated across component stylesheets** - promoted to global `src/styles.css` (2026-07-24). Three components shared an identical outline variant; the confirm dialog keeps a local filled override, since there the destructive action is the primary one, and component-scoped styles out-specify the global rule. Verified by comparing computed styles at all four sites.
- **`get_entry_impl` swallowed DB errors** - now uses `OptionalExtension::optional()?`, so a genuine failure surfaces as `Database` instead of a false `EntryNotFound`. The same fix was later applied to `vault_status_impl`, where swallowing an error had shown the user a create-vault screen over an unreadable vault.
- **Unique-name constraint mapping duplicated** in `create_category_impl` / `update_category_impl` - extracted to `map_category_write_error`.
- **clippy `type_complexity` in `encrypt_optional`** - named `OptionalCiphertext` alias.
- **Dead `last_used_at` read in `get_entry_impl`** - removed.
- **clippy `io_other_error`** in `error.rs` test code - fixed.
- **Inline component templates and styles** - all 12 components now use `templateUrl` / `styleUrl`.

## 5. Deliberately Not Changed

- **`last_used_at` is written on every `get_entry`.** It records "viewed", not "used". Rather than change the write, the UI now says "Last viewed" and the sort option is labelled "Recently viewed". Making it mean "used" would need a separate signal from the copy action.
- **CSV import stays lenient.** Bad rows are skipped rather than failing the file, which is the point of the format. The one invariant it now enforces is clipping over-long folder names, because an over-long category imported fine and then could not be renamed.
- **The frontend strength meter and the backend health audit use different definitions of "weak".** The meter is an entropy estimate for live feedback; the audit is a conservative length-plus-character-class policy. They answer different questions and are intentionally not unified.
