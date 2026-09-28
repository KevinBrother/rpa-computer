#!/usr/bin/env bash
# QA pure-script regression tests (no Rust, no GUI, no real claude):
#   1. package-release.sh refuses dangerous release destinations, refuses
#      script-stub binaries, and produces a complete manifest (excluding
#      MANIFEST.sha256 itself) on the happy path — using an ISOLATED source
#      fixture, never touching the real repo target/ tree
#   2. run-blackbox-claude.sh --verify-only audits the tool-disable argv
#      policy; --no-sandbox requires --diagnostics; empty / non-screenshot
#      tasks are refused
#   3. audit-claude-transcript.py audits REAL-shaped stream-json transcripts
#      (fail-closed on extra tools, unexpected servers, denials, API errors)
#   4. windows stop/start-script ownership policy (static) + deploy script
#      manifest exclusion (static)
#   5. mcp_protocol.py end-to-end against an isolated fake-host fixture
#
# Exit 0 = all checks passed, 1 = failures.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PASS=0
FAIL=0

ok()   { PASS=$((PASS+1)); echo "PASS $1"; }
bad()  { FAIL=$((FAIL+1)); echo "FAIL $1: $2"; }

expect_refusal() { # name, expected-exit-not-0, cmd...
  local name="$1"; shift
  if "$@" >/dev/null 2>&1; then
    bad "$name" "command unexpectedly succeeded"
  else
    ok "$name"
  fi
}

# QA scratch prefix must NOT match the runner's staged-source-copy name
# pattern (rpa-computer-*/rpa-core-*): the sandbox profile denies that
# shape under temp roots, so QA-owned scratch named that way would deny
# the test's own fake claude / release dir (rc=126 red herring). This is
# a harness naming choice, NOT a weakening of the source-exclusion deny.
TMP="$(mktemp -d -t qa-scripttests.XXXXXX)"
trap 'rm -rf "$TMP"' EXIT

echo "== 1. package-release.sh destination safety =="

# A pre-existing ARBITRARY user dir must never be rm -rf'd: the script must
# refuse it AND leave its content intact.
USER_DIR="$TMP/precious"
mkdir -p "$USER_DIR"
echo "important user data" > "$USER_DIR/keep.txt"
if scripts/package-release.sh --release-dir "$USER_DIR" --skip-build >/dev/null 2>&1; then
  bad "release into existing user dir refused" "script succeeded"
elif [ -f "$USER_DIR/keep.txt" ] && grep -q "important" "$USER_DIR/keep.txt"; then
  ok "release into existing user dir refused (data intact)"
else
  bad "release into existing user dir refused" "user data was DELETED"
fi

# Home dir, root, the source tree, a repo ancestor, and a repo SIBLING (the
# old default destination) must all be refused.
expect_refusal "release dir = \$HOME refused" \
  scripts/package-release.sh --release-dir "$HOME" --skip-build
expect_refusal "release dir = / refused" \
  scripts/package-release.sh --release-dir / --skip-build
expect_refusal "release dir inside repo refused" \
  scripts/package-release.sh --release-dir "$ROOT/release-out" --skip-build
expect_refusal "release dir = repo ancestor refused" \
  scripts/package-release.sh --release-dir "$(dirname "$ROOT")" --skip-build
expect_refusal "release dir = repo sibling (old default) refused" \
  scripts/package-release.sh --release-dir "$(dirname "$ROOT")/rpa-computer-release" --skip-build
# A symlink alias of a safe location must be refused.
ln -s "$TMP" "$TMP/link-alias"
expect_refusal "release dir symlink refused" \
  scripts/package-release.sh --release-dir "$TMP/link-alias" --skip-build

echo "== 1b. package-release.sh happy path on an ISOLATED source fixture =="

# The test stages its own throwaway "source tree" (fixture scripts + a stub
# binary) OUTSIDE the repo. The real repo's target/ tree is never touched,
# so a concurrently built real executable can never be replaced or removed
# by these tests (continuation-review QA item 5).
FIXTURE_SRC="$TMP/fixture-src"
mkdir -p "$FIXTURE_SRC/scripts" "$FIXTURE_SRC/target/release" "$FIXTURE_SRC/examples/acceptance"
sed "s|$ROOT|$FIXTURE_SRC|g" "$ROOT/scripts/package-release.sh" > "$FIXTURE_SRC/scripts/package-release.sh"
chmod +x "$FIXTURE_SRC/scripts/package-release.sh"
printf '#!/usr/bin/env bash\necho stub\n' > "$FIXTURE_SRC/target/release/computer-host"
chmod +x "$FIXTURE_SRC/target/release/computer-host"
printf '# task\nobserve the screenshot.\n' > "$FIXTURE_SRC/examples/acceptance/00-fixture.md"

# A script stub must be REFUSED for packaging (never mistaken for a
# product artifact).
if "$FIXTURE_SRC/scripts/package-release.sh" --release-dir "$TMP/release-stub" --skip-build >/dev/null 2>&1; then
  bad "script-stub binary refused" "packaged a #! stub"
else
  ok "script-stub binary refused (not a built executable)"
fi

# Replace the stub with a Mach-O-looking fixture binary (magic bytes only;
# still not a real product, but proves the packaging path end to end).
printf '\xcf\xfa\xed\xfe fixture-binary-not-a-product\n' > "$FIXTURE_SRC/target/release/computer-host"
chmod +x "$FIXTURE_SRC/target/release/computer-host"

