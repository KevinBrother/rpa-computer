# Native Fixture — text_check Oracle Details (2026-09-29)

Task: `.agents/tasks/native-fixture-oracle-details-20260929.md` + appended requirement (`actual_utf16_hex`). Scope honored: only `acceptance-fixture/macos/main.swift` and `acceptance-fixture/windows/MainForm.cs` touched; no GUI/SSH/commit; no font or sample changes.

## Changes (three fields total)

Added `expected_text` and `actual_text` JSON strings to the existing `text_check` evidence log on both fixtures, logged only on explicit "Check text" click, from the fixture's own input box (no global/user app input involved):

- macOS (`main.swift` checkText): added `"expected_text": kSampleText, "actual_text": got` to the `[String: Any]` dict. JSONSerialization handles all escaping — newlines, quotes, and nonBMP (emoji as UTF-8) — automatically and correctly.
- Windows (`MainForm.cs` CheckText): added `"expected_text", SampleText, "actual_text", got` to the kv-pair log call. Existing `EvidenceLogger.Escape()` escapes quote, backslash, and all chars < 0x20 as `\uXXXX` (so newlines and CR are escaped); surrogate pairs pass through and are UTF-8-encoded on `File.AppendAllText`, so nonBMP is correct.
- Appended requirement — `actual_utf16_hex` on both: space-separated four-digit uppercase hex UTF-16 code units of the typed text, preserving malformed/unpaired surrogate evidence that round-tripping through JSON strings alone would mask. macOS: `got.utf16.map { String(format: "%04X", $0) }.joined(separator: " ")`. Windows: `got.ToCharArray().Select(c => ((int)c).ToString("X4"))` joined with spaces (`System.Linq` already imported).

Evidence files remain private coordinator-only JSONL (path supplied only via `--evidence-file`, never given to the agent). Trial/nonce/target/UI semantics untouched; lengths (`expected_len`/`got_len`) retained alongside the new fields.

## Verification

- macOS build (hex requirement): fresh dir `.agents/runs/native-fixture-oracle-hex-build-20260929/ComputerUseAcceptance.app`, sha256 `4acff79921cfbb11c205578ac6779f74ac18b2d8b6d6f149e646d701fabe9354`. Previous run-dir binaries (`native-fixture-build-20260929`, `native-fixture-oracle-build-20260929` sha `8e560ca0e554b660cb6128b7b62bd6c5c0762630899591bc691936c3933cf2fc`) untouched; not launched.
- Windows: Mono `mcs` syntax check with Forms/Drawing references — COMPILE OK (0 errors), including the new LINQ hex conversion. Native build on acer-win remains the normal gate.
- Static inspection: escaping paths verified per above; field order/format matches existing log entries.
