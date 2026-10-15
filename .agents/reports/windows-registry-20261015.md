# Windows registry-bound follow-up — October 15, 2026

Status: native test PASS; coordinator verified the raw evidence and independent
read-only evidence review found no blocker. The eighth historical ignored case now has explicit
passing evidence; default ignore annotations remain unchanged.

## Scope

Explicitly execute the remaining ignored runtime contract:
`registry::request_registry_is_bounded_at_1000_without_evicting_retriable_records`.
This is a deterministic runtime test with a mock backend running on Windows,
not another screenshot transcription or real desktop input test.

Assertions preserved: 1000 distinct successful requests; request 1001 rejected
with `resource_limit`; the first request remains queryable with its recorded
`dispatched` outcome. The production 300ms settle delay is unchanged.
No source, test assertions, timing configuration, or original deployment changes.

## Build and execution provenance

Evidence: `.agents/runs/windows-registry-20261015/`.
`source-head.txt` and `source-hashes.json` record the current source before build.
The initial compile-only command incorrectly selected the unavailable Windows GNU
Rust target and failed with `can't find crate for core`; raw `build.jsonl` and
`build.stderr` are preserved. No tests executed during that attempt.
The established xwin/MSVC environment was then used with:

```text
cargo test --release --target x86_64-pc-windows-msvc --test runtime_contract --no-run --locked --offline --message-format=json
```

Compile succeeded (exit 0, 37.86s). `build-msvc.jsonl` identifies the test artifact;
`artifact.json` records its SHA256 and new isolated Windows staging directory.
No Mac/Linux test executable was run. All test execution is delegated to actual
Claude CLI with the existing sonnet route; assistant response tags in
`cc-registry.jsonl` identify `glm-5.3-flash`.

The existing bounded Windows runner receives `request.json` with `--ignored`,
`--exact`, the full test name, `--nocapture`, `--test-threads=1`, and a 900s limit.
It reserves new evidence paths and records native exit, executable hash, output
truncation, timeout and root-process cleanup. It does not claim descendant containment.

## Layered acceptance rule

The project is judged against received tool parameters and its execution contract.
The earlier fixture supplied-text fidelity result was 4/4; screenshot-derived
expected-text correctness was 2/4. The missing backtick and substituted exclamation
were already present in model-generated tool arguments, not introduced by input.
These model-layer failures remain recorded but do not block deterministic tool
acceptance. Unclassified failures must not be automatically attributed to a model.

## Remaining boundaries

Original deployed host is not replaced. No commit, push, worktree, global settings
change or GUI operation is authorized by this follow-up. Prior full-suite results
and later targeted regressions remain distinct artifact-specific evidence, not a
claim that all suites were re-executed against one final binary.

## Native result and coordinator verification

Actual Windows execution on October 15, 2026, 07:25:11–07:30:11 UTC
(15:25:11–15:30:11 Asia/Shanghai), Session0. This is appropriate for this mock
runtime contract; no interactive desktop is required.

```text
running 1 test
test registry::request_registry_is_bounded_at_1000_without_evicting_retriable_records ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 35 filtered out; finished in 300.43s
```

`result/summary.json`: native_exit_code=0, harness_exit_code=0, outcome=success,
timed_out=false, cleanup_state=root_exited. Both output drains completed;
stdout208/208 bytes, stderr0 bytes, neither truncated. Executable SHA256:
`1aacddb12619da267671a235e59940b4ecc74c9f466140b0e2e1f0ada16c5ce8`.
`verification.json` records source drift and executable/config/runner hash checks.
`final-inventory.json` records a read-only process check restricted to this owned
staging root; binaries and evidence are retained, original deployment untouched.

CLI transcript ended success/is_error=false, actual assistant tags glm-5.3-flash.
Its final prose incorrectly identified the runner summary as native test stdout,
and claimed no other commands despite an earlier read of its automatic memory.
These reporting discrepancies are preserved; the directly collected native
stdout/summary and one test-launch tool call establish the result, not CLI prose.
There was no test retry and no test-source change.

## Combined scope conclusion

Historical control-manager run: 480 passed, 0 failed, 8 ignored across13 suites.
The October14 follow-up explicitly passed the7 environment-sensitive library
cases; this October15 run explicitly passes the remaining registry-bound case.
Thus all8 historical skipped cases now have appropriate passing evidence across
separate runs. This does not turn their original skipped outcomes into passes,
nor claim a single final-artifact full-suite rerun. Later clipboard-targeted
regressions and real input/lifecycle evidence retain their own recorded scope.

## Independent review

Read-only reviewer Aquinas found no blocker for the scoped closeout. It verified
all128 source-manifest entries against current files and recorded HEAD, unchanged
registry assertions against both current and historical HEAD, artifact/config/
runner hashes, exact test name/result, actual response-model tags, and the single
test launch. It confirmed the CLI prose discrepancies above. It did not execute
tests or independently reproduce the build; prior7-case raw bundles were not
re-audited in this review, only their existing reports. No claim of universal
Windows coverage or final-artifact all-suite rerun is made.
