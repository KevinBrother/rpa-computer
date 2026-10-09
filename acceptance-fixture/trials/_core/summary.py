"""Diagnostic counters of claims only. No GUI or evidence certification here."""
from collections import defaultdict

from .catalog import ACTIONS
from .plan import validate_manifest
from .records import conflict_slots, validated_records


def summarize(manifest, records):
    plan = validate_manifest(manifest)
    unique, signatures, duplicates = validated_records(plan, records)
    conflicts = conflict_slots(unique, signatures)
    by_slot = defaultdict(list)
    for record in unique:
        by_slot[record["slot_id"]].append(record)
    by_action = {a: {"planned_eligible": 0, "reported_first_matches": 0, "quota": 10}
                 for a in ACTIONS}
    by_variant, results = {}, []
    total = 0
    for slot in plan["slots"]:
        sid = slot["slot_id"]
        variant_key = f'{slot["suite"]}/{slot["case_id"]}/{slot["variant"]}'
        variant = by_variant.setdefault(variant_key, {"planned_slots": 0, "reported_first_matches": 0})
        variant["planned_slots"] += 1
        if slot["eligible"]:
            by_action[slot["action"]]["planned_eligible"] += 1
        attempts = sorted(by_slot[sid], key=lambda r: (r["attempt_index"], r["record_id"]))
        first = next((r for r in attempts if r["attempt_index"] == 1), None)
        conflicted = sid in conflicts
        candidate = bool(not conflicted and first is not None and slot["eligible"]
                         and first["state"] == "matched" and first["action"] == slot["action"])
        if candidate:
            total += 1
            by_action[slot["action"]]["reported_first_matches"] += 1
            variant["reported_first_matches"] += 1
        results.append({
            "slot_id": sid,
            "first_state": "conflict" if conflicted else "missing" if first is None else first["state"],
            "first_record_id": None if conflicted or first is None else first["record_id"],
            "retry_record_ids": [r["record_id"] for r in attempts if r["attempt_index"] > 1],
            "reported_first_match": candidate,
        })
    return {
        "schema": "windows-first-trial-summary-v1", "campaign_id": plan["campaign_id"],
        "plan_digest": plan["plan_digest"], "mode": plan["mode"], "platform": "windows",
        "planned_slot_count": 120, "semantic_case_count": 60, "run_group_count": 12,
        "reported_first_matches": total,
        "candidate_count_met": all(v["reported_first_matches"] >= 10 for v in by_action.values()),
        "gui_verified": False, "action_trial_gate_satisfied": False,
        "by_action": by_action, "by_variant": by_variant, "slots": results,
        "duplicates": duplicates, "conflicts": sorted(conflicts),
        "evidence_status": "supplied_claims_only",
        "independent_review_required": ["raw_evidence_linkage", "actual_model_and_tool_policy",
                                        "screenshots_and_window_ownership"],
        "coverage_limitations": plan["coverage_limitations"],
    }
