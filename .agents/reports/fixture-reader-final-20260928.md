# Fixture reader final report — 2026-09-28

## Task
Replace `PipeHost::read_wire` in `src/mcp/stdio/tests/fixture.rs` with the
specified simple loop design (no redesign), removing the blocking
`fill_buf` preamble. Sole file modified; all tests unchanged.

## Change applied (exact simple design)

`read_wire` is now a single loop with four steps and NO BufRead calls:

1. **Completed-frame check before ANY IO** — `take_completed_frame()`
   drains a frame already buffered in `self.frame` (trailing frames from a
   previous wake, split UTF-8 continuations) and returns `Frame(f)`.
2. **Budget check** — `deadline.saturating_duration_since(now)`; zero →
   `FrameRead::Timeout`.
3. **Arm the socket** — `self.stdout.get_mut().set_read_timeout(Some(remaining))`
   before every blocking read (overrides stale/bare socket timeouts).
4. **One bounded raw read** on `get_mut()`:
   - `Ok(0)` → `FrameRead::Eof`
   - `Ok(n)` → append to `self.frame`, loop
   - `WouldBlock | TimedOut | Interrupted` → loop back to steps 1/2
   - other error → panic

Removed:
- The entire `fill_buf` drain preamble (which blocked before any timeout
  was armed, `continue`d past the deadline check on a timeout wake, and
  never served already-buffered complete frames before a new socket read).
- The now-unused `BufRead` import (`use std::io::{BufReader, Read, Write};`).
- The stale/misleading invariant comments in the module header and on the
  method; replaced with comments matching the new behavior.

The `BufReader<TcpStream>` wrapper is retained (no other reader uses its
buffering; all reads go through `get_mut`), so nothing can be stranded in
it. `take_completed_frame` unchanged — trailing bytes and split codepoints
still handled in `self.frame`.

## Verification

- `rustfmt --edition 2021 src/mcp/stdio/tests/fixture.rs` — clean.
- Grep: no `fill_buf` / `BufRead` / `read_line` / `read_until` usage
  remains in the stdio test code.
- Isolated regression (`cargo test fixture_`, external 20s subprocess
  timeout, captured output, returncode propagated):
  **6/6 passed in 0.52s, returncode 0** —
  `fixture_read_for_id_matches_later_id_and_keeps_earlier_frame`,
  `fixture_read_wire_overrides_stale_long_socket_timeout`,
  `fixture_read_wire_timeout_is_distinct_from_eof`,
  `fixture_partial_frame_reassembled_across_socket_timeout`,
  `fixture_partial_frame_reassembled_with_multibyte_utf8`,
  `fixture_read_wire_trickling_peer_cannot_exceed_budget`.
- Full stdio suite (`cargo test mcp::stdio`, external 40s subprocess
  timeout): **15/15 passed in 0.81s, returncode 0** — all `stdio_*`
  integration tests (EOF cancel, overflow latch, spoofed ids, queued
  cancel, resume refusal) plus the 6 fixture units. No hangs, no flakes.

No test hung; no speculative redesign was needed.
