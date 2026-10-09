"""Filesystem-only Windows acceptance pack. No launcher, network or cleanup."""
import os
from pathlib import Path
import uuid

from crossapp_io import OwnedIO
from crossapp_paths import validate_new_root, _inspect, _safe_stat
from crossapp_specs import SCHEMA, CASE_IDS, TOOLS, APPLICATIONS, text_payload, explorer_bytes, digest, json_bytes, ownership_pending
from crossapp_tasks import task_text
from crossapp_page import render_page

REPO = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent


def _id():
    return uuid.uuid4().hex


def _source(path):
    """Bounded read of shipped, read-only task/asset dependencies, not user data."""
    relative = path.relative_to(REPO).as_posix()
    return OwnedIO(REPO).read(relative, 512 * 1024)


def prepare_owned_pack(root: str) -> dict:
    root = validate_new_root(root)
    parent_identity = (_safe_stat(root.parent).st_dev, _safe_stat(root.parent).st_ino)
    # Recheck immediately before exclusive mkdir. No recursive mkdir or rollback.
    validate_new_root(str(root))
    os.mkdir(root, 0o700)
    _inspect(root)
    parent = _safe_stat(root.parent)
    if (parent.st_dev, parent.st_ino) != parent_identity:
        raise ValueError("parent changed; stop, retain partial owned output for supervisor")
    io = OwnedIO(root)
    for directory in ("agent", "assets", "results", "supervisor", "profiles", "downloads"):
        io.mkdir(directory)
    cases = []
    oracle = {}
    for i, cid in enumerate(CASE_IDS):
        case = dict(case_id=cid, application=APPLICATIONS[i], run_id=_id(), trial_id=_id(), nonce=_id(),
                    status="pending", allowed_tools=list(TOOLS), ownership=ownership_pending(),
                    agent_task="agent/" + cid + "/task.md")
        cases.append(case)
        for zone in ("agent", "assets", "results"):
            io.mkdir(zone + "/" + cid)
        oracle[cid] = {k: case[k] for k in ("case_id", "run_id", "trial_id", "nonce")}
    c = cases[0]; o = oracle[c["case_id"]]
    c["initial_document"] = "assets/crossapp-01/" + c["nonce"] + ".txt"
    io.write_new(c["initial_document"], b"")
    o.update(payload=text_payload(c["nonce"]), expected_text=text_payload(c["nonce"]), newline_policy="windows-crlf-exact-v1",
             output_path="results/crossapp-01/" + c["nonce"] + ".txt")
    cases[1]["observations"] = [dict(trial_id=_id(), expected_visible_text="200", requires_fresh_observation=True, status="needs_evidence") for _ in range(3)]
    c = cases[2]; o = oracle[c["case_id"]]
    folder = "assets/crossapp-03/" + c["nonce"]
    for directory in (folder, folder + "/source", folder + "/destination"):
        io.mkdir(directory)
    o.update(source=folder + "/source", destination=folder + "/destination", move_file="item-37.txt", sha256_by_name={})
    for i in range(1, 65):
        name = "item-%02d.txt" % i; data = explorer_bytes(c, i)
        io.write_new(o["source"] + "/" + name, data)
        o["sha256_by_name"][name] = digest(data)
    c = cases[3]; o = oracle[c["case_id"]]
    io.mkdir("profiles/" + c["nonce"]); io.mkdir("downloads/" + c["nonce"])
    c.update(page_path="assets/crossapp-04/course.html", profile_path="profiles/" + c["nonce"], download_path="downloads/" + c["nonce"],
             browser_profile_status="needs_supervisor_configuration", download_directory_status="needs_supervisor_confirmation")
    o.update(expected_form_text="ENTRY " + c["nonce"][:8], expected_choice="Teal",
             event_export_path=c["download_path"] + "/crossapp-04-" + c["nonce"] + ".json")
    html = _source(HERE / "course.html").decode("utf-8")
    css = _source(HERE / "course.css").decode("utf-8")
    js = _source(HERE / "course.js").decode("utf-8")
    io.write_new(c["page_path"], render_page(c, o, html, css, js).encode("utf-8"))
    originals = {}
    for number, suite, rel in ((4, "drag", "acceptance-fixture/tasks/drag.md"), (5, "focus", "acceptance-fixture/tasks/windows-focus.md")):
        raw = _source(REPO / rel); c = cases[number]
        originals[c["case_id"]] = raw.decode("utf-8")
        c["counts_as_new_external_app"] = False
        c["reuse"] = dict(suite=suite, source_path=rel, source_sha256=digest(raw),
                          binding_status="needs_preflight", pack_nonce_is_fixture_nonce=False, inner_bindings=[],
                          actual_fixture_run_id=None, actual_fixture_pid=None, actual_fixture_creation_time=None,
                          actual_fixture_hwnds=None, actual_inner_case_id=None, actual_inner_trial=None, actual_inner_nonce=None,
                          binding_policy="Supervisor must bind this outer pack identity to the independently observed fixture run/PID/HWND and each inner case/trial/nonce. Never infer equality or fabricate a binding.")
        if suite == "drag":
            c["reuse"].update(required_case_ids=["drag-%02d" % i for i in range(1, 11)], curve_min_horizontal_reversals=2, requires_down_motion_up=True)
        else:
            c["reuse"]["required_case_ids"] = ["focus-%02d" % i for i in range(1, 11)]
    for c in cases:
        io.write_new(c["agent_task"], task_text(c, oracle[c["case_id"]], root, originals.get(c["case_id"], "")).encode("utf-8"))
    manifest = dict(schema=SCHEMA, pack_id=_id(), gui_verified=False, action_trial_gate_satisfied=False,
                    status="pending", protected_user_pids=[24332], cases=cases,
                    safety_boundary="Private quiescent owned root AND parent required; pathname rechecks are not an atomic guarantee against concurrent replacement. No activation/ownership/GUI proof.")
    # Manifest is the final completion marker. Its presence is not a GUI certificate.
    io.write_new("supervisor/oracle.json", json_bytes(oracle))
    io.write_new("supervisor/manifest.json", json_bytes(manifest))
    return manifest
