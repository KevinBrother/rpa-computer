# Linux parity harness implementation plan

Scope: only this directory and docs/linux-parity-validation.md. User's frozen design
and explicit implementation request authorize this plan; no further agents, commits,
worktrees, tests or GUI execution by the author.

1. Implement bounded stdlib JSON-RPC and file-only evidence; owned Tk fixture and
   a first usable native smoke scenario (entry, count button, actual text).
2. Extend independently selectable display, Unicode, editing, drag, wheel, chord,
   hold, lifecycle and cancellation cases. All success needs application evidence.
3. Snapshot core keymap/modifiers/held state using a small read-only Xlib helper
   in a timeout-controlled subprocess. Do not pretend this is a full XKB snapshot.
4. Add isolated Xvfb wrappers for authority and DPI; never mutate an existing
   desktop's configuration. Preserve environment by child-only overrides.
5. Write exact CC commands, status semantics and remaining scenarios in runbook.
6. Static review and py_compile only. Independent CC owns all execution, including
   harness self-tests. Compilation is not acceptance.
