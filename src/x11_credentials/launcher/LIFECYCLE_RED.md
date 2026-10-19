# Lifecycle regression RED checkpoint — 2026-10-16

**Tests and necessary production observation seams only. No lifecycle fix and
no consumer migration.** Keep all previous acceptance requirements and archives.

## Review checked against source and real evidence

Read `2026-10-16-linux-credentials-launcher-green-review.md` and
`2026-10-16-linux-credentials-launcher-green-1.md`, plus the final per-group/native
logs. The latter establish original pure26/policy12/tracker9/native8+dispatcher
PASS and actual Auto Legacy/clone3 ENOSYS on the preceding immutable candidate.
They do not establish lifecycle containment or apply to these new tests/seams.

All four review findings are confirmed against the preceding real source:

1. **Creator handoff loss:** `Native::spawn` writes the atomic pidfd to a stack
   local; `restore_signals` and `Ownership::attach` come later. Starting slots
   have no identity and reaper snapshots omit them. A non-unwinding creator exit
   can therefore lose recoverable child/IPC ownership even after child death.
2. **TID reuse:** leaf prctl/getppid/tgkill(0) checks only numeric liveness, not
   the original thread lifetime. This is a death/ownership hole, NOT evidence of
   credential access or a bare-PID kill vulnerability.
3. **Startup expiry:** `allow` checks around the byte write, but the raw child
   does not check freshness when consuming it. Late-success rejection already
   exists in the parent. A late path operation is a separate regression target.
4. **Resource admission:** the reaper can free a slot after positive wait while
   the foreground Native/reply still retain resources. Four live-child slots
   alone do not enforce a four-outstanding-resource-set bound.

No fixes to those decisions are included here. `tracker.rs`, `raw.rs`, original
nine test-source files, original CHECKPOINT/DESIGN/NATIVE_VALIDATION documents,
pure implementation and every consumer remain unchanged.

## New tests and the production seams they exercise

### Portable identity-policy counterexample (2 tests)

`lifecycle_policy_tests` calls `creator::admitted`, which evaluates the exact
`creator::ADMISSION` truth table embedded into BOTH actual raw child leaves.
Current production has numeric group+TID evidence but no lifetime witness, so
it consumes index3. The table retains old behavior (index3 allowed), intentionally
failing the replacement-TID/no-original-witness assertion. The second test keeps
numeric mismatches fail-closed. A live-original positive assertion is retained.
This is a deterministic identity model, **not a real recycled-TID observation**.
Changing only the model or rejecting all legitimate creators is not a valid fix.

### Real production calls (6 ignored scenarios +1 dispatcher)

Each scenario owns a disposable host subprocess; it calls the SAME production
`policy::run`, `Native` raw launcher, tracker, reaper and transport. No parallel
launcher or scripted success result. Existing eight native tests are untouched.

- `creator_death_after_clone_keeps_recoverable_ownership`
- `creator_death_after_mask_restore_keeps_recoverable_ownership`
- `creator_death_after_attach_retires_parent_resources`

Parent hooks stop immediately after kernel return/before Handle construction,
after exact mask restoration/before attachment, and after actual attachment.
A raw child barrier proves prctl and FD containment happened before the fixture
requests ONLY the creator thread's raw SYS_exit, with no Rust unwinding. Another
host thread stays alive. The fixture duplicates the actual kernel-returned
pidfd, observes thread disappearance and child exit independently, and captures
product positive-wait counts, tracker view and exact original descriptor state
BEFORE fixture cleanup. Under ordinary isolated completion these must recover
and retire; pidfd readability alone does not count as product reap. An uncertain
child in other failure cases must still retain a recoverable owned identity.

The disposable host owns intentionally bypassed harness thread/TLS cleanup; no
claim is made that raw-exited Rust harness stacks unwind. Product descriptor
assertions use their exact published FDs, not a vague process-wide memory delta.
Only owned observer pidfds are used for final fixture kill/wait. No generic
wait(-1), PID kill, signal-handler replacement or consumer operation is added.

- `expired_permission_is_rejected_at_child_consumption`
- `permission_write_race_cannot_authorize_expired_startup`

