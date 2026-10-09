# Windows owned geometry — screenshot-only GLM task

You are the GUI operator. The actual model must be `glm-5.3-flash`. Use ONLY these
computer MCP tools: `computer_describe`, `computer_open`, `computer_observe`,
`computer_step`, `computer_get_step`, `computer_pause`, `computer_resume`,
`computer_close`. No shell, files, browser automation, accessibility tree,
clipboard inspection, process enumeration, network tools or other tools.

Operate only the supervisor-prepared **Owned Windows Geometry** window. Each
attempt is a separately supervised case with the large visible geometry ID and
nonce. Read the visible instructions; report that exact ID/nonce in your final
answer. Never claim an input occurred merely because you intended it.

## First-frame gate

1. Call Describe. Open using the limits for the visible case specified below (the
   supervisor supplies only the intended visible case ID, not coordinates).
2. Observe. If the owned window, target, current ID or nonce is not clearly
   readable, **STOP and Close without input**. A permission prompt, locked screen,
   unknown foreground window or unexpected desktop means STOP. Do not dismiss
   prompts or change permissions. Never touch other applications or user windows.
3. Derive all coordinates from the returned image. Positions are image pixels,
   not OS screen pixels. Each step has a unique request ID and the exact most
   recent observation ID as `based_on`, except the explicitly supervised stale
   tests 07/08. After any dispatched/partial input, Observe again; do not blindly
   repeat an operation when output is incomplete. End every session with Close.

## Cases

- **01:** Open max_width=4096, max_height=4096. Observe and click the visible TARGET
  once, Observe, click Check, Observe, Close. This requests the supported upper
  bound, NOT a promise of unscaled pixels. Supervisor distinguishes scale 1 from
  needs_capability when the display exceeds the limit.
- **02:** Same procedure, Open max_width=1920, max_height=1920. This is a bounding
  constraint; the image need not be exactly 1920 pixels wide.
- **03:** Same procedure, Open max_width=1440, max_height=1440.
- **04:** Same procedure, Open max_width=1000, max_height=700. Do not calculate
  native coordinates or assume a fixed ratio; click only the observed target.
- **05:** Read the visible HWND DPI/coordinate basis. If DPI is not reliably >96,
  STOP and Close: DPI 96 is needs_environment; unknown/virtualized basis is
  needs_capability. Otherwise Observe, click TARGET once,
  Observe, Check, Observe, Close. Never resize the form to simulate high DPI.
- **06:** Pure negative-origin mapping, NOT a GUI trial. Do not input. Close and
  report not_gui. It is not proof of multiple physical monitors or the runtime.
- **07/08:** Requires prior supervisor/user authorization and a manually prepared
  resolution/DPI change. Do NOT change display settings yourself. Observe the
  target before the change; remember only this observation ID and image target
  position. Wait for the supervisor to confirm the actual change. If no such
  preparation occurs, Close with needs_environment. Submit exactly ONE click
  request with the **old** observation ID and old image position, expecting
  `geometry_changed`, `not_started`, zero events. If any input dispatch or other
  error occurs, STOP; never retry the click. Close the faulted session. Open a
  new session, Observe the changed environment, Close; do not click Check/Next.
  The fixture produces an automatic local check and heartbeat. Report the exact
  tool outcome, not "passed". The supervisor retains before/after proof.
- **09:** Observe and click the visible **REMOVE TARGET** button exactly once.
  Observe the target disappearing. Then STOP all input, including Check/Next,
  and Close. Do not click the old target position to probe it. If disappearance
  is uncertain, still STOP and report unknown. Removal is an application change,
  NOT necessarily a runtime topology change. Do not invent an expected runtime
  rejection. The local automatic check is not a global no-input certificate.
- **10:** Observe, click TARGET once, Observe, Check, Observe, Close. You must not
  decode or inspect screenshot data outside the computer tools. Real returned
  PNG validation is a separate supervisor activity, not your own claimed proof.

Do not move to another case yourself. The supervisor prepares Next between
independent sessions. After Close, send no more input. The supervisor collects
post-Close heartbeat evidence. If any result is partial/unknown or a screenshot
is missing, report that explicitly; do not silently convert it to success.
