# Windows explicit text-paste acceptance

## Scope and ownership

The main coordinator compiles and tests on Windows through **CC GLM Windows**.
The implementation task itself must not launch the GUI, use SSH, edit any other
files, commit, or push. The two source deliverables are:

- `acceptance-fixture/text-paste/TextPasteFixture.cs`
- `acceptance-fixture/tasks/windows-text-paste.md`

The fixture is a standalone .NET Framework WinForms executable with its own
`Main`. Its source deliberately lives **outside `acceptance-fixture/windows/`**.
Compile that one file only; do not add it to the existing fixture build glob or
compile it together with another fixture's `Main`.

## Coordinator: compile and non-GUI regression check

Run from the repository root on Windows. This uses the installed .NET Framework
compiler, not an SDK project, NuGet, or Roslyn-only language features. Build and
runtime artifacts go to a temporary directory, not the repository:

```powershell
$ErrorActionPreference = 'Stop'
$csc = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
if (-not (Test-Path $csc)) {
    $csc = Join-Path $env:WINDIR 'Microsoft.NET\Framework\v4.0.30319\csc.exe'
}
if (-not (Test-Path $csc)) { throw '.NET Framework 4.x csc.exe is required.' }
$source = Join-Path $PWD 'acceptance-fixture\text-paste\TextPasteFixture.cs'
$runDir = Join-Path $env:TEMP ('rpa-text-paste-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $runDir | Out-Null
$exe = Join-Path $runDir 'TextPasteFixture.exe'
$evidence = Join-Path $runDir 'text-paste.jsonl'
& $csc /nologo /target:exe /platform:anycpu /codepage:65001 `
    /r:System.dll /r:System.Drawing.dll /r:System.Windows.Forms.dll `
    "/out:$exe" $source
if ($LASTEXITCODE -ne 0) { throw 'Compilation failed.' }
& $exe --self-test
if ($LASTEXITCODE -ne 0) { throw 'Non-GUI self-test failed.' }
```

`--self-test` returns before WinForms initialization, opens no window, reads no
clipboard, invokes no IMM functions, and writes no evidence file. It validates
payload-only CRLF normalization, strict ordinal UTF-16 comparison, whitespace,
width/case/canonical-Unicode differences, JSON control/surrogate escaping,
codepoint diagnostics, the four samples, and single-line evidence serialization.
An executable console target is intentional so compiler/self-test diagnostics
are visible; the GUI itself is the owned WinForms window.

## Coordinator: separately authorized Windows GUI run

Only perform this phase when running Windows acceptance, not while implementing
the fixture. Reuse `$exe` and `$evidence` from above. The parent directory must
already exist and be writable. Supply a fresh evidence path for each run:

```powershell
$fixture = Start-Process -FilePath $exe -ArgumentList @(
    '--suite', 'text-paste', '--evidence-file', ('"' + $evidence + '"')
) -PassThru
```

The fixture consumes and ignores `--suite <value>` for compatibility with the
argument shape of `start-windows.ps1`; this does **not** integrate the standalone
source into that script's build or change the existing launcher. The evidence
path is required in GUI mode and is probed before opening the form. Each Check
appends one UTF-8 JSONL record. The title is exactly:

**RPA Text Paste Acceptance 20261013**

The fixture owns its process and form. Escape closes that form only. A fixed
WinForms timer closes it after 15 minutes without being reset by interaction
(the UI event loop must remain responsive). Do not close other applications or
kill processes by a broad name match. The coordinator may clean up only the
specific process it launched if a hung UI prevents normal close.

## Send this prompt to the CC GUI acceptance operator

The coordinator provides this section as the operator prompt; do not provide the
source, helper outputs, evidence content, or sample strings as extra context.

