# Test Coverage Improvement Report

_Originally generated 2026-05-28 on `chore/repo-cleanup` (backend pass). Updated 2026-05-29 on `main` for a frontend pass, then again 2026-05-29 for a backend boundary pass._

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
  - This pass took the frontend suite from `26 passed (26)` to `39 passed (39)` tests across the same 2 files (+13: Gaps F/G/H).
- **Backend** — 65 passing unit tests as of the prior pass (was 57; +8 from Gaps A/B/C below), across `commands/{entries,categories,generator,settings}`, `crypto/{aead,kdf}`, `db`, `state`, `error`.

## 2. Current Coverage Quality Summary

The suite is **high quality and behavior-focused**, not implementation-coupled. The Rust command layer is consistently structured as testable `*_impl(&AppState, …)` functions behind thin `#[tauri::command]` wrappers, exercised against an in-memory SQLite DB. The frontend deliberately extracts pure logic out of services so it can be tested without a DOM/DI harness.

The **prior pass closed the backend gaps** (§3 A–C, §5 improvements 1–3). This update focuses on the **frontend**, where re-inspection found that the previous report's claim that "the meaningful pure functions are already covered" was slightly too broad:

- `isBackendError` — an **exported** type guard that gates *all* backend-error handling — has no direct tests at all (Gap F).
- `filterEntries` and `formatBackendError` each have a couple of **untested branches / boundaries** despite otherwise-thorough coverage (Gaps G, H).

Remaining structural gaps are unchanged: modules reachable only through Tauri runtime types (`vault`, `clipboard` on the Rust side; thin signal/timer services and the guard on the TS side) would need a production refactor or a new component-testing style to unit-test, which these test-only passes avoid.

**This backend boundary pass (2026-05-29)** re-audited the already-tested Rust command modules and found that two numeric range checks test only their *rejecting* side, not their *accepting* boundary — the same off-by-one risk that motivated the category 64-char test (Gap C), but left open in two other modules:

- `update_settings_impl` rejects `29` and `86_401` but never confirms exactly `30` (MIN) and `86_400` (MAX) are *accepted* (Gap I).
- `generate` rejects `257` and accepts lengths up to `128`, but never confirms exactly `256` (MAX) is *accepted* (Gap J). The MIN boundary `4` is already covered.

Both are exact-boundary regressions: tightening a `<` to `<=` (or vice versa) would pass every existing test while silently rejecting a legitimate value. Both are closeable with test-only additions and zero production change.

## 3. Highest-Value Coverage Gaps

### Gap F — `isBackendError` type guard is untested _(this pass)_

- **Location:** `src/app/core/services/tauri-invoke.ts` (`isBackendError`)
- **Why it matters:** This is the predicate that classifies an unknown thrown value as a structured `{ kind, message }` backend error. Every call to `formatBackendError` delegates to it: if it returns a false negative, a real backend error silently degrades to `String(e)` (e.g. `"[object Object]"`); a false positive would route a non-error object through the `kind`/message mapping. It is exported and pivotal, yet only exercised indirectly via `formatBackendError`'s always-well-formed fixtures.
- **Existing tests:** None directly.
- **Missing cases:** `null`; `undefined`; primitives (`string`, `number`, `boolean`); arrays; an object missing `kind`; an object missing `message`; a well-formed object (`true`); a well-formed object with extra props (`true`).
- **Suggested tests:** Direct boolean assertions for each shape in a new `describe('isBackendError')` block.
- **Risk level:** Low (pure, no production change).
- **Validation:** `npm test -- --watch=false`
- **Status:** Implemented (see §5, improvement 4)

### Gap G — `filterEntries` whitespace-only query and empty-input boundaries _(this pass)_

- **Location:** `src/app/core/services/password-entry.service.ts` (`filterEntries`)
- **Why it matters:** A query made only of whitespace collapses to `''` after `trim()` and must behave like "no query" — returning everything in the current category scope, not matching nothing. That is a real path when a user types or pastes spaces into the search box. The empty-entries input is the natural lower boundary. The existing "trims whitespace" test uses a non-empty term (`'   github   '`), so neither of these is currently exercised.
- **Existing tests:** 9 cases, including category-only filtering, substring matches, AND-combination, and trimming a populated term.
- **Missing cases:** whitespace-only query returns all entries (and still respects an active category filter); an empty `entries` array returns `[]`.
- **Suggested tests:** Two `filterEntries` cases covering the collapsed-query branch and the empty-input boundary.
- **Risk level:** Low.
- **Validation:** `npm test -- --watch=false`
- **Status:** Implemented (see §5, improvement 5)

