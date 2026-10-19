# hotkey-nkn-revalidation-red-1 — production-connected, not a live build

2026-10-16. Coordinator approved **RED only**, after the observed main-pump
NewKeyboardNotify. No test/SSH/GUI/input/server operation was run by developer.
No dependency/runner/keymap/profile edits. No GREEN or default-input success claim.

## Exact gate and explicit scaffold

New tests: **26** `revalidation::tests`, **8** `xkb_aux_wire::tests` = **34**.

- `capture_initial`, `revalidate`, `scratch_notice`, `decode_controls`, and
  `decode_compatibility` deliberately return explicit Err. They are NOT the old
  real failure reproduction. New defensive cases have positive controls before
  rejection assertions; reject-all is not evidence of safety.
- Source expectation ONLY: revalidation25 assertion failures,1 fixed-event adapter
  pass; auxiliary8 positive-decoding assertion failures. Independent CC must
  report actual counts/nonzero execution. No observed RED/actual PASS claimed.
- Initial serve is deliberately connected to `capture_initial` **before grabs**.
  Consequently this scaffold cannot arm even a healthy real server. Do not run
  live qualification or ship this checkpoint. It is a test-first development tree.
- Production main-pump real NKN calls `revalidate`, not an ignore predicate;
  already-permitted scratch notices call its restricted update path. Defaults
  in `xkb_events::changed`/`validate_queue` and old invalidation assertions remain
  unchanged. Init/candidate/nested notifications have no exemption.
- `LiveSource` is the actual same-connection core/Controls/full8-map/Compat reader
  and read-only GetInputFocus fence adapter. `Source` provides actual owned images,
  raw events and errors, not a trusted equivalent/stable/safe bool. Its methods
  are unreachable until the explicit orchestration scaffold is implemented.
- `SemanticImage::validate` invokes the existing real planner and extracted
  `validate_owned`; controls rejection uses the same shared policy as old read().
  Core/XKB/device identity and all config bytes are retained. No dynamic Action
  getter or generated GetMap decoder, no catch_unwind fix, no dependency upgrade.

## Tests/contract

The 26 protocol tests cover two full reads then one fence (no retries); initial
capture; unchanged opaque unknown dependency; FIFO press/release/other including
existing pending; shared absolute deadline; all8 map components, core/identity,
Controls all81 configuration bytes and all Compat groups/SI; unknown/synthetic
NKN; A/B mismatch and ABA notifications (including legacy); queue errors and one
256-event aggregate capacity; stop/failure/expiry/generation before commit;
unsafe selected/modifier action/type on initial capture; bounded scratch-only
baseline update and later equivalent NKN, forbidden controls/compat/out-of-range
or new unknown dependency; actual fixed-event metadata and scratch range adapters.

The 8 aux tests use independent safe byte oracles from pinned XML: Controls92
bytes; Compat header32 + SI16 + four ModDef4; zero SI full group coverage;
sequence/padding excluded but every actual field retained; unknown action bytes,
nonzero virtual modifiers, signed curve, repeat bytes; partial coverage/truncated/
wrong declared length/huge declared length; unaligned safe slices. No fabricated
Reply pointer or generated dynamic decoder is used in tests.

Normalization representation: `Controls.fields` contains exactly 81 config bytes
(offsets8..15,16..46,48..92), with concrete device separate. Compatibility owns all
SI16-byte records and all four group4-byte records. Native wire order is retained;
comparison is on the same connection/platform, never cross-host hash equality.
The auxiliary owned-reply wrapper follows the existing live-libxcb allocation
boundary with checked length/cap before byte slice creation. **1MiB is post-wait
only**, not a claim bounding preceding libxcb allocation or wait. Existing
registration-owned socket watchdog is not replaced or refreshed by proof steps.

`capture_initial` will retain the original2s init deadline. Main-pump creates one
200ms deadline before starting the existing min-deadline socket lease, then passes
that same instant to all suboperations. No regrab, new ready/active publication,
prewarm, server grab, input or credential change is introduced. The successful
proof path is not implemented; lifecycle linearization remains a GREEN review gate.

