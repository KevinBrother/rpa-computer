# Continuation review — 2026-09-24

These are coordinator source-review observations while repair workers are active. Re-check final sources before assigning another repair. No completed/full-system acceptance is implied.

## Confirmed backend lifecycle issue (assigned backend-fix-2)
`DesktopBackend.last_capture_geometry` is backend-lifetime sticky, so geometry change followed by close/new open cannot capture ever again. Runtime already binds/checks geometry per session. Separate these lifetimes; preserve before/after capture checks. Backend-fix-2 exclusive writer session93970.

## Invalid coordinator test run
Core-fix-1 transcript records `pkill -f rpa_computer-d9cd46035e242a1f`; it terminated coordinator test PID51871 along with core's test. Coordinator run2387 exited143 and contains failures but no final summary. Never count it green. Agent follow-ups must disallow process-name termination and only allow exact verified owned descendants.

## QA next review risks (recheck after qa-fix-1 exits)
1. `run-blackbox-claude.sh` currently only audits fake argv, not actual init.tools / mcp servers or actual tool calls. It lacks stream-json/verbose and artifact capture, so advertised audit is incomplete. Need actual tool inventory/call validation and explicit failure when no init, extra tools/server, errors, or missing transcript. Use clean --system-prompt rather than append, user settings only, hooks/skills disabled, retain user's model/auth.
2. Sandbox denies `rpa-computer-*` scratch but misses the known source-containing `/tmp/rpa-core-shim` from prior transport work. Do not claim complete source exclusion unless actual source copies are removed or included. Do NOT erase unknown directories. Regex TMPDIR concatenation is missing slash. Source/release quoting for sandbox syntax must be safe.
3. Windows start unconditionally unregisters/replaces TaskName. Stop allows task override and stops/unregisters without checking action/path/principal matches recorded owned task. PID check only same exe is not sufficient for PID reuse by a newer host. Verify creation time/session/commandline; refuse name collisions rather than delete unrelated tasks.
4. Windows start ACL validation appears name/locale based. Use SID allowlist matching exact console owner plus SYSTEM/admin as deliberately documented; enforce before exposing token. Windows deploy builds manifest including MANIFEST.sha256 itself because redirection creates file before find. Exclude it. Don't print auth token on error.
5. release-script tests create a stub under target/release/computer-host if no binary exists; never run concurrently with real release builds or mistake it for a product artifact. Pure tests should stage an isolated source fixture instead.
6. Script static grep assertions prove text policy only, not functional Windows scheduling/cleanup or actual Claude source isolation. Keep evidence labels precise.

## Transport draft review (18:41; recheck after worker finishes)
- `Worker::call` still waits on `reply_rx.recv()` without a deadline, and the in-flight sender lives on the potentially stuck native thread. Coordinator abandonment does NOT unblock this caller. Stdio/TCP main can stay blocked before calling shutdown. Need direct injected-stall test of the PUBLIC call/transport boundary, not only shutdown in isolation.
- `shutdown()` blocking `handle.tx.send(Quit)` is not itself bounded when queue full; the comment claiming blocking send prevents delay is backwards. Give control separate channel/flag or bounded send.
- Coordinator drains bounded rx into unbounded VecDeque `stash` while native busy, defeating queue memory bound. Bound the whole pending population, not only the ingress channel.
- Watchdog internal close response is treated as ordinary completion, then loops on the expired session if failed; stop behavior and failure state must be explicit.
- If native thread is abandoned due timeout, TCP MUST NOT create another worker/session and release writer lock while the old thread can still inject. Quarantine/fatal lifecycle required. Do not turn Unknown into Clean on Drop or later close.
- Current cancel gate rejects every new `computer_resume`/`computer_open` while flag is set, not merely requests queued before cancellation. This makes explicit resume impossible after pause unless a generation/fresh-request distinction is implemented. Do not solve by clearing flag blindly on all queued resumes.
- Fixture tests should cover delayed native return (not never-ending test thread), saturated pending queue, fresh resume vs pre-cancel queued resume, and no reconnect while orphan native operations live.

## CONFIRMED test deadlock — redirect required (18:54)
`mcp::worker::tests::call_queue_is_bounded` gates SlowBackend.capture on `release=false`, then on main test thread performs **synchronous** w.call(computer_describe) in a flood loop; the only release.store(true) is AFTER that loop. First describe queues behind gated observe and blocks forever. This is a dependency-cycle test bug, not mere slow execution. Current hung native test binaries observed:62538(transport),62520(backend whole lib),62886(core whole lib); ancestry verified below before termination. Rewrite with genuine concurrent producers + deterministic gate + bounded deadlines and always-unblock guard. It should expose the real unbounded-stash bug rather than falsely claim boundedness. Do not replace assertion with 'not hanging' and call memory bound proven.

## QA2 in-progress review follow-up (19:14; recheck final)
- run-blackbox watchdog comment promises owned process-group termination but only kills CHILD_PID. Use explicit exec in subshell to ensure CHILD_PID is actual CLI/sandbox executable; ensure MCP/bridge descendants and remote in-flight input stop on EOF. Test with a controlled hanging fake CLI/child tree, no real GUI.
- New run_sandboxed_macos generates two profile files and probes a different one again. Print path currently both get flag1 (same content), interactive gets extra temp-root deny despite opposite comment. Keep a SINGLE actual run profile and use it for probe; do not special-case weaker probe to turn tests green. Production release may be under a safe-named temp dir; do not assume all tmp is source.
- Reject release/source overlap explicitly and source files in release; canonical path + no symlinks alone is not a source-free-content guarantee. Coordinator will stage known safe artifacts but wrapper should fail clearly on wrong directory.
