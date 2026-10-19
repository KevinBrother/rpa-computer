# Native Linux UI RED checkpoint — 2026-10-15

## Status and hard scope boundaries

- Frozen core independently GREEN: `cc-renderer-core-diagnostics-1/probe.log`, run1 rc0,
  36/36 on Linux Rust 1.89. Its original six source/package files and original six test
  files are unchanged. The full locked optional dependency graph was vendored separately
  by the coordinator. This is not native-feature compilation or UI evidence.
- This checkpoint adds **43 intended RED tests**: 27 lifecycle, 7 visual/text policy,
  9 executable transport-wiring contracts. They have not been executed by this agent.
  Each reaches an explicit `native-ui RED:` unimplemented entry point; no ignored tests,
  `should_panic`, unconditional failing assertion masquerading as behavior, or fake success.
- The only implemented additions are typed interfaces, test fakes and a test-only link
  probe. There is **no renderer executable, X11 adapter, Pango painter, pipe worker,
  watchdog or real UI readiness implementation**. The link probe is NOT a renderer.
- Genuine clean-frame exclusion is still REQUIRED, unsupported and unverified. No Host
  auto-enablement, provider contract, compositor fork/replacement, capture-time hiding,
  output scrubbing or runtime/environment change is introduced. Adapter architecture is
  a separate review; this checkpoint does not select or implement one.

## Standalone executable architecture (decision; not implemented)

Future artifact: `rpa-desktop-feedback-linux`, with source entry point
`src/bin/rpa-desktop-feedback-linux.rs` and a `native-ui` required feature. Add that target
and a real build script only after this checkpoint's independent RED and explicit GREEN
implementation authorization; the current manifest/lock remain frozen. Reuse pinned XCB
1.7.1, Cairo/Pango/PangoCairo 0.21.2 and libc 0.2.177; no new crate requested.

Bootstrap sequence:

1. Parse only `--accent '#RRGGBB'` / `--label TEXT`; reject invalid/duplicate/unknown options
   before desktop allocation. Stdout is protocol-only. Do not log labels, wire payloads,
   application titles, X authority material or raw native errors.
2. Validate native X11 session, establish the renderer's own XCB connection, probe native
   RandR/root facts, visual formats, Shape/input-shape capability and compositor presence.
   Never start a compositor or change DISPLAY/XAUTHORITY, global DPI or pointer settings.
   Host environment forwarding is a separate integration issue, not bypassed here.
3. Start bounded nonblocking pipe workers and construct one !Send/!Sync `Runtime` on the
   main thread. Runtime owns frozen FeedbackModel/geometry use; the XCB adapter and all
   Cairo/Pango objects remain on this UI thread. Shared wire types/codec are not forked.
4. Checked create/configure/paint/present sequence creates three owned ordinary windows,
   installs input/focus policy **before mapping**, draws the waiting state, maps status and
   Stop, and receives a checked server round trip plus window/shape attributes. The ring
   window exists and is initialized but is not shown without a confirmed pointer fact.
5. Only then enqueue exactly one real `ready` with `pointer_feedback:true` and
   `capture_exclusion:"unsupported"`. Failure at any stage destroys partial resources,
   never emits ready, and exits. A generic fixture receipt is not native proof.
6. Run a poll-driven main loop using the X connection FD, mailbox wakeup and monotonic timer.
   Recheck terminal transport state and consume the latest Host snapshot before each input
   authorization. Bound native events per turn (initial proposal: 64); do not let X event
   floods starve Stop, Host state, ring expiry or heartbeat. No blocking stdio/join on UI.
7. UI turns originate heartbeat at one-second intervals; missed intervals do not generate
   a catch-up burst. An I/O worker only transmits, never generates heartbeat. Bounded process
   shutdown/watchdog behavior must be tested in the later process fixture, including UI/X
   server stalls; old queued heartbeats must not indefinitely mask a stalled UI.

### Testable boundaries and source ownership

| Boundary | Current scaffold | Future concrete owner / proof |
|---|---|---|
| UI lifecycle and Stop authorization | `src/native_ui/lifecycle.rs`: Runtime, Native, Transport, Receipt, Scene | UI-thread controller delegates core state; trace fake checks ordering, not X behavior |
| Real window creation/presentation | Native methods `facts/create/configure/paint/present_checked/destroy_all` | Future XcbNative; checked requests and live independent X observer |
| Pipe/queue wiring | `src/native_ui/transport.rs`: PipeBridge | Frozen Inbox/Outbox, fixed 4096-byte read scratch, 32-frame output bound, 16 KiB wire limit |
| Policy and drawing requests | `mod.rs`, `paint.rs` | Native input/bounding shapes; Cairo image surfaces; Pango literal text, ellipsis and font fallback |
| Main/process lifetime | Design only | Future binary + bounded worker ownership, wakeup FD, shutdown deadline; process fixture needed |

