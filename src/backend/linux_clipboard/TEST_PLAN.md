# Linux/X11 explicit clipboard paste — RED handoff and implementation boundary

**Status:** tests-only. No clipboard production implementation or test execution
by the implementation worker. Existing approved parity spec/plan remains the
source of scope. Independent real Claude CLI (CC) executes all tests.

## Decisions and risks recorded before implementation

- Explicit paste replaces **CLIPBOARD only**, never reads/restores the prior
  clipboard and never mutates PRIMARY. No native fallback, hidden retry,
  focus change, layout change or enter/tab injection from pasted data.
- UTF-8 is preserved verbatim, including tabs and CR/LF; Windows CRLF handling
  remains Windows-only. Validate all 4096 Unicode scalars before mutation.
  Maximum payload is 16,384 bytes. Check actual inline ChangeProperty capacity
  before publishing; insufficient capacity fails without starting INCR.
- Advertise TARGETS, UTF8_STRING, TIMESTAMP and MULTIPLE. Do not advertise
  STRING, TEXT, INCR, SAVE_TARGETS or manager persistence. Bound MULTIPLE to
  64 pairs with a bounded property read, reject malformed/truncated/aliased
  lists before writes, mark unsupported pairs failed and send one notify.
  At the maximum, MULTIPLE may plan 1 MiB of text plus small metadata, not an
  unbounded allocation; service outside any server grab and check stop between
  writes. INCR is intentionally unnecessary with the strict inline bound;
  introducing larger payloads later requires a separately bounded transfer FSM.
- TIMESTAMP is acquisition **server time**, not local wall clock or zero. Use
  wrapping X timestamp comparisons. A real owner event loop must answer
  requests; storing a Rust String or only calling SetSelectionOwner is not paste.
- Driver lifetime retains already published text for requestors. Session
  cleanup invalidates pending authorization but does not erase a successfully
  published selection. Driver/host exit ends that guarantee. An independent
  clipboard manager may save contents; this code cannot promise it will.
- Receipt expiry, generation, focus, physical Ctrl/Shift/Alt/Meta/V, selection
  owner and irreversible lease revocation are guards, not an application
  receipt. Existing BackendCore retains tracked Ctrl+V and held-input rejection.
  Actual receiving-application tests remain mandatory and pending.
- Canonical writer selection, owner and exact STRING/8/16-byte nonce must be
  checked on the SAME XCB connection inside a short server grab immediately
  before publishing. Token identity access is read-only, not authority itself.
  Foreign owner/nonce mismatch revokes all token clones. No environment/global
  lease lookup and no authorization from a file lock.
- No connection/auth handshake, arbitrary client wait, event service, payload
  allocation, sleep or thread join inside a grab. Bound replies and cleanup;
  abandon/stop must fence late setup and mutations. Cancellation after an X
  request was already queued cannot retract its side effect: report uncertainty,
  never pretend it was unchanged, retry automatically, or report clean release.
  The model tests check boundaries/order; they do NOT prove socket liveness or
  post-cancellation thread quiescence. Those require the later fault fixture.

## Ownership and integration

This worker creates/owns:

- `src/backend/linux_clipboard/mod.rs` (test registrations only at RED)
- `src/backend/linux_clipboard/policy_tests.rs`
- `src/backend/linux_clipboard/protocol_tests.rs`
- `src/backend/linux_clipboard/authority_tests.rs`
- `src/backend/linux_clipboard/dispatch_tests.rs`
- `src/backend/linux_clipboard/support.rs`
- `src/backend/linux_clipboard/live_tests.rs`
- `src/backend/linux_clipboard/TEST_PLAN.md`
- `tests/linux_clipboard_lease_identity.rs`
- `src/backend/dispatch.rs` and new `src/backend/dispatch_tests.rs` (existing
  inline tests extracted without semantic edits; native cleanup forwarding and
  `dispatch_cleanup_tests.rs` registration preserved).

Display owner, via coordinator, must register exactly:

```rust
#[cfg(any(target_os = "linux", test))]
mod linux_clipboard;
```

After independent RED, not before: native owner exposes the existing Linux
`X11Lease::identity(&self) -> (u32, u32, &[u8; 16])` read-only method publicly.
Do not move tests into the native crate: the external visibility failure is
intentional. Keep `Clone`, `revoke()` and `is_revoked()` semantics intact.

Later display ctor handoff (no edit by this worker): preserve the token by
passing `lease.clone()` to `native_driver_with_x11_lease`; then construct
`BackendCore::new(DriverInput::new_with_x11_lease(driver, lease))` on Linux.
Other-platform construction stays unchanged. Linux unleased `DriverInput::new`
remains useful for headless tests but refuses nonempty explicit paste with
`permission_denied` before connecting. Clipboard connection setup is lazy;
constructing the adapter does not itself publish or connect.

## Exact independent CC RED commands

