#!/usr/bin/env python3
"""Layered GUI evidence analyzer (offline, diagnostic-only).

Cross-checks the coordinator-only oracle JSONL against a Claude CLI
stream-json transcript and performs a three-layer comparison per attempt:

  layer 1 (recognition/task-parameter): transcript payload vs catalog
          task_payload compared RAW and exactly for known cases (no
          normalization; a raw_payload_vs_task_payload deviation is always
          reported); oracle expected_text for unknown cases
  layer 2 (input/application): oracle actual_text vs normalize_newlines(payload)
  layer 3 (final, oracle-level): oracle expected_text == actual_text

`exact` is gated on ALL layers plus hex validation, matched flag and an
observed tool_result — a failing verdict can never produce a pass count.

Pairing is per case and only when provable: payloads are assigned to cases
via oracle trial intervals (payload ts within [trial ts, next trial ts)),
and a check is paired with a payload only when exactly one unconsumed
same-case payload can causally precede it (payload input ts <= check ts,
tool_result completion ts <= check ts when known). Same-second multiple
candidates, missing oracle ts, or missing payload ts degrade to unknown —
never to a blind global Nth-to-Nth pairing.

This is a text/oracle diagnostic. It does NOT replace a strict tool-policy
audit, does NOT prove screenshots looked correct, and retries inside one
case never upgrade first-attempt results. tool_result content is never
inspected or logged (it may embed image base64).
"""
from __future__ import annotations

import argparse
import bisect
import json
import re
import sys
from datetime import datetime, timedelta, timezone

MODEL_EXPECTED = "glm-5.3-flash"

DISCLAIMER = (
    "Text/oracle-based diagnostic only: it does NOT replace a strict tool-policy "
    "audit, does NOT prove screenshots looked correct, and retries within a case "
    "never upgrade first-attempt results."
)

ORACLE_EVENT_TYPES = ("session", "trial", "hit", "wrong", "text_check")


# ---------------------------------------------------------------- helpers

def normalize_newlines(text: str) -> str:
    """Mirror src/runtime/plan.rs normalize_newlines: CRLF -> LF once,
    lone CR and lone LF are kept. Pure, order/content preserving."""
    out = []
    i = 0
    n = len(text)
    while i < n:
        ch = text[i]
        if ch == "\r" and i + 1 < n and text[i + 1] == "\n":
            out.append("\n")
            i += 2
        else:
            out.append(ch)
            i += 1
    return "".join(out)


def utf16_code_units_hex(s: str) -> str:
    """UTF-16 code units as uppercase hex, space separated (matches the
    fixture's string.Join(\" \", c.ToString(\"X4\")) over ToCharArray())."""
    data = s.encode("utf-16-le", errors="surrogatepass")
    return " ".join(
        "%04X" % int.from_bytes(data[i : i + 2], "little") for i in range(0, len(data), 2)
    )


_HEX_SEP_RE = re.compile(r"[\s,]+")


def canon_hex(value):
    if not isinstance(value, str):
        return None
    return _HEX_SEP_RE.sub("", value).upper()


_TS_RE = re.compile(
    r"^(\d{4})-(\d{2})-(\d{2})[T ](\d{2}):(\d{2}):(\d{2})(?:\.(\d+))?(Z|[+-]\d{2}:?\d{2})?$"
)


def parse_ts(value):
    """Epoch seconds (float) or None. Accepts ISO-8601 with any fractional
    digit count (Windows 'o' emits 7, macOS ISO8601 emits none) and epoch
    numbers (seconds). Fractional digits beyond microseconds are truncated
    (conservative for ambiguity detection)."""
    if isinstance(value, bool):
        return None
    if isinstance(value, (int, float)):
        return float(value)
    if not isinstance(value, str):
        return None
    m = _TS_RE.match(value.strip())
    if not m:
        return None
    y, mo, d, hh, mm, ss = (int(m.group(i)) for i in range(1, 7))
    frac = (m.group(7) or "")[:6]
    micro = int(frac.ljust(6, "0")) if frac else 0
    tz = m.group(8)
    if tz in (None, "Z", "z"):
        offset = timezone.utc
    else:
        sign = 1 if tz[0] == "+" else -1
        body = tz[1:].replace(":", "")
        offset = timezone(sign * timedelta(hours=int(body[:2]), minutes=int(body[2:] or 0)))
    try:
        return datetime(y, mo, d, hh, mm, ss, micro, offset).timestamp()
    except ValueError:
        return None


