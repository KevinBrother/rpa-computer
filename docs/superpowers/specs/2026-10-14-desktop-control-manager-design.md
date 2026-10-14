# Desktop control manager — approved phase-one scope

Date: 2026-10-14. User approved implementation after discussion.
This supersedes the earlier proposed session-desktop-lease design for remote TLS.

## Contract

Multiple authenticated MCP transports may stay connected. Initialization, tool
listing and status do not reserve the desktop. A central host authority admits
one task at a time at computer_open. Nonowners receive a structured lease_conflict
without disconnecting; no queue or automatic replay. computer_close completes
cleanup, retires the task's execution authority, and only then permits handoff.
Pause retains ownership and the absolute task deadline. A new task observes afresh.
Old connection/session/request identities cannot operate a subsequent owner.
Disconnect, lifetime expiry, stuck native operations and cleanup errors go through
bounded cleanup; unknown or failed cleanup quarantines authority rather than
silently making the desktop available. Existing cancellation generations and
local emergency stop remain effective.

## Architecture

Separate persistent TLS client connections from owned task execution. The remote
host owns one small shared authority state machine (available, acquiring, owned,
cleaning, quarantined) with connection identity and monotonically increasing task
generation. Retain a separate owned stdio execution process per admitted task,
reusing native runtime, OS desktop exclusion, hotkey, feedback and watchdog. Idle
connections do not spawn a desktop executor. An executor's current process-level
OS lock is retained until that executor is safely retired; it also excludes a
second independent host. No two executors may inject concurrently.

The remote MCP front end serves discovery and routes admitted operations to the
single task executor. Protocol initialization must remain correct across fresh
executor instances. Transport ownership is not inferred from caller-provided
session IDs. Bound authenticated clients, pending messages, frame size and all
waits; slow/auth-failed peers cannot stop an owner or exhaust unbounded threads.
No lock is held across slow IO. Cancellation/EOF must bypass ordinary work queues.

No new UI, auto-wait/queue, force takeover, heartbeat renewal, window-level locks,
or VM scheduler. Status is machine-readable through the existing interface; it
must not leak other clients' arguments, clipboard, screenshots or credentials.
Read-only discovery must report unavailable dynamic display data honestly rather
than fabricate a captured desktop. Preserve the full existing computer tool set.

## Safety invariants

- Acquire/reserve atomically before spawning or granting a desktop task.
- Validate local request/open arguments before consuming authority where possible;
  any failed acquisition/open unwinds only its own resources.
- Release authority only after successful cleanup and confirmation of executor
  exit. A forced exit is not proof held keys or pending OS effects were undone.
- Unknown/failed cleanup remains quarantined with explicit diagnostics, even if
  process termination succeeds. Never let guard-drop on handler panic unlock an
  uncertain live executor; manager retains ownership until verified retirement.
- Invalid close, stale session, delayed reply, cancelled acquisition and repeated
  close cannot release or mutate a newer task.
- Abrupt lost connection does not automatically replay input on reconnect.
- At handoff, old executor/hotkey/feedback must no longer operate; no restoring
  application state is promised. New task must take its own observation.

## Verification

CC + GLM executes Windows tests. Test source may use mock backends for transport
isolation but results must not be claimed as actual GUI acceptance. Required:
concurrent initialization/discovery; A open/B conflict; same B connection opens
following A close; stale/foreign requests rejected; idle and nonowner disconnects
cannot affect owner; owner disconnect cleanup; pause ownership; invalid/failed
open/close; repeated task lifecycle; malformed and slow peers; bounded connections;
watchdog and cancellation; cleanup failure/native timeout quarantine; all existing
transport, runtime and clipboard regressions. Use deterministic gated helpers for
failure paths. Freeze old baseline for a genuine RED run of new contracts.

Windows GUI acceptance uses real Claude CLI routed to glm-5.3-flash and owned
fixture only, not real DingTalk contacts. Do not kill existing user host/client
without explicit permission. No worktree/commit/push. Preserve pre-existing
uncommitted clipboard changes. macOS/Linux testing remains pending by user policy.
