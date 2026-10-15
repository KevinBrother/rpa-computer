# Windows live desktop acceptance — 2026-10-14

Status: running; no GUI acceptance conclusion yet.

## Authorization and isolation

User confirmed a fresh interactive login and explicitly requested real interaction
testing. Read-only preflight found console Session1 active, explorer PID6320,
no computer-host/computer-client/TextPasteFixture processes, and port18413 free.
User was asked not to operate mouse/keyboard during the run. No real DingTalk
contacts/messages are used; actions are restricted to the owned test window.

User committed the preceding implementation externally: current HEADd464c27
(`feat: multil session connect`). All Rust/Cargo inputs still match the prior
source-freeze-v2.json. Coordinator made no commit/push.

Owned release: C:\Temp\rpa-control-gui-20261014-67542ffc.
Host SHA256 ab259ddf0a10239ee85b142c533919465778c8d508a0ebf9712db0e3b9068e65,
identical to the final470-test regression candidate. Host launched in interactive
Session1 as PID6800; fixture as PID23292 with a fresh GUID-unique evidence path.
Fixture SHA256756035dd268a4de369893bf113dbcf9a38692a2f9f80a96ea5270f97be164d77
matches the previously compiled/self-tested immutable fixture.

Two MCP aliases are separate persistent SSH-stdio/TLS client connections to the
same interactive host. Actual screenshot-only GUI operator: Claude CLI, model
flag sonnet, actual assistant tag glm-5.3-flash. Only computer_a/computer_b MCP
tools exposed, no built-in tools; hooks disabled for this invocation, no global
configuration changes. Operator runs from an isolated release directory with an
OS sandbox denying reads/writes to repository source. A Cargo.toml read probe
returned Operation not permitted before starting the operator. The operator was
not given sample strings, source, or answer-bearing oracle files.

## Intended live checks

- Both connections discover without reserving desktop authority.
- A open; B structured conflict yet remains connected.
- A pause retains ownership; A resume requires fresh observation.
- First two text cases on A; verified close; same B transport acquires.
- Old A close is rejected; B remains usable and observes afresh.
- Last two cases on B; Escape closes only the owned fixture; B close and status.
- Each case: queried local IME attempt, one explicit paste, one Check; no retry,
  repair, native baseline or external clipboard writes. Compare screenshots with
  exact durable fixture evidence only after the operator phase.

Evidence root: .agents/runs/control-gui-20261014/.

## First run — preserved failure, operator stopped

`cc-gui-first.jsonl` contains exactly one text action, explicit method=paste,
into the verified fixture TextBox. Product rejected before any clipboard write
or paste chord: `cannot inspect clipboard sequence for paste; clipboard unchanged
by this paste attempt; no paste chord dispatched`. The operator clicked Check
once; fixture displayed FAIL with evidence saved. No retry/repair was performed.

The model skipped requested ownership checks. During case2 window resizing it
clicked outside the fixture, then guessed taskbar buttons (showing unrelated
Chrome/Notepad windows) and used Alt+Tab. This violated the restricted operator
scope; coordinator terminated ONLY its exact owned local Claude PID19940 after
verifying command/config identity. No additional text input occurred. The run is
NOT accepted, and cases2–4 are untested rather than counted as failures/passes.

The owned fixture was then stopped using its PID/path/session/creation-time and
task-identity-verified cleanup script. Original user apps were neither closed nor
killed. Follow-up owned-process inventory showed only the supervisor PID6800,
so the disconnected task executor retired. First oracle was copied only AFTER
operator termination. A separate lifecycle-only CC run (no input or screenshots)
is being used to avoid mixing control assertions with visual payload work.

## Native lifecycle-only checks — passed

`cc-live-handoff.jsonl` (actual glm-5.3-flash) executed15 MCP calls, no step or
observe, against the interactive native backend. Raw call results independently
parsed into handoff-call-results.json confirm: A/B initial available; A ready;
B lease_conflict and continued describe response; A paused while authority owned;
B still lease_conflict; A resume requires fresh observation; A close released and
available; same B transport opens generation3; old A close lease_conflict; B
still owns; B close released; final A/B available. This establishes native
lifecycle behavior, not text correctness or successful GUI handoff typing.

