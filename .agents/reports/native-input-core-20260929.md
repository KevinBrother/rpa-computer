# native-input core/task1 report (2026-09-29)

Worker: core (Claude CLI + GLM). Scope: contract skeleton, crate core files,
root host integration, plan/execute text pipeline, tests. No live desktop
input, no SSH, no commit/push, no other agents spawned.

## Files written/owned by this worker

Crate `crates/native-input` (package `rpa-native-input`, edition 2021,
standalone via empty `[workspace]`, zero dependencies except the target-gated
`windows` binding):

- `Cargo.toml` — windows dep `0.58` with features `Foundation`,
  `Win32_Foundation`, `Win32_UI_Input_KeyboardAndMouse`,
  `Win32_UI_WindowsAndMessaging`, `Win32_UI_HiDpi`. DEVIATION from contract
  text: the contract listed `UI_Input_KeyboardAndMouse` etc., but in the
  published windows 0.58 crate the real feature names carry the `Win32_`
  prefix (verified by cargo feature resolution — the bare names do not
  exist). `Win32_Foundation` added because BOOL/DPI types referenced by the
  HiDpi functions live there. This is the same feature set, spelled as the
  registry requires.
- `src/lib.rs` — module declarations per contract (`#[cfg(target_os=...)]`
  windows/macos/linux_x11), public re-exports, and `native_driver() ->
  Result<Box<dyn Driver>>`: the documented raw Driver factory. The host uses
  this factory through its own state machine and deliberately does NOT use
  the crate's `Input` wrapper, so exactly ONE component tracks held
  key/button state in the host process (`backend::dispatch_core::BackendCore`).
  `Input` remains the higher-level API for standalone users. This resolves
  the "two authoritative state trackers" question the task posed.
- `src/types.rs` — `Key`/`Button`/`Axis`/`Direction` (Copy,Debug,Eq,
  PartialEq,Hash), `InputError` (code: &'static str, message: String,
  Error+Display, `new()`), `Result<T>`, the `Driver` trait exactly as the
  contract specifies, plus `Key::is_valid()`/`Key::name()` helpers.
- `src/session.rs` — `Input` with `new`, `move_mouse`, `key`, `button`,
  `scroll`, `text_scalar`, `release_all`, owning held state. ADDITION to the
  shared interface (documented here, not silently): `Input::from_driver(
  Box<dyn Driver>)` as the test/alternative factory — required to test the
  held-state machine with a mock driver without real platform construction.
  Semantics: Character keys validated ASCII alphanumeric (`invalid_key`,
  no effect otherwise); NUL rejected (`invalid_text`); `'\n'`/`'\r'` → ONE
  Return click, `'\t'` → ONE Tab click; press recorded BEFORE dispatch,
  removed ONLY on successful release; `release_all` attempts every held item,
  retains failures, one aggregated `input_failed` error; only session-owned
  items are ever released.
- `README.md` — usage/semantics summary.

Root host integration (all in my ownership):

- `Cargo.toml` — `enigo` replaced by `rpa-native-input = { path = ... }`;
  target-gated `windows 0.58` dep (`Win32_Foundation`, `Win32_UI_HiDpi`,
  `Win32_System_Threading`) for the DPI bindings. `cargo tree` confirms no
  enigo/libxdo remains (`core-graphics` still present via `screenshots`,
  which is the capture path, not input).
- `src/backend/keys.rs` — alias table now maps to `rpa_native_input::{Key,
  Button}`; `ParsedKey::key()`; `enigo_direction` removed (root Direction →
  crate Direction mapped in the adapter).
- `src/backend/dispatch.rs` — `NativeInput` seam re-typed to crate types with
  `text_scalar(char)`; production `DriverInput` wraps `Box<dyn Driver>` and
  maps `InputError` → `BackendError("input_failed")`. All existing regression
  tests preserved and extended: per-scalar text pass-through, CRLF→ONE Return
  click, lone `\r`/`\n`/`\t` clicks, failed Return click half stays tracked,
  invalid names fail before any dispatch, press/release/release_all/Drop
  semantics unchanged.
- `src/backend/dispatch_core.rs` — `BackendCore` unchanged semantics; Text
  branch now validates the whole payload, then iterates scalars: CRLF
  normalized once, `'\n'`/`'\r'`→Return click and `'\t'`→Tab click THROUGH
  the held-state machine (`click_key`), other scalars → `text_scalar`.
- `src/backend/desktop.rs` — `BackendCore<DriverInput>`; platform hook
  renamed `create_enigo` → `create_driver`.
