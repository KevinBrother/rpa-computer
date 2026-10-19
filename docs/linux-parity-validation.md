# Linux parity: deterministic CC-only validation

## Status and boundaries

This is **test code, not acceptance evidence**. The implementation author has not
executed these tests, started a GUI, sent input, used SSH, or run the host. Syntax
compilation is the only permitted local execution check. Independent CC owns
execution/review and must record its actual model identity, deployed binary build
identity and environment. The latest task authorizes this Linux work despite the
older Windows-first note at the end of `.agents/README.md`.

Write ownership is only `scripts/linux-parity/**` and this document. No product,
manifest, existing Xvfb scripts, commits, worktrees or recursive agents are needed.
Python uses the standard library. `python3-tk`, Xlib and the optional Xvfb wrapper
programs are **test environment dependencies**, not product dependencies.

The oracle is intentionally **whitebox**: an owned 720×500 Tk window exports its
actual Entry/Text strings, focus, selection/copy state, key edges, button counters,
canvas coordinates, wheel deltas and currently held inputs. The host is the real
`computer-host`, not the transport doubles used by harness unit tests. There is
no `event_generate`, `xdotool`, direct XTest input, injected clipboard data from the
runner, or external application control. The fixture's Ctrl+A/C implementation
is its declared normal editing UX. Initial focus is set only on this owned window;
all later interactions go through computer tools.

No universal pixel oracle is attempted. PNG signature, IHDR CRC, dimensions,
resize bounds, provenance and mapping are checked; screenshots are saved for CC
inspection. Pixel content correctness/full image-only blackbox acceptance remains
separate. Correct widget state alone does not prove screenshot content.

## Recommended first command: clean private X server, native smoke

Run the following **inside** the dedicated Linux environment, as its non-root test
user. `HARNESS` is the directory containing the supplied files; `HOST` is the
actual built/deployed Linux binary. Every `--out` directory must be new. The
wrapper creates a new authenticated, TCP-disabled Xvfb, without touching an
existing desktop, its resources, or its original smoke fixture.

```bash
HARNESS=/opt/linux-parity
HOST=/opt/rpa-host/computer-host
OUT=/home/rpa/linux-parity-evidence
mkdir -p "$OUT"
python3 "$HARNESS/isolate.py" \
  --host "$HOST" --out "$OUT/smoke-$(date -u +%Y%m%dT%H%M%S)" \
  --mode native --scenarios smoke --overall 120
```

The smoke verifies a real click increments a counter **exactly once**, focuses the
owned Entry, types `abc123`, verifies its exact value, takes images, closes the
host, checks no held key/button, and compares the core keymap/modifier map before
host startup and after host exit. It is an initial native path, not Unicode parity.

### Copy into the existing dedicated container (CC commands, no rebuild required)

From the repository root, with the existing container already provisioned and the
real Linux binary already deployed:

```bash
# These are for independent CC to execute, NOT commands the author has run.
N=rpa-linux-xvfb
STAMP=$(date -u +%Y%m%dT%H%M%S)
docker exec --user 10001:10001 "$N" mkdir -p /home/rpa/parity-code
docker cp scripts/linux-parity/. "$N":/home/rpa/parity-code/
docker exec --user 10001:10001 "$N" \
  python3 /home/rpa/parity-code/isolate.py \
    --host /opt/rpa-host/computer-host \
    --out "/home/rpa/parity-$STAMP" --mode native --scenarios smoke --overall 120
# Inspect JSON and retrieve files, not terminal image/base64 dumps.
docker cp "$N:/home/rpa/parity-$STAMP" "./linux-parity-$STAMP"
```

`docker cp` may give source files root ownership; ordinary read permissions are
sufficient. Do not chmod private evidence/auth files world-readable. The existing
image's Xvfb, xauth, xrdb, python3-tk and Xlib suffice for the intended wrappers.
If a dependency is absent, the wrapper writes `NOT_IMPLEMENTED` and exits 2;
provision the disposable environment separately (no product/deployment edits).
No SSH or GUI client is needed to launch the harness.