The underlying paste issue is under investigation. A bounded C# diagnostic was
cross-reviewed from source and compiled on Windows (compile exit0), but not yet
executed. It only reads station/token metadata and sequence values, plus one
OpenClipboard/CloseClipboard pair; no contents/formats, EmptyClipboard, Set,
input, retries, ACL changes, or global IME changes. LastError is explicitly NOT
used as the undocumented error contract of GetClipboardSequenceNumber.

Diagnostic launcher attempt1 did NOT execute the probe: Scheduled Tasks
canonicalized the supplied principal SID to account name Administrator, and the
launcher's literal SID/string comparison refused it. Retained task was Ready,
not running; all other inspected task fields match. The repair compares resolved
SID identities rather than names and keeps fail-closed identity validation.
Original scripts/transcript and metadata remain preserved; this is a diagnostic
harness failure, not another product failure or clipboard observation.

## Read-only interactive sequence probe — discriminating result

Corrected launcher invocation through CC+GLM succeeded. Probe executed once in
Session1, WinSta0/Default. NtQueryObject succeeded and reported actual station
GrantedAccess0x000F037F including WINSTA_ACCESSCLIPBOARD. Sequence samples before,
pre-open, while clipboard was successfully locked, and after successful close
were all0. No clipboard contents/formats were read or written. Probe nativeexit0;
its task was removed only after identity verification. Evidence sequence-probe-v2/.
Token-elevation query reported error24 and remains UNKNOWN; do not infer elevation
from that failed field. Other identity/access evidence is retained locally.

This disproves the blanket assumption that sequence0 necessarily means unusable
clipboard access. It proves this probe's current context, not the past counter
history or retrospectively the frozen child's exact station handle. The proposed
fix does not rely on that attribution: each production transaction must itself
successfully OpenClipboard before accepting unchanged raw initial baseline0.
Postpublication ownership/payload proof, strict nonzero final sequence guards,
focus/modifier guards and no-retry/no-restore behavior remain unchanged.

The product worker is adding a stage-specific regression before implementing the
minimal fix. Test execution remains CC+GLM-only; no product changes from coordinator.

## Zero-baseline regression RED and minimal repair

Behavior-preserving stage helpers plus four filtered prewrite tests were compiled
for Windows and executed by actual CC+GLM. Native101:2 passed,2 failed exactly at
initial-zero and locked-unchanged-zero acceptance. Failure message matches the
live GUI rejection. Raw evidence zero-red/{stdout.log,stderr.log,summary.json}.

The repair now records raw initial sequence (including0) and compares raw equality
inside successful OpenClipboard before any mutation. Any change (including0→1
or1→0) is still rejected. Postpublication strict zero rejection unchanged. Five
new pure regressions cover both policies; real lock ordering is independently
source-reviewed rather than falsely claimed as exercised by those pure tests.
Final release build/regression and fresh GUI acceptance remain pending below.

## Zero-baseline fix: Windows GREEN and new GUI attempt

CC+GLM (`cc-zero-green.jsonl`, actual `glm-5.3-flash`) executed the frozen
Windows library and paste-contract binaries. Raw stdout and runner summaries
confirm library335 passed/0 failed/7 ignored and contract12 passed/0 failed;
both nativeexit0, no timeout or output truncation. Binary hashes match
`fixed-artifact-hashes.json`. Source hashes matched `fixed-source-hashes.json`
before deployment. Independent narrow source review found no blocking issues;
pure tests do not claim native lock-order coverage.

Fixed isolated host SHA256: `228dfda8f12c21c0cb5ac670e9855e217257da865f88d4213db34514e68c6ccd`.
Original binaries retained in remote `original-bin/`. Started owned host PID19192
in interactive Session1, port18413, and fresh fixture PID19072/run
`fb2be599ed8047d684fd4f2e75d3e896`. New screenshot-only operator uses one MCP
alias, native1920x1080 observations and no resizing/taskbar/Alt+Tab. New transcript
`cc-gui-fixed-first.jsonl`; outcomes pending. Old failed attempt is unchanged.

