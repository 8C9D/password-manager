# Test Coverage Improvement Report

_Originally generated 2026-05-28 on `chore/repo-cleanup` (backend pass). Updated 2026-05-29 on `main` for a frontend pass._

## 1. Repository Test Overview

This is a local-only password manager built as a **Tauri 2 + Angular 21** application.

| Layer | Language | Test framework | How to run |
| --- | --- | --- | --- |
| Frontend (`src/`) | TypeScript / Angular 21 | Vitest (`@angular/build:unit-test`) | `npm test -- --watch=false` |
| Backend (`src-tauri/src/`) | Rust (Tauri 2) | built-in `#[cfg(test)]` unit tests | `cargo test --manifest-path src-tauri/Cargo.toml --lib` |

Existing tests at the time of this update:

- **Frontend** — 2 spec files, both targeting *pure exported functions* rather than Angular DI/rendering:
  - `tauri-invoke.spec.ts` → `formatBackendError` (backend-error → user-message mapping)
  - `password-entry.service.spec.ts` → `filterEntries`, `validateEntryInput`
  - Baseline run: `2 passed (2)` test files, `26 passed (26)` tests.
- **Backend** — 65 passing unit tests as of the prior pass (was 57; +8 from Gaps A/B/C below), across `commands/{entries,categories,generator,settings}`, `crypto/{aead,kdf}`, `db`, `state`, `error`.

## 2. Current Coverage Quality Summary

The suite is **high quality and behavior-focused**, not implementation-coupled. The Rust command layer is consistently structured as testable `*_impl(&AppState, …)` functions behind thin `#[tauri::command]` wrappers, exercised against an in-memory SQLite DB. The frontend deliberately extracts pure logic out of services so it can be tested without a DOM/DI harness.

The **prior pass closed the backend gaps** (§3 A–C, §5 improvements 1–3). This update focuses on the **frontend**, where re-inspection found that the previous report's claim that "the meaningful pure functions are already covered" was slightly too broad:

- `isBackendError` — an **exported** type guard that gates *all* backend-error handling — has no direct tests at all (Gap F).
- `filterEntries` and `formatBackendError` each have a couple of **untested branches / boundaries** despite otherwise-thorough coverage (Gaps G, H).

Remaining structural gaps are unchanged: modules reachable only through Tauri runtime types (`vault`, `clipboard` on the Rust side; thin signal/timer services and the guard on the TS side) would need a production refactor or a new component-testing style to unit-test, which these test-only passes avoid.

## 3. Highest-Value Coverage Gaps

### Gap F — `isBackendError` type guard is untested _(this pass)_

- **Location:** `src/app/core/services/tauri-invoke.ts` (`isBackendError`)
- **Why it matters:** This is the predicate that classifies an unknown thrown value as a structured `{ kind, message }` backend error. Every call to `formatBackendError` delegates to it: if it returns a false negative, a real backend error silently degrades to `String(e)` (e.g. `"[object Object]"`); a false positive would route a non-error object through the `kind`/message mapping. It is exported and pivotal, yet only exercised indirectly via `formatBackendError`'s always-well-formed fixtures.
- **Existing tests:** None directly.
- **Missing cases:** `null`; `undefined`; primitives (`string`, `number`, `boolean`); arrays; an object missing `kind`; an object missing `message`; a well-formed object (`true`); a well-formed object with extra props (`true`).
- **Suggested tests:** Direct boolean assertions for each shape in a new `describe('isBackendError')` block.
- **Risk level:** Low (pure, no production change).
- **Validation:** `npm test -- --watch=false`
- **Status:** Planned

### Gap G — `filterEntries` whitespace-only query and empty-input boundaries _(this pass)_

- **Location:** `src/app/core/services/password-entry.service.ts` (`filterEntries`)
- **Why it matters:** A query made only of whitespace collapses to `''` after `trim()` and must behave like "no query" — returning everything in the current category scope, not matching nothing. That is a real path when a user types or pastes spaces into the search box. The empty-entries input is the natural lower boundary. The existing "trims whitespace" test uses a non-empty term (`'   github   '`), so neither of these is currently exercised.
- **Existing tests:** 9 cases, including category-only filtering, substring matches, AND-combination, and trimming a populated term.
- **Missing cases:** whitespace-only query returns all entries (and still respects an active category filter); an empty `entries` array returns `[]`.
- **Suggested tests:** Two `filterEntries` cases covering the collapsed-query branch and the empty-input boundary.
- **Risk level:** Low.
- **Validation:** `npm test -- --watch=false`
- **Status:** Planned