REL_OUT="$TMP/good/release"   # not a sibling of the fixture "source" dir
if "$FIXTURE_SRC/scripts/package-release.sh" --release-dir "$REL_OUT" --skip-build >"$TMP/pkg.log" 2>&1; then
  ok "package-release happy path succeeded (isolated fixture)"
  # Manifest completeness: RELEASE.json must be covered (defect #14).
  if grep -qE ' \./RELEASE\.json$' "$REL_OUT/MANIFEST.sha256"; then
    ok "manifest covers RELEASE.json"
  else
    bad "manifest covers RELEASE.json" "missing entry"
  fi
  # MANIFEST.sha256 must NOT cover itself (redirect-before-find defect).
  if grep -q 'MANIFEST.sha256' "$REL_OUT/MANIFEST.sha256"; then
    bad "manifest excludes itself" "self-entry present"
  else
    ok "manifest excludes MANIFEST.sha256 itself"
  fi
  n_manifest="$(wc -l < "$REL_OUT/MANIFEST.sha256" | tr -d ' ')"
  n_files="$(find "$REL_OUT" -type f ! -name MANIFEST.sha256 | wc -l | tr -d ' ')"
  if [ "$n_manifest" = "$n_files" ]; then
    ok "manifest covers every staged file ($n_files)"
  else
    bad "manifest completeness" "$n_manifest entries vs $n_files files"
  fi
  # Tool-owned marker exists, and a SECOND run reuses the dir safely.
  if grep -q '^rpa-computer-release-v1$' "$REL_OUT/.rpa-computer-release"; then
    ok "tool-owned marker written"
  else
    bad "tool-owned marker" "missing/invalid"
  fi
  if "$FIXTURE_SRC/scripts/package-release.sh" --release-dir "$REL_OUT" --skip-build >/dev/null 2>&1; then
    ok "re-run into tool-owned dir allowed"
  else
    bad "re-run into tool-owned dir" "refused"
  fi
  # mcp.json is valid JSON with the release binary path.
  if python3 -c "import json,sys; c=json.load(open('$REL_OUT/mcp/mcp.json')); assert c['mcpServers']['computer']['command'].endswith('/bin/computer-host')" 2>/dev/null; then
    ok "mcp.json valid JSON with release binary path"
  else
    bad "mcp.json" "invalid"
  fi
else
  bad "package-release happy path" "$(tail -3 "$TMP/pkg.log")"
fi

echo "== 2. run-blackbox-claude.sh argv + task policy =="

if scripts/run-blackbox-claude.sh --verify-only >/dev/null 2>&1; then
  ok "verify-only argv audit passes"
else
  bad "verify-only argv audit" "policy check failed"
fi

expect_refusal "--no-sandbox without --diagnostics refused" \
  scripts/run-blackbox-claude.sh --release-dir "$TMP" --task /dev/null --no-sandbox
expect_refusal "arbitrary trailing claude args refused" \
  scripts/run-blackbox-claude.sh --release-dir "$TMP" --task /dev/null -- --model opus
expect_refusal "empty task file refused" \
  scripts/run-blackbox-claude.sh --release-dir "$TMP" --task /dev/null --print
printf 'no outcome words here at all.\n' > "$TMP/task-nogui.md"
expect_refusal "task without screenshot-based outcome refused" \
  scripts/run-blackbox-claude.sh --release-dir "$TMP" --task "$TMP/task-nogui.md" --print

# Release/source overlap must be refused in BOTH directions, and a release
# dir containing source artifacts must be refused even when path-disjoint.
mkdir -p "$TMP/rel-in-src" "$TMP/rel-with-src"
printf 'fn main() {}\n' > "$TMP/rel-with-src/lib.rs"
printf 'Task: open, observe the screenshot, describe, close.\n' > "$TMP/task-ok.md"
expect_refusal "release dir = source dir refused" \
  scripts/run-blackbox-claude.sh --release-dir "$ROOT" --task "$TMP/task-ok.md" --print
expect_refusal "release dir inside source dir refused" \
  scripts/run-blackbox-claude.sh --release-dir "$ROOT/scripts" --task "$TMP/task-ok.md" --print
expect_refusal "release dir containing source artifacts refused" \
  scripts/run-blackbox-claude.sh --release-dir "$TMP/rel-with-src" --task "$TMP/task-ok.md" --print

# Static policy: the clean system prompt must carry the lock-screen safety
# instruction (continuation-review QA D).
if grep -q 'lock screen' "$ROOT/scripts/run-blackbox-claude.sh" && \
   grep -q 'never type credentials' "$ROOT/scripts/run-blackbox-claude.sh"; then
  ok "clean prompt includes lock-screen stop rule"
else
  bad "clean prompt includes lock-screen stop rule" "missing"
fi

# Static policy: the script must never EMIT a permission-bypass flag in the
# claude argv (the word appears only in comments/the denial check itself).
if grep -E '^\s*CLAUDE_ARGS=|CLAUDE_ARGS\+=' "$ROOT/scripts/run-blackbox-claude.sh" | grep -q 'dangerously-skip-permissions\|bypassPermissions'; then
  bad "no permission bypass in launcher" "bypass flag in claude args"
else
  ok "no permission bypass in launcher"
fi
# Static policy: real tool isolation (--tools '') is present, not just
# --allowedTools (auto-approval).
if grep -q -- "--tools ''" "$ROOT/scripts/run-blackbox-claude.sh"; then
  ok "launcher disables built-in tools (--tools '')"
else
  bad "launcher disables built-in tools" "--tools '' missing"
fi
# Print acceptance path carries the auditable transcript flags.
for flag in --output-format stream-json --verbose --no-session-persistence --setting-sources user --system-prompt --max-turns; do
  if grep -q -- "$flag" "$ROOT/scripts/run-blackbox-claude.sh"; then
    ok "print path sets $flag"
  else
    bad "print path sets $flag" "missing"
  fi
done

echo "== 3. audit-claude-transcript.py on real-shaped transcripts =="

AUDIT="$ROOT/scripts/audit-claude-transcript.py"
mk_init() { # $1 = tools json array, $2 = servers json array
  printf '{"type":"system","subtype":"init","session_id":"s","tools":%s,"mcp_servers":%s,"slash_commands":[]}\n' "$1" "$2"
}
GOOD_TOOLS='["mcp__computer__computer_describe","mcp__computer__computer_open","mcp__computer__computer_observe","mcp__computer__computer_step","mcp__computer__computer_get_step","mcp__computer__computer_pause","mcp__computer__computer_resume","mcp__computer__computer_close"]'
GOOD_SERVERS='[{"name":"computer","status":"connected"}]'

# 3a. GOOD transcript passes.
GOOD="$TMP/good.jsonl"
{
  mk_init "$GOOD_TOOLS" "$GOOD_SERVERS"
  printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__computer__computer_open","input":{}}]}}'
  printf '%s\n' '{"type":"user","message":{"content":[{"type":"tool_result","is_error":false,"content":"ok"}]}}'
  printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__computer__computer_observe","input":{"session_id":"x"}}]}}'
  printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"num_turns":2,"duration_ms":12345}'
} > "$GOOD"
if python3 "$AUDIT" "$GOOD" --max-turns 10 --max-seconds 300 >/dev/null 2>&1; then
  ok "good transcript passes audit"
