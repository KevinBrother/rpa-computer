# G1/G2/G3 limited supplemental RED — 2026-10-16

**Source authoring and compile only; no tests, SSH, GUI/input, server startup or
signals. No decoder GREEN.** Original frozen decoder-red-1 archive remains
`8507156a1f407577cd34d462dd5136917f980ae8f1b481deca523343d1007677` (358 files).
Coordinator reports CC84743 compile0, original21 actual13PASS/8FAIL, exit101.
Those eight failures are explicit Err-scaffold assertions, NOT old unsafe
behavior reproduced. Defensive passes do not prove parser correctness.

## Production seams, not a test implementation

```
xkb_map::read (same full=all, partial=empty, virtual_mods=all)
  -> from_reply(live owned GetMapReply)
  -> decode_full(bytes) -> decode(bytes)
  -> validate_owned(core, plan, controls.num_groups(), owned_map)
```

- `decode` stays the **same explicit Err**. It is the shared structural parser
  boundary for all eight components, including packed virtual subsets and
  internally consistent partial type/key ranges. It is not a new general
  partial-request consumer and still rejects missing/unknown component bits.
- `decode_full` is the actual production caller's full-request boundary, also
  explicitly unimplemented/Err after `decode`. Future GREEN must require full
  type coverage (first0/n=total), every requested key range spanning min..max,
  and virtualMods=ffff. Present=ff alone is insufficient. Zero-count actions
  within a covered range are distinct from missing action-key coverage.
- `validate_owned` is a direct extraction of the existing production validation
  body, not reimplementation in tests. Request/control/subscription/queue
  checks stay in the read/session path. Semantic tests do not claim those checks
  or actual X delivery occurred. No action filtering, range bypass, new query,
  panic catcher or dependency upgrade has been introduced.
- The 1 MiB cap is **post-wait_for_reply parser admission only**. libxcb has
  already allocated/read its reply. This cap does not limit prior libxcb malloc
  or waiting, and `reply_size` does not verify arbitrary allocation ownership.
  Only a live libxcb-owned `Reply::as_raw` enters the byte-slice boundary; tests
  use safe byte vectors/owned Maps, never fabricated Reply objects or UB.

## G1: independent production-shaped oracle

`review_fixture::production_full` is independent of the parameterized writer
and the original136-byte structural fixture. It describes the requested FULL
shape, not a capture/default-layout claim or proof of semantic eligibility:

| Range | Component |
|---|---|
| 0..40 | fixed header, length40 words after32; min8/max11/presentff |
| 40..72 | all4 types; levels1,2,2,2 |
| 72..120 | all4 key symbol rows; one symbol each |
| 120..124 | all4 action-key counts, each1; no padding |
| 124..156 | NoAction, Control/Shift/Alt SetMods (4 x8 bytes) |
| 156..160 | one nondefault behavior record |
| 160..176 | virtualMods=ffff; exactly16 CARD8 mappings |
| 176..180 | one explicit pair +2 pad |
| 180..188 | three nonzero modifier pairs +2 pad |
| 188..192 | one virtual modifier map (native-endian CARD16) |

The original136-byte fixture remains byte-identical, including historical
`full_reply` name/comment. That name means complete structural component
encoding, NOT server-normalized full coverage: packed virtual subset0x8011,
default behavior and zero modifier record are intentional structural data.
The supplemental `full_zero_sparse...` case also permits zero sparse records
and full-range zero action counts, without treating absent ranges as NoAction.

## G2: bounded layout and paired rejection groups (19 tests total with G1)

`review_fixture::variant` is a test-only XML-layout writer, not production code.
At most4 keys/4 types/2 sparse records per component. It supplies known offsets
for precise byte mutations, not a second decoder. Its rich type fields test
structure preservation, not claims of server-normalized semantic resolution.

Positive layouts cover action-count padding0..3 with all-one/mixed counts;
virtual populations0,1,2,3,4,15,16; empty/odd/even explicit and modifier lists;
two different behavior/vmod sparse records; every type's distinct nonzero
CARD16 type/entry/preserve masks; separate action high/low bytes; and key255
with legal exclusive endpoint256. Native-endian multi-byte fields use byte
copies, never alignment casts. Deliberately nonzero padding is not data.

Each rejection group first decodes an adjacent legal control: own sparse range
vs global range, duplicate keys in each sparse component, symbol/action totals,
type first+n exceeding total without integer overflow,
active BOOL in types0..3, excessive internal counts/invalid preserve BOOL,
truncation with recomputed outer length, range overflow, and sparse total>range.
Full-policy pairs keep presentff and coherent structural payloads but shorten
symbol/type/action coverage or virtual bits. Action shortening includes all
zero counts, including an empty action range, not just missing nonzero actions.