### Gap H — `formatBackendError` prefix-regex and override edge branches _(this pass)_

- **Location:** `src/app/core/services/tauri-invoke.ts` (`formatBackendError`, `VALIDATION_PREFIX`)
- **Why it matters:** The `validation:` prefix is stripped with `/^validation:\s*/`. The `\s*` means a no-space `validation:foo` must also strip to `foo` — a contract worth pinning so the regex is not "tightened" to require a space later. An override mapped to an **empty string** is a legitimate way to blank a message (`override !== undefined` is the guard), which is behaviorally distinct from "no override provided". And unknown non-error inputs such as `undefined` must coerce safely. None of these specific branches are covered by the existing cases.
- **Existing tests:** 11 cases (prefix-with-space strip, no-prefix passthrough, `kind` default maps, overrides replacing defaults and the validation strip, `Error` instances, and `string`/`number`/`null` coercion).
- **Missing cases:** no-space `validation:` still strips; an empty-string override returns `''`; `undefined` coerces to `"undefined"`.
- **Suggested tests:** Three additional `formatBackendError` cases.
- **Risk level:** Low.
- **Validation:** `npm test -- --watch=false`
- **Status:** Planned

### Gap A — `AppError` serialization contract (Rust → TypeScript) _(prior pass)_

- **Location:** `src-tauri/src/error.rs` (`impl Serialize for AppError`)
- **Why it matters:** Every backend command returns `AppError`, which serializes to `{ kind, message }`. The frontend switches on the exact `kind` strings and strips the `validation: ` prefix; if the Rust mapping drifts, error handling breaks silently. Opaque variants (`database`, `io`, `internal`) must not leak internal detail — a security property.
- **Risk level:** Low. **Status:** Implemented (see §5, improvement 1).

### Gap B — `read_secs` fallback on a corrupt/unparseable settings value _(prior pass)_

- **Location:** `src-tauri/src/commands/settings.rs` (`read_secs`)
- **Why it matters:** A malformed `auto_lock_secs` row must fall back to the default rather than panicking or yielding a `0`/garbage timeout, which would defeat the auto-lock security feature.
- **Risk level:** Low. **Status:** Implemented (see §5, improvement 2).

### Gap C — `validate_name` length boundary for categories _(prior pass)_

- **Location:** `src-tauri/src/commands/categories.rs` (`validate_name`)
- **Why it matters:** The 64-character cap is the only numeric boundary in the module; off-by-one boundary regressions are a classic bug.
- **Risk level:** Low. **Status:** Implemented (see §5, improvement 3).

### Gap D — `vault.rs` command logic is untested _(still open)_

- **Location:** `src-tauri/src/commands/vault.rs`
- **Why it matters:** Master-password length validation, "vault already exists", and wrong-password mapping are core security behaviors. Logic lives directly in `#[tauri::command]` functions taking `State<'_, AppState>`, so it needs a behavior-preserving `*_impl` extraction before it can be unit-tested.
- **Risk level:** Medium — requires a production refactor. **Status:** Skipped (see §6).

### Gap E — `copy_to_clipboard` clear-timeout logic is untested _(still open)_

- **Location:** `src-tauri/src/commands/clipboard.rs`
- **Why it matters:** The auto-clear delay clamp (`clamp(1, 600)`) and the "only clear if the clipboard still holds our token" guard are security-relevant, but bound to `AppHandle` and an async runtime.
- **Risk level:** Medium — needs a production refactor. **Status:** Skipped (see §6).

## 4. Test Improvement Plan

This pass implements the three Low-risk, zero-production-change **frontend** gaps, one commit each, validating after each with `npm test -- --watch=false`:

1. **Gap F** — add a `describe('isBackendError')` block to `tauri-invoke.spec.ts`.
2. **Gap G** — add whitespace-only-query and empty-input cases to `filterEntries` in `password-entry.service.spec.ts`.
3. **Gap H** — add prefix-regex/override/coercion edge cases to `formatBackendError` in `tauri-invoke.spec.ts`.