else
  bad "good transcript passes audit" "rejected"
fi

# 3b. Builtin tool in init inventory -> FAIL.
EXTRA="$TMP/extra.jsonl"
{
  mk_init '["Read","mcp__computer__computer_describe","mcp__computer__computer_open","mcp__computer__computer_observe","mcp__computer__computer_step","mcp__computer__computer_get_step","mcp__computer__computer_pause","mcp__computer__computer_resume","mcp__computer__computer_close"]' "$GOOD_SERVERS"
  printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__computer__computer_describe","input":{}}]}}'
  printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"duration_ms":1000}'
} > "$EXTRA"
expect_refusal "builtin tool in inventory rejected" python3 "$AUDIT" "$EXTRA"

# 3c. Unexpected tool CALL -> FAIL.
BADCALL="$TMP/badcall.jsonl"
{
  mk_init "$GOOD_TOOLS" "$GOOD_SERVERS"
  printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"cat src/lib.rs"}}]}}'
  printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"duration_ms":1000}'
} > "$BADCALL"
expect_refusal "unexpected tool call rejected" python3 "$AUDIT" "$BADCALL"

# 3d. No init event -> FAIL.
NOINIT="$TMP/noinit.jsonl"
printf '%s\n' '{"type":"result","subtype":"success","is_error":false}' > "$NOINIT"
expect_refusal "missing init event rejected" python3 "$AUDIT" "$NOINIT"
expect_refusal "empty transcript rejected" python3 "$AUDIT" "$TMP/empty.jsonl"
: > "$TMP/empty.jsonl"
expect_refusal "zero-byte transcript rejected" python3 "$AUDIT" "$TMP/empty.jsonl"

# 3e. Extra/failed MCP server -> FAIL.
SRV="$TMP/srv.jsonl"
{
  mk_init "$GOOD_TOOLS" '[{"name":"computer","status":"connected"},{"name":"github","status":"connected"}]'
  printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__computer__computer_describe","input":{}}]}}'
  printf '%s\n' '{"type":"result","subtype":"success","is_error":false}'
} > "$SRV"
expect_refusal "unexpected MCP server rejected" python3 "$AUDIT" "$SRV"

# 3f. Permission denial recorded -> FAIL.
DENY="$TMP/deny.jsonl"
{
  mk_init "$GOOD_TOOLS" "$GOOD_SERVERS"
  printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__computer__computer_step","input":{}}]}}'
  printf '%s\n' '{"type":"user","message":{"content":[{"type":"tool_result","is_error":true,"content":"Claude requested permissions to use mcp__computer__computer_step, but you denied it."}]}}'
  printf '%s\n' '{"type":"result","subtype":"success","is_error":false}'
} > "$DENY"
expect_refusal "permission denial rejected" python3 "$AUDIT" "$DENY"

# 3g. API/model error -> FAIL.
APIERR="$TMP/apierr.jsonl"
{
  mk_init "$GOOD_TOOLS" "$GOOD_SERVERS"
  printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__computer__computer_describe","input":{}}]}}'
  printf '%s\n' '{"type":"result","subtype":"error_during_execution","is_error":true,"num_turns":3}'
} > "$APIERR"
expect_refusal "model/API error result rejected" python3 "$AUDIT" "$APIERR"

# 3h. Budget overrun -> FAIL.
expect_refusal "turn budget overrun rejected" python3 "$AUDIT" "$GOOD" --max-turns 1
expect_refusal "time budget overrun rejected" python3 "$AUDIT" "$GOOD" --max-seconds 1

# 3i. Disconnected MCP server -> FAIL.
SRVDOWN="$TMP/srvdown.jsonl"
{
  mk_init "$GOOD_TOOLS" '[{"name":"computer","status":"failed"}]'
  printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__computer__computer_describe","input":{}}]}}'
  printf '%s\n' '{"type":"result","subtype":"success","is_error":false}'
} > "$SRVDOWN"
expect_refusal "disconnected MCP server rejected" python3 "$AUDIT" "$SRVDOWN"

# 3j. Recorded hook execution -> FAIL.
HOOK="$TMP/hook.jsonl"
{
  mk_init "$GOOD_TOOLS" "$GOOD_SERVERS"
  printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__computer__computer_describe","input":{}}]}}'
  printf '%s\n' '{"type":"system","subtype":"hook_response","hook_name":"SessionStart"}'
  printf '%s\n' '{"type":"result","subtype":"success","is_error":false}'
} > "$HOOK"
expect_refusal "recorded hook execution rejected" python3 "$AUDIT" "$HOOK"

echo "== 3b. launcher e2e: sandboxed print run + transcript audit =="

# Full launcher path (probe -> sandbox-exec -> evidence -> audit) with a
# fake claude that EMITS a transcript. Release dir under $HOME (never a
# temp root — the acceptance profile denies temp roots, so a temp-root
# release dir must fail closed; that is also asserted).
E2E_BASE="$HOME/rpa-qa-scripttests-e2e"
if [ -e "$E2E_BASE" ]; then
  bad "e2e fixture setup" "$E2E_BASE already exists (refusing to touch)"
else
  mkdir -p "$E2E_BASE/release/bin" "$E2E_BASE/release/mcp" "$E2E_BASE/release/tasks" "$E2E_BASE/fakebin"
  printf '\xcf\xfa\xed\xfe fake\n' > "$E2E_BASE/release/bin/computer-host"
  chmod +x "$E2E_BASE/release/bin/computer-host"
  printf '{"mcpServers":{"computer":{"command":"%s","args":[],"env":{}}}}\n' \
    "$E2E_BASE/release/bin/computer-host" > "$E2E_BASE/release/mcp/mcp.json"
  printf 'Task: open, observe the screenshot, describe, close.\n' > "$E2E_BASE/release/tasks/t.md"
  cat > "$E2E_BASE/fakebin/claude" <<'FAKEE2E'
