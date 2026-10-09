# Windows crossapp pack — Stage A contract, NOT an implementation

Stage A freezes executable unittest requirements and **five unimplemented API
functions**. Every stub unconditionally raises a capability-specific
`NotImplementedError`. No preparation/checking, CLI entrypoint, application launch
or offline UI is implemented yet. CC must observe real Windows RED before Stage B.

## API surface (Python 3.12, standard library)

| Module / function | Stage B contract |
|---|---|
| `crossapp_paths.validate_new_root(root: str) -> Path` | Inspect only. New absolute local root with an existing ordinary parent; unsafe input raises `ValueError`. No mkdir/delete. |
| `crossapp_paths.resolve_owned_path(root: str, relative: str) -> Path` | Existing ordinary local root; validate root/ancestors/components/leaf without following symlink/reparse entries. Return contained path or `ValueError`. Not a window-ownership assertion. |
| `crossapp_pack.prepare_owned_pack(root: str) -> dict` | Filesystem-only, create exclusively; no existing root accepted even if empty. Return same manifest persisted under supervisor. Never launch/kill/listen/configure/delete. |
| `crossapp_artifacts.check_notepad_bytes(actual: bytes, expected: str) -> dict` | Strict raw-byte decode, max 64 KiB. UTF-8 / UTF-8 BOM / UTF-16 LE BOM / UTF-16 BE BOM. Invalid/oversized => unknown; preserve actual text/hash, never normalize or trim. |
| `crossapp_artifacts.check_artifacts(root: str) -> dict` | Read-only prepared-root inventory. Unsafe/unprepared root or escaping/reparse artifact => `ValueError`. Missing proof => needs_evidence; no GUI certification. |

Path policy rejects relative/drive-relative/UNC/device namespace, traversal in
either separator, ADS, reserved Windows names, trailing dot/space aliases,
symlinks and junction/reparse entries. `os.lstat` is the explicit inspection seam.
Tests inject lstat facts ONLY for their own synthetic temporary paths; no mklink,
real link privileges, shell or application activation is needed. Stage B must
recheck safe components when opening/creating files, not treat a lexical check as
protection against concurrent path replacement. Failure may leave owned partial
files; no rollback deletion of unknown assets.

## Prepared layout and minimum schema

- `agent/crossapp-01/task.md` through `agent/crossapp-06/task.md`: task text only.
  No other agent-zone files; no manifest/oracle/checker/source published there.
- `assets/`: visible local application assets, browser page, Explorer source and
  destination. `results/`: case-specific planned output parents, initially no
  successful result files. All stored paths below are root-relative.
- `supervisor/manifest.json` and `supervisor/oracle.json`: coordinator only.
- Manifest: `schema="windows-crossapp-pack-v1"`, `gui_verified=false`,
  `action_trial_gate_satisfied=false`, `protected_user_pids` includes 24332;
  exactly six ordered `cases` with `case_id`, distinct nonempty `run_id`,
  `trial_id`, `nonce` (each at least 12 chars), `agent_task`, `allowed_tools`,
  `status` pending/needs_evidence. Each case's `ownership.status=needs_preflight`;
  `owned`, `exe_path`, `exe_sha256`, `pid`, `creation_time`, `hwnd`,
  `interactive_session_id`, `visible_title_nonce` are **null**, never zero/true.
- Allowed tools exactly computer describe/open/observe/step/get_step/pause/resume/
  close. Task text requires STOP + Close on prompts, unknown ownership or partial
  input/cleanup failure; forbids shell/files/DOM/a11y/CDP/external network. Visible
  known input is allowed; machine answers or private evidence are not.

Case contracts:
1. Oracle `crossapp-01`: `payload` = `expected_text` =
   `ASCII: Hello 123\r\nUnicode: 中文 Ω 😀 e\u0301\r\nNonce: {nonce}\r\nTrailing space: END \r\n`;
   `newline_policy="windows-crlf-exact-v1"`, `output_path` below
   `results/crossapp-01/`. Task includes text_input and selection/key_chord steps.
   BOM labels are `utf-8-sig`, `utf-16-le-bom`, `utf-16-be-bom`; BOM-free UTF-8 is
   `utf-8`. Diagnostics return `state`, `encoding`, `actual_text` when decodable,
   `actual_sha256` of untouched bytes, `gui_verified=false`. NFD/NFC, LF/CRLF,
   mixed newlines and final spaces remain different. No read_text newline folding.
2. Calculator manifest case has `observations`: three distinct `trial_id` records,
   each `expected_visible_text="200"`, `requires_fresh_observation=true`,
   `status=needs_evidence`. Tasks perform visible clear → 10×20= and three actual
   independent Observe calls. File checker cannot judge these screenshots.
3. Explorer oracle: `source`, `destination`, `move_file="item-37.txt"`,
   `sha256_by_name` for precisely item-01.txt..item-64.txt. Every initial file has
   the case nonce; destination initially empty. Only exact 63+1 final hash/inventory
   is artifact_match; a copy, corruption or unknown file mismatches. Unknown files
   remain untouched. `drag_verified=false` even on artifact_match.
4. Browser case: `page_path`, `browser_profile_status=needs_supervisor_configuration`,
   `download_directory_status=needs_supervisor_confirmation`. Offline inline UI
   has IDs vertical-course, horizontal-course, entry-form, drag-source, drop-target,
   visible-result and visible case/nonce; no server/listener/external dependencies.
   Oracle names `event_export_path`; >256 KiB export => unknown. Page events/model
   claims are untrusted application data, not independently verified GUI evidence.
   Stage B UI work must read frontend-design skill; it is not part of Stage A.
5. Native canvas reuses unchanged `acceptance-fixture/tasks/drag.md` in generated
   task text (embed content, never tell GLM to open source). Supervisor `reuse`
   has `suite=drag`, actual `source_sha256`, `required_case_ids` including drag-10,
   `curve_min_horizontal_reversals=2`, `requires_down_motion_up=true`.
6. Focus similarly embeds unchanged `acceptance-fixture/tasks/windows-focus.md`,
   `reuse.suite=focus` and actual source hash. Both reused cases set
   `counts_as_new_external_app=false`; keep all original owned-window restrictions.

Checker report: `cases` entries `{case_id,state,...}`, top-level `gui_verified=false`
and `action_trial_gate_satisfied=false` always. Allowed states artifact_match /
mismatch / unknown / needs_evidence. Calculator and native cases remain
needs_evidence without independent GUI proof. Manifest/model self-claims never
upgrade evidence. File results cannot prove drag paths, screenshot correctness or
10 first-valid trials per action.

Installed executables/packages do not establish safe independent activation.
Do not hardcode machine package versions or Edge paths. Only a future authorized
supervisor may confirm application location/hash and fresh owned HWND identity;
Store-app activation reuse is unverified. Protected Notepad PID24332 is untouched.

## Stage A execution (CC only, Windows)

`python.exe -B tests/windows_crossapp_pack.py -v`

Every current requirement is expected to FAIL via a real invocation of one of
the five missing capabilities. The adapter converts NotImplementedError into a
named unittest assertion failure; it never skips or treats import success as a
pass. Test-created temporary directories/files are owned synthetic data, not GUI
artifacts. Only their test harness cleanup may remove them. Author has not run
this command. Stage B must remove stubs by implementing the requirements, not
weaken the frozen assertions. Thin CLI scripts and generated six-case tasks are
also Stage B work, not silently claimed delivered here.
