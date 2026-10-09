# native-input-gui-helpers-20260929 — Coordinator-only GUI test helpers

Task: `.agents/tasks/native-input-gui-helpers-20260929.md`. All new files are
beneath `.agents/runs/native-gui-helpers-20260929/`. No product source was
touched; nothing was launched live, no GUI/SSH was driven, no config changed.

## Delivered

| File | Purpose |
|---|---|
| `macos-backdrop.swift` | macOS AppKit neutral desktop backdrop helper (source) |
| `macos-backdrop` | Compiled binary (swiftc, non-live) |
| `gui-runner-launch-20260929.ps1` | Generic bounded Interactive-ScheduledTask launcher |
| `gui-runner-stop-20260929.ps1` | Verified cleanup for records produced by the launcher |
| `ps-structural-check.py` | Offline quote/brace/here-string structural checker (no pwsh on this host) |

## 1. macOS backdrop helper (`macos-backdrop.swift`)

- Args: `--fixture-pid <PID>` (required), `--fixture-exe <ABS PATH>` (required,
  validated), `--max-minutes N` (default 20, hard-capped at 20).
- Validates BEFORE creating any UI: pid must be live, its executable path must
  equal `--fixture-exe` (standardized and symlink-resolved). Mismatch exits 5;
  bad args exit 2; dead pid exits 3; headless (no NSScreen) exits 6.
- Non-live validation performed: compiled clean with `swiftc -O`; ran the two
  invalid-identity paths (`exit=3` for pid 1, `exit=2` for missing args) —
  neither created a window.
- One opaque borderless neutral (pale gray 0.94 white) `NSWindow` per
  `NSScreen`, covering the FULL `screen.frame`, level `.normal`
  (`hidesOnDeactivate=false`). No user window is moved/minimized/closed.
- After ordering front, activates ONLY the fixture app (`activate()` on 14+,
  plus the deprecated `activate(options: [.activateAllWindows])` fallback) so
  the fixture sits above the backdrop and other regular apps. No elevation,
  no new permissions, no TCC change — per task, "normal level + fixture
  activate" is the accepted mechanism; activation success is best-effort and
  honestly reported by whatever the coordinator observes on screen.
- Bounded lifetime: self-terminates at `--max-minutes` and every 5 s if the
  fixture process disappears. SIGTERM handled for clean coordinator stop.
- Own PID printed to stdout first (`BACKDROP_PID=<pid>`), before the run loop.
  Reads nothing from disk, no clipboard, no network, no synthesized input.

Coordinator usage (local desktop only):

```
.agents/runs/native-gui-helpers-20260929/macos-backdrop \
  --fixture-pid <FIXTURE_PID> --fixture-exe <ABSOLUTE FIXTURE PATH>
```

Cleanup is kill-EXACT-pid only, after identity verification — no pkill:

```
ps -o comm=,lstart= -p <BACKDROP_PID>   # must match the compiled helper path & start time
kill -TERM <BACKDROP_PID>
```

## 2. Windows launcher (`gui-runner-launch-20260929.ps1`)

Generic, pass-through (injects nothing), bounded, Interactive-session only:

- Params: `-ScriptPath`, `-EvidenceDir` (mandatory), `-ScriptArgs[]`,
  `-ExpiryMinutes` (default 30, 1..60), `-RecordDir` (optional, defaults to
  EvidenceDir). Stdout/stderr redirect support was REMOVED in the 20260929
  review fix (a supplying caller fails via parameter binding); only the safe
  direct `-File` invocation is used — the coordinator reads the inner
  runner's own evidence logs.
- Fail-closed validation: control characters, single quotes, and double
  quotes are each rejected INDIVIDUALLY (explicit `$s.Contains(...)`
  checks — the earlier combined char-class regex was replaced after review
  flagged its quoting as suspicious); ScriptPath must be an explicit
  ABSOLUTE existing `.ps1`, not a reparse point; path-like ScriptArgs
  (drive-letter rooted) must exist; EvidenceDir/RecordDir absolute.
- Session 0 is REFUSED explicitly: the console session id must be neither
  `0` nor `0xFFFFFFFF`, and an explorer.exe owner must exist in that
  session (task principal Interactive / RunLevel Limited). Never Session 0,
  never SSH Start-Process/WMI.
- GUID-unique task name `RpaGuiRunner-<guid>`; refuses overwrite of an
  existing task or record. Action executes `powershell.exe` DIRECTLY with
  the `-File` form. Nothing about the inner scripts is modified.
- Provisional instance record (`status=provisional`, pid 0) written BEFORE
  `Start-ScheduledTask`; upgraded to `healthy` only after the EXACT owned
  `powershell.exe` verifies within a bounded 20 s wait. The 20260929 review
  fix hardened the identity predicate: it now requires, ALL of — name,
  `ExecutablePath`, console session, created at/after start, AND a
  `CommandLine` CONTAINING the full unique script invocation `argList` — so
  an unrelated powershell in the same session/time window can never be
  recorded. If MORE THAN ONE process matches, the result is ambiguous:
  record marked `unhealthy` with reason `ambiguous_process`, exit 1, nothing
  killed. On any failure the record is marked `unhealthy` (with reason) and
  the script exits WITHOUT killing anything.
