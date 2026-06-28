# Security Sanity Check Report

_Last updated 2026-05-29 on branch `main` (re-verification pass). First generated 2026-05-28 on the since-merged `chore/repo-cleanup` branch. This is a practical hygiene review, not a formal penetration test or full audit._

## 1. Scope

Local, static inspection of the tracked repository: secrets/sensitive files, `.gitignore` publish safety, the crypto/auth flow, input validation and unsafe operations (Rust + frontend), Tauri security configuration, dependencies, and CI/CD. No exploitation, no external network probing.

## 2. Project Overview

- **App:** local-only personal password manager.
- **Frontend:** Angular 21 (TypeScript), Vitest tests.
- **Backend:** Tauri 2 (Rust) exposing 17 `#[tauri::command]`s.
- **Storage:** SQLite (`rusqlite`, bundled) at the OS app-data dir (`app_data_dir/vault.db`).
- **Crypto:** Argon2id KDF (m=64 MiB, t=3, p=1) → AES-256-GCM; random salt + per-message nonce; keys/passwords wrapped in `Zeroizing`.
- **Env vars:** none used. **Deployment/CI:** none present.
- **Dependency managers:** npm (frontend), Cargo (backend); both lockfiles committed.

## 3. Executive Summary

**Overall risk: Low.**

The codebase is notably security-conscious. Crypto choices are sound, all SQL is parameterized, there is no `unsafe` Rust, no shell execution, no XSS-prone frontend APIs, no secrets committed, and the local vault database is git-ignored and stored outside the repo. Dependency audit (npm) is clean.

The main hardening gap is the **absent Content-Security-Policy** (`csp: null`), which is a defense-in-depth concern for a high-value target rather than an active vulnerability today. It remains the single open item and requires manual action (see Finding 1).

The 2026-05-29 re-verification pass reconfirmed every prior finding on `main`. The eight template/style refactor commits merged since 2026-05-28 (splitting component templates and styles into separate files) introduced **no new XSS sinks** — no `[innerHTML]`, `bypassSecurityTrust*`, `DomSanitizer`, `target="_blank"`, or external navigation. Two safe, behavior-neutral hygiene fixes were applied in prior passes (a `.gitignore` hardening and a README "Security & data" section) and are now in `main` history. **No code or configuration changes were made this pass** — only this report was updated.

## 4. Findings

### Finding 1 — No Content-Security-Policy configured

- **Severity:** Medium
- **Title:** CSP disabled in Tauri config and absent from `index.html`
- **Location:** `src-tauri/tauri.conf.json` (`app.security.csp = null`); `src/index.html` (no CSP `<meta>`)
- **Evidence:** `"security": { "csp": null }`; `index.html` `<head>` has no `Content-Security-Policy` meta tag.
- **Why it matters:** A password manager is a high-value target. With no CSP, a future XSS regression (e.g., someone introducing `[innerHTML]` or a `bypassSecurityTrust*` call) would face no second layer of containment — script injection and exfiltration to a remote origin would be unrestricted. Tauri's security guidance recommends an explicit, restrictive CSP. Exploitability **today is low** (see §8: no untrusted HTML is rendered and Angular auto-escapes), so this is hardening, not an open hole.
- **Recommended fix:** Define a restrictive CSP in `tauri.conf.json`. A reasonable starting point for a Tauri 2 + Angular app is `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self' ipc: http://ipc.localhost; object-src 'none'; base-uri 'self'; frame-ancestors 'none'`, then tighten/loosen against the built app. Notes: Angular component `styles:` blocks are injected as inline `<style>` tags, so `style-src 'unsafe-inline'` is required unless nonces/hashes are wired up; Tauri v2 IPC needs the `ipc:`/`http://ipc.localhost` `connect-src` entries on some platforms — omitting them breaks all backend calls.
- **Auto-fix status:** Manual action required — changing CSP alters runtime behavior and can break Angular styling or, if IPC origins are wrong, the entire app. It must be validated against the built Tauri app per target platform, which is outside a safe auto-fix.
- **Secret/value redacted:** No.
- **Confidence:** High (that CSP is absent); Medium (on exact directives needed).

