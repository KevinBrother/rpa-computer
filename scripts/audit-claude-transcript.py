#!/usr/bin/env python3
"""Audit a REAL Claude Code stream-json transcript from a black-box GUI
acceptance run (scripts/run-blackbox-claude.sh).

This audits actual recorded run evidence — never synthetic argv. It does NOT
run Claude and does NOT validate GUI behavior; it validates run policy:

  * exactly one `system`/`init` event, and its MCP server inventory is
    exactly the expected server ("computer") with status connected;
  * the effective tool inventory is EXACTLY the expected
    mcp__computer__computer_* tools (8) — any built-in (Read/Bash/Edit/...)
    or unexpected MCP tool invalidates the run;
  * hooks and slash commands are disabled for the invocation;
  * every assistant tool_use call is one of the allowed computer_* tools;
  * no permission-denial, no recorded hook execution, no API/model error,
    and no turn/time budget overrun;
  * a final `result` event exists and is not an error.

Fail closed: any missing init, missing/extra tool, unexpected server,
denied permission, or error event is a non-zero exit with a precise reason.

Usage:
  scripts/audit-claude-transcript.py TRANSCRIPT.jsonl \
      [--expect-server computer] [--max-turns N] [--max-seconds S]

Exit 0 = transcript passes the policy audit; 1 = audit failure; 2 = usage.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
from typing import Any, Dict, List, Optional, Set

EXPECTED_TOOLS: Set[str] = {
    "mcp__computer__computer_describe",
    "mcp__computer__computer_open",
    "mcp__computer__computer_observe",
    "mcp__computer__computer_step",
    "mcp__computer__computer_get_step",
    "mcp__computer__computer_pause",
    "mcp__computer__computer_resume",
    "mcp__computer__computer_close",
}

FAILURES: List[str] = []
CHECKS = 0


def fail(msg: str) -> None:
    FAILURES.append(msg)


def note(ok: bool, name: str, detail: str = "") -> None:
    global CHECKS
    CHECKS += 1
    if not ok:
        fail(f"{name}: {detail}" if detail else name)


def load_events(path: str) -> List[Dict[str, Any]]:
    events: List[Dict[str, Any]] = []
    with open(path, "r", encoding="utf-8") as fh:
        for lineno, line in enumerate(fh, 1):
            stripped = line.strip()
            if not stripped:
                continue
            try:
                obj = json.loads(stripped)
            except json.JSONDecodeError as e:
                fail(f"line {lineno} is not valid JSON: {e}")
                continue
            if isinstance(obj, dict):
                events.append(obj)
            else:
                fail(f"line {lineno} is not a JSON object")
    return events


def _is_init(ev: Dict[str, Any]) -> bool:
    return ev.get("type") == "system" and ev.get("subtype") == "init"


def _content_blocks(ev: Dict[str, Any]) -> List[Dict[str, Any]]:
    msg = ev.get("message")
    if not isinstance(msg, dict):
        return []
    content = msg.get("content")
    if isinstance(content, list):
        return [b for b in content if isinstance(b, dict)]
    return []


def _is_denial(block: Dict[str, Any]) -> bool:
    """A tool_result that records a permission denial (policy violation)."""
    if block.get("type") != "tool_result":
        return False
    if block.get("is_error") is not True:
        return False
    content = block.get("content")
    if isinstance(content, str):
        text = content
    elif isinstance(content, list):
        text = " ".join(
            b.get("text", "") for b in content if isinstance(b, dict)
        )
    else:
        text = ""
    lowered = text.lower()
    return "permission" in lowered and (
        "denied" in lowered or "reject" in lowered or "not allowed" in lowered
    )


def audit(path: str, expect_server: str, max_turns: Optional[int],
          max_seconds: Optional[float]) -> int:
    if not os.path.isfile(path):
        fail(f"transcript file missing: {path}")
        return 1
    if os.path.getsize(path) == 0:
        fail(f"transcript file is empty: {path} (no init -> invalid run)")
        return 1
    events = load_events(path)
    if not events:
        return 1

    inits = [ev for ev in events if _is_init(ev)]
    note(len(inits) >= 1, "stream-json init event present",
         "no system/init event found (invalid run)")
    note(len(inits) <= 1, "exactly one init event", f"found {len(inits)}")
    if not inits:
        return 1
    init = inits[0]

    # ---- tool inventory: exactly the 8 computer_* MCP tools ----------------
    tools = init.get("tools")
    note(isinstance(tools, list), "init.tools is a list", f"got {type(tools)}")
    tool_set = set(tools) if isinstance(tools, list) else set()
    note(
        tool_set == EXPECTED_TOOLS,
        "effective tool inventory is exactly the 8 computer_* tools",
        f"missing={sorted(EXPECTED_TOOLS - tool_set)} "
        f"extra={sorted(tool_set - EXPECTED_TOOLS)}",
    )

    # ---- MCP servers: exactly the expected one, connected ------------------
    servers = init.get("mcp_servers")
    note(isinstance(servers, list), "init.mcp_servers is a list",
         f"got {type(servers)}")
    server_names: Set[str] = set()
    if isinstance(servers, list):
        for srv in servers:
            if not isinstance(srv, dict):
                fail(f"malformed mcp_servers entry: {srv!r}")
                continue
            name = srv.get("name")
            server_names.add(str(name))
            status = srv.get("status")
            note(
                str(status).lower() in ("connected", "ready", "ok"),
                f"MCP server {name!r} connected",
                f"status={status!r}",
            )
    note(server_names == {expect_server},
         "only the expected MCP server is present",
         f"servers={sorted(server_names)} expected={expect_server!r}")

    # ---- per-invocation isolation flags recorded in init -------------------
    note(init.get("slash_commands") in (None, [], False)
         or init.get("slashCommands") in (None, [], False)
         or "slash_commands" not in init,
         "slash commands not advertised in init",
         f"slash_commands={init.get('slash_commands')!r}")

    # ---- walk the trace: every tool call must be an allowed computer_* -----
    tool_calls = 0
    computer_calls = 0
    hook_events = 0
    api_errors: List[str] = []
    result_events = 0
    result_is_error = False
    num_turns: Optional[int] = None
    duration_ms: Optional[float] = None
    denial_count = 0

    for ev in events:
        etype = ev.get("type")
        subtype = ev.get("subtype")
        if etype == "assistant":
            for block in _content_blocks(ev):
                if block.get("type") == "tool_use":
                    tool_calls += 1
                    name = block.get("name", "")
                    if name in EXPECTED_TOOLS:
                        computer_calls += 1
                    else:
                        fail(f"unexpected tool call: {name!r} "
                             f"(only mcp__computer__computer_* allowed)")
        elif etype == "user":
            for block in _content_blocks(ev):
                if _is_denial(block):
                    denial_count += 1
        elif etype == "system" and subtype == "hook_response":
            hook_events += 1
        elif etype == "result":
            result_events += 1
            result_is_error = bool(ev.get("is_error"))
            if isinstance(ev.get("num_turns"), int):
                num_turns = ev["num_turns"]
            if isinstance(ev.get("duration_ms"), (int, float)):
                duration_ms = ev["duration_ms"]
            if subtype and "error" in str(subtype):
                api_errors.append(f"result subtype={subtype}")
        elif etype == "error" or (etype == "system" and subtype == "error"):
            api_errors.append(json.dumps(ev)[:300])
        elif isinstance(ev.get("is_error"), bool) and ev.get("is_error") and etype != "result":
            # top-level error envelopes that are not the final result
            api_errors.append(json.dumps(ev)[:300])

    note(denial_count == 0, "no permission-denial tool results",
         f"{denial_count} denial(s) recorded")
    note(hook_events == 0, "no hook executions recorded",
         f"{hook_events} hook_response event(s)")
    note(tool_calls > 0, "at least one tool call recorded",
         "trace contains no tool calls")
    note(tool_calls == computer_calls, "all tool calls are computer_*",
         f"{tool_calls - computer_calls} non-computer call(s)")

    note(result_events == 1, "exactly one final result event",
         f"found {result_events}")
    note(not result_is_error, "final result is not an error",
         "result.is_error=true")
    note(not api_errors, "no API/model error events",
         "; ".join(api_errors[:3]))

    if max_turns is not None and num_turns is not None:
        note(num_turns <= max_turns, "turn count within budget",
             f"num_turns={num_turns} > max_turns={max_turns}")
    if max_seconds is not None and duration_ms is not None:
        note(duration_ms <= max_seconds * 1000.0, "wall time within budget",
             f"duration_ms={duration_ms} > {max_seconds * 1000.0:.0f}")

    if FAILURES:
        print(f"AUDIT FAILED ({len(FAILURES)} problem(s), {CHECKS} checks): {path}")
        for fmsg in FAILURES:
            print(f"  FAIL {fmsg}")
        return 1
    print(f"AUDIT OK ({CHECKS} checks): {path}")
    print(f"  tools={len(tool_set)} calls={tool_calls} "
          f"turns={num_turns} duration_ms={duration_ms}")
    print("NOTE: this validates run POLICY on a real transcript; it is not "
          "proof of GUI task correctness (coordinator compares ground truth).")
    return 0


def main(argv: List[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("transcript", help="path to a stream-json .jsonl transcript")
    ap.add_argument("--expect-server", default="computer",
                    help="the only MCP server name allowed (default: computer)")
    ap.add_argument("--max-turns", type=int, default=None,
                    help="fail if result.num_turns exceeds this budget")
    ap.add_argument("--max-seconds", type=float, default=None,
                    help="fail if result.duration_ms exceeds this budget")
    args = ap.parse_args(argv)
    return audit(args.transcript, args.expect_server, args.max_turns,
                 args.max_seconds)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
