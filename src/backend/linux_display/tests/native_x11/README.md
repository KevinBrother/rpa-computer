# CC-only native X11 display coverage

These **four ignored tests** exercise real RandR queries, real root `GetImage`
replies and the production provider/composition engine. Authorship is not
acceptance. Ordinary `cargo test` does not execute them. No test sends input,
focus requests, clipboard writes or hotkeys. PNGs remain in memory.

## Isolation and prerequisites (independent runner owns these)

- Linux, the project's locked Rust dependencies and usual Linux build/native
  X11 development libraries, including XCB/RandR. No new dependency is required.
- An **exclusively owned disposable Xvfb**, one TrueColor screen, RandR >=1.5,
  local `:N` with N >=90 and no screen suffix, e.g. `:199`. Recommended screen:
  `800x600x24`, with **`-noreset`** so resources configured by `xrdb` survive
  that client disconnecting. The fixture accepts at least 640x480 and at most
  2,000,000 pixels.
  The runner must arrange actual Xvfb creation and record server identity,
  ownership, readiness, version and teardown; these tests do not start servers.
- No WM, compositor, other clients/windows, other suites or parallel processes
  on that server. Fixture rejects any initial root children and any initial
  monitor layout other than one automatic root-sized monitor. The root fallback
  test additionally requires the original monitor/output to have **no primary**.
  Unset inherited `WAYLAND_DISPLAY`; use `XDG_SESSION_TYPE=x11` for this isolated
  command. Do not point these tests at a logged-in desktop.
- `RPA_X11_TEST_ISOLATED=owned-disposable-xvfb` is an explicit ownership assertion;
  `RPA_X11_TEST_DISPLAY` must exactly equal `DISPLAY`. A process-local mutex
  serializes these fixtures, but does **not** isolate other processes. Use
  `--test-threads=1` and coordinate exclusive server use.
- Two topology cases require effective RandR `SetMonitor`/`DeleteMonitor`:
  the server must expose output-backed and output-less logical monitors through
  `GetMonitors(get_active=true)`, respect nonzero origins and primary flags, and
  allow deletion/recreation of the owned secondary using the same atom and new
  origin. Some Xvfb builds ignore/reject these writes.
  **UNAVAILABLE is a panic/nonzero test result, never a skip or passing case.**
  Do not substitute a fake provider, alter assertions, or count the other two
  passing cases as full native topology acceptance on such a build.

## Bounded writes and cleanup

The fixture creates only its own override-redirect 120x96 windows and GCs. It
fills each window with three asymmetric full-channel colors, avoiding fonts,
render timing, visual inspection and decoder-derived pixel expectations. It
waits at most two seconds for Viewable and uses X replies as draw barriers.

Topology cases create two uniquely named logical monitors: `(32,24,200,160)`
(primary) and `(280,64,200,160)` (secondary). The first borrows the original
output association, suppressing its automatic monitor while the fixture runs;
the second has no outputs. The stale case changes only the owned secondary by
**+8 root pixels once**, using checked DeleteMonitor then SetMonitor with the
**same atom**, outputs, primary flag, dimensions and millimeter sizes. It verifies
its atom/name association and the original complete owned topology first. No
provider enumeration/capture occurs during the intermediate absence; this is
not a claim that the two server requests are globally atomic. The test never
deletes/rewrites a pre-existing non-owned monitor or changes the screen mode,
original output-primary, root properties or other clients' resources. Temporary logical topology does affect the server, hence
exclusive disposable ownership is mandatory. `SetMonitor` replies are checked;
visibility is polled for at most two seconds.

Drop attempts cleanup of every owned logical monitor, GC and window even after
an assertion failure. Owned monitors retain their name/atom and explicit
Present/Absent/Unknown state across replacement. Successful checked writes also
check the connection; pending/failed writes remain Unknown. Cleanup reconciles
Unknown against a real GetMonitors(get_active=false) reply, including inactive
monitors, and deletes only an owned atom found present. A confirmed deletion or
observed absence needs no second DeleteMonitor. A failed query leaves cleanup
unresolved and reports failure, never guessed absence. Original write errors
always fail the test, even if cleanup subsequently succeeds. Cleanup then verifies
restoration of the original monitor facts and unchanged output-primary. Cleanup failures fail the case (or are reported while
already unwinding). X11 atoms cannot be deleted: uniquely named fixture atoms
persist until the disposable server exits. The runner must tear down **only its
recorded owned server** in a finally/trap path, including after external timeout.
No `killall`, shared-display reset or guessed resource cleanup is appropriate.

