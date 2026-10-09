# native-input Windows repair (2026-09-29)

Task: `.agents/tasks/native-input-windows-repair-20260929.md` (coordinator-authorized repair).

## Scope (owned files only)

- `crates/native-input/src/windows.rs`
- `crates/native-input/src/windows/builder.rs`

No other files touched. No GUI input, no SSH, no commit, no spawned processes beyond local cargo/rustfmt.

## Critical fix: wVk must be ZERO for KEYEVENTF_UNICODE

Per the Microsoft `KEYBDINPUT` contract (https://learn.microsoft.com/windows/win32/api/winuser/ns-winuser-keybdinput), for `KEYEVENTF_UNICODE` events `wVk` **MUST be 0** and `wScan` carries the UTF-16 code unit. `VK_PACKET` is the virtual-key code the system *synthesizes* for the resulting `WM_KEYDOWN`/`WM_KEYUP` messages — it is an output of the pipeline, never a valid input `wVk`.

The previous writer's `Platform::to_input` assigned `wVk = VK_PACKET` for every Unicode event (and the builder planned text events with `vk = VK_PACKET`), violating the contract.

Fix:

1. `builder.rs` `text_plan`: BMP and surrogate-pair Unicode events now carry `vk: 0` with the code unit in `scan` + `KEYEVENTF_UNICODE` (± `KEYEVENTF_KEYUP`). The `VK_PACKET` constant moved into the builder test module with a contract doc comment (it is only referenced by tests now; keeping it in non-test code triggered `dead_code`).
2. `windows.rs` `to_input`: Unicode branch sets `wVk = VIRTUAL_KEY(0)` (the `vk` field of the plan, now 0) and passes `wScan` through unchanged; only non-Unicode VK events get `wScan` enriched via `MapVirtualKeyW`. Removed the now-unused `VK_PACKET` import.

## scroll(0) is now a no-op

`Driver::scroll` returns `Ok(())` for `length == 0` before any planning or `SendInput`, matching the macOS driver's semantics (a zero-delta wheel event is not meaningful input). `builder::wheel` still rejects zero as a plan-level invariant, but the driver no longer reaches it.

## FFI conversion review (no further defects found)

- Non-Unicode VK path: `wVk = vk`, `wScan = MapVirtualKeyW(VK_TO_VSC) & 0xFFFF`, extended-key flag preserved — correct per contract.
- Mouse absolute move: `MOVE | ABSOLUTE | VIRTUALDESK` with inclusive 0..=65535 normalization over virtual-screen origin/extent, i64 math, negative origins handled — verified by existing edge/monotonicity tests, unchanged.
- Wheel direction: vertical positive length negated to DOWN, horizontal positive = RIGHT — unchanged, correct.
- Partial-send cleanup in `text_scalar`: releases only downs this driver injected whose up was not injected; original failure detail preserved, and cleanup failure is reported with both counts — reviewed, logic sound, covered by pure tests.
- `send`/`single` count handling and empty-plan short-circuit — sound.

## Regression tests (no SendInput anywhere in tests)

1. **builder.rs (pure, host-independent, run on this macOS host):**
   - `unicode_events_never_carry_vk_packet_as_input_vk` (new): asserts every text event for `a`, `é`, `中`, `😀`, `U+10FFFD` has `vk == 0` and `vk != VK_PACKET` with the UNICODE flag set.
   - Updated `bmp_scalar_uses_unicode_down_up_pair` / `supplementary_scalar_uses_high_down_low_down_high_up_low_up` to expect `vk: 0`.
2. **windows.rs `#[cfg(test)]` module (new, actual INPUT conversion):** builds real `INPUT` records via `Platform::to_input` and inspects `KEYBDINPUT`/`MOUSEINPUT` fields — no `SendInput` calls; the only FFI touched is side-effect-free `MapVirtualKeyW`:
   - `unicode_input_has_zero_wvk_and_code_unit_in_wscan` — wVk == 0 (explicitly `!= 0xE7`), wScan == U+00E9, flags exact for down/up.
   - `supplementary_unicode_input_uses_surrogate_units_with_zero_wvk` — all four surrogate events: wVk == 0, wScan == D83D/DE00, exact flags.
   - `vk_input_carries_vk_and_mapped_scan` — Return press/release: wVk == 0x0D, wScan equals a direct `MapVirtualKeyW` reference call, flags exact.
   - `extended_key_input_keeps_extended_flag` — Delete keeps `KEYEVENTF_EXTENDEDKEY`.
   - `mouse_move_input_carries_absolute_virtualdesk_fields` and `wheel_input_carries_signed_delta` — mouse field mapping.
   - `scroll_zero_is_noop_success` — `scroll(0, …)` returns Ok for both axes with zero injection (early return before planning).

## Verification (CARGO_TARGET_DIR=.agents/runs/cargo-native-windows-repair)

1. **Pure tests, macOS host** via scratch harness `.agents/runs/cargo-native-windows-repair/pure-tests/` (contract enums mirrored verbatim; real `builder.rs` included by `#[path]`): `cargo test` → **24 passed, 0 failed** (was 23; +1 regression test).
2. **Windows-target compile:** `cargo check --target x86_64-pc-windows-msvc` → Finished, **zero errors, zero warnings** from my files.
3. **Windows-target test code type-check:** `cargo check --target x86_64-pc-windows-msvc --all-targets` → Finished clean. (Note: running this from the repo root instead builds the main workspace and fails in `ring`'s build script — expected cross-compile limitation of the root crate, unrelated to this crate.)
4. **Clippy, Windows target, all targets:** no diagnostics for `windows.rs`/`builder.rs`. One remaining warning is in core-owned `session.rs:310` (`clippy::type_complexity`) — out of my scope.
5. **rustfmt:** `rustfmt --edition 2021 --check` clean on both owned files.

## Limitations

- The `windows.rs` INPUT-conversion tests **type-check** for the Windows target here but can only **execute** on a real Windows host (no emulator on this Mac; running cross-compiled test binaries was out of scope). Coordinator can run `cargo test --target x86_64-pc-windows-msvc` inside `crates/native-input` on acer-win.
- The tests prove correct `INPUT` construction only; `SendInput` count success still does not prove a target app processed the events — actual text injection success still requires scheduled GUI verification per contract.
- macOS-host pure harness cannot execute `Platform`-level code (display checks, `SendInput`); those paths are verified by compile/type-check only, as before.
