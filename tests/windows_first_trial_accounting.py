"""Frozen Stage A requirements. Execution belongs exclusively to CC + GLM.

Summary tests use independent fixed plan data, NEVER build_manifest in setUp.
NotImplementedError is a missing-capability FAIL; import/OS errors remain ERROR.
Synthetic records are accounting inputs only, never verified GUI evidence.
"""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest
import uuid

ROOT = Path(__file__).resolve().parents[1]
API_DIR = ROOT / "acceptance-fixture" / "trials"
SPEC = importlib.util.spec_from_file_location("first_trial_api", API_DIR / "__init__.py")
API = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(API)
CAMPAIGN = "c4105120-573e-4a35-8dd5-602d3fa12000"
COUNTS = {"move": 12, "click": 25, "drag": 10, "scroll": 14,
          "text_input": 10, "key_chord": 12, "key_hold": 12}
ROUNDS = {"pointer": 4, "multiclick": 1, "drag": 1, "scroll": 2,
          "keyboard": 3, "known-input": 1}


def fixture(mode="glm"):
    manifest = json.loads((API_DIR / "canonical-fixture.json").read_text(encoding="utf-8"))
    manifest["mode"] = mode
    manifest["required_model"] = "glm-5.3-flash" if mode == "glm" else None
    redigest(manifest)
    return manifest


def redigest(manifest):
    body = {k: v for k, v in manifest.items() if k != "plan_digest"}
    manifest["plan_digest"] = hashlib.sha256(json.dumps(
        body, ensure_ascii=True, sort_keys=True, separators=(",", ":"),
        allow_nan=False).encode("utf-8")).hexdigest()


def record(manifest, slot, attempt=1, state="matched"):
    """Explicitly synthetic identity; never acquired from a Windows session."""
    token = slot["slot_id"] + ":" + str(attempt)
    return {
        "schema": "windows-first-trial-record-v1", "record_id": "synthetic:" + token,
        "campaign_id": manifest["campaign_id"], "plan_digest": manifest["plan_digest"],
        "slot_id": slot["slot_id"], "plan_run_id": slot["plan_run_id"],
        "mode": manifest["mode"], "platform": "windows", "suite": slot["suite"],
        "case_id": slot["case_id"], "trial": slot["trial"], "attempt_index": attempt,
        "model": manifest["required_model"],
        "runtime": {"run_id": "synthetic-run:" + slot["plan_run_id"],
                    "session_id": "synthetic-session:" + slot["plan_run_id"],
                    "nonce": "synthetic-nonce:" + slot["slot_id"], "trial": slot["trial"]},
        "action": slot["action"], "state": state,
        "target_calls": [{"tool_call_id": "call:" + token, "request_id": "request:" + token,
                          "observation_id": "observation:" + token}],
        "evidence_refs": [{"path": "synthetic/not-real.json", "sha256": "0" * 64,
                           "kind": "trace"}],
    }


class Requirements(unittest.TestCase):
    def call(self, name, *args, **kwargs):
        try:
            return getattr(API, name)(*args, **kwargs)
        except NotImplementedError as exc:
            self.fail(f"MISSING_CAPABILITY[{name}]: {exc}")

    def reject(self, name, *args):
        with self.assertRaises(ValueError):
            self.call(name, *args)

    def assert_unverified(self, summary):
        self.assertIs(summary["gui_verified"], False)
        self.assertIs(summary["action_trial_gate_satisfied"], False)
        self.assertEqual(summary["evidence_status"], "supplied_claims_only")
        self.assertEqual(summary["independent_review_required"], [
            "raw_evidence_linkage", "actual_model_and_tool_policy",
            "screenshots_and_window_ownership"])
        self.assertNotIn("model_verified", summary)