The tests load the new scaffold by `#[path]`; the frozen `src/lib.rs` is not modified.
Runtime/bridge constructors hold storage only. All behavior entry points intentionally panic.
Native/Transport fakes are confined to tests; production must never substitute fake receipts.
`PipeBridge` is a deterministic single-owner seam, not a claim that it may be shared across
threads without synchronization. The future worker/mailbox adapter must offer bounded,
nonblocking access (including contention failure) rather than sharing a blocking mutex with
an OS read/write. Its actual thread shutdown and wakeup implementation needs further tests
before production code. The pure reader/writer contracts do not establish that property.

### Native window, input and geometry rules

- Status and ring: ordinary override-redirect InputOutput windows, empty **input** regions.
  Stop: a third independent window with the full rectangular hit region. Explicitly no
  keyboard-focus/activation requests or explicit pointer/keyboard grabs. Ordinary X server
  implicit button grabs are not a claim of universal Stop reachability during another
  application's drag/grab; test that separately with the control-lane emergency path.
- Do not borrow the Composite overlay. With a compositor and suitable ARGB visual, alpha
  drawing is permitted; otherwise use root-depth opaque surfaces and bounding shapes.
  Input shaping is independent of alpha/bounding shaping. Missing input-shape support fails.
  A compositor-loss transition must revalidate visual policy, never keep fake transparency.
- Native adapter owns partial allocation cleanup and monotonically changing window-set epoch.
  Reject zero/duplicate IDs and stale presentation receipts. `present_checked` must establish
  current status/Stop viewability, ring initialization and matching topology/window epochs;
  it is not a self-reported `requested` flag. Later X errors or lost windows fail closed.
- Stop press latches the displayed session+generation and local window epoch. Release requires
  matching current values, primary button, valid press and inside-Stop coordinates. Consume
  newly arrived Host facts first. A new grant during a press cannot redirect that click.
  Core once-only state remains authoritative; Host still validates the resulting request.
  Window-close intent requests Stop once; it does not claim cleanup or authorize an old grant.
- Physical X11 root/window coordinates stay separate from logical style dimensions. Validated
  style scale affects 88×48 logical Stop size and fonts, not pointer coordinates. Use the frozen
  geometry resolver/layout; no desktop ratios, screenshot coordinates or guessed Host IDs.
  Native facts must match Host surface; topology changes cancel pressed state and clear ring.
  Reject too-small workareas instead of shrinking Stop. Root coordinate normalization and
  source attachment identity remain the display lane's responsibility.
- Preserve existing dark palette, 16-unit margin, 8-unit gap, status two-line hierarchy and
  clear Stop hover/pressed/disabled states. Text is Pango plain text (never markup); immutable
  Stop label; 48 UTF-16-unit label bound with control/format rejection. Use physical layout
  clip width and scaled absolute font size once. CJK/Arabic/combining glyph correctness is a
  native font/shaping requirement, NOT proved by the pure TextRequest test.

### Terminal behavior

Clean parent EOF outranks pending snapshots, destroys owned windows once, stops accepting
controls and exits without Stop or any claim that Host released inputs. Truncated EOF,
malformed/oversized input, poisoned output, failed ready enqueue, native loss and queue
saturation are terminal failure. Error output is best effort, bounded and sanitized; never
wait for a broken writer. Shutdown must have an independent finite deadline (proposed upper
bound: 1 second) and must not block UI joining stuck I/O or synchronizing with a dead server.
Host supervision remains responsible for authoritative input cleanup. No failure falls back
to unfiltered capture or silently authorizes an unsupported renderer.

## Compile-only and independent commands

This agent performed only macOS Rust 1.98.1 **type checking** of the three new pure test
binaries (no tests/UI/process executed):

```sh
cargo check --locked --offline --manifest-path desktop-feedback/linux/Cargo.toml \
  --no-default-features \
  --test native_ui_contract --test native_ui_policy --test native_ui_transport
```

Use the coordinator's current locked vendor and isolated Cargo configuration. On Linux
Rust 1.89, independent RED command (not executed here):

```sh
cargo-1.89 test --locked --offline --manifest-path desktop-feedback/linux/Cargo.toml \
  --no-default-features --no-fail-fast \
  --test native_ui_contract --test native_ui_policy --test native_ui_transport \
  -- --nocapture --test-threads=1
```

Expected target: 43 ordinary test failures from explicit missing behavior, not build errors.
This is **expected**, not observed RED. Capture counts, exact panic sites and exit status.
Do not mark missing native libraries/vendor entries as behavioral RED. Do not modify tests
or frozen dependencies to make the checkpoint pass.

For a core-only regression run select the original targets explicitly: a broad `cargo test`
on this new tree now includes intentionally failing native contracts. The earlier frozen
`renderer-core-green-1` snapshot remains independently GREEN.

```sh
cargo-1.89 test --locked --offline --manifest-path desktop-feedback/linux/Cargo.toml \
  --no-default-features --no-fail-fast \
  --test codec_contract --test geometry --test ipc_input --test ipc_output --test model
```

Test-only derived builder: `testing/Dockerfile.native-ui-builder`, based on
`rpa-linux-xvfb-builder:local`. Adds only pkg-config/Cairo/Pango/GLib development packages.
Requires the base's Rust 1.89, C linker and libxcb1-dev; do not silently install new deps on
an existing runtime. No image build, package install, container launch or probe run here.
The independent image owner may build it under a **different tag**:

```sh
docker build -f desktop-feedback/linux/testing/Dockerfile.native-ui-builder \
  -t rpa-linux-native-ui-builder:red1 desktop-feedback/linux/testing
```

Then, inside that explicitly selected builder with the existing offline vendor:

```sh
cargo-1.89 test --locked --offline --manifest-path desktop-feedback/linux/Cargo.toml \
  --features native-ui --no-run \
  --test native_ui_contract --test native_ui_policy --test native_ui_transport
cargo-1.89 build --locked --offline --manifest-path desktop-feedback/linux/Cargo.toml \
  --features native-ui --example native_link_probe
```

The example references actual Cairo/Pango/PangoCairo and XCB bindings so Linux link checking
is not merely unused optional dependencies. **Do not execute it for this checkpoint.** On
non-Linux or without the feature it compiles a diagnostic-only alternative, not native proof.
It neither connects X11 nor emits ready. Successful linking does not prove executable/UI
lifecycle, font coverage, native behavior or capture exclusion. Record the selected base
image ID, apt package versions, compiler, source hashes and all build exits for reproducibility.

## Live fixture plan — future independently authorized execution ONLY

The fixture must drive a real binary over private stdin/stdout and observe it through a
separate X connection. No test-only command may synthesize ready/Stop/heartbeat. A separate
owned target app records pointer/key/focus events. Discover windows by child/client ownership
and inspect actual resource attributes; do not rely on titles or renderer diagnostics alone.
Set fixed per-case/whole-run deadlines, bounded logs and owned-process cleanup. Do not install
or launch a compositor silently. Test both an explicitly selected existing compositor profile
and an explicitly selected compositor-absent profile, plus real supported desktops.

| Fixture case | Required independent evidence / failure assertion |
|---|---|
| Startup success and failure | Correlate first ready timestamp with checked creation/mapping/presentation and externally observed three resources. Status+Stop viewable, ring allocated/unmapped initially. Bad DISPLAY/auth, missing shapes, injected create/paint errors: no ready, bounded exit, no surviving owned resources. |
| Focus/nonactivation | Target app stays focused through startup, repaint, pointer ring, hover, first Stop click and topology changes. Log GetInputFocus and target focus events. No unsolicited keyboard or pointer-grab requests in X protocol trace. |
| Cross-client clickthrough | Target records clicks at status and ring geometry (including ring stroke and center); no renderer Stop on those points. Stop first click is received separately, never delivered to underlying app; physical hit size 88×48 at scale1 and132×72 at1.5. |
| Stop identity races | Press under A/g1, send B/g2 or A/g2 before release: no Stop for B/g2. Fresh click yields exactly that current grant once. Replay stale snapshots, release outside, wrong button, old window epoch, close intent and session:null. |
| UI-before-ready/heartbeat | Observe real UI progress then ready. One-second UI cadence. Stop only UI thread via test-build synchronization or debugger, leave writer running: no fresh heartbeat generated; Host timeout must not be defeated by stale buffered pulses. Resume cannot generate catch-up flood. Never add this hook to v1. |
| Pipe/parent failure | Flood fragmented/superseding snapshots, exactly16KiB/overlimit frames, malformed and truncated EOF. Stop reading stdout until backpressure. Parent death and broken stdout: bounded renderer exit, no resource leak, no fake cleanup success. Confirm no UI blocking read/flush/join; working-set/queue bounds remain stable. |
| Physical topology and DPI | Exact native pointer center on offset monitors; scale1/1.25/1.5/2 only changes style. Reconfigure RandR during pressed Stop and active ring; stale geometry suppressed, no reauthorized click. Reject ambiguous/mismatching or too-small workareas. Never alter global DPI to fake the test. |
| Pango and visuals | Fix font inventory in a separately approved fixture; render CJK, Arabic RTL, combining marks, emoji and literal markup. Inspect glyph coverage, correct shaping, ellipsis, no clipping at scales, readable dark contrast and distinct Stop states. Pure string assertions are not shaping proof. |
| Bare/composited visual paths | Inspect actual visuals/depth, input and bounding shapes. Confirm no opaque rectangle obscures ring interior on bare X11. Compositor disappearance/restart revalidates/fails explicitly; no implicit installation/replacement. |
| Grabs and lifecycle | Target app holds pointer grab/drag; measure Stop reachability and separately verify control-lane emergency stop. Kill X connection/renderer/parent; no orphan windows or new control after terminal shutdown. Record unsupported behavior rather than claim parity. |
| Capture exclusion (separate gate) | Later approved provider must show continuously visible feedback AND correct dynamic underlying pixels in actual Host/model frames, with no hide/shape/opacity timing trick or pixel scrubbing. Native UI pass and `unsupported` ready cannot pass this gate. |

No native UI, native-feature linked compile, live-fixture result or provider qualification
has been observed at this checkpoint. Obtain independent RED first; then authorize native
implementation. Full feedback completion still requires native behavior AND genuine exclusion.