def iter_jsonl(path, warnings, label):
    """Yield parsed JSON objects from a JSONL file, BOM-tolerant."""
    try:
        fh = open(path, "r", encoding="utf-8-sig", errors="replace")
    except OSError as exc:
        raise SystemExit("error: cannot read %s file %s: %s" % (label, path, exc))
    with fh:
        for line_no, line in enumerate(fh, 1):
            line = line.strip()
            if not line:
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError as exc:
                warnings.append("%s:%d: malformed JSON (%s)" % (label, line_no, exc))
                continue
            yield line_no, obj


# ---------------------------------------------------------------- oracle

def parse_oracle(path):
    events = []
    warnings = []
    session_suite = None
    for line_no, obj in iter_jsonl(path, warnings, "oracle"):
        if not isinstance(obj, dict):
            warnings.append("oracle:%d: non-object line skipped" % line_no)
            continue
        etype = obj.get("type")
        if etype == "session":
            if isinstance(obj.get("suite"), str):
                session_suite = obj["suite"]
            events.append({"type": "session", "ts": parse_ts(obj.get("ts")), "line": line_no})
            continue
        if etype not in ("trial", "hit", "wrong", "text_check"):
            warnings.append("oracle:%d: unknown event type %r skipped" % (line_no, etype))
            continue
        ev = {
            "type": etype,
            "ts": parse_ts(obj.get("ts")),
            "suite": obj.get("suite") if isinstance(obj.get("suite"), str) else None,
            "case_id": obj.get("case_id"),
            "trial": obj.get("trial"),
            "nonce": obj.get("nonce"),
            "line": line_no,
        }
        if ev["suite"] is None:
            ev["suite"] = session_suite or "unknown"
        if ev["case_id"] is None:
            warnings.append("oracle:%d: %s event without case_id" % (line_no, etype))
            ev["case_id"] = "?"
        if etype == "trial":
            ev["expected_text"] = obj.get("expected_text")
        for k in ("case_index", "case_total"):
            if k in obj:
                ev[k] = obj[k]
        if etype in ("hit", "wrong"):
            ev["slot"] = obj.get("slot")
            ev["target_slot"] = obj.get("target_slot")
        if etype == "text_check":
            for k in ("matched", "expected_text", "actual_text", "actual_utf16_hex", "expected_utf16_hex"):
                ev[k] = obj.get(k)
        events.append(ev)
    return events, warnings


# ---------------------------------------------------------------- transcript

