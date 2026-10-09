# rpa-native-input

Extractable native desktop input for Windows, macOS and X11, built on raw
platform APIs (SendInput, CoreGraphics, X11/XTest). No host, MCP, screenshot,
serde, image, agent or network dependencies.

## Layers

- `Driver` (raw seam): per-platform `Platform` implementations behind
  `native_driver()`. Absolute desktop-native coordinates (Windows
  virtual-screen pixels, macOS CoreGraphics points, X11 root pixels); scroll
  positive = right/down in wheel ticks.
- `Input` (session wrapper): owns local held-key/button state. Presses are
  recorded BEFORE dispatch (a partial press is still cleaned up); releases
  are removed from tracking only on success; `release_all` attempts every
  held item, retains failures and reports one aggregated error. Only items
  the session itself pressed are ever released.

## Atomic button metadata

Both `Driver::button(button, direction, click_count)` and
`Input::button(button, direction, click_count)` emit one atomic press or
release. `click_count: u8` must be `1..=3`; invalid metadata returns
`invalid_button` before dispatch and does not change held state. It is the
native click-state of THIS pair, not a request for a driver-side click loop.
The caller emits count-N pairs with metadata 1, 2, ..., N and owns timing
and cancellation. Drag uses count 1.

macOS places the count in `kCGMouseEventClickState`. Ordinary mouse moves
use click-state 0, and drag motion uses 1. Windows and X11 emit real atomic
SendInput/XTest events; OS/application timing and position determine
multi-click recognition. Drivers add no multi-click sleeps or loops.

`Input` remembers the last intended press count per button BEFORE dispatch,
including uncertain failed presses. Repeated presses replace the metadata;
cleanup never accumulates duplicate count-bearing held entries.
`release_all` and `Drop` use that stored count. A failed explicit release or
cleanup keeps the original press metadata for a later attempt; invalid
releases do not erase it. Explicit releases use the caller-supplied count.

## Key semantics

- `Key::Character` is a physical/layout key for chords — single ASCII letters
  or digits only, never Unicode text injection. Everything else is rejected
  with `invalid_key` before any effect.
- `text_scalar` injects one Unicode scalar: NUL, other C0 controls and DEL
  are rejected by CALLERS (`invalid_text`); only TAB/LF/CR are accepted
  controls, mapping to real Tab/Return clicks. CRLF sequences are normalized
  once by the caller. Text never uses the clipboard — there is no fallback
  path for unmapped characters.
- **X11 text support is RESTRICTED, not full Unicode**: the platform repair
  deliberately removed the unsafe transient keymap remapping / error-handler
  swapping. `text_scalar` on X11 accepts only characters mapped in the
  CURRENT keymap without Shift (plus TAB/LF/CR handling above); currently
  unmapped or shifted-only characters are rejected honestly with
  `invalid_text`. Windows and macOS inject arbitrary Unicode via
  `SendInput`/event-stream Unicode paths.
- Errors carry stable codes: `permission_denied`, `no_display`,
  `unsupported_session`, `input_failed`, `invalid_key`, `invalid_button`, `invalid_text`.
  Drivers never report fake success.

## Held state and the cooperative desktop assumption

`Input` tracks only what IT pressed. Consequences:

- `release_all` releases session-owned items only; it never touches keys or
  buttons pressed by other processes or by a physical user.
- Explicit low-level `key`/`button` release calls for items the session did
  not press remain the caller's responsibility (documented, not prevented).
- There is NO protection against a physical user pressing the same key the
  session holds pressed. Input injection assumes a cooperative desktop:
  automation and any human user must not contend for the same input state
  simultaneously. `release_all` cleans up the session's own tracking — it
  cannot restore keyboard state changed by a concurrent human press.

## Thread model

Devices are thread-bound: construct and use `Input` / the raw driver on the
same worker thread. No `Send`/`Sync` promises are made.

## Tests

```sh
cargo test --manifest-path crates/native-input/Cargo.toml
```

Tests use recording mock drivers and pure platform event specifications.
macOS also constructs CGEvents and reads their fields WITHOUT posting;
these tests never initialize `Platform` or request accessibility access.
The Windows event planner tests are compiled on non-Windows hosts too.
No test injects real desktop input.
