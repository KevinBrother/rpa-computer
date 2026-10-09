Real Windows GUI calculator test. Only computer_* tools, screenshot-driven mouse/keyboard; no shell, Run dialog commands, files, browser, APIs, or source code. No mental arithmetic as evidence.
1. Open computer session max_width=960,max_height=540 and observe.
2. Open Windows Start/search by GUI and search for the Calculator application by name (English or Chinese according to UI). Launch only the Calculator app via visible search results. Do not execute command text in a terminal or Run dialog. If another application's content appears, do not modify it.
3. Through calculator buttons or key chords, clear the calculator, enter 10 multiplied by 20 and evaluate. Use text_input only for plain text/search; prefer individual calculator buttons for the expression.
4. Capture/inspect a screenshot with the displayed calculator result. Report the number actually visible, plus what you did. If no result can be verified, report failure, not an inferred answer.
5. Close ONLY the Calculator window you just opened (using its visible close button). Do not close other windows. computer_close.
Use latest observation_id in based_on and unique request_id each step. Observe after launches/transitions when needed, never reuse stale screenshots or invent coordinates. Bound to one task and 30 turns.