## Core now, clipboard later

Use distinct output directories for every invocation:

```bash
python3 "$HARNESS/isolate.py" --host "$HOST" \
  --out "$OUT/core-$(date -u +%Y%m%dT%H%M%S)" \
  --mode native --scenarios core --overall 600

python3 "$HARNESS/isolate.py" --host "$HOST" \
  --out "$OUT/cancellation-$(date -u +%Y%m%dT%H%M%S)" --mode native \
  --scenarios cancel-pause-key,cancel-pause-mouse,cancel-close-key,cancel-close-mouse,cancel-eof-key,cancel-eof-mouse \
  --overall 600

# Explicitly separate this capability probe from native Unicode:
python3 "$HARNESS/isolate.py" --host "$HOST" \
  --out "$OUT/clipboard-$(date -u +%Y%m%dT%H%M%S)" \
  --mode native --scenarios clipboard --overall 180
```

| Scenario | Application evidence required |
|---|---|
| `smoke` | Counter +1, Entry `abc123`, focus, released inputs |
| `native` | Exact uppercase/punctuation ASCII, then `中文`, then `🙂🚀`; real key press/release; no synthetic SendEvent |
| `editing` | Actual Ctrl+A selection, Ctrl+C exact copied text, then Ctrl+V into Text; real Control modifier events |
| `multiline` | Native Tab from Entry moves focus to Text; native LF, CR and CRLF each yield one newline/Return; Text Tab inserts a literal tab, with exactly three Enter effects |
| `drag` | Canvas target A's actual rectangle reaches target B, completed drop count, released mouse |
| `scroll` | Exact positive/negative horizontal/vertical wheel counts and signed displacement |
| `hold` | Shift down/up with observed duration; following native `lower` remains lowercase |
| `displays` | primary/exact advertised id/desktop at full root size and 640×400 bounds; actual mapped root click coordinates |
| `lifecycle` | Pause rejects input without fixture events, resume with fresh observation, text works, close/EOF/idle |
| `cancel-{pause,close,eof}-{key,mouse}` | First observe held Shift or dragging button; interrupt before natural 5s completion; real release event + X server idle + no late events |
| `clipboard` | Explicit `method:paste`; exact multiline/tab/Chinese/emoji without native Return/Tab events or application Enter effects, retaining Text focus; second Ctrl+V still has payload |
| `authority` | Contender fails with explicit `lease_conflict`, no map/pointer/fixture changes, original owner remains usable |

`core` expands to the first nine rows only. `all` selects every implemented
scenario, including clipboard and the same-screen authority check. Individual
names and comma-separated lists are supported. A case fails immediately on its
first failed condition; cleanup still runs. Fresh fixture/host processes isolate
subsequent cases. Uncertain cleanup aborts later cases as `NOT_RUN`.

The editing case uses **the fixture's own normal Ctrl+C clipboard ownership**;
it is not evidence of the product's explicit paste API. A global schema advertising
`paste` is also not proof of Linux support. The clipboard case probes that exact
method; absent schema or an explicit unsupported/not-implemented error with
`not_started` and unchanged fixture becomes `NOT_IMPLEMENTED`, never PASS. Other
errors, partial effects, or unknown outcomes are failures, not capability skips.
No native fallback, automatic retry or clipboard restoration is used. Post-close
selection persistence/clipboard-manager behavior is not covered.

## Authority and HiDPI: separate controlled environments

```bash
# One fresh server, two screens. Owner uses .0, contender .1 and a different TMPDIR.
python3 "$HARNESS/isolate.py" --host "$HOST" \
  --out "$OUT/authority-$(date -u +%Y%m%dT%H%M%S)" --mode authority --overall 180

# Two fresh servers sequentially: 1280x720 physical root at 96 and 192 DPI,
# plus matching Xft.dpi resources. Each runs display mappings and native smoke.
python3 "$HARNESS/isolate.py" --host "$HOST" \
  --out "$OUT/hidpi-$(date -u +%Y%m%dT%H%M%S)" --mode hidpi --overall 240
```