class PlanRequirements(Requirements):
    def test_exact_canonical_manifest(self):
        self.assertEqual(self.call("build_manifest", CAMPAIGN), fixture())

    def test_cardinality_and_minimum_action_quota(self):
        m = self.call("build_manifest", CAMPAIGN)
        self.assertEqual(m["planned_slot_count"], 120)
        self.assertEqual(m["semantic_case_count"], 60)
        self.assertEqual(m["run_group_count"], 12)
        self.assertEqual(m["suite_rounds"], ROUNDS)
        self.assertEqual(len(m["groups"]), 12)
        self.assertEqual(len(m["slots"]), 120)
        self.assertEqual(len({s["case_id"] for s in m["slots"]}), 60)
        self.assertEqual(sum(s["eligible"] for s in m["slots"]), 95)
        self.assertEqual({a: sum(s["eligible"] and s["action"] == a for s in m["slots"])
                          for a in COUNTS}, COUNTS)
        for g in m["groups"]:
            self.assertEqual([s["trial"] for s in m["slots"]
                              if s["plan_run_id"] == g["plan_run_id"]], list(range(1, 11)))

    def test_stable_ids_and_no_runtime_forgery(self):
        m = self.call("build_manifest", CAMPAIGN)
        self.assertEqual(m, self.call("build_manifest", CAMPAIGN))
        for g in m["groups"]:
            self.assertEqual(g["plan_run_id"], str(uuid.uuid5(uuid.UUID(CAMPAIGN),
                             f'run:{g["suite"]}:{g["round_index"]}')))
            self.assertIsNone(g["runtime_run_id"])
            self.assertEqual(g["binding_state"], "needs_preflight")
            for s in (s for s in m["slots"] if s["plan_run_id"] == g["plan_run_id"]):
                self.assertEqual(s["slot_id"], str(uuid.uuid5(uuid.UUID(CAMPAIGN),
                    f'slot:{g["suite"]}:{g["round_index"]}:{s["case_id"]}')))
                self.assertIsNone(s["runtime"])
                self.assertEqual(s["binding_state"], "needs_preflight")
        other = self.call("build_manifest", "d4105120-573e-4a35-8dd5-602d3fa12000")
        self.assertTrue({s["slot_id"] for s in m["slots"]}.isdisjoint(
            {s["slot_id"] for s in other["slots"]}))

    def test_fresh_objects(self):
        m = self.call("build_manifest", CAMPAIGN)
        m["slots"].clear()
        self.assertEqual(self.call("build_manifest", CAMPAIGN), fixture())

    def test_separate_modes(self):
        self.assertEqual(self.call("build_manifest", CAMPAIGN, "deterministic"),
                         fixture("deterministic"))
        self.assertNotEqual(fixture()["plan_digest"], fixture("deterministic")["plan_digest"])

    def test_invalid_campaign_and_modes(self):
        for value in (None, True, 1, "", "../campaign", CAMPAIGN.upper(), "0" * 4097):
            with self.subTest(campaign=value):
                self.reject("build_manifest", value)
        for mode in (None, True, "windows", "GLM", "mixed"):
            with self.subTest(mode=mode):
                self.reject("build_manifest", CAMPAIGN, mode)

    def test_exact_classification_variants_and_native_text(self):
        m = self.call("build_manifest", CAMPAIGN)
        expected = fixture()["slots"]
        self.assertEqual(m["slots"], expected)
        cases = {s["case_id"]: s for s in m["slots"]}
        for n in (1, 2, 7, 8):
            self.assertEqual(cases[f"keyboard-{n:02d}"]["action"], "key_hold")
        for n in (3, 4, 5, 6):
            self.assertEqual(cases[f"keyboard-{n:02d}"]["action"], "key_chord")
        self.assertEqual(cases["known-09"]["payload"], "第一行\r\n第二行\r\n第三行")
        self.assertEqual(cases["known-09"]["native_text_policy"],
                         "windows-winforms-known09-crlf-v1")
        self.assertIn("\u00a0", cases["known-06"]["payload"])
        self.assertIn("\u0301", cases["known-08"]["payload"])
        self.assertIn("🦄", cases["known-07"]["payload"])
        self.assertIn("\t", cases["known-10"]["payload"])
        self.assertNotIn("key_press", m["actions"])
        self.assertTrue(m["coverage_limitations"])

    def test_source_reference_hashes(self):
        m = self.call("build_manifest", CAMPAIGN)
        self.assertEqual(m["sources"], fixture()["sources"])
        for source in m["sources"]:
            self.assertEqual(source["sha256"], hashlib.sha256(
                (ROOT / source["path"]).read_bytes()).hexdigest())


