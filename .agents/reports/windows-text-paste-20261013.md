# Windows explicit text paste — 2026-10-13

## Status

Implementation and Windows noninteractive verification completed. **Interactive exact-text / IME / DingTalk acceptance is NOT complete.** The original user host still has an established client and owns the desktop writer lease. The isolated candidate's child returned `lease_conflict`; no lock bypass, original process termination, original binary replacement or real-chat sends were performed.

## Contract and implementation

- `text_input` adds optional `method: "native" | "paste"`, default `native`.
- Paste is explicit and Windows-only. The host writes CF_UNICODETEXT and dispatches tracked Ctrl+V itself; callers no longer need SSH clipboard writes.
- Whole-payload preflight, max4096 Unicode scalars, forbidden-control rejection and empty no-op. Tabs/newlines are text, not Tab/Enter events; Windows line endings normalize to CRLF.
- Paste overwrites the shared clipboard and leaves the requested text there. No timed restoration, previous-format reads, automatic retries, hidden native/paste fallback or success claims based on SendInput alone.
- Reject held session input/external modifiers/V; fail fast on clipboard busy; verify foreground/focused-control identity and clipboard sequence. Races with simultaneous user input remain best-effort and require serialized desktop control.
- Win32 allocation, clipboard lock and owner window use scoped ownership. Chord failures retain attempted keys for existing release_all. Errors disclose clipboard mutation and uncertainty.
- Independent review caught a pre-CloseClipboard sequence-baseline issue. Repaired by retaining own hidden HWND, reopening after initial close, proving ownership BEFORE bounded readback of only the text just published, verifying exact UTF-16 including NUL, then taking/guarding the finalized sequence. Foreign clipboard data is not read or adopted. Hidden owner destruction is explicit before chord dispatch.
- Raw rpa-native-input remains native-only. Paste tests do not certify the unresolved original native/IME path.

## Verification evidence

Local evidence root (ignored): `.agents/runs/clipboard-20261013/`.

| Gate | Actual outcome | Evidence |
|---|---|---|
| Frozen HEAD new-contract RED | 3 expected assertions fail, native101 | red-0/result/{summary.json,stdout.log}; cc-red-glm.jsonl |
| Final paste integration contract | 12 pass, native0 | green-release-0/result/; cc-final-tests.jsonl |
| Final optimized Windows library | 291 pass, 0 fail, 7 ignored;6.16s; native0 | green-release-1/result/; cc-final-tests.jsonl |
| Real Win32 clipboard lifecycle | 1 pass, native0; finalized sequence5, after-owner-destruction sequence5; exact UTF-16 retained | lifecycle-0/result/; cc-final-tests.jsonl |
| Standalone fixture compile | Windows .NET Framework csc success | fixture-build.stdout/stderr |
| Fixture non-GUI self-tests | native0 | fixture-0/result/; cc-fixture.jsonl |
| Windows all-tests cross-check | exit0 | final-cross-check.log |
| Scoped rustfmt / diff check | pass | format-check-final.log; final direct git diff --check |
| Independent repair review | no remaining critical/important finding | coordinator's Poincare review messages |
| Real desktop/IME/DingTalk | BLOCKED, not passed | host-diagnostic-ssh.log, cc-connection-ssh.jsonl |

Native lifecycle is deliberately opt-in/ignored in the ordinary suite. Before any clipboard operation it requires **SessionId==0 and nonvisible window station**; it fails closed if either query fails. It invokes the production publication/finalization/verification/disposal helpers, then checks its known rendered text after owner destruction. It injects no input and does not access the interactive user's clipboard.

All accepted CC execution transcripts were checked at **assistant message.model**, which is `glm-5.3-flash` with `--model sonnet`. Direct `--model glm-5.3-flash` was unexpectedly routed to kimi-k3 despite the final modelUsage request label; that initial command was not executed (exact command approval mismatch), and `cc-red.jsonl` is excluded from test evidence. No global model/settings changes were made.

### Preserved failed attempts

- Initial new-contract run:11 pass/1 failure from an incorrect TEST expectation. Existing runtime maps unknown backend `paste_failed` to protocol `input_error`. Corrected only the test expectation and added preservation-of-diagnostic assertion; subsequent final12 pass.
- Initial full library debug run hit its **90-second** runner budget while existing PNG-heavy `result_metadata_retained_for_all_registered_requests_images_trimmed` was still executing. Not proof of a deadlock, and not a passing run. Rebuilt optimized with unchanged suite; all291 nonignored tests completed in6.16s. Claude's earlier summary incorrectly described120seconds; raw config/summary take precedence.
- Initial direct Mac→candidate18413 TCP connectivity failed (cause not established). Without changing firewall rules, diagnostic MCP transport used SSH stdio→Windows computer-client→TLS candidate host in Session1. Authentication succeeded and the host explicitly rejected its child for lease_conflict. This is not a clipboard workaround or interactive acceptance.

## Artifacts and cleanup

- Local final release: `target/x86_64-pc-windows-msvc/release/computer-host.exe`.
- Staged final Windows candidate: `C:\Temp\rpa-text-paste-20261013-7b01338f\bin\computer-host.exe`.
- SHA256 manifest: `.agents/runs/clipboard-20261013/final-hashes.json`; remote hash/cleanup: `final-remote-state.json`.
- Owned candidate task `RpaTextPaste-7b01338f` and its root host were stopped using exact path/PID/creation-time identity checks. Its isolated artifacts/evidence remain. Original `D:\workspace\datagrand\rpa-computer-test\computer-host.exe` processes were not modified or terminated.
- GUI fixture authored but NOT launched; native compile/self-test does not establish rendering/IME correctness.
- No commits, pushes, worktrees, macOS GUI or Linux tests.

## Pending handoff

User should disconnect the client holding the original host's desktop lease. Then restart only the owned isolated candidate, launch the owned text-paste fixture, and use screenshot-only Claude CLI+GLM via its MCP tools for four first-attempt cases (Chinese, mixed ASCII/punctuation, emoji, multiline/tab), checking exact fixture evidence and queried IME state separately. Never send test messages to real contacts. Only after this gate should original host replacement be considered; it has not been performed here.

## Fresh lease investigation (2026-10-13 18:47–18:49 +08:00)

User requested a fresh check rather than relying on the previous lease conflict. Read-only process/socket inventory confirmed original listener PID28772 and stdio child PID28052 remain in interactive Session1. Port8399 has an established peer `100.200.20.163:54141`. Local socket inspection identified this peer as computer-client PID89476, parent Claude CLI PID83909, launched from a VS Code terminal (zsh PID83170). No command-line secrets were collected.

A non-mutating open of the existing `C:\Users\Administrator\AppData\Local\Temp\computer-host-Administrator.lock` followed by an immediate one-byte lock attempt failed with Win32 error33 (ERROR_LOCK_VIOLATION). This confirms a current OS-held lock, not merely a stale file. The unlocked diagnostic tail after byte0 reads `id=28052`; together with process ancestry and socket evidence, this identifies the original stdio worker. Diagnostic contents alone were not used as proof. The probe neither wrote/deleted the file nor injected input nor changed the clipboard.

The actionable owner is an existing local Claude CLI MCP client, not Windows screen lock or clipboard contention. No original process was killed and no test candidate was restarted. Evidence: `lock-recheck-20261013-184755.json`, `lock-recheck-probe.json`, `lock-recheck-local-client.txt`. GUI acceptance remains pending disconnection of that client.