The authority case never sends input or even initialize to the contender; the
current CLI acquires authority before reading stdio. It requires exit code 3 and
the structured `lease_conflict` stderr code, rather than treating any crash/EOF as
exclusion. Stderr is parsed locally for this code, otherwise only counts/digests
are saved. This is same UID, same server, different TMPDIR and (in the wrapper)
different screen; not cross-UID revocation or a hostile X-client security proof.

HiDPI is not an arbitrary multiplied click coordinate. Each environment exports
its actual fixture geometry; observations must retain physical-root dimensions
and mapping, and a real click must reach the measured button center. The wrapper
sets DPI **only on newly owned X servers**. Parent environment, old DISPLAY,
Xauthority and X resources are never edited. On exit, owned process groups are
stopped and temporary cookies/runtime files are removed. No process-name kills,
`killall`, `pkill`, lock-file deletion or shared server mutation is used.

## Running directly inside an already reserved Xvfb

Only use this if CC has confirmed exclusive control of a dedicated disposable
server and no original fixture/other GUI is running. The attestation flag is not
an automatic proof that DISPLAY is safe. Prefer `isolate.py` above.

```bash
/usr/local/bin/exec-session.sh python3 "$HARNESS/run.py" \
  --host "$HOST" --out "$OUT/direct-$(date -u +%Y%m%dT%H%M%S)" \
  --dedicated-xvfb --scenarios smoke --overall 120 \
  --request-timeout 20 --scenario-timeout 90
```

A host action or observation always uses the current schema from `tools/list`.
Unsupported schema constructs fail closed; the harness does not guess new fields.
Observations require session/surface/geometry/topology provenance, actual image
width/height, physical-pixel mapping regions and fresh observation IDs. Request
IDs are unique; responses are routed strictly by JSON-RPC id, including concurrent
cancellation replies. Coordinates are derived from fresh measured widget geometry
and the current region mapping. No blind hardcoded click center or direct OS
coordinate substitution is allowed.

## Evidence and exit semantics

- `summary.json`: selected/unselected scenarios, outcomes, assertion counts,
  remaining coverage, **`acceptance_claim: false`**.
- Per scenario: `initialize.json`, `tools.json`, `describe.json`, `events.jsonl`,
  `result.json`, `cleanup.json`, `fixture-before.json`, `fixture-after.json` (or
  failure snapshot), `x11-before.json`, `x11-after.json`, intermediate held-state
  snapshots, and numbered PNG files. Live fixture state is atomically replaced.
- Wrapper: per-environment `environment.json`, nested `cases/summary.json`, and
  wrapper `summary.json`. The host binary path is recorded, not its entire env.
- stdout contains only case statuses/evidence locations. Images are decoded to
  files and replaced in the transcript by path/byte count/hash/dimensions. No raw
  stderr, auth cookie, secret environment dump, or image blob is printed.
- Exit 0: **selected checks** passed with clean cleanup, not Linux parity accepted.
  Exit 1: failed assertion, protocol error, deadline, or uncertain cleanup.
  Exit 2: selected capability not implemented, or selected case not run.
- `dispatched` is only a protocol prerequisite; each scenario also needs actual
  fixture evidence. `partial`/`unknown` and failed/unknown cleanup remain verbatim
  in evidence. Normal cases fail on these; they are never silently retried.
- EOF cancellation may intentionally have no response. Evidence records
  `input_outcome: unknown`, not an invented successful step. That case can establish
  only prompt process exit, observed release, idle X server and no late events.
- Paused input currently may return only a `cancelled` guard error without a
  StepRecord. The harness records the missing outcome as missing, and separately
  checks no fixture events; it does not label it `not_started` without evidence.

