# Native renderer implementation handoff — 2026-10-15

## Freeze meaning

This is implementation against independently observed43 RED contracts, plus the real
standalone executable/XCB/Pango adapter. **Not a claim of43 passes,79 passes, Linux linked
executable, UI acceptance or product parity.** All original36 and new43 tests/support remain
byte-for-byte unchanged. Four core source files and Cargo.lock remain unchanged. The package
manifest adds only the feature-gated executable and descriptive metadata/comments, not new
dependencies. Other workers' Host/capture/display/native/control files are untouched.

The RED baseline remains `testing/native-ui-red.sha256`; do not interpret its old source
hashes as current. Current snapshot: `testing/native-ui-green.sha256` (repo-relative paths).

## Evidence and compile caveat

Independent pre-implementation evidence:

- `cc-renderer-core-diagnostics-1/probe.log`: core36/36 PASS, rc0, Linux Rust1.89.
- `cc-renderer-native-red-1/probe.log`: native contracts27+7+9 fail from missing behavior,
  compile success, rc101, no infrastructure failure.
- Independent derived builder `rpa-linux-native-ui-builder:20261015`:
  `sha256:36ee97ebed577e7bd803914539dec1f30282cccb8e48ee8d46772e7434d5aaf1`.
  Exact pinned native feature link-probe built on Linux Rust1.89 rc0, NOT run.
  Package inventory/full transcript: `.agents/reports/2026-10-15-linux-renderer-native-red-1.md`.

Current local evidence is **compile-only on macOS Rust1.98.1**, not execution:

1. Pure `cargo check --locked --offline --no-default-features --tests`.
2. Linux-target `cargo check --target x86_64-unknown-linux-gnu --features native-ui
   --all-targets --locked --offline` with command-local `DOCS_RS=1`.
3. Source formatting and checksums.

The initial cross-check with `PKG_CONFIG_ALLOW_CROSS=1` failed because local pkg-config lacked
libffi metadata. No packages/configuration were installed or changed to bypass that failure.
The locked gtk-rs sys build scripts explicitly support `DOCS_RS=1` to skip native probes and
link flags; that mode was used solely to type-check Linux-specific source. Therefore **no
native library validation/link claim follows from this local check**. Do not use DOCS_RS in
CC's real builder. No tests, SSH, GUI, binary/example, Docker/image, or fixture was executed.

## Production boundaries now implemented

- UI controller produces ready only after Native.create/configure/paint/present_checked.
  The actual adapter creates three server resources, installs focus/input policy before
  mapping, paints Cairo/Pango into server backing pixmaps, maps status/Stop and checks their
  attributes/geometries/input shapes. This is a server-level receipt, not a compositor
  scan-out fence or proof of human-visible presentation on every WM. Ring is initialized
  but unmapped until a confirmed
  pointer fact. Success calls the shared encoder with unsupported exclusion, never requested.
- The binary constructs X11Native and Pipes, not test fakes. Pango uses set_text, absolute
  physical font sizes, natural direction, end ellipsis and glyph-coverage checking. Cairo
  never reads desktop pixels. PutImage uploads renderer-owned artwork, not capture edits.
- Status/ring input regions are empty. Stop uses the full physical rectangle derived from
  frozen88×48 logical layout; bare Stop's bounding region also remains rectangular. No
  SetInputFocus/activation or explicit grab calls. Root event selection is client-local.
- The UI owns model/native objects. Only completed UI turns emit heartbeat, at most once per
  second. Pipe workers cannot generate heartbeat; they only forward bounded shared-codec
  output. Native event processing is capped at64 events per outer turn.
- Real control events carry the session and local presentation revision observed by the
  native adapter. XCB expanded event sequence is compared with a checked presentation
  barrier when grant/surface/topology changes, so queued old native clicks are not rebound
  to a newly painted grant. UI drains latest Host facts before authorization. The local
  revision/sequence is a freshness guard, never a replacement for Host session+generation.
  Stale close events are guarded too. Pure test events still exercise the original contract.
- Clean EOF outranks pending snapshots and exits without Stop or safe-cleanup assertion.
  Queue overflow, malformed/truncated input, broken output/native connection and presentation
  failure are terminal. Cleanup severity remains frozen-core authoritative.
- Current compositor owner is observed, never changed. An owner change is terminal; no
  compositor-fork/replacement decision, private provider channel, Host registration or
  admission change is made. Clean-frame architecture remains the separate investigation.

## Bounded lifetime and memory rules to verify live