> Test the already-launched window titled exactly
> **RPA Text Paste Acceptance 20261013**. Use **MCP screenshots only for
> observation** and supported MCP mouse/keyboard/text actions for interaction.
> Do not use filesystem reads, shell commands, source inspection, evidence-file
> reads, accessibility/UIA trees, DOM, OCR APIs, clipboard reads/writes outside
> MCP, or another channel to discover expected strings or application state.
> Reason visually from the screenshots; an MCP text action may internally use
> its own clipboard implementation, but do not manipulate the clipboard yourself.
>
> There are four fixed cases. For each case, take a screenshot, read the large
> expected sample and the exact JSON string displayed beneath it, and derive the
> payload from that screenshot only. Decode the JSON escapes and omit its outer
> quotes. `\t` means one actual TAB; `\r\n` means actual CR followed by LF, not
> the four visible escape characters. Decode emoji surrogate escapes as the
> displayed emoji. Preserve every space, case, punctuation mark and backslash;
> do not append an extra final newline. The JSON string is authoritative if the
> sample wraps visually or a tab's width is ambiguous. If text is unreadable,
> resize only this fixture via MCP and take another screenshot before input.
>
> Before entering each case, click **Enable IME Full Width** via MCP. Screenshot
> and record the reported queried IME state, including unavailable/not-verified
> outcomes. This button addresses only the fixture TextBox context; do not use
> system keyboard-layout switches, taskbar controls, global IME shortcuts, or
> settings in another window. An unavailable IME is an environment limitation,
> never proof that full-width mode was enabled. Continue the paste checks and
> report IME coverage separately.
>
> Focus the empty **paste target** and invoke exactly **one** MCP text-input
> action for the whole case with explicit **`method: "paste"`**. Do not leave the
> method unspecified, use native/auto typing, split multiline content into
> multiple actions, manually send Enter/Tab, prepare the clipboard separately,
> or fall back to Ctrl+V. Use the MCP tool's actual documented schema. If it does
> not support explicit paste, report BLOCKED rather than substitute a method.
>
> Click **Check** once and screenshot the conspicuous PASS/FAIL result together
> with the current case and IME status. Report what happened on the first attempt,
> including input-action errors, evidence-write failures, and mismatches. Never
> clear, repair, retry, restart, or re-paste to get green. If the single input
> action errors or appears to do nothing, still click Check once to capture the
> observed value and report the error. The fixture freezes a checked case so
> evidence cannot be overwritten by a second attempt. Clear exists only for
> manual setup before input; do not use it during this acceptance run.
>
> Click **Next** to move forward only after recording the result. Repeat for all
> four cases. Do not perform a native-input baseline. After the final screenshot,
> press Escape with this fixture focused. Return a four-row table with case,
> first-attempt result, IME state/coverage, MCP action errors, and screenshot
> references. Do not report unavailable IME coverage as a full-width success.

## Evidence and coordinator verdict

Only the coordinator, **after the screenshot-only operator phase**, may read the
JSONL file. Keep these responsibilities separate: the GUI operator must not use
evidence/source reads to discover or repair its input. Expected evidence:

- Exactly four records in order: `chinese`, `mixed-ascii`, `emoji`, `multiline-tab`.
- Each record has an ISO-8601 UTC `timestamp`, `case`, `expected`, `actual`,
  `expectedCodepoints`, `actualCodepoints`, `expectedUtf16Length`,
  `actualUtf16Length`, boolean `match`, and queried `ime` state.
- `expected` normalizes **only the fixture payload's** line endings to CRLF.
  `actual` is the TextBox value verbatim. Ordinal equality compares UTF-16 code
  units without trimming, width folding, Unicode normalization, or transforming
  the observed line endings. Thus LF-only actual text fails a CRLF expectation.
- JSON escapes all control characters and surrogate code units, preserving even
  unpaired surrogates. Codepoints combine valid surrogate pairs into one scalar;
  unpaired surrogates remain visible as e.g. `U+D800`. No tabs/newlines inside
  strings can accidentally split a JSONL record.
- GUI PASS requires both an exact text match and a successful evidence append.
  Missing/truncated records, screenshot/evidence disagreement, or write errors
  invalidate a claimed successful run. Process exit code alone is not acceptance.

Classify **text-paste correctness** separately from **IME full-width coverage**.
Four first-attempt matches with durable records establish the former. The latter
requires screenshots and check-time state showing queried open status plus
`NATIVE | FULLSHAPE` bits; an absent/unsupported IMM context or a refused mode
change leaves that coverage unverified. IMM state is API evidence, not a claim
about every modern TSF IME's behavior. The fixture attempts to restore the saved
open/conversion/sentence state of the same TextBox context when closing; it never
switches global keyboard layouts. No native baseline is included, avoiding
unsafe multiline/native key side effects.

Record the actual compiler/self-test output and Windows GUI results; do not infer
that compilation, rendering, IMM behavior, or acceptance passed from this task
file or from a source-only review.
