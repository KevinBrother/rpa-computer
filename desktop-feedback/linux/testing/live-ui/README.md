# Owned private-X native feedback fixture — implementation checkpoint, NOT UI acceptance

## F1/F2 amendment — current compile-only checkpoint

The current source contains the bounded F1/F2 repair; see **`review_green/HANDOFF.md`**
and **`review_green/fixture-green.sha256`** for the current freeze and independent commands.
The original `fixture-freeze.sha256`, HANDOFF and evidence below are historical baseline
artifacts, not a manifest for the changed current files. They have not been rewritten.

Independent CC established baseline32 units + real Linux1.89 observer/product links and
then actual Linux F1/F2 RED:10 assertion failures,3 controls,zero errors. This lane has
only compiled/syntax-checked the repair; current45 units/link/review/live are pending.

- Backpressure PASS now also requires finite ordered pre-Popen/EOF/post-exit witnesses
  with **exit observation strictly less than1s after the pre-Popen timestamp**. A quick
  click or short EOF→exit alone is insufficient. Slow startup or ambiguous running-age
  attribution yields INCONCLUSIVE, not PASS; no timing origin is reset to accommodate it.
- Ring clickthrough now brackets real XTEST with same-XID painted/viewable witnesses and
  retained Map/Unmap/Destroy interval events, ending with an explicit server-reply fence
  before event drain. Unmap→remap, replacement or missing witnesses yield INCONCLUSIVE.
  The observer advertises the required trace schema; an old observer is not accepted.
- These gates exclude the reviewed false positives; they are not worker/syscall tracing,
  continuous raster/physical scan-out proof or capture exclusion. No GUI is authorized here.


2026-10-16, Asia/Shanghai. This is test tooling only. All files are isolated under
`desktop-feedback/linux/testing/live-ui/`; no frozen renderer, old tests, shared crate,
Host, capture path, root manifests, compositor or builder image was changed.

**Capture exclusion remains `unsupported`. Host admission stays closed.** This fixture
observes the deliberately visible feedback UI, not a clean model capture. No capture-time
hiding, unmapping, pixel erasing or provider/compositor substitution is implemented.
Passing any/all cases cannot qualify genuine exclusion or complete Linux parity.

## Gate / evidence status

Coordinator evidence (not re-executed here): CC74928 original107 + new10 PASS and actual
Linux Rust1.89 executable link PASS; Nash R1/R2 source review has no blocking findings.
Product freeze archive: `0796ba52582752bedbbc7446f669d6ddaa327c9270951c441d5b570eea57a7e6`.
See the two `.agents/reports/2026-10-16-linux-renderer-{process-eof-green-1,eof-green-review}.md`
reports. Product link/117 assertions are NOT live-UI acceptance.

This lane performed **only syntax and compile checks**:

- `evidence/observer-locked-check.log`: offline/locked `cargo check --all-targets`, Linux
  x86_64 target, local macOS Rust1.98.1; exit0. Includes compiling the unit-test targets,
  not executing them and not Linux linking. No `DOCS_RS`.
- `evidence/python-syntax-check.log`: AST + bytecode compilation (no imports or execution),
  Python3.14.0 using Python3.10 grammar; exit0. Linux Python3.10 runtime compatibility
  remains for CC to check.
- `evidence/product-freeze-check.log`: local product manifest49 entries unchanged.
- `evidence/full-product-freeze-check.log`: coordinator full66-entry manifest unchanged,
  including shared crate, product source, old tests and locks.
- **Fixture unit tests, observer Linux1.89 link and every live case: NOT RUN here.**
  Tests were authored before their new acceptance/resource behavior, but no observed
  RED/GREEN is claimed because execution is prohibited in this lane.

## Architecture and dependencies

- `run.py`: executable Python stdlib parent; explicit opt-in and SHA-verified ELF paths,
  serial cases, sanitized child environment, bounded ownership and artifact manifest.
- `owned.py`: fresh authenticated Xvfb, private0600 cookie, process groups, monotonic
  deadlines, bounded terminate/kill/reap. Never reuses/removes a foreign display lock.
- `ipc.py`, `wire.py`: actual `os.pipe` stdin/stdout, nonblocking writes/reads,16KiB frames,
  strict bounded v1 output reader, retained raw bytes and explicit test-prefill accounting.
  The product still uses its shared Rust codec unchanged. The independent Python decoder
  is an oracle, not a replacement product protocol implementation.
- `case.py`, `cases_ui.py`, `cases_shutdown.py`, `rules.py`: real OS actions plus independent
  observations and small pure acceptance rules. No fake-native policy receipts.
- `observer/`: standalone Rust binary using **xcb=1.7.1** (`shape`, `randr`, `xtest`) and
  **serde_json=1.0.151**, own workspace/lock, declared MSRV1.89. Versions already exist in
  the coordinator's vendor bundle; XTEST feature is test-only. No product dependency edits.
  No Python packages, ctypes, libXtst binding, xdotool, WM or compositor are required.

Independent execution prerequisites (check; never silently install):