Both park the child BEFORE permission consumption and parent AFTER successful
permission write but BEFORE its post-write deadline check. The second also parks
the parent after its pre-write check. Advance the real original800ms deadline,
release the child first, and record a raw marker immediately before openat plus
a second marker after the actual syscall returns. Require no path operation,
and separately retain parent Timeout/no-cookie behavior. A future pre-write
expiry rejection is accepted without demanding a write that should be denied.
No test can pass merely because the child never resumed: exit or actual syscall
return must be observed before releasing parent cancellation, otherwise INFRA.
Authorization freshness is the required boundary, not an impossible promise
that the scheduler cannot suspend between a final check and the next instruction.

- `reaped_children_do_not_bypass_foreground_admission_cap`

Hold four real calls after complete receive, before parsing/return. Observe each
positive PRODUCT waitid (not mocked tracker evidence), keep foreground replies
parked, and make a fifth production acquire. It must reject at admission with
CapacityExhausted. Socket states and original tracker occupancy are diagnostic;
early IPC closure is not forbidden. Caller release/cleanup precedes assertion.

### Instrumentation boundaries

`native/lifecycle_layout.rs` defines the atomic shared observation ABI;
`native/lifecycle.rs` is cfg(test)-only parent control. The child only uses raw
assembly stores, acquire/release loads and shared futex barriers—no Rust callback,
allocator, formatting, TLS or destructor. Both assembly probe blocks are removed
by `.if cfg!(test)` for non-test builds, and the context pointer is null there.
Existing native fixtures also use null pointers: their behavior is not replaced.
Pre-prctl and post-prctl raw barriers are available for the next death/reuse gate;
this checkpoint's creator cases deliberately use post-containment/pre-permission
holds to isolate handoff loss from the separate pre-prctl identity counterexample.
Parent observer hooks add no recovery/expiry/admission policy. Positive-reap
instrumentation increments ONLY after actual nonzero-si_pid product waitid.

## Independent CC commands — DO NOT run in implementation worker

Use a fresh manifest-verified Linux snapshot and actual rebuild, retaining both
.S files. Preserve the earlier seeded-Cargo-artifact freshness guard. No extra
capabilities, seccomp relaxation, GUI, X server, host input or global settings.
Keep the original highFD resource prerequisite **per-container
`--ulimit nofile=8192:8192`**, record measured limits, do not modify product limits.

```sh
cargo-1.89 test --locked --offline --lib --no-run
# New deterministic model: inventory2, expected one assertion RED + one baseline.
timeout 120 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::lifecycle_policy_tests -- --nocapture --test-threads=1
# New real lifecycle cases: inventory6 +1 dispatcher, expected six assertion RED.
timeout 120 cargo-1.89 test --locked --offline --lib \
  x11_credentials::launcher::lifecycle_native_tests -- --ignored --nocapture --test-threads=1
```

Expected inventories/failure intentions are NOT execution evidence. Retain each
exit code/log separately. `LIFECYCLE_FIXTURE_INFRA` (barrier/setup/unsupported path,
missing prerequisites or outer10s host timeout) is not behavioral assertion RED.
`LIFECYCLE_ASSERTION_RED` identifies product observations, not a skip. Model RED
must be the lifetime-witness assertion. Existing groups should also be rerun
separately: api_tests, auth_tests, ipc_tests, display_tests, launcher::policy_tests,
launcher::tracker_tests, and launcher::native_tests (the latter --ignored).

## Explicit next mandatory gates — no contract reduction

Real same-thread-group TID reuse still requires an observed recycled TID under a
bounded churn/time budget and explicit pre-prctl ordering. The deterministic
model is only the FIRST RED checkpoint; no real-TID-reuse test or PASS is claimed.
Failure to obtain real reuse is an unmet prerequisite, never a pass. Do not write
global PID limits, loosen seccomp, or fabricate numeric identity evidence. Whole-
host death pre/post-prctl/startup/read remains mandatory as well; creator-only
cases do not substitute for it. Add those native cases before claiming a repair.

Keep all other gates from the review: each setup/close_range failure before path
work, post-stat growth/native frame faults, denied-launch masks, competing reapers
and SIGCHLD auto-reap, real Pending/Unknown cap/FD/process/thread accounting, both
ABIs/routes. This checkpoint only authors RED; wait for independent assertion RED
before implementing lifecycle ownership, lifetime witness, child freshness or
foreground admission repairs. No D-state/immediate-disappearance claim.
