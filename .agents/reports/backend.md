# Backend worker report: native desktop backend (Task 3)

Date: 2026-09-24. Scope: `src/backend/**` only (plus one-line module registration
request in lib.rs, see Conflicts). No commits, no pushes. No desktop input was
injected during implementation; no other application was operated.

## Files written

| File | Contents |
|---|---|
| `src/backend/mod.rs` | Public contract API: `Geometry`, `Capture`, `Direction`, `InputEvent`, `BackendError`, `Backend` (NOT Send), `DesktopBackend` re-export; error-code mapping; module docs defining the coordinate model. |
| `src/backend/desktop.rs` | `DesktopBackend`: pressed-state tracking (`HashSet<HeldItem>`), `inject` with pre-input validation, `release_all` (tries every held item, aggregates failures), fail-fast `geometry_changed` on capture-target change, Drop best-effort release. |
| `src/backend/keys.rs` | Key/button validation: all contract aliases (`ctrl/control`, `alt/option`, `meta/cmd/command/win/super`, `enter/return`, `escape/esc`, `pageup/page_up`, f1–f12, …) + single ASCII alphanumeric as a **real key event** (`Key::Unicode(c)`, never `text()`), so chords are genuine key presses. Structured `invalid_key`/`invalid_button`/`invalid_text` errors before any input. |
| `src/backend/capture.rs` | Primary-display selection on every call, geometry version string (display id + origin + input size + capture pixel size + rotation), PNG encoding with dimensions read back from the encoded image. |
| `src/backend/macos.rs` | macOS primitives: `AXIsProcessTrusted` preflight WITHOUT prompt (`permission_denied` otherwise), `CGPreflightScreenCaptureAccess` before capture (`screen_capture_denied`), capture via `screencapture -x -D <id> -t png -` (real backing-store pixels), dimension cross-check against `CGDisplayPixelsWide/High`. |
| `src/backend/windows.rs` | Windows primitives: Per-Monitor-V2 DPI awareness **set and verified** (error, not silent, if it fails), GDI capture via `screenshots`, pixel-dimension validation against `display_info`-derived native size. |
| `src/backend/live_test.rs` | Ignored manual capture-only diagnostic (`cargo test --lib -- --ignored live_backend`). |

`capture`/`desktop` modules are private; the public surface is exactly the
contract items plus `keys::{parse_key, parse_button, KEY_NAMES, BUTTON_NAMES}`
and `capture::{target_screen, encode_png, TargetScreen, ScreenCaptureError}`
(reusable helpers; no extra contract types changed).

## Semantics verified against installed crate sources

- **display-info 0.4.8 macOS**: `bounds()` = global points; `scale_factor` from
  `pixel_width/point_width`. So `input_size` = points, capture = points ×
  backing scale (Retina). `screencapture` pixels are cross-checked with
  `CGDisplayPixelsWide/High` — mislabeled dimensions are a hard error.
- **display-info 0.4.8 Windows**: `rcMonitor` + `DESKTOPHORZRES/HORZRES` scale;
  with PMv2 awareness these are native pixels, matching GDI capture and
  enigo `SendInput` absolute coordinates. `screenshots` win32 capture multiplies
  by `scale_factor`; our process is made PMv2-aware so all three agree.
- **enigo 0.3 scroll**: macOS negates (`-length`) into CGEvent wheel deltas;
  Windows negates into `WHEEL_DELTA`. Native semantic = positive up. The
  backend negates both axes once (`inject`: `scroll(-x)`, `scroll(-y)`) so the
  contract semantic **positive = right/down** holds on both OSes.
- **enigo 0.3 macOS `Enigo::new`**: calls `AXIsProcessTrustedWithOptions` with
  the Settings prompt flag; backend passes `open_prompt_to_get_permissions:
  false` and preflights itself — the backend never opens the system permission
  dialog.
- **enigo 0.3 Windows move**: absolute moves scale `(x,y)` by main-display
  `GetSystemMetrics` into 0..65535; with PMv2 awareness these are native
  pixels, matching `display_info` units. Multi-monitor negative offsets are a
  known enigo limitation, but the primary display (our target) starts at
  (0,0), so in-target coordinates are correct.
- **`Key::Unicode(c)`** produces real key press/release events on both OSes
  (macOS: CGEvent key events; Windows: SendInput Unicode KEYEVENTF), used for
  ASCII alphanumeric so `key_chord` modifiers are real held keys.

## Held-state / cleanup semantics