### Gap H — `formatBackendError` prefix-regex and override edge branches _(this pass)_

- **Location:** `src/app/core/services/tauri-invoke.ts` (`formatBackendError`, `VALIDATION_PREFIX`)
- **Why it matters:** The `validation:` prefix is stripped with `/^validation:\s*/`. The `\s*` means a no-space `validation:foo` must also strip to `foo` — a contract worth pinning so the regex is not "tightened" to require a space later. An override mapped to an **empty string** is a legitimate way to blank a message (`override !== undefined` is the guard), which is behaviorally distinct from "no override provided". And unknown non-error inputs such as `undefined` must coerce safely. None of these specific branches are covered by the existing cases.
- **Existing tests:** 11 cases (prefix-with-space strip, no-prefix passthrough, `kind` default maps, overrides replacing defaults and the validation strip, `Error` instances, and `string`/`number`/`null` coercion).
- **Missing cases:** no-space `validation:` still strips; an empty-string override returns `''`; `undefined` coerces to `"undefined"`.
- **Suggested tests:** Three additional `formatBackendError` cases.
- **Risk level:** Low.
- **Validation:** `npm test -- --watch=false`
- **Status:** Implemented (see §5, improvement 6)

### Gap I — `update_settings_impl` accepting boundaries (auto-lock min/max) _(this pass)_

- **Location:** `src-tauri/src/commands/settings.rs` (`update_settings_impl`, `MIN_AUTO_LOCK_SECS = 30`, `MAX_AUTO_LOCK_SECS = 86_400`)
- **Why it matters:** The guard is `secs < MIN || secs > MAX`. The existing tests prove `29` and `86_401` are rejected, but nothing proves `30` and `86_400` are *accepted*. If someone "tidied" the comparison to `<=`/`>=`, the auto-lock UI's own min/max options would start failing validation and every existing test would still pass. The auto-lock timeout is a security control, so its exact accepted range is a contract worth pinning on both ends.
- **Existing tests:** `update_rejects_value_below_minimum` (29), `update_rejects_value_above_maximum` (86_401), plus mid-range save/overwrite and locked-path tests.
- **Missing cases:** exactly `30` is accepted and round-trips; exactly `86_400` is accepted and round-trips.
- **Suggested tests:** Two tests mirroring the existing reject pair — `update_accepts_value_at_minimum_boundary` and `update_accepts_value_at_maximum_boundary`.
- **Risk level:** Low (pure boundary assertions, no production change).
- **Validation:** `cargo test --manifest-path src-tauri/Cargo.toml --lib settings`
- **Status:** Planned

### Gap J — `generate` accepting MAX length boundary _(this pass)_

- **Location:** `src-tauri/src/commands/generator.rs` (`generate`, `MIN_LEN = 4`, `MAX_LEN = 256`)
- **Why it matters:** The guard is `length < MIN_LEN || length > MAX_LEN`. `rejects_length_outside_range` covers `3` and `257`, and `returns_password_of_requested_length` covers `[4, 8, 16, 24, 64, 128]` — so the MIN accept boundary (`4`) is pinned but the MAX accept boundary (`256`) is not. A regression to `>=` on the upper bound would reject the documented maximum length while every current test stays green.
- **Existing tests:** `returns_password_of_requested_length` (up to 128), `rejects_length_outside_range` (3 and 257).
- **Missing cases:** exactly `256` generates a password of length 256.
- **Suggested tests:** One test `accepts_length_at_both_boundaries` asserting both `4` and `256` produce correctly-sized output (symmetric with `rejects_length_outside_range`).
- **Risk level:** Low.
- **Validation:** `cargo test --manifest-path src-tauri/Cargo.toml --lib generator`
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

**Backend boundary pass (2026-05-29, current):** two Low-risk, zero-production-change boundary gaps, one commit each, validating each with the targeted `cargo test … --lib <module>` then the full backend suite:

