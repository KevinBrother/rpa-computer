# Windows platform implementation report — native-input (2026-09-29)

Task: `.agents/tasks/native-input-windows-20260929.md`
Contract: `.agents/tasks/native-input-contract-20260929.md`
Scope respected: only `crates/native-input/src/windows.rs` and `crates/native-input/src/windows/builder.rs` were written. No shared files (Cargo.toml, lib.rs, types.rs, session.rs), no other platforms, no root integration touched. No commits, no worktree, no live desktop input, no SSH, no global config changes.

## Files owned / written

| File | Purpose |
|---|---|
| `crates/native-input/src/windows.rs` | `Platform` driver: `INPUT` construction, `SendInput` injection, partial-send cleanup, display-presence check |
| `crates/native-input/src/windows/builder.rs` | Pure, platform-independent event planning (VK mapping, absolute-coordinate math, wheel direction, Unicode text plans, partial-send cleanup plans) + 23 unit tests |

`builder.rs` duplicates the needed Win32 flag/VK values as raw constants (each value verified against the locally cached `windows-0.58.0` bindings, e.g. `KEYEVENTF_UNICODE=0x4`, `MOUSEEVENTF_VIRTUALDESK=0x4000`, `VK_PACKET=0xE7`). This keeps the planning logic and its tests host-independent.

## What was implemented

### Driver (`windows.rs`)
- `Platform::new()` — best-effort display presence check via `GetSystemMetrics(SM_CXSCREEN / SM_CXVIRTUALSCREEN)`; both zero → `no_display`. No DPI-awareness calls, no other process-global side effects.
- All five `Driver` methods; each plans events in `builder`, converts to `INPUT`, injects with `SendInput(&inputs, size_of::<INPUT>() as i32)` (windows-0.58 slice signature), and checks the returned count.
- Non-VK key events get their scan code from `MapVirtualKeyW(vk, MAPVK_VK_TO_VSC)` at send time (Unicode/VK_PACKET events keep the UTF-16 code unit in `wScan`).
- **Partial sends**: for single-event operations (`move_mouse`, `button`, `key`, `scroll`) a count < 1 is reported as `input_failed` with the injected/total counts; nothing is auto-released (the session wrapper owns held key/button state per contract). For `text_scalar`, a partial injection triggers cleanup that releases exactly the downs *this driver injected* whose up was not injected (`builder::text_cleanup`), never unrelated user input; if the cleanup itself is partial, that is stated in the error. No fake success anywhere.
- No sleeps, no screenshots, no global key release, no clipboard.

