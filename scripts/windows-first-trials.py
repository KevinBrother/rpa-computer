#!/usr/bin/env python3
"""Read-only accounting CLI; emits ASCII-escaped JSON on every Windows codepage."""
import argparse
import importlib.util
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("first_trial_cli_api", ROOT / "acceptance-fixture/trials/__init__.py")
API = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(API)
# Resolve the already-loaded implementation's strict input helpers, no sys.path edits.
BOUNDS = sys.modules[API.build_manifest.__module__.rsplit(".", 1)[0] + ".bounds"]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    plan = commands.add_parser("plan", help="emit canonical preplanned Windows slots; no GUI execution")
    plan.add_argument("--campaign-id", required=True, help="canonical lowercase UUID")
    plan.add_argument("--mode", choices=("glm", "deterministic"), default="glm")
    summary = commands.add_parser("summary", help="count supplied records, never certify evidence")
    summary.add_argument("--manifest", required=True, type=Path)
    summary.add_argument("--records", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        if args.command == "plan":
            result = API.build_manifest(args.campaign_id, args.mode)
        else:
            manifest = BOUNDS.load_json_file(args.manifest, BOUNDS.MANIFEST_BYTES)
            records = BOUNDS.load_json_file(args.records, BOUNDS.RECORDS_BYTES)
            result = API.summarize(manifest, records)
        # ASCII JSON is codepage-independent and round-trips every Unicode scalar.
        # No output-file option; redirection and new evidence destinations belong to CC.
        sys.stdout.write(json.dumps(result, ensure_ascii=True, sort_keys=True,
                                    separators=(",", ":"), allow_nan=False) + "\n")
        return 0
    except (ValueError, OSError) as exc:
        # Escape diagnostics too, without echoing submitted records/evidence content.
        sys.stderr.write(json.dumps({"error": type(exc).__name__, "message": str(exc)},
                                    ensure_ascii=True) + "\n")
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
