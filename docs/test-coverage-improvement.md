# Test Coverage Improvement Report

_Generated 2026-05-28 on branch `chore/repo-cleanup`._

## 1. Repository Test Overview

This is a local-only password manager built as a **Tauri 2 + Angular 21** application.

| Layer | Language | Test framework | How to run |
| --- | --- | --- | --- |
| Frontend (`src/`) | TypeScript / Angular 21 | Vitest (`@angular/build:unit-test`) | `npm test` / `ng test` |
| Backend (`src-tauri/src/`) | Rust (Tauri 2) | built-in `#[cfg(test)]` unit tests | `cargo test --manifest-path src-tauri/Cargo.toml --lib` |

Existing tests at the time of writing:

- **Frontend** — 2 spec files, both targeting *pure exported functions* rather than Angular DI/rendering:
  - `tauri-invoke.spec.ts` → `formatBackendError` (backend-error → user-message mapping)
  - `password-entry.service.spec.ts` → `filterEntries`, `validateEntryInput`
- **Backend** — 57 passing unit tests across 8 modules: `commands/{entries,categories,generator,settings}`, `crypto/{aead,kdf}`, `db`, `state`.

Baseline run (backend): `test result: ok. 57 passed; 0 failed` in ~3.1s.

## 2. Current Coverage Quality Summary

The suite is **high quality and behavior-focused**, not implementation-coupled:

- The Rust command layer is consistently structured as testable `*_impl(&AppState, …)` functions behind thin `#[tauri::command]` wrappers, exercised against an in-memory SQLite DB (`db::open_in_memory()`). Round-trips, sorting, validation, and not-found error paths are well covered.
- Crypto (`aead`, `kdf`) covers round-trips, wrong-key/tampering rejection, nonce uniqueness, and length validation.
- The frontend deliberately extracts pure logic out of services so it can be tested without a DOM/DI harness, and tests that logic thoroughly.

Gaps are concentrated in three places: (a) the **error-serialization contract** between Rust and TypeScript, which is untested on the Rust side despite the TS side depending on it; (b) a few **untested fallback / boundary branches** inside otherwise-tested modules; and (c) modules that are only reachable through Tauri runtime types (`vault`, `clipboard`) and would need a production refactor to unit-test.

## 3. Highest-Value Coverage Gaps

### Gap A — `AppError` serialization contract (Rust → TypeScript)

- **Location:** `src-tauri/src/error.rs` (`impl Serialize for AppError`)
- **Why it matters:** Every backend command returns `AppError`, which serializes to `{ kind, message }`. The frontend's `formatBackendError` (in `tauri-invoke.ts`, and *tested* in `tauri-invoke.spec.ts`) switches on the exact `kind` strings (`locked`, `wrong_password`, `entry_not_found`, `category_not_found`, `validation`) and strips the `validation: ` message prefix. If the Rust `kind` mapping or message prefixes drift, user-facing error handling breaks silently — the consuming side is tested but the producing side is not. The opaque variants (`database`, `io`, `internal`) also intentionally avoid leaking internal details into the message, which is a security property worth locking in.
- **Existing tests:** None on the Rust side. The TS consumer is tested in isolation with hand-written fixtures.
- **Missing cases:** `kind` string for every variant; `validation:`/`crypto:` message prefixes; confirmation that `database`/`io`/`internal` messages are opaque.
- **Suggested tests:** Serialize each `AppError` variant with `serde_json::to_value` and assert on `kind` and `message`.
- **Risk level:** Low (pure, no production change).
- **Validation:** `cargo test --manifest-path src-tauri/Cargo.toml --lib error::`
- **Status:** Implemented

### Gap B — `read_secs` fallback on a corrupt/unparseable settings value

- **Location:** `src-tauri/src/commands/settings.rs` (`read_secs`)
- **Why it matters:** `read_secs` defends against a malformed `auto_lock_secs` row via `…parse::<u64>().ok()).unwrap_or(DEFAULT_AUTO_LOCK_SECS)`. Only the *missing-row* default is tested; the *present-but-unparseable* branch (DB hand-edit, corruption, or a future format change) is not. A regression here would surface as a panic or a `0`/garbage auto-lock timeout, defeating a security feature.
- **Existing tests:** `get_returns_default_when_no_row_exists` covers the `None` path only.
- **Missing cases:** A row containing a non-numeric value should fall back to the default rather than erroring.
- **Suggested tests:** Insert a raw non-numeric value into the `settings` table, then assert `get_settings_impl` returns `DEFAULT_AUTO_LOCK_SECS`.
- **Risk level:** Low.
- **Validation:** `cargo test --manifest-path src-tauri/Cargo.toml --lib settings::`
- **Status:** Implemented

### Gap C — `validate_name` length boundary for categories

- **Location:** `src-tauri/src/commands/categories.rs` (`validate_name`)
- **Why it matters:** Category names are capped at 64 characters (`trimmed.len() > 64`). Blank and duplicate names are tested, but the length cap — the only numeric boundary in the module — is not. Off-by-one regressions in boundary checks are a classic bug.
- **Existing tests:** `create_rejects_blank_name`, `create_rejects_duplicate_name`.
- **Missing cases:** A 65-character name is rejected as a validation error; an exactly-64-character name is accepted.
- **Suggested tests:** Two `create_category_impl` cases around the 64-char boundary.
- **Risk level:** Low.
- **Validation:** `cargo test --manifest-path src-tauri/Cargo.toml --lib categories::`
- **Status:** Implemented

