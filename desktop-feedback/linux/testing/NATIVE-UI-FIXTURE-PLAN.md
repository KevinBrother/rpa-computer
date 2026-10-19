# Next-stage minimal live UI qualification plan (not implemented/run here)

Prerequisite: independent GREEN for this R1/R2 snapshot's unchanged107 + extra10 assertions
and actual Linux Rust1.89 binary link without DOCS_RS. This plan does not block that handoff.
Next stage must implement/review the controlled parent + observer, then run them under the
separate live-UI authorization. No fixture executable is claimed to exist in this checkpoint.

## Ownership and bounded launcher

Use one fresh, disposable, privately namespaced test container with the approved native UI
libraries and frozen executable mounted read-only. No host X socket, host DISPLAY, input
/devices, Host executable or application credentials. No image rebuild/package install in
this plan; preflight missing Xvfb/Xauth/X11/Shape/RandR/XTEST/observer/font tools as a blocker.
Use a new private artifact directory and Xauthority cookie. For an otherwise empty private
container, the intended server command is:

```sh
Xvfb :91 -screen 0 1920x1080x24 -nolisten tcp -auth "$OWNED_XAUTHORITY"
```

The launcher must reject an existing :91 socket/lock, not delete it, and create the cookie
with `xauth -f "$OWNED_XAUTHORITY" add :91 MIT-MAGIC-COOKIE-1 "$FRESH_COOKIE"` first.
Child env explicitly sets DISPLAY=:91 and XAUTHORITY; removes WAYLAND_DISPLAY and sets
XDG_SESSION_TYPE=x11. Bare Xvfb is the minimal first profile: do not start/replace a compositor
or silently add a WM. Verify root geometry1920x1080, active RandR1.5 monitor, Shape1.1 and
style scale1 before cases. Missing supported facts are a qualification failure/blocker.

Launcher owns every PID/FD/window; keep exact PIDs and wait/reap them. Whole run deadline120s;
per case10s; Ready deadline3s; EOF/error observation deadline2s (record elapsed separately
from nominal200ms drain/300ms main wait). On overrun: save evidence, fail case, terminate/kill
only the owned process group and reap. Finally close own descriptors and terminate/reap own
Xvfb; never pkill, reset a shared display, kill another X client, or modify global DPI.

## Controlled parent IPC and independent observer

Parent uses `os.pipe`/`subprocess.Popen` or equivalent, NOT terminal/file stdin/stdout.
Use nonblocking reads and selectors with monotonic deadlines. Decode strict v1 NDJSON with
16KiB maximum per frame and capped logs (e.g.64KiB per case); stdout is protocol-only.
Keep input open until the case's EOF step; drain output continuously except the explicit
backpressure case. Do not treat an unparsed line, pipe buffering or a queued Stop as delivery.

An independent X client starts BEFORE the renderer. It creates a patterned target window
under the feedback area, explicitly focuses its own target, records real input events and
root child create/map/destroy events. Query renderer windows by BOTH child PID `_NET_WM_PID`
and `_RPA_DESKTOP_FEEDBACK_ROLE` (`status`, `stop`, `ring`); never use guessed XIDs or a global
name match. Verify actual attributes, geometry, input shapes and image pixels; retain
screenshots on this private display. Inject genuine XTEST press/release/motion through normal
hit-testing, not XSendEvent directly to a renderer window (which bypasses clickthrough proof).
Focus and pixel observation are separate from source properties or fake-native receipts.

## Minimum cases, each a fresh renderer

1. **Real startup / feedback / clickthrough.** Launch actual binary with `--label 'AI Control'`.
   Require exactly one Ready with `capture_exclusion:"unsupported"`, pointer_feedback true.
   At Ready, status/Stop are viewable, their image pixels contain drawn UI, and the ring has
   initialized resources (not necessarily mapped yet). Inspect event trace/checked X ordering
   for pre-ready presentation; cross-client log timestamps alone are not a scan-out fence.
   Root scale1 yields separate88x48 Stop; status/ring input shapes empty, Stop full rectangle.
   Focus must remain on observer target throughout mapping and painting.

   Send this complete bounded line (LF-terminated, real geometry required):

   ```json
   {"type":"snapshot","version":1,"sequence":1,"session":{"id":"fixture-a","generation":1},"phase":"executing","cleanup":"not_needed","surface":{"id":"desktop","version":"fixture-1920x1080","x":0,"y":0,"width":1920,"height":1080},"pointer":{"x":600,"y":400,"kind":"click"}}
   ```

   Observe actual ring pixels at600,400 during its650ms lifetime. Click ring center/stroke and
   status; target must receive actual clicks and retain focus. Exercise idle/paused/stopping,
   then cleanup failed+closed with increasing sequences; inspect actual status pixels and
   failure priority, not just send-success. Keep all frames explicit, not state deltas.

2. **Real Stop / stale grant.** With fresh active fixture-a/g1, XTEST click the observed Stop
   rectangle (including edge cases). Decode exactly one matching Stop, and no duplicate on
   another click. Verify focus unchanged and the underlying target does NOT receive it.
   For a fresh process: press under g1, send g2 while held, wait for the new presentation,
   release: no g2 Stop from the old press. A fresh g2 click must yield exactly one g2 Stop.
   Save wire, input/focus observations, XIDs/geometries and rendered disabled-state evidence.

3. **Normal EOF and startup cancellation.** Drain prior output; close only parent input.
   Expect exit0 with confirmed drained output, no new Stop/error or cleanup-success claim,
   and owned windows destroyed. Separately close input before child launch and during startup
   observations; retain all map/Ready/output timing. Race coverage is NOT deterministic proof
   of every internal boundary: EOF is ordered when the worker observes it, Ready at its
   mailbox commit; a previously committed in-flight Ready is not silently discarded. Blocking
   native-call cancellation still uses the watchdog; do not claim synchronous X preemption.
   Send truncated JSON then EOF separately: expect nonzero, never clean cancellation.

4. **Actual output EAGAIN + final drain.** Parent supplies its own stdout pipe and retains a
   duplicate write FD only for bounded fixture prefill. After draining Ready and showing a
   current grant, pause reading; fill the pipe nonblocking with distinct fixture sentinel
   bytes until even a1-byte write gets EAGAIN. Record capacity and exact sentinel byte count
   separately; close that duplicate afterward. Never parse/count sentinel bytes as renderer
   protocol. Keep the read end open. Perform an actual Stop click; independently observe
   disabled Stop repaint before EOF to show the UI processed it. Close input promptly, before
   the1s queue-age timeout; require bounded **nonzero** exit, not0 after the200ms drain expiry.
   Preserve child exit and full byte accounting. If the queue cannot be established from
   evidence or timing slips to another failure gate, mark inconclusive and do not claim R1
   live proof. In a separate case close the output reader during drain to exercise EPIPE;
   its nonzero result must not be reclassified as clean input EOF.

Record binary/source/image hashes, package/font inventory, per-case rc/timing, raw bounded wire,
X events/attributes/focus/target input, screenshots and owned-resource cleanup. Xvfb is only a
test environment; a pass is not a real-desktop WM/compositor, mixed-DPI, Unicode font inventory,
active-pointer-grab or scan-out qualification. Those broader profiles remain later gates.

**Never enable Host admission. Never change exclusion from unsupported.** Screenshots here
observe the visible UI; they do NOT prove genuine clean model captures. No capture-time
hiding/unmapping, pixel scrubbing, new provider or compositor alteration is part of this plan.
