# Correct independent diagnosis review with additional evidence
Write ONLY .agents/reviews/unicode-diagnosis-20260929.md. No GUI/SSH/source changes. GLM only. Read original and correct factual errors:
1. Same exact short text succeeds 6 subsequent batch cases:09 and13x5. 08/10 are DIFFERENT texts. Compute counts from JSON.
2. Short native char modes use13 total SendInput calls, batch uses2; do not claim all use2 or products identical.
3. Original MCP multiline length compute from JSON, do not guess34.
4. queue_char also runs for '\r', so NOT silently dropped; match arm empty only means no special key. Bulk sending is a strategy, not independently proven defect. Surrogate key-up mismatch not applies all Unicode/BMP.
5. New evidence: read .agents/runs/unicode-native-comparisons-20260929.json (18 cases,11match,7fail; all event arrays verified coordinator). Native long batch2/2fail, char0 1/2fail, char10 2/2fail. Noemoji-batch fails so newline/surrogate NOT necessary. Same observable corruption reproduced independent of MCP/Runtime/enigo; underlying root component still unknown, native evidence does not prove every product failure same cause.
6. Low-level keyboard hook is NOT proof of delivery to Notepad: avoid overstating boundary. GUITHREADINFO does not expose IME open state; don't claim it does. Next work should compare standard owned WinForms editor and Notepad, sample scoped foreground/thread/key/IME state via documented APIs. No broad keylogging of user desktop, no hooks/injection into user application now.
Keep concise <=100 lines. Must explicitly acknowledge broader native evidence supersedes initial short6 conclusions. No product fix or fully diagnosed claim.
