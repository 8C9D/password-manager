# Test Coverage

_Last verified 2026-07-25 on `main`: 274 Rust unit tests and 139 Vitest tests, all green._

## 1. How to Run

| Layer | Language | Framework | Command |
| --- | --- | --- | --- |
| Frontend (`src/`) | TypeScript / Angular 21 | Vitest (`@angular/build:unit-test`) | `npm test -- --watch=false` |
| Backend (`src-tauri/src/`) | Rust (Tauri 2) | built-in `#[cfg(test)]` | `cargo test --manifest-path src-tauri/Cargo.toml --lib` |

Both run in CI on every push, alongside `cargo clippy -D warnings` and `ng build`.

## 2. Where the Coverage Is

### Backend - 274 tests

| Module | Tests | What they cover |
| --- | --- | --- |
| `commands/entries` | 61 | CRUD, TOTP set/keep/clear, tags and favorites, `password_changed_at` semantics, trash/restore/purge, rotation-reminder due dates, custom fields, bulk move/star/trash |
| `commands/transfer` | 50 | Encrypted export/import round trips, CSV import and export, RFC 4180 parsing, atomic file writes, trash exclusion, custom-field round trip, hand-edited-file bounds |
| `commands/vault` | 25 | Create/unlock/lock, master-password change and its re-encryption sweep (trashed entries, history, and custom fields included), unlock backoff including a concurrent-attempt race |
| `commands/settings` | 20 | Persistence, range validation, clamping of hand-edited rows, published bounds matching the real validator |
| `commands/generator` | 20 | Class guarantees, ambiguous exclusion, length bounds, passphrase word count/separator/entropy and the wordlist's size invariant |
| `crypto/totp` | 16 | RFC 6238 vectors, base32 decoding, `otpauth://` parsing |
| `commands/health` | 16 | Weak/reused/stale/due classification, trash exclusion, the save-time reuse count |
| `commands/history` | 13 | Recording on rotation, retention pruning, cascade on delete |
| `commands/categories` | 13 | CRUD, unique-name mapping, character-based length limit |
| `db` | 12 | Migration sequencing, idempotency, rollback, and an upgrade from **every** shipped version to the latest |
| `crypto/aead` | 7 | Round trip, tamper detection, randomized property test |
| `state`, `error` | 12 | Lock gating, error redaction |
| `crypto/kdf`, `commands/clipboard` | 9 | Key derivation, clipboard clear ownership, write/record atomicity |

### Frontend - 139 tests across 13 spec files

| Spec | Tests | What it covers |
| --- | --- | --- |
| `password-entry.service.spec.ts` | 47 | Filtering, sorting, tag parsing, TOTP action resolution, validation, rotation-reminder text, reuse-warning wording |
| `tauri-invoke.spec.ts` | 22 | Backend-error to user-message mapping |
| `password-strength.spec.ts` | 13 | Entropy scoring, and the bit-banding shared with the passphrase generator |
| `vault-layout.component.spec.ts` | 11 | Keyboard-shortcut dispatch, text-entry detection, suppression behind an open modal |
| `entry-list.component.spec.ts` | 10 | Bulk-result wording, and the selection pruning and mode exit driven by the visible set |
| `settings.service.spec.ts` | 7 | Form validation driven by the backend's published bounds |
| `vault.service.spec.ts` | 6 | Create-vault form validity |
| `entry-form.component.spec.ts` | 5 | Route-driven reloads: duplicate prefill, edit-to-edit, and a garbage id |
| `entry-detail.component.spec.ts` | 5 | Stale-response guards for the entry, its TOTP, and its history; custom-field masking |
| `confirm-dialog.component.spec.ts` | 5 | Modal focus-trap wrapping |
| `theme.service.spec.ts` | 3 | Theme preference parsed from untrusted storage |
| `clipboard.service.spec.ts` | 3 | Banner countdown, and dropping copies superseded by a lock or a newer copy |
| `auto-lock.service.spec.ts` | 2 | Which visibility transitions count as activity |

## 3. The Testing Approach

Two deliberate patterns make this suite cheap to write and hard to couple to implementation details.

**Backend: `_impl` functions behind thin command shims.**
Every command is a `#[tauri::command]` wrapper over a `fn *_impl(&AppState, …)`, so tests drive real logic against an in-memory SQLite database with no Tauri runtime.
Time-dependent code takes the clock as a parameter (`generate_totp_at`, `is_stale`) so behavior is asserted against fixed instants rather than sleeps.