def parse_transcript(path):
    """Parse a Claude CLI stream-json transcript. Only top-level assistant
    message content tool_use blocks are counted; tool_result content (which
    may embed duplicated tool_use JSON or image base64) is never traversed
    or logged."""
    tool_uses = []
    results = {}  # tool_use_id -> {"status": "present"|"empty", "ts": float|None}
    models = []
    other_actions = {}
    warnings = []
    assistant_events = 0
    missing_model_events = 0
    for line_no, obj in iter_jsonl(path, warnings, "transcript"):
        if not isinstance(obj, dict):
            continue
        mtype = obj.get("type")
        ts = parse_ts(obj.get("timestamp") or obj.get("ts"))
        if mtype == "assistant":
            assistant_events += 1
            msg = obj.get("message")
            if not isinstance(msg, dict):
                continue
            if isinstance(msg.get("model"), str):
                models.append(msg["model"])
            else:
                missing_model_events += 1
            content = msg.get("content")
            if not isinstance(content, list):
                continue
            for block in content:
                if not isinstance(block, dict) or block.get("type") != "tool_use":
                    continue
                tu = {
                    "id": block.get("id"),
                    "name": block.get("name"),
                    "ts": ts,
                    "result_ts": None,
                    "line": line_no,
                    "kind": None,
                    "text": None,
                }
                inp = block.get("input")
                action = inp.get("action") if isinstance(inp, dict) else None
                if isinstance(action, dict) and isinstance(action.get("kind"), str):
                    tu["kind"] = action["kind"]
                    if tu["kind"] == "text_input":
                        text = action.get("text")
                        if isinstance(text, str):
                            tu["text"] = text
                        else:
                            warnings.append(
                                "transcript:%d: text_input without string text" % line_no
                            )
                    else:
                        other_actions[tu["kind"]] = other_actions.get(tu["kind"], 0) + 1
                tool_uses.append(tu)
        elif mtype == "user":
            msg = obj.get("message")
            content = msg.get("content") if isinstance(msg, dict) else None
            if not isinstance(content, list):
                continue
            for block in content:
                if not isinstance(block, dict) or block.get("type") != "tool_result":
                    continue
                tid = block.get("tool_use_id")
                if not isinstance(tid, str):
                    continue
                # Intentionally do not inspect or log tool_result content:
                # it can embed image base64 and nested duplicated tool_use.
                c = block.get("content")
                results[tid] = {
                    "status": "empty" if (c is None or c == "" or c == []) else "present",
                    "ts": ts,
                }
    for tu in tool_uses:
        r = results.get(tu["id"])
        if r is not None:
            tu["result_ts"] = r.get("ts")
    return {
        "tool_uses": tool_uses,
        "results": results,
        "models": models,
        "other_actions": other_actions,
        "assistant_events": assistant_events,
        "missing_model_events": missing_model_events,
        "warnings": warnings,
    }


# ---------------------------------------------------------------- catalog

def load_catalog(path):
    with open(path, "r", encoding="utf-8-sig") as fh:
        obj = json.load(fh)
    raw = []
    if isinstance(obj, dict):
        if isinstance(obj.get("cases"), list):
            raw = obj["cases"]
        elif isinstance(obj.get("suites"), list):
            for s in obj["suites"]:
                if isinstance(s, dict):
                    for c in s.get("cases") or []:
                        if isinstance(c, dict):
                            cc = dict(c)
                            cc.setdefault("suite", s.get("suite"))
                            raw.append(cc)
        elif isinstance(obj.get("suites"), dict):
            for name, lst in obj["suites"].items():
                for c in lst or []:
                    if isinstance(c, dict):
                        cc = dict(c)
                        cc.setdefault("suite", name)
                        raw.append(cc)
    elif isinstance(obj, list):
        raw = obj
    cases = []
    for c in raw:
        if isinstance(c, dict) and c.get("case_id") is not None:
            cases.append(
                {
                    "suite": c.get("suite") if isinstance(c.get("suite"), str) else "unknown",
                    "case_id": str(c["case_id"]),
                    "task_payload": c.get("task_payload"),
                    "expected_text": c.get("expected_text"),
                }
            )
    return cases


# ---------------------------------------------------------------- analysis

def utf16_status(check):
    """'ok' | 'missing' | 'mismatch' for the oracle check's hex fields."""
    problems = []
    for text_key, hex_key in (("actual_text", "actual_utf16_hex"), ("expected_text", "expected_utf16_hex")):
        text = check.get(text_key)
        hexfield = check.get(hex_key)
        if hexfield is None:
            problems.append("%s missing" % hex_key)
            continue
        if not isinstance(text, str):
            problems.append("%s not a string" % text_key)
            continue
        if canon_hex(hexfield) != canon_hex(utf16_code_units_hex(text)):
            problems.append("%s does not match %s" % (hex_key, text_key))
    if any("missing" in p for p in problems):
        return "missing", problems
    if problems:
        return "mismatch", problems
    return "ok", []


