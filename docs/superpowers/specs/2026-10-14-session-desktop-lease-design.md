# Session-scoped desktop control lease

Date: 2026-10-14
Status: Historical proposal, superseded for remote TLS by
`2026-10-14-desktop-control-manager-design.md` after the approved scope discussion.

## Goal and scope

Allow multiple MCP clients to remain connected to the same desktop without an
idle connection reserving input authority. Reserve the desktop for a complete
`computer_open` → `computer_close` session, never for individual actions.
Preserve the pending explicit Windows clipboard-paste changes and their tests.
No commits, pushes, worktrees, global configuration edits, or interruption of
existing user clients are authorized by this design.

## Current evidence

- `src/bin/computer-host.rs::serve` acquires `DesktopLock` before worker startup
  and retains it for the stdio process lifetime.
- `src/mcp/lock.rs` provides nonblocking cross-process file locking. The stable
  rendezvous file must not be deleted; file presence is not lock ownership.
- `Runtime::open/close/shutdown` already define session lifecycle and cleanup
  results. A failed close leaves the session faulted.
- `src/mcp/worker.rs` already enforces an absolute 600-second session deadline
  from successful open, including when the client sends no further calls.
- Native calls run on a separate thread. A timeout can leave a call alive;
  transport cancellation is not proof that native input has stopped.
- Host emergency hotkey registration currently has process-level lifetime.
- Desktop feedback has separate grant, retirement, and termination semantics;
  a cleanup path must not silently reopen a locally revoked authority.

## Alternatives

1. Retain connection-level exclusivity: simplest, but idle Claude MCP clients
   continue to exclude each other and manual disconnection remains necessary.
2. **Recommended: explicit session-level exclusivity.** Existing open/close
   boundaries define ownership; idle transports coexist, tasks remain isolated.
3. Per-action locking or transparent waiting queues: rejected for this change.
   Action interleaving can redirect text into another task's window. Transparent
   queues can execute stale intent much later and complicate cancellation.

No new queue, force-takeover operation, heartbeat, or idle timeout is introduced.
The existing absolute 600-second cap remains, without request-based renewal.

## User-visible contract

- MCP initialization, tool listing, and `computer_describe` need no desktop
  writer lease. Describe remains read-only and does not take a screenshot or
  grant input authority. Native display/permission initialization can still
  fail independently; multi-client support does not promise headless operation.
- `computer_open` validates arguments/local state, then nonblockingly acquires
  desktop authority before granting the session or performing owner-only work.
- Contention returns structured `lease_conflict` from the tool, not process
  termination. The MCP connection remains usable; the caller explicitly retries
  open after the other task closes. No automatic waiting or input replay.
- A successful open retains ownership through observe, step, get-step, pause,
  and resume until safe close or safe forced cleanup.
- Pause stops input and releases held keys as today, but **retains ownership**:
  another task must not invalidate the paused task's desktop context. Use close
  to yield control. Pause does not extend the lifetime deadline.
- Successful close releases authority only after native input and owner-only
  resources are safely cleaned up. A later open uses a fresh session identity.
- Invalid close, another session's ID, unknown requests, and denied open do not
  release another live session's lease or reset its deadline.
- A fresh owner must open and observe; old session IDs and observation IDs are
  never transferable between clients. No claims of restoring window/focus state
  on handoff are made.

## Lease lifecycle and safety

Use a small dedicated authority module instead of embedding more platform
lifecycle logic into the already-large runtime facade. Production authority
uses the existing OS-lock primitive; mock/test authority is explicitly injected,
never a fallback from production acquisition failure. Real-lock contention is
also tested independently of fake backends.

Ownership transitions:

| State/event | Result |
| --- | --- |
| Connected without session | No lease; no emergency hotkey ownership |
| Valid open, OS lock unavailable | lease_conflict; connection stays alive |
| OS lock acquired, open succeeds | Active session owns lease and local stop resources |
| Open fails before authority grant | Unwind only resources acquired by this attempt; release after verified safe cleanup |
| Pause/resume | Same owner; no lease handoff |
| Close succeeds | Retire session authority and disarm owner resources, then unlock |
| Deadline/EOF/cancelled transport shutdown | Cancel, finish bounded native cleanup, then unlock only if safe |
| Native timeout / cleanup failure / uncertain teardown | Quarantine: retain exclusion, refuse new work until safe terminal process exit |

The lease guard must not disappear while a native call can still inject. A
coordinator timeout must not drop a guard held elsewhere merely to return a
response. Panic/unwind paths likewise cannot accidentally hand off an uncertain
native execution; retain a process-lifetime quarantine owner or terminate the
faulted process without an intermediate unlock. Failed cleanup cannot be
reported as a successful close. Use existing shutdown_unknown/quarantine
semantics rather than inventing success from a elapsed timeout.

Explicit ordering on safe release: stop dispatch → finish/release native input
→ settle/retire feedback and disarm the session's emergency-stop registration
→ release OS lock. Teardown failure keeps exclusion. A client without ownership
must never unregister another client's hotkey or cancel its session.

Hotkey arming occurs after obtaining the lease, not during idle connection
startup. Preserve current honest reporting when a platform cannot arm a hotkey;
do not silently downgrade claimed emergency-stop capability. A new owner must
attempt its own registration. Local permanent revocation must still require a
new connection; normal close must permit a fresh open on the same connection.
Separate session feedback retirement from terminal process retirement where
needed; never blindly reuse a permanently revoked feedback controller.

## Implementation boundaries

- Introduce a modular session authority lifecycle abstraction shared by runtime
  and worker wiring, keeping OS primitives out of business state transitions.
- Integrate acquisition/rollback/release with open, close, shutdown and watchdog.
- Move hotkey lifecycle to the active authority; preserve cancellation generation
  checks and abandoned native-thread isolation.
- Remove host startup's process-wide lock acquisition only after production
  worker wiring and quarantine tests cover the replacement.
- Update tool descriptions/README with ownership, contention, pause and close
  semantics. Do not promise automatic queuing or a force takeover.
- Keep clipboard input behavior and its API unchanged by this change.

## Verification gates

1. Before implementation, failing tests for two independent authorities sharing
   one lock path: both connect, A opens, B conflicts without disconnecting,
   A closes, B opens, old A IDs cannot operate B's desktop.
2. Failed/invalid open and invalid close do not leak or incorrectly release
   ownership; repeat close is safe; same connection can open a fresh session.
3. Pause retains ownership, resume preserves it, and the existing deadline is
   not extended by requests or pause/resume.
4. Watchdog releases after verified idle-session cleanup with no next request;
   failed cleanup and blocked native calls retain exclusion. Include bounded
   gated-thread tests and panic/quarantine tests; never use sleep as proof of
   stopped input.
5. Hotkey handoff and feedback retirement/revocation tests cover two sequential
   owners and normal reopen without weakening permanent local stop.
6. Cross-process actual OS-lock tests, plus existing worker/transport suites and
   the clipboard contract suite. Compile applicable Windows/macOS code paths;
   no macOS GUI manipulation is part of acceptance.
7. Windows interactive two-client validation using owned isolated processes and
   fixture only. Claude CLI with verified glm-5.3-flash performs actual desktop
   acceptance. Then complete pending paste cases (Chinese, ASCII/punctuation,
   emoji, multiline/tab) with exact fixture evidence and honest IME reporting.

## Deployment boundary

An already-running old host retains the old connection-level lock. Updating a
separate candidate cannot change that process's behavior. Any old-client
interruption or original-host replacement requires explicit authorization.
Never bypass contention with a different lock path or delete the lock file.
No DingTalk contact receives test messages. Keep failed and incomplete evidence;
unit tests are not proof of successful real-application input.