The default request budget is 20 seconds, scenario budget 90, overall run 600.
Nonblocking writes and reads both respect original per-request deadlines and the
overall budget; notification/frame/pending counts and stderr are bounded. EOF in
an incomplete frame, malformed JSON, duplicate/unknown IDs, stdout logging,
missing images and mismatched request IDs are errors. All polling is monotonic and
bounded. Read-only Xlib calls run in a separate child with a 4s timeout. Twenty
seconds are reserved before starting/continuing work for close/EOF, process teardown
and post-host snapshots. Emergency OS process teardown may take an additional
small bounded grace interval on interruption. The wrapper has its own bounded
supervisor timeout and terminates only its owned runner/server groups. A private
`owned-processes.jsonl` ledger records PID/group/start-time identities so the
wrapper can reap live registered child groups after runner interruption; any such
emergency cleanup makes the run FAIL. There is no process-name kill.

## Keymap comparison scope

`x11_snapshot.py` reads XDisplayKeycodes, XGetKeyboardMapping,
XGetModifierMapping, XQueryKeymap and XQueryPointer. ctypes signatures use native
`unsigned long` for X11 KeySym/Window and pointer-sized Display pointers. It never
changes maps/modifiers or injects input. The complete **core projection** (including
keysyms-per-keycode width and all eight modifier slots) is compared exactly before
host initialization and after process exit; held keys/buttons and effective
modifier state are also checked. No normalization hides changed slots.

This is **not full XKB actions/types/groups/controls/compatibility-map restoration**.
That limitation appears in every summary. A future independent full XKB dump
comparison can supplement it; do not handwave ABI-sensitive native XKB structs or
claim the core projection proves the entire XKB state restored.

## CC harness self-tests (no GUI, separate from product validation)

The transport doubles below exercise the harness only. They are never used by
`run.py` and are not product parity evidence:

```bash
python3 -m unittest discover -s "$HARNESS" -p 'test_*.py' -v
# Compilation-only alternative (does not run tests or instantiate Tk/Xlib):
PYTHONPYCACHEPREFIX=/tmp/linux-parity-pycache python3 -m py_compile "$HARNESS"/*.py
```

Review covers strict schema types, PNG header/CRC, negative-origin region mapping,
out-of-order replies, request and overall deadline ceilings, a nonreading child,
partial EOF, invalid/unknown frames, frame flood, and no raw stderr artifact leak.
CC should additionally inspect the PNGs, protocol responses and per-case fixture
snapshots; a zero exit code alone is not the acceptance review.

## Remaining / NOT_IMPLEMENTED scenarios and integration requirements

1. **Full isolated blackbox**, non-Tk real apps, image-content oracle; independent
   filesystem-isolated validation remains required by the frozen spec.
2. **Full XKB state** beyond core map/modifiers; multigroup layouts, lock states,
   keyboard-map slot exhaustion, external map mutation and restoration faults.
3. **RandR multimonitor** negative origins, gaps, rotations, hotplug/generation
   changes, pixel-budget rejection. Current display case deliberately requires one
   actual RandR display; the two-screen authority wrapper is not that coverage.
4. **Authority revocation/cross-UID**, nonce theft/change, late init/quarantine,
   writer handoff, initialization faults and X server disconnect recovery.
5. **Clipboard integration:** the existing platform-global schema says paste is
   Windows-only. Linux backend preparation/verification and truthful capability
   description must be integrated by its product owner. This harness does not
   change that code. When integrated, rerun `clipboard` independently. Large/INCR,
   owner loss/focus changes, rejection without mutation and post-close lifetime
   need additional scenarios; short payload success is not the full contract.
6. **Feedback/emergency stop**, capture exclusion and user-facing UX, remote TLS,
   Windows regressions and macOS compile evidence remain external gates.
