"""Validate supplied claims and identify all parties to conflicts, never evidence files."""
from collections import defaultdict
from pathlib import PurePosixPath, PureWindowsPath
import re

from .bounds import MAX_RECORDS, RECORDS_BYTES, canonical_json
from .catalog import ACTIONS

RECORD_KEYS = frozenset((
    "schema", "record_id", "campaign_id", "plan_digest", "slot_id", "plan_run_id",
    "mode", "platform", "suite", "case_id", "trial", "attempt_index", "model",
    "runtime", "action", "state", "target_calls", "evidence_refs",
))
STATES = ("matched", "failed", "rejected", "zero", "blocked", "unknown")
DIGEST = re.compile(r"[0-9a-f]{64}\Z")


def exact_keys(value, keys, label):
    if type(value) is not dict or value.keys() != set(keys):
        raise ValueError(f"{label}: exact object fields required")


def identifier(value, label):
    if type(value) is not str or not value or len(value) > 128 or not value.isascii():
        raise ValueError(f"{label}: nonempty ASCII ID <=128 required")


def integer(value, label):
    if type(value) is not int or not 1 <= value <= 10:
        raise ValueError(f"{label}: integer 1..10 required (not boolean)")


def validate_evidence(refs):
    if type(refs) is not list or len(refs) > 16:
        raise ValueError("evidence_refs must be a list of at most 16 claims")
    for ref in refs:
        exact_keys(ref, ("path", "sha256", "kind"), "evidence ref")
        path = ref["path"]
        if type(path) is not str or not path or len(path) > 1024 or "\x00" in path:
            raise ValueError("invalid evidence reference path")
        posix, windows = PurePosixPath(path), PureWindowsPath(path)
        if (posix.is_absolute() or windows.drive or windows.root
                or ".." in posix.parts or ".." in windows.parts):
            raise ValueError("evidence reference must be relative without traversal")
        if type(ref["sha256"]) is not str or DIGEST.fullmatch(ref["sha256"]) is None:
            raise ValueError("evidence sha256 must be lowercase 64-digit hex")
        if type(ref["kind"]) is not str or ref["kind"] not in ("trace", "screenshot", "oracle", "transcript"):
            raise ValueError("invalid evidence reference kind")
        # Deliberately no exists/read/hash operation on this untrusted reference.


def validate_one(record, manifest, slots):
    exact_keys(record, RECORD_KEYS, "record")
    for key in ("record_id", "campaign_id", "plan_digest", "slot_id", "plan_run_id", "suite", "case_id"):
        identifier(record[key], key)
    if record["schema"] != "windows-first-trial-record-v1":
        raise ValueError("invalid record schema")
    integer(record["trial"], "trial")
    integer(record["attempt_index"], "attempt_index")
    slot = slots.get(record["slot_id"])
    if slot is None:
        raise ValueError("foreign slot")
    for field in ("campaign_id", "plan_digest", "mode", "platform"):
        if record[field] != manifest[field]:
            raise ValueError(f"record {field} does not bind to manifest")
    if record["model"] != manifest["required_model"]:
        raise ValueError("record model declaration does not match mode")
    for field in ("plan_run_id", "suite", "case_id", "trial"):
        if record[field] != slot[field]:
            raise ValueError(f"record {field} does not bind to slot")
    runtime = record["runtime"]
    exact_keys(runtime, ("run_id", "session_id", "nonce", "trial"), "runtime")
    for key in ("run_id", "session_id", "nonce"):
        identifier(runtime[key], "runtime " + key)
    integer(runtime["trial"], "runtime trial")
    if runtime["trial"] != slot["trial"]:
        raise ValueError("runtime trial does not bind to source trial")
    action = record["action"]
    if action is None:
        if slot["classification"] != "blocked":
            raise ValueError("null action allowed only on blocked slot")
    elif type(action) is not str or action not in ACTIONS:
        raise ValueError("invalid action kind")
    if type(record["state"]) is not str or record["state"] not in STATES:
        raise ValueError("invalid record state")
    calls = record["target_calls"]
    if type(calls) is not list or len(calls) > 16:
        raise ValueError("target_calls must be a list of at most 16 claims")
    if not calls and record["state"] != "blocked" and slot["classification"] != "blocked":
        raise ValueError("nonblocked records require target call linkage")
    for call in calls:
        exact_keys(call, ("tool_call_id", "request_id", "observation_id"), "target call")
        for key in call:
            identifier(call[key], key)
    validate_evidence(record["evidence_refs"])


def validated_records(manifest, records):
    if type(records) is not list or len(records) > MAX_RECORDS:
        raise ValueError("records must be a list of at most 1200")
    canonical_json(records, RECORDS_BYTES)
    slots = {s["slot_id"]: s for s in manifest["slots"]}
    unique, signatures, seen, duplicates = [], [], set(), set()
    for record in records:
        validate_one(record, manifest, slots)
        signature = canonical_json(record, RECORDS_BYTES)
        if signature in seen:
            duplicates.add(record["record_id"])
        else:
            seen.add(signature)
            unique.append(record)
            signatures.append(signature)
    return unique, signatures, sorted(duplicates)


def conflict_slots(records, signatures):
    """Index conflicts before counting: order never chooses a winning submission."""
    conflicts = set()

    def disagree(key, claim):
        groups = defaultdict(list)
        for record, signature in zip(records, signatures):
            groups[key(record)].append((record["slot_id"], claim(record, signature)))
        for members in groups.values():
            if len({claim for _, claim in members}) > 1:
                conflicts.update(slot for slot, _ in members)

    # Same logical submission with different contents, even if IDs are relabelled.
    disagree(lambda r: r["record_id"], lambda r, signature: signature)
    disagree(lambda r: (r["slot_id"], r["attempt_index"]), lambda r, signature: signature)
    # Within a planned run the actual run/session must be consistent.
    disagree(lambda r: r["plan_run_id"],
             lambda r, _: (r["runtime"]["run_id"], r["runtime"]["session_id"]))
    # All attempts of a slot must refer to the same native fixture identity.
    disagree(lambda r: r["slot_id"], lambda r, _: tuple(r["runtime"][k]
             for k in ("run_id", "session_id", "nonce", "trial")))
    # One real run/session cannot be rebound to another preplanned run.
    for field in ("run_id", "session_id"):
        disagree(lambda r, field=field: r["runtime"][field], lambda r, _: r["plan_run_id"])
    # A nonce is unique per slot within a fixture run; source trial is not an escape.
    disagree(lambda r: (r["plan_run_id"], r["runtime"]["nonce"]), lambda r, _: r["slot_id"])
    disagree(lambda r: tuple(r["runtime"][k] for k in ("run_id", "session_id", "nonce", "trial")),
             lambda r, _: r["slot_id"])
    for field in ("tool_call_id", "request_id"):
        owners = defaultdict(set)
        for record in records:
            for call in record["target_calls"]:
                owners[call[field]].add(record["slot_id"])
        for slots in owners.values():
            if len(slots) > 1:
                conflicts.update(slots)
    return conflicts
