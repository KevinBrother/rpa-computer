# Clipboard GREEN candidate handoff — 2026-10-15

## Status and provenance

Implementation candidate frozen for independent Linux linked build, CC tests
and code review. No tests/SSH/GUI were executed by this worker. No commits,
pushes, worktrees or dependency changes. This is NOT completed Linux parity.

Independent clipboard RED was supplied by the coordinator:
`cc-clipboard-red-1/probe-clipboard-red-1.log`: external E0624 (two identity
access sites), then E0432 for absent policy/protocol/transaction; exit 101 each,
no assertions ran. Immutable archive SHA-256:
`439b2fa2d523b03c7e12a73c1e495c689e5c0b23fb1e0eafa0046e4a164f238e`.

F4 assertion RED was separately supplied: native 2 pass/4 fail, host 1 pass/3
fail. Only afterward, dispatch gained `NativeInput::control_text_uses_scalar`
default false and DriverInput forwarding. Native owner owns Driver capability
and Input/BackendCore changes; this worker did not edit dispatch_core.rs.

Original RED test/support/TEST_PLAN contents and extracted dispatch_tests.rs
retain their frozen hashes. The four native/transport_tests.rs tests are new
supplemental GREEN coverage, not retroactively claimed as RED observations.

## Implemented design

- DriverInput explicitly receives the native X11Lease, lazily creates its
  clipboard owner, and refuses unleased paste before native transport work.
- Preserve shared Windows code/behavior and native cleanup forwarding. Cleanup
  combines native and clipboard failures; a healthy owner survives session
  cleanup while receipts do not. Driver drop ends the clipboard owner lifetime.
- Pure policy validates 4096 scalars/16,384 bytes; Linux keeps UTF-8/CR/LF/tab
  bytes unchanged. No previous clipboard contents are read or restored.
- Pure bounded response planner supports TARGETS/UTF8_STRING/TIMESTAMP and
  MULTIPLE (64 pairs). No INCR/STRING/TEXT/SAVE_TARGETS advertisement. Check
  the server's core request size before publishing; unsupported conversions
  reply with property=None. Malformed/truncated/aliased MULTIPLE is refused.
- Real core-XCB owner event loop, acquisition server timestamps, bounded
  metadata reads/replies, requestor-error handling and selection-loss fencing.
  PRIMARY, focus and keymap/layout are never modified by production paste.
- Canonical writer selection + exact owner/STRING-format8-16-byte nonce and
  irreversible revocation checked under the same short connection grab before
  publication. Focus/physical modifiers/V, selection ownership, generation,
  receipt TTL and one-shot checks gate the existing tracked Ctrl+V path.
- The owner and deadline watchdog share one preopened Unix socket through owned
  duplicates. Shutdown interrupts XCB setup/read/flush/write, including idle
  selection service. Only finished threads are joined; explicit teardown
  retains unfinished handles and reports termination unconfirmed after bounded
  grace. Drop interrupts the sole transport before handle disposal; it cannot
  promise scheduler-immediate thread destruction. No worker reconnects.
- Backend does not import MCP. Binary-safe auth/local-socket/raw-XCB setup was
  adapted locally from the hotkey implementation. No new crate dependencies.

## Explicit pending blocker: arbitrary auth-file I/O

`native/transport.rs::load_cookie` runs `OpenOptions::open`, `metadata` and
regular-file reads **before** the watchdog/socket exist. The 1 MiB bound,
O_NONBLOCK, FIFO/device rejection and checks between reads do **not** provide
an interruptible deadline for a regular file on a stalled NFS/FUSE filesystem.
Consequently paste preparation can remain blocked there; cancellation/timeout
is not fully bounded for arbitrary inherited HOME/XAUTHORITY paths. Later
checks prevent a late return from publishing, but cannot unblock the file read.

This remains a **liveness blocker**, not an accepted guarantee or a passing
transport test. Socket shutdown does NOT solve arbitrary filesystem I/O.
Requires a coordinated, reviewed containment design (for example a killable
helper process with bounded credential IPC and reap semantics) and independent
fault tests. Do not silently replace it with another detached reader thread,
recv_timeout-only wrapper, or an unapproved environment restriction. Sharing a
future fixed low-level transport with hotkey needs coordinated file ownership.

## Compile-only evidence

Successful locally, no test binary run:

```sh
cargo check --locked --offline --all-targets
cargo build --locked --offline -p xcb --target x86_64-unknown-linux-gnu
cargo build --manifest-path crates/native-input/Cargo.toml --locked --offline --target x86_64-unknown-linux-gnu
```