| Mechanism | Implemented bound / failure action |
|---|---|
| Startup | Watchdog thread starts before native connection/font work; absolute3s deadline, `_exit(70)` if wedged |
| Running UI | Deadline2.5s refreshed only after completed outer UI turns; server facts/stack round trips at least every500ms when idle |
| Shutdown | Explicit exit path arms1s watchdog; output drain200ms, main waits at most300ms, no unbounded worker join |
| Native cleanup | Disconnect own X connection (all owned resources), no final server round trip; server-side destruction is observable once server processes disconnect |
| Pipe calls | Private pipe/socket fds set O_NONBLOCK; one4KiB read and one bounded Outbox write per worker turn; no flush |
| Mailbox synchronization | UI try-lock retries stop after2ms; contention/poison fails closed instead of authorizing stale controls; worker holds no lock across sleeps |
| Backpressure |32 frames including partial in-flight; nonempty queue age1s triggers terminal failure. Kernel pipe buffering is separate/bounded and does not prove Host consumed a Stop |
| Input | Shared16KiB framing and single pending latest snapshot; poison is terminal |
| Rendering | At most8MiB per image, three bounded rasters; upload chunks at most16KiB of pixel payload; bounded shape rows |
| Native facts | At most64 monitors; root property reply bound16KiB; unsupported/ambiguous geometry rejected |

The watchdog is a process fail-safe, not thread cancellation or proof of input release. A
whole-process SIGSTOP also stops its watchdog and is not a valid UI-thread-only liveness
experiment. Host supervision remains mandatory. Scheduling/OS failures can add latency;
record measured timing and failures, do not reinterpret bounds as hard real-time guarantees.
The worker has exit/panic signaling; an unexpected worker exit cannot silently retain healthy
transport. Sanitized errors only; no wire bytes/labels/application titles in diagnostics.

## Stage A — independent regression and actual binary link (no GUI)

Use the current full locked vendor and isolated Cargo configuration in the approved derived
builder. No DOCS_RS or fake pkg-config metadata in these acceptance commands.

```sh
cargo-1.89 test --locked --offline --manifest-path desktop-feedback/linux/Cargo.toml \
  --no-default-features --no-fail-fast \
  --test codec_contract --test geometry --test ipc_input --test ipc_output --test model \
  --test native_ui_contract --test native_ui_policy --test native_ui_transport \
  -- --nocapture --test-threads=1

cargo-1.89 build --locked --offline --manifest-path desktop-feedback/linux/Cargo.toml \
  --features native-ui --bin rpa-desktop-feedback-linux
```

Target79 passes is unobserved. Freeze logs, rc, compiler, native package inventory, image ID,
manifest/lock/source and resulting executable hashes. Failures are not permission to weaken
assertions, remove native dependencies, edit core, or activate feedback in the Host.

## Stage B — real X11 fixture, separately authorized/owned by CC

Use a bounded subprocess fixture with the actual built executable and private binary-mode
stdin/stdout pipes. No ordinary-file redirection and no terminal stdio (rejected by binary).
The fixture can act as a v1 peer for standalone UI qualification; it must NOT patch Host
validate_ready or treat unsupported as admitted. Preserve the selected existing DISPLAY and
authority, and run sequentially in the approved owned X session; no silent compositor start,
replacement, profile switch or global DPI modification. Do not stop shared/unowned processes.

Set per-case and whole-run deadlines, bounded stderr/wire/X-event logs, an owned resource list
and cleanup of fixture-owned processes only. Record the UI, target app and observer PIDs/X
clients; identify the renderer's three windows using ownership plus their diagnostic
`_RPA_DESKTOP_FEEDBACK_ROLE` property (status/stop/ring). Titles/PID properties alone are not
security-grade ownership proof. The observer must be an independent X client.

### 1. Startup must be genuine

- Keep a separate event-recorder app focused. Start the real executable (no self-test path).
  Observe window creation/configuration/map and initial pixels independently before accepting
  its first ready. Ready must be v1, pointer_feedback=true, exclusion=unsupported, once only.
- Inspect override_redirect, WM_HINTS input=false, initial input regions, depths/visuals,
  geometry, role/client ownership, status+Stop viewability and allocated ring window.
  Before a grant, show waiting/disabled Stop, not active input or safe completion.
- There is no externally exposed fake presentation receipt. Correlate server events, actual
  displayed pixels and protocol timestamps; pure43 tests cannot establish this ordering.
- Invalid DISPLAY/auth, missing extensions/unsupported visual/font coverage: no ready and
  bounded exit; any partially created owned windows disappear. Do not count a quick failure
  as a successful visual fallback.

### 2. Drive only valid, exact fixture facts

Query live RandR/root coordinates first. Send shared v1 snapshots with a unique test session,
exact u64 generation, monotonic sequence and exact native surface rectangle. Use an opaque
fixture surface ID/version; do not pretend it is a production Host attachment. Example field
shape (replace geometry with the queried fixture facts):