### Finding 2 — `.gitignore` did not protect `.env` / extra local-DB files _(resolved)_

- **Severity:** Low
- **Title:** Missing preventive ignore rules for secret/DB files
- **Location:** `.gitignore`
- **Evidence:** Before the 2026-05-28 fix, `.gitignore` ignored `*.db`/`*.db-shm`/`*.db-wal` but had no `.env*` rules and no `*.sqlite`/`*.sqlite3`.
- **Why it matters:** No env files or alternate DB files exist today and the app uses no env vars, but the guards prevent a future contributor from accidentally committing secrets or a local vault database under a different extension.
- **Recommended fix:** Add `.env`, `.env.*` (with a `!.env.example` exception) and `*.sqlite`/`*.sqlite3` to `.gitignore`.
- **Auto-fix status:** Fixed (commit `81142fa`, now in `main`). Re-verified present this pass.
- **Secret/value redacted:** No.
- **Confidence:** High.

### Finding 3 — README had no security/setup notes _(resolved)_

- **Severity:** Info
- **Title:** Default Angular CLI README
- **Location:** `README.md`
- **Evidence:** The README was the generated Angular boilerplate; it didn't mention that the vault DB is stored locally in the OS app-data dir and must never be committed, nor the master-password/auto-lock model.
- **Why it matters:** Minor; documenting the local-storage/secret-handling model helps contributors avoid mistakes (e.g. accidentally committing a vault DB or assuming a forgotten master password can be recovered).
- **Recommended fix:** Add a short "Security & data" note to the README.
- **Auto-fix status:** Fixed (commit `68c8b72`, now in `main`). Re-verified present this pass.
- **Secret/value redacted:** No.
- **Confidence:** High.

## 5. Secrets and Sensitive Files Review

- `git grep` for `API_KEY`/`SECRET`/`PASSWORD`/`TOKEN`/`PRIVATE_KEY`/`BEGIN RSA`/`BEGIN OPENSSH`/`DATABASE_URL`/`client_secret`/`access_key`/`bearer` over tracked source returned **only legitimate code** — struct fields (`password`, `clipboard_token`), error strings, and a redaction **unit test** in `error.rs` that asserts a fake path (`/Users/secret/vault.db`) does *not* leak to the frontend. Test fixtures use clearly-fake values (`"hunter2"`, `"newpass"`). No real secrets.
- No `.env`, `*.pem`, `*.key`, `id_rsa*`, service-account JSON, or `*.db`/`*.sqlite` files are tracked or present on disk (searched to depth 3, excluding `node_modules`/`target`/`.git`).
- `.vscode/mcp.json` (tracked) configures only the `@angular/cli` MCP server via `npx` — **no tokens/keys**. `launch.json`/`tasks.json`/`extensions.json` are standard Angular scaffolding.
- The build output directory `dist/` is **not tracked** (0 files under git).
- **No potential real secrets were found.**

## 6. `.gitignore` and Publish Safety Review

Strong baseline: `node_modules`, `/dist`, `/src-tauri/target`, `/gen/schemas`, `.angular/cache`, `.DS_Store`, `coverage`, the **local vault data** (`*.db`, `*.db-shm`, `*.db-wal`, `*.sqlite`, `*.sqlite3`), and **environment/secret files** (`.env`, `.env.*` with a `!.env.example` exception) are all ignored. `.vscode/*` is ignored with a small whitelist of non-sensitive shared configs (`settings.json`, `tasks.json`, `launch.json`, `extensions.json`, `mcp.json`). No tracked file matches the secret/DB patterns.

## 7. Authentication and Authorization Review