**Frontend: pure functions extracted out of components and services.**
Logic worth testing is exported as a standalone function (`filterEntries`, `sortEntries`, `shortcutFor`, `countsAsActivity`, `canCreateVault`, `describeBulkResult`) and tested without a DOM or DI harness.
Components keep only the wiring.

**Frontend: TestBed for the components with real async state machines.**
`@angular/build:unit-test` generates the TestBed setup itself, so `TestBed.createComponent` works with no extra configuration.
`entry-detail` and `entry-form` are driven this way against service doubles whose promises the test resolves by hand, which is the only reliable way to hold a request open across a navigation - the stale-response guards misbehave only while one is in flight.
`entry-form` uses `RouterTestingHarness` with `provideRouter`, because the defect worth testing there is what Angular does when it *reuses* the component across a navigation.

## 4. The Encrypted-Column Ripple Rule

Any new encrypt-at-rest column or table must be wired through **every** path, or it silently loses data.
The paths are:

1. create / update / read in `commands/entries.rs`
2. the re-encryption loop in `change_master_password_impl` (`commands/vault.rs`)
3. `gather_payload` **and** the import loop in `commands/transfer.rs`

A new *table* adds one more: its rows must cascade with the entry (`ON DELETE CASCADE` plus the per-connection `foreign_keys` pragma), or purging an entry leaves its secrets behind.

This is not hypothetical.
A 2026-07-22 audit found that `change_master_password` re-encrypted only passwords and notes, destroying every TOTP secret on rekey, because TOTP had been added as new columns after those sweeps were written.

**Every such column needs a rekey regression test and an export/import regression test that carry the new field.**
`change_reencrypts_password_history_so_it_survives_a_rekey` / `export_import_preserves_password_history` and their `custom_fields` counterparts are the current examples; all four were confirmed to fail when their wiring is removed.
Custom fields (migration v7) were built against this checklist rather than discovering it afterwards, and the three tests were each run against deliberately broken wiring before being kept.

## 5. What Is Not Covered

- **Most component rendering and DI.** TestBed covers `entry-detail`, `entry-form`, and `entry-list`; the other eleven components are still verified only by driving the real UI in a browser against a mocked Tauri IPC layer (see §6).
- **The Tauri shell itself.** CSP enforcement, native file dialogs, and real OS clipboard behavior cannot be exercised headlessly and need a built binary.
- **CSS and layout.** No automated coverage; two layout defects in 2026-07 were found only by looking at the rendered app.
- **Concurrency.** Now covered where it matters: the unlock gate has a ten-thread test asserting only one guess gets through, and the clipboard write/record pair has an overlap test. Both were verified by breaking the code, not by assuming.

## 6. Browser-Based UI Verification

The frontend reaches the backend through exactly one funnel, `call()` in `core/services/tauri-invoke.ts`, plus two plugin imports.
That makes the whole UI drivable in an ordinary browser: serve the app, install `window.__TAURI_INTERNALS__` with a mock `invoke` before Angular boots, and back it with an in-memory vault.
Errors must reject with raw `{kind, message}` objects, matching the `AppError` wire format.

This is how the password-history panel, the keyboard shortcuts, the duplicate-entry flow, the CSV-export confirmation, and the vault-status error state were verified.
It complements the unit suites rather than replacing them, and it proves nothing about CSP or native dialogs.

## 7. Highest-Value Gaps to Close Next

The three gaps this section listed on 2026-07-25 are all closed:
a TestBed harness now covers `entry-detail` and `entry-form` (and found two real defects doing it),
the unlock backoff has a genuine ten-thread concurrency test,
and `a_vault_from_any_shipped_version_upgrades_straight_to_the_latest` migrates from every version 0 through 7 rather than only from v0.

What is left, in order:

1. **A component harness for the remaining components.** `settings`, `vault-health`, and `category-manage` all have async state and are covered only by browser checks.
2. **A CSV-import fuzz or property test.** The parser is hand-rolled and deliberately lenient; the current tests are example-based.
3. **An automated check that the ripple checklist was followed.** Every encrypted column is currently kept in step by discipline plus the regression tests named in §4; nothing fails if a future column simply never gets one.
