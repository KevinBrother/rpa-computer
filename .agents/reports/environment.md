# Environment evidence — coordinator

2026-09-24, no desktop input yet.

- Project is now an independent Git repository; no commits created. Work remains inside rpa-computer.
- Local: Darwin aarch64, rustc/cargo 1.98.1. Windows Rust std target x86_64-pc-windows-msvc installed successfully during this goal.
- Claude CLI readiness: `.agents/runs/claude-readiness.json` returned AGENT_READY, is_error=false. No model configuration modified.
- SSH acer-win responds hostname node1, user node1\\administrator. explorer process ID 8648 had SessionId 1 at discovery time; revalidate at deployment.
- Windows discovery: no cargo/rustc in PATH, no ~/.cargo/bin cargo, no VS vswhere at known installer paths, no LLVM clang at standard path. Cross-compilation is being prepared locally.
- macOS source denial negative probe: `sandbox-exec` policy deny file-read* at original project path caused `/bin/cat .../Cargo.toml` to fail with Operation not permitted. This is a filesystem probe, not yet proof of full Claude+MCP image operation under isolation.
- cargo install initially inherited a broken global ustc git registry. Retried command explicitly selects rsproxy sparse for tooling installation; no global registry files modified.

Live cross-toolchain installer handle: 57254 (`.agents/runs/toolchain-xwin.log`). Verify handle before retrying.

Update 18:00-ish local: local displays were asleep, not missing permissions. CGPreflightScreenCaptureAccess and AXIsProcessTrusted returned true; online displays [2,1] but active []. A non-input wake request `caffeinate -u -t 2` restored active displays [2,1]. This does NOT yet prove Rust Host capture/input or sandboxed CLI operation. No screenshot captured by coordinator.

Windows SDK/CRT preparation via cargo xwin env completed. cargo-xwin cache provides clang-cl and lld-link symlinks (Apple clang / Rust rust-lld). Don't assume standalone lld-link must be installed globally. Try cargo xwin build once integration compiles.

Actual source-isolated Claude startup proof (not GUI acceptance): launched Claude Code from `/private/tmp/computer-agent-isolation.ZjlmAz` under OS sandbox denying read/write of the original source tree. Existing model authentication still worked. stream-json init reported `tools: []`, `mcp_servers: []`; result `ISOLATED_AGENT_READY`, is_error=false. Log directory is recorded in `.agents/runs/isolation-check-dir.txt`. No MCP was attached and no image was sent, so this does not validate visual reasoning or Host behavior.

## Coordinator continuation: actual preliminary capture

- Windows cross build exec48657 completed exit0; PE32+ x86-64 artifact initial SHA256 `26c24ae5ad82aaf4eef8313a4413850fe80771a155db4d9488d51860aa9ef29d`. Source still changing; NOT the final artifact.
- `ssh acer-win` rechecked Explorer PID8648 SessionId1, path C:\WINDOWS\Explorer.EXE. No Windows GUI host deployed/started during this check.
- Existing compiled test snapshot `target/debug/deps/rpa_computer-d9cd46035e242a1f backend::`: 24 passed, 1 intentionally ignored. This is a prior compiled source snapshot, NOT verification of pending edits.
- Explicit non-input live capture via `caffeinate -d -u -t 45 <snapshot-test-binary> backend::live_test::live_backend_capture_only --ignored --exact --nocapture`: exit0; real capture 1920x1080, 3331134 PNG bytes, surface macos:2, geometry input 1920x1080. PNG dimensions decoded/asserted twice. No allow-skip flag, no input injection. Evidence `.agents/runs/macos-live-capture-preliminary.log`.
- This proves native capture was available at that moment. It does NOT prove Claude vision, interactive input, final Host build, or black-box acceptance.
- Windows loader smoke executed on acer-win: initial compiled exe copied to the new directory recorded in `.agents/runs/windows-loader-check-dir.txt`; `--version` returned `computer-host 0.1.0`, exit0. Remote SHA256 matches `26c24ae5ad82aaf4eef8313a4413850fe80771a155db4d9488d51860aa9ef29d`. This only proves PE loads on Windows; command did NOT start native desktop service and does NOT prove GUI access. Output `.agents/runs/windows-loader-check.log`.

## macOS real GUI precondition — locked screen (18:56)

Coordinator verified the new native visual fixture compiled binary hash matches worker report and copied ONLY its .app to the fresh outside-source directory in `.agents/runs/macos-fixture-release-dir.txt`. Started it with an oracle path under denied source `.agents/runs/macos-fixture-layout.oracle.jsonl`. Process PID64330 observed; oracle emitted a session and first trial, proving app logic started, NOT that it was visible/usable.

Actual OS screenshot `.agents/runs/macos-fixture-layout.png` was visually inspected: macOS LOCK/LOGIN screen with password field, not the fixture. User was asked to manually unlock; no credentials entered, no attempt to bypass lock. Do not count this as GUI acceptance. Earlier backend screenshot decode evidence remains only screen-pixel capture proof, not desktop application availability. No input performed. Continue implementation and Windows checks while awaiting an unlocked local console.

The fixture release must not be given to black-box Agent with this oracle; it contains only the app, while oracle stays under the source-denied tree. Fresh nonce should be generated for actual blind trial so coordinator's known preflight trial is not reused as model prompt.