7. Current native text test uses a small explicit Chinese/emoji sample suitable
   for finite safe slots; it does not pretend arbitrary Unicode sequences or
   exhaustion are already covered. If real Tk text differs, retain the failure;
   do not change expected text, prefill the widget, normalize away characters,
   fallback to clipboard, or patch the fixture to accept synthetic events.

After any CC failure, report the deployed build identity, selected case, status,
request_id, input/observation/cleanup outcomes and relevant artifact paths to the
product owner. Do not fix product code from this test-only scope or change expected
behavior merely to obtain a green result.

## Follow-up: frozen first-GUI heartbeat failure (2026-10-15)

### Evidence attribution — not the mutable product tree

Independent CC reported 14 harness unit tests passing in
`cc-hotkey-renderer-harness-1`, then smoke passing in `cc-first-gui-1`. The latter
used these immutable identities, which this investigation checked against the
local archives/log (no host/test execution):

- Product archive `display-control-green-1.tar.gz`:
  `3bfc42bba7a516fcc9da79a4e63209b12250f9cabed98ddf47fca91e5dbc8878`.
- Built host SHA-256 reported by CC before/after its run:
  `7947763812c39e7598e4b60c51457fad278930ff38a1cf4496c9df19f6676e17`.
- Harness archive `hotkey-renderer-harness-1.tar.gz`:
  `de2e79201b3a1a2fabd3161c409885e36eeb1c1f36c06787e265e6ab44741484`.

Before making diagnostic edits, the archived `fixture.py`, `context.py`,
`common.py` and `isolate.py` matched their workspace bytes. No later native fixes
are inferences about that tested binary. No frozen artifact was rewritten.

Evidence root:
`.agents/runs/linux-parity-20261015/cc-first-gui-1/evidence/hidpi/hidpi-96/cases/displays`.
The adjacent run root contains `probe-first-gui-1.log`.

| Relative time | Saved fact |
|---|---|
| 0.414s | Describe completed; one physical 1280×720 root matched enumeration |
| 1.357s | First full-size primary observation returned |
| 2.177s | Second observation returned |
| 2.179s | Real click requested at mapped image/root `[148,290]` |
| 3.301s | Click reply: `dispatched`, `not_needed`, step duration 705ms |
| ≤3.382s | Fixture heartbeat rejected; **exact rejected bytes/age were not saved** |
| 3.420s / 3.477s | Close reported released / host exited 0 |
| Post-host | X11 snapshot has identical core map, no held keycodes/buttons |
| Teardown | `fixture-live.json` overwritten by shutdown: revision 85, ready=false |

`image-0003.png` visibly contains **Count click: 1**. The final fixture file has
matching real button press/release, counter=1, and no callback errors. This proves
application receipt of that click, **not ongoing fixture liveness**. The case did
not reach resize/id/desktop rounds or **192 DPI**. It is a 96-DPI fixture-liveness
failure within the HiDPI suite, not demonstrated HiDPI coordinate breakage.

### What is known vs. still hypothetical

**Concrete harness defect fixed here: failure evidence was lost.** Previously
`Context.state()` raised on age≥1s without preserving the rejected file;
`close_resources()` tried the same freshness-checked read to save failure evidence,
which raised again; `Fixture.close()` then replaced the live file with a new
shutdown snapshot. The frozen revision 85 is a shutdown revision: the precise
rejected revision, age and last successful publication cannot be recovered from
that file. Unit tests for this defect were authored first, but have not been run
by the implementation author.

**The underlying heartbeat stall is not yet attributed.** Plausible mechanisms:

1. `Fixture.tick()` is a one-shot callback: it serializes widget state and writes
   the file **before** scheduling the next `after(25, ...)`. A blocking Tk query or
   filesystem write delays both publication and rearming. A lost Tcl callback or
   Tcl-side background error could stop that chain without a Python callback
   error. Python callback exceptions normally would populate `callback_errors`,
   so the empty final list is evidence against that narrower mechanism, not proof
   the Tcl loop was healthy. The old fixture stderr was discarded.
