# Native-input core follow-up — implementation report, 2026-09-29

Scope: `.agents/tasks/native-input-core-followup-20260929.md` findings 1-8.
Owned files only: `crates/native-input/src/{lib,types,session}.rs`, crate
README, root `Cargo.toml`/`Cargo.lock`, `src/backend/**`, `src/runtime/**`,
`.gitignore`. Platform-writer files
(`crates/native-input/src/{windows,macos,linux_x11}*`) were NOT touched.

## Finding 1 — whole-payload text preflight (FIXED)

- `src/backend/keys.rs::validate_text`: shared C0 policy — NUL, every other
  C0 control (U+0000-U+001F) and DEL (U+007F) rejected; TAB/LF/CR accepted
  (they map to real Tab/Return clicks downstream). Error code
  `invalid_text`, message names the offending `U+XXXX`.
- `src/runtime/actions.rs::parse_action` (text_input branch): whole-payload
  preflight now calls the shared `validate_text` after the length check, so
  `"abc\0"` is rejected at parse time — no scalar is planned, let alone
  injected. Message context: `text_input.text: ...`.
- Tests:
  - `keys.rs::text_validation_rejects_c0_and_del_but_keeps_crlf_tab`
    (rejects NUL/0x01/0x07/0x08/0x0B/0x0C/0x1B/0x1F/0x7F; accepts CRLF/TAB
    and `hello 世界`).
  - `actions.rs::text_input_rejects_c0_and_del_before_any_plan_exists`
    (parse-time rejection, `invalid_action` protocol code, `\t\n\r` passes).
  - Session-level zero-events regression
    `session/tests.rs::invalid_chord_injects_zero_events` extended with
    `"abc\0"`, `"ok\u{1}later"`, `"bell\u{7}"`, `"a\u{7f}b"` — asserts
    `InputOutcome::NotStarted`, zero injected events, zero release_all
    calls (no partial injection of the scalars before the control char).

## Finding 2 — preserve InputError.code (FIXED)

- `src/backend/dispatch.rs::input_error`: now passes the driver's code
  straight through (`BackendError::new(e.code.to_owned(), ...)`) instead of
  flattening everything to `input_failed`. `permission_denied`,
  `no_display`, `unsupported_session`, `invalid_text` all survive; the
  protocol mapper (`src/runtime/error.rs::backend_code`) maps each to its
  proper protocol code, with a safe `INPUT_ERROR` wildcard for any future
  code. Verified no control-flow in the tree matches specific backend
  codes. Diagnostic context stays in the message.
- Platform `create_driver` wrappers now preserve the code too:
  `macos.rs`, `windows.rs`, `linux.rs` use `e.code.to_owned()` with
  context in the message. (`unsupported_session` preflight in linux.rs was
  already explicit.)
- New test `dispatch.rs::driver_input_preserves_driver_error_codes`: a
  `FailingDriver` stub (raw `rpa_native_input::Driver`) wrapped in
  `DriverInput`, asserting all five `NativeInput` methods preserve each of
  `permission_denied`/`no_display`/`unsupported_session`/`invalid_text`
  and keep the operation context in the message.

## Finding 3 — wayland_session_detected OR bug (FIXED)

- `src/backend/linux.rs`: rewritten as an independent OR — nonempty
  `WAYLAND_DISPLAY` OR `XDG_SESSION_TYPE=wayland` (case-insensitive). An
  empty-but-set `WAYLAND_DISPLAY` no longer short-circuits to false.
- Tests: new `empty_wayland_display_does_not_mask_wayland_session_type`
  (`Some("")+Some("wayland")` → true, case-insensitive variant) and
  extended `x11_sessions_pass` (`Some("")+None` → false; existing
  `Some("")+Some("x11")` → false kept).

## Finding 4 — restricted X11 text documented honestly (DONE)

- Platform repair state verified: `crates/native-input/src/linux_x11.rs`
  no longer contains `XChangeKeyboardMapping` remapping or an
  `XSetErrorHandler` swap (module docs state the restriction).
- `crates/native-input/README.md`: new statement that X11 text support is
  RESTRICTED (current-keymap, non-Shift-mapped characters only; unmapped/
  shifted-only chars rejected with `invalid_text`), NOT full Unicode;
  Windows/macOS inject arbitrary Unicode. C0 policy paragraph updated to
  the shared policy (no clipboard fallback for unmapped characters).
