# Test Coverage

_Last verified 2026-07-25 on `main`: 230 Rust unit tests and 111 Vitest tests, all green._

## 1. How to Run

| Layer | Language | Framework | Command |
| --- | --- | --- | --- |
| Frontend (`src/`) | TypeScript / Angular 21 | Vitest (`@angular/build:unit-test`) | `npm test -- --watch=false` |
| Backend (`src-tauri/src/`) | Rust (Tauri 2) | built-in `#[cfg(test)]` | `cargo test --manifest-path src-tauri/Cargo.toml --lib` |

Both run in CI on every push, alongside `cargo clippy -D warnings` and `ng build`.

## 2. Where the Coverage Is

### Backend - 230 tests

| Module | Tests | What they cover |
| --- | --- | --- |
| `commands/transfer` | 46 | Encrypted export/import round trips, CSV import and export (tags and rotation reminder included), RFC 4180 parsing, atomic file writes, trash exclusion |
| `commands/entries` | 44 | CRUD, TOTP set/keep/clear, tags and favorites, `password_changed_at` semantics, trash/restore/purge, rotation-reminder due dates |
| `commands/vault` | 21 | Create/unlock/lock, master-password change and its re-encryption sweep (trashed entries included), unlock backoff |
| `commands/settings` | 20 | Persistence, range validation, clamping of hand-edited rows, published bounds matching the real validator |
| `crypto/totp` | 16 | RFC 6238 vectors, base32 decoding, `otpauth://` parsing |
| `commands/history` | 13 | Recording on rotation, retention pruning, cascade on delete |
| `commands/categories` | 13 | CRUD, unique-name mapping, character-based length limit |
| `commands/generator` | 10 | Class guarantees, ambiguous exclusion, length bounds |
| `db` | 9 | Migration sequencing, idempotency, rollback, v0 upgrade with data |
| `commands/health` | 12 | Weak/reused/stale/due classification, trash exclusion |
| `crypto/aead` | 7 | Round trip, tamper detection, randomized property test |
| `state`, `error` | 12 | Lock gating, error redaction |
| `crypto/kdf`, `commands/clipboard` | 7 | Key derivation, clipboard clear ownership |

### Frontend - 111 tests across 10 spec files

| Spec | Tests | What it covers |
| --- | --- | --- |
| `password-entry.service.spec.ts` | 44 | Filtering, sorting, tag parsing, TOTP action resolution, validation, rotation-reminder parsing and countdown text |
| `tauri-invoke.spec.ts` | 22 | Backend-error to user-message mapping |
| `password-strength.spec.ts` | 11 | Entropy scoring |
| `vault-layout.component.spec.ts` | 11 | Keyboard-shortcut dispatch, text-entry detection, suppression behind an open modal |
| `vault.service.spec.ts` | 3 | Create-vault form validity |
| `auto-lock.service.spec.ts` | 2 | Which visibility transitions count as activity |
| `settings.service.spec.ts` | 7 | Form validation driven by the backend's published bounds |
| `clipboard.service.spec.ts` | 3 | Banner countdown, and dropping copies superseded by a lock or a newer copy |
| `confirm-dialog.component.spec.ts` | 5 | Modal focus-trap wrapping |
| `theme.service.spec.ts` | 3 | Theme preference parsed from untrusted storage |

## 3. The Testing Approach

Two deliberate patterns make this suite cheap to write and hard to couple to implementation details.

**Backend: `_impl` functions behind thin command shims.**
Every command is a `#[tauri::command]` wrapper over a `fn *_impl(&AppState, …)`, so tests drive real logic against an in-memory SQLite database with no Tauri runtime.
Time-dependent code takes the clock as a parameter (`generate_totp_at`, `is_stale`) so behavior is asserted against fixed instants rather than sleeps.

**Frontend: pure functions extracted out of components and services.**
Logic worth testing is exported as a standalone function (`filterEntries`, `sortEntries`, `shortcutFor`, `countsAsActivity`, `canCreateVault`) and tested without a DOM or DI harness.
Components keep only the wiring.

## 4. The Encrypted-Column Ripple Rule

Any new encrypt-at-rest column or table must be wired through **every** path, or it silently loses data.
The paths are:

1. create / update / read in `commands/entries.rs`
2. the re-encryption loop in `change_master_password_impl` (`commands/vault.rs`)
3. `gather_payload` **and** the import loop in `commands/transfer.rs`

This is not hypothetical.
A 2026-07-22 audit found that `change_master_password` re-encrypted only passwords and notes, destroying every TOTP secret on rekey, because TOTP had been added as new columns after those sweeps were written.

**Every such column needs a rekey regression test and an export/import regression test that carry the new field.**
`change_reencrypts_password_history_so_it_survives_a_rekey` and `export_import_preserves_password_history` are the current examples; both were confirmed to fail when their wiring is removed.

## 5. What Is Not Covered

- **Angular component rendering and DI.** No TestBed harness exists. Component behavior is verified instead by driving the real UI in a browser against a mocked Tauri IPC layer (see §6).
- **The Tauri shell itself.** CSP enforcement, native file dialogs, and real OS clipboard behavior cannot be exercised headlessly and need a built binary.
- **CSS and layout.** No automated coverage; two layout defects in 2026-07 were found only by looking at the rendered app.
- **Concurrency.** The unlock-backoff TOCTOU fix is covered by reasoning and a single-threaded test, not by a race harness.

## 6. Browser-Based UI Verification

The frontend reaches the backend through exactly one funnel, `call()` in `core/services/tauri-invoke.ts`, plus two plugin imports.
That makes the whole UI drivable in an ordinary browser: serve the app, install `window.__TAURI_INTERNALS__` with a mock `invoke` before Angular boots, and back it with an in-memory vault.
Errors must reject with raw `{kind, message}` objects, matching the `AppError` wire format.

This is how the password-history panel, the keyboard shortcuts, the duplicate-entry flow, the CSV-export confirmation, and the vault-status error state were verified.
It complements the unit suites rather than replacing them, and it proves nothing about CSP or native dialogs.

## 7. Highest-Value Gaps to Close Next

1. **A component-level harness** for the two components with real async state machines (`entry-detail`, `entry-form`). Their stale-response guards are currently protected only by manual browser checks.
2. **A concurrency test for unlock backoff**, asserting that N simultaneous attempts consume N counts rather than sharing one pre-attempt read.
3. **A migration test from each historical version**, not just v0, so a v2-era vault upgrading straight to v6 is covered.
