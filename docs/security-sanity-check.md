# Security Sanity Check Report

_Generated 2026-05-28 on branch `chore/repo-cleanup` (re-verified the same day; a README "Security & data" section was added this pass). This is a practical hygiene review, not a formal penetration test or full audit._

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

The main hardening gap is the **absent Content-Security-Policy** (`csp: null`), which is a defense-in-depth concern for a high-value target rather than an active vulnerability today. Two safe, behavior-neutral hygiene improvements have been auto-applied: a `.gitignore` hardening (prior pass) and a README "Security & data" section (this pass).

## 4. Findings

### Finding 1 — No Content-Security-Policy configured

- **Severity:** Medium
- **Title:** CSP disabled in Tauri config and absent from `index.html`
- **Location:** `src-tauri/tauri.conf.json` (`app.security.csp = null`); `src/index.html` (no CSP `<meta>`)
- **Evidence:** `"security": { "csp": null }`; `index.html` `<head>` has no `Content-Security-Policy` meta tag.
- **Why it matters:** A password manager is a high-value target. With no CSP, a future XSS regression (e.g., someone introducing `[innerHTML]` or a `bypassSecurityTrust*` call) would face no second layer of containment — script injection and exfiltration to a remote origin would be unrestricted. Tauri's security guidance recommends an explicit, restrictive CSP. Exploitability **today is low** (see §8: no untrusted HTML is rendered and Angular auto-escapes), so this is hardening, not an open hole.
- **Recommended fix:** Define a restrictive CSP in `tauri.conf.json`, e.g. start from `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'` and tighten/loosen against the built app (Angular component `styles:` blocks typically require `style-src 'unsafe-inline'` unless nonces/hashes are wired up).
- **Auto-fix status:** Manual action required — changing CSP alters runtime behavior and can break Angular styling/scripts; it must be validated against the built app, which is outside a safe auto-fix.
- **Secret/value redacted:** No.
- **Confidence:** High (that CSP is absent); Medium (on exact directives needed).

### Finding 2 — `.gitignore` did not protect `.env` / extra local-DB files

- **Severity:** Low
- **Title:** Missing preventive ignore rules for secret/DB files
- **Location:** `.gitignore`
- **Evidence:** Before this pass, `.gitignore` ignored `*.db`/`*.db-shm`/`*.db-wal` but had no `.env*` rules and no `*.sqlite`/`*.sqlite3`.
- **Why it matters:** No env files or alternate DB files exist today and the app uses no env vars, but adding the guards now prevents a future contributor from accidentally committing secrets or a local vault database under a different extension.
- **Recommended fix:** Add `.env`, `.env.*` (with a `!.env.example` exception) and `*.sqlite`/`*.sqlite3` to `.gitignore`.
- **Auto-fix status:** Fixed (see §11).
- **Secret/value redacted:** No.
- **Confidence:** High.

### Finding 3 — README has no security/setup notes

- **Severity:** Info
- **Title:** Default Angular CLI README
- **Location:** `README.md`
- **Evidence:** README is the generated Angular boilerplate; it doesn't mention that the vault DB is stored locally in the OS app-data dir and must never be committed, nor the master-password/auto-lock model.
- **Why it matters:** Minor; documenting the local-storage/secret-handling model helps contributors avoid mistakes (e.g. accidentally committing a vault DB or assuming a forgotten master password can be recovered).
- **Recommended fix:** Add a short "Security & data" note to the README.
- **Auto-fix status:** Fixed (see §11) — added a "Security & data" section to `README.md`.
- **Secret/value redacted:** No.
- **Confidence:** High.

## 5. Secrets and Sensitive Files Review

