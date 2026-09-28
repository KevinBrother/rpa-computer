# GUI acceptance fixture & controlled-desktop plan (coordinator-run, never QA)

QA never starts GUI tests. This document is the **setup plan** the
coordinator follows before running `scripts/run-blackbox-claude.sh` for
real. Its purpose: the screenshots the external Agent receives must contain
ONLY the controlled test content — no source code, no terminals, no shells.

## 1. Desktop preparation (before every acceptance run)

- [ ] Dedicated test user account (or a desktop the owner has cleared).
- [ ] **Close every window**: terminals, editors, IDEs, browsers, Finder
      windows showing the repository, chat clients with source snippets.
- [ ] Set a **neutral wallpaper** (solid color; no project-related imagery).
- [ ] Verify the source tree is not open in ANY visible application
      (`scripts/run-blackbox-claude.sh` additionally denies file-level
      access via sandbox-exec; this checklist covers the *visual* channel).
- [ ] Stage the controlled content the task needs:
      - task 01: a single window showing a large colored block + one big
        word (the ground truth; NOT written in the task prompt);
      - task 02: the target-click fixture window (colored circular target
        that records where clicks land);
      - task 03: desktop clear so the Calculator can be launched via GUI;
      - task 04: desktop clear so TextEdit/Notepad can be launched;
      - task 05: desktop clear; coordinator stands by to send cancellation.
- [ ] No notifications/screen-saver/password prompts pending.

## 2. Fixture applications (source-free, on the test desktop only)

The fixtures must NOT be built from this repository at test time (a build
step would leave source artifacts visible). Coordinator options:

1. **Task 01 ground-truth board**: any preinstalled image viewer or a
   full-screen `Keynote`/`Preview` slide with the agreed color + word.
   The answer is agreed out-of-band and recorded in the run log, never in
   the task prompt.
2. **Task 02 target**: simplest honest fixture is a small web page opened
   in a browser — **but browsers are banned for the Agent**, so prefer a
   preinstalled utility: e.g. a sticky-note/whiteboard app with a drawn
   target, or the macOS `Digital Color Meter`-style overlay. Whatever is
   chosen, click-landing must be verifiable (target changes color/records
   the hit position). If a tiny fixture binary is wanted, build it in a
   SEPARATE throwaway directory outside this repo, copy only the binary to
   the test desktop, and delete the build directory before the run.
3. Tasks 03–05 use OS-bundled apps only (Calculator, TextEdit/Notepad).

## 3. Run procedure

```bash
scripts/sandbox-probe.sh                      # profile sanity (standalone)
scripts/run-blackbox-claude.sh --release-dir ../rpa-computer-release \
    --task ../rpa-computer-release/tasks/0N-*.md --print
```

- The launcher enforces: `--tools ''` (no built-ins), strict MCP config,
  `--disable-slash-commands`, `disableAllHooks`, `--setting-sources user`
  (user model/auth untouched; stray project/local settings ignored),
  `--system-prompt` clean replacement prompt, sandbox-exec deny of the
  source tree + known source copies + scratch roots, nonempty
  screenshot-demanding task text, and bounded `--max-turns` /
  `--max-seconds` budgets.
- **Print acceptance path** writes a real stream-json transcript to
  `<release>/evidence/<run-id>/transcript.jsonl` (0700; outside the source
  tree; tokens are never passed to Claude so transcripts carry no
  credentials). After the process exits the launcher runs
  `scripts/audit-claude-transcript.py` over the transcript and FAILS THE
  RUN unless it passes — the audit is executable evidence, re-runnable by
  hand:
  `scripts/audit-claude-transcript.py <transcript.jsonl> --expect-server computer --max-turns 12 --max-seconds 600`.
- **Audit at run start (automated)**: the init event's effective tool
  inventory must be exactly the 8 `mcp__computer__computer_*` tools and
  the only MCP server must be `computer` (connected). Any other tool
  (Read/Bash/Glob/Grep/Edit/Write/WebFetch/WebSearch/Task/...) or server
  aborts the run as invalid.
- **Audit at run end (automated)**: every assistant tool call in the trace
  must be a `computer_*` tool; permission denials, recorded hook
  executions, API/model errors, and budget overruns invalidate the run.
- Coordinator watches the desktop live; if the Agent tries to open a
  terminal/Finder-to-source, the run is invalid (stop it).
- NOTE: a policy-clean transcript is NOT proof of GUI task correctness;
  §4 ground-truth comparison is still required.

## 4. Evidence to record per run

- [ ] `<release>/evidence/<run-id>/transcript.jsonl` — the real stream-json
      transcript (init event = effective tool inventory; full tool-call
      trace = computer_* only). Verified automatically by the launcher via
      `scripts/audit-claude-transcript.py`; keep the audit output.
- [ ] Final screenshots returned by the Agent — coordinator eyeballs that
      no source/terminal content is visible in ANY screenshot.
- [ ] sandbox-probe output from the run profile (proves the exact profile
      denied the source tree).
- [ ] Task-specific ground truth comparison (01 description, 02 click
      coordinates vs. recorded hit, 03 calculator display, 04 text
      content, 05 cancellation timing).

## 5. Explicit non-claims

- A passing run under this fixture proves the Agent drove the GUI through
  the MCP tools with file-level source isolation enforced by the OS. It
  does NOT prove the GUI is an airtight sandbox (visual channel risk is
  mitigated by §1, not eliminated).
- `--verify-only` runs of the launcher validate argv policy against a fake
  claude binary; they are never GUI evidence.
- The transcript audit validates run POLICY on a real transcript (tool
  inventory, tool-call trace, denials, errors, budgets). It is not proof
  of GUI task correctness — only the §4 ground-truth comparison is.
- The sandbox denies this project's source tree, KNOWN project source
  copies (e.g. /tmp/rpa-core-shim), and temp scratch roots; it does not
  claim to enumerate every possible copy a human made by hand elsewhere.
