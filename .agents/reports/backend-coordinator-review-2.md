# Backend second repair — coordinator review

2026-09-24. Sources were inspected after Claude backend-fix-2 changes. The worker was stopped after its scoped patch was independently verified, while it was repeatedly investigating unrelated whole-library failures; exit143 was verified. No claim that the worker itself delivered a completion report.

- Actual `DesktopBackend` no longer stores a backend-lifetime `last_capture_geometry`; fresh Runtime sessions can bind current geometry rather than being permanently rejected by old backend state.
- Existing held-input release tracking and scroll-sign fixes remain in the actual production BackendCore path.
- `cargo test --lib backend:: --offline`: exit0,30 passed,1 intentionally ignored live diagnostic. Evidence `.agents/runs/coordinator-backend-tests-3.log`.
- Important evidence boundary: newly added `CaptureGeometryBinding`/scripted-capture tests live only under cfg(test). They illustrate intended per-session policy but are NOT tests of production Runtime or OS capture. Independent runtime tests are still the authority for pre-input geometry safety, currently failed and assigned core-fix-2.
- No native GUI input was tested. No claim of Windows capture correctness from compilation alone. Windows dimension derivation explanation was reviewed against installed dependency code, but scaled-display live coverage remains missing. Formula-rounding comment versus dependency cast should not substitute for real native verification.
- Native capture currently samples target geometry before capture; an OS geometry change during native capture may require a post-capture recheck. Runtime must at least recheck live geometry before input (core-fix-2 priority). This gap is not waived by a fake backend test.
- macOS preliminary screen capture succeeded, but later visual inspection revealed local desktop LOCKED. Operator must unlock for native application tests. No attempted credential input/bypass.
