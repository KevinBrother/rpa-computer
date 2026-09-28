# README factual closeout (documentation only)
You are Claude Code. Sole writes README.md and .agents/reports/readme-closeout-1.md; read local code/scripts as needed. No product source/tests/scripts modifications, no GUI/remote launches, worktree/commit/push, global config or model changes. Other agents own all code/scripts.

Fix outdated README against actual implementation without inventing success:
- old source demo migration is not ongoing; inspect tree and describe actual current modules rather than abandoned worker roadmap.
- package-release.sh default no longer ../rpa-computer-release (forbidden sibling). Document actual safe mktemp release output and how user obtains/uses exact printed path; don't recommend conflicting rpa-computer-* scratch names under temp.
- Windows '无需额外权限' overclaims: interactive unlocked same desktop and permissions/secure desktop boundaries remain; current Windows capture refused0x80070005 with LogonUI present. Do not claim GUI success.
- clearly document real default vs explicit --mock-backend (protocol only, never actual GUI).
- correct build commands (macOS cargo; cached Windows cargo xwin on this environment, ordinary cargo on Windows native). Distinguish build support from verified input.
- model is configured existing Claude; no Runner/Model Bridge/API keys setup or source access for acceptance. Tool-list and filesystem sandbox limits: 8 tools only plus actual macOS deny probe; GUI can still expose files if user navigates, not absolute OS confinement.
- status concise: real Claude Windows MCP preflight made5calls describe/open/observe2errors/close, no image or input. Last independent full snapshot209passed2ignored, but transport hardening still in progress; don't call this released/final. GUI success0, original 10-trial/platform gates pending desktop unlock. Avoid embedding volatile exact counts if unnecessary; link .agents/reports/blackbox-readonly-preflight-1.md and review4.
Keep README practical, concise Chinese. Check documented flags with --help or source read only; no launches. Report changed claims and verification.
