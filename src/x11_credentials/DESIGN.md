# Shared X11 credential-only helper — review checkpoint, 2026-10-15

> Checkpoint update: pure implementation and the next production-connected launcher/tracker RED scaffold are now authored, not executed. See `CHECKPOINT.md`. The original status/evidence statements below describe the pre-takeover checkpoint; launcher/containment requirements remain unchanged.

Status: **follow-up API/auth/IPC/display assertion-RED scaffold only.** `acquire`, the parent
parser, and IPC state transitions explicitly return `Unimplemented`. No process,
file, or socket is opened. No consumer has been migrated. This is not bounded
helper implementation or acceptance evidence. Hotkey's separate frozen RED is
unchanged. The original 19-test checkpoint is preserved in coordinator archive
`review-gates-1` (SHA256
`92998418cfd1cebd5d38659f01216e6e19d151de8212846213983c7d224e4d7a`);
its independent CC run is not evidence for the new follow-up assertions.
Root ownership currently covers this directory and its module line
in `src/lib.rs`, not lock/clipboard/native consumers.

## API and dependency direction

`crate::x11_credentials::acquire(Request<'_>, deadline: Instant)` returns
`Result<Acquired, Failure>`; the module is `pub(crate)` under Linux/test cfg.

- Request has an optional authority pathname, bounded decimal local display
  number, and hostname bytes. Parent-side callers snapshot inherited
  XAUTHORITY/HOME precedence; this module does not modify/read global environment
  or invent a fallback path. All pathname I/O belongs to the child.
- Parent validation bounds pathname bytes to 4095 (plus preallocated NUL), hostname
  bytes to 255, and display input to 64 ASCII digits. Parse display numbers with
  checked nonnegative `c_int` arithmetic (0..c_int::MAX), not u16. Accept bounded
  leading zeros, canonicalize in the parent, and reject signs, whitespace,
  non-ASCII digits, overflow or overlong input. Local DISPLAY fixtures above
  100000 remain supported. This is an implementation/resource bound, not a
  claim that the X11 local display namespace is 16-bit. Validate without I/O.
- The absolute deadline is supplied by the consumer before credential acquisition
  and is never restarted. Success requires complete IPC, parent parsing, and
  observed/reaped helper completion before that deadline. Final acceptance needs
  a deadline check after parsing and the same irreversible timeout gate.
- An absent pathname returns `cookie=None, cleanup=NotStarted`; it is not a claim
  that the server will accept authentication or that the caller owns a desktop.
- A cookie is exactly 16 binary bytes; its Debug is redacted. Failures contain
  typed categories and cleanup state, never paths, file contents, or cookies.
- Cleanup is NotStarted, Reaped, Pending, or Unknown. Pending/Unknown represent
  retained process ownership, not successful termination. Timeout returns no
  usable credential, regardless of bytes queued or arriving later.
- Future hotkey/lock/clipboard consumers call this root module. It must never
  import MCP. It neither depends on nor calls XCB/Xlib, opens X11 connections,
  receives cancellation Arcs, or changes desktop/session ownership policy.
- This does not contain native Xlib constructors. Their independent
  unknown-cleanup containment contract remains intact.

## Parent authentication policy

Move/copy the existing small pure policy into this dependency-independent module
at GREEN, without importing `mcp::hotkey` or changing the frozen source now:
FamilyLocal requires matching hostname; FamilyWild ignores hostname; match the
normalized display number and MIT-MAGIC-COOKIE-1; choose the first matching
record; matching cookie must be exactly 16 bytes. Validate the entire authority
stream, including malformed data after a matching record. No match is `None`,
not an implicit authentication/desktop grant. Embedded NUL cookie bytes remain
unchanged. Only the parent parses Xauthority.

## Private IPC contract

Anonymous private socketpair; no named sockets, standard I/O, command-line
credentials, environment additions, or credential logging. Maximum file payload
is 1 MiB. Preallocate bounded storage before launching; reject declared overflow
before payload allocation, including integer overflow.

