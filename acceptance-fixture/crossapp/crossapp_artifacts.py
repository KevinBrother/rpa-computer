"""Strict, read-only artifact diagnostics. Never a GUI or ownership verifier."""
import codecs
import json
import re

from crossapp_io import OwnedIO, TooLarge
from crossapp_specs import SCHEMA, CASE_IDS, TOOLS, text_payload, explorer_bytes, digest
from crossapp_browser_check import check_browser_export


def check_notepad_bytes(actual: bytes, expected: str) -> dict:
    if not isinstance(actual, bytes) or not isinstance(expected, str):
        raise ValueError("Notepad comparison requires bytes and expected str")
    r = dict(state="unknown", encoding=None, actual_text=None, actual_sha256=None, gui_verified=False)
    if len(actual) > 64 * 1024:
        return dict(r, reason="text exceeds 64 KiB limit")
    r["actual_sha256"] = digest(actual)
    if actual.startswith((codecs.BOM_UTF32_LE, codecs.BOM_UTF32_BE)):
        return dict(r, reason="UTF-32 is not an accepted text encoding")
    if actual.startswith(codecs.BOM_UTF8):
        label, encoding, payload = "utf-8-sig", "utf-8", actual[3:]
    elif actual.startswith(codecs.BOM_UTF16_LE):
        label, encoding, payload = "utf-16-le-bom", "utf-16-le", actual[2:]
    elif actual.startswith(codecs.BOM_UTF16_BE):
        label, encoding, payload = "utf-16-be-bom", "utf-16-be", actual[2:]
    else:
        label, encoding, payload = "utf-8", "utf-8", actual
    r["encoding"] = label
    try:
        text = payload.decode(encoding, errors="strict")
    except UnicodeError:
        return dict(r, reason="strict byte decoding failed; no replacement decoding")
    return dict(r, actual_text=text, state="artifact_match" if text == expected else "mismatch",
                reason="exact decoded codepoints; original byte hash retained; no normalization or trimming")


def _object(pairs):
    result = {}
    for key, value in pairs:
        if key in result: raise ValueError("duplicate JSON key")
        result[key] = value
    return result


def _json(raw):
    try:
        value = json.loads(raw.decode("utf-8-sig"), object_pairs_hook=_object,
                           parse_constant=lambda _: (_ for _ in ()).throw(ValueError("nonfinite JSON number")))
    except (UnicodeError, RecursionError, json.JSONDecodeError) as exc:
        raise ValueError("invalid bounded metadata JSON") from exc
    if not isinstance(value, dict): raise ValueError("metadata must be an object")
    return value


def _metadata(io):
    try:
        m = _json(io.read("supervisor/manifest.json", 512 * 1024))
        oracle = _json(io.read("supervisor/oracle.json", 512 * 1024))
    except FileNotFoundError as exc:
        raise ValueError("unprepared/incomplete owned pack: supervisor metadata missing") from exc
    cases = m.get("cases")
    if m.get("schema") != SCHEMA or not isinstance(cases, list) or len(cases) != 6 or any(not isinstance(c, dict) for c in cases):
        raise ValueError("invalid six-case manifest")
    if [c.get("case_id") for c in cases] != CASE_IDS:
        raise ValueError("duplicate/missing/out-of-order case identities")
    for key in ("run_id", "trial_id", "nonce"):
        values = [c.get(key) for c in cases]
        if not all(isinstance(v, str) and re.fullmatch(r"[a-f0-9]{32}", v) for v in values) or len(set(values)) != 6:
            raise ValueError("invalid/nonunique case identity")
    for c in cases:
        o = oracle.get(c["case_id"])
        if not isinstance(o, dict) or any(o.get(k) != c[k] for k in ("case_id", "run_id", "trial_id", "nonce")):
            raise ValueError("oracle identity does not match case")
        if c.get("allowed_tools") != TOOLS:
            raise ValueError("tool scope differs from frozen case contract")
    # Validate ALL declared artifact paths before reading any application results.
    for cid, key in (("crossapp-01", "output_path"), ("crossapp-03", "source"), ("crossapp-03", "destination"), ("crossapp-04", "event_export_path")):
        io.path(oracle[cid].get(key))
    return cases, oracle


def _entry(case, state, reason, **extra):
    return dict({k: case[k] for k in ("case_id", "run_id", "trial_id", "nonce")}, state=state, reason=reason,
                gui_verified=False, ownership_verified=False, **extra)