- Disposable isolated Linux container and C linker/libxcb development/runtime environment;
  approved native builder `rpa-linux-native-ui-builder:20261015`, supplied ID
  `sha256:36ee97ebed577e7bd803914539dec1f30282cccb8e48ee8d46772e7434d5aaf1`.
  **This lane has not probed its current runtime packages.**
- Linux Python>=3.10, Rust1.89, complete vendor bundle, actual frozen renderer ELF.
- `Xvfb`, `xauth`, `pkg-config`, `fc-list`, `dpkg-query`; Cairo/Pango/GLib/libxcb from the
  approved image. Inventory output is saved; absent commands/packages are blockers, not
  skip/PASS and not permission to install. This first inventory profile is Debian-based.
- Server Shape>=1.1, RandR>=1.5 with exactly one active1920x1080 monitor, XTEST>=2.2;
  TrueColor24-depth/32-bit storage RGB masks `ff0000/00ff00/0000ff`; no compositor owner.
  Root geometry is checked; actual88x48 Stop checks scale1. No global DPI mutation.
- Fonts covering the modest sample `Contrôle Δ` and normal status text. Unknown glyphs
  must not be treated as Unicode success. Missing coverage fails the case; no font install.

## Serial live cases (fresh renderer AND observer per case)

| Case | Actual observations required |
|---|---|
| `feedback` | Exactly one Ready with unsupported exclusion; PID+role windows, viewability, override-redirect/no-focus hints, input shapes, visible pixels; status and live ring center/stroke clicks reach patterned target; heartbeat; idle/paused/stopping changes; failed-cleanup presentation survives Closed; released+Closed differs. |
| `stop-once` | Real XTEST edge click then repeated center click; exactly one decoded current-session Stop; disabled repaint matches startup reference; no target click or focus theft. |
| `stale-grant` | g1 press acknowledged by pressed pixels; g2/paused acknowledged by new status; old release emits no Stop; fresh g2 click emits exactly one, repeat suppressed. |
| `unicode` | Actual Pango-rendered Latin-accent/Greek label, viewable pixels, normal exit. Retain screenshot for human review; not all-script coverage. |
| `clean-eof` | Actual active process, close input only, rc0 AND full protocol/output EOF, no Stop/error, PID-matched windows destroyed. |
| `preclosed-eof` | Input already closed before spawn; clean exit, complete output preserved and all transient X events retained. |
| `startup-eof` | Close input after independently observing startup windows; clean exit, retain in-flight Ready and event trace. This is a sampled race, not deterministic internal-boundary coverage. |
| `truncated-eof` | Actual truncated JSON + EOF; rc65, not clean/watchdog/signal. |
| `backpressure` | Actual stdout pipe filled to even1-byte EAGAIN, duplicate writer closed; real Stop click and disabled pixels before input EOF; reader kept open and undrained until rc65; then exact filler accounting and protocol-tail validation. |
| `output-closed` | Same processed Stop/full-pipe evidence; input EOF then sole reader closes; bounded rc65. No queued-control delivery claim. The real no-reader condition is established; syscall errno is not traced. |

Every observation uses an independent X client started before the renderer. Window identity
requires both `_NET_WM_PID` and `_RPA_DESKTOP_FEEDBACK_ROLE`; XIDs are discovered, not guessed.
XTEST uses normal server hit-testing (never XSendEvent targeting a feedback window). Only
the observer's own patterned target gets explicit focus. Even transient target focus loss
fails. All actions occur on the newly owned/authenticated display, default`:91`.

### Bounds / accounting

- Work deadline120s; each case10s including observer/renderer startup; Ready3s;
  EOF-to-exit observation2s. Timeout is failure, never a clean shutdown.
- Observer startup3s/command1s; requests4KiB, replies60KiB; max256 root children,
 512 events/command,2048 event trace/case. A stuck synchronous X call is bounded by the
  parent timeout and owned-process kill, not falsely described as interruptible XCB.
- Protocol16KiB/frame,64KiB aggregate,256 messages; raw stdout68KiB including filler;
  trace512KiB/case; image rectangle<=65536pixels,<=32 PPMs/case. Child file size limit8MiB
  and disabled core dumps. Stderr retained, not terminal pipes that can silently block.
- Whole/case work timers stop during finite owned cleanup (TERM0.5s, KILL/reap1s per
  launched PID, observer command/quit bounded). Cleanup is additional to work bounds;
  no unlimited joins or global pkill. Descendants are not launched by the fixture clients.
- Backpressure records real EAGAIN, filler count/capacity, disabled-image proof, actual
  input-EOF timestamp and exit latency. If processed-Stop evidence is absent or actual
  click-to-EOF>=0.8s, return INCONCLUSIVE, not R1 live PASS. It does not drain to make the
  case pass. Only **after process exit** are test0xff bytes accounted/removed for parsing;
  `stdout.raw` retains them exactly. This is IPC instrumentation, not image manipulation.
- Output-reader-close case necessarily cannot recover bytes still in the kernel pipe.
  It explicitly records `filler_recovered:false` and never presents them as delivered.
