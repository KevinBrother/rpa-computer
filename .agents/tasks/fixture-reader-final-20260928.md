# Sole narrow implementation: remove the blocking fill_buf preamble
Previous writer93894 terminal; no other source writer. Only modify src/mcp/stdio/tests/fixture.rs and your .agents/reports/fixture-reader-final-20260928.md. Keep all tests unchanged. No other files/config/model/GUI/commit/push.
Current read_wire is plainly broken: FIRST calls fill_buf, which BLOCKS before any timeout is armed; on timeout CONTINUE prevents ever reaching deadline check; buffered complete frames aren't checked before new socket read. Do not research; replace this whole method with the following SIMPLE DESIGN (implement Rust yourself):
loop:
  1 if take_completed_frame() returns frame, return Frame(frame) (before ANY IO)
  2 compute deadline.remaining; if zero return Timeout
  3 self.stdout.get_mut().set_read_timeout(Some(remaining))
  4 self.stdout.get_mut().read(&mut [u8;8192])
     0 -> Eof
     n -> append bytes to self.frame then loop
     WouldBlock/TimedOut/Interrupted -> loop to 1/2
     other error -> panic
NEVER call fill_buf/read_line/read_until. This fixture has NO other reader using BufReader buffering. It is fine to retain BufReader wrapper just use get_mut consistently; remove unused BufRead import. self.frame retains trailing frames and split UTF8; existing take_completed_frame already supports trailing data. Remove misleading old comments. That's all.
Run isolated regression filter fixture_ (external subprocess timeout20seconds, capture_output print result; propagate returncode) then full stdio tests timeout40seconds. If test hangs stop and report, don't do another speculative redesign. rustfmt --edition2021 src/mcp/stdio/tests/fixture.rs only. Report before turnlimit. 8-12 toolcalls maximum. No /tmp/nothing or other scratch outside repo.