2. The state's monotonic timestamp is evaluated during `state()`, before all later
   widget queries/serialization/atomic replacement finish. Publication latency
   can therefore produce an already-old sample. This is a hypothesis to measure,
   **not grounds to timestamp later or relax freshness**.
3. Scheduling/I/O stalls remain possible in the recorded 2-CPU, debug-build,
   Docker bind-mounted environment. Similar ~0.8–1.1s captures in the successful
   smoke show that capture latency alone does not explain the failure. No live
   scheduler/wait-channel trace was retained to distinguish these cases.
4. Temporary X-server contention is possible, but the frozen source does **not**
   support asserting that PNG encoding holds a server grab. In the archived
   `src/backend/linux_display/native/frame.rs`, capture uses `GetImage` plus
   topology queries without an explicit capture-long grab. The archived native
   click uses an atomic `ServerGrab`; its `Drop` in
   `crates/native-input/src/linux_x11/xkb.rs` calls `XUngrabServer` and `XFlush`.
   A successful capture reply and the post-host X11 query/core-map equality argue
   against a persistent server grab after host exit. Runtime contention still
   needs evidence; these source facts are not a product exoneration.

No timer restart, `update()`/`update_idletasks()`, synthetic input, forced state
write, acceptance retry, later timestamp, or timeout relaxation was added.

### New diagnostics (same one-second freshness gate)

- `fixture-rejected-NNNN.json`: exact age, threshold, revision, seq, ready state,
  read start/end/duration, file inode/size/mtime, current phase/wait label, last
  fresh read, transport pending deadlines, and read-only owned-process data.
- Matching `.raw`: **exact bytes read and rejected**, saved before raising.
  `-last-fresh.raw` preserves the last fresh read separately. Repeated cleanup
  rejection gets a new filename; it cannot replace the original failure.
- `owned-processes-start.json`: comparison baseline for the fixture, host and
  runner. Only recorded PID/group/start-tick identities are inspected. `/proc`
  stat/status/wchan/schedstat/io and up to 16 owned threads are bounded and
  allowlisted; no process names, environments, cmdlines, ptrace or other windows
  are queried. Permission denial/exited PID is recorded as unavailable, **not idle**.
- `fixture-stderr.log`: first 64KiB of owned fixture stderr (including Tcl background
  errors); excess is drained, counted and hashed rather than blocking the fixture.
  Raw product-host stderr remains digest-only. `fixture-stderr-summary.json`
  records completeness/truncation. None is printed to the terminal.
- `fixture-shutdown.json`: separate shutdown state. Closing the fixture no longer
  overwrites its last live heartbeat. The process-exit/liveness check remains
  authoritative; an old ready=true live file does not mean its process is alive.
- Optional **`--fixture-tracebacks`** on `isolate.py`/`run.py`: a Python faulthandler
  watchdog saves stacks every 0.5s to `fixture-tracebacks.log` during the bounded
  owned-fixture lifetime. It does not send GUI input, pump Tk, reschedule its timer,
  or refresh ground truth. This additional diagnostic thread can perturb timing;
  first collect without it, then use it in a separately named reproduction.

Stale state always raises immediately, even if its counter equals the expected
value. Diagnostic errors do not make it pass. Cleanup uncertainty still prevents
later scenarios. This patch fixes **diagnostic preservation**, not a proven fix
for the stall.

### Exact CC reproduction protocol — new harness, same frozen product

Do not overwrite the frozen archives, logs or their staged source trees. Freeze
this revised test harness under a **new** directory/archive, retaining the original
product directory read-only. Use the same Docker image and resource limits as the
first run. No production deployment/rebuild is part of this comparison.

On CC's existing Linux executor, after staging the new harness separately:

