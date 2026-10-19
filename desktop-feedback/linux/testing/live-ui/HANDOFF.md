# Native UI fixture freeze — 2026-10-16

## Deliverable / authorization boundary

This is the executable fixture source checkpoint for independent CC review, unit/link and
then serial real private-X execution. **No tests, executable, X server, GUI, SSH, Docker,
package installation or delegation was run by this lane.** No commit/push/worktree.

Only `desktop-feedback/linux/testing/live-ui/**` was authored. The product66-file freeze
remains content-identical to the coordinator manifest, including original107 and new10 tests.
CC117 PASS + product actual Linux link + Nash's R1/R2 source review are prerequisite evidence
from the coordinator, not results produced by this fixture.

## Frozen inventory

`fixture-freeze.sha256` lists the exact repo-relative paths and content digests; it excludes
itself, build/cache directories and runtime outputs. Its own digest is handed off in the
conversation. Do not snapshot the entire working directory including observer/target or
cargo-home. Freeze contents:

- `.gitignore`, `README.md`, `HANDOFF.md`
- Python executable/orchestration: `run.py`, `owned.py`, `ipc.py`, `wire.py`, `case.py`,
  `cases_ui.py`, `cases_shutdown.py`, `rules.py`
- Python units: `tests/test_wire.py` (10), `test_rules.py` (4), `test_acceptance.py` (8),
  `test_resources.py` (5): **27 authored, unexecuted**
- Standalone observer: `observer/Cargo.toml`, `observer/Cargo.lock`,
  `observer/src/{main,observe,pixels,requests}.rs`; **5 authored, unexecuted** Rust units
- Evidence: `evidence/{observer-check,observer-locked-check,python-syntax-check,
  product-freeze-check,full-product-freeze-check}.log`

`run.py` has an executable bit and is also explicitly invokable with `python3` (content
hashes do not authenticate mode bits). Standalone observer must be linked first on Linux.
All32 fixture unit tests are additional; they must not be added to the frozen product117
count or reported as executed because their targets compile.

## Checks actually performed

- Rust1.98.1/macOS → Linux x86_64, `cargo check --locked --offline --all-targets`: exit0.
  Existing vendor bundle only; no changed product dependencies. Package-local observer lock.
  This is not Linux1.89 link or runtime evidence.
- Python3.14.0 AST/bytecode compile with3.10 grammar: exit0; no imports, tests or processes.
- Product local manifest49/49 and coordinator full freeze66/66 hash checks: exit0.

## Immediate independent sequence

1. Verify `fixture-freeze.sha256` from the repository snapshot root (`sha256sum -c ...`).
2. Use README commands for Python27, Rust5 and actual observer Linux1.89 build/link. Keep
   unit/link logs separately. No `DOCS_RS`, dependency rewriting or lock regeneration.
3. Review the fixture ownership/acceptance paths; report missing Xvfb/xauth/font/native
   packages as blockers. This lane did not probe or install them.
4. With independent execution authorization, run README's opt-in command in an isolated
   disposable container, own private auth/display, selected cases sequentially. Collect
   `result.json`, all selected cases, traces, screenshots, raw bytes and `artifacts.sha256`.
5. Inspect screenshots manually. Do not promote a subset PASS, a timeout, an inconclusive
   queue setup, a source invariant, or a link success to full UI acceptance.

Ten real cases are implemented: feedback, stop-once, stale-grant, unicode, clean-eof,
preclosed-eof, startup-eof, truncated-eof, backpressure, output-closed. Backpressure keeps
its real reader open/undrained until child exit; sentinel bytes remain in stdout.raw and
are accounted separately afterward. Closed-reader case cannot recover pipe contents and
explicitly declares that; no control-delivery assertion. Startup race evidence is sampled,
not deterministic internal-boundary proof. The full rationale/bounds are in README.

## Remaining blockers / nonclaims

Observer Linux1.89 link, all32 fixture units, real UI execution and screenshot review are
pending. Passing the private-X profile later still leaves real desktop WM/compositor,
mixed-DPI/font/grab/watchdog/scan-out profiles separate. The fixture performs no capture
provider work. **Genuine exclusion remains unresolved and mandatory, renderer capability
must remain unsupported, Host admission stays closed.**