- Stop enqueued-before-disabled-paint and UI-before-Ready are source-checked product
  orderings; pixels/attributes substantiate actual visible state at observation time.
  Cross-client timestamps do **not** prove precise pre-Ready scan-out or worker EOF
  observation time. Startup cases retain committed Ready instead of discarding it.

## Unit-test inventory — authored, not executed

Python27 tests:

- `tests/test_wire.py`:10 strict-wire/limit/UTF8/generation/exclusion/prefill tests.
- `tests/test_rules.py`:4 display safety, Stop input region, role uniqueness, complete
  backpressure proof requirements.
- `tests/test_acceptance.py`:8 pixel/focus/clean-exit/control/hash/deadline rules.
- `tests/test_resources.py`:5 failed-construction FD/handle cleanup and real parent-EOF
  timing rules. Uses local pipes/mock failed spawn; launches no child/X server.

Rust5 tests in `observer/src/requests.rs`: unsafe coordinates/path, unterminated/unknown
requests, valid bounded motion. Test harness never calls X startup. **32 total fixture
unit assertions** are separate from the frozen product117.

## Independent CC commands (handoff only; NOT executed by this lane)

Run unit/link stages first on Linux Rust1.89. `CARGO` below must name the coordinator's
verified1.89 cargo; `VENDOR` must be the absolute existing full vendor path. Do not alter
manifests/locks to accommodate a missing bundle; report the dependency blocker.

```sh
L="$PWD/desktop-feedback/linux/testing/live-ui"  # start at repository snapshot root
export PYTHONDONTWRITEBYTECODE=1
PYTHONPATH="$L" python3 -m unittest discover -s "$L/tests" -v

CARGO=/path/to/verified/cargo-1.89
VENDOR=/absolute/existing/vendor
export CARGO_HOME="$L/cargo-home" CARGO_TARGET_DIR="$L/observer/target"
"$CARGO" --config 'source.crates-io.replace-with="fixture-vendor"' \
  --config "source.fixture-vendor.directory=\"$VENDOR\"" \
  test --locked --offline --manifest-path "$L/observer/Cargo.toml" \
  --bin rpa-feedback-live-observer
"$CARGO" --config 'source.crates-io.replace-with="fixture-vendor"' \
  --config "source.fixture-vendor.directory=\"$VENDOR\"" \
  build --locked --offline --manifest-path "$L/observer/Cargo.toml" \
  --bin rpa-feedback-live-observer
```

Only after independent fixture review/unit/link approval, run live cases **serially in a
fresh isolated container**, with no host X sockets/devices/desktop environment/credentials
or Host executable mounted. Mount the frozen renderer and source read-only as appropriate;
copy the fixture/build outputs into its owned scratch space. Do not use `xvfb-run` because
the fixture itself owns display/auth/lifecycle. No Docker/SSH execution is part of this handoff.

```sh
RENDERER=/absolute/frozen/rpa-desktop-feedback-linux
OBSERVER="$L/observer/target/debug/rpa-feedback-live-observer"
python3 "$L/run.py" --ack-owned-private-xvfb \
  --renderer "$RENDERER" --renderer-sha256 "$(sha256sum "$RENDERER" | cut -d' ' -f1)" \
  --observer "$OBSERVER" --observer-sha256 "$(sha256sum "$OBSERVER" | cut -d' ' -f1)" \
  --source-sha256 0796ba52582752bedbbc7446f669d6ddaa327c9270951c441d5b570eea57a7e6 \
  --image-id sha256:36ee97ebed577e7bd803914539dec1f30282cccb8e48ee8d46772e7434d5aaf1 \
  --display 91 --out /tmp/feedback-live-NEW
```

The command defaults to all10 cases, stops on first failure/inconclusive, and never falls
back to an inherited display. `--cases feedback` (or other explicit subset) may be used
for serial diagnostic runs with a **fresh output directory**. An input hash calculated
from a binary confirms transport integrity, not its source provenance: CC must independently
bind that binary's build log/source/image digest. Supplied image/source IDs are labelled
as supplied, not cryptographically verified by this fixture. Exit0 means selected case
assertions and cleanup passed; exit1 failure/blocker; exit2 inconclusive. Always inspect
`result.json`, selection, stderr, trace, raw wire, screenshots and `artifacts.sha256`.

## What remains even after a future private-X PASS

Independent observer1.89 link/unit execution; actual runtime qualification and human pixel
review come first. Private Xvfb is just a controlled test environment, not the implementation
platform and not a substitute for real-desktop WM/compositor, mixed-DPI, arbitrary font/script,
active-pointer-grab, wedged-X watchdog or physical scan-out qualification. Fingerprints prove
changes/equality, not text semantics, aesthetic quality or successful input cleanup. Source
review plus screenshots are needed for those limited assertions; real desktop profiles remain
separate. **Genuine clean-frame capture/provider qualification is unresolved and mandatory;
this fixture cannot authorize Host admission or change unsupported to requested/verified.**