def _notepad(io, case, oracle):
    expected = text_payload(case["nonce"])
    path = "results/crossapp-01/" + case["nonce"] + ".txt"
    if oracle.get("expected_text") != expected or oracle.get("payload") != expected or oracle.get("newline_policy") != "windows-crlf-exact-v1" or oracle.get("output_path") != path:
        return _entry(case, "unknown", "private expected policy differs from deterministic contract")
    try:
        raw = io.read(path, 64 * 1024)
    except FileNotFoundError:
        return _entry(case, "needs_evidence", "no saved owned result document")
    except TooLarge:
        return _entry(case, "unknown", "saved text exceeds byte limit")
    result = check_notepad_bytes(raw, expected)
    return _entry(case, result["state"], result["reason"], encoding=result["encoding"], actual_sha256=result["actual_sha256"], actual_text=result["actual_text"])


def _explorer(io, case, oracle):
    base = "assets/crossapp-03/" + case["nonce"]
    src, dst, moved = base + "/source", base + "/destination", "item-37.txt"
    expected = {"item-%02d.txt" % n: digest(explorer_bytes(case, n)) for n in range(1, 65)}
    if (oracle.get("source"), oracle.get("destination"), oracle.get("move_file")) != (src, dst, moved) or oracle.get("sha256_by_name") != expected:
        return _entry(case, "unknown", "Explorer private oracle differs from deterministic case", drag_verified=False)
    try:
        source_names, destination_names = io.names(src), io.names(dst)
        initial = source_names == sorted(expected) and destination_names == []
        final = source_names == sorted(set(expected) - {moved}) and destination_names == [moved]
        if not initial and not final:
            return _entry(case, "mismatch", "unexpected final inventory (includes copies/unknown files); nothing deleted", drag_verified=False)
        for name in expected:
            folder = dst if final and name == moved else src
            if digest(io.read(folder + "/" + name, 64 * 1024)) != expected[name]:
                return _entry(case, "mismatch", "owned file bytes changed: " + name, drag_verified=False)
        if io.names(src) != source_names or io.names(dst) != destination_names:
            return _entry(case, "unknown", "inventory changed during scan", drag_verified=False)
    except FileNotFoundError:
        return _entry(case, "unknown", "inventory disappeared during scan", drag_verified=False)
    except TooLarge:
        return _entry(case, "unknown", "bounded inventory/file size exceeded", drag_verified=False)
    return _entry(case, "needs_evidence" if initial else "artifact_match",
                  "initial files untouched" if initial else "exact 63+1 inventory and hashes; not proof that a drag occurred", drag_verified=False)


def _browser(io, case, oracle):
    expected = "downloads/" + case["nonce"] + "/crossapp-04-" + case["nonce"] + ".json"
    if oracle.get("event_export_path") != expected or oracle.get("expected_form_text") != "ENTRY " + case["nonce"][:8] or oracle.get("expected_choice") != "Teal":
        return _entry(case, "unknown", "browser oracle differs from deterministic contract")
    try:
        raw = io.read(expected, 256 * 1024)
    except FileNotFoundError:
        return _entry(case, "needs_evidence", "no browser application export; profile/download ownership still requires supervisor")
    except TooLarge:
        return _entry(case, "unknown", "browser export exceeds byte limit")
    try:
        data = _json(raw)
    except ValueError:
        return _entry(case, "unknown", "malformed browser application JSON; raw bytes preserved")
    names = io.names("downloads/" + case["nonce"])
    if names != [expected.split("/")[-1]]:
        return _entry(case, "mismatch", "unexpected downloads preserved; no overwrite/cleanup")
    state, reason = check_browser_export(data, case, oracle)
    return _entry(case, state, reason, application_data_only=True, export_sha256=digest(raw))


def check_artifacts(root: str) -> dict:
    io = OwnedIO(root)
    cases, oracle = _metadata(io)
    outputs = []
    for case in cases:
        cid = case["case_id"]
        if cid == "crossapp-01": entry = _notepad(io, case, oracle[cid])
        elif cid == "crossapp-03": entry = _explorer(io, case, oracle[cid])
        elif cid == "crossapp-04": entry = _browser(io, case, oracle[cid])
        else: entry = _entry(case, "needs_evidence", "requires independent screenshot/tool trace and owned-window identity; model/manifest claims ignored")
        outputs.append(entry)
    return dict(schema="windows-crossapp-artifact-diagnostic-v1", cases=outputs,
                gui_verified=False, action_trial_gate_satisfied=False, ownership_verified=False,
                disclaimer="Read-only filesystem/application-data diagnosis, not GUI certification. No startup/activation, input or cleanup performed. Concurrent mutation of root/ancestors is unsupported.")
