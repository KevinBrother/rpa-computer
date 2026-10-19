# Stage 3 writer review seam — action-only / type-only invalidation

Date: 2026-10-16. **Design/API checkpoint only. No live execution, no acceptance
claim, no product or frozen fixture/runner changes.** Scratch continuity is a
later stage, not part of this fixture. Do not run a mutation from this document.

## Why stop for this review seam

The existing generated xcb 1.7.1 bindings provide a checked `xkb::SetMap` and
component-specific payloads. However, a keyboard-master map write can propagate
to attached key-capable devices. Core-only restoration is not sufficient. This
fixture must inventory, snapshot and read back the affected set, not assume that
`UseCoreKbd` names exactly one independently mutable keyboard.

Existing Cargo enables xcb `randr,xkb`, not `xinput`. **Request to manifest owner:
add xcb feature `xinput` at the same pinned version; no new package/version.**
No manifest edit is made by this lane. The vendored xcb protocol XML declares
`XIQueryDevice(deviceid)` (opcode 48), `XIDeviceInfo`, and key-class information;
the generated `xinput::XiQueryDevice`/device-class accessors must be compile-checked
when the feature is supplied, not assumed to be present in the current build. Without that reviewed binding availability,
stop at this design checkpoint; do not silently replace inventory with hardcoded
Xvfb device IDs, a core-only snapshot, or a global Xlib error handler.

Options considered:
1. **Recommended:** typed XCB XI2 inventory + typed XKB checked writes, all on one
   fixture connection; a bounded server fence covers each read/modify/readback
   transaction, not the product's asynchronous invalidation wait.
2. Xlib/Xi inventory plus XCB mutation: introduces a second connection and error
   handling/fencing concerns unnecessarily; not selected.
3. Raw hand-encoded XI requests or probing guessed device IDs: avoids a feature
   flag but expands unsafe protocol parsing and weakens identity checks; rejected.

## Two separate tests and two separate server lifetimes

Proposed exact tests under `x11_registration_tests::invalidation_qualification`:

- `action_only_change_revokes_and_cancels`
- `type_only_change_revokes_and_cancels`

Both ignored, no input APIs linked/called by these modules. Each requires its
own fresh authenticated default-map single-screen Xvfb, its own runner child
identity and a distinct explicit opt-in value:

```text
RPA_X11_HOTKEY_INVALIDATE_QUALIFY=owned-disposable-action-only-v1
# OR, never both / never reuse the first server:
RPA_X11_HOTKEY_INVALIDATE_QUALIFY=owned-disposable-type-only-v1
RPA_X11_INVALIDATE_RUNNER_PID=<live direct parent>
RPA_X11_INVALIDATE_SERVER_PID=<retained own server child>
RPA_X11_INVALIDATE_SERVER_START_TICKS=<positive proc field22>
DISPLAY=:<fresh canonical number>
XAUTHORITY=<fresh private authority file>
XDG_SESSION_TYPE=x11
WAYLAND_DISPLAY=
```

A later disjoint Stage 3 runner will copy reviewed pidfd/private-auth primitives
rather than editing/importing frozen runners. One invocation selects ONE exact
test/server; no blanket ignored run, retries, resets or input. No Stage 3 runner
is authored/authorized for execution at this checkpoint.

## Baseline and healthy-arm proof

1. Preserve existing ownership strictness: exact parent/start ticks/UID/executable/
   argv and server-owned listening inode; reject unexpected root children,
   preheld keys/buttons, group/latched state, nondefault carrier map or focus.
2. Negotiate XKB and XI2. Resolve the concrete master ID from `GetMapReply`,
   enumerate XI2 devices/classes, retain bounded identity records. Include the
   enabled key-capable attached set, including a slave pointer with a key class
   reached through the paired master pointer. Reject unsupported topology,
   unobservable identity, excessive counts or topology changes.
3. Before arming, under a bounded SAME-connection server fence, capture every
   affected device's full XKB map, controls, compatibility and state; also capture
   complete core symbols/modifiers, root/focus/pointer/held baseline. Keep both
   full normalized snapshots and exact component restoration payloads. No write.
4. Restrict this fixture to the observed default Escape layout: key 9, one group,
   one level, existing ONE_LEVEL type index 0, no entries/preserve/virtual modifiers
   in that type. Baseline Escape actions must be absent or all NoAction. Refuse
   deviations rather than rewriting a map to create a passing fixture. Every
   affected device's target component must agree with the master before mutation;
   otherwise no write, because one broadcast restore cannot restore divergence.