Header (12 bytes): magic `XACR`; version byte `1`; status byte; two reserved zero
bytes; payload length as unsigned 32-bit big-endian. Status 0 carries raw file
bytes; 1 missing file; 2 read failure; 3 nonregular file; 4 oversized file. Statuses
1..4 require zero payload. Unknown status/version/reserved bits, trailing bytes,
multiple frames, or truncated EOF fail. A zero-length status-0 payload is an empty
file, distinct from missing file. IPC errors never emit private payloads.

`ReplyGate::ingest` accepts bounded fragments before the deadline;
`expire` irreversibly rejects subsequent input;
`finish` is called only on EOF and rejects late/incomplete input. Timeout erases
buffered contents and never accepts late bytes even with stale timestamps.
ReplyGate success is necessary but not sufficient for public acquisition success:
child reaping, parsing, and final deadline acceptance remain parent obligations.

## Approved launcher direction / next implementation checkpoint

Use fork-like raw Linux clone3 with CLONE_PIDFD, no
CLONE_VM/CLONE_FILES/CLONE_THREAD/VFORK, and SIGCHLD exit notification. On a
**definitive failure with no child created** (ENOSYS, EPERM or EACCES), try exactly
one identity-safe raw legacy clone(CLONE_PIDFD | SIGCHLD). Legacy clone stores
its pidfd in the parent_tid output; do not combine CLONE_PARENT_SETTID. Both paths
use the same reviewed raw child leaf, original absolute deadline, preallocated
arguments/buffers and already reserved tracker slot. Check deadline again before
fallback. No second fallback, no retry on EAGAIN/ENOMEM/EINVAL/resource errors,
no flag stripping or container profile/capability changes.

Do not call libc fork (inherited atfork handlers), libc's callback clone wrapper,
Command/exec on the timed path, or fork then pidfd_open. Raw legacy argument
ordering requires explicit x86-64/ARM64 review; unsupported architectures return
an explicit launcher error, not an unreviewed ABI guess. If both raw launch paths
are denied/unavailable, report that bounded failure without changing the host.

Initialize pidfd output invalid. Never accept a returned child PID without a
valid atomic owned pidfd, and never signal a bare reusable PID. A successful
return with a missing/invalid handle is not a definitive no-child failure and
must NOT trigger another spawn. Retain cleanup uncertainty in the reserved slot.
Proposed private startup gate below contains this anomalous case without letting
an unverified child begin credential reads. Exact syscall/handle validation and
native supported-environment coverage remain review/acceptance gates.

Before the raw launch, the parent must reserve a tracker slot, prepare all bounded
arguments/NUL-terminated path/storage, and block catchable signals in the calling
thread for the launch boundary. Restore the parent's original mask on every
parent path. Child keeps signals blocked; SIGKILL remains usable. No inherited
signal handler may run arbitrary application/library code in the child.

Child path must be a separately audited, non-unwinding leaf with only reviewed
async-signal-safe/raw syscall operations and initialized preallocated buffers.
No allocator, destructors, formatting, environment parsing, libc atfork hooks,
Rust panic paths, standard logging, or general library callbacks. No helper
executable/loader or dynamic symbol lookup in that path.

Child sequence:

1. Set PR_SET_PDEATHSIG to SIGKILL; immediately compare current parent PID with the
   precomputed parent PID. Mismatch/error exits without reading credentials.
   Review creator-thread lifetime semantics too: this mechanism is additional
   containment, not a blanket proof that a helper disappeared with a host.
2. Relocate only the child IPC endpoint to reserved fd 3. Close all inherited fds
   0..2 and 4..UINT_MAX using close_range with flags=0, not CLOEXEC. This includes
   inherited X transports, authority descriptors, parent IPC endpoint, logging
   pipes and pidfds. A close/setup failure exits before pathname operations.
   Do not enumerate /proc or guess an upper descriptor bound.