- Press records the item as held **before** the inject call returns (partial
  injection still gets a release attempt); release removes it before calling
  (a failed release is reported, and `release_all`/Drop remain the recovery
  paths — enigo's own Drop also releases held keys).
- `release_all` drains the set first, then attempts **every** item,
  aggregating all failures into one `input_failed` error listing each item.
- `capture()` compares the live geometry version with the last capture's and
  returns `geometry_changed` instead of silently returning a new display's
  pixels under old metadata.
- `geometry()` is a pure query; success does not claim input works. `new()`
  performs zero input.

## Error codes

`no_display`, `screen_capture_denied`, `capture_failed`, `permission_denied`,
`input_failed`, `invalid_key`, `invalid_button`, `invalid_text`,
`geometry_changed`, `internal`. All via structured `BackendError{code,message}`.

## Test evidence (this machine, aarch64-apple-darwin)

- `cargo build --lib` — OK.
- `cargo test --lib` — **18 passed, 0 failed, 1 ignored** (unit tests cover:
  key/button validation incl. all aliases and rejection cases, text NUL
  rejection, geometry version sensitivity to id/origin/size/DPI/rotation, PNG
  actual-dimension reporting + size-mismatch rejection, error-code mapping,
  scroll event shape, backend error structure).
- `rustfmt --check src/backend/*.rs` — clean. (Note: whole-repo
  `cargo fmt --check` currently flags `src/bin/computer-host.rs`, owned by
  transport, not backend.)
- `cargo clippy --lib` (macOS) — 0 warnings. `cargo clippy --lib --target
  x86_64-pc-windows-msvc` — 0 warnings.
- `cargo build --lib --target x86_64-pc-windows-msvc` — **compiles OK** on the
  installed cross target (rustc codegen; no native Windows linker run here —
  full Windows binary build belongs to QA's acer-win task).
- Live capture-only diagnostic: `cargo test --lib -- --ignored live_backend`
  → `DesktopBackend::new()` returned `[no_display] no primary display found`
  on this headless SSH session. **This is reported honestly, not worked
  around**: geometry/capture need a real GUI session (coordinator runs it from
  the interactive console). The diagnostic proves the error path is
  structured and non-panicking.
- `cargo clippy --all-targets` fails in `tests/runtime_contract.rs`
  (`rpa_computer::runtime` unresolved) — **core has not yet published the
  `runtime` module**; expected per contract ("core may temporarily declare
  modules before files arrive"). Backend compiles independently.

## True limitations / known gaps

1. **Geometry change between a capture and a later `inject`**: the backend
   versions captures and fails fast on capture-side changes, but `inject`
   itself has no geometry parameter; staleness between capture and inject is
   the Runtime's `based_on`/geometry-version check (per contract). Backend
   cannot detect a display change that happens with no intervening capture.
2. **Windows multi-monitor**: enigo 0.3 absolute moves use main-display
   metrics without `MOUSEEVENTF_VIRTUALDESK`; coordinates on non-primary
   displays with negative origins would be wrong. Target is the primary
   display, so contract behavior is correct; documented here for completeness.
3. **Windows permission honesty**: there is no input grant on Windows;
   `DesktopBackend::new()` succeeding does NOT prove inject will work
   (Session 0 / locked desktop / UIPI). Inject failures surface as
   `input_failed`. The acer-win Host must run in the interactive session
   (transport/QA responsibility).
4. **macOS capture via `screencapture` CLI**: ~1 subprocess per capture;
   permission preflight is done via `CGPreflightScreenCaptureAccess` because
   the CLI exits 0 with a wallpaper-only image when denied. If the deployment
   target is macOS < 10.15 the preflight symbol is unavailable (not handled;
   macOS 10.15+ assumed).
5. Non-macOS/Windows targets: hard `compile_error!` — no fake fallback.
6. `Backend` is intentionally not `Send`; nothing in `src/backend` spawns
   threads or moves the enigo instance across threads. Construct on the worker
   thread (transport does this per its design).

## Dependency requests (for core / Cargo.toml)

- **None required.** Backend uses only existing deps: `screenshots` 0.8 (with
  re-exported `display_info`), `enigo` 0.3, `image` 0.24/png, `thiserror` not
  needed (hand-written error). macOS uses direct FFI to
  CoreGraphics/ApplicationServices (always-present frameworks).

## Conflicts / coordination notes

- Contract says core owns `src/lib.rs`. During this task `pub mod backend;`
  was added to lib.rs to prove compilation; core's rewrite owns the final
  file — please keep `pub mod backend;`. No other shared files touched.
- Contract API implemented exactly as specified: `Geometry`, `Capture`,
  `InputEvent`, `Direction`, `BackendError{code,message}`, `Backend` trait
  (not Send), `DesktopBackend::new() -> Result<Self, BackendError>`.
- Runtime contract note: `Move` coordinates arrive already mapped to native
  input units; backend does **not** rescale (per contract "core maps model
  image coordinates to your native units"). Click-count/drag/hold timing are
  Runtime concerns; backend only executes atomic events and never sleeps.
