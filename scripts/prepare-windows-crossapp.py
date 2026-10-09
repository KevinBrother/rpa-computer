#!/usr/bin/env python3
"""Supervisor-only filesystem preparation. Never launches an application."""
import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "acceptance-fixture" / "crossapp"))
from crossapp_pack import prepare_owned_pack


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, help="NEW absolute local private directory; existing roots are rejected")
    args = parser.parse_args()
    try:
        manifest = prepare_owned_pack(args.root)
    except (OSError, ValueError) as exc:
        print(json.dumps(dict(state="unknown", gui_verified=False, error=str(exc), partial_output="If created, retained for supervisor inspection; no rollback deletion")), file=sys.stderr)
        return 2
    print(json.dumps(dict(state="pending", gui_verified=False, action_trial_gate_satisfied=False,
                          pack_id=manifest["pack_id"], cases=len(manifest["cases"]), root=args.root,
                          ownership="needs_preflight", message="Prepared files only. Application activation and GUI remain unauthorized/unverified.")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