## G3: actual shared semantic body (11 tests)

`xkb_map::tests` calls `validate_owned` with a small Map and the **real planner**.
Covers numeric SetMods1, Caps LockMods3, rejection of locking required modifiers,
USE_MOD_MAP_MODS bit2 versus raw realMods; nonzero high and low virtual-action
bits with swapped-byte negative; Escape NoAction0 vs SwitchScreen13/unknown;
all previous state-changing tags plus unknown independent action keys entering
unwanted/dependency sets; actual `decide` replay with a held independent action;
actual `invalidates` Action/Type positive and unrelated-range negative cases;
modifier-map mismatch, nondefault dependent behavior, virtual type/entry
resolution and unknown modifier action rejection. No copied translator or
mock invalidation function. The same owned-map body runs in `read`.

## Exact independent commands (headless only)

From a new immutable Linux snapshot using the existing locked/offline deps:

```sh
cargo-1.89 test --locked --offline --lib --no-run
# Original21: retained assertions and evidence identity, no edits to tests.rs.
cargo-1.89 test --locked --offline --lib \
  'mcp::hotkey::x11::xkb_wire::tests::' -- --nocapture --test-threads=1
# Supplemental G1/G2: 19 tests. These all have positive decoding controls.
cargo-1.89 test --locked --offline --lib \
  'mcp::hotkey::x11::xkb_wire::review_tests::' -- --nocapture --test-threads=1
# G3: 11 tests on the extracted, already-existing production semantic body.
cargo-1.89 test --locked --offline --lib \
  'mcp::hotkey::x11::xkb_map::tests::' -- --nocapture --test-threads=1
```

Record each exit/status/assertion independently. **Expected, not observed**:
original21 keep13PASS/8FAIL; G1/G2 all19 fail at positive controls because decode
is Err; G3 may pass existing semantics (source expectation11PASS), not evidence
of a working wire parser. Total scoped tests51, of which30 newly authored.
Do not call this a 30-failure gate or a reproduction of the former UB. Independent
CC must report actual counts; compilation alone cannot satisfy the gate.

No ignored/live test invocation, no all-tests run, no test-time generated decoder.
Old57 tests and all frozen real registration/Stage1/Stage2 fixtures remain
unchanged and required for later acceptance. No expansion or execution of the
later mutation fixture. Stage2's old precise abort cause remains UNATTRIBUTED.

## Compile and freeze scope

Developer Linux `rustc --test --emit=metadata` compile completed exit0; no binary
was linked/run. This is type checking only, not a test/link/real-X result.

Files changed BY THIS CHECKPOINT relative to decoder-red-1:
- `src/mcp/hotkey/x11/xkb_map.rs`: extract same validation body; register new tests.
- `src/mcp/hotkey/x11/xkb_wire/mod.rs`: full-policy Err boundary/test registrations;
  post-wait cap comment.
- `src/mcp/hotkey/x11/xkb_wire/RED.md`: accurate original21 evidence/fixture/cap scope.
- `src/mcp/hotkey/x11/xkb_wire/SHA256SUMS`: refreshed root-relative checkpoint hashes.

New:
- `src/mcp/hotkey/x11/xkb_map_tests.rs`
- `src/mcp/hotkey/x11/xkb_wire/review_fixture.rs`
- `src/mcp/hotkey/x11/xkb_wire/review_tests.rs`
- `src/mcp/hotkey/x11/xkb_wire/REVIEW_RED.md`

No root/Cargo/helper/native/platform/runners/other lane changes. Original
`tests.rs`, model, generated-source evidence archive and x11 module registration
remain unchanged. No fresh generated-source/XML hashes replace the evidence
preserved in RED.md; no remote generated hash equality is inferred.


Static inventory caveat: the current lane already has the Stage2
`stopped_qualification` registration appended in `x11_registration_tests.rs`
(current hash `85a8b644d8419a4890182e83c17ab568e430ef67b2cbcf690683fbc6742860b3`),
whereas the original decoder-red-1 tree's copy lacks that registration
(`e752ba8301a22e85c980cc99f227cfb69ad0a554261214184eb70ee05c762285`).
This checkpoint did not edit it. Do not reset/remove it when packaging, and do
not attribute this pre-existing delta to G1/G2/G3. The supplemental archive is
only a 12-file root-relative overlay (11 manifest members + SHA256SUMS), not a
standalone Cargo tree; coordinator owns the complete independent CC snapshot.