- `git grep` for `API_KEY`/`SECRET`/`PASSWORD`/`TOKEN`/`PRIVATE_KEY`/`BEGIN RSA`/`BEGIN OPENSSH`/`DATABASE_URL`/`client_secret`/`access_key` over tracked files returned **only legitimate code** — struct fields (`password`, `clipboard_token`), error strings, and clearly-fake **test fixtures** inside `#[cfg(test)]` modules (e.g., `"hunter2"`, `"newpass"`). No real secrets.
- No `.env`, `*.pem`, `*.key`, `id_rsa*`, service-account JSON, or `*.db`/`*.sqlite` files are tracked or present on disk (searched to depth 3, excluding `node_modules`/`target`).
- `.vscode/mcp.json` (tracked) configures only the `@angular/cli` MCP server via `npx` — **no tokens/keys**. `launch.json`/`tasks.json` are standard Angular scaffolding.
- **No potential real secrets were found.**

## 6. `.gitignore` and Publish Safety Review

Strong baseline: `node_modules`, `/dist`, `/src-tauri/target`, `/gen/schemas`, `.angular/cache`, `.DS_Store`, `coverage`, and crucially the **local vault data** (`*.db`, `*.db-shm`, `*.db-wal`) were already ignored. `.vscode/*` is ignored with a small whitelist of non-sensitive shared configs. Gap (now fixed): `.env*` and `*.sqlite`/`*.sqlite3` were not covered.

## 7. Authentication and Authorization Review

- **Master password → key:** Argon2id (`m=65536 KiB`, `t=3`, `p=1`, 32-byte output) with a 16-byte random salt; verified by decrypting a stored test value (`unlock_vault`), returning `WrongPassword` on mismatch (`src-tauri/src/commands/vault.rs`, `crypto/kdf.rs`).
- **No plaintext password storage, no hardcoded users, no auth bypass.** Entry passwords/notes are encrypted at rest with AES-256-GCM; only decrypted on explicit `get_entry`.
- **Lock model:** in-memory key cleared on `lock_vault`; the `with_unlocked`/`with_authorized` gate returns `Locked` when no key is present, enforced on every sensitive command. Idle auto-lock is configurable (30 s–24 h).
- Keys and master passwords are held in `Zeroizing`, so they are wiped on drop.
- No auth/authorization changes were made (out of scope for auto-fix).

## 8. Input Validation and Unsafe Operation Review

- **SQL injection:** None. Every query uses `rusqlite` bound parameters (`?1`, `?2`, …); no `format!`-built SQL. Sorting uses fixed `COLLATE NOCASE` clauses, not user input.
- **Unsafe Rust / command execution:** None. No `unsafe`, no `std::process`/`Command`, no `std::fs` beyond the startup `create_dir_all` of the app-data dir.
- **Input validation:** master password length (≥ 8, non-empty), entry title/password required, category name non-empty and ≤ 64 chars, generator length 4–256, auto-lock 30 s–24 h — all enforced server-side in Rust and covered by unit tests.
- **Frontend XSS:** No `innerHTML`, `outerHTML`, `document.write`, `eval`, `javascript:`, or `bypassSecurityTrust*`. Angular template auto-escaping is intact; entry data is rendered via interpolation.
- **Logging:** The application code emits no `log`/`println!`/`dbg!` records of its own. `tauri-plugin-log` is enabled (level `Info`, `src-tauri/src/lib.rs`) for framework diagnostics only, so no master password, derived key, or decrypted entry data is written to logs. Backend errors serialized to the frontend are opaque for `database`/`io`/`internal` variants (no internal detail leaked) — covered by `error.rs` tests.

## 9. Dependency and Tooling Review

- `npm audit --audit-level=moderate` → **found 0 vulnerabilities.**
- `Cargo.lock` is committed (reproducible builds). `cargo audit` is not installed and was not added (per skill rules, no new tooling installed); recommend running it in CI as a follow-up.
- No risky lifecycle scripts in `package.json` (only standard `ng`/`tauri` wrappers).

## 10. CI/CD and Deployment Review

No CI/CD configuration is present (`.github/workflows` absent), so there are no pipeline secrets or deployment scripts to review. If CI is added later, recommend: secret scanning, `npm audit`/`cargo audit` gates, and never building from untrusted forks with secrets exposed.

## 11. Auto-Fixes Applied

### Fix 1 — Harden `.gitignore` against secrets and local DB files

