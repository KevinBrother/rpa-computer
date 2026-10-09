# Windows test-only portable TLS repair: CC + GLM execution

Workdir /Volumes/doc/workspace/datagrand/rpa/rpa-computer. Actual model must glm-5.3-flash (existing sonnet alias). No agents/Task delegation. No product/test source changes, commit/push/worktree, configuration/credentials/firewall changes. Windows ONLY; Mac cross compilation allowed, no Mac/Linux test execution. Logs and temporary invocation scripts only.

Read .agents/reports/windows-portable-test-assets-sol-20260930.md; verify six frozen SHA256 before compiling. Prior first red must remain intact. Use its prescribed cargo-xwin environment, NEW target .agents/runs/windows-portable-test-assets-cc-20260930/target. Build lib and remote_transport tests --no-run --locked and capture Cargo JSON + stderr + actual exit. Resolve exact test executable paths from compiler-artifact JSON; hash both before and after transfer.

Run only these gates on acer-win:
1. lib `mcp::remote::tls::` --test-threads=1, full stdout/stderr + native exit.
2. if targeted TLS passes, full lib (default ignored remain ignored), --test-threads=1. Allow up to 420 sec since previous full lib took 238 sec. SSH execution is permitted only because default lib tests are mock/pure; do not opt into any ignored/native/desktop tests.
3. remote_transport --list and ONLY helper filters `remote_assets::tests` and `remote_bundle::tests` individually. Inspect actual names from --list. These helper tests do not launch Host/client/network. Do NOT run full remote_transport: network/Host gates are deferred pending desktop firewall situation. Report deferred rather than passed.

Use a NEW unique Windows TEMP bundle named portable-assets-cc-20260930-r1 (confirm no existing directory, else choose r2). Test working directory must not be source tree; embedded cert assets must work without copying fixtures or recreating /Volumes. No need to deploy Host/client for these helper-only gates. Record source SHA, exe SHA, exact native args, starts/ends, exit and test counts. Bound wrappers, retain identity records for test PID/exepath/start/session before stopping any timeout, never global kill or name-only cleanup. User Notepad PID24332 is protected. Do not open/close GUI, run capture, use input, or start services. Another CC runs fake renderer process tests independently: do not touch its files, task, process, ports or env.

If failures: preserve raw red and report specific case/assertion; DO NOT implement fixes or silently skip. Own report .agents/reports/windows-portable-test-assets-cc-20260930.md and run directory of same basename. CLI success isn't test success. No claim real TLS transport or feedback UI passed from these tests.
