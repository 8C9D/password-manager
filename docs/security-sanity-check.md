# Security Sanity Check

_Last verified 2026-07-24 on `main`, against Angular 21.2.18 / Tauri 2.11 and 27 exposed commands._
_This is a practical hygiene review, not a formal penetration test or full audit._

## 1. Scope

Local, static inspection of the tracked repository: secrets and sensitive files, `.gitignore` publish safety, the crypto and auth flow, input validation and unsafe operations (Rust + frontend), Tauri security configuration, dependencies, and CI.
No exploitation, no external network probing.

## 2. Project Overview

- **App:** local-only personal password manager. No server, no network sync.
- **Frontend:** Angular 21 (TypeScript), Vitest tests.
- **Backend:** Tauri 2 (Rust) exposing 27 `#[tauri::command]`s.
- **Storage:** SQLite (`rusqlite`, bundled) at `app_data_dir/vault.db`, outside the repo.
- **Crypto:** Argon2id KDF (m = 64 MiB, t = 3, p = 1) then AES-256-GCM; random salt, fresh per-message nonce; keys and master passwords wrapped in `Zeroizing`.
- **Env vars:** none used.
- **Dependency managers:** npm (frontend), Cargo (backend); both lockfiles committed.

## 3. Executive Summary

**Overall risk: Low.**

Crypto choices are sound, all SQL is parameterized, there is no `unsafe` Rust, no shell execution, no XSS-prone frontend APIs, no secrets committed, and the vault database is git-ignored and stored outside the repository.

The two findings that were open in the 2026-05 passes are both closed: a restrictive CSP is now configured, and CI runs both `npm audit` and a blocking `cargo audit`.
The one item found open in this pass (vulnerable Angular runtime packages) was fixed during it.

## 4. Findings

### Finding 1 - Vulnerable Angular runtime packages shipped in the binary _(resolved 2026-07-24)_

- **Severity:** Medium
- **Location:** `package.json` / `package-lock.json`
- **Evidence:** `npm audit --omit=dev` reported 6 advisories against `@angular/{common,compiler,core,forms,platform-browser,router}` at 21.2.13, including two XSS sanitization bypasses (two-way property binding, and template/attribute namespace) plus a DoS in `formatDate`. The vulnerable range was `21.0.0-next.0 - 21.2.16`.
- **Why it matters:** Unlike the dev-tooling advisories CI treats as informational, these packages are compiled into the shipped app. A sanitization bypass is exactly the second-layer failure the CSP exists to contain, so leaving both weak at once is worse than either alone.
- **Fix applied:** Updated the Angular runtime and tooling to 21.2.18 / 21.2.19, inside the existing `^21.2.x` range (no breaking change). `npm audit --omit=dev` now reports **0 vulnerabilities**; `ng build`, the Vitest suite, and a browser smoke test of the app all pass on the new version.
- **Confidence:** High.

### Finding 2 - No Content-Security-Policy configured _(resolved)_

- **Severity:** Medium
- **Location:** `src-tauri/tauri.conf.json` (`app.security.csp`)
- **Evidence:** The config previously set `"csp": null` and `index.html` carried no CSP meta tag.
- **Fix applied:** A restrictive policy is now configured:
  `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' ipc: http://ipc.localhost; object-src 'none'; base-uri 'self'; frame-ancestors 'none'`.
  `style-src 'unsafe-inline'` is required because Angular injects component styles as inline `<style>` tags; the `ipc:` / `http://ipc.localhost` `connect-src` entries are required for Tauri v2 IPC.
- **Residual note:** Enforcement can only be observed in the built Tauri shell, not in the browser-based UI harness. Confirm IPC and styling in a `npm run tauri:build` binary after any CSP edit.
- **Confidence:** High.

### Finding 3 - `.gitignore` did not protect `.env` / extra local-DB files _(resolved)_

Preventive rules for `.env`, `.env.*` (with a `!.env.example` exception), `*.sqlite`, and `*.sqlite3` are present and re-verified.

### Finding 4 - README had no security/setup notes _(resolved)_

The README now documents the local-storage model, the KDF and cipher, and that a forgotten master password is unrecoverable.

## 5. Secrets and Sensitive Files

Searches for `API_KEY` / `SECRET` / `PASSWORD` / `TOKEN` / `PRIVATE_KEY` / `BEGIN RSA` / `BEGIN OPENSSH` / `DATABASE_URL` / `client_secret` / `access_key` / `bearer` over tracked source return only legitimate code: struct fields (`password`, `clipboard_token`), error strings, and a redaction unit test in `error.rs` asserting that a fake path does *not* reach the frontend.
Test fixtures use obviously-fake values (`hunter2`, `newpass`).