Scoped rustfmt check exposed one wrapping-only discrepancy in the new source;
no semantic issue was reported by review. Formatting will be corrected after
this frozen-build GUI run, with provenance recorded.

## Fixed-build GUI: completed, mixed end-to-end outcome

`cc-gui-fixed-first.jsonl` exited0; actual model tags all `glm-5.3-flash`.
The screenshot-only operator used exactly four explicit paste actions and four
Check actions, no text retries/repairs or native baseline. Foreground click was
within the visible fixture title; no taskbar/Alt+Tab/resize/other-app input.
A final Escape `key_chord` with empty modifiers was rejected before dispatch;
`key_hold` Escape then closed the fixture. `computer_close` released control.
This schema correction was not a text-paste retry.

Oracle copied only after CLI completion. Four durable ordered rows match the
screenshots. Comparison evidence: `fixed-gui-comparison.json`, tool results in
`fixed-gui-call-results.json`,22 images under `fixed-screenshots/`.

| Case | Fixture exact expected match | Tool-submitted text equals actual TextBox | Attribution |
|---|---|---|---|
| Chinese | FAIL | Yes | Operator submitted U+003A instead of displayed U+FF1A |
| Mixed ASCII | FAIL | Yes | Operator misread quote/backslash and omitted backtick before calling tool |
| Emoji | PASS | Yes | Exact expectation and input match |
| Multiline/TAB | FAIL | Yes | Operator submitted half-width colons and U+3000 instead of ordinary space |

Thus fixture end-to-end result is **1/4**, not4/4. Independently, **4/4 submitted
MCP payloads arrived unchanged**, including Chinese, emoji, literal TAB and CRLF;
all four paste calls returned dispatched without input errors. These observations
support the repaired paste path but must not relabel failed visual-task cases.
No product code was changed to accommodate the model's incorrect reading.

IME Enable reported open=True/conversion0x9 each time; every Check query reported
open=True/conversion0x401, explicitly NOT VERIFIED. Full-width-at-paste coverage
is therefore unproven. Neither setter success nor screenshots after setting
establishes that full-width stayed active through paste. No DingTalk contact or
real chat input/send was tested.

The earlier frozen-host zero-sequence error was absent on the first fixed-build
paste. No external clipboard seed, fallback or manual Ctrl+V was used. The earlier
zero-sequence failure, interrupted operator and all four new first outcomes remain
preserved separately.

## Final source hygiene and limitations

After binary freeze, only one rustfmt line-wrap was corrected in windows_clipboard.rs.
New SHA256: `a70371357d9e82a00990cb532dbab6ddd9441fa1111e52224d5814c6b7031a9b`.
The tested binary corresponds to the earlier recorded source hash; the final
difference is whitespace only, not a second semantic build. Scoped rustfmt and
git diff --check passed afterward. No commit/push/worktree. Original deployment
was not replaced; fixed binaries remain in the isolated test deployment.

Remaining acceptance gaps: full-width IME persistence at actual paste time,
DingTalk-specific behavior, and screenshot-only exact transcription of visually
ambiguous punctuation/spacing. Native manager ownership lifecycle is passed,
not a claim of concurrent GUI typing (which is intentionally disallowed).

## Cleanup verified

Fixture PID19072 had exited via Escape; identity-verified cleanup removed its
owned scheduled task. Owned host PID19192 stopped via the verified deployment
stop script; instance record removed. Final bounded inventory returned no
processes under the owned test root, no current host/fixture tasks, no listener
on18413, and no instance record (`final-owned-inventory.json`, exit0).
Evidence, original/fixed binaries and prior failed outcomes remain retained.

## Later October14 completion of previously pending coverage

See `.agents/reports/control-followup-20261014.md`: actual DingTalk self-chat
explicit paste + one Enter send verified; all7 previously ignored library tests
explicitly passed in their required Session0/Session1 contexts. New bounded
fixture trace observed open=true/conversion0x9 before AND after WM_PASTE in all
four cases, with exact submitted-text readback4/4 and no dropped trace samples.
Check-time0x401 was a later state and did not represent paste-time state.
Screenshot-only transcription is still not all green: new run2/4, with the two
remaining model-submitted text errors preserved. Previous failures/history here
are not overwritten. Original deployed host was not replaced.