The first command is the full root **macOS** check. The latter two build Linux
rlib dependencies, not a linked Linux host. The native crate emitted an existing
dead-code warning during the recorded build.

A `rustc --target x86_64-unknown-linux-gnu --test --emit=metadata,obj` probe
also compiled the actual `keys.rs`, `dispatch.rs`, `dispatch_core.rs`, complete
`linux_clipboard/mod.rs` hierarchy and their tests, including raw C FFI setup,
transport watchdog and ignored real-XCB fixtures. The probe copies only exact
shared value-type declarations from backend/mod.rs and path-includes the actual
modules: no replacement behavior implementations. It excludes capture/MCP/
desktop construction and is NOT a full-root integration or link result.

A separate identical-target object compilation of
`tests/linux_clipboard_lease_identity.rs` succeeded against the updated native
crate. Both output files were identified as x86-64 Linux ELF relocatable objects:
`/tmp/linux_clipboard_compile_probe.o` and
`/tmp/linux_clipboard_lease_identity.o`. They were not linked or executed.

Attempted full-root Linux `cargo check --locked --offline --all-targets --target
x86_64-unknown-linux-gnu` failed in ring's build script because the local Mac
lacks `x86_64-linux-gnu-gcc`. This is an infrastructure failure, not GREEN.

## Independent CC commands on the Linux snapshot

Use the environment's approved cargo/cargo-1.89 executable. Run separately and
retain full logs plus exit codes. Normal unit commands do not need DISPLAY.
The host build must actually link against Linux X11/XCB libraries.

```sh
cargo build --locked --offline --bin computer-host
cargo test --locked --offline --test linux_clipboard_lease_identity -- --nocapture
cargo test --locked --offline --lib backend::linux_clipboard -- --nocapture --test-threads=1
cargo test --locked --offline --lib backend::dispatch -- --nocapture
```

The clipboard filter includes the four supplemental socket tests; four live X11
fixtures remain ignored. `backend::dispatch` also selects dispatch_core tests.
Run platform-appropriate existing Windows paste regressions separately; none
were weakened or executed here.

The four ignored, real-XCB fixture commands and disposable-display rules remain
in TEST_PLAN.md. They use a recording raw driver: success would prove selection
service/lifetime, not a real Ctrl+V reaching an application. The coordinator
must explicitly schedule these separately, serially, with outer timeouts.

## Review and remaining verification gates

1. Resolve the auth-file liveness blocker above; no full bounded-init claim yet.
2. Independent Linux linked host build, all clipboard assertions and regression
   execution; none are implied by metadata/object compilation.
3. Review raw FFI fd ownership and interrupt races; unknown cleanup status;
   requestor/property failures; deadlines while server-grabbed; stale ownership
   notifications and bounded MULTIPLE work. Native socket tests are supplemental
   and have not run; actual blocked XCB flush/disconnect/grab fault tests remain.
4. D-owned full host ctor integration was reported landed/frozen by coordinator:
   desktop.rs SHA-256
   `d81a9775e2985cb609a43a2d83e4a7d06f726637ab46393587249ca1fcd6927a`.
   Include that integration and matching native F4/identity changes in the host
   snapshot. This worker did not edit desktop.rs or backend/mod.rs.
5. Real X11 interoperability for MULTIPLE/property=None, requestor destruction,
   ownership steal-and-return, expiry/reprepare, physical left/right modifiers,
   alternate layouts, focus races and loss during Ctrl+V remains unverified.
6. Native GUI receiving application: exact mixed Unicode/control data, max
   payload and independent native-vs-paste behavior remain unverified. X focus
   identifies an X window, not every toolkit's internal focused widget. A
   successful chord is never an application receipt.
7. Only local Unix DISPLAY is supported, consistent with the adopted hotkey
   transport. TCP/SSH-forwarded DISPLAY is explicitly rejected. Wayland session
   environments and an advertised XWAYLAND extension are rejected; do not claim
   comprehensive detection of every old XWayland deployment.
8. Clipboard persistence after driver/host exit is not provided. Actual external
   clipboard-manager behavior and owner-lifetime tests remain unverified.
9. README/schema/support claims and cross-platform regression results are
   coordinator-owned outstanding integration work, not modified by this lane.

## Exact lane snapshot

`GREEN_MANIFEST.sha256` lists every owned lane source/test/document, plus
`src/backend/dispatch.rs`, extracted `src/backend/dispatch_tests.rs`, and the
external identity test. Manifest excludes itself. Existing
`src/backend/dispatch_cleanup_tests.rs` is a required unchanged scaffold input,
not rewritten here. Use the coordinator's native/root scaffold for other modules.