Scratch extraction is a separate explicit Err scaffold. The current API models
one component/range; multi-component notifications must not be guessed from one
range or wholesale accepted. Coherent companion notifications remain conservative
failures; no real scratch continuity success is claimed by these fixtures.

## NKN/legacy ordering check BEFORE coding

Read cached primary source, **not identified as the deployed server binary**:
Xorg revision `fc625fe172d9f6a149a594b5214364bedf680239`,
`xkb/xkbEvents.c` SHA256
`f5f9f662495fe728706d50ab4ee9490f1c748bb94edea4ce162e8a5576f624ca`.

`XkbSendNewKeyboardNotify` emits NKN then calls `XkbSendLegacyMapNotify`.
The latter explicitly suppresses core legacy events emulated from NKN for clients
with `_XkbClientInitialized`; its separately selected XI notifications have a
separate delivery path. This client negotiates XKB and does not subscribe to XI.
This is source mechanism only, NOT remote observation or proof of absence of
companion core/Map/Controls/Compat notices. Production capture classifies any
received real such event as invalidating, even identical A/B images. No suppression
by request_minor, matching ranges/device, event sequence, or input-source inference.
Default actual complete baseline/pivot snapshots are still UNKNOWN.

Pinned xcb1.7.1 `xml/xkb.xml` SHA256
`f23ac2d1693701203cac6537ec41e8928d9cd20bd4013c02d195d7cdd69f0d99`;
GetControls opcode6, GetCompatMap opcode10, SymInterpret/SIAction/ModDef used.
The fixed NKN getter retains raw CARD16 changed bits; no Action enum is involved.

## Independent CC commands (NOT executed by developer)

In fresh frozen Linux source with the existing locked/offline deps and GUI env
unset; do not select ignored/all tests:

```sh
cargo-1.89 test --locked --offline --lib --no-run
cargo-1.89 test --locked --offline --lib \
  'mcp::hotkey::x11::revalidation::tests::' -- --nocapture --test-threads=1
cargo-1.89 test --locked --offline --lib \
  'mcp::hotkey::x11::xkb_aux_wire::tests::' -- --nocapture --test-threads=1
```

Run filters separately, capture exit/assertions and reject zero-case results.
The original51 (21+19+11), old57 and3 socket fixtures, all existing real/canonical
assertions remain required acceptance gates and byte-identical in this authoring
checkpoint. They were not rerun. Stage1 old default-input failure and diagnostic
NKN evidence remain separate; Stage2 old precise abort callsite remains
unattributed. No later mutation fixture was authored/run.

## Compile-only and overlay scope

Developer ran only Linux `rustc --test --emit=metadata`, exit0. It compiled all
registered hotkey test sources, did not link or run a test binary. Expected dead
code warnings reflect explicit scaffold. Exact command/log archived separately.

Edited4 prior files:
- `src/mcp/hotkey/x11.rs`: register modules; initial and main-pump seam wiring.
- `src/mcp/hotkey/x11/xkb_map.rs`: extract same full-map read and controls check;
  expose actual owned validator within x11; semantic body unchanged.
- `src/mcp/hotkey/x11/xkb_wire/model.rs`: Clone derives only.
- `src/mcp/hotkey/x11_plan.rs`: KeyboardMap PartialEq/Eq derives only.

New7 Rust files:
- `src/mcp/hotkey/x11/revalidation/{mod,live,fixture,tests}.rs`
- `src/mcp/hotkey/x11/semantic_image.rs`
- `src/mcp/hotkey/x11/xkb_aux_wire/{mod,tests}.rs`

Plus this README and its new sibling SHA256SUMS. Do not replace historical wire
SHA256SUMS or reset Stage2 registration. Overlay only, not mutable whole-tree
packaging; coordinator owns the complete frozen tree. No existing source outside
these4 paths changed relative to the pre-authoring hotkey inventory.
