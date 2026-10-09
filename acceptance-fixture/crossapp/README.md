# Windows crossapp filesystem acceptance pack

Implementation scope: prepare files and diagnose saved artifacts only. No app
activation, process management, input injection, HTTP server or system changes.
Standard-library Python 3.12; browser assets are self-contained HTML/CSS/JS.
`API.md` preserves the original Stage A RED contract as historical context;
Stage B implements those five functions without changing its 49 assertions.

## Layout

- `agent/crossapp-01..06/task.md`: exactly six operator tasks, no machine oracle.
- `assets/`: empty nonce-named Notepad document; 64 nonce-labelled file contents in
  a nonce-bearing Explorer directory; inline offline operation course.
- `results/`: existing empty per-case output parents, no pre-created success file.
- `profiles/<browser nonce>/`, `downloads/<browser nonce>/`: isolated planned
  locations; creating directories does NOT configure or verify a browser.
- `supervisor/`: manifest and private expected-data oracle. Manifest is written
  last; interrupted preparation can leave incomplete owned output, never cleaned
  up automatically. A partial/existing root cannot be reused.

Supervisor CLI commands (not to be run by the GUI operator):

```text
python -B scripts/prepare-windows-crossapp.py --root <NEW_ABSOLUTE_LOCAL_ROOT>
python -B scripts/check-windows-crossapp-artifacts.py --root <PREPARED_ROOT>
```

Prepare returns 0 for successful file creation, 2 for errors. Check returns 0 when
a diagnostic was produced (including mismatch/unknown), 2 for unsafe/unprepared
roots or read errors. Inspect JSON case states; native0 does NOT mean GUI pass.
Both commands report gui_verified=false. Checker writes only stdout/stderr and
never rewrites an artifact. Real GUI work remains a separate authorization gate.

## Data and safety limits

- Reject existing roots (even empty), nonlocal/UNC/device/drive-relative paths,
  traversal, Windows reserved names, ADS, trailing aliases, symlinks/junctions/
  reparse points. On Windows, a read-only GetDriveTypeW query also rejects
  mapped network/unknown drive types. Inspect ancestors, components and leaf via lstat.
- IO rechecks these facts before mkdir/open and after access, compares file/root
  identities, uses exclusive creation, rejects nonregular/multi-link files and
  bounds reads. No overwrite, recursive deletion, process launch or socket API.
- **Not atomic against hostile concurrent path replacement.** Python pathname
  operations on Windows do not provide an ancestor-handle sandbox. The supervisor
  must use a private local root AND parent, deny other writers, and stop if they
  can change concurrently. Best-effort checks are not a TOCTOU security guarantee.
  Do not deploy this to shared/synchronizing/untrusted-writer directories.
- Notepad reads raw bytes, max64KiB; strict UTF-8 (optional BOM), UTF-16 LE/BE with
  BOM. LF, CRLF, mixed newlines, NFD/NFC and trailing spaces remain different.
  No replacement decoding, read_text universal-newline conversion or trimming.
- Explorer reads bounded inventories and exact hashes; unknown files stay intact.
  A copy is not a move, and final file placement is not evidence of drag motion.
- Browser export max256KiB/256 events. Empty form and uncompleted drag initially;
  no prefill or automatic action. Only explicit input produces feedback. Export
  starts disabled; only a future authorized supervisor may confirm the dedicated
  profile/download setting and enable the supervisor-only page checkbox before
  operator handoff. The checkbox/isTrusted/GUI flags are application data, NOT
  independent proof that those external settings or GUI actions are correct.
- Calculator needs three distinct screenshots/observations. Reused native drag
  and focus need their original evidence flow; pack nonce is not fixture nonce.
  Their null `reuse` binding fields and empty `inner_bindings` require explicit
  supervisor linkage for EACH inner case/trial/nonce (not just one global nonce). They do
  not add external-application coverage and never read source through GUI tools.

A minimal HTML operation-course design uses high-contrast cream/teal/amber,
large numbered vertical checkpoints, a real horizontal scroll panel, an empty
form and a large held-drag target. No external fonts, libraries, images, fetch,
analytics, animation dependency or network listener. Browser syntax/UI still needs CC
review and future authorized GUI execution; source preparation is not execution.

## CC-only pure regression (not executed by the author)

```text
python -B tests/windows_crossapp_pack.py -v
python -B tests/windows_crossapp_pack_stage_b.py -v
```

The first command preserves all 49 Stage A methods. The second adds 13 methods
for identity binding, initial document state, exact bytes, tampered expectations,
IO rechecks, malformed application records and the distinction between artifact
match and GUI proof. Use the existing bounded Windows runner for native exits,
stdout/stderr and timeout/root-exit evidence. The added native-wrapper contract also checks shared Open, the initial same-session
pause/resume prelude, and zero tools after the original final Close.
Compile-only is not a green result.