1. **Gap I** — add accepting-boundary tests (`30`, `86_400`) to `update_settings_impl` in `settings.rs`.
2. **Gap J** — add an accepting MAX-boundary test (`256`) to `generate` in `generator.rs`.

**Frontend pass (earlier 2026-05-29):** three Low-risk, zero-production-change frontend gaps, validated with `npm test -- --watch=false`:

1. **Gap F** — add a `describe('isBackendError')` block to `tauri-invoke.spec.ts`.
2. **Gap G** — add whitespace-only-query and empty-input cases to `filterEntries` in `password-entry.service.spec.ts`.
3. **Gap H** — add prefix-regex/override/coercion edge cases to `formatBackendError` in `tauri-invoke.spec.ts`.

Backend Gaps A–C were implemented in the original pass. Gaps D and E remain deferred because they require production refactors.

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
- **Commit hash:** `5484a43`
- **Push result:** Pushed to `origin/main`.

### Improvement 5 — `filterEntries` boundary branches (Gap G) _(this pass)_

- **Files changed:** `src/app/core/services/password-entry.service.spec.ts` (added 3 cases to the `filterEntries` block).
- **Behavior covered:** The collapsed-query branch (a whitespace-only query trims to `''` and behaves like "no query") and the empty-input boundary.
- **New test cases:** whitespace-only query returns all entries; whitespace-only query still respects an active category filter (returns only that category); an empty `entries` array returns `[]` for both unfiltered and filtered calls.
- **Validation run:** `npm test -- --watch=false`.
- **Result:** Pass — 3 new tests; frontend suite 34 → 37 passing, 0 failed.
- **Commit hash:** `63889e4`
- **Push result:** Pushed to `origin/main`.

### Improvement 6 — `formatBackendError` edge branches (Gap H) _(this pass)_

- **Files changed:** `src/app/core/services/tauri-invoke.spec.ts` (2 new cases + `undefined` added to the coercion case).
- **Behavior covered:** The `/^validation:\s*/` optional-space contract, the `!== undefined` override guard (empty string is a valid override), and `undefined` coercion.
- **New test cases:** a no-space `validation:` message still strips to its body; an empty-string override blanks the message intentionally (distinct from "no override"); `undefined` coerces to `"undefined"`.
- **Validation run:** `npm test -- --watch=false`.
- **Result:** Pass — 2 new tests; frontend suite 37 → 39 passing, 0 failed.
- **Commit hash:** `ac37b49`
- **Push result:** Pushed to `origin/main`.

## 6. Skipped Opportunities

- **`vault.rs` / `clipboard.rs` (Gaps D, E):** Both keep their logic inside `#[tauri::command]` functions bound to Tauri runtime types (`State`, `AppHandle`). Testing them properly needs a small behavior-preserving extraction, which is a production change these test-only passes avoid. Recommended as a follow-up.
- **Frontend services / components / guard** (`auto-lock`, `clipboard`, `category`, `settings`, `vault` services; `unlocked.guard`; feature components): these are thin wrappers around `call()` plus Angular signals, timers, and event listeners. Some hold genuinely valuable validation logic (e.g. the settings auto-lock bounds check `secs < 30 || secs > 86400`, and `vault-unlock`'s `canCreate` requiring `length >= 8 && pw1 === pw2`), but that logic is embedded in non-exported component methods. Covering it would require either introducing Angular `TestBed` (a new testing style not used anywhere in this repo) or extracting the logic into exported pure functions (a production change). Both are out of scope for a test-only pass; flagged as a follow-up if component-level testing is later adopted.

## 7. Final Notes

- The backend suite remains the home of the security-critical logic and was strengthened in the prior pass (57 → 65 tests).
- This frontend pass targets exported pure functions only — matching the repo's established strategy — and adds no production code or new test framework. It closes a genuinely overlooked gap (`isBackendError` had no direct tests) plus a handful of untested branches/boundaries in `filterEntries` and `formatBackendError`.
- The most valuable remaining follow-ups are: (a) extracting `*_impl` helpers in `vault.rs` so the master-password/unlock paths can be unit-tested; and (b) deciding whether to extract the frontend component validation rules into pure functions so they too can be covered without `TestBed`.
