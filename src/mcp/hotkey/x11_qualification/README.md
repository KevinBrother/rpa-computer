# Staged REAL-X qualification — author-only checkpoint

Stage 1 adds **one ignored test**, not a fabricated RED or claimed live PASS:

`mcp::hotkey::x11_registration_tests::qualification::default_map_real_cancel_and_negative_replay`

The production GREEN source and the previous five live test bodies are unchanged.
The only edit to an existing file is append-only registration in
`x11_registration_tests.rs`. No Cargo or other worker's files are changed.

## Stage 1 assertions

The receiver is a new, override-redirect window belonging to this test's own
connection. The test focuses only that window. Input uses libXtst's
`XTestFakeKeyEvent`, never SendEvent. No XkbSetMap, xmodmap, XkbLockModifiers,
keyboard reset, desktop-control changes, or server signals are issued.

For each Caps/Num state (neither, Caps, Num, both):

- Exercise left/right Control and Shift, both ordinary Alts and the additional
  Alt carrier. Ordinary Alts are tested with both Shift-first and Alt-first order.
  Carrier 204 is exercised Shift-first because its unshifted symbol is NoSymbol.
  **80 positive cases total** must actually set the shared atomic while the
  protection remains Armed; a runtime protection failure cannot count as a pass.
- **36 negatives total** cover dedicated Meta instead of Alt, Alt plus dedicated
  Meta, extra Super/Level3/Level5, each missing required modifier, and plain Escape.
  Both Meta cases deliberately have the exact grabbed modifier mask. They must
  reach the focused receiver as original, non-SendEvent Escape presses, followed
  by releases, without cancellation. These eight cases exercise replay rather
  than relying on an unmatched passive-grab mask. Other negatives exercise
  ordinary delivery. Press count/state, release completion, and a bounded
  post-delivery observation period are checked.

Before Escape, the expected held keys and exact effective modifier mask are
asserted. Carrier input that fails to establish that state is reported as a
fixture-state failure, never reclassified as a product cancellation result.
The test does not change mappings to obtain a positive result.

Core keymap/modifier tables and full XKB map, controls and compatibility reply
snapshots (request sequences normalized) must remain unchanged around each
case. The original complete XKB state, Caps/Num locks, input focus/revert mode,
empty held-key bitmap, root child list, and cancel-reference count are verified
at final cleanup. Caps/Num are changed and restored only with their actual key
presses. There is no all-key release/reset: cleanup releases only recorded keys
that this fixture injected. Cleanup attempts disarm, key release, lock restore,
focus restore, and owned-window destruction independently even during assertion
unwind, and prints `HOTKEY_FIXTURE cleanup exact_baseline=...`.

## Required runner guards — do not run against a shared desktop

CC must create a **new disposable, private, single-screen Xvfb** specifically
for this test, with its default keyboard map and `-noreset`. Do not reuse the
native-inventory, clipboard, host, or other fixture's server. No WM/clipboard
owner/other GUI client is allowed. The fixture refuses any baseline root child,
pre-held key/button, unexpected baseline focus/group/modifier state, or changed
default carrier identities/modifier-slot membership. Per-case ownership checks
refuse unexpected root children or loss of receiver focus.

Use a local canonical `DISPLAY=:N` with `N <= 65535` for this checkpoint because
the existing product parser limit remains an explicit blocker awaiting helper
migration; this is not a claim that larger X11 display numbers are invalid.

The live runner must be the direct parent of the Xvfb **and** an ancestor of the
test process, all in the same PID namespace/UID. It supplies:

```text
DISPLAY=:<owned-number>
XDG_SESSION_TYPE=x11
WAYLAND_DISPLAY=
RPA_X11_HOTKEY_QUALIFY=owned-disposable-default-map-v1
RPA_X11_HOTKEY_SERVER_PID=<runner's own Xvfb child PID>
RPA_X11_HOTKEY_SERVER_START_TICKS=<that child's /proc/PID/stat field 22>
RPA_X11_HOTKEY_RUNNER_PID=<live parent runner PID>
```

The fixture checks executable/command line, parent PID and start ticks, UID,
runner ancestry, and that the declared server owns a listening Unix socket inode
for this exact DISPLAY. It never signals that PID. Do not weaken these checks to
attach to an existing server, change container security profiles, or share other
workers' display sockets. Inherited authority credentials must match the owned
fixture; never print them.

**Boundedness boundary:** case polling/event batches are bounded, but Xlib/XCB
fixture calls can block if the fixture server dies/stalls. Run under the hard
external timeout below. The owning runner must retain its child identity, dispose
of/reap its own Xvfb and test descendants on EVERY exit (including timeout), and
record postflight. SIGKILL, Xlib fatal I/O, or process death bypasses Rust Drop:
then exact in-server restoration is **unknown**, not proven; discard that server.
Timeout, guard refusal, link/setup failure, or absent cleanup evidence is not a
positive qualification result. The runner must not signal unrelated servers.

## Exact CC commands (not run by author)

First compile/link without execution in the frozen Linux environment:

```sh
cargo-1.89 test --locked --offline --lib --no-run
```

Only after coordinator authorizes this private input slot and the owning runner
establishes the environment above:

```sh
timeout --signal=TERM --kill-after=5s 120s cargo-1.89 test --locked --offline --lib mcp::hotkey::x11_registration_tests::qualification::default_map_real_cancel_and_negative_replay -- --ignored --exact --nocapture --test-threads=1
```

Keep full output/exit status, server identity/default-map preflight, individual
`HOTKEY_CASE verified` records (80 positive, 36 negative), explicit cleanup record,
and runner-owned disposal/reap postflight. Do not use blanket `--ignored` or run
alongside any other real-input fixture. Test success requires actual cancellation
AND receiver evidence; the Armed log alone is never acceptance.

## Subsequent separate stages — not implemented or claimed by this checkpoint

1. **Post-arm stopped-server disarm:** a separate opt-in fixture must arm and
   establish healthy protection before the owning runner stops only its own
   Xvfb (identity-safe retained ownership, no guessed PID). Measure disarm with
   no incoming events, verify reference release, then have the owner resume and
   prove old grabs are released before disposal. Timeout/failure paths must also
   resume/dispose/reap only that owned server. Existing stopped-setup test is not
   a substitute.
2. **Action-only / type-only invalidation:** separate mutation opt-in and separate
   disposable server for each. Capture original full native map first, perform
   one checked action-only or referenced-type-only change with no input during
   the unsafe map, and prove failure + cancellation. Restore exact original
   components and verify full readback even on assertion failure; no core-symbol
   mutation may masquerade as action/type-only evidence.
3. **Scratch-map continuity:** a separate mutation fixture identifies a genuinely
   unused, non-modifier NoAction scratch key, snapshots all original components,
   applies an ordinary scratch-symbol edit, then demonstrates genuine emergency
   cancellation while protection remains healthy. Restore the exact core AND
   XKB components and verify readback. No reset-to-default approximation.

Those stages need reviewable mutation/restoration and runner ownership protocols;
this first stage intentionally does not mix map mutation or server signals with
the default-map cancellation/replay gate.
