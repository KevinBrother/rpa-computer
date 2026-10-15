# Continued Windows acceptance — October 14, 2026

Status: follow-up execution complete. DingTalk self-chat input/send, seven explicit
ignored cases and full-width IMM paste-boundary coverage obtained. Screenshot-only
fixture transcription remains2/4, with both failures preserved and attributed below.
Tool-layer submitted-text fidelity is4/4; model-generated argument errors are
recorded separately and do not block this deterministic input acceptance.
The separate eighth skipped case (1000-step registry contract) passed on October15;
see windows-registry-20261015.md. Original deployment remains unchanged.

Evidence: `.agents/runs/control-followup-20261014/`.
Baseline HEAD externally changed to `670169b`; coordinator did not commit/push.
Original production deployment is untouched; same isolated host root/port18413.
All test execution uses actual Claude CLI + GLM. No Mac/Linux tests.

## DingTalk self-chat: one real paste and one Enter send

`cc-dingtalk-inspect.jsonl` located the visibly self-only file-helper chat and
confirmed empty composer. `cc-dingtalk-paste.jsonl` independently confirmed the
same self-chat/empty composer, clicked it, submitted one explicit method=paste
payload containing Chinese, ASCII punctuation/numbers, emoji, CRLF lines and TAB,
then used one60ms Enter key_hold. Post-send screenshot shows the new self-message
with `[RPA自测-20261014]` marker and cleared composer. All input steps dispatched;
control closed/released. Screenshots are local evidence, not republished because
unrelated existing chat UI is visible. No other recipient, no retry or deletion.

This is real application visible input/send evidence, NOT exact invisible UTF16
proof or verified full-width-at-paste coverage. Task-specified synthetic payload
is independent of the fixture oracle. No shell/external clipboard was used by
the GUI operator. Actual response model tags must be verified at final review.

## Explicitly ignored tests: correct environment matters

Initial `cc-ignored-six.jsonl` ran clipboard-service + five feedback tests in SSH
Session0. Raw stdout:2passed/4failed/native101; no timeout or output truncation.
The CLI prose inferred an immediate startup crash; raw logs prove the tests ran.
Service clipboard lifecycle passed. Four feedback tests rejected SpawnFailed;
negative startup-rejection test passed in that environment. Existing production
preflight explicitly rejects Session0, so these feedback results do not satisfy
intended acceptance. No product workaround or protection bypass is being added.

Corrected run executes five feedback tests plus live capture-only diagnostic in
interactive Session1, without RPA_LIVE_TEST_ALLOW_SKIP. Prior failed evidence is
preserved, not overwritten. Existing bounded runner and reviewed interactive
launcher used; run-specific wrapper only sets/removes process-local environment
and verifies the frozen fake-renderer hash. Native capture has no input actions.

## Fixture transcription / IME evidence

Previous exact-fixture1/4 and submitted-text-fidelity4/4 results remain unchanged.
A bounded diagnostic fixture improvement is being authored: more readable
canonical escaped JSON for ambiguous glyphs, and metadata-only IME samples around
focus/key/WM_PASTE boundaries. Cases, expected bytes, exact matcher, original IME
setters and one-attempt rules must remain unchanged. No hidden answer delivery to
the screenshot-only operator. New suite remains pending until raw results exist.

## Verified additional results

Actual response model tags for reconnaissance, DingTalk input/send, initial
ignored-suite, corrected ignored-suite and fixture RED are `glm-5.3-flash`.

The first interactive-launch attempt had malformed Base64 in CC's generated
command; PowerShell rejected it before launch. Preserved as
`cc-interactive-ignored.jsonl`. Replaced long encoded commands in the operator
task with short -File calls to narrowly scoped authored wrappers. No global
configuration or product behavior changed.

Corrected `cc-interactive-ignored-v2.jsonl`: raw nativeexit0, root Session1,
6passed/0failed/0ignored in18.27s. Five feedback cases and live capture passed;
RPA_LIVE_TEST_ALLOW_SKIP was explicitly removed in process scope. Capture
reported physical1920x1080 geometry and valid1365x768 image. Combined with the
separate Session0 clipboard lifecycle pass, all7 previously ignored library
cases now have appropriate explicit passing evidence. This does not erase the
initial wrong-environment4fail result. Both suite binaries match frozen
SHA256c4067d73b106a4a3dab0aaeebbaa8accb7e56f74ac1c4602e25223ca7215b77e.

Interactive test launcher task RpaGuiRunner-b0815e0e71004b419dd8a703c0be76bd
cleaned by exact identity-verified stop script; evidence retained. Bounded runner
does not generically prove descendant cleanup; later inventory is required.