- `src/backend/macos.rs` — `create_driver`: AXIsProcessTrusted preflight (no
  prompt) + `rpa_native_input::native_driver()`.
- `src/backend/windows.rs` — DPI switched from hand-written user32 externs to
  official `windows` bindings (`SetProcessDpiAwarenessContext` returns
  `Result<()>`, BOOL via `.as_bool()`, `HPROCESS::default()` for the
  current-process query); `create_driver` via the crate factory. NOTE:
  Windows code path could NOT be compiled locally (macOS host, no SSH
  allowed) — needs the acer-win build verification step.
- `src/backend/linux.rs` (NEW) — Wayland refusal (`WAYLAND_DISPLAY` or
  `XDG_SESSION_TYPE=wayland`, even when DISPLAY is also set →
  `unsupported_session`; unit-tested pure detector), `create_driver` via the
  crate, X11 capture via `screenshots` with the same hard
  expected-vs-actual dimension check as Windows, documented scale audit
  (root pixels ARE input units; exotic compositor scales fail loudly rather
  than shipping mislabeled geometry). `mod.rs` now selects linux as platform
  and the compile_error guard covers remaining targets. Not compiled locally
  (macOS host) — needs a real Linux/X11 build to validate.
- `src/runtime/plan.rs` — TextInput now compiles to ONE `PlanEvent::TextScalar`
  event per Unicode scalar with interruptible `PlanEvent::Sleep(1)`
  (`TEXT_SCALAR_INTERVAL_MS`) BETWEEN scalars and no sleep after the last;
  `normalize_newlines()` collapses `"\r\n"` once (pure, tested). The 1ms
  interval is a candidate responsiveness measure, not a guarantee: the
  executor's cancellation check runs at event boundaries and the 5s
  INPUT_PHASE_BUDGET still bounds the whole phase — long texts (4096 chars →
  4096 events + 4095 intervals) can legitimately exceed it and are reported
  partial/deadline truthfully. INPUT_PHASE_BUDGET left unchanged (session.rs
  untouched per scope).
- `src/runtime/execute.rs` — test updated for per-scalar counting
  (events_total 640 for the 640-char cancellation case).

## Tests actually run (CARGO_TARGET_DIR=.agents/runs/cargo-native-core)

- TDD note: crate/session tests were authored before/with the implementation;
  a scratch copy under `.agents/runs/native-input-selftest` with a placeholder
  platform module let the mock-based tests run (red→green fixed two compile
  bugs and the error-message assertion) BEFORE the real macos.rs landed.
  Scratch copy removed after the real crate compiled.
- `cargo test --manifest-path crates/native-input/Cargo.toml` (macOS, real
  worker macos.rs): **27 passed, 0 failed** (types 4, session 13, macos
  platform tests 10).
- `cargo test --lib` (root, macOS): **210 passed, 0 failed, 1 ignored**
  (ignored = opt-in live capture diagnostic).
- `cargo test --tests` (root integration): protocol 12, remote_transport 17,
  runtime_contract 35, transport_lifecycle 3 — **all passed**.
- `cargo build` (root, dev incl. bins): clean, no warnings.
- Post-`rustfmt` (own files only, no `cargo fmt --all`) rerun: crate 27 +
  root lib 210 all green.
- `cargo tree`: no enigo, no libxdo.

## Blockers / deferred verifications

1. **Windows compile not verified locally** — root `src/backend/windows.rs`
   DPI rewrite uses official bindings written against windows 0.58 docs
   (signatures verified via docs: `Result<()>`, BOOL, HPROCESS); needs the
   real acer-win `cargo test/build` step from the plan. Windows worker's own
   crate files compile only on a Windows host.
2. **Linux compile/runtime not verified** — no Linux host here; `linux.rs`
   and the linux_x11 crate module are unvalidated until the authorized
   Linux environment runs them.
3. **Contract deviation (reported, not silent)**: windows feature names
   require `Win32_` prefixes (above); `Input::from_driver` added to the
   crate's public API; `native_driver()` factory added to lib.rs (both are
   additions, existing shared interface unchanged).
4. Platform worker files (`windows.rs`, `windows/builder.rs`, `macos.rs`,
   `linux_x11.rs`, `linux_x11/keysym.rs`) were created concurrently by their
   owners and were NOT touched by this worker. macos.rs was observed mid-edit
   (transient test compile errors) and compiled clean by final verification.
5. Text success on real GUIs (Windows Unicode scalars, per prior 12/34/38
   failures) remains unproven until the serialized real-GUI acceptance
   phase; nothing here claims document-level correctness from event counts.
