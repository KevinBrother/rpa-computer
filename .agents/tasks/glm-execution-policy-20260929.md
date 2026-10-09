# Explicit GLM execution policy (user approved 2026-09-29)

Use real local Claude CLI with --model sonnet (mapped to glm-5.3-flash) for this implementation task. No GUI/SSH/tool execution outside local tests. No git commit/push/worktree, no global configuration changes.

Write scope ONLY:
- scripts/run-blackbox-claude.sh
- scripts/test-release-scripts.sh (only if necessary for regression test)
- .agents/runs/gui-remote-agent-glm-20260929.ps1 (new copy)

Task:
1. Inspect run-blackbox-claude.sh and its --verify-only harness. Ensure actual CLI invocation ALWAYS has explicit --model sonnet rather than inheriting default Kimi. Add/update harness assertions so the actual argv proves model selection (print and interactive if supported); adjust positional assertions correctly, or append model after system prompt and update indexes. Preserve tool isolation and all existing safety checks. Do not remove tests.
2. Copy .agents/runs/gui-remote-agent-20260928.ps1 to the NEW path above, retaining original historical helper unchanged. In new helper default Model=sonnet and ValidateSet only sonnet. It must still pass --model $Model. Do not otherwise change functionality.
3. Run bash -n scripts/run-blackbox-claude.sh and bash scripts/run-blackbox-claude.sh --verify-only, and bash scripts/test-release-scripts.sh if practical. Report commands/results and exact modified files. Do not claim passing if failed.
4. No new dependencies, no model calls during tests, no desktop input. Stop after narrow changes and tests.
