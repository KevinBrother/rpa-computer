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

## Observability/readability contracts and trace

The six additional pure self-test groups cover explicit ambiguous-character
escapes, payload roundtrip preservation, intact escape/surrogate token wrapping,
bounded metadata entries and dropped counts, query exceptions that cannot skip
modeled dispatch, and flush acknowledgment without resetting event sequence.
The RED checkpoint was coordinator-observed; GREEN must be compiled and executed
independently. All groups should pass with native exit 0; no author-run result is
implied. Compile references and the single-source build command above are unchanged.

The canonical display uses 18-point Consolas and measured token-safe wrapping.
It explicitly escapes fullwidth colon, ASCII quotes/apostrophe/backtick, and
spaces, preserving the expected UTF-16 sequence. Its visual lines concatenate
without separators before decoding once. Valid surrogate pairs remain on one
visual row. If an intact display cannot fit, it shows DISPLAY INCOMPLETE instead
of silently truncating the authoritative representation; do not type or resize
to work around this during acceptance. The original literal sample remains.

The target observes only its own existing HWND at focus gain/loss, relevant
Ctrl/Shift/Alt/Win/V key boundaries, and WM_PASTE. It samples before/after exactly
one base dispatch, including ProcessCmdKey preprocessing because Ctrl+V may not
expose WM_PASTE to the subclass. It adds no focus calls, setters, layout changes,
clipboard calls, text reads, or composition queries. Acquisition/release of the
current IMM context is scoped to each observation. Exceptions become unavailable
metadata; no exception message is stored. A reentrant query is dropped explicitly.

A fixed 512-entry buffer retains earliest events until successful flush; excess
or reentrant events increment dropped without invoking another query. Sidecar I/O
occurs only at existing Check/close boundaries, to:

```text
<evidence-file>.ime-trace.jsonl
```

It appends a batch header then buffered samples and clears only after successful
append. A write failure disables later flushes (no retry) and reports only an
exception type to stderr; text-match acceptance stays independent. Partial batches,
missing traces, release failures, or dropped events limit IME conclusions. Do not
read the sidecar/source/self-test output into the GUI operator's context.

**Metadata schema:** batch rows contain `type=batch`, `flush` (Check/Close), UTC,
Stopwatch `frequency`, `count`, and `dropped`. Sample rows contain `type=sample`,
monotonic `seq`/`ticks`, zero-based `caseIndex`, `boundary`/`phase`, numeric `message`
and whitelisted `virtualKey`, `commandKey` (preprocessing vs WndProc), input/focus/
context handles, thread ID, `availability`, open/conversion/sentence state,
`modifiersAvailable`, `modifierMask`, and `releaseFailed`. Mask bits: Ctrl=1,
Shift=2, Alt=4, Win=8, V=16. It is the calling GUI thread's sampled keyboard state,
not a global physical-key oracle; an unavailable mask must not be read as released.
No expected/actual text, composition data, clipboard data, or general key stream
is included. Handles/timing remain local operational metadata.

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
> sample wraps visually or a tab's width is ambiguous. Canonical visual wraps
> add no characters: concatenate the displayed rows, then decode once. Use the
> configured 1920x1080 screenshot output and a single client. Do not resize the
> fixture or use taskbar controls. If the authoritative display is unreadable or
> reports DISPLAY INCOMPLETE, stop and report the limitation rather than guess.
>
> Before entering each case, click **Enable IME Full Width** via MCP. Screenshot
> and record the reported queried IME state, including unavailable/not-verified
> outcomes. This button addresses only the fixture TextBox context; do not use
> system keyboard-layout switches, taskbar controls, global IME shortcuts, or
> settings in another window. An unavailable IME is an environment limitation,
> never proof that full-width mode was enabled. Continue the paste checks and
> report IME coverage separately.
>
> Enable already focuses the empty **paste target**: do not click it again.
> Invoke exactly **one** MCP text-input
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
text-evidence JSONL and metadata-only IME sidecar. Keep these responsibilities separate: the GUI operator must not use
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
requires queried open status plus `NATIVE | FULLSHAPE` at the actual paste/key
boundary, not merely immediately after Enable or after Check refocus. Prefer
WM_PASTE before/after records when present; otherwise inspect V preprocessing/key
boundaries with an available Ctrl modifier mask and correlate the first-attempt
MCP action. Missing WM_PASTE alone is not proof no paste occurred. Check-time state
remains useful but cannot establish when an intervening transition happened.
Unavailable/unsupported contexts, missing relevant events, or dropped records leave
coverage unverified rather than successful. IMM state is API evidence, not a claim
about every modern TSF IME's behavior. The fixture attempts to restore the saved
open/conversion/sentence state of the same TextBox context when closing; it never
switches global keyboard layouts. No native baseline is included, avoiding
unsafe multiline/native key side effects.

Record the actual compiler/self-test output and Windows GUI results; do not infer
that compilation, rendering, IMM behavior, or acceptance passed from this task
file or from a source-only review.
