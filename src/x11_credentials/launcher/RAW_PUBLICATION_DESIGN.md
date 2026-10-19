# Raw result publication: review candidate, NOT implemented — 2026-10-16

Scope: close the known post-clone-return-to-Rust handoff gap WITHOUT treating a
numeric TID as original thread identity, increasing minimum kernel requirements,
or changing the existing actual-creator-death tests. This is not a blanket
recovery claim for every possible instruction interruption.

## Fixed ownership and identities before clone

1. A process-owned operation slot/generation owns resource storage before clone:
   IPC endpoints, prepared path/context/buffer, and TWO fixed attempt records.
   No kernel pidfd output pointer or sole FD owner may be a creator stack local.
   Parent context/storage remains valid while any foreground use is possible;
   child COW context is not allowed to write the parent's record.
2. Each attempt record has distinct preinitialized kernel pidfd output storage
   and result=UNPUBLISHED (a sentinel outside syscall result/negative-errno range).
   Records cannot be overwritten for retries or reused for another generation.
3. There is ONE canonical FD owner in the operation record. Creator and reaper
   may inspect/borrow that identity through synchronized access, not each build
   an OwnedFd from the same raw integer. No concurrent close/take with an active
   borrow; no cleanup under an identity from a reused slot.

## Entered, published, fallback

| State / evidence | Allowed action |
|---|---|
| Prepared, no attempt entered | Original caller may enter attempt1 before deadline; no child claim yet |
| Attempt1 Entered, result unpublished | Reserve all ownership; never infer success from fd>=0 or infer NoChild from fd<0; no fallback |
| Attempt1 published positive result | Establish this child only; prohibit fallback |
| Attempt1 published ENOSYS/EPERM/EACCES | Original caller may explicitly enter the ONE legacy attempt, subject to original deadline/cancellation state |
| Attempt1 other published error | No retry; original caller publishes terminal NoChild |
| Attempt2 Entered, unpublished | Same uncertainty rules; no third attempt |
| Attempt2 published positive/error | Establish that child or terminal NoChild; no retry |

A failed first attempt alone does NOT say the whole operation is NoChild: the
second attempt may still be entered. Parent must publish a terminal no-fallback/
NoChild decision, or publish the result of the last permitted attempt. Entering
a fallback must be serialized with cancellation so a cancelled generation cannot
later start an unaccounted helper. This is an explicit state transition, not a
fresh reservation or a renewed deadline.

## Raw parent publication and FD validity

- After syscall, distinguish zero(child) from nonzero(parent/error) inside the
  trampoline. Child goes straight to raw leaf and NEVER publishes to parent.
- Parent publishes the exact syscall return into its attempt record BEFORE any
  Rust return, test hook, allocation, callback, or restoration of blocked signals.
  Publication uses release ordering (review explicit x86-64 store ordering and
  ARM64 STLR); reaper/parent consumers use acquire loads.
- Positive published return + this attempt's kernel CLONE_PIDFD output establishes
  the provenance needed to initialize its canonical handle. A positive integer
  FD alone does not. Missing/malformed output or validation failure stays owned
  Unknown and never opens the startup gate; no guessed PID signaling.
- Acquire-published negative return establishes NoChild for that attempt only.
  Kernel-output validity/rollback and memory ordering must be reviewed against
  both exact clone ABIs. Reaper must never touch output while publication is absent.
- Creator-vs-reaper handle installation/retirement needs one owner transition,
  not two independently created Arc/OwnedFd wrappers. Parent must still check
  cancellation/generation before IPC admission. Reaper is never a GO sender.

## Actual interruptions versus injected instructions

Existing executable creator exits occur in Rust lifecycle hooks AFTER raw return,
after mask restoration, and after tracker attach. Leave all three hook positions
and the actual killed clone-thread identity intact. The first hook will observe
an already-published result, not be moved after cleanup or replaced by a request-
thread exit. The existing assertions for product reap/FD/slot retirement remain.

Current parent raw rt_sigprocmask blocks the full kernel signal set before clone,
including libc's normally reserved cancellation signal. Successful mask setup is
a prerequisite to spawn. Ordinary catchable-handler execution and signal-driven
pthread asynchronous cancellation are deferred while that mask remains blocked;
restoring it must occur AFTER publication. The assembly interval contains no Rust,
libc cancellation point, callback or SYS_exit instruction. An imagined inserted
SYS_exit between every instruction is not an executable path of that program.

This does NOT make thread death universally impossible. A kernel syscall that has
not returned, a real thread-level seccomp termination, an invalid-memory fault,
or external debugger intervention must be classified according to its actual
point/outcome, not handwaved as normal cancellation. If no result was published,
retain bounded Unknown and do not replay. A process-fatal event is whole-host
termination, not evidence that the product reaper successfully ran. Default
seccomp is unchanged; do not substitute an environment to claim acceptance.

## Deliberately NOT solved by publication

- Published child identity is not proof that the creator/caller is dead. Reaper
  cannot free still-borrowed foreground context merely because child exited.
- ARMED->GO by the original actual clone thread proves admission causality only;
  it is not a general death detector or permission to recover unpublished output.
- A no-GO raw wait must use the original absolute deadline. Current blocking read
  plus post-consumption expiry check alone is insufficient. Separate future work.
- Whole-host death, actual TID reuse, both-ABI/source-to-object review, and all
  existing actual-creator gates remain independent acceptance requirements.

Decision for this checkpoint: implement foreground permits only; do NOT implement
this raw publication protocol until these ownership/transition details are reviewed.
