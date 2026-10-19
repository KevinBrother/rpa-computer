# Safe GetMap decoder — GREEN implementation candidate, 2026-10-16

**Implementation + compile only. No developer test execution, SSH, GUI/input,
server launch/signaling, dependency upgrade or keymap change.** Not a claim of
passing tests, successful registration, actual cancellation or Linux parity.

## Gate and immutable baseline

Coordinator verified CC73671 against `hotkey-wire-review-red-2`, full362-file
archive SHA256 `ee75ef8fca0d530774d79045ac27524fe6a893585f1edddfae4ecdb57dd8b131`:
compile0, original21=13PASS/8FAIL, review19=19 assertionFAIL on positive controls,
owned semantic11=11PASS. This authorized the implementation, not new acceptance.
The failures were explicit Err scaffold failures, not an executable reproduction
of the old unsafe decoder defects. See the independent report
`.agents/reports/2026-10-16-linux-hotkey-wire-review-red-2.md`.

Original tests/fixtures and semantic validation body remain unchanged. RED.md
and REVIEW_RED.md describe their historical checkpoints; this document describes
the current candidate. Original Stage2 abort's exact call/pointer condition is
still UNATTRIBUTED. Existing dependency source/XML/generator evidence archive
and its hashes remain byte-identical; no new remote hash identity is claimed.

## Implementation and safety boundaries

```
xkb_map::read (unchanged controls, full/all GetMap)
  -> from_reply(real live owned Reply; unchanged raw ownership boundary)
  -> decode_full -> parser::read(bytes, require_full=true)
  -> existing validate_owned + existing session queue/invalidation checks

structural tests -> decode -> parser::read(bytes, require_full=false)
```

Both entries use the same parser, header validation and component readers. The
only difference is requiring complete request coverage before variable payload
allocation. There are no partial rereads, alternate generated decoders or
independently timed map components.

- `cursor.rs`: single cumulative offset, checked offset addition and list-size
  multiplication, bounded `.get` slices and safe fixed-array byte copies. List
  extents are admitted before collecting owned values. Alignment consumes the
  exact padding byte count and does not require padding to be zero. Complete
  consumption is mandatory; trailing data is not silently ignored.
- `header.rs`: exact reply type, checked declared size vs actual slice, all eight
  supported component bits, valid global key bounds. Typed spans use u16 so
  exclusive endpoint256 remains valid and no keycode cast wraps. Every sparse
  range/count is validated independently. `require_full` enforces type range
  first0/n=total, symbol/action AND all sparse key ranges=min..max, and
  virtualMods=ffff. A covered range with zero records/actions is accepted;
  omitted or shortened ranges are not equivalent to zero action facts.
- `components.rs`: native-endian CARD16/CARD32 reads, distinct 8-byte type-entry
  and 4-byte preserve records, validated BOOLs, exact symbol/action totals, and
  each sparse component's own membership/duplicate checks. Action counts are
  admitted and summed before any action vectors are allocated; their sum must
  equal totalActions. The complete action byte extent is admitted once. Covered
  zero-count keys remain empty vectors, not fabricated Action::NoAction values.
- `parser.rs`: consumes all eight parts in protocol order. Virtual mask values
  are one CARD8 per set bit, not Rust ModMask-sized values; all16 bindings are
  resolved into a fixed array. It explicitly aligns count/virtual/explicit/mod
  lists. Behavior and action bytes stay numeric/opaque, never generated enums.
  Unknown actions survive intact for the UNCHANGED semantic dependency checks.

The parser cap is **1 MiB of input after wait_for_reply**. It does not bound
libxcb's preceding allocation/read/wait, nor prove ownership of arbitrary raw
pointers. Only the existing real owned-reply boundary accepts Reply::as_raw;
new parser modules are entirely safe Rust. No fake Reply objects in tests.

