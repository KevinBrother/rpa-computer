# F1/F2 bounded fixture repair — compile-only GREEN candidate freeze

2026-10-16. This is a source checkpoint, **not independently observed GREEN**, live UI,
worker receipt, scan-out, genuine exclusion, or Linux parity acceptance.
No tests/GUI/Xvfb/SSH/Docker/install/product execution/delegation were run by this lane.
Only `desktop-feedback/linux/testing/live-ui/**` changed. No product/dependency/Host/provider
changes. Capture exclusion remains unsupported; Host admission remains closed.

## Authoritative precondition / preserved assertions

Coordinator authorized GREEN after actual Linux raw output in
`.agents/runs/linux-parity-20261015/cc-ui-fixture-review-red-1/remote-logs/review-red-run.log`:
13 tests, **10 expected assertion failures +3 controls, zero errors**, Python3.12.3.
This lane read that original log. Unscheduled local runs mentioned by the coordinator
(one import error and a local13-test RED) are not used as Linux evidence.

Earlier baseline93 independently passed Python27 + Rust5 and actual observer/product
Linux Rust1.89 link per `2026-10-16-linux-native-ui-fixture-unit-1.md`; no GUI was run.
Those results do not establish this changed snapshot's45 tests or observer link.

`unchanged-audit.log` confirms:
- Frozen product66/66 unchanged, including original107+10 product tests and all locks.
- Entire additive RED7-file manifest unchanged, including all13 assertions and support.
- Original fixture tests, Rust request tests, package manifests/locks unchanged.
No tests were weakened, renamed, deleted or newly added for GREEN.

## Exact implementation delta

Seven existing fixture files changed relative to baseline93:

1. `ipc.py`: capture `spawn_before` immediately before `owner.spawn`/Popen, and
   `exit_observed_at` immediately after `proc.poll()` observes termination, before
   draining/logging. Persist the witnesses in normal trace records. Require native observer
   handshake `mapping_trace:"map-unmap-destroy-v1"`; an old observer is INCONCLUSIVE.
2. `cases_shutdown.py`: after existing actual blocked-reader exit, raw filler accounting,
   frame validation and window destruction checks, call the running-age exclusion gate.
   Store finite witness values and whole-process interval on successful exclusion.
3. `case.py`: `ring_clickthrough` arms event cursor BEFORE pre-image, requires an actual
   painted/viewable ring with XID, then invokes the existing real target-hit clickthrough.
   Capture post-image and attributes; issue a final native fence/event-drain; inspect the
   complete interval against the same ring XID. No cursor/event-list reset during injection.
4. `cases_ui.py`: both center/stroke clicks invoke the new ring path. Status-only
   clickthrough remains unchanged. Existing650ms budget is not used as continuity proof.
5. `observer/src/observe.rs`: retain native UnmapNotify alongside Map/Destroy, advertise
   the trace schema, and explicitly get the X server focus reply BEFORE draining events
   for the `events` command (also normal move/press/release responses). This ordering no
   longer relies on JSON macro field evaluation. XTEST remains normal server hit-testing.
6. `observer/src/pixels.rs`: include discovered exact window XID in image facts, including
   unmapped-window facts. Root GetImage is still observation of visible UI, never clean
   frame reconstruction or capture-time hiding.
7. `README.md`: current-amendment banner, with historical baseline evidence clearly marked.

One new runtime helper: `proof.py` (stdlib only). Package manifests and locks unchanged.
Other additions are this handoff, compile/syntax/hash-audit logs and the new freeze manifest.
Historical `fixture-freeze.sha256`, `review_red/review-red.sha256`, old HANDOFF and evidence
remain untouched; the first manifest intentionally describes the old snapshot, not the
changed current files. Use the new manifest below for current-source verification.

## F1 criterion and feasibility

Require finite ordered `spawn_before <= eof_at <= exit_observed_at` and strictly
`exit_observed_at - spawn_before < 1.0`. Missing/nonfinite/unordered witnesses, equality,
or slower observation produce **INCONCLUSIVE**, never R1-attributed PASS. Existing
failure65, actual EAGAIN/filled pipe, disabled Stop, reader-not-drained-before-exit and
byte accounting requirements remain intact.

