# Windows native-text default build independent verification — CC+GLM

Read latest `.agents/reports/windows-native-text-policy-sol-20260930.md` INCLUDING final default-build amendment and final manifest. The original opt-in compile flag handoff was rejected: NEW default build-windows.ps1 must include NativeText modules without special flag and there should be no canonical fallback path. Launch only after final SOURCE_FROZEN. Same actual glm-5.3-flash user alias, noAgent/Task. You own execution/review only; no product/test/reusable build script edits. Windows-only Session0 pure tests; no GUI/Host/network listener/capture/nativeinput, no Mac/Linux tests. Protect Notepad24332; no permission/globalchanges/commit/push/worktree; don't touch root Rust (its full275+integrations59 now green).

Static spec/code review before running:
- native expected narrow known-input/known-09 with explicit version and export; raw actual/tool payload untouched, Utf16ExactMatch retained. No global CRLF/LF tolerance, no swallowing extra CR/LF.
- Legacy63 canonical export/parity unchanged, old GUI fail remains fail, new checker must not backdate old run as green. New native policy export CreateNew, first/final/unknown strict evidence; no GUI acceptance from synthetic fixtures.
- ordinary build helper enables correct policy. Prior178 C# assertions plus focus/B2 analyzer regressions unchanged.
If concrete blocker, report stop; don't fix source yourself.

Reuse your prior focus staging/runner patterns (session be96bff3...). NEW unique Windows TEMP and local `.agents/runs/windows-native-text-policy-cc-20260930`. Stage final frozen source dependencies + approved runner layout scripts/windows-test-runner.ps1 + scripts/windows-test-runner/Runner.cs byte-exact, preserve relative paths. Include parity tools and readonly Swift catalog DATA only; include NativeText analyzer/test + focus/B2 scripts/tests. Verify all hashes local/remote before/after. Don't reuse old focus exe or opt-in prototype exe. Windows in-box Framework csc via default frozen build-windows.ps1 in NEW outdir, capture numeric build exit & exeSHA. No special define, no localMac execution. Real Windows python absolute path, not Store alias.

Use bounded existing runner (precreate evidence PARENT not leaf), perstep120s; disposable invocation/config/collection scripts permitted, no new test oracle implementation:
1 --self-test (original178 retained, actual new count from stdout, original lines comparison).
2 --export-native-text-policy, --export-cases63, --export-platform-cases20, --export-focus-cases10 to fresh output paths.
3 Windows Python tests/windows_native_text_analyzer.py; then tests/focus_gui_analyzer.py and tests/basic_input_gui_analyzer.py regressions.
4 legacy63 parity on newexport via acceptance-fixture/tools/check-cases-parity.py; independently read manifest canonical known09 LF vs native expected CRLF, same payload, version, actual_normalized false. No historical oracle analysis/reclassification.
First native failure stops remaining sequence, preserve first raw; no rerun-for-green or loosening assertions. CLI exit alone isn't pass. Wrapper errors retained separately.

All raw summary/stdout/stderr/config/identities/SHA/drain/timeouts archived, exactowned read-only residual check (no kill). Final report `.agents/reports/windows-native-text-policy-cc-20260930.md`, state actual tested default build, all counts/nativecodes and boundaries. Existing Windows known-input GUI9/10 historical result unchanged; NEW GUI run10/10 remains pending. Stop after report.
