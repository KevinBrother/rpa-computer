# Raw parent publication: LIMITED GREEN candidate — 2026-10-16

On credentials-admission-red-1. Implements only the approved A–D raw-publication
scope; not all-three creator cleanup GREEN or full helper acceptance. Admission
no-GO product behavior is unchanged and still awaits its own assertion RED.
Previous design document RAW_PUBLICATION_DESIGN.md remains the historical design
checkpoint; this document records which portions are now code and which are not.

## Production ownership / ordering

- Native::prepare constructs one stable Arc<Operation> owning parent IPC, path,
  buffer, Child context and TWO distinct Process records. policy registers both
  canonical Handles in the process Tracker BEFORE blocking signals/entering clone.
  Original Native stays !Send/!Sync; reaper Handles can share the owning record.
- Each Process has separate UnsafeCell<i32> kernel output and aligned64-bit
  AtomicIsize result, initialized to isize::MIN. Kernel writes output once; only
  acquire-positive result readers may access output. Negative return, including
  positive stale output after kernel rollback, NEVER adopts/closes/signals that
  integer. Unpublished output is equally inaccessible to handle/drop paths.
- clone3 and legacy receive their own record pointers. All four raw parent paths
  publish FULL RAX/X0 after branching away from zero(child), before any Rust,
  hook or mask restoration: aligned MOV on x86-64, STLR on ARM64. Consumers use
  Acquire. Child never publishes and goes directly to the unchanged assembly leaf.
  External global-asm calls retain their memory side effects; no nomem/readonly.
- Publication pointer survives syscall in r13/x20. Child pointer stays r12/x19.
  x86 preserves r12/r13 and adjusts stack alignment before child call; ARM64 saves
  x19/x20/x30 with32-byte aligned frame. Legacy argument reordering is retained.
- There is no OwnedFd->Arc installation step after clone. Process IS the owning
  record from initialization through final drop, giving BorrowedFd access only
  after positive publication. All Handle clones reference that same Operation.
  Only Process::drop can close its positive published output, after final borrow
  ownership retires. No second from_raw_fd, duplicate adoption, or Installing gap.

## Cancellation, fallback and unresolved outcome

REGISTERED, attempt-entry, CANCELLED and terminal-no-child share one atomic control
word. Entered and cancellation linearize through CAS/fetch_or on that same word:
if cancel wins, no entry; if entry wins, that attempt stays accounted even if its
actual syscall occurs later. Original caller/Native performs cancellation; reaper
still only attaches/observes, never cancels IPC, starts fallback or sends GO.
Marker rejects an already-cancelled operation before reading/authorizing startup.
This does not introduce a generic cross-thread GO-write cancellation API.

Only the original caller can enter one legacy fallback after acquire-published
ENOSYS/EPERM/EACCES, under existing original-deadline policy. Two output/result
records never mix. The reviewed direct-legacy fixture uses only its legacy record.
Repeated entry, success->fallback, unpublished->fallback, and terminal re-entry
are refused. Cancellation never resets entry bits or budgets.

Only after the final fallback decision can finish_no_child mark all entered
attempts as confirmed negative. Positive/unpublished results cannot take this
transition. Entry failure with an unresolved existing attempt returns the new
internal Spawn::Uncertain, so policy retains Unknown ownership rather than
re-arming its NotStarted destructor. There is no ambiguous replay.

## Reaper handoff and foreground boundary

The same single reaper scans at most8 pre-owned attempt handles (4 operations ×2),
acquires positive published PID, then attach_attempt resolves the canonical Arc
already registered in that generation. Creator and reaper attach are idempotent
only for that SAME stored handle/PID/generation; neither constructs another FD
owner. Reaper then uses existing nonblocking pidfd wait and cached positive reap.
It does not change child permissions or infer original-thread liveness.

Parent Resources are interior-mutable ONLY through original !Send Native. Reaper
never dereferences/clears them. Return/unwind closes resources before foreground
permit retirement. A non-unwinding creator exit leaves its foreground obligation
and leaked Rust references owned/bounded by the unreleased slot. No one claims
those borrows ended. Consequently original FD/slot cleanup assertions may STILL
FAIL even when reaper can now record the child correctly. Known open gap, not a
reason to drop/relocate the existing tests or synthesize recovery.