### Pure builder (`builder.rs`)
- `Key` → (VK, extended) mapping: letters `0x41..=0x5A`, digits `0x30..=0x39`; extended flag for Delete, Home, End, PageUp/Down, arrows. `Meta` = `VK_LWIN` (0x5B). `Key::Character` accepts ASCII letters/digits only; everything else → `invalid_key` **before any injection**.
- Absolute mouse conversion over the virtual screen with `MOUSEEVENTF_ABSOLUTE | VIRTUALDESK | MOVE`, inclusive `0..=65535` mapping `((x-vx) * 65535 / (cx-1))` in i64 math, clamping off-screen/negative-origin positions into range.
- Wheel: positive length = DOWN (vertical, delta negated against Windows' positive-up convention) / RIGHT (horizontal); zero length and `i32` overflow rejected (`input_failed`).
- `text_scalar` plans: NUL → `invalid_text`; other C0 controls and DEL (except `\n \r \t`) → `invalid_text` (see contract note below); `\n`/`\r` → exactly one `VK_RETURN` click; `\t` → exactly one `VK_TAB` click (plain VK events, no duplicate Unicode); BMP scalars → one `KEYEVENTF_UNICODE` down/up pair with the code unit (VK_PACKET path, matching the Python `keyboard` library's successful path); supplementary scalars → highDown, lowDown, highUp, lowUp surrogate-pair events as the contract specifies.

## Contract compliance notes / one deliberate extension

- The contract only mandates rejecting NUL in `text_scalar`. I additionally reject the remaining C0 controls (0x01–0x1F excluding \n\r\t) and DEL (0x7F) with `invalid_text`, because `KEYEVENTF_UNICODE` does not reliably inject them; treating them as injectable would be fake success. **Flagging for core review** — if callers need them, the contract should say how.
- `Meta` press alone will open the Start menu on release-without-other-keys (inherent to VK_LWIN); callers should use it only inside chords. Noted for the session/acceptance layers.

## Verification actually performed

1. **Pure tests — 23 passed, 0 failed** on this macOS host. The crate gates `mod windows` behind `target_os = "windows"`, so the tests cannot run under the real crate here. They run through a scratch harness that includes the real `builder.rs` by `#[path]`:
   - Harness: `.agents/runs/cargo-native-windows/pure-tests/` (lib.rs mirrors the contract enums exactly and includes `crates/native-input/src/windows/builder.rs` verbatim)
   - Command: `cd .agents/runs/cargo-native-windows/pure-tests && CARGO_TARGET_DIR=../target cargo test` → `test result: ok. 23 passed; 0 failed` (two earlier failures during development: an off-by-one `#[path]` depth, and a test bug where `step_by(64)` never sampled the final pixel — fixed in the test, not the implementation).
   - Coverage: full VK table incl. extended flags; Character validation (`'!'`, `'é'`, `'中'`, space, NUL rejected); key press/release flag combos; all 6 button×direction flag combos; virtual-screen edge/clamp/monotonicity over all 5760 positions of a negative-origin triple-monitor layout; single-monitor edges; wheel sign/direction/overflow/zero; text plans for `\n`/`\r`/`\t`/NUL/controls/BMP/surrogates; cleanup plans at every partial-send prefix (0..4) for BMP, surrogate and Return-click plans.
2. **Windows-target compile of the lib — pass.** `x86_64-pc-windows-msvc` is installed locally; `cd crates/native-input && cargo check --target x86_64-pc-windows-msvc` → `Finished` with **zero errors and zero warnings from my files**. Clippy on the Windows target also reports nothing for `windows.rs`/`builder.rs`.
3. **Windows-target type-check of test code — blocked, not by my files.** `cargo check --target x86_64-pc-windows-msvc --all-targets` fails with 16 errors, **all in core-owned `src/session.rs`** (missing `Rc`/`RefCell` imports in its `#[cfg(test)]` module, lines ~233–305). My builder tests are in the same lib-test target and produced no diagnostics, so they type-check for the Windows target too. Core should add `use std::{cell::RefCell, rc::Rc};` to session.rs's test module.

## Not verified (honest gaps)

- **No real Windows GUI injection was performed** — per contract/README, live desktop input must be scheduled by the coordinator on acer-win's interactive session. I never ran SendInput anywhere; count of injected events is verified only as far as the API contract allows (SendInput returning the count does **not** prove the target application processed the events — actual text success still needs GUI verification as the contract states).
- The `MapVirtualKeyW` scan-code fill and the `Platform::new()` display check are Windows-only code paths that compile clean but have no runnable tests here.
- I did not attempt to *run* cross-compiled msvc test binaries (impossible on this host; no emulator, and running one would still not exercise a desktop).

## Blockers / asks for core

1. `src/session.rs` test module is missing `Rc`/`RefCell` imports — blocks `--all-targets` checks for every platform target.
2. If `types` module layout changes (I assumed `crate::types::Key/…`, which lib.rs confirms today), my files need a matching one-line import update.
3. Contract decision requested on C0-control rejection in `text_scalar` (above).
4. Live GUI verification of text (BMP + emoji) and chords on acer-win interactive desktop remains open and must be coordinator-serialized.

## API reference used

Local cargo registry sources only: `windows-0.58.0` bindings (`SendInput(&[INPUT], i32)`, `MOUSEINPUT`, `KEYBDINPUT`, `GetSystemMetrics(SYSTEM_METRICS_INDEX)`) and `enigo-0.3.0` was available but not copied from. No xdotool-style external tooling introduced.
