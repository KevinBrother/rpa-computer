# XKB owned-reply byte decoder — frozen RED checkpoint

2026-10-16. **Authoring + compile only. No tests/SSH/GUI/input. No GREEN claim.**
Later action/type mutation work is paused at its immutable DESIGN checkpoint.

## Observed failure versus source-proven defects

Authoritative raw logs (do not edit):

```
.agents/runs/linux-parity-20261015/cc-hotkey-qualified-runners-1/remote-logs/stage1/test.log
.agents/runs/linux-parity-20261015/cc-hotkey-qualified-runners-1/remote-logs/stage2/test.log
```

Stage1 records `computer-hotkey-x11` panic at generated `xkb.rs:7621`, unexpected
`xkb::Action` type; product reports setup failure. Zero of 116 cases ran. Stage2
records a non-unwinding `ptr::copy_nonoverlapping` precondition failure in the
same worker, then abort; no healthy-arm/stop phase. **There is no backtrace or
raw reply in either log. Stage2's exact call site/pointer condition is therefore
UNATTRIBUTED.** Do not convert the report's proposed stack explanation into an
observed trace. Infrastructure unit/auth/identity gates passed independently;
these are product-visible failures, not runner refusals.

Source-proven defects in the pinned dependency used by the production read path:

1. Generated `GetMapReply::map()` (local generated lines 14317–14334) starts the
   variable payload at byte 40, calls `Vec<GetMapReplyMap>::unserialize` once.
2. That decoder (lines 8716 onward) starts **each enabled bitcase** with local
   `offset=0` against the same `wire_data` pointer. `*out_offset += offset` does
   not advance the next bitcase's input pointer. Full-map symbols/actions and
   later components are therefore read at the wrong address. Dynamic lengths
   then come from unrelated bytes without bounded slices.
3. The generated virtual-mod list (around 8795–8811) casts CARD8 wire data to
   `*const xproto::ModMask`, then copies/counts `size_of::<ModMask>()` per item
   (u32 representation). XML specifies ONE BYTE per set virtual-mod bit, plus
   alignment padding. This can consume unrelated/out-of-bounds bytes even in a
   separate virtual-modifier-only request.
4. Generated `Action::from_data` matches only tags 0..20 and calls unreachable
   for any other byte (line 7621). An unknown/private tag is not safely
   representable there. Raw logs do not identify the actual tag or establish
   whether the Stage1 tag came from a real action or the wrong cursor.

`build/cg/switch.rs` emits the reset cursor (line 429), accumulates only the
output offset (around 696), and has the primitive list path that uses Rust
representation sizes. Source inspection proves these defects independently of
attributing the precise Stage2 abort. No extra GUI/backtrace is required before
replacing this demonstrated unsafe path.

## Preserved dependency evidence (not compiled or executed)

`xcb-1.7.1-decoder-evidence.tar.gz` contains the exact LOCAL generated files,
selected generator/XML files, and the original complete crates.io `.crate`
archive (including the rest of the generator/source/license). Internal
`SHA256SUMS` verifies every member. It is evidence, not a vendored replacement.
The complete generated file is preserved verbatim rather than reformatted or
trimmed; its size is justified by provenance, not production source modularity.

| Member | SHA256 |
|---|---|
| generated/xkb.rs | `8cfcce5a55b57c7f516f55a49d7862fffaecb867686bd9020f15159a7e19ba17` |
| generated/xproto.rs | `407f0f7986e59939a4f07db3ec8c6711390e6ec94df11f038508b864dbbf8d8f` |
| generator/switch.rs | `264f979371a49495499cad97d39cfd4f8d1ee594156d678333af0ab08c14681f` |
| generator/union.rs | `69152ae177b031e286be8fbb42b6d513aad721ef70018d1be2f0a641343582ca` |
| protocol/xkb.xml | `f23ac2d1693701203cac6537ec41e8928d9cd20bd4013c02d195d7cdd69f0d99` |
| protocol/xproto.xml | `45a0e10ef7b01346797a352c2c8eec9a90307fb6693f50802072ae2693680802` |
| source/xcb-1.7.1.crate | `a6c2ad15e0e922856ee89afe862b8992334bbe7953adad56cd1199358cb30566` |

