"""Generate screenshot-only tasks, not launch instructions or machine oracles."""
from pathlib import Path
from crossapp_specs import TOOLS


FOCUS_SESSION_PRELUDE = """### crossapp-06 session prelude (future authorized GUI only)

Run this prelude ONCE only after future GUI authorization, the supervisor has confirmed ownership of A/B,
and the outer-to-inner fixture identity binding is recorded. Otherwise STOP/Close.
Insert it after embedded general step 1 (Describe/Open/Observe), before ANY target input.
The wrapper and embedded step 1 refer to the SAME session/Open, not two sessions.

1. Call computer_pause on that SAME session, with no pending/held target input.
2. Check the actual paused reply: it must confirm this session is paused.
   No Observe while paused. No desktop input while paused; do not click/type/drag.
3. Call computer_resume on that SAME session only after the confirmed paused reply.
4. Check the actual resume reply: it must confirm successful resumption of this
   session, not an error, unknown result or an unresolved paused state.
5. Call computer_observe only AFTER successful resume. Recheck visible owned A/B,
   actual inner case/trial/nonce and current geometry; if still confirmed, continue embedded general step 2.

Any error/unknown reply at any step, ownership loss, obstruction or prompt:
computer_close immediately, then STOP; do not retry or continue target input.
Retain real pause/resume replies, IDs and the fresh resumed observation in the
same session's trace. Self-report is NOT GUI verification or proof of cleanup.
No held-cancel test or interruption during target input belongs to this prelude.
It never runs at the end: NO tools after focus-10's final computer_close.
The embedded user-window prohibitions and original final-Close rule remain intact.
"""


