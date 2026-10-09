# Windows Multidisplay Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. User role split overrides defaults: code/test-source implementation is Codex gpt-6.1-sol; actual test execution is CC + GLM. No worktree, commit or push.

**Goal:** Connect the validated display-topology/windows-display libraries to real Windows Host observation, selection, coordinate mapping and input cancellation, with optional desktop feedback correctly handling the selected desktop. This is part of the already approved complete-computer-actions scope, not an alternative definition of completion.

**Architecture:** Runtime owns session selection and complete topology generation. Backend owns persistent OS enumeration/capture authority on its worker thread. Model coordinates map through the exact observation regions; only internal drag interpolation may traverse display gaps. Feedback remains optional and consumes confirmed native facts, never becomes a native-input dependency.

**Tech Stack:** Rust, existing windows-rs 0.58, rpa-display-topology/rpa-windows-display, existing MCP/TLS transport, Windows C#5 optional renderer. Windows-only test execution.

## Evidence and operational boundary

- Existing pure topology22 and Windows-display31 tests passed on Windows via CC+GLM; neither proves native GDI/root integration.
- Root baseline lib244/0/6 and fake-renderer process5/5 are frozen artifacts; never overwrite existing deployments or first failures.
- Windows GUI is paused because a manually controlled network authorization dialog covers the target. No new GUI/network Host runs until that gate is cleared. No Mac/Linux tests.
- B2 fixture owner has independent frozen scope acceptance-fixture/** and basic analyzer/tests; do not modify those in root/renderer tasks.

## Locked public choices for this integration

1. Retain 8 computer tools. Add optional `computer_open.display` discriminated object: `{"kind":"primary"}` (default), `{"kind":"id","id":"opaque-runtime-display-id"}`, or `{"kind":"desktop"}`. Reject extra/invalid fields and missing explicit IDs; no silent primary fallback.
2. `computer_describe` reports actual display descriptors and full generation + native coordinate unit when available, plus supported selections; enumeration failure is unavailable/structured error, not invented topology. Do not expose screens from a cache as current after query failure.
3. Observation metadata must describe actual PNG size, full topology generation, selected displays and mapping regions. A session retains full authoritative mapping, not a bounding-box width ratio. Generation includes tracker identity and revision; identical revision from another tracker is not equal.
4. Host feedback v1 wire is unchanged. For full Desktop only, Surface.id is exactly `desktop`, coordinates are the bounding box of ALL active Windows physical-pixel screens, and version remains opaque full geometry/generation token. Single display uses its existing opaque ID and exact bounds. Renderer must explicitly branch on this reserved ID; never infer Desktop solely because a rectangle is large or happens to contain two screens. Stop stays on a real selected monitor; a gap is not a valid stop placement or pointer-ring target.
5. Keep existing max_width/max_height semantics, implement real resizing not just metadata. Budget source/canvas/PNG memory plus base64/JSON/MCP framing. Windows-display stored-DEFLATE estimator is only a preflight estimate, not proof of message size. Actual serialized reply must fit existing output cap; do not increase cap to hide a regression.
6. Other platform primary-only paths may remain explicit; unsupported selectors return honest unsupported errors. No fake multiscreen on Mac/Linux. Preserve compilability through cfg; tests there remain deferred.

## Task 1 — Baseline red contract tests (root owner)

**Files:** root runtime test module registration and a NEW focused test module under src/runtime/runtime/tests/; own implementation report.

- [ ] Write representative tests against existing Runtime/testutil API that expose missing display selection/describe/observation contract (no production stubs just to pass).
- [ ] Compile-check Windows test sources only; record frozen source hashes and precise filters; stop for CC.
- [ ] CC cross-builds and runs only these tests on Windows mock backend, captures first red/native exit. No live desktop.

## Task 2 — Backend ownership and real Windows capture (root owner after red)

**Files:** root Cargo.toml/lock, src/backend/{mod,desktop,windows}.rs; new small display adapter modules. Do not edit the frozen libraries unless a concrete defect is separately assigned.

- [ ] Introduce explicit selection/topology/capture mapping boundary, persistent WindowsDisplay after verified PMv2 prepare on same thread.
- [ ] Native Windows observe uses real library enumeration and GDI provider, not old screenshots primary path; source/pre-post topology errors remain errors.
- [ ] Primary/id/Desktop errors are explicit; maps/physical bounds never fabricated from scaled image metadata.

## Task 3 — Runtime observation, mapping and drag (root owner)

**Files:** focused src/runtime/display*.rs or submodules, existing runtime.rs/session.rs/image.rs/plan.rs/execute.rs and associated test modules.

- [ ] Session binds selection and complete generation; observe/resume/step reject changes before input. New session may rebind after close; no permanent stale latch.
- [ ] Valid single-click/move/scroll/user drag waypoints must map to actual display regions. Gap/padding rejected before native input; edge rounding cannot leak outside a screen.
- [ ] Use topology DragPlan for internal cross-gap interpolation; validate topology before every native move and cancel/release on changes or query failures, keeping partial/cleanup truth.
- [ ] Keep idempotency, stale observation, input-outcome/observation-outcome separation, cancellation/quarantine and held-state semantics intact.
- [ ] Split responsibility rather than growing session.rs past1000 lines.

## Task 4 — MCP metadata, budgets and feedback adapter (root owner)

**Files:** src/runtime/tools.rs, capability/metadata modules, src/mcp output handling as required, src/feedback/{authority,backend}.rs or actual corresponding modules, root tests/docs.

- [ ] Schema/describe/open/observation contract matches locked choices above and rejects unsupported arguments.
- [ ] Checked budget admission before allocation; actual final JSON/encoded output length verified for local and remote transport; preserve existing caps and fail with structured error rather than silently truncating.
- [ ] FactBackend forwards new selection/topology API exactly, including errors; optional feedback off still works without renderer.
- [ ] Desktop feedback Surface.id=desktop and bbox derived from actual selected plan. No new feedback protocol fields.

## Task 5 — Windows renderer Desktop surface (disjoint renderer owner)

**Files:** desktop-feedback/windows/**, renderer README relevant Windows note, own report. No Rust/macOS/build artifacts edits.

- [ ] Extract pure screen/layout resolver; strict single-match vs declared fullDesktop union, reject mismatch/overlap/nonfinite geometry, handle negative origins and gaps.
- [ ] Keep status/Stop inside a real selected monitor working area; pointer rings only on actual selected screen regions, not desktop bbox gaps. Missing topology remains fail-stop, not silent fallback success.
- [ ] Keep stop callback, no-activation/clickthrough split, actual GetDpiForWindow scaling, affinity setup and capture-exclusion claim boundaries.
- [ ] Add pure C#5 selftests preserving existing147 checks; compile-only, freeze; CC later builds/runs Windows selftests (no GUI).

## Task 6 — Independent Windows verification and follow-up

- [ ] CC+GLM audits changed contracts, cross-builds Windows (new output dir), executes new mock tests plus affected existing tests with numeric native exit.
- [ ] CC Windows csc/selftests for renderer separately, preserve first failures and return code changes to corresponding Codex owner.
- [ ] Only after pure gates and authorization/multiscreen equipment: true source-free CC+GLM visual trials multi01..12, stale/hotplug/mixedDPI/crossscreen drag/Stop/capture exclusion.
- [ ] Update acceptance evidence without claiming library/mock green is native GUI completion; remaining cancel/focus/geometry/crossapp suites still required by full plan.