Backend Gaps A–C were implemented in the prior pass. Gaps D and E remain deferred because they require production refactors.

## 5. Implemented Test Improvements

### Improvement 1 — `AppError` serialization contract (Gap A) _(prior pass)_

- **Files changed:** `src-tauri/src/error.rs`.
- **Behavior covered:** The `{ kind, message }` wire format the frontend's `formatBackendError` consumes.
- **Result:** Pass — 5 new tests; backend suite 57 → 62.
- **Commit hash:** `1e380d6` · **Push result:** Pushed (merged to `main`).

### Improvement 2 — `read_secs` corrupt-value fallback (Gap B) _(prior pass)_

- **Files changed:** `src-tauri/src/commands/settings.rs`.
- **Behavior covered:** `read_secs` fallback when the stored value is non-numeric.
- **Result:** Pass — 1 new test; backend suite 62 → 63.
- **Commit hash:** `036afb8` · **Push result:** Pushed (merged to `main`).

### Improvement 3 — category name length boundary (Gap C) _(prior pass)_

- **Files changed:** `src-tauri/src/commands/categories.rs`.
- **Behavior covered:** `validate_name`'s 64-character cap.
- **Result:** Pass — 2 new tests; backend suite 63 → 65.
- **Commit hash:** `983979d` · **Push result:** Pushed (merged to `main`).

### Improvement 4 — `isBackendError` type guard (Gap F) _(this pass)_

- **Files changed:** `src/app/core/services/tauri-invoke.spec.ts` (added a `describe('isBackendError')` block).
- **Behavior covered:** The exported type guard that decides whether an unknown thrown value is a structured `{ kind, message }` backend error — the predicate every `formatBackendError` call delegates to.
- **New test cases:** well-formed object → `true`; object with extra props → `true`; `null`/`undefined` → `false`; primitives (`string`/`number`/`boolean`) → `false`; arrays → `false`; objects missing `kind` or `message` → `false`; a plain `Error` (has `message` but no `kind`) → `false`; and a documentation case showing the guard checks key *presence*, not value type.
- **Validation run:** `npm test -- --watch=false`.
- **Result:** Pass — 8 new tests; frontend suite 26 → 34 passing, 0 failed.
- **Commit hash:** `PENDING_4`
- **Push result:** PENDING.

### Improvement 5 — `filterEntries` boundary branches (Gap G) _(this pass)_

- _Pending implementation._

### Improvement 6 — `formatBackendError` edge branches (Gap H) _(this pass)_

- _Pending implementation._

## 6. Skipped Opportunities

- **`vault.rs` / `clipboard.rs` (Gaps D, E):** Both keep their logic inside `#[tauri::command]` functions bound to Tauri runtime types (`State`, `AppHandle`). Testing them properly needs a small behavior-preserving extraction, which is a production change these test-only passes avoid. Recommended as a follow-up.
- **Frontend services / components / guard** (`auto-lock`, `clipboard`, `category`, `settings`, `vault` services; `unlocked.guard`; feature components): these are thin wrappers around `call()` plus Angular signals, timers, and event listeners. Some hold genuinely valuable validation logic (e.g. the settings auto-lock bounds check `secs < 30 || secs > 86400`, and `vault-unlock`'s `canCreate` requiring `length >= 8 && pw1 === pw2`), but that logic is embedded in non-exported component methods. Covering it would require either introducing Angular `TestBed` (a new testing style not used anywhere in this repo) or extracting the logic into exported pure functions (a production change). Both are out of scope for a test-only pass; flagged as a follow-up if component-level testing is later adopted.

## 7. Final Notes

- The backend suite remains the home of the security-critical logic and was strengthened in the prior pass (57 → 65 tests).
- This frontend pass targets exported pure functions only — matching the repo's established strategy — and adds no production code or new test framework. It closes a genuinely overlooked gap (`isBackendError` had no direct tests) plus a handful of untested branches/boundaries in `filterEntries` and `formatBackendError`.
- The most valuable remaining follow-ups are: (a) extracting `*_impl` helpers in `vault.rs` so the master-password/unlock paths can be unit-tested; and (b) deciding whether to extract the frontend component validation rules into pure functions so they too can be covered without `TestBed`.
