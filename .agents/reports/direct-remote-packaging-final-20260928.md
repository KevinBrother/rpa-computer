# Direct-Remote Packaging Final Followup — 2026-09-28

Scope: `scripts/windows-remote-start.ps1`, `tests/remote_packaging.py`, `docs/remote-connection.md` only. No GUI/SSH/config changes; no commit/push; no new features. Runtime function semantics untouched — only the TEST assertions and one test-hygiene bug in the ps1 source were fixed.

## 1. Independent run ERROR at line 496 (StopIteration) — FIXED (test-side)

**Root cause:** `test_start_writes_provisional_record_before_start_and_unhealthy_first` located the literal word `mark_unhealthy` with `next(...)` over **comment-stripped** code lines. `mark_unhealthy` appears only in a comment in the ps1, so `next()` threw `StopIteration` — and even where it matched a comment, the "record update precedes failure exit" assertion was vacuous.

**Fix (test only):** replaced the marker-string check with a **semantic ordering assertion on actual code calls** in `tests/remote_packaging.py`:

- `$recordObj.status = 'unhealthy'` (regex `\.status\s*=\s*'unhealthy'`) →
- `Set-Content -LiteralPath $record` (record rewrite) →
- `exit 1` (failure exit),

each located with `next()` over comment-stripped lines and asserted strictly ordered. This asserts the real behavior (unhealthy record persisted before exit) rather than the presence of a comment word. The ps1 failure-path logic itself was correct and is unchanged.

## 2. Literal NUL/control bytes in windows-remote-start.ps1 — FIXED (test-introduced bug in source)

A previous Python-side edit wrote the raw characters from a `\x00-\x1f` escape into the parameter-validation regex, embedding literal `0x00` and `0x1f` bytes: the file registered as binary (`grep: Binary file ... matches`) and would break PowerShell parsing on the target.

- Replaced the literal range with ASCII escapes: `if ($v -match '[\x00-\x1f"]') {` (PowerShell regex supports `\xNN`; semantics identical).
- Verified zero bytes below 0x20 other than tab/LF remain in the file.
- **New test** `test_ps_scripts_contain_no_control_characters`: both ps1 files must contain no bytes `< 0x20` except tab/CR/LF — guards against any future tool re-introducing raw control bytes.

## 3. `evil-release/` artifact at repo root — CLEANED UP, test hardened

- **Validated before removal:** `RELEASE.json` had `"kind": "computer-remote-client"`; `bin/computer-client` was the known `TEST-ONLY fake computer-client` shell fixture; `ca_cert_path`/`token_file_path` pointed into a `/var/folders/.../rpa-pkg-test-*` temp dir from a prior test run. Confirmed test-owned, then removed.
- **Test fix:** `test_refuses_source_overlap` previously leaked `evil-release/` into the repo root on a RED run (assertion failed before any cleanup). It now wraps the assertions in `try/finally` and removes **only** that exact directory, and **only** while its `RELEASE.json` still parses with `kind == "computer-remote-client"` — never arbitrary user paths. Verified: after this full test run, `evil-release/` does not exist.

## 4. Docs (`docs/remote-connection.md`)

- Added §1b "构建二进制": `cargo build --release --bins` with expected artifacts.
- Documented the **optional Mac-specific** cross-compile workaround (no new installation needed — existing `rust-lld` provides `-flavor link` / `lib` compatibility): `cargo xwin env`, `DYLD_LIBRARY_PATH` from `rustc --print sysroot`, `AR_x86_64_pc_windows_msvc=<sysroot>/lib/rustlib/<host-triple>/bin/rust-lld`, `ARFLAGS='-flavor link /lib'`, then `cargo build --release --target x86_64-pc-windows-msvc --bins --offline`. Explicitly marked Mac-specific/non-universal; native Windows `cargo build` with a standard toolchain works as-is. No hardcoded user home (host triple derived from `rustc -vV`).

## 5. Verification (actual, not claimed)

- Command: `python3 tests/remote_packaging.py` (true exit code captured, no pipe-hiding).
- Log: `.agents/runs/direct-remote-packaging-independent2-20260928.log`.
- **Result: EXIT=0 — Ran 30 tests, OK (skipped=1).** (29 pass + 1 new control-byte test pass = 30 total.)
- **SKIP (explicit):** `test_powershell_parser_if_available` — "no PowerShell available for parse check" (`pwsh`/`powershell` absent locally). Coordinator will scp + parse on the Windows target; note the earlier failure there was an EncodedCommand length limit, **not** a script parse error.
- Prior failing test now passes with semantic assertions; ps1 file is clean ASCII text again.

## Not done / out of scope

- No live Windows deployment, no GUI acceptance, no firewall/SSH changes.
- No commit or push (per repo rules; awaiting human review).