No `.env`, `*.pem`, `*.key`, `id_rsa*`, service-account JSON, or `*.db` / `*.sqlite` files are tracked.
`dist/` and `src-tauri/target/` are untracked.
`.vscode/mcp.json` configures only the `@angular/cli` MCP server via `npx` and carries no tokens.

**No real secrets found.**

## 6. Authentication and Authorization

- **Master password to key:** Argon2id (m = 65536 KiB, t = 3, p = 1, 32-byte output) with a 16-byte `OsRng` salt, verified by decrypting a stored AES-256-GCM test value. Mismatch returns `WrongPassword`.
- **Unlock backoff:** consecutive failures are counted *at the gate* before the KDF runs, then escalate 1 s doubling to a 300 s cap after 5 free attempts. This slows interactive guessing only; the Argon2id cost is what defends a copied vault file.
- **No plaintext password storage, no hardcoded users, no auth bypass.** Entry passwords, notes, TOTP secrets, and retained previous passwords are all encrypted at rest.
- **Lock model:** the in-memory key is cleared on `lock_vault`, which also clears the clipboard token; `with_unlocked` / `with_authorized` return `Locked` when no key is present. The frontend additionally purges cached entry and category state and dismisses any open dialog on lock.
- **Auto-lock:** configurable 30 s to 24 h, clamped on read so a hand-edited row cannot disable it. Only a return to visibility counts as activity; the window becoming hidden does not restart the countdown.
- Keys and master passwords are held in `Zeroizing`, including the per-command stack copy handed to `with_unlocked`.

## 7. Input Validation and Unsafe Operations

- **SQL injection:** none. Every query uses bound parameters; no `format!`-built SQL. Sorting uses fixed `COLLATE NOCASE` clauses.
- **Unsafe Rust / command execution:** none. No `unsafe`, no `std::process`.
- **Filesystem:** the startup `create_dir_all`, plus export/import paths chosen through the native dialog. Exports are written to a temp file and renamed into place, so a failed write cannot truncate an existing backup.
- **Input validation, enforced server-side and unit-tested:** master password length; entry title and password non-empty; category name non-empty and 64 *characters* or fewer; generator length 4–256 with at least one class; auto-lock 30 s–24 h; clipboard clear 1–600 s; password-history retention 0–50; TOTP configs validated on import and before every generation.
- **Import hardening:** encrypted-vault imports hold hand-edited files to the same invariants as the UI write paths. CSV import stays deliberately lenient (bad rows are skipped) but clips over-long folder names to the category limit so imported data cannot become un-editable.
- **Password generator:** `OsRng` with `gen_range` (no modulo bias), rejection-sampled so every selected class appears.
- **Frontend XSS:** no `innerHTML` / `[innerHTML]`, `outerHTML`, `document.write`, `eval`, `javascript:`, `bypassSecurityTrust*`, or `DomSanitizer` usage. Angular auto-escaping is intact.
- **Logging:** the application emits no log records of its own. `tauri-plugin-log` is enabled at `Info` for framework diagnostics. `database` / `io` / `internal` errors are serialized opaquely so internal detail never reaches the frontend.

## 8. Dependencies and Tooling

- `npm audit --omit=dev` → **0 vulnerabilities** (2026-07-24, after Finding 1).
- `npm audit` including dev dependencies reports 3 moderate advisories in build tooling (`@angular/cli` and two transitive MCP-server packages). These are not compiled into the shipped binary, which is why CI treats the npm audit as informational.
- `cargo audit` runs as a **blocking** CI job. `.cargo/audit.toml` ignores exactly two advisories, RUSTSEC-2026-0194 and RUSTSEC-2026-0195 (quick-xml DoS reachable only via `tauri` → `plist`, which pins `quick-xml ^0.39.2`), each with a written rationale and a revisit condition.
- Backend crates are current: `argon2 0.5`, `aes-gcm 0.10`, `rand 0.8`, `zeroize 1.8`, `rusqlite 0.32`, `tauri 2.11`. The release profile strips symbols.
- Both lockfiles are committed. No risky lifecycle scripts in `package.json`.

## 9. CI

`.github/workflows/ci.yml` runs `cargo test` and `cargo clippy -D warnings` on a pinned toolchain, the frontend `ng test` and `ng build`, an informational `npm audit`, and a blocking `cargo audit`.

## 10. Known Limits of This Review

- Static inspection only; no fuzzing, no runtime instrumentation, no side-channel analysis.
- CSP enforcement, native dialogs, and real clipboard behavior can only be confirmed in a built Tauri binary. The browser harness used for UI verification mocks the IPC layer and therefore proves nothing about CSP.
- Threat model assumes an unprivileged attacker. Anything with code execution as the user, or memory access to the process while unlocked, is out of scope by design.