class SummaryRequirements(Requirements):
    def setUp(self):
        self.m = fixture()
        self.s = self.m["slots"][0]
        self.r = record(self.m, self.s)

    def summary(self, records, manifest=None):
        return self.call("summarize", self.m if manifest is None else manifest, records)

    def test_empty_preserves_all_missing_and_gate_false(self):
        out = self.summary([])
        self.assertEqual(out["planned_slot_count"], 120)
        self.assertEqual(out["semantic_case_count"], 60)
        self.assertEqual(out["run_group_count"], 12)
        self.assertEqual(len(out["slots"]), 120)
        self.assertTrue(all(s["first_state"] == "missing" for s in out["slots"]))
        self.assertEqual(out["reported_first_matches"], 0)
        self.assertIs(out["candidate_count_met"], False)
        self.assertEqual(out["duplicates"], [])
        self.assertEqual(out["conflicts"], [])
        for action, count in COUNTS.items():
            self.assertEqual(out["by_action"][action], {
                "planned_eligible": count, "reported_first_matches": 0, "quota": 10})
        self.assert_unverified(out)

    def test_exact_summary_schema_and_slot_shape(self):
        out = self.summary([self.r])
        self.assertEqual(set(out), {
            "schema", "campaign_id", "plan_digest", "mode", "platform",
            "planned_slot_count", "semantic_case_count", "run_group_count",
            "reported_first_matches", "candidate_count_met", "gui_verified",
            "action_trial_gate_satisfied", "by_action", "by_variant", "slots",
            "duplicates", "conflicts", "evidence_status", "independent_review_required",
            "coverage_limitations"})
        self.assertEqual(out["schema"], "windows-first-trial-summary-v1")
        for field in ("campaign_id", "plan_digest", "mode", "platform", "coverage_limitations"):
            self.assertEqual(out[field], self.m[field])
        self.assertEqual([s["slot_id"] for s in out["slots"]],
                         [s["slot_id"] for s in self.m["slots"]])
        self.assertEqual(out["slots"][0], {
            "slot_id": self.s["slot_id"], "first_state": "matched",
            "first_record_id": self.r["record_id"], "retry_record_ids": [],
            "reported_first_match": True})
        self.assertEqual(set(out["by_action"]), set(COUNTS))
        self.assert_unverified(out)

    def test_one_action_below_quota_does_not_meet_candidate_count(self):
        records = [record(self.m, s) for s in self.m["slots"]
                   if s["case_id"] != "known-10"]
        out = self.summary(records)
        self.assertEqual(out["reported_first_matches"], 94)
        self.assertIs(out["candidate_count_met"], False)
        self.assertEqual(out["by_action"]["text_input"]["reported_first_matches"], 9)
        self.assert_unverified(out)

    def test_complete_synthetic_candidates_never_gui_verified(self):
        out = self.summary([record(self.m, s) for s in self.m["slots"]])
        self.assertEqual(out["reported_first_matches"], 95)
        self.assertIs(out["candidate_count_met"], True)
        self.assertEqual({a: v["reported_first_matches"] for a, v in out["by_action"].items()}, COUNTS)
        self.assertEqual(sum(v["planned_slots"] for v in out["by_variant"].values()), 120)
        self.assertEqual(len(out["by_variant"]), 60)
        for s in self.m["slots"]:
            key = f'{s["suite"]}/{s["case_id"]}/{s["variant"]}'
            self.assertEqual(out["by_variant"][key]["planned_slots"], ROUNDS[s["suite"]])
        self.assert_unverified(out)

    def test_first_failure_not_upgraded_by_retry(self):
        first = record(self.m, self.s, state="failed")
        retry = record(self.m, self.s, attempt=2)
        out = self.summary([retry, first])
        self.assertEqual(out["reported_first_matches"], 0)
        self.assertEqual(out["slots"][0]["first_state"], "failed")
        self.assertEqual(out["slots"][0]["first_record_id"], first["record_id"])
        self.assertEqual(out["slots"][0]["retry_record_ids"], [retry["record_id"]])

    def test_missing_first_never_promotes_retry(self):
        retry = record(self.m, self.s, attempt=2)
        out = self.summary([retry])
        self.assertEqual(out["reported_first_matches"], 0)
        self.assertEqual(out["slots"][0]["first_state"], "missing")
        self.assertEqual(out["slots"][0]["retry_record_ids"], [retry["record_id"]])

    def test_nonmatched_states_never_count(self):
        for state in ("failed", "rejected", "zero", "blocked", "unknown"):
            with self.subTest(state=state):
                out = self.summary([record(self.m, self.s, state=state)])
                self.assertEqual(out["reported_first_matches"], 0)
                self.assertEqual(out["slots"][0]["first_state"], state)

    def test_noninput_and_coverage_only_never_count(self):
        records = [record(self.m, s) for s in self.m["slots"] if not s["eligible"]]
        self.assertEqual(len(records), 25)
        self.assertEqual(self.summary(records)["reported_first_matches"], 0)

    def test_check_focus_click_cannot_replace_target(self):
        for case in ("keyboard-01", "known-01", "pointer-01"):
            with self.subTest(case=case):
                s = next(s for s in self.m["slots"] if s["case_id"] == case)
                r = record(self.m, s)
                r["action"] = "click"
                self.assertEqual(self.summary([r])["reported_first_matches"], 0)

    def test_two_hold_calls_still_one_slot(self):
        s = next(s for s in self.m["slots"] if s["case_id"] == "keyboard-02")
        r = record(self.m, s)
        r["target_calls"].append({"tool_call_id": "tab-call", "request_id": "tab-request",
                                  "observation_id": "tab-observation"})
        out = self.summary([r])
        self.assertEqual(out["reported_first_matches"], 1)
        self.assertEqual(out["by_action"]["key_hold"]["reported_first_matches"], 1)

    def test_exact_duplicates_deduplicate(self):
        out = self.summary([self.r, copy.deepcopy(self.r), copy.deepcopy(self.r)])
        self.assertEqual(out["reported_first_matches"], 1)
        self.assertEqual(out["duplicates"], [self.r["record_id"]])

    def test_different_content_same_id_or_attempt_conflicts(self):
        for same_id in (True, False):
            with self.subTest(same_id=same_id):
                other = copy.deepcopy(self.r)
                other["state"] = "failed"
                if not same_id:
                    other["record_id"] = "different-record"
                for records in ([self.r, other], [other, self.r]):
                    out = self.summary(records)
                    self.assertEqual(out["reported_first_matches"], 0)
                    self.assertEqual(out["conflicts"], [self.s["slot_id"]])
                    self.assertEqual(out["slots"][0]["first_state"], "conflict")

    def test_cross_slot_tool_and_request_replay_conflicts_both(self):
        other = record(self.m, self.m["slots"][1])
        for field in ("tool_call_id", "request_id"):
            with self.subTest(field=field):
                replay = copy.deepcopy(other)
                replay["target_calls"][0][field] = self.r["target_calls"][0][field]
                out = self.summary([self.r, replay])
                self.assertEqual(out["reported_first_matches"], 0)
                self.assertEqual(out["conflicts"], sorted([self.s["slot_id"], other["slot_id"]]))

    def test_fixture_identity_reuse_between_groups_conflicts(self):
        s = self.m["slots"][10]  # same source trial in pointer round 2
        other = record(self.m, s)
        other["runtime"] = copy.deepcopy(self.r["runtime"])
        out = self.summary([self.r, other])
        self.assertEqual(out["reported_first_matches"], 0)
        self.assertEqual(out["conflicts"], sorted([self.s["slot_id"], s["slot_id"]]))

    def test_relabelled_record_cannot_split_one_target_call(self):
        other = record(self.m, self.m["slots"][1])
        other["target_calls"] = copy.deepcopy(self.r["target_calls"])
        out = self.summary([self.r, other])
        self.assertEqual(out["reported_first_matches"], 0)
        self.assertEqual(len(out["conflicts"]), 2)

    def test_retry_identity_drift_conflicts(self):
        other = record(self.m, self.s, attempt=2)
        other["runtime"]["nonce"] = "different-nonce"
        out = self.summary([self.r, other])
        self.assertEqual(out["reported_first_matches"], 0)
        self.assertEqual(out["conflicts"], [self.s["slot_id"]])

    def test_same_group_runtime_inconsistency_conflicts(self):
        other = record(self.m, self.m["slots"][1])
        other["runtime"]["run_id"] = "different-run"
        out = self.summary([self.r, other])
        self.assertEqual(out["reported_first_matches"], 0)
        self.assertEqual(len(out["conflicts"]), 2)

    def test_shared_observation_not_a_replayed_input(self):
        other = record(self.m, self.m["slots"][1])
        other["target_calls"][0]["observation_id"] = self.r["target_calls"][0]["observation_id"]
        self.assertEqual(self.summary([self.r, other])["reported_first_matches"], 2)

    def test_foreign_binding_and_modes_rejected(self):
        changes = {"schema": "other", "campaign_id": str(uuid.UUID(int=1)),
                   "plan_digest": "0" * 64, "slot_id": str(uuid.UUID(int=2)),
                   "plan_run_id": str(uuid.UUID(int=3)), "mode": "deterministic",
                   "platform": "macos", "model": "self-reported-glm",
                   "suite": "drag", "case_id": "pointer-02", "trial": 2}
        for field, value in changes.items():
            with self.subTest(field=field):
                r = copy.deepcopy(self.r)
                r[field] = value
                self.reject("summarize", self.m, [r])
        r = copy.deepcopy(self.r)
        r["runtime"]["trial"] = 2
        self.reject("summarize", self.m, [r])

    def test_deterministic_records_separate(self):
        m = fixture("deterministic")
        out = self.summary([record(m, m["slots"][0])], m)
        self.assertEqual(out["mode"], "deterministic")
        self.assertEqual(out["reported_first_matches"], 1)
        self.assert_unverified(out)
        self.reject("summarize", m, [self.r])

    def test_manifest_tampering_even_with_recomputed_digest(self):
        for field, value in (("quota_per_action", 1), ("platform", "macos"),
                             ("schema", "other"), ("planned_slot_count", 119),
                             ("mode", "deterministic")):
            with self.subTest(field=field):
                m = copy.deepcopy(self.m)
                m[field] = value
                redigest(m)
                self.reject("summarize", m, [])
        for mutation in ("remove", "duplicate", "eligibility", "variant", "source", "extra"):
            with self.subTest(mutation=mutation):
                m = copy.deepcopy(self.m)
                if mutation == "remove": m["slots"].pop()
                elif mutation == "duplicate": m["slots"][1] = copy.deepcopy(m["slots"][0])
                elif mutation == "eligibility": m["slots"][0]["eligible"] = False
                elif mutation == "variant": m["slots"][0]["variant"] = "made-up"
                elif mutation == "source": m["sources"][0]["sha256"] = "0" * 64
                else: m["gui_verified"] = True
                redigest(m)
                self.reject("summarize", m, [])

    def test_input_objects_not_mutated(self):
        records = [self.r]
        before = copy.deepcopy((self.m, records))
        self.summary(records)
        self.assertEqual((self.m, records), before)

    def test_unknown_fields_cannot_override_gates(self):
        for field in ("gui_verified", "action_trial_gate_satisfied", "model_verified", "eligible"):
            with self.subTest(field=field):
                r = copy.deepcopy(self.r)
                r[field] = True
                self.reject("summarize", self.m, [r])

    def test_strict_integer_fields_and_bounds(self):
        for field, values in (("attempt_index", (True, False, 0, 11, 1.0, "1", None)),
                              ("trial", (True, 0, 11, 1.0))):
            for value in values:
                with self.subTest(field=field, value=value):
                    r = copy.deepcopy(self.r)
                    r[field] = value
                    self.reject("summarize", self.m, [r])
        r = copy.deepcopy(self.r)
        r["runtime"]["trial"] = True
        self.reject("summarize", self.m, [r])

    def test_invalid_shapes_missing_identity_and_nonfinite(self):
        for records in (None, {}, (), [None], [True], [float("nan")], [float("inf")]):
            with self.subTest(records=repr(records)):
                self.reject("summarize", self.m, records)
        for field in self.r:
            with self.subTest(missing=field):
                r = copy.deepcopy(self.r)
                del r[field]
                self.reject("summarize", self.m, [r])
        for field in ("run_id", "session_id", "nonce"):
            r = copy.deepcopy(self.r)
            r["runtime"][field] = ""
            self.reject("summarize", self.m, [r])
        r = copy.deepcopy(self.r)
        r["target_calls"] = []
        self.reject("summarize", self.m, [r])

    def test_public_record_limit_accepts_full_plan_and_retries(self):
        records = [record(self.m, s, attempt) for s in self.m["slots"]
                   for attempt in range(1, 11)]
        out = self.summary(records)
        self.assertEqual(out["reported_first_matches"], 95)
        self.assertIs(out["candidate_count_met"], True)
        self.assert_unverified(out)
        self.reject("summarize", self.m, records + [copy.deepcopy(records[0])])

    def test_json_byte_limit_and_container_depth(self):
        records = [record(self.m, s, attempt) for s in self.m["slots"]
                   for attempt in range(1, 11)]
        for r in records:
            r["evidence_refs"] = [{"path": "x" * 1024, "sha256": "0" * 64,
                                   "kind": "trace"} for _ in range(16)]
        self.reject("summarize", self.m, records)
        r = copy.deepcopy(self.r)
        nested = "trace"
        for _ in range(9):
            nested = [nested]
        r["evidence_refs"][0]["kind"] = nested
        self.reject("summarize", self.m, [r])
        r = copy.deepcopy(self.r)
        r["evidence_refs"][0]["path"] = "\ud800"
        self.reject("summarize", self.m, [r])

    def test_string_list_and_evidence_bounds(self):
        mutations = [
            ("record_id", "x" * 129), ("record_id", "é"), ("state", "x" * 4097),
            ("target_calls", [copy.deepcopy(self.r["target_calls"][0])] * 17),
            ("evidence_refs", [copy.deepcopy(self.r["evidence_refs"][0])] * 17),
        ]
        for field, value in mutations:
            with self.subTest(field=field):
                r = copy.deepcopy(self.r)
                r[field] = value
                self.reject("summarize", self.m, [r])
        for path in ("../escape", "/absolute", "C:\\escape", "x" * 1025):
            r = copy.deepcopy(self.r)
            r["evidence_refs"][0]["path"] = path
            self.reject("summarize", self.m, [r])
        for digest in ("A" * 64, "0" * 63, "g" * 64):
            r = copy.deepcopy(self.r)
            r["evidence_refs"][0]["sha256"] = digest
            self.reject("summarize", self.m, [r])


if __name__ == "__main__":
    unittest.main(verbosity=2)
