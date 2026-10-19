# Stage 2: post-arm stopped-server teardown (author-only checkpoint)

One new ignored test:

`mcp::hotkey::x11_registration_tests::stopped_qualification::post_arm_stopped_server_disarm_releases_grabs_after_resume`

No execution or live result is claimed. Production GREEN, Stage 1, and the five
original live test bodies remain frozen. The initial registration edit was another append-only module declaration.
The 2026-10-16 runner-only revision changes private authentication and proc-stat
parsing; production, Stage 1 and Stage 2 Rust assertions remain unchanged. Stage 2 includes the frozen Stage 1 snapshot
utility read-only; it neither modifies nor runs the Stage 1 input test.

## What must be proved

1. A new, default-map, single-screen private Xvfb has no root children, held
   keys/buttons, or unexpected focus. Capture full core/XKB map/control/compat
   and keyboard-state baseline. No input, focus change, window creation, map
   writes, lock toggles, or reset occurs in Stage 2.
2. An independent observer first acquires and releases **all four** planned
   Escape+Ctrl+Alt+Shift/lock grabs, proving baseline vacancy. Arm the product;
   the observer must then receive **BadAccess for every variant**, with a healthy
   guard and unset cancellation flag. Armed text alone does not prove this gate.
3. Only then does the test request stop. The runner signals its **own** Xvfb
   through a retained pidfd and verifies the matching process reaches state T.
   The test independently checks server parent/start ticks/state before teardown.
4. While the server remains stopped, time the actual `guard.disarm()` call.
   Assert **less than 2 seconds**, not Armed, no remaining disarm handle, exactly
   one cancellation Arc reference, and no synthetic cancellation. Recheck state T
   AFTER disarm returns. No fixture X request occurs between STOPPED and RESUMED.
5. Runner resumes the same retained server. While it is still alive, the observer
   performs an actual roundtrip, independently acquires **every old grab**, and
   explicitly releases its probe grabs. At most one second of collision retries
   permits the resumed server to process the old client's shutdown; no server or
   keymap reset is used. Baseline map/state/focus/empty root and held-key set must
   match. Only after this evidence may normal runner disposal start.

This tests stopped-server cleanup AFTER healthy registration, not just the older
stalled-setup test. It does not claim real emergency key delivery; Stage 1 covers
that separately. No map-mutation qualification is bundled here.

## Private runner protocol and retained identity

`runner.py` is a CC-only executable authoring artifact, **not executed by the
implementation agent**. It launches a new Xvfb and exactly one compiled Rust test
binary as its own direct children. It refuses an existing socket/lock/listener,
requires the explicit `--owned-disposable` option, and never removes another
owner's files. Use only inside a fresh private test container with no host X
sockets/other GUI workers. The runner uses `-noreset -nolisten tcp -auth` with a
fresh 0600 authority file in its own 0700 directory, never `-ac`. The file is
exclusively created and contains one exact FamilyLocal hostname/display
MIT-MAGIC-COOKIE-1 record with 16 random bytes. No inherited authority, xauth
subprocess, credential argv/env value/log, host access changes or profile changes.

Before spawning the test, the runner verifies listener inode ownership and
SO_PEERCRED PID/UID **before** sending any cookie. One no-cookie setup must reject;
one fresh-cookie setup must succeed. These are setup-only probes, not input/map
requests. Failure is infrastructure-blocked, not a reason to remove authentication.
The same private file is supplied to the test as XAUTHORITY. Inherited
XAUTHLOCALHOSTNAME is removed only in the test's environment. All primitives are
standalone copies, not imports from the canonical Stage 1 task-artifact runner.

The runner establishes SIGCHLD default behavior, has no reaper threads, and calls
`pidfd_open` for each own unreaped child before polling/waiting. Thus an immediate
child exit cannot produce a PID-reuse window before acquisition. Every signal
uses `pidfd_send_signal`; there is **no numeric-PID kill, pidfd fallback, security
profile relaxation, or signal from the Rust test**. Start ticks are additional
checks/evidence, not a replacement for retained pidfd identity. The parser uses
`text.rsplit(')', 1)[1].split()[19]`: field22 after the last comm terminator,
including comm containing spaces/newlines/`)`. Zero ticks are rejected. This
corrects the ad hoc Stage 1 CC runner's field21 mistake without touching guards.
That previous attempt qualified **zero cases**; it is not product RED/PASS.

A private 0700 directory contains a 0600 Unix control socket. SO_PEERCRED checks
bind both ends: runner accepts only the exact test child PID/UID; test accepts only
its direct parent runner PID/UID. ASCII frames are newline-delimited, at most
128 bytes, with absolute phase deadlines. No credentials cross this channel.

| Direction | Frame | Required condition/action |
|---|---|---|
| Runner → test | `HELLO <server_pid> <start_ticks>` | Retained own server running; test validates identity/endpoint |
| Test → runner | `ARMED_PROVEN` | All vacancy/collision assertions, healthy guard, flag false |
| Runner → test | `STOPPED <pid> <ticks>` | pidfd SIGSTOP completed; matching server state T |
| Test → runner | `DISARMED <microseconds>` | Actual disarm returned under 2s, refs released, still T |
| Runner → test | `RESUMED <pid> <ticks>` | pidfd SIGCONT completed; same server running |
| Test → runner | `VERIFIED` | Old grabs demonstrably absent and exact baseline matched |
| Runner → test | `FINISH` | Protocol complete; Rust must still exit successfully |
| Test → runner | `ABORT` / EOF | Failure: runner resumes/disposes only owned children |