3. Proposed private startup gate: after FD containment, wait on fd 3 for one
   fixed startup byte. Parent sends it only after validating its atomic pidfd and
   checking the original deadline. EOF, malformed input or parent death exits
   without opening the file. Timeout closes the gate, including when handle
   validation fails. This is an internal launch handshake, not desktop authority.
4. Open only the precomputed credential pathname read-only; verify regular-file
   type and cap; read into the preallocated bounded buffer. Blocked open/stat/read
   is contained here. Size checks must cover growth after stat as well.
5. Send the single bounded frame on fd 3 and raw-exit. No X connection/input code
   exists in this child. Any failure never falls through to host/application code.

Parent polls IPC/process state with the original remaining deadline. At timeout,
first latch failure and revoke/close IPC, then request kill through the owned
pidfd and transfer the child to the tracker. Never wait synchronously for an
uninterruptible child. A full frame already buffered is not a successful timeout.

## Process-global resource ownership, not desktop authority

One bounded-capacity process tracker (proposed maximum four slots, including
starting/running/quarantined helpers) owns PID + pidfd + internal generation,
cleanup outcome, and timing metadata. **It owns no credentials**, X connections,
DISPLAY/session grants, or permission cache. A slot is reserved before launching;
capacity exhaustion rejects new attempts, never bypasses containment or spawns
unlimited helpers/threads.

Use one explicitly owned process-lifetime reaper, not a thread per attempt. Only
bounded nonblocking wait observations (WNOHANG / pidfd equivalent) and no blocking
wait/join on the caller path. Retain Pending/Unknown slots until actual completion
is established. Never signal a bare reusable PID; stale identities must not
release a slot or target a new process. Account for competing SIGCHLD handlers /
external reapers: ECHILD alone is not successful cleanup proof. No process-global
signal-handler replacement and no generic wait(-1) stealing unrelated children.

The strongest claim is **no X mutations are possible from a timed-out
credential-only child**, not guaranteed immediate kernel disappearance. Unknown
kill/reap outcomes remain owned and visible; bounded capacity prevents buildup.
This tracker conveys no implicit desktop authorization to any consumer.

## Tests / evidence split

Original `review-gates-1`: 5 API + 4 auth + 10 IPC = **19 tests**, unchanged in
that archive. This follow-up: 6 API + 5 auth + 10 IPC + 5 display = **26 tests**.
The previously invalid 65536 input is now explicitly supported; the rejection
case is checked c_int overflow instead. Existing hotkey u16 parsing remains
frozen and must be corrected in its own later gate, not silently edited here. All
exercise the declared production boundaries. Unimplemented errors are intended
assertion RED, not missing symbols or compiler errors. They perform no real file,
process, socket, X server, or desktop work. One redaction assertion may already
pass; no execution result is claimed here.

Detailed next-stage test names, fixtures, expected assertions, and evidence
boundaries are in LAUNCHER_TEST_PLAN.md. That plan is not executable evidence.
Next tests-first checkpoint (not implemented or claimed by this revision):

- Child setup plan/raw adapter: no atfork callbacks, blocked inherited handlers,
  PDEATHSIG parent-death race, failure before reads if fd closure/setup fails.
- Real Linux fixture FD containment: high/low descriptor sentinels, inherited X-like
  socketpair/authority/log pipes all closed, only private endpoint retained before
  blocked acquisition; no X server/input needed.
- Controlled blocking read/helper fixture: bounded acquire timeout, late-frame
  rejection, parent IPC closure, kill request, retained unknown/uninterruptible
  outcome, independent eventual reaping evidence, no caller join.
- Tracker capacity, no credential storage, stale PID/generation protection,
  competing reap/ECHILD and kill failure, process-lifetime bounded reaper count.
- Startup/partial-frame/child-crash failure paths; final-deadline race after a full
  reply and after parsing; stdout/stderr secrecy.

Pure IPC assertions do NOT establish native FD containment, OS timing, or actual
process termination. No GREEN implementation until independent CC RED and API /
launcher review. Existing frozen hotkey tests remain a separate gate.
