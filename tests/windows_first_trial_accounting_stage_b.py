"""Additional StageB requirements. Only CC + GLM execute; author compiles only."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
TRIALS = ROOT / "acceptance-fixture/trials"
CLI = ROOT / "scripts/windows-first-trials.py"
CAMPAIGN = "c4105120-573e-4a35-8dd5-602d3fa12000"


def load_api():
    # Match frozen old36: intentionally do not insert facade in sys.modules.
    spec = importlib.util.spec_from_file_location("stage_b_unregistered_api", TRIALS / "__init__.py")
    api = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(api)
    return api


API = load_api()


def manifest():
    return json.loads((TRIALS / "canonical-fixture.json").read_text(encoding="utf-8"))


def claim(plan, index=0, attempt=1):
    slot = plan["slots"][index]
    token = f'{slot["slot_id"]}:{attempt}'
    return {
        "schema": "windows-first-trial-record-v1", "record_id": "synthetic:" + token,
        "campaign_id": plan["campaign_id"], "plan_digest": plan["plan_digest"],
        "slot_id": slot["slot_id"], "plan_run_id": slot["plan_run_id"],
        "mode": plan["mode"], "platform": plan["platform"], "model": plan["required_model"],
        "suite": slot["suite"], "case_id": slot["case_id"], "trial": slot["trial"],
        "attempt_index": attempt, "action": slot["action"], "state": "matched",
        "runtime": {"run_id": "synthetic-run:" + slot["plan_run_id"],
                    "session_id": "synthetic-session:" + slot["plan_run_id"],
                    "nonce": "synthetic-nonce:" + slot["slot_id"], "trial": slot["trial"]},
        "target_calls": [{"tool_call_id": "call:" + token, "request_id": "request:" + token,
                          "observation_id": "observe:" + token}],
        "evidence_refs": [{"path": "not-verified/no-file.json", "sha256": "0" * 64, "kind": "trace"}],
    }


class StageBApiRequirements(unittest.TestCase):
    def setUp(self):
        self.plan = manifest()
        self.record = claim(self.plan)

    def test_original_loader_without_global_sys_path_changes(self):
        before = list(sys.path)
        self.assertNotIn("stage_b_unregistered_api", sys.modules)
        second = load_api()
        self.assertEqual(second.build_manifest(CAMPAIGN), self.plan)
        self.assertEqual(sys.path, before)
        self.assertNotIn("stage_b_unregistered_api", sys.modules)

    def test_production_never_opens_canonical_fixture(self):
        # Source pin reads are expected; fixture/test reads as production answers are not.
        import builtins
        import io
        def guard(opener):
            def checked(path, *args, **kwargs):
                if isinstance(path, (str, os.PathLike)):
                    self.assertNotEqual(Path(path).resolve(), (TRIALS / "canonical-fixture.json").resolve())
                    self.assertNotIn("tests", Path(path).parts)
                return opener(path, *args, **kwargs)
            return checked
        with patch("io.open", side_effect=guard(io.open)), \
                patch("builtins.open", side_effect=guard(builtins.open)), \
                patch("os.open", side_effect=guard(os.open)):
            self.assertEqual(API.build_manifest(CAMPAIGN), self.plan)
            self.assertEqual(API.summarize(self.plan, [self.record])["reported_first_matches"], 1)

    def test_source_drift_and_source_unavailable_are_hard_errors(self):
        package = API.build_manifest.__module__.rsplit(".", 1)[0]
        catalog = sys.modules[package + ".catalog"]
        real_read = catalog.read_regular
        def drift(path, limit):
            return real_read(path, limit) + b"\nchanged by test in memory only"
        with patch.object(catalog, "read_regular", side_effect=drift):
            with self.assertRaisesRegex(ValueError, "source drift"):
                API.build_manifest(CAMPAIGN)
            with self.assertRaisesRegex(ValueError, "source drift"):
                API.summarize(self.plan, [])
        with patch.object(catalog, "read_regular", side_effect=PermissionError("test denial")):
            with self.assertRaisesRegex(ValueError, "source unavailable"):
                API.build_manifest(CAMPAIGN)

    def test_untrusted_manifest_and_evidence_paths_never_read(self):
        package = API.build_manifest.__module__.rsplit(".", 1)[0]
        catalog = sys.modules[package + ".catalog"]
        real_read, seen = catalog.read_regular, []
        def read(path, limit):
            seen.append(Path(path).relative_to(ROOT).as_posix())
            return real_read(path, limit)
        with patch.object(catalog, "read_regular", side_effect=read):
            out = API.summarize(self.plan, [self.record])
            poisoned = copy.deepcopy(self.plan)
            poisoned["sources"][0]["path"] = "../../untrusted.json"
            with self.assertRaises(ValueError):
                API.summarize(poisoned, [])
        self.assertEqual(set(seen), {s["path"] for s in self.plan["sources"]})
        self.assertEqual(out["evidence_status"], "supplied_claims_only")
        self.assertIs(out["gui_verified"], False)
        self.assertIs(out["action_trial_gate_satisfied"], False)

    def test_boolean_manifest_fields_cannot_equal_integer_fields(self):
        for field in ("trial", "eligible"):
            changed = copy.deepcopy(self.plan)
            changed["slots"][0][field] = True if field == "trial" else 1
            with self.subTest(field=field), self.assertRaises(ValueError):
                API.summarize(changed, [])

    def test_cycles_and_nonplain_values_are_rejected_not_recursion_errors(self):
        cyclic = []
        cyclic.append(cyclic)
        class CustomList(list):
            pass
        class CustomInt(int):
            pass
        for records in (cyclic, CustomList(), (self.record,), [float("nan")], [object()]):
            with self.subTest(value=type(records)), self.assertRaises(ValueError):
                API.summarize(self.plan, records)
        for value in (CustomInt(1), float("inf"), 1.0):
            r = copy.deepcopy(self.record)
            r["attempt_index"] = value
            with self.subTest(value=type(value)), self.assertRaises(ValueError):
                API.summarize(self.plan, [r])

    def test_record_and_nested_unknown_fields_fail_closed(self):
        for field in ("runtime", "target_calls", "evidence_refs"):
            r = copy.deepcopy(self.record)
            target = r[field] if field == "runtime" else r[field][0]
            target["gui_verified"] = True
            with self.subTest(field=field), self.assertRaises(ValueError):
                API.summarize(self.plan, [r])

    def test_primitive_record_states_actions_and_links_are_strict(self):
        for field, value in (("action", []), ("action", "key_press"), ("state", True),
                             ("state", "MATCHED"), ("runtime", None), ("target_calls", {}),
                             ("evidence_refs", {}), ("record_id", "")):
            r = copy.deepcopy(self.record)
            r[field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                API.summarize(self.plan, [r])

    def test_replay_and_conflict_are_input_order_independent(self):
        other = claim(self.plan, 1)
        other["target_calls"][0]["request_id"] = self.record["target_calls"][0]["request_id"]
        independent = claim(self.plan, 2)
        records = [self.record, other, independent, copy.deepcopy(independent)]
        first = API.summarize(self.plan, records)
        self.assertEqual(first, API.summarize(self.plan, list(reversed(records))))
        self.assertEqual(first["reported_first_matches"], 1)
        self.assertEqual(first["duplicates"], [independent["record_id"]])
        self.assertEqual(first["conflicts"], sorted([self.record["slot_id"], other["slot_id"]]))

    def test_same_record_id_across_slots_conflicts_all_parties(self):
        other = claim(self.plan, 1)
        other["record_id"] = self.record["record_id"]
        out = API.summarize(self.plan, [other, self.record])
        self.assertEqual(out["reported_first_matches"], 0)
        self.assertEqual(len(out["conflicts"]), 2)

    def test_cross_group_session_reuse_is_not_a_fresh_run(self):
        other = claim(self.plan, 10)
        other["runtime"]["session_id"] = self.record["runtime"]["session_id"]
        out = API.summarize(self.plan, [other, self.record])
        self.assertEqual(out["reported_first_matches"], 0)
        self.assertEqual(len(out["conflicts"]), 2)

    def test_nonce_reuse_different_trials_is_conflict(self):
        other = claim(self.plan, 1)
        other["runtime"]["nonce"] = self.record["runtime"]["nonce"]
        out = API.summarize(self.plan, [other, self.record])
        self.assertEqual(out["reported_first_matches"], 0)
        self.assertEqual(len(out["conflicts"]), 2)

    def test_blocked_without_target_call_stays_noninput(self):
        r = claim(self.plan, 8)
        r["state"], r["target_calls"] = "blocked", []
        out = API.summarize(self.plan, [r])
        self.assertEqual(out["reported_first_matches"], 0)
        self.assertEqual(out["slots"][8]["first_state"], "blocked")

    def test_foreign_campaign_rebinding_digest_cannot_promote_replay(self):
        r = copy.deepcopy(self.record)
        foreign = API.build_manifest("d4105120-573e-4a35-8dd5-602d3fa12000")
        r["campaign_id"], r["plan_digest"] = foreign["campaign_id"], foreign["plan_digest"]
        with self.assertRaises(ValueError):
            API.summarize(foreign, [r])

    def test_retry_order_and_no_first_overwrite(self):
        self.record["state"] = "failed"
        out = API.summarize(self.plan, [claim(self.plan, attempt=10), self.record,
                                        claim(self.plan, attempt=2)])
        self.assertEqual(out["slots"][0]["first_state"], "failed")
        self.assertEqual(out["slots"][0]["retry_record_ids"], [claim(self.plan, attempt=2)["record_id"],
                                                               claim(self.plan, attempt=10)["record_id"]])
        self.assertEqual(out["reported_first_matches"], 0)

    def test_new_known_input_task_payloads_decode_exactly(self):
        task = (ROOT / "acceptance-fixture/tasks/windows-known-input.md").read_text(encoding="utf-8")
        rows = [line for line in task.splitlines() if line.startswith("| known-")]
        self.assertEqual(len(rows), 10)
        decoded = [json.loads(line.split("`", 2)[1]) for line in rows]
        expected = [s["payload"] for s in self.plan["slots"] if s["suite"] == "known-input"]
        self.assertEqual(decoded, expected)
        self.assertIn("windows-winforms-known09-crlf-v1", task)
        self.assertIn("key_chord", task)


class StageBCliRequirements(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="first-trial-stage-b-test-")
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)

    def invoke(self, *args, encoding=None):
        env = os.environ.copy()
        env["PYTHONDONTWRITEBYTECODE"] = "1"
        if encoding:
            env["PYTHONIOENCODING"] = encoding
        return subprocess.run([sys.executable, "-B", str(CLI), *map(str, args)],
                              cwd=self.directory, env=env, capture_output=True, timeout=20,
                              check=False)

    def write(self, name, content):
        path = self.directory / name
        with path.open("xb") as stream:
            stream.write(content)
        return path

    def summary_args(self, raw_records=b"[]", raw_manifest=None):
        if raw_manifest is None:
            raw_manifest = json.dumps(manifest(), ensure_ascii=True).encode("ascii")
        plan_path = self.write("manifest.json", raw_manifest)
        records_path = self.write("records.json", raw_records)
        return ("summary", "--manifest", plan_path, "--records", records_path)

    def assert_error(self, result):
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertEqual(result.stdout, b"")
        self.assertTrue(result.stderr)

    def test_plan_windows_codepage_roundtrip_no_unicode_normalization(self):
        result = self.invoke("plan", "--campaign-id", CAMPAIGN, encoding="cp1252:strict")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr, b"")
        text = result.stdout.decode("ascii")
        self.assertEqual(json.loads(text), manifest())
        known = {s["case_id"]: s["payload"] for s in json.loads(text)["slots"]
                 if s["suite"] == "known-input"}
        self.assertIn("\u00a0", known["known-06"])
        self.assertIn("🦄", known["known-07"])
        self.assertIn("e\u0301", known["known-08"])
        self.assertNotIn("é", known["known-08"])
        self.assertEqual(known["known-09"], "第一行\r\n第二行\r\n第三行")
        self.assertEqual(known["known-10"], "姓名\t部门\t工号")

    def test_plan_deterministic_has_separate_digest(self):
        result = self.invoke("plan", "--campaign-id", CAMPAIGN, "--mode", "deterministic")
        self.assertEqual(result.returncode, 0, result.stderr)
        parsed = json.loads(result.stdout)
        self.assertEqual(parsed["mode"], "deterministic")
        self.assertIsNone(parsed["required_model"])
        self.assertNotEqual(parsed["plan_digest"], manifest()["plan_digest"])

    def test_empty_summary_and_input_bytes_preserved(self):
        args = self.summary_args()
        before = {p: p.read_bytes() for p in self.directory.iterdir()}
        result = self.invoke(*args)
        self.assertEqual(result.returncode, 0, result.stderr)
        out = json.loads(result.stdout)
        self.assertEqual(len(out["slots"]), 120)
        self.assertTrue(all(s["first_state"] == "missing" for s in out["slots"]))
        self.assertEqual(out["reported_first_matches"], 0)
        self.assertIs(out["candidate_count_met"], False)
        self.assertIs(out["gui_verified"], False)
        self.assertIs(out["action_trial_gate_satisfied"], False)
        self.assertEqual({p: p.read_bytes() for p in self.directory.iterdir()}, before)

    def test_malformed_truncated_nonfinite_and_duplicate_json_are_rejected(self):
        samples = (b"[{", b"[NaN]", b"[Infinity]", b"[1.0]", b"null", b"[] trailing",
                   b'[{"runtime":{"trial":1,"trial":2}}]', b"\xff", b'["\\ud800"]')
        for index, raw in enumerate(samples):
            plan_path = self.write(f"m-{index}.json", json.dumps(manifest()).encode("ascii"))
            record_path = self.write(f"r-{index}.json", raw)
            with self.subTest(raw=raw):
                self.assert_error(self.invoke("summary", "--manifest", plan_path, "--records", record_path))

    def test_manifest_duplicate_key_rejected_before_last_value_can_win(self):
        data = json.dumps(manifest()).encode("ascii")
        duplicate = b'{"quota_per_action":1,' + data[1:]
        self.assert_error(self.invoke(*self.summary_args(raw_manifest=duplicate)))

    def test_raw_byte_limit_applies_before_json_parse(self):
        oversized = b" " * (8 * 1024 * 1024) + b"[]"
        self.assert_error(self.invoke(*self.summary_args(raw_records=oversized)))

    def test_directory_and_missing_inputs_rejected(self):
        records = self.write("empty.json", b"[]")
        for plan in (self.directory, self.directory / "absent.json"):
            with self.subTest(path=plan):
                self.assert_error(self.invoke("summary", "--manifest", plan, "--records", records))

    def test_invalid_mode_campaign_and_output_option_rejected(self):
        for args in (("plan", "--campaign-id", "bad"),
                     ("plan", "--campaign-id", CAMPAIGN, "--mode", "mixed"),
                     ("plan", "--campaign-id", CAMPAIGN, "--output", str(self.directory / "no.json"))):
            with self.subTest(args=args):
                self.assert_error(self.invoke(*args))
        self.assertFalse((self.directory / "no.json").exists())

    def test_help_has_no_side_effects(self):
        result = self.invoke("--help")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(b"plan", result.stdout)
        self.assertIn(b"summary", result.stdout)
        self.assertEqual(list(self.directory.iterdir()), [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