5. Observer proves all four passive grabs vacant; product `arm` succeeds; every
   independent checked grab now returns specifically BadAccess. Product status
   remains Armed and cancel=false after this barrier. Start all assertions from
   the real default map, not a preinstalled type/action mutation.

## Exact component writers / XCB bindings

Both requests target the resolved concrete master, use the existing min/max
keycodes, `SetMapFlags::empty()` (NO RESIZE_TYPES or RECOMPUTE_ACTIONS), zero every
unrelated range/total/virtual mask, and contain exactly ONE `SetMapValues` variant.
`present` is derived by the generated binding from `values`, not a caller mask.
Checked send + checked completion + independent readback are all mandatory.

### Action-only

`SetMapValues::KeyActions { actions_count: vec![1], actions: vec![...] }`,
`first_key_action=9`, `n_key_actions=1`, `total_actions=1`.

Install exactly one valid `Action::SetMods { flags: 0, mask: 1, real_mods: 1,
vmods_high: 0, vmods_low: 0 }` in Escape's sole slot. This is a real action-table
change, not a keysym alias or fake event. The key must NEVER be pressed while
changed. Restore the exact saved action count/list (including zero count/zero
total when originally absent), not a guessed NoAction replacement.

Request and readback must demonstrate: only Escape's actions changed; types,
symbols/type indices/group widths, explicit flags, behavior, modifier/vmod maps,
controls/compatibility and other device/key components are unchanged.

### Type-only

Use `SetMapValues::KeyTypes(vec![SetKeyTypeBuf::new(...)])`, `first_type=0`,
`n_types=1`; every key-symbol/action range and total stays zero.

Change the existing one-level type's real modifier/mask from empty to SHIFT;
keep `num_levels=1`, no virtual modifiers, no map entries and no preserve entries.
This changes a real dependent type descriptor while preserving its mandatory
width, Escape's type index, all key symbols and all action tables. It is a
conservative dependency-invalidation probe: it need not make Escape unusable to
require invalidation. Do not claim it demonstrates an input action was consumed.
The readback must confirm the modifier mask actually changed, not merely that a
no-op SetMap returned success. Restore the exact original descriptor.

The generated `SetKeyTypeBuf::new(mask, real_mods, virtual_mods, num_levels,
preserve, entries, preserve_entries)` accepts `KtSetMapEntry` for preserve entries,
whereas a `GetMap` type returns `ModDef`. This seam avoids that representation
conversion entirely by requiring empty preserve/entries in the selected original
type. General type rewriting is out of scope; do not cast GetMap wire payloads
into SetMap layouts or change a different type to bypass that precondition.

## Proposed local API (test-only, not product seams)

```rust
enum Component { EscapeAction, EscapeType }
struct DeviceIdentity { /* id, use, attachment, enabled, key-class, name */ }
struct Original { /* all device snapshots + root/core/state/focus baseline */ }
struct Patch { /* one component, owned changed AND restore SetMapValues */ }
struct MutationLease { /* original, patch, owned connection, may_have_written */ }

fn inventory(c: &Connection) -> Result<Vec<DeviceIdentity>, FixtureError>;
fn prepare(c: &Connection, kind: Component) -> Result<(Original, Patch), FixtureError>;
fn request<'a>(patch: &'a Patch, restore: bool) -> xkb::SetMap<'a>;
fn apply_checked(lease: &mut MutationLease) -> Result<MutationProof, FixtureError>;
fn restore_and_verify(lease: &mut MutationLease) -> Result<RestoreProof, FixtureError>;
```

No product private state injection, manual cancellation, fake MapNotify,
SendEvent, XTest, core `ChangeKeyboardMapping`, XkbSetMap via Xlib, or keyboard
reset. Raw protocol/reply helpers, if necessary, remain inside this test lane.

Suggested small modules: `mod.rs` (two tests), `environment.rs`, `inventory.rs`,
`snapshot.rs`, `writer.rs`, `writer_tests.rs`, `transaction.rs`, `deadline.rs`,
`README.md`. Register only via a new append at the existing test registration.
No existing Stage1/Stage2 utility will be modified to export additional fields.

## Mutation proof and actual invalidation