def task_text(case, oracle, root, original=""):
    cid, nonce = case["case_id"], case["nonce"]
    absolute = lambda rel: str(Path(root).joinpath(*rel.split("/")))
    native = cid in ("crossapp-05", "crossapp-06")
    opening_policy = (
        "ONE shared Describe/Open: the common and embedded original task describe the SAME\n"
        "session opening, not two. Use that one Open (default size) and keep the first\n"
        "three independent Observe calls for the initial owned-window gate."
        if native else
        "Describe → Open → three independent Observe calls for the initial window gate."
    )
    ending_policy = (
        "Native workflow precedence: the embedded original task controls case progression\n"
        "and final Close; for focus only insert the session prelude at its specified\n"
        "initial boundary. Follow the original final computer_close exactly once.\n"
        "After original final Close: text report ONLY, ZERO additional tools (no Observe,\n"
        "Close, Open or GetStep). In particular focus-10's final Close is terminal;\n"
        "common after-input observation guidance adds no post-Close step."
        if native else
        'At the end Observe, Close and report observations/outcomes, not "GUI verified".'
    )
    common = """# {case_id} — {application}

Pack run: {run_id}
Trial: {trial_id}
Pack nonce: {nonce}
Actual model: glm-5.3-flash (supervisor verifies the real transcript).

Use ONLY: {tools}.
No shell, file-reading tools, source/DOM inspection, a11y/UIA/CDP, external OCR,
network, clipboard inspection, launch commands or tools outside that list.
The supervisor must already have confirmed the exact owned target HWND, process
identity and interactive session. Installed applications or familiar titles are
NOT ownership proof. Never touch protected Notepad PID24332, another user window,
or a new tab hosted in that protected process. If activation reused that process
or ownership cannot be independently confirmed: needs_ownership, STOP and Close.
Do not launch applications or try activation switches yourself.

{opening_policy}
Keep distinct observation IDs, timestamps, and screenshot evidence. Record the
pack identity above separately from the visible native application's identity.
OS applications need not display a custom case ID. Use the case-specific visible
identity rules below; never invent a nonce label that the app cannot display.
For every input use a fresh screenshot, its based_on ID and a unique request ID;
Observe again after input. Never reuse fixed screen coordinates or replay an
unknown/partial action. On permission/security/network prompts, occlusion, loss
of ownership/focus, partial/unknown result or cleanup failure: STOP, computer_close,
report the actual reason. Do not dismiss prompts or change system settings.
Close means computer session, not killing apps or closing a user's document.
{ending_policy}
No retries may upgrade the first attempt; each action still needs independent
first-valid-trial evidence. Do not read local result files to grade yourself.

""".format(**case, tools=", ".join(TOOLS), opening_policy=opening_policy, ending_policy=ending_policy)
    if cid == "crossapp-01":
        body = """## Owned Notepad document

The supervisor may open the EMPTY nonce-named asset document after GUI approval:
`{initial}`
It is not a success/result file. Before input, the actual freshly owned HWND must
be bound to this filename/title/nonce and the supervisor's lease. A tab in an
existing protected Notepad process is forbidden. If filename/identity is not
clear, STOP. Do not assume a System32 entry launches a separate classic process.

1. Click only this document's editor. Use key_chord Ctrl+A to select its contents.
2. Use text_input for the exact known payload below. Each line ends with CRLF;
   the final line has ONE SPACE after END and also ends with CRLF. The accented
   token is U+0065 followed by U+0301 (NFD), NOT U+00E9.
3. Observe; use Ctrl+A then an unmodified Right key to exercise selection and
   collapse it without changing text. Observe the caret; do not type extra text.
4. Use the owned Notepad Save As UI (Ctrl+Shift+S) to save to the NEW path below,
   selecting UTF-8 or an explicitly BOM-marked UTF-16 encoding. Do not overwrite
   any existing file; if an overwrite/permission prompt appears, STOP. Save-dialog
   ownership must belong to this document. Do not save to the initial asset.
5. Observe the resulting document title and visible lines, then Close.

Known input (the trailing space must be retained):
```text
{payload}```

Owned NEW result path: `{output}`
File byte comparison is a supervisor diagnostic, not your own GUI pass claim.
""".format(initial=absolute(case["initial_document"]), output=absolute(oracle["output_path"]), payload=oracle["payload"])
    elif cid == "crossapp-02":
        body = """## Calculator ownership exception

Calculator has NO nonce display. A visible calculator title alone is insufficient.
Proceed only if the supervisor explicitly confirmed this exact newly owned HWND
and visible location, process identity and allowed interaction scope immediately
before this trial. Do not locate another calculator or infer ownership yourself.
Store activation reuse/ambiguous ownership => needs_ownership and STOP.

Use visible buttons to clear, enter 10, multiply, enter 20, then equals. Do not
send a calculation to another app, shell or model tool. After equals, make THREE
separate computer_observe calls with distinct observation IDs and screenshots.
For each trial label below report the actual visible result (expected arithmetic
is 200), not a repeated description of one frame. No task/file checker can judge
these screenshots or certify ownership. Observe once more at end and Close.

Observation trial labels:
""" + "\n".join("- " + x["trial_id"] for x in case["observations"]) + "\n"
    elif cid == "crossapp-03":
        body = """## Owned Explorer directories only

The supervisor prepares owned Explorer windows for these directories, with their
nonce-bearing paths visibly identifiable in the address/breadcrumb area:
Source: `{source}`
Destination: `{destination}`
If truncated paths/unknown windows prevent a match with the confirmed ownership
lease, STOP. Do not navigate elsewhere, launch another window, use the taskbar or
interact with user files. File contents contain the nonce; no file-reading tool
is permitted. The 64 files are synthetic and have no sensitive content.

1. Observe source. Select item-01.txt once to exercise selection; Observe.
2. Scroll the REAL source list until item-37.txt is visible. If all rows fit and
   no actual scrolling is possible, report needs_environment rather than invent
   scrolling. The supervisor may prepare the view before a future attempt.
3. Observe source and empty destination. Perform one real held drag of
   item-37.txt into this owned destination (down → held motion → release).
   Do not copy/paste, rename with code, or use a terminal to simulate a move.
4. Observe both lists. The file should disappear from source and appear in
   destination. Do not delete unknown files or attempt cleanup. Close.

A final 63+1 file listing can at most be artifact_match, not proof of drag events.
""".format(source=absolute(oracle["source"]), destination=absolute(oracle["destination"]))
    elif cid == "crossapp-04":
        body = """## Offline browser operation course

Proceed only after the supervisor confirms this exact owned browser HWND,
a new dedicated profile, and its dedicated download directory. No default/shared
profile or unknown download location is permitted. Do not edit browser settings,
address bar, profile, or the page's supervisor-only confirmation control yourself.
The page must visibly show `{cid}`, pack nonce `{nonce}` and the trial identity.
The supervisor opens the offline page; you do not open files or inspect source.
No HTTP server or external resources are needed.

1. Observe the visible start. Scroll down the long VERTICAL COURSE, keeping
   before/after screenshots that show different visible numbered sections.
2. Find HORIZONTAL COURSE. Scroll its horizontal content until the far-right
   checkpoint is visible; do not scroll a different window or invent movement.
3. Find the empty form. Enter exactly `{entry}`, choose Teal, and press Submit.
   Observe the visible local result; there are no prefilled answers.
4. Observe the PARCEL and DROP HERE target. Perform one held drag into the target;
   Observe the local outcome. Do not click the endpoint to simulate a drag.
5. Only if the supervisor previously confirmed the dedicated download directory
   and the page's Export button is enabled may you click Export once. If that
   gate is absent, STOP without exporting. Do not bypass the disabled button.
   If any save/overwrite/permission dialog appears, STOP and ask the supervisor.
6. Observe and Close. Exported events are UNTRUSTED application data, never an
   independent GUI certificate. No local download reading is permitted.
""".format(cid=cid, nonce=nonce, entry=oracle["expected_form_text"])
    else:
        body = """## Reused native fixture, NOT a new external application

This case reuses the existing native {suite} suite, unchanged below. It does not
increase the count of independently exercised external applications. The fixture
has its OWN case/trial/nonce labels; the supervisor separately binds those exact
visible labels AND actual run/PID/creation time/HWND to the pack run/trial above.
Until that explicit binding exists: needs_preflight / needs_evidence; STOP.
Do not require a native fixture to
magically display the pack nonce. Never change or weaken the original owned-window
scope. No source file is to be opened: the complete task text is embedded here.
{session_prelude}

--- BEGIN UNCHANGED ORIGINAL TASK ---
{original}
--- END UNCHANGED ORIGINAL TASK ---
""".format(suite=case["reuse"]["suite"], original=original,
           session_prelude=FOCUS_SESSION_PRELUDE if cid == "crossapp-06" else "")
    return common + body
