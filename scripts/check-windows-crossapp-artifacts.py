#!/usr/bin/env python3
"""Supervisor-only read-only artifact diagnostic. Exit 0 is NOT GUI acceptance."""
import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "acceptance-fixture" / "crossapp"))
from crossapp_artifacts import check_artifacts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, help="Existing completed private pack root")
    args = parser.parse_args()
    try:
        result = check_artifacts(args.root)
    except (OSError, ValueError) as exc:
        print(json.dumps(dict(state="unknown", gui_verified=False, action_trial_gate_satisfied=False, error=str(exc))), file=sys.stderr)
        return 2
    print(json.dumps(result, ensure_ascii=True, indent=2))
    # Exit 0 means a diagnostic was produced. Inspect each state; no aggregate pass.
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