Before mutation, subscribe an independent observer to real XKB MapNotify and
core MappingNotify, drain its bounded preexisting queue and require stable healthy
arm. Under the fixture writer's bounded server fence, recheck device identity and
all baselines; set `may_have_written=true` BEFORE sending the request (also on a
checked-error path). Check request, read all affected components, prove the exact
expected delta, release the server fence. Never wait for product while fenced.

After release, within 2s require BOTH `HotkeyStatus::Failed` and cancel=true,
with the failure diagnostic identifying mapping invalidation, not timeout or
fixture-initiated teardown. Require a genuine observer XKB MapNotify for the
specific component/range, SendEvent bit clear; fail the component-only gate if
other relevant components changed. Core symbols must independently remain equal.
Record incidental core events; if they provide an alternative invalidation path,
do not claim isolated XKB-subscription proof. No input at any point.

Only after observing failure/cancel call disarm; bound it, check single remaining
cancel Arc reference and independently prove old passive grabs are absent on the
still-live server. Disarm must not be used to cause the asserted failure/cancel.

## Restoration and boundedness

- Restoration is armed before the first possible write, including requests that
  error or lose their reply. A normal failure/unwind runs a separately bounded
  restore attempt; cleanup errors are accumulated without a second panic.
- Recheck affected identity, send ONLY the saved component back, check completion,
  then read back ALL device/core/configuration/state/root/focus baselines. A
  successful SetMap acknowledgement or only the master matching is insufficient.
- Fences are short capture/modify/restore scopes. No global server grab spans
  product arming, cancellation polling, disarm, or independent grab observation.
- Proposed per-transaction absolute budget: 2s; cleanup budget: 3s; event polling:
  2s and bounded queue count. Use an owned duplicate of each fixture transport for
  watchdog shutdown so blocking XCB calls cannot hold a server fence indefinitely.
  No raw-fd close/reuse race, detached join or bare-PID signal.
- Fixture connection setup is separately bounded by the owned-runner deadline;
  no map write occurs during setup. This does not claim the frozen product's
  end-to-end auth blocker has been repaired.
- If a timed-out writer transport is shut down, its server fence is released but
  its map mutation is NOT automatically undone. Reserve a separately watched
  recovery connection established before mutation for the one bounded restore
  attempt. A cross-client roundtrip alone is NOT proof that queued writer requests
  are gone. Before any mutation, give the writer a private 1x1 pixmap sentinel
  (`x::CreatePixmap`, normal DestroyAll close-down mode), not a root child/window.
  On writer timeout, the recovery connection must observe specifically BadDrawable
  from checked `x::GetGeometry` on that sentinel after transport shutdown. Only
  server-side owner-resource destruction establishes that the old client cannot
  later apply a buffered mutation. Do not free the sentinel to fake this barrier.
  If the bounded disappearance check fails, do NOT race a restoration against the
  old writer; report restoration unknown and request owned-server disposal. On
  normal cleanup, free the sentinel only after exact restoration. Lost identity/
  server responsiveness or unsuccessful exact readback
  yields restoration UNKNOWN/FAILED, never PASS; private runner disposes only its
  owned server after preserving evidence. Restoration is attempted even on product
  assertion failure, but cannot be promised if the server/kernel is unresponsive.

## Tests-first gate after API review

Before live writer execution, author pure tests for:
1. Each request has exactly its intended component, range and totals.
2. Both requests prohibit symbol changes, resizing and recomputation.
3. Action restore preserves zero-count absence versus allocated NoAction slots.
4. Type mutation/restore preserves width, index, entries and preserve policy.
5. Nondefault layout/target disagreement/topology ambiguity refuses before write.
6. Delta comparison rejects unchanged target, unrelated changes or wrong device.
7. Mutation ownership is armed before send; checked error still restores.
8. Cancellation/status must precede explicit disarm; timeout cannot pass as revoke.
9. Readback mismatch or timeout is never classified as restored.
10. Restore failure cannot hide the original assertion or manufacture qualification.

Then author the two ignored live fixtures. Independent CC owns execution and
verdicts. A guard/protocol writer rejection is infrastructure failure, not fabricated
product RED. Freeze compile-ready source and exact commands before any scheduling.

## Review decisions requested

Approve the narrow one-level type-mask mutation and Escape SetMods action;
approve the affected-device snapshot/fanout guard and the `xinput` feature request;
approve bounded writer/recovery transports with the server-observed writer-gone
barrier as the cleanup boundary. Only then
implement the writer/test API, keeping action and type live gates separate.
