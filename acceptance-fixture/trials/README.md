# Windows first-trial accounting

A read-only, standard-library diagnostic planner/counter. This is **not** a GUI
scheduler, a trace verifier, or proof of ten actual successful operations.
`API.md` is the immutable Stage A contract; its opening stub status describes that
historical stage. Stage B implements the same schema without changing that document.

## Scope and data provenance

- Fixed preplan: pointer×4, multiclick×1, drag×1, scroll×2, keyboard×3, known-input×1.
  12 run groups / 120 slots / 60 semantic cases. All slots stay in the denominator.
- Eligible 95: move12/click25/drag10/scroll14/text_input10/key_chord12/key_hold12.
  The other 25 are noncounting coverage/blocked/rejected/zero slots.
- Quota10 applies to each of seven action kinds, not every parameter combination.
  All 60 case/variant rows remain separately reported.
- Plan IDs are stable UUIDv5 identifiers, not runtime run/session/nonce identities.
  No runtime bindings are invented; every new slot remains needs_preflight/null.
- `canonical-fixture.json` is coordinator-only test input, not a production input.
  The production modules do not load it or import tests.
- `_core/catalog.py` independently declares reviewed semantics and pins nine fixed
  task/catalog source files by SHA256. Builds/summaries check raw bytes each time.
  Drift is a hard error requiring re-review/versioning, not a new silent digest.
  Preserve file bytes when staging Windows source; line-ending conversion is drift.
- API v1 intentionally retains exactly nine source references. The new
  `tasks/windows-known-input.md` is **separately frozen** with Stage B; before giving
  it to the GUI agent the supervisor must verify its SHA against the StageB manifest.
  API v1 does not assert that task's staging/ownership was checked.

## APIs and modules

Load `acceptance-fixture/trials/__init__.py`, then call
`build_manifest(campaign_id, mode="glm")` / `summarize(manifest, records)`.
The facade supports the original tests' `spec_from_file_location` + `exec_module`
style, even when the caller does not insert that facade into `sys.modules`.
Only its location-namespaced private `_core` package is registered for relative
imports; no `sys.path` modifications or test-data fallback.

Responsibilities: `catalog.py` source pins/semantics; `plan.py` IDs/canonical
construction; `bounds.py` strict bounded JSON/file reads; `records.py` validation
and all-party conflict detection; `summary.py` counters. APIs do not mutate inputs.
No evidence path is opened, statted or hashed. Such paths/IDs/hashes are claims only.
Runtime paths to the nine trusted source files are fixed, never taken from a caller.

## CLI (Python 3.12 on Windows)

From a complete staged source tree (keep the nine referenced files at their paths):

```powershell
$python = 'C:\Users\Administrator\AppData\Local\Programs\Python\Python312\python.exe'
& $python -B scripts\windows-first-trials.py --help
& $python -B scripts\windows-first-trials.py plan --campaign-id c4105120-573e-4a35-8dd5-602d3fa12000 --mode glm
& $python -B scripts\windows-first-trials.py summary --manifest C:\NEW_RUN\plan.json --records C:\NEW_RUN\records.json
```

`plan --mode deterministic` creates a separate digest with required_model=null;
records from GLM and deterministic modes cannot be mixed. `summary` takes a manifest
object file and a JSON **array** of records; an empty records file contains `[]`.
The paths above are placeholders for supervisor-owned new evidence files. There is
no output-file, overwrite, repair, cleanup, GUI, network or process-launch option.
stdout contains one JSON object plus newline; invalid input emits escaped JSON to
stderr and returns exit2 (argparse usage errors also exit2). Unexpected import/runtime
errors remain errors, not synthetic success. Successful accounting exits0 even with
all slots missing: exit0 is **not** a GUI pass or evidence verification.

stdout uses `ensure_ascii=True`, including Chinese, emoji surrogate-pair escapes,
NBSP, combining accents, CRLF and Tab. The ASCII wire JSON round-trips to the exact
Unicode payloads independent of Windows codepage. JSON byte budgets use that same
canonical ASCII representation. UTF-8 inputs required; malformed/truncated JSON,
duplicate keys at any level, floats/nonfinite numbers, invalid surrogates, cycles
(API), wrong types, unknown fields and oversized data are rejected, never salvaged.
Only bounded regular nonsymlink files are read, not stdin/devices/pipes/directories.

Public limits: manifest1MiB, records8MiB; <=1200 records, attempt1..10; depth8;
strings4096 code points, ASCII IDs128; evidence paths1024; <=16 evidence refs and
<=16 target calls per record. 120 first records plus nine retries per slot fit the
record-count limit; evidence-heavy inputs must also fit the byte budget.

## Counting and interpretation

Attempt1 is the only first. Later matched retries cannot repair a failed or missing
first; exact duplicates count once. Same-ID/different-content and same-slot/attempt
conflicts exclude all involved slots, independent of submission order. Runtime
identity reuse or inconsistent bindings and cross-slot tool/request reuse conflict.
Observation ID sharing alone is not input replay. Complete synthetic submissions
can meet `candidate_count_met`, but `gui_verified` and `action_trial_gate_satisfied`
are always false. `evidence_status=supplied_claims_only` and explicit outstanding
raw-evidence/model-policy/screenshots-and-window-ownership review remain mandatory.
Each summary checks the supplied batch, not a persistent cross-report evidence ledger.

## Validation handoff — CC + GLM only

The author performs **py_compile only**, with cache in a fresh /tmp directory; neither
API nor tests are executed locally. The original36/API/canonical fixture stay unchanged.
The coordinator has confirmed native Windows StageA RED (36 methods, 120 failures,
zero errors). That does not certify this implementation. CC owns independent final
review and Windows GREEN, using frozen `scripts/windows-test-runner.ps1` and its
`windows-test-runner/Runner.cs` sibling; each run needs a fresh evidence leaf and the
five-field runner config. Precise commands/configs are in the StageB sol report.

Planned native commands (not run by the author):

```powershell
& $python -B tests\windows_first_trial_accounting.py -v
& $python -B tests\windows_first_trial_accounting_stage_b.py -v
```

The new suite uses owned temporary input files and subprocess timeouts for CLI
requirements, including Windows cp1252 stdout roundtrip. No screenshots/GUI inputs
are produced. All planned120 actual GUI slots and independent evidence review remain
outstanding, irrespective of pure-test or CLI results.