def analyze_attempt(check, payload, result_status, catalog_case):
    """Build the layered comparison record for one proven (payload, check)."""
    actual = check.get("actual_text")
    oracle_expected = check.get("expected_text")
    matched = check.get("matched")
    hex_status, hex_problems = utf16_status(check)

    known = catalog_case is not None and isinstance(catalog_case.get("task_payload"), str)
    raw_task = catalog_case["task_payload"] if known else None
    if known:
        # Exact raw comparison: the agent must send the task payload as-is
        # (user requirement: precise input). No normalization here.
        expected_for_payload = raw_task
        expected_source = "catalog.task_payload(raw, exact)"
    else:
        expected_for_payload = oracle_expected if isinstance(oracle_expected, str) else None
        expected_source = "oracle.expected_text"

    payload_ok = isinstance(payload, str)
    normalized_payload = normalize_newlines(payload) if payload_ok else None
    layer1 = bool(payload_ok and expected_for_payload is not None and payload == expected_for_payload)
    layer2 = bool(
        payload_ok and isinstance(actual, str) and normalized_payload is not None and actual == normalized_payload
    )
    layer3 = bool(isinstance(oracle_expected, str) and isinstance(actual, str) and oracle_expected == actual)

    # Independent informational fields (known cases only)
    raw_payload_vs_task_payload = layer1 if known else None
    normalized_payload_vs_task_payload = (
        bool(payload_ok and normalize_newlines(payload) == normalize_newlines(raw_task))
        if known else None
    )

    result_status = {"present": "ok", "empty": "empty", None: "missing"}.get(result_status, "missing")

    failures = []
    if result_status != "ok":
        failures.append("result_%s" % result_status)
    if hex_status != "ok":
        failures.append("utf16_hex_%s" % hex_status)
    if matched is None:
        failures.append("oracle_matched_missing")
    elif matched is False and layer3:
        failures.append("oracle_inconsistent(matched=false but expected==actual)")
    elif matched is True and not layer3 and isinstance(oracle_expected, str) and isinstance(actual, str):
        failures.append("oracle_inconsistent(matched=true but expected!=actual)")
    if payload_ok and not layer1:
        failures.append(
            "raw_task_payload_deviation" if known else "payload_mismatch(unknown_case)"
        )
    if payload_ok and isinstance(actual, str) and not layer2:
        failures.append("input_divergence")

    # Fully gated: a failing verdict can never produce a pass count (F1).
    exact = bool(
        result_status == "ok"
        and hex_status == "ok"
        and matched is True
        and layer1
        and layer2
        and layer3
    )

    if not payload_ok:
        verdict = "malformed_payload"
    elif result_status != "ok":
        verdict = "missing_result" if result_status == "missing" else "empty_result"
    elif hex_status != "ok":
        verdict = "utf16_hex_%s" % hex_status
    elif matched is None:
        verdict = "oracle_matched_missing"
    elif not layer1:
        verdict = "payload_mismatch"
    elif not layer2:
        verdict = "input_divergence"
    elif not layer3:
        verdict = "oracle_mismatch"
    elif exact:
        verdict = "exact"
    else:
        verdict = "not_exact"

    return {
        "check_ts": check.get("ts"),
        "trial": check.get("trial"),
        "nonce": check.get("nonce"),
        "payload": payload,
        "payload_result": result_status,
        "expected_for_payload": expected_for_payload,
        "expected_source": expected_source,
        "raw_payload_vs_task_payload": raw_payload_vs_task_payload,
        "normalized_payload_vs_task_payload": normalized_payload_vs_task_payload,
        "normalized_payload": normalized_payload,
        "oracle_expected_text": oracle_expected,
        "oracle_actual_text": actual,
        "matched": matched,
        "layer1_payload_vs_expected": layer1,
        "layer2_actual_vs_normalized_payload": layer2,
        "layer3_expected_vs_actual": layer3,
        "utf16_hex_status": hex_status,
        "utf16_hex_problems": hex_problems,
        "exact": exact,
        "verdict": verdict,
        "failures": failures,
    }


def assign_payloads_to_cases(payloads, trials):
    """Assign each payload to a (suite, case_id) via oracle trial intervals.
    A payload belongs to the last trial whose ts <= payload ts; ties between
    trials at the same ts (or unknown payload ts) are unprovable -> None."""
    trials_with_ts = sorted((t for t in trials if t["ts"] is not None), key=lambda t: t["ts"])
    ts_list = [t["ts"] for t in trials_with_ts]
    unassignable = []
    for p in payloads:
        p["case_key"] = None
        if p["ts"] is None or not ts_list:
            unassignable.append(p)
            continue
        idx = bisect.bisect_right(ts_list, p["ts"]) - 1
        if idx < 0:
            unassignable.append(p)
            continue
        if idx > 0 and ts_list[idx] == ts_list[idx - 1]:
            unassignable.append(p)  # tie between trials: cannot prove
            continue
        t = trials_with_ts[idx]
        p["case_key"] = (t["suite"], t["case_id"])
    return unassignable