#!/usr/bin/env bash
# Fake claude emitting a well-formed stream-json transcript.
cat <<'JSON'
{"type":"system","subtype":"init","session_id":"s","tools":["mcp__computer__computer_describe","mcp__computer__computer_open","mcp__computer__computer_observe","mcp__computer__computer_step","mcp__computer__computer_get_step","mcp__computer__computer_pause","mcp__computer__computer_resume","mcp__computer__computer_close"],"mcp_servers":[{"name":"computer","status":"connected"}],"slash_commands":[]}
{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__computer__computer_open","input":{}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","is_error":false,"content":"ok"}]}}
{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__computer__computer_close","input":{"session_id":"s1"}}]}}
{"type":"result","subtype":"success","is_error":false,"num_turns":2,"duration_ms":5000}
JSON
echo "fake stderr" >&2
exit 0
FAKEE2E
  chmod +x "$E2E_BASE/fakebin/claude"

  if CLAUDE_BIN="$E2E_BASE/fakebin/claude" "$ROOT/scripts/run-blackbox-claude.sh" \
      --release-dir "$E2E_BASE/release" --task "$E2E_BASE/release/tasks/t.md" \
      --print --max-seconds 60 >"$TMP/e2e-good.log" 2>&1; then
    ok "sandboxed print run with transcript audit passes (fake claude)"
  else
    tail -8 "$TMP/e2e-good.log" | sed 's/^/    e2e-log: /'
    bad "sandboxed print run with transcript audit passes" "nonzero exit"
  fi
  if grep -q '^0$' "$E2E_BASE"/release/evidence/*/exit-code.txt 2>/dev/null && \
     [ -s "$E2E_BASE"/release/evidence/*/transcript.jsonl ] && \
     grep -q 'fake stderr' "$E2E_BASE"/release/evidence/*/stderr.log; then
    ok "evidence captured in release dir (transcript + stderr + exit code)"
  else
    bad "evidence captured in release dir" "missing transcript/stderr/exit-code"
  fi

  # The EXACT sandbox profile used by BOTH the negative probe and the real
  # launch must be persisted into the run's evidence dir (previously the
  # generated profile was deleted on exit, so a report could never show the
  # profile a run actually ran under). Verify by CONTENT IDENTITY, not by
  # comments: the recorded SHA256 must match the persisted file byte for
  # byte, the recorded generator-script SHA256 must match the on-disk
  # script, and the evidence dir + profile copy must be restrictive (0700 /
  # 0600 — the profile contains the real source path).
  EVD="$(ls -d "$E2E_BASE"/release/evidence/*/ 2>/dev/null | head -1)"
  if [ -n "$EVD" ] && [ -f "$EVD/sandbox.sb" ] && [ -f "$EVD/sandbox.sb.sha256" ]; then
    evd_hash="$(shasum -a 256 "$EVD/sandbox.sb" | awk '{print $1}')"
    rec_hash="$(cat "$EVD/sandbox.sb.sha256")"
    script_hash="$(shasum -a 256 "$ROOT/scripts/run-blackbox-claude.sh" | awk '{print $1}')"
    prof_perm="$(stat -f '%Lp' "$EVD/sandbox.sb")"
    evd_perm="$(stat -f '%Lp' "$EVD")"
    if [ "$evd_hash" = "$rec_hash" ] && [ -s "$EVD/sandbox.sb" ] && \
       grep -q '(deny file-read\* file-write\* (subpath "' "$EVD/sandbox.sb" && \
       grep -qF "generator-script-sha256=$script_hash" "$EVD/sandbox.sb.meta" && \
       grep -q 'generator-script=.*scripts/run-blackbox-claude\.sh$' "$EVD/sandbox.sb.meta" && \
       [ "$prof_perm" = "600" ] && [ "$evd_perm" = "700" ]; then
      ok "exact run sandbox profile persisted (hash/meta/identity verified, 0600 in 0700 evidence dir)"
    else
      bad "exact run sandbox profile persisted" \
          "hash/meta/perms mismatch (hash=$evd_hash rec=$rec_hash prof=$prof_perm dir=$evd_perm)"
    fi
    # Tamper-evidence: mutating the persisted copy must be detectable via
    # the recorded SHA256 (the record binds the exact bytes that ran).
    printf '; tampered\n' >> "$EVD/sandbox.sb"
    if [ "$(shasum -a 256 "$EVD/sandbox.sb" | awk '{print $1}')" != "$rec_hash" ]; then
      ok "persisted profile tamper is detectable via recorded SHA256"
    else
      bad "persisted profile tamper is detectable via recorded SHA256" "mutation went undetected"
    fi
  else
    bad "exact run sandbox profile persisted" "sandbox.sb / sandbox.sb.sha256 missing from evidence dir"
  fi

  # Rogue transcript (builtin tool call) must fail the RUN even when the
  # claude process exits 0.
  cat > "$E2E_BASE/fakebin/claude" <<'FAKEROGUE'
