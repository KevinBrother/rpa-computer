# Linux/X11 emergency hotkey checkpoint

Linux uses **Ctrl+Alt+Shift+Escape**, as explicitly approved on 2026-10-15.
Windows remains Ctrl+Alt+F12; macOS implementation is unchanged. Earlier frozen
F12 failure evidence must not be relabeled as an Escape result.

## Registration and event policy

- Only native local X11 is supported; Wayland/XWayland and forwarded DISPLAY
  are explicitly rejected. No keymap, desktop controls, or user configuration
  is rewritten.
- `x11_plan` builds exact Control/Alt/Shift masks plus discovered Caps/Num lock
  variants. First non-NoSymbol identity preserves physical Alt's secondary
  Meta alias, without treating a dedicated Meta key as Alt.
- `x11/xkb_map` decodes the server's actual XKB types, actions, virtual modifier
  resolution, groups, behaviors, and controls. All accepted Escape cases must
  resolve to Escape with NoAction. `x11/xkb_events` subscribes before reading;
  dependent action/type/symbol changes and modifier/control/compatibility or
  keyboard changes revoke protection. Ordinary scratch-key edits are allowed
  only when they do not change the validated dependencies/modifier identities.
- Core grabs contain only core modifier bits. Grabs are checked individually on
  each root with synchronous keyboard mode. On a candidate press, QueryKeymap
  supplies held identities while event processing is frozen. A dedicated Meta,
  extra unwanted modifier, or missing required physical modifier is replayed
  with ReplayKeyboard at the event timestamp. A genuine emergency press sets
  the shared cancellation flag and releases the active keyboard grab.
- `x11_lifecycle` linearizes acceptance with failure/expiry. Failure is published
  before explicit cleanup, and an active protection failure sets cancellation.
- One registration-owned watchdog retains an owned duplicate of the same socket.
  It bounds setup and event/cleanup roundtrips by shutdown, not a borrowed raw-fd
  close or detached timeout thread. Normal idle processing does not wait for a
  new key event to stop. Healthy rollback is acknowledged; timed-out rollback
  disconnects, which a stopped server observes when it resumes. Disarm joins
  both owned threads. Plain guard drop retains the existing process-lifetime
  contract; callers must explicitly disarm before rearming.

## Open blockers and evidence boundary

**Not whole-product GREEN or acceptance.** Developer verification is compilation
only; no tests, SSH, GUI, input, commits, or pushes were executed in this checkpoint.

- F4 remains open: inherited authority-file reads in `x11/transport` are still
  synchronous. O_NONBLOCK on a regular file does not bound those operations.
  Migrate only after Gibbs's shared credential-only helper has independently
  verified native containment. Helper and root-lib ownership are not this lane's.
- Interim `x11_auth::local_display_number` still has the old u16 limit. The shared
  helper's checked c_int display policy must replace it during that migration;
  this checkpoint does not claim support for display numbers above 65535.
- No idle server heartbeat is implemented. An idle, stopped server without an
  event/roundtrip is not proactively detected. Explicit disarm still interrupts
  the owned socket; outstanding roundtrips have watchdog deadlines.
- Checked passive grabs and pure delivery policy assertions do NOT establish
  actual cancellation, replay to another focused client, XKB event decoding,
  action/type-only invalidation, or post-arm stopped-server cleanup. Those are
  independent CC real-X gates, on unchanged default Xvfb and without xmodmap
  workarounds. The old F12 failures remain authoritative for their snapshot.

## Independent CC commands (not executed by developer)

Compile the frozen archive in the existing Linux CC environment first:

```sh
cargo-1.89 test --locked --offline --lib --no-run
timeout 90s cargo-1.89 test --locked --offline --lib mcp::hotkey::x11_ -- --nocapture --test-threads=1
timeout 30s cargo-1.89 test --locked --offline --lib mcp::hotkey::x11::transport_tests -- --ignored --nocapture --test-threads=1
```

Source inventory: the `x11_` filter contains **57 default tests + 5 ignored
real-registration tests**; transport contains **3 ignored Unix-socket tests**.
These are source counts, not reported results. The broad review assertions are
unchanged: three plan initializers only gained `requires_identity: false`.

Real-X tests require separate coordinator authorization and isolated fixture
ownership. Run them by exact name rather than blanket `--ignored`. In addition
to idle/rearm, collision rollback, competing guards, real shared-flag delivery,
and stopped-setup tests, CC must independently verify exact-modifier negatives,
Meta/Alt+Meta replay, lock combinations, action/type-only invalidation, ordinary
scratch-map continuity, and post-arm stalled-server teardown. No real-X commands
in this file authorize input or a server signal.