def build_report(oracle_path, transcript_path, catalog_path):
    warnings = []
    oracle_events, w = parse_oracle(oracle_path)
    warnings += w
    transcript = parse_transcript(transcript_path)
    warnings += transcript["warnings"]
    catalog_cases = load_catalog(catalog_path) if catalog_path else []
    catalog_by_key = {(c["suite"], c["case_id"]): c for c in catalog_cases}

    # ---- model check (F4: missing model is a failure, not silent)
    observed_models = sorted(set(transcript["models"]))
    model_ok = (
        observed_models == [MODEL_EXPECTED]
        and transcript["missing_model_events"] == 0
        and transcript["assistant_events"] > 0
    )
    if transcript["assistant_events"] == 0:
        model_ok = False
        warnings.append("transcript: no assistant events; model unverifiable")
    elif transcript["missing_model_events"]:
        warnings.append(
            "transcript: %d assistant event(s) without a model field"
            % transcript["missing_model_events"]
        )

    # ---- per-case pairing, only when provable
    inputs = [t for t in transcript["tool_uses"] if t["kind"] == "text_input"]
    checks = [e for e in oracle_events if e["type"] == "text_check"]
    trials = [e for e in oracle_events if e["type"] == "trial"]
    unassignable = assign_payloads_to_cases(inputs, trials)

    checks_by_case = {}
    for c in checks:
        checks_by_case.setdefault((c["suite"], c["case_id"]), []).append(c)
    payloads_by_case = {}
    for p in inputs:
        if p["case_key"] is not None:
            payloads_by_case.setdefault(p["case_key"], []).append(p)
    for key in payloads_by_case:
        payloads_by_case[key].sort(key=lambda p: (p["ts"], p["line"]))

    def oracle_check_validity(c):
        l3 = isinstance(c.get("expected_text"), str) and isinstance(c.get("actual_text"), str) \
            and c["expected_text"] == c["actual_text"]
        hs, _ = utf16_status(c)
        return {"layer3_expected_vs_actual": l3, "matched": c.get("matched"), "utf16_hex_status": hs}

    cases = {}
    case_keys = []
    for e in oracle_events:
        if e["type"] in ("trial", "hit", "wrong", "text_check"):
            k = (e["suite"], e["case_id"])
            if k not in case_keys:
                case_keys.append(k)
    for k in sorted(catalog_by_key):
        if k not in case_keys:
            case_keys.append(k)
    # oracle-observed cases: only those with actual trial/hit/wrong/check events
    observed_keys = {
        (e["suite"], e["case_id"])
        for e in oracle_events
        if e["type"] in ("trial", "hit", "wrong", "text_check")
    }

    n_proven = 0
    n_unresolved = 0
    n_payloads_without_check = 0
    for key in case_keys:
        suite, case_id = key
        cat = catalog_by_key.get(key)
        chk_list = checks_by_case.get(key, [])
        P = payloads_by_case.get(key, [])
        trial_events = [e for e in oracle_events if e["type"] == "trial" and (e["suite"], e["case_id"]) == key]
        hits = [e for e in oracle_events if e["type"] == "hit" and (e["suite"], e["case_id"]) == key]
        wrongs = [e for e in oracle_events if e["type"] == "wrong" and (e["suite"], e["case_id"]) == key]
        has_oracle_events = any(
            e["type"] in ("trial", "hit", "wrong", "text_check")
            and (e["suite"], e["case_id"]) == key
            for e in oracle_events
        )

        attempts = []
        unresolved = []
        # merged check-file-order sequence: proven records and unresolved marks
        sequence = []
        unconsumed = {id(p) for p in P}
        for c in chk_list:
            if c["ts"] is None:
                n_unresolved += 1
                u = dict(oracle_check_validity(c), reason="missing_oracle_ts",
                         check_ts=None, trial=c.get("trial"), nonce=c.get("nonce"),
                         candidate_count=len(unconsumed))
                unresolved.append(u)
                sequence.append({"proven": None, "unresolved": u})
                continue
            cands = [
                p for p in P
                if id(p) in unconsumed
                and p["ts"] <= c["ts"]
                and not (p["result_ts"] is not None and p["result_ts"] > c["ts"])
            ]
            if len(cands) == 1:
                p = cands[0]
                unconsumed.discard(id(p))
                n_proven += 1
                rec = analyze_attempt(
                    c, p["text"],
                    transcript["results"].get(p["id"], {}).get("status"),
                    cat,
                )
                rec["alignment"] = "proven"
                rec["payload_tool_use_id"] = p["id"]
                rec["payload_ts"] = p["ts"]
                attempts.append(rec)
                sequence.append({"proven": rec, "unresolved": None})
            elif len(cands) == 0:
                n_unresolved += 1
                u = dict(oracle_check_validity(c), reason="no_candidate_payload",
                         check_ts=c["ts"], trial=c.get("trial"), nonce=c.get("nonce"),
                         candidate_count=0)
                unresolved.append(u)
                sequence.append({"proven": None, "unresolved": u})
            else:
                n_unresolved += 1
                u = dict(oracle_check_validity(c), reason="ambiguous_candidates",
                         check_ts=c["ts"], trial=c.get("trial"), nonce=c.get("nonce"),
                         candidate_count=len(cands))
                unresolved.append(u)
                sequence.append({"proven": None, "unresolved": u})
        payloads_without_check = [
            {"tool_use_id": p["id"], "text": p["text"], "ts": p["ts"]}
            for p in P if id(p) in unconsumed
        ]
        n_payloads_without_check += len(payloads_without_check)

        # first/final over the merged sequence (F2 follow-up): an unknown
        # check at the start or end must not be upgraded or inherited.
        first_exact = None
        final_exact = None
        if sequence:
            first_exact = sequence[0]["proven"]["exact"] if sequence[0]["proven"] else None
            final_exact = sequence[-1]["proven"]["exact"] if sequence[-1]["proven"] else None

        flags = []
        if not has_oracle_events:
            flags.append("missing_from_oracle")
        if not chk_list and has_oracle_events:
            flags.append("no_text_check")
        if unresolved:
            flags.append("unresolved_checks")
        if payloads_without_check:
            flags.append("payload_without_check")

        case_failures = []
        for idx, a in enumerate(attempts, 1):
            for f in a.get("failures", []):
                case_failures.append({"attempt": idx, "class": f})
        for u in unresolved:
            case_failures.append({"attempt": None, "class": "unresolved_alignment:%s" % u["reason"]})

        cases["%s::%s" % key] = {
            "suite": suite,
            "case_id": case_id,
            "in_catalog": cat is not None,
            "case_index": (trial_events[0].get("case_index") if trial_events else None),
            "case_total": (trial_events[0].get("case_total") if trial_events else None),
            "known": cat is not None and isinstance(cat.get("task_payload"), str),
            "attempts": len(attempts),
            "attempt_records": attempts,
            "first_attempt_exact": first_exact,
            "final_exact": final_exact,
            "hit_count": len(hits),
            "wrong_count": len(wrongs),
            "trial_count": len(trial_events),
            "unresolved_checks": unresolved,
            "check_sequence": [
                {"proven_exact": (e["proven"]["exact"] if e["proven"] else None),
                 "unresolved_reason": (e["unresolved"]["reason"] if e["unresolved"] else None)}
                for e in sequence
            ],
            "payloads_without_check": payloads_without_check,
            "alignment_flags": flags,
            "failures": case_failures,
            "missing": not has_oracle_events,
        }

    # ---- suite summaries
    suite_names = []
    for key in case_keys:
        if key[0] not in suite_names:
            suite_names.append(key[0])
    suites = {}
    for suite in suite_names:
        suite_cases = [c for c in cases.values() if c["suite"] == suite]
        catalog_ids = {c["case_id"] for c in catalog_cases if c["suite"] == suite}
        missing_ids = sorted(c["case_id"] for c in suite_cases if c["in_catalog"] and c["missing"])
        suites[suite] = {
            "case_count": len(catalog_ids) if catalog_ids else len(suite_cases),
            "observed_case_count": len({k[1] for k in observed_keys if k[0] == suite}),
            "attempts": sum(c["attempts"] for c in suite_cases),
            "first_attempt_exact": sum(1 for c in suite_cases if c["first_attempt_exact"] is True),
            "final_exact": sum(1 for c in suite_cases if c["final_exact"] is True),
            "hit_count": sum(c["hit_count"] for c in suite_cases),
            "wrong_count": sum(c["wrong_count"] for c in suite_cases),
            "missing": missing_ids,
            "unknown_cases": sorted(c["case_id"] for c in suite_cases if not c["in_catalog"]),
            "unresolved_checks": sum(len(c["unresolved_checks"]) for c in suite_cases),
            "failures": [dict(f, case_id=c["case_id"]) for c in suite_cases for f in c["failures"]],
            "has_unresolved_alignment": any(c["unresolved_checks"] for c in suite_cases),
        }

    missing_result_ids = [
        t["id"] for t in transcript["tool_uses"]
        if t["kind"] == "text_input" and t["id"] not in transcript["results"]
    ]

    return {
        "generator": "scripts/analyze-layered-gui.py",
        "kind": "layered-gui-evidence-diagnostic",
        "disclaimer": DISCLAIMER,
        "model_check": {
            "expected": MODEL_EXPECTED,
            "observed": observed_models,
            "missing_model_assistant_events": transcript["missing_model_events"],
            "ok": model_ok,
        },
        "transcript_stats": {
            "top_level_tool_use_count": len(transcript["tool_uses"]),
            "text_input_count": len(inputs),
            "other_action_counts": transcript["other_actions"],
            "text_input_missing_tool_result_ids": missing_result_ids,
        },
        "oracle_stats": {
            "text_check_count": len(checks),
            "proven_pairs": n_proven,
            "unresolved_checks": n_unresolved,
            "unassignable_payloads": [
                {"tool_use_id": p["id"], "text": p["text"], "ts": p["ts"]} for p in unassignable
            ],
            "payloads_without_check": n_payloads_without_check,
        },
        "alignment": {
            "proven_pairs": n_proven,
            "unresolved_checks": n_unresolved,
            "note": "pairing is per case via trial intervals + input/result timestamps; "
                    "unprovable alignments are unknown, never blind Nth-to-Nth",
        },
        "suites": suites,
        "cases": cases,
        "warnings": warnings,
    }