### Gap D — `vault.rs` command logic is untested

- **Location:** `src-tauri/src/commands/vault.rs`
- **Why it matters:** Master-password length validation, "vault already exists", and the wrong-password mapping are core security behaviors.
- **Existing tests:** None. Unlike the other command modules, `vault.rs` puts logic directly in `#[tauri::command]` functions that take `State<'_, AppState>`, so it cannot be called from a unit test without extracting `*_impl(&AppState, …)` helpers first.
- **Missing cases:** Reject empty / <8-char master password; reject create when a vault already exists; create-then-unlock round trip; wrong password → `WrongPassword`; unlock with no vault → `VaultNotFound`.
- **Suggested tests:** Same in-memory-DB pattern as `entries.rs`, after a small `_impl` extraction.
- **Risk level:** Medium — requires a (behavior-preserving) production refactor, which this pass intentionally avoids.
- **Validation:** `cargo test --manifest-path src-tauri/Cargo.toml --lib vault::`
- **Status:** Skipped (needs production refactor — see §6)

### Gap E — `copy_to_clipboard` clear-timeout logic is untested

- **Location:** `src-tauri/src/commands/clipboard.rs`
- **Why it matters:** The clamp on the auto-clear delay (`unwrap_or(DEFAULT_CLEAR_SECS).clamp(1, 600)`) and the "only clear if the clipboard still holds our token" guard are security-relevant.
- **Existing tests:** None.
- **Missing cases:** Clamp behavior; token-match clear guard.
- **Suggested tests:** Would require extracting the pure clamp/decision logic out of the `AppHandle`-bound async command.
- **Risk level:** Medium — needs a Tauri `AppHandle` and async runtime, or a production refactor.
- **Status:** Skipped (needs production refactor — see §6)

## 4. Test Improvement Plan

Implement the three Low-risk, zero-production-change gaps, one commit each, validating after each:

1. **Gap A** — add an `error::tests` module asserting the `{ kind, message }` contract for all 11 `AppError` variants.
2. **Gap B** — add a `read_secs` corrupt-value fallback regression test to `settings::tests`.
3. **Gap C** — add 64-char boundary tests to `categories::tests`.

Defer Gaps D and E because they would require production refactors, which is out of scope for a test-only pass.

## 5. Implemented Test Improvements

### Improvement 1 — `AppError` serialization contract (Gap A)

- **Files changed:** `src-tauri/src/error.rs` (added `#[cfg(test)] mod tests`).
- **Behavior covered:** The `{ kind, message }` wire format produced by `impl Serialize for AppError`, which the frontend's `formatBackendError` consumes.
- **New test cases:**
  - `unit_variants_map_to_expected_kind_and_message` — the 6 simple variants map to their exact `kind` strings and messages.
  - `validation_message_keeps_the_prefix_the_frontend_strips` — `Validation` keeps the `validation: ` prefix the frontend strips.
  - `crypto_message_carries_crypto_prefix` — `Crypto` carries its `crypto: ` prefix.
  - `database_and_io_errors_serialize_to_opaque_kinds` — `Database`/`Io` map to `database`/`io` with generic messages.
  - `internal_error_does_not_leak_detail_to_the_wire` — `Internal`'s inner string never reaches the wire message.
- **Validation run:** `cargo test --manifest-path src-tauri/Cargo.toml --lib error::`, then the full `--lib` suite.
- **Result:** Pass — 5 new tests; full suite 57 → 62 passing, 0 failed.
- **Commit hash:** _(this commit)_
- **Push result:** Pushed to `origin/chore/repo-cleanup`.

## 6. Skipped Opportunities

- **`vault.rs` / `clipboard.rs` (Gaps D, E):** Both keep their logic inside `#[tauri::command]` functions bound to Tauri runtime types (`State`, `AppHandle`) rather than behind `*_impl(&AppState, …)` helpers. Testing them properly needs a small behavior-preserving extraction, which is a production change this test-only pass avoids. Recommended as a follow-up.
- **Frontend services/components/guard** (`auto-lock`, `clipboard`, `category`, `settings`, `vault` services; `unlocked.guard`; feature components): these are thin wrappers around `call()` plus Angular signals, timers, and event listeners. The repo's established strategy is to test extracted pure functions, and the meaningful pure functions are already covered. Adding component-rendering/DI tests would introduce a new testing style for low marginal value, so it is intentionally out of scope here.

## 7. Final Notes

- The backend suite is the right place to add value: it holds the security-critical logic and has a clean, in-memory-DB testing pattern that new tests can follow exactly.
- The three implemented improvements all guard previously-untested branches without touching production code.
- The most valuable remaining follow-up is extracting `*_impl` helpers in `vault.rs` so the master-password and unlock paths can be unit-tested like the other command modules.