```json
{"type":"snapshot","version":1,"sequence":1,"session":{"id":"fixture-a","generation":1},"phase":"idle","cleanup":"not_needed","surface":{"id":"fixture-monitor","version":"fixture-1","x":0,"y":0,"width":1280,"height":720},"pointer":null}
```

The numbers above are illustrative, not an assumption about the running session. A desktop
union with gaps requires id=desktop and the exact union; mismatched/ambiguous facts fail.

### 3. Clickthrough, focus, Stop and stale-event cases

- With pointer=click at a known app point, observe the actual ring during its650ms lifetime;
  repeat move/drag at350ms. Click ring stroke/center and status with authorized fixture input.
  The target app must receive those clicks; mere shape inspection is insufficient.
- Focus stays on the target through creation, painting, hover and first Stop click. Confirm
  no focus/activation/global-grab request in the X trace. Stop's first click emits one exact
  current session+generation; additional clicks cannot duplicate it or claim cleanup.
- Query Stop's physical full rectangle and test edges/corners:88×48 at scale1,132×72 at1.5.
  Unlike status/ring, Stop must not pass clicks to the underlying app. Show distinct normal,
  hover, pressed and disabled states without changing its wording.
- Press under A/g1, deliver A/g2 or B/g2 before release: no control for the new grant. Then a
  fresh click must work exactly once. Repeat with queued native press events before repaint,
  stale snapshots, release outside, wrong button, session:null and changed surface/topology.
- Send a legitimate close intent to a current window: Stop once, not silent exit or cleanup
  success. Old-window/grant close intent must not be rebound. Actual resource destruction is
  terminal renderer loss and must invoke the separate Host failure policy, not false success.
- Test another app's pointer grab/drag. If Stop cannot receive it, record the gap; the separate
  emergency-control lane is not evidence that the button itself worked.

### 4. EOF, backpressure, native stalls and heartbeat

- Close fixture stdin while UI active and while a press is pending. Clean EOF should remove
  owned UI and normally exit0; malformed/truncated stream or concurrent transport failure
  is failure, never an emitted cleanup-success claim. Check no new Stop after EOF.
- Send malformed/oversized and fragmented/superseding frames; preserve bounded allocations
  and real UI responsiveness. Exceeding limits must be failure, not a successful retry.
- Force actual stdout EAGAIN with a fixture-owned bounded pipe (record any prefill separately;
  never count fixture bytes as renderer messages) and observe bounded terminal cleanup.
  Unit Outbox tests are not evidence of real OS pipe behavior. Do not misinterpret kernel
  buffering as application consumption or a delivered/acted-on Stop.
- To exercise a blocked X call, only in a separately approved exclusive disposable fixture,
  suspend that fixture's own X server or use an owned fault-injection proxy. Watchdog stays
  runnable in the renderer: no fresh UI-generated heartbeat during the blocked call and
  bounded exit70; resume/clean only owned resources. Never suspend a shared real desktop.
- For UI-thread-only stall tests use a separately reviewed debugger/test-build mechanism
  that leaves writer/watchdog running; whole-process SIGSTOP is not this proof. No production
  v1 test command was added. After a pause, no heartbeat catch-up flood may appear.

### 5. DPI, Pango and both visual profiles

- Use explicitly prepared owned profiles at Xft style scales1/1.25/1.5/2. Do not mutate a
  user's global DPI. Compare the same physical pointer coordinates across profiles; only
  style dimensions/font sizes change. Record that per-monitor separate XSettings DPI is not
  implemented, rather than declaring mixed-DPI parity from one root resource setting.
- Reconfigure the owned fixture's topology/workarea with a pending press and ring: invalidate
  both before using new placement. Exact Host surface mismatch must fail, not silently move
  feedback to a guessed monitor or shrink Stop below its logical size.
- Run labels containing CJK, Arabic RTL, combining marks and literal `<b>...</b>` text. Fonts
  must exist in an explicitly approved fixture inventory; this lane installs none. Missing
  glyphs are a startup/paint failure, not Unicode success. Inspect shaping, fallback, RTL,
  ellipsis, clipped glyphs, contrast and Stop states on actual rendered pixels.
- Observe a compositor-absent profile and a separately selected existing-compositor profile.
  Include fullscreen/unredirected windows: compositor selection ownership alone does not
  prove active composition of each frame. Verify opaque shaped versus ARGB paths, empty
  input regions, no opaque ring interior and
  stable stacking without an Expose loop. Compositor-owner change intentionally fails closed.

## Stage C — still separate and mandatory

A real-window/UI pass must not enable the Host or qualify captures. The future independently
reviewed provider must prove correct dynamic underlying pixels in actual Host/model output
while all feedback remains human-visible, without hide/unmap/shape/opacity tricks tied to
capture and without erasing/scrubbing screen pixels. This executable contains no capture
request path or exclusion provider. Clean-frame architecture/admission remains unresolved.
