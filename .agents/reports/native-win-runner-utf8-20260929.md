# native-win-runner-utf8-20260929 — Diagnostic runner UTF8 stdin fix

Task: `.agents/tasks/native-win-runner-utf8-20260929.md`. New files only; the
original `gui-remote-agent-glm-20260929.ps1` was NOT modified (a live run
still uses it). No product files, no global config, no SSH/GUI/commit.

## Delivered

| File | Purpose |
|---|---|
| `.agents/runs/gui-remote-agent-glm-utf8-20260929.ps1` | UTF8-stdin fix variant of the GLM runner |
| `.agents/runs/stdin-utf8-roundtrip-probe-20260929.ps1` | Tiny no-GUI stdin byte roundtrip probe |

## Runner fixes (`gui-remote-agent-glm-utf8-20260929.ps1`)

Verified by `diff` against the original: ONLY the changes below differ
(everything else — args, allowed tools, system prompt, bounded drain/timeout,
owned-PID identity-recheck kill, summary structure — is byte-identical logic).

1. **UTF8 stdin, no OEM transcode path**: the task text is encoded once as
   `$taskBytes = [System.Text.Encoding]::UTF8.GetBytes($taskText)` (UTF-8,
   no BOM) and written verbatim via
   `proc.StandardInput.BaseStream.Write(bytes,0,len)` → `Flush()` →
   `Close()`. The default StreamWriter is NEVER used for text, so the
   observed console encoding (coordinator evidence: `[Console]::InputEncoding
   = ibm850`) can no longer transcode a Unicode prompt before Claude reads
   it.
2. **Forensic summary fields** (no prompt contents or secrets printed):
   `prompt_encoding = 'utf-8'`, `prompt_sha256` (SHA-256 over the exact bytes
   sent), `prompt_bytes`, `default_stdin_encoding` (captured from
   `proc.StandardInput.Encoding.WebName` — what the OLD path would have
   used), and `max_thinking_tokens`.
3. **Coordinator-visible progress**: transcript and stderr streams are opened
   with explicit `FileMode.Create / FileAccess.Write / FileShare.Read`
   instead of `[IO.File]::Create`, so the coordinator can tail/collect them
   while the run is active.
4. **Bounded thinking latency, child-only**:
   `$psi.EnvironmentVariables['MAX_THINKING_TOKENS'] = '2048'` is set on this
   child process only — no global model/config change, still `--model sonnet`
   (GLM glm-5.3-flash route). Recorded in the run summary; the GLM response
   remains verifiable in the transcript.

## Probe (`stdin-utf8-roundtrip-probe-20260929.ps1`)

No GUI, no Claude/model invocation — spawns only two short-lived child
`powershell.exe` processes reading their own stdin. Coordinator runs it
remotely on the Windows host:

```
powershell -NoProfile -ExecutionPolicy Bypass -File \
  <dir>\stdin-utf8-roundtrip-probe-20260929.ps1
```

- Parent sends a fixed prompt containing CJK (`你好，世界`), a non-BMP
  character (`🌍` U+1F30D), fullwidth Latin (`ＦＵＬＬＷＩＤＴＨ`), and
  `✅` — encoded to UTF-8 bytes and written through the SAME mechanism as the
  fixed runner (raw BaseStream bytes, never the StreamWriter).
- Child mode `-ChildRaw` reads stdin as RAW bytes (what node/claude does) and
  returns byte count + SHA-256. **PASS criterion: received bytes == sent
  bytes (sha256 equality).** Exit 0 on PASS, 1 on FAIL.
- Child mode `-ChildConsole` is forensic contrast only: it reads stdin via
  `[Console]::In` (the OEM-risk decoding the old runner's path implied),
  reports the observed `[Console]::InputEncoding.WebName` (expected `ibm850`
  on the target host) and the SHA-256 of what it decoded — expected to DIFFER
  from the sent hash, demonstrating why the fix is necessary. The verdict is
  based solely on `raw_match`.
- Output is a single JSON verdict line (`verdict`, `sent_sha256`,
  `raw_*`, `console_*`, note); no secrets.

## Verification status (honest)

- Both new scripts pass the offline structural checker
  (`.agents/runs/native-gui-helpers-20260929/ps-structural-check.py`:
  here-strings/quotes/comments tokenizing + bracket balance). No real
  PowerShell engine is available on this macOS host and SSH is forbidden by
  the task, so a live engine parse and the probe run itself are pending on
  the Windows host — the probe is designed to be the first remote command.
- The runner variant has not been executed (no GUI/SSH per task); its
  bounded-drain/timeout/owned-PID semantics are unchanged from the live-used
  original, so the only behavioral deltas are the four documented fixes.

## Coordinator quick reference

```
# 1) probe first (proves byte fidelity on the target console)
ssh acer-win powershell -NoProfile -ExecutionPolicy Bypass -File ^
  C:\rpa-remote\stdin-utf8-roundtrip-probe-20260929.ps1

# 2) then run the fixed runner instead of the original
ssh acer-win powershell -NoProfile -ExecutionPolicy Bypass -File ^
  C:\rpa-remote\gui-remote-agent-glm-utf8-20260929.ps1 ^
  -ReleaseDir C:\rpa-remote\release -EvidenceDir C:\rpa-remote\gui-evidence ^
  -TaskFile C:\rpa-remote\task.md
```

The run summary in the evidence GUID dir now carries `prompt_sha256` — a
diagnostic prompt containing Unicode can be verified as received intact by
the transcript, and `default_stdin_encoding` documents the OEM risk that was
removed.