Runner-provided environment uses `RPA_X11_HOTKEY_STOP_QUALIFY=
owned-disposable-postarm-v1`, `RPA_X11_STOP_RUNNER_PID`,
`RPA_X11_STOP_SERVER_PID`, `RPA_X11_STOP_SERVER_START_TICKS`, and
`RPA_X11_STOP_CONTROL_SOCKET`. It also sets this fixture's DISPLAY/native-X11
markers and the newly created private XAUTHORITY file, never inherited credentials. The
Rust test verifies executable/command line and that the declared Xvfb owns the
matching local listening socket inode. Invoke via the runner, not cargo directly.

## Boundedness and failure handling

- Xvfb socket startup: 5s. Each setup/auth exchange: absolute 2s, reply at most
  64 KiB. Control accept: 8s. Arm-proof phase: 8s.
- Stopped-disarm response: 4s, but the measured product-call limit remains 2s.
- Other control phases: at most 5s; frame reads use an overall 30s protocol limit.
- Stop/resume state observation: at most 2s per transition. Test exit: 5s.
- Cleanup attempts resume first, then signals only owned children through pidfds;
  TERM/reap wait 2s, then KILL/reap wait 2s per child. No unbounded joins/waits.
- Rust guard Drop attempts disarm on assertion failure. Control Drop sends bounded
  best-effort ABORT; runner phase timeout/EOF independently triggers cleanup.
- Runner SIGTERM/INT records an interruption rather than raising in the tiny
  spawn/identity-acquisition interval. Bounded phase return leads to normal
  retained-child cleanup. Do not infer immediate exit from a signal request.

Blocked kernel/file/exec I/O or a forcibly killed runner can defeat userspace
cleanup. An unavailable pidfd after spawning is **cleanup unknown**, never an
excuse to signal a bare PID. Unreaped children after bounded KILL waits are also
reported unknown. The outer disposable-container owner must destroy its own
container on such failures; no successful termination/restoration is claimed.
The mandatory outer 90s deadline plus 20s kill grace below bounds the test job,
not proof that a kernel-stuck child disappeared. No logs saying `qualified=true`
are sufficient without `cleanup_confirmed=true`, a zero runner exit, and the
actual one-test Rust result.

## Exact independent CC commands (author did NOT execute)

In the frozen source/container, first compile only:

```sh
cargo-1.89 test --locked --offline --lib --no-run --message-format=json > stage2-build.jsonl
```

Select `TEST_BINARY` from the successful `compiler-artifact` JSON record for the
project's **library test target** (`profile.test == true`, `target.kind` contains
`lib`, non-null `executable`). Resolve it to an absolute path. Do not select a
helper/integration binary or use a guessed stale target hash. Metadata-only local
compilation is not this Linux link check.

After coordinator authorizes the new private fixture slot:

```sh
timeout --signal=TERM --kill-after=20s 90s \
  python3 src/mcp/hotkey/x11_stopped_qualification/runner.py \
  --owned-disposable --display 91 \
  --test-binary "$TEST_BINARY" \
  --evidence-dir "$NEW_STAGE2_EVIDENCE_DIR"
```

Display 91 is only an example fresh private-container number; the runner must
refuse if occupied. Use 1..65535 while the existing product display-parser limit
remains open. Evidence directory must not already exist. The runner hardcodes
exactly the ignored test above with `--exact --nocapture --test-threads=1`; it does
not execute Stage 1 or any other ignored test. No GUI/input/native inventory or
clipboard task may share this server/container.

Keep `runner.jsonl`, `test.log`, `xvfb.log`, exit status, archive/source hashes,
actual test count, measured disarm microseconds, before/after server identity and
stop state, all-four-grab release proof, exact-baseline line, and owned child reap
postflight. Guard refusal, timeout, unavailable pidfd, compilation/linking error,
or missing protocol stages is NOT a product assertion RED/PASS.

## 2026-10-16 runner-only freeze / no execution

Current runner and this README are hashed in
`.agents/runs/linux-parity-20261015/hotkey-qualification-runner/SHA256SUMS`.
The original Stage 2 Rust files/registration have not changed in this revision.
Only AST/static source checks were performed; the fixture is still **queued
pending review**, not executed or qualified. Keep the previous archive immutable.

CC can exercise nine pure runner contracts (plus four Stage-1-only skips) with:

```sh
HOTKEY_RUNNER_MODULE="$PWD/src/mcp/hotkey/x11_stopped_qualification/runner.py" \
  python3 .agents/runs/linux-parity-20261015/hotkey-qualification-runner/test_runner_contract.py -v
```

This imports the runner but never calls main, spawns/signals a child, or talks to
X. Counts are authored expectations only. Live Stage 2 requires its own explicit
coordinator authorization and serial private-container slot; a Stage 1 retry
authorization alone does not authorize stopped-server qualification.