- **Master password → key:** Argon2id (`m=65536 KiB`, `t=3`, `p=1`, 32-byte output) with a 16-byte random salt from `OsRng`; verified by decrypting a stored AES-256-GCM test value (`unlock_vault`), returning `WrongPassword` on mismatch (`src-tauri/src/commands/vault.rs`, `crypto/kdf.rs`).
- **No plaintext password storage, no hardcoded users, no auth bypass.** Entry passwords/notes are encrypted at rest with AES-256-GCM; only decrypted on explicit `get_entry`.
- **Lock model:** in-memory key cleared on `lock_vault` (which also clears the clipboard token); the `with_unlocked`/`with_authorized` gates return `Locked` when no key is present, enforced on every sensitive command. Idle auto-lock is configurable (30 s–24 h, default 5 min).
- Keys and master passwords are held in `Zeroizing`, so they are wiped on drop.
- No auth/authorization changes were made (out of scope for auto-fix).

## 8. Input Validation and Unsafe Operation Review

- **SQL injection:** None. Every query uses `rusqlite` bound parameters (`?1`, `?2`, …); no `format!`-built SQL. Sorting uses fixed `COLLATE NOCASE` clauses, not user input.
- **Unsafe Rust / command execution:** None. No `unsafe`, no `std::process`/`Command`, no `std::fs` beyond the startup `create_dir_all` of the app-data dir.
- **Input validation:** master password length (≥ 8, non-empty), entry title (non-blank after trim) and password (non-empty) required, category name non-empty and ≤ 64 chars, generator length 4–256 with at least one character class, auto-lock 30 s–24 h, clipboard clear delay clamped to 1–600 s — all enforced server-side in Rust and covered by unit tests.
- **Password generator:** uses `OsRng` (CSPRNG) with `rand`'s `gen_range`, which samples without modulo bias. Character classes and ambiguous-character exclusion are honored.
- **Frontend XSS:** No `innerHTML`/`[innerHTML]`, `outerHTML`, `document.write`, `eval`, `javascript:`, `bypassSecurityTrust*`, or `DomSanitizer` usage. No `target="_blank"` or external `href`/`window.open` navigation. Angular template auto-escaping is intact; entry data is rendered via interpolation. Re-verified after the 2026-05-29 template/style split refactors.
- **Logging:** The application code emits no `log`/`println!`/`dbg!` records of its own. `tauri-plugin-log` is enabled (level `Info`, `src-tauri/src/lib.rs`) for framework diagnostics only, so no master password, derived key, or decrypted entry data is written to logs. Backend errors serialized to the frontend are opaque for `database`/`io`/`internal` variants (no internal detail leaked) — covered by `error.rs` tests.

## 9. Dependency and Tooling Review

- `npm audit --audit-level=moderate` → **found 0 vulnerabilities** (re-run 2026-05-29).
- Backend crates are current and security-appropriate: `argon2 0.5`, `aes-gcm 0.10`, `rand 0.8`, `zeroize 1.8`, `rusqlite 0.32` (bundled SQLite), `tauri 2.11`. The release profile strips symbols (`strip = true`).
- `Cargo.lock` is committed (reproducible builds). `cargo audit` is not installed and was not added (per skill rules, no new tooling installed); recommend running it in CI as a follow-up.
- No risky lifecycle scripts in `package.json` (only standard `ng`/`tauri` wrappers).

## 10. CI/CD and Deployment Review

No CI/CD configuration is present (`.github/workflows` absent), so there are no pipeline secrets or deployment scripts to review. If CI is added later, recommend: secret scanning, `npm audit`/`cargo audit` gates, and never building from untrusted forks with secrets exposed.

## 11. Auto-Fixes Applied

**This pass (2026-05-29): no code or configuration changes.** The two safe fixes below were applied in earlier passes and are already merged into `main`; they were re-verified present this pass. Only this report was updated.

### Fix 1 — Harden `.gitignore` against secrets and local DB files _(prior pass; in `main`)_