No frame can predate creation of its renderer process, so this conservative full-process
bound excludes the worker's independent >=1s oldest-queue branch, even for a heartbeat
older than Stop. A parent delayed past1s loses qualification even if the child exited
sooner. Bound origin is never reset at Ready, grant, click, EOF or after a slow Popen.

Feasibility remains conditional/unmeasured: startup+paint+current grant+input+200ms final
drain+exit observation must all fit under1s. A slow machine returns INCONCLUSIVE; neither
renderer behavior, queue timeout, clock nor compositor is changed to force success.
The bound excludes this competing gate; it does not trace the worker's precise branch or
rule out every unrelated native failure. `output-closed` remains the narrower actual
no-reader/failure65 case and does not claim a traced EPIPE or recovered pipe contents.

## F2 criterion and evidence limits

Pre/post images must be painted and viewable, same exact XID/geometry; pre/post attributes
must retain that role/XID and map_state2. The trace interval begins before the pre-image
and ends after post-image/attributes and the final server-reply-before-drain fence.
Any same-XID Map/Unmap/Destroy, including unmap→remap, or replacement/missing witnesses
returns INCONCLUSIVE. Unrelated-window transitions are not grounds for rejecting ring
coverage. Focus theft and wrong target-hit coordinates keep their existing failure rules.

Native handshake prevents silently using the older observer that omitted Unmap events.
Actual event/image replies are retained in trace.jsonl, including named before/after PPMs.
This proves a conservative mapping interval plus bracketed pixels and real target events
when independently executed; it is NOT uninterrupted raster-content/physical scan-out
proof or all real-desktop compositor behavior. Native trace wiring still needs source
review and real execution; the13 scripted tests alone do not establish it.

## Checks actually performed / pending independent gates

- `observer-check.log`: local Rust1.98.1/macOS, Linux x86_64 target,
  `cargo check --locked --offline --all-targets`, exit0. Includes changed native code and
  test-target typechecking. No `DOCS_RS`. Not actual Linux1.89 linking or execution.
- `syntax-check.log`: Python3.14 AST/bytecode compilation with3.10 grammar, exit0;
  no imports/tests.40 Python test methods remain authored (27 original +13 review).
- `unchanged-audit.log`: read-only hash checks described above, exit0.

**Current45 units (27+13 Python,5 Rust), actual changed-observer Linux1.89 link, independent
F1/F2 source review and every GUI case remain pending.** No extra test/feature expansion.

## Independent commands — not run here

The new `fixture-green.sha256` enumerates the complete current fixture source/docs/evidence
set and historical manifests, excluding itself and build/cache/pycache artifacts. Verify
from repository snapshot root. Its own SHA is supplied in the conversation. The companion
`fixture-green.tar.gz` contains the40 listed fixture files plus that manifest (41 total),
with repo-relative paths; it contains no product files, caches or the archive itself. Combine only
with the unchanged product66; do not replace frozen archives or historical manifests.

```sh
L="$PWD/desktop-feedback/linux/testing/live-ui"
sha256sum -c "$L/review_green/fixture-green.sha256"
PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$L" \
  python3 -m unittest discover -s "$L/tests" -v
PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$L:$L/review_red" \
  python3 -m unittest discover -s "$L/review_red" -p 'test_*_red.py' -v

CARGO=/path/to/verified/cargo-1.89
VENDOR=/absolute/existing/vendor
export CARGO_HOME="$L/cargo-home" CARGO_TARGET_DIR="$L/observer/target"
"$CARGO" --config 'source.crates-io.replace-with="fixture-vendor"' \
  --config "source.fixture-vendor.directory=\"$VENDOR\"" \
  test --locked --offline --manifest-path "$L/observer/Cargo.toml" \
  --bin rpa-feedback-live-observer
"$CARGO" --config 'source.crates-io.replace-with="fixture-vendor"' \
  --config "source.fixture-vendor.directory=\"$VENDOR\"" \
  build --locked --offline --manifest-path "$L/observer/Cargo.toml" \
  --bin rpa-feedback-live-observer
```

Keep27/13/5 results and link logs distinct; no retry/source changes to force PASS. Review
first, then independent CC execution. **These commands do not authorize GUI.** Any later
live run must still be separately authorized/serialized with owned private auth Xvfb,
matching rebuilt observer, exact binary/source/image hashes and unchanged product renderer.
Dependency gaps are blockers, not installation permission. Real clean-frame/provider
qualification remains unresolved and mandatory; no Host admission is opened.