# ---------------------------------------------------------------- CLI

def main(argv=None):
    ap = argparse.ArgumentParser(
        description="Offline layered GUI evidence analyzer (diagnostic-only; coordinator-run)."
    )
    ap.add_argument("--oracle", required=True, help="coordinator-only oracle JSONL path")
    ap.add_argument("--transcript", required=True, help="Claude CLI stream-json transcript path")
    ap.add_argument("--output", required=True, help="JSON report output path")
    ap.add_argument("--catalog", help="optional suite/case catalog JSON (task_payload/expected_text)")
    args = ap.parse_args(argv)

    report = build_report(args.oracle, args.transcript, args.catalog)
    with open(args.output, "w", encoding="utf-8") as fh:
        json.dump(report, fh, ensure_ascii=False, indent=2)
        fh.write("\n")

    print("layered GUI diagnostic report written to %s" % args.output)
    print("disclaimer: %s" % DISCLAIMER)
    for suite, s in report["suites"].items():
        print(
            "suite %s: case_count=%s attempts=%s first_attempt_exact=%s final_exact=%s "
            "hit=%s wrong=%s missing=%s unresolved_checks=%s"
            % (
                suite,
                s["case_count"],
                s["attempts"],
                s["first_attempt_exact"],
                s["final_exact"],
                s["hit_count"],
                s["wrong_count"],
                len(s["missing"]),
                s["unresolved_checks"],
            )
        )
    if not report["model_check"]["ok"]:
        print("WARNING: model check FAILED (expected %s, observed %s, missing-model events %s)" % (
            report["model_check"]["expected"],
            report["model_check"]["observed"],
            report["model_check"]["missing_model_assistant_events"]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