Synchronous XCB reply waits themselves have no cancellation deadline. The
external timeout below is the hard safety bound; a killed process cannot promise
Drop ran. Treat a timeout or cleanup failure as failure and discard its server.

## Commands for CC (not executed by the implementation worker)

Run from the repository root. First compile the full Linux test target:

```sh
cargo check --locked --offline --all-targets
```

After the runner has prepared and exclusively reserved its disposable server:

```sh
env -u WAYLAND_DISPLAY DISPLAY=:199 XDG_SESSION_TYPE=x11 \
  RPA_X11_TEST_DISPLAY=:199 \
  RPA_X11_TEST_ISOLATED=owned-disposable-xvfb \
  timeout 90s cargo test --locked --offline --lib \
  backend::linux_display::tests::native_x11:: \
  -- --ignored --nocapture --test-threads=1
```

Require **4 passed, 0 failed, 0 ignored** within this filter; retain complete
stdout/stderr and exit status. An empty filter/zero executed cases is not success.
If the offline build cannot resolve cached dependencies, fix infra rather than
changing the lockfile. Allow a separate compilation stage before the 90s native
window if building on a slow runner (CC may use `cargo test --no-run`).

### Root-pixel / font DPI matrix

On the runner's owned server only, the runner may configure `RESOURCE_MANAGER`
using its X resource utility before invoking the test. Example with `xrdb`:

```sh
printf 'Xft.dpi: 144\n' | DISPLAY=:199 xrdb -merge

env -u WAYLAND_DISPLAY DISPLAY=:199 XDG_SESSION_TYPE=x11 \
  RPA_X11_TEST_DISPLAY=:199 \
  RPA_X11_TEST_ISOLATED=owned-disposable-xvfb \
  RPA_X11_TEST_EXPECT_XFT_DPI=144 \
  timeout 90s cargo test --locked --offline --lib \
  backend::linux_display::tests::native_x11::root_pixels_png_and_single_monitor_selections \
  -- --ignored --exact --nocapture --test-threads=1
```

Repeat independently with **96, 144 and 192**, changing both configured resource
and expected env value. Require one executed passing case per value. The test
reads and verifies that exact resource, then checks unchanged root geometry,
physical-pixel mapping, encoded size and every painted pixel across all three
selections. A wrong/missing resource fails rather than producing DPI evidence.
Tests never write the resource or mutate the process environment. Record each
scenario; the default run without expected-DPI env is not DPI-matrix evidence.

## Assertions and evidence limits

1. `root_pixels_png_and_single_monitor_selections`: original no-primary fallback,
   root dimensions independent of font DPI, PhysicalPixels mapping, decoded
   RGBA8 PNG dimensions, **all 11,520 painted pixels**, asymmetric mapped sample
   positions, and identical primary/exact-ID/desktop pixels. Three actual native
   reads/replies/decodes are required.
2. `nonzero_origin_primary_id_desktop_pixels_and_gap`: exact native request bounds
   for nonzero monitor origins, different color palettes on the two monitors,
   primary and both IDs, spatial desktop composition, black gap and gap mapping
   rejection. Requires five actual native reads and correct source byte permits.
3. `topology_requery_invalidates_mapping_and_rejects_stale_native_read`: real
   owned-monitor mutation between engine enumeration and native revalidation;
   rejects an unchanged selected primary because the **complete** topology
   changed. No GetImage/decode on the rejected attempt, stale map rejected after
   fresh requery, generation changed, then real capture recovers. The uninterrupted
   baseline provider must retain BOTH display IDs and all descriptor fields
   except the secondary x origin. The racing provider may retire attachment IDs
   after its failed capture, as required by existing production behavior.