#!/usr/bin/env bash
cat <<'JSON'
{"type":"system","subtype":"init","session_id":"s","tools":["mcp__computer__computer_describe","mcp__computer__computer_open","mcp__computer__computer_observe","mcp__computer__computer_step","mcp__computer__computer_get_step","mcp__computer__computer_pause","mcp__computer__computer_resume","mcp__computer__computer_close"],"mcp_servers":[{"name":"computer","status":"connected"}]}
{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Bash","input":{"command":"cat /etc/passwd"}}]}}
{"type":"result","subtype":"success","is_error":false,"num_turns":2,"duration_ms":5000}
JSON
exit 0
FAKEROGUE
  chmod +x "$E2E_BASE/fakebin/claude"
  expect_refusal "rogue tool call fails the sandboxed run" \
    env CLAUDE_BIN="$E2E_BASE/fakebin/claude" "$ROOT/scripts/run-blackbox-claude.sh" \
      --release-dir "$E2E_BASE/release" --task "$E2E_BASE/release/tasks/t.md" \
      --print --max-seconds 60

  rm -rf "$E2E_BASE"
fi

# A SAFE-NAMED source-free release dir under a temp root must be ACCEPTED:
# the profile denies only exact source paths/copies and this tool family's
# own rpa-* scratch names — never the whole temp root (continuation-review:
# do not special-case temp roots that Claude or a legitimate release needs).
TMP_REL="$TMP/tmp-release"
mkdir -p "$TMP_REL/bin" "$TMP_REL/mcp" "$TMP_REL/tasks"
printf '\xcf\xfa\xed\xfe fake\n' > "$TMP_REL/bin/computer-host"; chmod +x "$TMP_REL/bin/computer-host"
printf '{"mcpServers":{"computer":{"command":"%s","args":[],"env":{}}}}\n' \
  "$TMP_REL/bin/computer-host" > "$TMP_REL/mcp/mcp.json"
printf 'Task: open, observe the screenshot, describe, close.\n' > "$TMP_REL/tasks/t.md"
FAKEBIN="$TMP/fakebin-tmp"
mkdir -p "$FAKEBIN"
cat > "$FAKEBIN/claude" <<'FAKETMP'
#!/usr/bin/env bash
cat <<'JSON'
{"type":"system","subtype":"init","session_id":"s","tools":["mcp__computer__computer_describe","mcp__computer__computer_open","mcp__computer__computer_observe","mcp__computer__computer_step","mcp__computer__computer_get_step","mcp__computer__computer_pause","mcp__computer__computer_resume","mcp__computer__computer_close"],"mcp_servers":[{"name":"computer","status":"connected"}],"slash_commands":[]}
{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__computer__computer_open","input":{}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","is_error":false,"content":"ok"}]}}
{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__computer__computer_close","input":{"session_id":"s1"}}]}}
{"type":"result","subtype":"success","is_error":false,"num_turns":2,"duration_ms":5000}
JSON
exit 0
FAKETMP
chmod +x "$FAKEBIN/claude"
if [ "$(uname -s)" = "Darwin" ] && command -v sandbox-exec >/dev/null 2>&1; then
  if CLAUDE_BIN="$FAKEBIN/claude" "$ROOT/scripts/run-blackbox-claude.sh" \
      --release-dir "$TMP_REL" --task "$TMP_REL/tasks/t.md" \
      --print --max-seconds 60 >"$TMP/tmp-rel.log" 2>&1; then
    ok "safe-named source-free release under temp root accepted"
  else
    tail -6 "$TMP/tmp-rel.log" | sed 's/^/    tmp-rel-log: /'
    bad "safe-named source-free release under temp root accepted" "run failed"
  fi
  # But an rpa-*-SHAPED release dir under a temp root (the scratch pattern
  # this tool family uses for staged source copies) must fail closed.
  RPA_REL="$TMP/rpa-computer-evil"
  mkdir -p "$RPA_REL/bin" "$RPA_REL/mcp" "$RPA_REL/tasks"
  printf '\xcf\xfa\xed\xfe fake\n' > "$RPA_REL/bin/computer-host"; chmod +x "$RPA_REL/bin/computer-host"
  printf '{"mcpServers":{"computer":{"command":"%s","args":[],"env":{}}}}\n' \
    "$RPA_REL/bin/computer-host" > "$RPA_REL/mcp/mcp.json"
  printf 'Task: open, observe the screenshot, describe, close.\n' > "$RPA_REL/tasks/t.md"
  expect_refusal "rpa-*-shaped release dir under temp root fails closed" \
    env CLAUDE_BIN="$FAKEBIN/claude" "$ROOT/scripts/run-blackbox-claude.sh" \
      --release-dir "$RPA_REL" --task "$RPA_REL/tasks/t.md" --print --max-seconds 30
else
  echo "SKIP temp-root release policy e2e (no sandbox-exec on $(uname -s))"
fi

echo "== 3b2. watchdog: controlled hung fake CLI + descendant tree =="

# A fake claude that ignores SIGTERM and hangs forever, with a descendant
# (simulated MCP/bridge child). The watchdog must terminate BOTH by exact
# verified pgid, and must not touch any process outside that group. The
# launcher exit code on budget expiry must be 124. No real GUI, no name kill.
HANG_BASE="$TMP/watchdog"
mkdir -p "$HANG_BASE/release/bin" "$HANG_BASE/release/mcp" "$HANG_BASE/release/tasks" "$HANG_BASE/fakebin"
printf '\xcf\xfa\xed\xfe fake\n' > "$HANG_BASE/release/bin/computer-host"
chmod +x "$HANG_BASE/release/bin/computer-host"
printf '{"mcpServers":{"computer":{"command":"%s","args":[],"env":{}}}}\n' \
  "$HANG_BASE/release/bin/computer-host" > "$HANG_BASE/release/mcp/mcp.json"
printf 'Task: open, observe the screenshot, describe, close.\n' > "$HANG_BASE/release/tasks/t.md"
cat > "$HANG_BASE/fakebin/claude" <<'FAKEHANG'
#!/usr/bin/env bash
# Hung fake CLI: descendant child + SIGTERM-ignoring infinite loop.
sleep 3600 &          # simulated MCP/bridge descendant in the same pgroup
echo "$!" > "${HANG_MARK_DIR:?}/descendant.pid"
trap '' TERM          # ignore SIGTERM like a wedged CLI would
while :; do sleep 1; done
FAKEHANG
chmod +x "$HANG_BASE/fakebin/claude"
HANG_MARK_DIR="$HANG_BASE/marks"; mkdir -p "$HANG_MARK_DIR"
# A lookalike process OUTSIDE the launcher's group: must survive untouched.
sleep 3600 &
OUTSIDER=$!
set +e
HANG_MARK_DIR="$HANG_MARK_DIR" CLAUDE_BIN="$HANG_BASE/fakebin/claude" \
  "$ROOT/scripts/run-blackbox-claude.sh" \
    --release-dir "$HANG_BASE/release" --task "$HANG_BASE/release/tasks/t.md" \
    --print --max-seconds 5 >"$TMP/hang.log" 2>&1
hang_rc=$?
set -e
if [ "$hang_rc" -eq 124 ]; then
  ok "watchdog exits 124 on wall-clock budget expiry"
else
  bad "watchdog exits 124 on wall-clock budget expiry" "rc=$hang_rc"
fi
hang_desc=""
[ -f "$HANG_MARK_DIR/descendant.pid" ] && hang_desc="$(cat "$HANG_MARK_DIR/descendant.pid")"
if [ -n "$hang_desc" ] && ! kill -0 "$hang_desc" 2>/dev/null; then
  ok "watchdog killed the owned descendant (verified pgid)"
else
  bad "watchdog killed the owned descendant" "descendant pid=$hang_desc still alive or never recorded"
fi
if kill -0 "$OUTSIDER" 2>/dev/null; then
  ok "watchdog did NOT touch the lookalike process outside its group"
  kill "$OUTSIDER" 2>/dev/null || true
else
  bad "watchdog did NOT touch the lookalike process" "outsider pid=$OUTSIDER was killed"
fi
if grep -q '^124$' "$HANG_BASE"/release/evidence/*/exit-code.txt 2>/dev/null; then
  ok "budget-exceeded run records exit code 124 in evidence"
else
  bad "budget-exceeded run records exit code 124" "missing"
fi

echo "== 3b3. watchdog: normal-exit leader + TERM-ignoring descendant =="

# Normal-exit path (NO budget expiry): the group leader exits 42 on its own
# while a SIGTERM-ignoring descendant (simulated orphaned MCP/bridge
# grandchild) stays alive in the same owned group. The watchdog must:
#   * return the leader's own exit code 42 (never 124);
#   * run the SAME owned-group cleanup (TERM/grace/KILL) it runs on expiry;
#   * leave the TERM-ignoring descendant GONE (this is the qa-fix-3
#     acknowledged leak; a watchdog that skips cleanup on normal exit FAILS
#     this test by leaving the descendant alive).
NORM_BASE="$TMP/watchdog-normal"
mkdir -p "$NORM_BASE/bin" "$NORM_BASE/marks"
printf '\xcf\xfa\xed\xfe fake\n' > "$NORM_BASE/bin/computer-host"
chmod +x "$NORM_BASE/bin/computer-host"
cat > "$NORM_BASE/leader.sh" <<'FAKENORM'
#!/usr/bin/env bash
# Leader exits immediately with 42, leaving a TERM-ignoring descendant alive.
bash -c 'trap "" TERM; echo "$$" > "${NORM_MARK_DIR:?}/descendant.pid"; sleep 3600' &
# Let the descendant install its TERM handler + pid record before we exit.
i=0
while [ ! -s "${NORM_MARK_DIR:?}/descendant.pid" ] && [ "$i" -lt 100 ]; do
  sleep 0.1; i=$((i+1))
done
exit 42
FAKENORM
chmod +x "$NORM_BASE/leader.sh"
set +e
NORM_MARK_DIR="$NORM_BASE/marks" \
  python3 "$ROOT/scripts/watchdog-launch.py" --max-seconds 60 \
    "$NORM_BASE/stdout.log" "$NORM_BASE/stderr.log" -- \
    bash "$NORM_BASE/leader.sh" >"$TMP/norm.log" 2>&1
norm_rc=$?
set -e
if [ "$norm_rc" -eq 42 ]; then
  ok "watchdog preserves leader exit code on normal exit (42, not 124)"
else
  bad "watchdog preserves leader exit code on normal exit" "rc=$norm_rc (want 42)"
fi
norm_desc=""
[ -f "$NORM_BASE/marks/descendant.pid" ] && norm_desc="$(cat "$NORM_BASE/marks/descendant.pid")"
if [ -n "$norm_desc" ] && ! kill -0 "$norm_desc" 2>/dev/null; then
  ok "normal-exit run cleaned the TERM-ignoring owned descendant"
else
  bad "normal-exit run cleaned the TERM-ignoring owned descendant" \
      "descendant pid=$norm_desc still alive or never recorded"
  # Test-owned final cleanup: never leak the fixture even on failure.
  [ -n "$norm_desc" ] && { kill "$norm_desc" 2>/dev/null; sleep 1; kill -9 "$norm_desc" 2>/dev/null; } || true
fi

echo "== 3b4. watchdog: interruption (SIGTERM/SIGINT) cleanup =="

# The watchdog ITSELF is interrupted while its child is a TERM-ignoring
# hanger with a live descendant. It must clean the whole owned group (leader
# AND descendant, both of which ignore TERM and need the KILL escalation),
# report a TRUTHFUL interrupted status (128+SIG = 143 for TERM / 130 for
# INT — never the never-exercised child rc), and fail LOUDLY (nonzero) if
# the group still has live members afterwards.
for INT_SIG in TERM INT; do
  case "$INT_SIG" in TERM) int_want=143 ;; INT) int_want=130 ;; esac
  INT_BASE="$TMP/watchdog-int-$INT_SIG"
  mkdir -p "$INT_BASE/bin" "$INT_BASE/marks"
  printf '\xcf\xfa\xed\xfe fake\n' > "$INT_BASE/bin/computer-host"
  chmod +x "$INT_BASE/bin/computer-host"
  cat > "$INT_BASE/leader.sh" <<'FAKEINT'
#!/usr/bin/env bash
# TERM-ignoring hung leader with a TERM-ignoring descendant.
bash -c 'trap "" TERM; echo "$$" > "${INT_MARK_DIR:?}/descendant.pid"; sleep 3600' &
trap '' TERM
echo "$$" > "${INT_MARK_DIR:?}/leader.pid"
while :; do sleep 1; done
FAKEINT
  chmod +x "$INT_BASE/leader.sh"
  INT_MARK_DIR="$INT_BASE/marks" \
    python3 "$ROOT/scripts/watchdog-launch.py" --max-seconds 600 \
      "$INT_BASE/stdout.log" "$INT_BASE/stderr.log" -- \
      bash "$INT_BASE/leader.sh" >"$TMP/int-$INT_SIG.log" 2>&1 &
  WD_PID=$!
  # Bounded wait for the leader + descendant to exist (records are written
  # by the fixture itself, never by guessing).
  int_ready=0; int_i=0
  while [ "$int_i" -lt 100 ]; do
    if [ -s "$INT_BASE/marks/leader.pid" ] && [ -s "$INT_BASE/marks/descendant.pid" ]; then
      int_ready=1; break
    fi
    sleep 0.1; int_i=$((int_i+1))
  done
  if [ "$int_ready" -ne 1 ]; then
    bad "watchdog interruption cleanup ($INT_SIG)" "fixture leader/descendant never appeared"
    kill -9 "$WD_PID" 2>/dev/null || true
    wait "$WD_PID" 2>/dev/null || true
  else
    int_leader="$(cat "$INT_BASE/marks/leader.pid")"
    int_desc="$(cat "$INT_BASE/marks/descendant.pid")"
    kill -s "$INT_SIG" "$WD_PID" 2>/dev/null
    set +e
    int_wait=0
    while kill -0 "$WD_PID" 2>/dev/null && [ "$int_wait" -lt 300 ]; do
      sleep 0.1; int_wait=$((int_wait+1))
    done
    if kill -0 "$WD_PID" 2>/dev/null; then
      kill -9 "$WD_PID" 2>/dev/null
      bad "watchdog interruption cleanup ($INT_SIG)" "watchdog did not exit within bounded wait"
    fi
    wait "$WD_PID"
    int_rc=$?
    set -e
    if [ "$int_rc" -eq "$int_want" ]; then
      ok "watchdog reports truthful interrupted status ($INT_SIG -> $int_want)"
    else
      bad "watchdog reports truthful interrupted status ($INT_SIG)" "rc=$int_rc (want $int_want)"
    fi
    int_gone=1
    kill -0 "$int_leader" 2>/dev/null && int_gone=0
    kill -0 "$int_desc" 2>/dev/null && int_gone=0
    if [ "$int_gone" -eq 1 ]; then
      ok "interrupted watchdog cleaned leader + TERM-ignoring descendant ($INT_SIG)"
    else
      bad "interrupted watchdog cleaned leader + TERM-ignoring descendant ($INT_SIG)" \
          "leader=$int_leader desc=$int_desc still alive"
      # Test-owned final cleanup: never leak the fixture even on failure.
      kill -9 "$int_leader" "$int_desc" 2>/dev/null || true
    fi
  fi
done

echo "== 3c. single-profile sandbox probe =="

# The negative probe must run against THE SAME profile file the real run
# uses: the launcher must reference exactly one generated profile path
# (no separate probe profile).
if grep -q 'probe_profile' "$ROOT/scripts/run-blackbox-claude.sh"; then
  bad "single sandbox profile for probe+run" "separate probe_profile still present"
else
  ok "single sandbox profile for probe+run (no probe_profile variant)"
fi
# Functional: standalone probe (its own fresh profile) still proves the
# deny rules work end to end on this host.
if [ "$(uname -s)" = "Darwin" ] && command -v sandbox-exec >/dev/null 2>&1; then
  if scripts/sandbox-probe.sh "$ROOT" >/dev/null 2>&1; then
    ok "standalone sandbox negative probe passes (exact profile semantics)"
  else
    bad "standalone sandbox negative probe" "isolation not proven"
  fi
else
  echo "SKIP standalone sandbox probe (no sandbox-exec on $(uname -s))"
fi

echo "== 4. windows ownership + deploy policy (static) =="

STOP="$ROOT/scripts/windows-stop-host.ps1"
START="$ROOT/scripts/windows-start-host.ps1"
DEPLOY="$ROOT/scripts/deploy-windows-test.sh"

# The stop script must not kill processes by name pattern.
if grep -q "Get-Process -Name 'computer-host'" "$STOP"; then
  bad "stop script has no pattern-kill" "Get-Process -Name computer-host present"
else
  ok "stop script has no name-pattern process kill"
fi
# It must verify the exe path AND creation time of the recorded PID.
if grep -q 'ExecutablePath -eq \$exeExpected' "$STOP" && grep -q 'CreationDate' "$STOP"; then
  ok "stop script verifies recorded PID exe path + creation date before stop"
else
  bad "stop script PID verification" "missing exe-path or creation-date check"
fi
# Stop must verify the live task's action/principal before unregistering
# (verify-first ordering: taskVerified flag + principal check).
if grep -q 'taskVerified' "$STOP" && grep -q 'Principal.UserId' "$STOP"; then
  ok "stop script verifies task action+principal before unregister"
else
  bad "stop script task ownership verification" "missing"
fi
# -TaskName override must not be able to target an unrecorded task.
if grep -q 'does not match the recorded owned task' "$STOP"; then
  ok "stop script refuses task-name override of unrecorded task"
else
  bad "stop script override guard" "missing"
fi
# Start script: console session via WTS, principal = explorer OWNER, and
# collision-safe task registration (no blind Unregister before checks).
if grep -q 'WTSGetActiveConsoleSessionId' "$START" && \
   ! grep -q 'New-ScheduledTaskPrincipal -UserId \$env' "$START" && \
   ! grep -q '"\$env:USERDOMAIN\\\$env:USERNAME"' "$START"; then
  ok "start script uses console session + explorer owner (not SSH env user)"
else
  bad "start script session selection" "missing WTS or principal still derived from SSH env user"
fi
if grep -q 'Get-ScheduledTask -TaskName \$TaskName' "$START" && \
   grep -q 'refusing to delete/replace an unrelated task' "$START"; then
  ok "start script refuses colliding unowned task"
else
  bad "start script task-collision guard" "missing"
fi
# Token ACL must be SID-based, not display-name based.
if grep -q 'Translate(\[System.Security.Principal.SecurityIdentifier\])' "$START" && \
   grep -q "S-1-5-18" "$START" && grep -q "S-1-5-32-544" "$START" && \
   ! grep -q "Everyone|BUILTIN" "$START"; then
  ok "start script token ACL uses SID allowlist (no display-name matching)"
else
  bad "start script token ACL" "not SID-based"
fi
# Deploy manifest must exclude MANIFEST.sha256 itself.
if grep -q '! -name MANIFEST.sha256' "$DEPLOY"; then
  ok "deploy manifest excludes MANIFEST.sha256 itself"
else
  bad "deploy manifest self-exclusion" "missing"
fi
# Remote verification must hash files ON the remote (Get-FileHash), not
# trust the uploaded MANIFEST (a corrupted transfer would validate itself),
# must normalize CRLF and hex case, and ps_remote must not reject the '$'
# that literal PowerShell syntax requires (the old blanket '$' rejection
# made the parse/ACL blocks impossible to send).
if grep -q 'Get-FileHash' "$DEPLOY" && ! grep -q "Get-Content '\$REMOTE_DIR..MANIFEST" "$DEPLOY"; then
  ok "deploy verifies via remote Get-FileHash (not uploaded manifest)"
else
  bad "deploy verifies via remote Get-FileHash" "still reads uploaded manifest"
fi
if grep -q "tr -d '\\\\r'" "$DEPLOY" && grep -q 'tolower(\$1)' "$DEPLOY"; then
  ok "deploy normalizes CRLF + hex case in remote hash comparison"
else
  bad "deploy CRLF/case normalization" "missing"
fi
if grep -q "contains a dollar sign" "$DEPLOY"; then
  bad "ps_remote accepts literal PowerShell ('\$' allowed)" "blanket '\$' rejection still present"
else
  ok "ps_remote accepts literal PowerShell ('\$' allowed)"
fi
# Deploy remote PowerShell must go through EncodedCommand (no nested quoting).
if grep -q 'EncodedCommand' "$DEPLOY"; then
  ok "deploy uses EncodedCommand remote PowerShell"
else
  bad "deploy remote PowerShell" "EncodedCommand missing"
fi
# Token must never be printed to the TERMINAL/logs: no echo, and no printf
# whose stdout is a terminal (redirections into the token file are fine).
if grep -nE '(^|[;|] *)echo [^>]*\$TOKEN' "$DEPLOY" >/dev/null || \
   grep -nE 'printf [^>]*\$TOKEN[[:space:]]*($|[;|])' "$DEPLOY" >/dev/null; then
  bad "token never printed" "found token in terminal output statement"
else
  ok "token never printed in deploy script"
fi

echo "== 5. mcp_protocol.py end-to-end on an isolated fake-host fixture =="

FAKE_HOST="$TMP/fake-host.py"
cat > "$FAKE_HOST" <<'PYEOF'
#!/usr/bin/env python3
"""Deterministic fake computer-host for the MCP protocol harness self-test.
Speaks newline-delimited JSON-RPC on stdio exactly like the real host
contract; serves a tiny valid PNG. No GUI, no input."""
import base64
import json
import struct
import sys
import zlib

def png(w, h, rgb=(64, 128, 200)):
    def chunk(tag, data):
        c = tag + data
        return struct.pack(">I", len(data)) + c + struct.pack(">I", zlib.crc32(c) & 0xFFFFFFFF)
    ihdr = struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)
    raw = b"".join(b"\x00" + bytes(rgb) * w for _ in range(h))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr)
            + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))

PNG_B64 = base64.b64encode(png(32, 32)).decode()
SESSION = "fake-session-1"
OBS = "fake-obs-1"
STEPS = {}

def result_text(data, image=False):
    content = [{"type": "text", "text": json.dumps(data)}]
    if image:
        content.append({"type": "image", "data": PNG_B64, "mimeType": "image/png"})
    return {"content": content, "isError": False}

def err_text(msg):
    return {"content": [{"type": "text", "text": json.dumps({"error": msg, "state": "closed"})}], "isError": True}

def handle(req):
    method = req.get("method")
    if method == "initialize":
        return {"protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "fake-computer-host", "version": "0.0.0"}}
    if method == "ping":
        return {}
    if method == "tools/list":
        names = ["computer_describe", "computer_open", "computer_observe", "computer_step",
                 "computer_get_step", "computer_pause", "computer_resume", "computer_close"]
        return {"tools": [{"name": n, "description": "fake", "inputSchema": {"type": "object"}} for n in names]}
    if method == "tools/call":
        name = req.get("params", {}).get("name")
        args = req.get("params", {}).get("arguments", {}) or {}
        if not isinstance(args.get("session_id", ""), (str,)) and name not in ("computer_describe", "computer_open"):
            return err_text("invalid_arguments")
        if name == "computer_describe":
            return result_text({"platform": "fake", "state": "closed"})
        if name == "computer_open":
            return result_text({"session_id": SESSION, "state": "open", "capabilities": {}})
        if name == "computer_observe":
            return result_text({"observation_id": OBS, "session_id": SESSION, "surface_id": "s0",
                                "geometry_version": "g1", "input_sequence": 0,
                                "width_px": 32, "height_px": 32}, image=True)
        if name == "computer_step":
            rid = args.get("request_id")
            action = args.get("action")
            if rid in STEPS:
                if json.dumps(STEPS[rid]["action"], sort_keys=True) != json.dumps(action, sort_keys=True):
                    return err_text("request_conflict")
                return result_text(STEPS[rid]["result"])
            if STEPS.get("__paused__"):
                return err_text("cancelled")
            res = {"request_id": rid, "input_outcome": "dispatched",
                   "observation_outcome": "captured", "cleanup_outcome": "not_needed",
                   "observation": {"observation_id": OBS, "session_id": SESSION, "surface_id": "s0",
                                   "geometry_version": "g1", "input_sequence": 1,
                                   "width_px": 32, "height_px": 32}}
            STEPS[rid] = {"action": action, "result": res}
            return result_text(res)
        if name == "computer_get_step":
            rid = args.get("request_id")
            if rid in STEPS and rid != "__paused__":
                return result_text(STEPS[rid]["result"])
            return err_text("request_not_found")
        if name == "computer_pause":
            STEPS["__paused__"] = {"action": None, "result": None}
            return result_text({"state": "paused"})
        if name == "computer_resume":
            STEPS.pop("__paused__", None)
            return result_text({"state": "open"})
        if name == "computer_close":
            return result_text({"state": "closed", "cleanup_outcome": "not_needed"})
        return err_text("unknown_tool")
    return None

def main():
    if "--mock-backend" not in sys.argv:
        print("fake host requires --mock-backend", file=sys.stderr)
        sys.exit(2)
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            print(json.dumps({"jsonrpc": "2.0", "id": None,
                              "error": {"code": -32700, "message": "parse error"}}), flush=True)
            continue
        if "id" not in req:  # notification
            continue
        res = handle(req)
        if res is None:
            print(json.dumps({"jsonrpc": "2.0", "id": req["id"],
                              "error": {"code": -32601, "message": "method not found"}}), flush=True)
        else:
            print(json.dumps({"jsonrpc": "2.0", "id": req["id"], "result": res}), flush=True)

if __name__ == "__main__":
    main()
PYEOF
chmod +x "$FAKE_HOST"

# The harness must run green end-to-end against the fake host. The
# cross-process lease scenario is product-only behavior (fixture has no
# lease) and is explicitly skipped in --selftest mode.
if python3 "$ROOT/tests/mcp_protocol.py" --host "$FAKE_HOST" "--host-arg=--mock-backend" \
      --selftest >"$TMP/mcp-selftest.log" 2>&1; then
  ok "mcp_protocol.py passes against fake host"
else
  tail -12 "$TMP/mcp-selftest.log" | sed 's/^/    selftest-log: /'
  bad "mcp_protocol.py passes against fake host" "$(grep -c '^FAIL' "$TMP/mcp-selftest.log") failures (log above)"
fi

# The harness must still REFUSE a real-looking host without --allow-real-gui.
expect_refusal "harness refuses non-mock host without --allow-real-gui" \
  python3 "$ROOT/tests/mcp_protocol.py" --host /bin/ls

echo
echo "=== script tests: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