Work/storage is bounded by admitted bytes plus small fixed tables: key/type
range counts are CARD8 and bounded before iteration; per-record variable counts
are CARD8/CARD16; list extents are checked before allocation; byte cursor never
moves backwards. At most256 key-index slots per map component,16 virtual slots,
and BTreeMap sizes bounded by those ranges. This is not an exact heap-byte or
wall-clock scheduling guarantee, and does not claim that low-memory allocation
failure is recoverable. The pre-existing auth F4 migration blocker is unchanged.

No semantic weakening: dependent key/type/group/modifier agreement, selected
Escape action, unknown state-changing dependency, required modifier locking,
controls, subscriptions and map invalidation checks remain outside the decoder
in their existing production code. Parser structure does not authorize a grab.

## Exact CC commands — separate logs and return codes

From a complete coordinator-frozen Linux source tree with locked/offline deps:

```sh
cargo-1.89 test --locked --offline --lib --no-run
cargo-1.89 test --locked --offline --lib \
  'mcp::hotkey::x11::xkb_wire::tests::' -- --nocapture --test-threads=1
cargo-1.89 test --locked --offline --lib \
  'mcp::hotkey::x11::xkb_wire::review_tests::' -- --nocapture --test-threads=1
cargo-1.89 test --locked --offline --lib \
  'mcp::hotkey::x11::xkb_map::tests::' -- --nocapture --test-threads=1
```

Scoped gate is the SAME51 tests (21+19+11); none added/edited/removed. No result
is predicted as observed. Run each command independently, not an `&&` chain
that loses later results if one fails.

Old57 headless tests remain unchanged. Exact filters/counts:

```text
mcp::hotkey::x11_plan_tests::          20
mcp::hotkey::x11_default_map_tests::   10
mcp::hotkey::x11_delivery_tests::      10
mcp::hotkey::x11_shortcut_tests::       7
mcp::hotkey::x11_lifecycle_tests::      6
mcp::hotkey::x11_auth_tests::           4
```

For each filter independently:
`cargo-1.89 test --locked --offline --lib '<filter>' -- --nocapture --test-threads=1`

Only after coordinator schedules the existing three Unix socket tests:

```sh
cargo-1.89 test --locked --offline --lib \
  'mcp::hotkey::x11::transport_tests::' -- --ignored --nocapture --test-threads=1
```

This filter is restricted to three owned local transport cases, NOT a blanket
ignored/live run. Actual X qualification remains a separate later CC gate using
the unchanged canonical owned/authenticated Stage1/Stage2 runners/fixtures.
Passive grab/Armed or pure parser PASS is not real cancellation proof. No new
mutation fixture work is bundled here. Do not fold mutable Stage2 registration
into the coordinator's frozen baseline overlay or change any fixture guards.

## Compile-only evidence and exact edit scope

Linux `rustc --test --emit=metadata` type check completed exit0 with the existing
xcb/libc rmeta dependencies. No test binary was executed. Full build/link,
51/57/socket and live outcomes are for independent CC to report separately.

Changed (relative to review-red-2):
- `src/mcp/hotkey/x11/xkb_wire/mod.rs`: connect both decode entries to shared parser.
- `src/mcp/hotkey/x11/xkb_wire/SHA256SUMS`: candidate root-relative manifest.

New:
- `src/mcp/hotkey/x11/xkb_wire/cursor.rs`
- `src/mcp/hotkey/x11/xkb_wire/header.rs`
- `src/mcp/hotkey/x11/xkb_wire/components.rs`
- `src/mcp/hotkey/x11/xkb_wire/parser.rs`
- `src/mcp/hotkey/x11/xkb_wire/GREEN.md`

All other included overlay files are unchanged identity anchors. The17-file
root-relative overlay contains16 manifest members plus SHA256SUMS; it is not a
standalone Cargo snapshot. Apply only these paths to frozen review-red-2. No
rootlib/manifests/auth-helper/native source/Windows/macOS/runners edited.