- The verified process's actual `CommandLine` is recorded as `command_line`
  and is what the stop script re-verifies by EXACT equality before any stop.
- Bounded expiry: task `ExecutionTimeLimit` = `-ExpiryMinutes` (30 default);
  record carries `expiry_utc`. Prints task name, verified pid, record path.
- Record fields (superset of the remote-host pattern): `task_name, status,
  pid, session_id, exe, creation_date_utc, creation_ticks_utc, command_line,
  script_path, script_args, action_execute, action_arguments, principal_user,
  principal_sid, principal_logon_type, principal_run_level, evidence_dir,
  expiry_minutes, expiry_utc, started_utc, unhealthy_reason`.

## 3. Windows cleanup (`gui-runner-stop-20260929.ps1`)

Same verified-identity discipline as `scripts/windows-remote-stop.ps1`,
generalized to the runner record:

- Requires ALL identity fields (including the new `command_line`); refuses
  unparseable/incomplete records.
- Process check (when pid ≠ 0): `command_line` must be present in the
  record; live process name must equal the leaf of `exe`, with EXACT
  `ExecutablePath`, `SessionId`, EXACT UTC-tick `CreationDate`, AND EXACT
  `CommandLine` equality against the recorded `command_line` (PID-reuse safe
  and immune to unrelated same-session powershells). Provisional record →
  task cleanup only, nothing is killed.
- Task check: exactly one action whose Execute/Arguments match the record
  verbatim; principal resolves (via SID equivalence, never name text) to the
  recorded `principal_sid`; LogonType Interactive; RunLevel matches.
- Any mismatch → exit 1, NOTHING stopped/unregistered.
- Only then: Stop-ScheduledTask → bounded 15 s wait → full identity
  revalidation of a still-live pid → `Stop-Process -Force` on THAT pid as
  last resort → Unregister task → remove ONLY the record file. Evidence,
  transcripts, stdout/stderr captures are preserved.

## Verification status (honest)

- macOS helper: compiled clean (`swiftc -O`, macOS 26.3.1 host) and the
  pre-UI validation paths were exercised non-live (exits 2/3, no window).
  The GUI path itself was NOT run (task forbids live GUI input).
- PowerShell scripts: NOT parsed by a real PowerShell engine — this macOS
  host has no pwsh and the task forbids SSH. `ps-structural-check.py`
  (tokenizer for here-strings/quotes/comments/backtick escapes + bracket
  balance) passes on both scripts after the 20260929 review fixes, and
  cross-validates clean on the two known-good references
  (`gui-neutral-desktop-20260928.ps1`, `gui-remote-agent-glm-20260929.ps1`).
  This proves quoting/bracing is well-formed only; first live use on
  acer-win should begin with a parameter-validation dry run (e.g. launch
  once with a deliberately relative `-ScriptPath` and confirm the refusal
  path).
- Review fixes applied 20260929 (task
  `native-input-helper-fix-20260929.md`), before any live use: (1) process
  verification now matches the full unique `argList` inside `CommandLine`
  and fails closed as `ambiguous_process` when more than one process
  matches — an unrelated same-session/time powershell can no longer be
  recorded; (2) the stop script requires and re-verifies the recorded
  `command_line` by EXACT equality before stop and again before force-kill;
  (3) Session 0 is refused explicitly (session id 0 or 0xFFFFFFFF);
  (4) input validation rejects control chars and each quote type
  individually; (5) stdout/stderr redirect (`-Command`) support removed —
  direct `-File` invocation only.
- The scheduled-task registration was not executed live; it follows
  patterns already proven by `windows-remote-start.ps1` /
  `windows-remote-stop.ps1`.

## Exact coordinator commands (Windows, via ssh acer-win)

Deploy both ps1 files under e.g. `C:\rpa-remote\gui\`, then:

```
ssh acer-win powershell -NoProfile -ExecutionPolicy Bypass -File ^
  C:\rpa-remote\gui\gui-runner-launch-20260929.ps1 ^
  -ScriptPath C:\rpa-remote\gui-remote-agent-glm-20260929.ps1 ^
  -EvidenceDir C:\rpa-remote\gui-evidence ^
  -ScriptArgs -ReleaseDir,C:\rpa-remote\release,-TaskFile,C:\rpa-remote\task.md
```

(stop uses the record path printed by the launcher; evidence is preserved)

```
ssh acer-win powershell -NoProfile -ExecutionPolicy Bypass -File ^
  C:\rpa-remote\gui\gui-runner-stop-20260929.ps1 ^
  -RecordPath C:\rpa-remote\gui-evidence\runner-instance-<TASK>.json
```

Note `-ScriptArgs` above is a PowerShell string array passed as one comma
list; every path-like element is validated to exist before registration.