Evidence archive SHA256:
`6f09a3f248b6754ecf161926f3174db1d16a09fd4c9a079c705d5893ef87a19d`.
The `.crate` checksum matches Cargo.lock's pinned xcb 1.7.1 checksum. Local output
path is `target/x86_64-unknown-linux-gnu/debug/build/xcb-c8a612745f94e103/out/`;
CC logs name `target/debug/build/xcb-b7329a2f86fd1922/out/`. No remote generated
file was fetched in this turn: **do not claim the local generated-file hash was
independently matched to the remote artifact**. Line 7621 and package version
match; preserved local evidence plus the immutable package generator establish
the source defect, not a remote artifact hash equality claim.

## RED implementation boundary / invariants

Actual production chain:

```
x11::serve (subscribed before reads; existing queue consistency unchanged)
  -> xkb_map::read (same full=all, virtual_mods=all GetMap request)
  -> xkb_wire::from_reply(&owned_GetMapReply)
  -> xkb_wire::decode_full(&[u8]) -> decode(&[u8])
  -> owned model -> xkb_map::validate_owned (existing dependency/delivery logic)
```

`from_reply` uses public `Reply::as_raw` on a live libxcb-owned reply. It reads
only the fixed header first. Checked `32 + length_words*4` must be >=40 and
<=1 MiB before constructing the payload byte slice, which is borrowed for no
longer than the owning reply. This relies on libxcb's owned allocation contract;
it is NOT a safe wrapper for arbitrary raw pointers. Pure tests never fabricate
Reply allocations, transmute pointers, run the generated decoder, or invoke UB.
**The 1 MiB cap is applied AFTER `wait_for_reply` returns an owned reply. It does
not bound libxcb's preceding malloc/read/wait.** This checkpoint does not rewrite
the transport or claim that parser tests prove those separate runtime bounds.

`decode` is intentionally an explicit error skeleton pending independent RED.
Thus this tree fails closed at setup and is **not a releasable GREEN build**.
It does not attempt/fallback to the old decoder. Only header-length arithmetic
and the real-reply boundary are implemented now; the byte parser is not.

The eventual decoder must:
- Consume one complete full GetMap reply, not independently timed partial queries.
  Retain subscriptions-before-read and existing bounded queue consistency checks.
- Require all requested parts; parse the 40-byte fixed header, then cumulative
  component offsets, counts, totals, ranges and padding with checked slice reads.
- Use native-endian `from_ne_bytes` for CARD16/CARD32 supplied by this XCB client's
  wire byte order; no typed pointer casts, Rust-layout copying or invalid enums.
- Parse type entries (8-byte) and preserve entries (4-byte) separately; align
  action counts, virtual CARD8 list, explicit map and modifier map explicitly.
- Validate lengths before allocating, reject duplicates/index wrapping/unknown
  component bits and trailing data; the total input cap bounds all work/storage.
- Preserve all eight bytes of every action, including unknown/private tags.
  Decoder is structural, not authorization. Modifier/selected Escape actions are
  still checked by dependency semantics; unknown is never converted to NoAction.
  Unresolved independent action kinds conservatively join held-key rejection and
  invalidation dependency tracking rather than being assumed inert.
- Keep core/XKB modifier agreement, type resolution, behavior and selected action
  checks; request no fewer components and do not disable XKB invalidation.

No dependency upgrade, Cargo edit, native-input edit, Docker/keymap change,
catch_unwind workaround, new fallback or narrowing of accepted test assertions.

## Original RED cases (21; independently executed by CC84743)

`mcp::hotkey::x11::xkb_wire::tests::` contains:

1. Multi-component reply gives exact symbols/actions and tail components.
2. Packed CARD8 virtual modifiers consume one byte each, including bit15.
3. Unknown/private action preserves all eight bytes without panic.
4. All 256 action tag bytes remain data, not enum discriminants.
5. Deliberately offset byte slice decodes without typed alignment assumptions.
6. Type map/preserve entries use distinct strides and advance next sections.
7. Zero-count action vector remains absent, not fabricated NoAction.
8. Nonzero padding does not become component data.
9. Every truncated prefix is rejected.
10. Truncated tail with a consistent outer length is rejected.
11. Huge declared length is rejected before payload access/allocation.
12. Fixed-header size/cap arithmetic is checked.
13. Error/event responses are not maps.
14. Full request cannot silently lose components.
15. Unknown component mask bit is rejected.
16. Symbol totals and action count sums must match.
17. Key/type range overflow cannot wrap indexes.
18. Duplicate sparse key entries are rejected.
19. Sparse keys outside the declared range are rejected.
20. Invalid boolean/oversized internal counts are rejected.
21. Undeclared or unconsumed trailing bytes are rejected.

The original byte fixture is a 136-byte STRUCTURAL reply with all eight components,
four canonical-width type records, two key-symbol rows, actual action slots,
behavior, three packed virtual masks, explicit/modifier/vmod-map data. It is a
synthetic protocol fixture, **not a claim to be captured Xvfb bytes or a default
keyboard layout**. Its historical `full_reply` name/comment does NOT mean it is
a server-normalized response to production full=all: virtualMods=0x8011 is a
packed subset, and its default/zero sparse records are deliberate structural
inputs. The original test file remains byte-for-byte unchanged. G1 adds a
separate full-request oracle; see REVIEW_RED.md. No X server state is changed.

Coordinator-confirmed independent CC84743 evidence: compile exit0; original21
**13 PASS / 8 FAIL**, exit101. All8 positive failures hit the deliberate Err
scaffold. Defensive PASS does not establish parser correctness; this is not an
executable reproduction of the old unsafe defect. Evidence report:
`.agents/reports/2026-10-16-linux-hotkey-wire-decoder-red-1.md`. Existing 57 pure cases, registration tests, socket cases and
both live acceptance fixtures remain present and unweakened. Do not run live
fixtures against the deliberately incomplete decoder skeleton.

## Exact independent CC commands

From a frozen Linux source snapshot with the existing locked/offline dependencies:

```sh
# Compile only first; no ignored/GUI/input tests.
cargo-1.89 test --locked --offline --lib --no-run
# Run ONLY this bounded pure decoder regression family:
cargo-1.89 test --locked --offline --lib \
  'mcp::hotkey::x11::xkb_wire::tests::' -- --nocapture --test-threads=1
```

Preserve actual output/counts/exit code. Do not execute generated evidence sources,
rerun crashed real fixtures, blanket --ignored, or change map/dependencies to
produce RED. GREEN authoring waits for the coordinator's observed RED gate.

Developer compile evidence (not tests): Linux `rustc --test --emit=metadata`
with existing xcb/libc `.rmeta` dependencies completed exit 0 after wiring the
production seam and 21 assertions. No link/runtime/real-X success is implied.

## Exact file scope

Changed:
- `src/mcp/hotkey/x11.rs` — registers xkb_wire only.
- `src/mcp/hotkey/x11/xkb_map.rs` — uses production byte-decoder seam and owned fields.

New under `src/mcp/hotkey/x11/xkb_wire/`:
- `mod.rs` — live-owned reply boundary, size cap, RED decode skeleton.
- `model.rs` — owned model, opaque eight-byte actions.
- `tests.rs` — 21 pure assertions.
- `RED.md` — this report.
- `xcb-1.7.1-decoder-evidence.tar.gz` — immutable dependency evidence.
- `SHA256SUMS` — root-relative freeze manifest excluding itself.

No registration-file change was needed. Stage1/Stage2 fixtures and runners,
macOS/Windows, Cargo, rootlib, credentials and native-input files are untouched.


## Limited review extension

See `REVIEW_RED.md` for G1/G2/G3, production full-policy separation, the shared
owned-map semantic entry, additional test counts/commands and freeze scope.
The original immutable 358-file snapshot and CC84743 evidence are not changed.
Decoder GREEN remains blocked on independent execution of the extended gate.