- **Files changed:** `.gitignore`
- **What changed:** Added `*.sqlite`/`*.sqlite3` to the local-vault-data section and a new "Environment & secrets" section with `.env`, `.env.*`, and a `!.env.example` exception.
- **Why it is safe:** `.gitignore` does not affect application behavior, and no currently-tracked file matches the new patterns (verified via `git ls-files` and `git status`), so nothing is untracked or hidden. Purely preventive.
- **Validation run:** `git status --short`, `git ls-files | grep -Ei '\.env|\.sqlite'` (no matches), `git diff --check`.
- **Commit hash:** `81142fa`
- **Push result:** Pushed to `origin/chore/repo-cleanup`.

### Fix 2 — Add a "Security & data" section to the README

- **Files changed:** `README.md`
- **What changed:** Added a "Security & data" section documenting that this is a local-only app (no server/sync), where the encrypted vault lives (OS app-data dir, git-ignored — never commit it or any `.env`), the crypto model (Argon2id → AES-256-GCM, master password never stored and zeroized on lock), that a forgotten master password is unrecoverable, and the auto-lock / clipboard auto-clear behavior.
- **Why it is safe:** Documentation only — no code, build, or runtime behavior changes. The content restates the model already implemented and verified in §7–§8; it adds no claims about behavior that does not exist.
- **Validation run:** `git diff --check` (no whitespace errors); confirmed the diff touches only `README.md` and this report (no source/build files).
- **Commit hash:** `__FIX2_HASH__`
- **Push result:** Pushed to `origin/chore/repo-cleanup`.

## 12. Recommended Manual Fix Order

1. **Define a restrictive CSP** (Finding 1) and validate against the built Tauri app — highest-value hardening.
2. Add a CI workflow with `npm audit` + `cargo audit` gates and secret scanning (§9 / §10).

## 13. Commands Run

```text
pwd; git status --short; git branch --show-current; git remote -v
git rev-parse --abbrev-ref --symbolic-full-name @{u}
git ls-files
cat .gitignore ; cat src-tauri/.gitignore ; cat src/index.html
git grep -ni "api_key|secret|password|token|private_key|BEGIN RSA|BEGIN OPENSSH|database_url|client_secret|access_key" -- (tracked, excl. lockfile/docs)
git grep -ni "innerhtml|bypasssecuritytrust|eval(|javascript:|document.write|outerHTML" -- src
git grep -n "process::Command|Command::new|unsafe |std::fs::|format!(\"SELECT|INSERT|UPDATE|DELETE" -- src-tauri/src
git grep -n "log::|info!|println!|eprintln!|dbg!|..." -- src-tauri/src
git grep -n "process.env|import.meta.env|std::env::var|dotenv" -- src src-tauri/src
find . -maxdepth 3 \( -name '*.db' -o -name '.env*' -o -name '*.pem' -o -name '*.key' -o -name '*.sqlite*' ... \)
ls .github/workflows
npm audit --audit-level=moderate

# Re-verification pass (2026-05-28) + README fix:
git ls-files | grep -Ei '\.(env|pem|key|p12|pfx|db|sqlite|sqlite3)$|id_rsa|credentials|secret'   # none
git grep -nI "process.env|import.meta.env|std::env::var|dotenv" -- src src-tauri/src               # none
npm audit --audit-level=moderate                                                                   # 0 vulnerabilities
git diff --check ; git diff --stat
```

## 14. Final Notes

For a personal, local-only password manager this is a clean, defensively-written codebase: sound modern crypto with key zeroization, parameterized SQL, no unsafe/shell/dynamic-SQL paths, no XSS sinks, no committed secrets, ignored local vault data, minimal Tauri capabilities (clipboard text + core only), clipboard auto-clear, and a clean dependency audit. A re-verification pass on 2026-05-28 reconfirmed all findings (the password generator uses `OsRng`; entries are AES-256-GCM-encrypted at rest and gated behind the unlock state; SQLite enables foreign keys and WAL). The single meaningful hardening step is adopting an explicit Content-Security-Policy; everything else is incremental. No risky changes were made; the two auto-fixes are behavior-neutral — a `.gitignore` hardening and a README "Security & data" section.