Fixture RED compile succeeded. Actual CC Windows self-test nativeexit1 with
exactly five intended new-contract failures and payload-preservation passing.
Source hash eb47aba4074d5b9b7a29763ceb73176b71d89a1763fc9cd4e4ecbaa2c6ec06ec.
No GUI was run from RED fixture. GREEN instrumentation/readability implementation
is in progress; product host/client binaries remain unchanged.

## Frozen diagnostic fixture GREEN

Source849lines SHA30e1067b5d13ac7100fb37d4bc9bb478c9ff7cf754b54471cbcde636239c4566;
binary E9C435619A657CB8A4682734D72B32D50EBB4F00606DADB8E33A9D571DCC1FFB.
Windows compile0 and actualCC/GLM selftest native0: all six new contract groups
and existing non-GUI tests passed. Exact frozen hashes recorded. Source-only
independent review found no GUI-launch blocker; minor caveat retained: a
secondary stderr failure in trace-write-failure logging is not itself guarded.
No new product host/client changes.

New GUI fixture PID21492/Session1, run fa6aa05204cd48a7b92f41992a8eeafb.
Operator task remains screenshot-only; input is derived only from large
canonical JSON, not supplied hidden case answers. It avoids redundant click
after Enable because the existing button already focuses the target, but adds
no IME reset/global shortcut. Exactly one paste and Check per case required.
`cc-fixture-observed.jsonl` is the new run; read trace/oracle only after it exits.

Trace records only own-context sampled IMM metadata at focus/key/WM_PASTE
boundaries, bounded512 entries, with dropped counts. ProcessCmdKey fallback
samples cannot establish uninterrupted insertion or arbitrary TSF behavior.
Actual GUI/oracle/trace results pending.

## Final observed GUI / full-width result

`cc-fixture-observed.jsonl` completed actual `glm-5.3-flash`, CLIexit0, four explicit
paste calls and four Checks, no retry/repair/native baseline. Closed fixture with
Escape and released control. Oracle/trace copied only after CLI exit.18 screenshots
retained; four durable ordered oracle rows agree with the visible results.

| Case | Exact expected match | Submitted MCP payload equals actual | Difference already present in submitted text |
|---|---|---|---|
| Chinese | PASS | Yes | None |
| Mixed ASCII | FAIL | Yes | Missing U+0060 backtick |
| Emoji | PASS | Yes | None |
| Multiline/TAB | FAIL | Yes | U+FF01 fullwidth exclamation instead of U+0021 |

Therefore **2/4 screenshot-task correctness**, and **4/4 exact submitted-text
fidelity**. No claim of four correct transcriptions. The earlier1/4 and its
original failure remain retained; this new diagnostic attempt is not a repair
of checked cases. Canonical text display improved readability but did not
eliminate GLM transcription errors. Final summary second line is partly clipped
at this DPI, although PASS/FAIL and2/4 remain readable; oracle is authoritative
for exact code-unit comparison.

**IME coverage is now observed at the actual native paste boundary:**
- Trace contains102 samples in4 Check batches, dropped=0 throughout.
- Each case has exactly one WM_PASTE (`message=770`) Before/After pair.
- All eight samples: availability=Available, open=true, conversion=9 (0x9),
  focus==target HWND, context acquired and released without reported failure.
- Ctrl+V and modifier-release samples likewise retain0x9.
- Later focus/check events differ (e.g. first case focus-reentry Before reports
  open=false/conversion0x400); Check oracle records0x401. Thus Check-time state
  did not describe the earlier paste state. Exact causality of later asynchronous
  IME changes is not inferred beyond those observed boundaries.

Evidence: `ime-paste-boundary-summary.json`, `observed-comparison.json`,
`observed-tool-results.json`, and the raw sidecar in
`run-fa6aa05204cd48a7b92f41992a8eeafb/`. This establishes queried IMM native/fullshape
at both native dispatch boundaries for this TextBox and these4 attempts—not
a universal TSF guarantee or proof of every intervening instant. Together with
verbatim input readback, no IME-induced mutation was observed in these pastes.

## Scope and cleanup

No product runtime changes in this follow-up. Only acceptance-fixture source/task
docs and this report changed; prior zero-sequence repair was already externally
committed as670169b. Some files were externally staged during the run; index state
was preserved. Coordinator performed no git add/commit/push/reset/worktree.

Fixture PID21492 exited by Escape; verified stop removed its owned task. Host
PID6848 stopped with identity-verified deployment script; record removed. GUI
runner task was already cleaned after its test. Original deployment and user
apps are untouched. DingTalk remains open; the ONE synthetic self-test message
was intentionally retained, not deleted or resent.

Final read-only cleanup inventory (`final-inventory.json`, exit0) confirmed no
process under the owned root or the two exact test-PID fake-renderer temp prefixes,
no remaining current host/fixture/runner task, no18413 listener, and no instance
record. Frozen fixture source/task hashes remain unchanged after testing;
`git diff HEAD --check` passed, including staged and unstaged changes.