Run from a frozen **Linux** root snapshot with the display declaration above.
Do not invoke this from macOS and call a cfg-skipped test RED. Other lanes may
be frozen to their compiling snapshots by the coordinator; no worker edits to
those lanes. No X server or GUI is needed for RED. The first command builds the
normal root library rather than its unit-test modules, isolating external API
visibility from the intentionally absent clipboard modules.

```sh
cargo test --locked --offline --test linux_clipboard_lease_identity -- --nocapture
cargo test --locked --offline --lib backend::linux_clipboard -- --nocapture --test-threads=1
```

Run both independently, recording exit status (not `&&`, which would skip the
second expected failure). Use the test environment's approved `cargo-1.89`
binary instead of `cargo` if that is its existing installed compiler wrapper.

Expected RED 1: E0624 private `X11Lease::identity` in the integration test.
Expected RED 2: unresolved `super::policy`, `super::protocol`, `super::transaction`
and absent `DriverInput::new_with_x11_lease`. These are intentional production
seams, not test-local implementations. Unexpected fixture syntax/type errors,
missing module registration, missing vendor/cache/dependencies, unrelated-lane
compile failures, linker/toolchain failures or zero selected tests do NOT prove
these RED contracts. Record them separately and return diagnostics.

**Freeze here.** Do not implement policy/protocol/owner/adapter until the
coordinator provides CC's RED evidence and authorizes the implementation turn.

## GREEN execution after implementation (CC only)

```sh
cargo test --locked --offline --test linux_clipboard_lease_identity -- --nocapture
cargo test --locked --offline --lib backend::linux_clipboard -- --nocapture --test-threads=1
cargo test --locked --offline --lib backend::dispatch -- --nocapture
```

CC also runs existing BackendCore text paste regressions and Windows clipboard
regressions on their appropriate platforms. Unchanged Windows contracts cannot
be weakened to accommodate Linux.

## Ignored real X11 fixture plan (CC only, later)

`live_tests.rs` contains actual XCB requestor fixtures, not passing placeholders.
They require explicit `RPA_COMPUTER_X11_CLIPBOARD_FIXTURE=disposable` and an
owned, fresh X11 server with no clipboard manager and no preheld keys. They
refuse occupied canonical writer/CLIPBOARD/PRIMARY selections. They deliberately
create/map/focus only their own fixture windows and use a recording input driver:
no native key chord is emitted, and results are NOT receiving-application proof.

On an already provisioned, exclusive disposable display, CC runs each fixture
in a separate process with an outer deadline (connect/auth is not pollable here):

```sh
RPA_COMPUTER_X11_CLIPBOARD_FIXTURE=disposable timeout 30s cargo test --locked --offline --lib backend::linux_clipboard::live_tests::live_owner_serves_real_utf8_targets_timestamp_and_unsupported_reply -- --ignored --exact --nocapture --test-threads=1
RPA_COMPUTER_X11_CLIPBOARD_FIXTURE=disposable timeout 30s cargo test --locked --offline --lib backend::linux_clipboard::live_tests::live_cleanup_keeps_owned_text_but_drop_finishes_owner_without_daemon -- --ignored --exact --nocapture --test-threads=1
RPA_COMPUTER_X11_CLIPBOARD_FIXTURE=disposable timeout 30s cargo test --locked --offline --lib backend::linux_clipboard::live_tests::live_stolen_selection_and_revoked_lease_block_chord_and_future_mutations -- --ignored --exact --nocapture --test-threads=1
RPA_COMPUTER_X11_CLIPBOARD_FIXTURE=disposable timeout 30s cargo test --locked --offline --lib backend::linux_clipboard::live_tests::live_focus_change_consumes_receipt_without_changing_focus_back -- --ignored --exact --nocapture --test-threads=1
```

Compile first so the 30s deadline measures a fixture, not a cold build. Run one
at a time. Timeout is FAILURE/unknown, never a clean owner-lifetime result.
Fixture isolation/provisioning is CC/coordinator infrastructure, not product
Docker integration. Further real checks required before parity completion:

1. Wire-level MULTIPLE success/partial-failure/malformed/oversized properties,
   legacy property=None conversion, requestor destruction mid-response.
2. Physical left/right modifier and V preholds, including non-US layout,
   without synthesizing release of keys owned by another actor.
3. Writer theft/nonce edit/revocation during prepare and verify, new clipboard
   owner stealing then releasing, repeat paste after receipt expiry/failure.
4. Stalled/disconnected X socket during connect, reply, flush, acquisition and
   shutdown: inspect threads/socket/grab lifetime and establish no late writes.
5. Focus switch and cancellation around tracked Ctrl+V with native raw driver;
   never count modeled guards alone as race freedom.
6. Actual owned receiving application checks exact UTF-8 mixed-language text,
   4096-scalar payload, tabs/newlines as data; compare native vs explicit paste
   independently and observe receipt, not merely API success.
7. Clipboard manager absent/present, driver/host exit and owner replacement:
   report observed persistence honestly; no background daemon or restoration.