4. `real_source_admission_precedes_getimage_and_decode`: same real provider and
   probe first produce verified pixels (including exact PNG-to-raw-RGBA equality),
   then source-bytes and total-bytes budgets independently reject. Requires typed
   BudgetExceeded, resource_limit mapping, and no additional real request/reply,
   payload bytes or decoder entry. No fabricated frames or permits are used.

`native/probe.rs` and its hooks exist only under `cfg(test)`. They observe calls
immediately before real GetImage submission, after its actual payload reply,
before source decoding, and after owned RGBA production. Positive controls check
the hooks are live. Unchanged counters establish that rejected attempts do not
reach this native image read/source-decoding path. They are **not malloc/RSS or
X-server allocator instrumentation**, and do not measure allocator bookkeeping,
X server internal overhead, or all topology-query allocations. Source inspection
places the decode entry before the checked source allocation; preserve this
qualification in evidence reports. No production behavior or supported topology
semantics are changed by this coverage.

## 2026-10-15 fixture interoperability correction (CC rerun pending)

Original independent evidence remains immutable:
`.agents/runs/linux-parity-20261015/cc-display-lease-x11-1/phaseB-container.log`,
lines 117–128, from frozen `review-gates-1`: 3 PASS / 1 FAIL, exit 101.
`move owned monitor` failed with RRSetMonitor (minor 43), BadValue, value **232**
before delegation to the real provider capture. The original fixture sent the
same existing atom and changed only secondary x=280 to x=288. The log did not
print the atom/name association: identifying 232 as that atom was source-based
inference, not separately recorded wire evidence.

Primary source inspection found a specific implementation/spec discrepancy:

- `xorgproto-2023.2/randrproto.txt:1675–1721` specifies same-name replacement
  by RRSetMonitor; output-name collision is a Value error.
- `xorg-server-21.1.12/randr/rrmonitor.c:492–500` rejects an existing monitor
  name and puts the name atom in errorValue. This occurs before its same-name
  replacement branch at lines 522–527. The environment build log records
  `xvfb 2:21.1.12-1ubuntu1.8`; the corresponding Ubuntu diff has no references
  modifying rrmonitor.c, RRMonitorAdd, or RRSetMonitor.

Reproducible source references inspected during diagnosis (not executed):

```text
https://archive.ubuntu.com/ubuntu/pool/main/x/xorgproto/xorgproto_2023.2.orig.tar.gz
SHA256 c791aad9b5847781175388ebe2de85cb5f024f8dabf526d5d699c4f942660cc3
https://archive.ubuntu.com/ubuntu/pool/main/x/xorg-server/xorg-server_21.1.12.orig.tar.gz
SHA256 f76a5878b0e6d16415cf0cd24ffc21090845fef3bc4ada45e57ea86b6c8fb75b
https://archive.ubuntu.com/ubuntu/pool/main/x/xorg-server/xorg-server_21.1.12-1ubuntu1.8.diff.gz
SHA256 5371522bd81cae405e5300957eb82ced84dea277eb5b7d8eba94e72176d37244
```

This change is a **fixture interoperability correction**, not a provider/product
fix and not a relaxation to an UNAVAILABLE/pass. It performs the real topology
mutation via explicit owned deletion/recreation while preserving native identity
inputs. Every previous stale-error, zero-read/decode, stale-map, generation and
fresh-recovery assertion remains; uninterrupted-provider ID stability is added.

New diagnostic lines record decimal atom, original generated name, old/new
bounds, confirmed deletion, verified recreation and cleanup state. GetAtomName
verifies the original atom/name before deletion. Future CC logs will establish
that association for the new run; they cannot retroactively prove the original
232 association. No new real-X result is claimed by authoring these diagnostics.

Rerun remains under coordinator scheduling: **wait for native-app33185 to end**
and explicit release of its reservation; do not overlap live suites. First CC
compiles with `cargo test --locked --offline --lib --no-run`, then runs the full
four-case command above on a fresh exclusively owned disposable server. Require
all four executed cases passing, same-atom BEGIN/CONFIRMED/VERIFIED diagnostics,
both retained-ID lines, and no cleanup failure. Preserve the original archive,
source manifest, report and RED log; attach rerun evidence to the new checkpoint.