Tracker remains metadata-only; it holds opaque Handles, not a credential cache.
Operation temporarily owns the same bounded parent buffers. No per-attempt thread,
extra helper, new pathname I/O, kernel-version prerequisite, or consumer change.

## Supplemental tests: eight, production-connected but not actual clone evidence

`launcher::native::linux::publication_tests` constructs the actual Native prepared
Operation and real Tracker registration, writes controlled return/output values
into the actual production records, then exercises real methods and Unix FDs:

1. Both negative clone routes with positive stale output never adopt/close it.
2. Unpublished output -1/positive neither adopts, falls back nor becomes NoChild.
3. Cancellation-before-fallback prevents entry and startup marker admission.
4. Entry-before-cancel stays owned and cannot admit/replay.
5. Explicit final-no-fallback cannot be reopened.
6. Concurrent creator/reaper attach resolves one canonical owner; real FD stays
   open with live borrowers and closes at final ownership retirement.
7. Positive result/missing output stays Unknown, no admission/fallback/reuse.
8. Real two-thread competition uses the same production CAS cancellation word.

These are supplemental record/FD regression tests, not a previously observed RED
set, not a standalone invented model and not evidence that either kernel clone
route executed. Original real creator-death tests provide the kernel path; their
full cleanup assertions remain byte-identical and can remain partial RED.

## Independent validation commands (separate logs/status)

```sh
cargo-1.89 test --locked --offline --lib --no-run

timeout 120 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::native::linux::publication_tests \
  -- --nocapture --test-threads=1

# Keep all THREE original assertions intact; inspect actual_product_reaps,
# tracked_child, original FD states and slots BEFORE fixture cleanup. Do not
# count expected remaining foreground failure as a fully recovered operation.
timeout 120 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::lifecycle_native_tests::creator_death_after_clone_keeps_recoverable_ownership \
  -- --exact --ignored --nocapture --test-threads=1
timeout 120 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::lifecycle_native_tests::creator_death_after_mask_restore_keeps_recoverable_ownership \
  -- --exact --ignored --nocapture --test-threads=1
timeout 120 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::lifecycle_native_tests::creator_death_after_attach_retires_parent_resources \
  -- --exact --ignored --nocapture --test-threads=1
```

Also rerun unchanged original26 pure /12 policy /9 tracker /9 native, permit6
tracker+normal-reuse1, original admission-cap case and both expiry cases, using
PERMIT_GREEN.md / EXPIRY_GREEN.md. New no-GO admission tests remain separate per
ADMISSION_RED.md; unqualified actual TID reuse is never PASS/behavior RED.

Fresh integrated source build, unchanged default seccomp and source/object review
for BOTH ABIs/clone routes. Original high-FD fixtures still require per-container
nofile8192:8192. Do not swap environment to make clone3 pass; report actual route.

## Compilation and artifacts (NOT execution)

macOS root all-targets; actual-source Linux helper --tests; production+cfg(test)
assembly/object emission on x86-64 and ARM64 all exit0. cfg(test) library emission
has expected unused test-import warnings; this is not a test run. No full Linux
root build claim. Logs credentials-publication-{check,asm,asm-tests}-*.log.
Raw emitted slices: credentials-publication-green-2-raw-{x86_64,aarch64}-{hooks,
production}.s, under .agents/runs/linux-parity-20261015. Local inspection shows
MOV/STLR precedes parent return on both routes and child branches avoid it; these
are not independent object/runtime approval.

All21 prior test-source files are byte-identical to admission-red-1. Leaf .S,
leaf.rs, deadline.rs and transport.rs are also unchanged. No no-GO behavior fix,
new stable thread witness, general caller-death reclamation, whole-host-death
acceptance, tests/SSH/GUI/commit, or consumer migration is claimed/performed here.

Final handoff is publication-green-2. Local green-1 archive is preserved but
superseded before handoff: final Drop closes the exclusively owned raw integer
without constructing a temporary BorrowedFd, and Native verifies published return
exactly matches its raw return before any outcome classification. No tests ran
on either snapshot.
