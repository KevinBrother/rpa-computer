"""Stable plan construction and strict canonical comparison."""
from hashlib import sha256
from uuid import UUID, uuid5

from .bounds import MANIFEST_BYTES, canonical_json
from .catalog import ACTIONS, COVERAGE_LIMITATION, SUITE_ROUNDS, semantic_cases, source_references


def build_manifest(campaign_id, mode="glm"):
    if type(campaign_id) is not str or len(campaign_id) != 36:
        raise ValueError("campaign_id must be a canonical UUID string")
    try:
        namespace = UUID(campaign_id)
    except ValueError as exc:
        raise ValueError("campaign_id must be a canonical UUID string") from exc
    if str(namespace) != campaign_id:
        raise ValueError("campaign_id must use lowercase hyphenated UUID spelling")
    if type(mode) is not str or mode not in ("glm", "deterministic"):
        raise ValueError("mode must be glm or deterministic")
    sources = source_references()
    groups, slots = [], []
    for suite, repetitions in SUITE_ROUNDS:
        for round_index in range(1, repetitions + 1):
            run_id = str(uuid5(namespace, f"run:{suite}:{round_index}"))
            groups.append({"plan_run_id": run_id, "suite": suite, "round_index": round_index,
                           "runtime_run_id": None, "binding_state": "needs_preflight"})
            for case in semantic_cases(suite):
                slots.append({**case, "plan_run_id": run_id,
                              "slot_id": str(uuid5(namespace, f'slot:{suite}:{round_index}:{case["case_id"]}')),
                              "runtime": None, "binding_state": "needs_preflight"})
    manifest = {
        "schema": "windows-first-trials-v1", "platform": "windows", "campaign_id": campaign_id,
        "mode": mode, "required_model": "glm-5.3-flash" if mode == "glm" else None,
        "quota_per_action": 10, "actions": list(ACTIONS), "suite_rounds": dict(SUITE_ROUNDS),
        "semantic_case_count": 60, "planned_slot_count": len(slots), "run_group_count": len(groups),
        "eligible_slot_count": sum(s["eligible"] for s in slots), "sources": sources,
        "groups": groups, "slots": slots, "coverage_limitations": [COVERAGE_LIMITATION],
    }
    manifest["plan_digest"] = sha256(canonical_json(manifest, MANIFEST_BYTES).encode("ascii")).hexdigest()
    return manifest


def validate_manifest(manifest):
    encoded = canonical_json(manifest, MANIFEST_BYTES)
    if type(manifest) is not dict or "campaign_id" not in manifest or "mode" not in manifest:
        raise ValueError("manifest must contain campaign_id and mode")
    canonical = build_manifest(manifest["campaign_id"], manifest["mode"])
    # Serialized equality distinguishes bool/int, unlike Python dict equality.
    if encoded != canonical_json(canonical, MANIFEST_BYTES):
        raise ValueError("manifest is not the canonical plan for this campaign/mode/source")
    return canonical
