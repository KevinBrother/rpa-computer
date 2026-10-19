# F1/F2 negative assertion checkpoint — RED execution pending

2026-10-16. Responds to
`.agents/reports/2026-10-16-linux-native-ui-fixture-review-1.md`.
Only additive files in `desktop-feedback/linux/testing/live-ui/review_red/`.
No fixture acceptance code, old tests, observer, manifest/lock or product source changed.
The coordinator's existing93-file snapshot remains immutable; its original32 units/link
may run independently. No tests/GUI/SSH/runtime/delegation/install were executed here.

## Authored assertions (13 total; NOT RUN)

| File | New cases | Expected against unchanged fixture, pending independent execution |
|---|---:|---|
| `test_backpressure_red.py` | 6 | 5 negative assertions fail; 1 eligible-window control passes |
| `test_timestamps_red.py` | 2 | 2 missing-witness assertions fail |
| `test_ring_red.py` | 5 | 3 negative assertions fail; 2 controls pass |

Expected **10 assertion failures +3 control passes**, not an observed result. No skips,
xfail, stub `NotImplemented` or new unimplemented acceptance function is involved.
Unexpected import/type/runtime errors are infrastructure/test-design errors, NOT valid RED.
All files were syntax/bytecode compiled without importing or executing them.

### Actual call paths under test

- F1 exercises existing `cases_shutdown.backpressure()` including its existing
  `queue_stop()`, raw-prefill accounting and `rules.classify_backpressure()`. The scripted
  backend supplies complete evidence that passes the old scalar checks; the assertion
  requires INCONCLUSIVE rather than R1 attribution when the running-age alternative
  cannot be excluded. Schedules include older-heartbeat/very-short-EOF-exit, exact1s,
  absent pre-spawn bound and nonfinite bound. A0.5s whole-process control remains eligible.
- Timestamp tests invoke real `ipc.Renderer.__init__` (real local pipes, mocked spawn;
  no child launch) and real `Renderer.exit`. Delayed spawn/evidence I/O distinguish the
  required pre-spawn and post-exit-observation witnesses from unsafe later timestamps.
- F2 invokes existing `cases_ui.feedback()` through its ring section and **real**
  `Case.image/inspect/until/clickthrough/target_presses`. The backend preserves target
  hit events but interposes same-ring unmap, unmap→remap or destroy→replacement.
  Pre-images are painted/viewable; the remap case's final image is also viewable.
  A sentinel stops only at the first unrelated heartbeat stage AFTER both old ring
  acceptance checks; it is caught as successful passage, never as the expected rejection.
  Continuously mapped ring and unrelated-window unmap are controls.

`support.py` is a scripted oracle, not a native observer/renderer implementation and not
live evidence. In particular it supplies Unmap events; the current Rust observer still
**drops** them. A future fix must retain native events as well as inspect them in Python;
these tests alone cannot establish that native wiring. Real X execution remains separate.

## Finite GREEN proposal — NOT IMPLEMENTED in this checkpoint

### F1: conservative oldest-possible-queue lower bound

Source-confirmed competing worker gate is `oldest age >=1s`, checked independently of
closing's200ms drain in frozen `native_ui/process_boundary.rs:78-90`. Click time is not
queue origin, and an EOF→exit bound alone does not cover older heartbeat/Ready frames.

Record `Renderer.spawn_before = time.monotonic()` immediately BEFORE `owner.spawn`/Popen.
No frame in that renderer can have been enqueued before its process exists. Record
`Renderer.exit_observed_at` immediately after a real `proc.poll()` observes termination,
before artifact I/O. Feed both witnesses through the actual backpressure acceptance path.
Require finite, ordered timestamps and `exit_observed_at < spawn_before + 1.0` (a small
conservative margin may be used). Otherwise report INCONCLUSIVE, not PASS. Keep actual
full-pipe, unconsumed reader, disabled Stop, filler accounting and failure65 requirements.
Never reset the bound at Ready, activate, click, EOF or after slow spawn.

Reason: the actual enqueue instant is no earlier than the pre-spawn lower bound, while
actual termination is no later than its observation. A valid entire interval strictly
under1s rules out any frame's running1s queue-age gate, including a heartbeat older than
Stop. Observation delayed past the limit conservatively loses qualification; it does not
prove that the running gate actually fired. This only excludes that competing branch,
not every imaginable native fault or a directly traced worker receipt.

**Feasibility is conditional, not measured:** startup/Pango paint, current grant, click,
closing's200ms drain and exit observation must all fit under1s. Owned local Xvfb may make
that possible, but the permitted3s startup budget and scheduler stalls can exceed it.
Such runs must remain INCONCLUSIVE; do not accelerate/alter the renderer, clock, compositor
or queue to force PASS. Capture the timestamp before potentially slow Popen, not after.

### F2: same-XID mapping continuity bracketing real injection

Preserve `UnmapNotify` alongside Map/Destroy in the standalone observer. Include observed
XID in captured-image facts. Arm an event cursor BEFORE the pre-injection observation;
retain the same ring XID, actual viewability/paint and geometry. After real XTEST target
press/release, obtain post-image/viewability for that same ID and a server-reply/event-drain
fence covering the interval. Reject missing witnesses, ID changes or any same-XID
Unmap/Destroy (including Unmap→Map) as INCONCLUSIVE. Ignore transitions on unrelated XIDs;
keep status-only clickthrough separate. Event lists must not be reset between witnesses.

This establishes continuous mapping plus bracketed paint and actual hit-testing, NOT
continuous physical scan-out or every intervening repaint pixel. It cannot infer mapping
continuity merely from expected650ms lifetime or a final screenshot. Native trace wiring
and real private-X qualification still need independent review/execution after GREEN.

## Independent commands (not executed here)

Start at repository snapshot root; overlay only this additive directory on the unchanged
93-file fixture snapshot, verify both manifests first. Existing32 unit command is unchanged.
New negative suite is separate so it cannot silently alter the queued original32 run:

```sh
L="$PWD/desktop-feedback/linux/testing/live-ui"
PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$L:$L/review_red" \
  python3 -m unittest discover -s "$L/review_red" -p 'test_*_red.py' -v
```

Record actual assertion/error/control counts and original logs. No display/environment
setup, subprocess/GUI launch, Rust build or package installation is needed for these13
Python assertions. Wait for independently observed assertion RED before implementing the
proposed GREEN acceptance changes. Product66, exclusion unsupported and Host closed remain
unchanged; this checkpoint neither authorizes live GUI execution nor changes provider scope.