- **Files changed:** `.gitignore`
- **What changed:** Added `*.sqlite`/`*.sqlite3` to the local-vault-data section and an "Environment & secrets" section with `.env`, `.env.*`, and a `!.env.example` exception.
- **Why it is safe:** `.gitignore` does not affect application behavior, and no currently-tracked file matches the new patterns (verified via `git ls-files`), so nothing is untracked or hidden. Purely preventive.
- **Commit hash:** `81142fa` (now in `main`).

### Fix 2 — Add a "Security & data" section to the README _(prior pass; in `main`)_

- **Files changed:** `README.md`
- **What changed:** Added a "Security & data" section documenting that this is a local-only app (no server/sync), where the encrypted vault lives (OS app-data dir, git-ignored — never commit it or any `.env`), the crypto model (Argon2id → AES-256-GCM, master password never stored and zeroized on lock), that a forgotten master password is unrecoverable, and the auto-lock / clipboard auto-clear behavior.
- **Why it is safe:** Documentation only — no code, build, or runtime behavior changes. The content restates the model already implemented and verified in §7–§8.
- **Commit hash:** `68c8b72` (now in `main`).

## 12. Recommended Manual Fix Order

1. **Define a restrictive CSP** (Finding 1) and validate against the built Tauri app on each target platform — highest-value hardening.
2. Add a CI workflow with `npm audit` + `cargo audit` gates and secret scanning (§9 / §10).

## 13. Commands Run

```text
# Startup
pwd; git status --short; git branch --show-current; git remote -v; ls -la
git rev-parse --abbrev-ref --symbolic-full-name @{u}        # origin/main
git log --oneline -15

# Sensitive files / secrets
git ls-files | grep -Ei '\.(env|pem|key|p12|pfx|db|sqlite|sqlite3|crt|cer)$|id_rsa|credentials|secret'   # none
find . -maxdepth 3 -type f \( -name '*.db' -o -name '.env*' -o -name '*.pem' -o -name '*.key' \
  -o -name '*.sqlite*' -o -name 'id_rsa*' -o -name '*credential*' \) \
  -not -path './node_modules/*' -not -path './src-tauri/target/*' -not -path './.git/*'                  # none
git grep -nIi 'api_key|secret|private_key|BEGIN RSA|BEGIN OPENSSH|database_url|client_secret|access_key|bearer ' \
  -- src src-tauri/src ':!*.spec.ts' ':!*test*'                                                          # only error.rs redaction test
git ls-files dist/ | wc -l                                                                              # 0 (not tracked)
git ls-files .vscode/                                                                                   # mcp/launch/tasks/extensions only

# Frontend safety
git grep -nIi 'innerhtml|outerhtml|bypasssecuritytrust|document.write|eval(|javascript:|DomSanitizer|sanitize' -- src   # none
git grep -nIi 'target=|window.open|href="http|location.href|location.assign' -- 'src/**/*.html' 'src/**/*.ts'          # none

# Backend safety
git grep -nI 'process.env|import.meta.env|std::env::var|dotenv' -- src src-tauri/src                     # none
# Manual read of all crypto/, db/, state.rs, error.rs, and command modules: SQL parameterized; no unsafe/Command/std::fs

# Dependencies
npm audit --audit-level=moderate                                                                        # 0 vulnerabilities

# Validation of this pass's change (docs only)
git diff --check; git diff --stat; git status --short
```

## 14. Final Notes

For a personal, local-only password manager this is a clean, defensively-written codebase: sound modern crypto with key zeroization, parameterized SQL, no unsafe/shell/dynamic-SQL paths, no XSS sinks, no committed secrets, ignored local vault data, minimal Tauri capabilities (clipboard text + core only), a clipboard auto-clear that only clears the value it set, and a clean dependency audit. The 2026-05-29 re-verification on `main` reconfirmed all findings and confirmed the intervening template/style refactors introduced no XSS sinks. The single meaningful hardening step remains adopting an explicit Content-Security-Policy (Finding 1), which is manual because it changes webview runtime behavior and must be validated against the built app. No risky changes were made this pass; the report was updated only.