```bash
# Existing frozen product; new, separately staged diagnostic harness.
PRODUCT=/opt/rpa-test/linux-parity-20261015/display-control-green-1/source
HARNESS=/opt/rpa-test/linux-parity-20261015/heartbeat-diagnostics-1/source/scripts/linux-parity
EVIDENCE=/opt/rpa-test/linux-parity-20261015/heartbeat-diagnostics-1/evidence
IMAGE=rpa-linux-xvfb-builder:local
HOST_SHA=7947763812c39e7598e4b60c51457fad278930ff38a1cf4496c9df19f6676e17

printf '%s  %s\n' "$HOST_SHA" "$PRODUCT/target/debug/computer-host" | sha256sum -c -
# New evidence root, owned by the same test UID; fail rather than reuse old evidence.
test ! -e "$EVIDENCE" && install -d -m 700 -o 10001 -g 10001 "$EVIDENCE"

# Unit tests only, using transport doubles/synthetic files. No product or GUI.
docker run --rm --network none --user 10001:10001 \
  --cap-drop ALL --security-opt no-new-privileges=true \
  --cpus 2 --memory 4g --pids-limit 256 -e PYTHONDONTWRITEBYTECODE=1 \
  --mount "type=bind,src=$HARNESS,dst=/harness,ro" "$IMAGE" \
  python3 -m unittest discover -s /harness -p 'test_*.py' -v

# Reproduce the first failing case only on a fresh private 96-DPI Xvfb.
# "native" mode here chooses the same 96-DPI environment; scenario is displays,
# not the native text scenario. This cannot be mistaken for testing 192 DPI.
docker run --rm --network none --user 10001:10001 \
  --cap-drop ALL --security-opt no-new-privileges=true \
  --cpus 2 --memory 4g --pids-limit 256 -e DISPLAY= -e PYTHONDONTWRITEBYTECODE=1 \
  --mount "type=bind,src=$PRODUCT,dst=/product,ro" \
  --mount "type=bind,src=$HARNESS,dst=/harness,ro" \
  --mount "type=bind,src=$EVIDENCE,dst=/evidence" -w /evidence "$IMAGE" \
  python3 /harness/isolate.py --host /product/target/debug/computer-host \
  --out /evidence/displays-96-plain --mode native --scenarios displays --overall 240

# Independent second reproduction, same frozen host, with passive stack capture.
docker run --rm --network none --user 10001:10001 \
  --cap-drop ALL --security-opt no-new-privileges=true \
  --cpus 2 --memory 4g --pids-limit 256 -e DISPLAY= -e PYTHONDONTWRITEBYTECODE=1 \
  --mount "type=bind,src=$PRODUCT,dst=/product,ro" \
  --mount "type=bind,src=$HARNESS,dst=/harness,ro" \
  --mount "type=bind,src=$EVIDENCE,dst=/evidence" -w /evidence "$IMAGE" \
  python3 /harness/isolate.py --host /product/target/debug/computer-host \
  --out /evidence/displays-96-stacks --mode native --scenarios displays --overall 240 \
  --fixture-tracebacks
```

These are **CC instructions only**, not commands the implementation author ran.
Capture each invocation's exit code/log without changing its result. The first
failure remains failure even if a second run passes. Do not automatically loop
until green. Recheck the frozen host hash after execution.

For the exact original suite invocation, choose a third, new output directory and
use `--mode hidpi --overall 240` without `--scenarios`; it runs displays/smoke at
96 first, and only proceeds to 192 if that environment passes. Record 192 as
NOT_RUN if it does not proceed.

Return the first rejection `.json`/`.raw`, last-fresh raw, owned process baseline,
fixture stderr/optional stacks, `fixture-shutdown.json`, events, post-host X11
snapshot and PNGs. A stack in `state()`/Tk or file I/O suggests a different cause
from a mainloop with no returning tick plus Tcl errors; runnable CPU/wait data
helps distinguish scheduler pressure. These observations guide the next targeted
fix, not a retrospective reclassification of the frozen failure.