- `crates/native-input/src/types.rs` `Driver::text_scalar` doc: NUL, other
  C0 and DEL rejected by callers.

## Finding 5 — cooperative desktop assumption documented (DONE)

- `crates/native-input/README.md` new section "Held state and the
  cooperative desktop assumption": `release_all` releases session-owned
  items only; explicit low-level release of not-owned items remains
  documented caller responsibility; NO protection against a physical user
  pressing the same key the session holds — concurrent human/automated
  contention is outside the crate's guarantees.
- Mirrored in `crates/native-input/src/session.rs` module docs.

## Finding 6 — review report disposition (READ + reported)

Read `.agents/reviews/native-input-initial-20260929.md` in full.
**No findings fall in my write set** (they target platform files owned by
active writers). Report for the coordinator:

- **B1** macOS `CGEventCreateScrollWheelEvent2` FFI 5-arg vs SDK 6-arg
  (macos.rs) — ABI/garbage-third-wheel class. NOT fixed here (not my file).
- **B2** macOS `UCKeyTranslate` `UniCharCount` width: `u32` vs `c_ulong`
  (macos.rs) — stack-corruption class. NOT fixed here.
- **B3** Linux existing-mapping path types lowercase for uppercase input
  (`keysym.rs::find_mapped_keycode` matches shift-level columns,
  `linux_x11.rs` clicks without Shift) — silent text corruption. NOT fixed
  here.
- **S1** `choose_spare_keycode` ignores `XQueryKeymap` bitmap (keysym.rs).
- **S2** global `XSetErrorHandler` swap not serialized vs other Xlib users
  — NOTE: moot now; the platform repair REMOVED the handler swap entirely
  (verified by grep, linux_x11.rs module docs).
- **S3** Windows `KEYEVENTF_UNICODE` with `wVk = VK_PACKET` (builder.rs):
  MSDN says wVk must be 0 with KEYEVENTF_UNICODE — verify live. Only
  visible artifact in my build: dead-code warning for VK_PACKET in
  windows/builder.rs (report only, not touched).
- **S4** temporary-mapping race — moot after the repair (remapping
  removed); restricted-text honesty implemented per follow-up finding 4.
- **S5/S6 + minors** — all in platform files or the contract text; no
  action in my write set. Cross-platform `scroll(0)` divergence noted as
  report-only (contract silent).

## Finding 7 — .gitignore nested target (DONE)

- `.gitignore` now ignores `/crates/native-input/target` (crate keeps its
  empty `[workspace]` / standalone target dir). No files deleted; builds
  and evidence under `.agents/runs/` untouched. Only owned-file formatting
  touched; no `cargo fmt --all` run.

## Finding 8 — Windows root compile fix (DONE, verified)

- `src/backend/windows.rs`: `HPROCESS`/`System::Threading` import removed;
  DPI types now use the actual windows 0.58 namespaces —
  `windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT, ...}` and
  `GetDpiAwarenessContextForProcess(HANDLE::default())` with
  `windows::Win32::Foundation::HANDLE` (no HPROCESS in 0.58).
- Root `Cargo.toml`: windows features trimmed to
  `["Win32_Foundation", "Win32_UI_HiDpi"]` (Win32_System_Threading no
  longer needed).
- Verified with the documented Mac→Windows env from
  `docs/remote-connection.md` (cargo xwin env + rust-lld AR/ARFLAGS, NO
  tool installs):
  `cargo check --target x86_64-pc-windows-msvc --lib` → **Finished, no
  warnings in owned files** (only the reported VK_PACKET dead-code warning
  in the platform writer's builder.rs).

## Test evidence (CARGO_TARGET_DIR=.agents/runs/cargo-native-core-followup)

| Suite | Result |
|---|---|
| `cargo test --manifest-path crates/native-input/Cargo.toml` | 29 passed, 0 failed |
| root `cargo test --lib` | 212 passed, 0 failed, 1 ignored |
| root `cargo test --tests` (integration) | 212 + 12 + 17 + 35 + 3 passed, 0 failed, 1 ignored |
| Windows cross-check `cargo check --target x86_64-pc-windows-msvc --lib` | Finished, clean |

No GUI/SSH/commit/subagents performed. All tests are mock-based; no real
desktop input was performed.
