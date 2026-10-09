# Native Input Fixture — Unicode Sample Update (2026-09-29)

Task: `.agents/tasks/native-input-fixture-20260929.md`. Scope honored: only `acceptance-fixture/macos/main.swift` and `acceptance-fixture/windows/MainForm.cs` modified. No launch, no GUI/SSH, no commit.

## Changes

New sample text on BOTH platforms (byte-identical, verified by grep count = 1 each):

```
你好，世界 🌍 | Unicode 测试 ✅ | 本文档共 3 段（含本段）
```

- macOS `main.swift`: `kSampleText` replaced. Sample label enlarged 26→38pt height, font 16→15, `lineBreakMode = .byCharWrapping`, `usesSingleLineMode = false`, `maximumNumberOfLines = 2`. Frame y:190 h:38 fits exactly in the 40pt gap between shapes canvas (bottom edge 230) and text area (top edge 190) — no overlap.
- Windows `MainForm.cs`: `SampleText` replaced. Sample label 24→44px height (y:382..426, above the 430 textbox), wraps within fixed bounds (AutoSize stays false). Shapes panel height 230→210 to make room; slot geometry (110px shapes, width-proportional centers) unaffected since 210 > 118 minimum.
- Text oracle unchanged in mechanism: both `checkText`/`CheckText` compare typed text `==` the same constant; macOS grapheme count via `String.count`, Windows via `StringInfo.LengthInTextElements`.
- Randomized target, nonce, next-trial, and evidence-file (`trial`/`hit`/`wrong`/`text_check` JSONL) semantics untouched. No prompt answers added.

## Verification

- macOS build: `./acceptance-fixture/build-macos.sh .agents/runs/native-fixture-build-20260929` — OK.
  Binary sha256 `7fd6c144af57be39daad5eabd023acd74bc3a04d3722fbfb650c8531d80cb000`.
- Old binaries preserved: `acceptance-fixture/build/macos/ComputerUseAcceptance.app` untouched (build script refuses overwrite anyway).
- Windows compile: real `csc` unavailable for a WinExe on this Mac, but Mono `mcs` syntax-checked `MainForm.cs` with `-r:System.Windows.Forms.dll -r:System.Drawing.dll` — **COMPILE OK** (0 errors). Native Windows build/launch still pending on acer-win per normal gate.
- Static inspection confirms layout has no overlaps; exact same oracle string on both platforms.

Not done (out of scope): launching the fixture, visual screenshot validation, Windows exe build on the Windows host.
