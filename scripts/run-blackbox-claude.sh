#!/usr/bin/env bash
# run-blackbox-claude.sh — launch the EXTERNAL black-box acceptance Claude
# process from the source-free release directory, with REAL tool isolation
# and a REAL transcript audit trail.
#
# Isolation policy (single source of truth in build_claude_args):
#   * --tools ''                     built-in tools DISABLED (Read/Bash/Glob/
#                                    Grep/Edit/Write/Web*/Task/...). NOTE:
#                                    --allowedTools alone is only an
#                                    AUTO-APPROVAL list — it does NOT remove
#                                    tools. That was defect #13.
#   * --strict-mcp-config            only the computer MCP server loads
#   * --disable-slash-commands       no skill/command injection
#   * --settings disableAllHooks     per-invocation hook isolation
#   * --system-prompt CLEAN_PROMPT   REPLACES the default system prompt
#                                    (append-mode left inherited role
#                                    context in place — continuation-review
#                                    QA item 1)
#   * --setting-sources user         project/local settings (if any stray
#                                    ones exist near the release dir) are
#                                    NOT loaded; the user's own model/auth
#                                    settings remain untouched
#   * --no-session-persistence       print-mode runs leave no resumable
#                                    session artifacts on disk
#   * --output-format stream-json --verbose   (print mode) real transcript
#   * no source auto-context: cwd = release dir (no CLAUDE.md/.git/source)
#   * OS-level deny of source reads+writes: the SAME generated sandbox
#     profile is used for the negative probe and the final Claude process
#     (canonicalized paths; no symlinked source aliases)
#
# Evidence (print acceptance path): the stream-json transcript AND stderr
# are captured to <release>/evidence/<run-id>/ (inside the release dir,
# outside the source tree; transcripts must not contain credentials — the
# launcher never prints tokens and claude does not echo auth material into
# stream-json output; if a credential ever appears, the evidence dir ACL is
# the mitigation — created 0700). After the process exits, the launcher
# FAILS unless scripts/audit-claude-transcript.py passes on the transcript
# (actual init tool inventory, actual tool calls, permission denials, API
# errors, budgets). A policy-valid run is still NOT proof of GUI task
# correctness — the coordinator compares ground truth separately.
#
# This script FAILS CLOSED: if the OS sandbox cannot be applied, it refuses
# to launch. --no-sandbox is honored ONLY with --diagnostics (a run that is
# explicitly NOT valid isolation evidence). There are no permission bypasses
# (--dangerously-skip-permissions is never used) and NO arbitrary trailing
# claude args (an unreviewed bypass channel — defect #13).
#
# Residual risk (documented honestly): GUI access is not an airtight
# capability sandbox. The agent sees whatever is on the test desktop and
# could in principle open a Finder/Explorer window and navigate to the
# source directory VISUALLY. The OS sandbox blocks process-level reads;
# residual visual access is mitigated only by the controlled desktop (no
# source/terminal windows open) plus prompt prohibitions, and must be listed
# as residual risk in the QA report. See
# examples/acceptance/GUI-TEST-FIXTURE.md for the controlled-desktop plan.
#
# Usage:
#   scripts/run-blackbox-claude.sh --release-dir PATH --task TASK_MD \
#       [--source-dir PATH] [--max-turns N] [--max-seconds S] [--print]
#       [--diagnostics --no-sandbox]
#   scripts/run-blackbox-claude.sh --verify-only          # dry-run w/ fake claude
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RELEASE_DIR=""
TASK=""
SOURCE_DIR="$ROOT"
USE_SANDBOX=1
PRINT_MODE=0
DIAGNOSTICS=0
VERIFY_ONLY=0
MAX_TURNS=12        # bounded turn budget (acceptance tasks need <= ~10)
MAX_SECONDS=600     # bounded wall-clock budget per run

while [ $# -gt 0 ]; do
  case "$1" in
    --release-dir)  RELEASE_DIR="$2"; shift 2 ;;
    --task)         TASK="$2"; shift 2 ;;
    --source-dir)   SOURCE_DIR="$2"; shift 2 ;;
    --max-turns)    MAX_TURNS="$2"; shift 2 ;;
    --max-seconds)  MAX_SECONDS="$2"; shift 2 ;;
    --no-sandbox)   USE_SANDBOX=0; shift ;;
    --print)        PRINT_MODE=1; shift ;;
    --diagnostics)  DIAGNOSTICS=1; shift ;;
    --verify-only)  VERIFY_ONLY=1; shift ;;
    *) echo "unknown arg: $1 (arbitrary trailing claude args are not accepted)" >&2; exit 2 ;;
  esac
done

case "$MAX_TURNS" in (*[!0-9]*|'') echo "--max-turns must be a positive integer" >&2; exit 2 ;; esac
case "$MAX_SECONDS" in (*[!0-9]*|'') echo "--max-seconds must be a positive integer" >&2; exit 2 ;; esac

# --no-sandbox requires an explicit diagnostics opt-in; the acceptance path
# may never run unsandboxed.
if [ "$USE_SANDBOX" -eq 0 ] && [ "$DIAGNOSTICS" -eq 0 ]; then
  echo "refusing: --no-sandbox requires --diagnostics (acceptance runs must be sandboxed)" >&2
  exit 2
fi

# ---------------------------------------------------------------------------
# Claude argument policy. Single source of truth, reused by --verify-only.
#   $1 = mcp config path, $2 = task text, $3 = "print"|"interactive"
# ---------------------------------------------------------------------------
ALLOWED_COMPUTER_TOOLS="mcp__computer__computer_describe,mcp__computer__computer_open,mcp__computer__computer_observe,mcp__computer__computer_step,mcp__computer__computer_get_step,mcp__computer__computer_pause,mcp__computer__computer_resume,mcp__computer__computer_close"

CLEAN_SYSTEM_PROMPT='You are a black-box acceptance-test agent driving a controlled test desktop ONLY through the computer_* MCP tools (screenshots, mouse, keyboard). Hard rules, any violation invalidates the run: (1) never open a terminal or run shell/AppleScript/PowerShell/CDP; (2) never read/write files or use a browser; (3) never open, view, or reference any source-code directory or project file on this machine; (4) every conclusion must come from screenshots you actually received — never from assumptions, memory, or mental arithmetic; (5) report failures honestly. When the task is complete or impossible, call computer_close and stop.'

build_claude_args() {
  local mcp_config="$1" task_text="$2" mode="$3"
  CLAUDE_ARGS=(
    # Disable ALL built-in tools; only the MCP computer tools remain.
    # (--allowedTools by itself is auto-approval, NOT isolation.)
    --tools ''
    --allowedTools "$ALLOWED_COMPUTER_TOOLS"
    --mcp-config "$mcp_config"
    --strict-mcp-config
    --disable-slash-commands
    # Per-invocation settings: no hooks from user/project config.
    --settings '{"disableAllHooks":true}'
    # Only user-level settings load: the user's model/auth config stays
    # exactly as-is; stray project/local settings near the release dir do
    # NOT affect the run.
    --setting-sources user
    # Clean system prompt REPLACES the default (no inherited role context).
    --system-prompt "$CLEAN_SYSTEM_PROMPT"
  )
  # Task text must be supplied in BOTH modes; interactive mode previously
  # launched with no task at all (defect #13). Print mode gets the
  # auditable transcript format, no on-disk session persistence, and a
  # bounded turn budget.
  if [ "$mode" = "print" ]; then
    CLAUDE_ARGS+=(
      --print
      --output-format stream-json
      --verbose
      --no-session-persistence
      --max-turns "$MAX_TURNS"
      "$task_text"
    )
  else
    CLAUDE_ARGS+=("$task_text")
  fi
}

# ---------------------------------------------------------------------------
# --verify-only: run the FULL argv policy against a throwaway fake `claude`
# that records its argv, then audit the recorded argv. This verifies script
# policy only — it is NOT evidence about a real GUI run.
# ---------------------------------------------------------------------------
if [ "$VERIFY_ONLY" -eq 1 ]; then
  TMP="$(mktemp -d -t rpa-computer-verify.XXXXXX)"
  trap 'rm -rf "$TMP"' EXIT
  mkdir -p "$TMP/bin" "$TMP/release/mcp" "$TMP/out"
  cat > "$TMP/bin/claude" <<'FAKE'
#!/usr/bin/env bash
# Fake claude CLI: records argv for policy auditing. Emits no GUI behavior.
i=0
out="${FAKE_CLAUDE_OUT:?}/argv.txt"
: > "$out"
for a in "$@"; do printf 'arg[%d]=%s\n' "$i" "$a" >> "$out"; i=$((i+1)); done
printf 'FAKE_CLAUDE_INVOKED\n'
FAKE
  chmod +x "$TMP/bin/claude"
  : > "$TMP/release/tasks.md"
  cat > "$TMP/release/mcp/mcp.json" <<EOF
{"mcpServers":{"computer":{"command":"$TMP/release/bin/computer-host","args":[],"env":{}}}}
EOF
  mkdir -p "$TMP/release/bin" && : > "$TMP/release/bin/computer-host" && chmod +x "$TMP/release/bin/computer-host"

  TASK_TEXT="VERIFY-ONLY TASK PLACEHOLDER"
  build_claude_args "$TMP/release/mcp/mcp.json" "$TASK_TEXT" "print"
  FAKE_CLAUDE_OUT="$TMP/out" CLAUDE_BIN="$TMP/bin/claude" \
    "$TMP/bin/claude" "${CLAUDE_ARGS[@]}" >/dev/null

  args_file="$TMP/out/argv.txt"
  fail=0
  need_fixed() { # a fixed arg token that must appear verbatim
    grep -qxF "arg[$1]=$2" "$args_file" || { echo "VERIFY FAIL: missing $2 at position $1" >&2; fail=1; }
  }
  # Fixed prefix positions (keep in sync with build_claude_args).
  need_fixed 0 "--tools"
  need_fixed 1 ""
  need_fixed 2 "--allowedTools"
  need_fixed 3 "$ALLOWED_COMPUTER_TOOLS"
  need_fixed 4 "--mcp-config"
  need_fixed 6 "--strict-mcp-config"
  need_fixed 7 "--disable-slash-commands"
  need_fixed 8 "--settings"
  need_fixed 9 '{"disableAllHooks":true}'
  need_fixed 10 "--setting-sources"
  need_fixed 11 "user"
  need_fixed 12 "--system-prompt"
  need_fixed 14 "--print"
  need_fixed 15 "--output-format"
  need_fixed 16 "stream-json"
  need_fixed 17 "--verbose"
  need_fixed 18 "--no-session-persistence"
  need_fixed 19 "--max-turns"
  need_fixed 21 "$TASK_TEXT"
  # Policy denials: no bypass flags anywhere in the argv.
  if grep -qE 'dangerously-skip-permissions|bypassPermissions|--permission-mode' "$args_file"; then
    echo "VERIFY FAIL: forbidden permission-bypass flag present" >&2; fail=1
  fi
  # Interactive mode must ALSO carry the task text as final positional arg.
  build_claude_args "$TMP/release/mcp/mcp.json" "$TASK_TEXT" "interactive"
  last="${CLAUDE_ARGS[$((${#CLAUDE_ARGS[@]}-1))]}"
  [ "$last" = "$TASK_TEXT" ] || { echo "VERIFY FAIL: interactive mode lacks task text" >&2; fail=1; }
  for a in "${CLAUDE_ARGS[@]}"; do
    case "$a" in
      --print|--output-format|--no-session-persistence)
        echo "VERIFY FAIL: interactive mode must not use print-only flag $a" >&2; fail=1 ;;
    esac
  done

  [ "$fail" -eq 0 ] || { echo "VERIFY-ONLY: FAILED (see above)" >&2; exit 1; }
  echo "VERIFY-ONLY OK: tool-disable argv policy correct ($(wc -l < "$args_file" | tr -d ' ') argv entries audited)"
  echo "NOTE: this validates script argv policy only — not real GUI behavior."
  exit 0
fi

# ---------------------------------------------------------------------------
# Normal (real) launch path
# ---------------------------------------------------------------------------
[ -n "$RELEASE_DIR" ] || { echo "--release-dir required" >&2; exit 2; }
[ -n "$TASK" ] || { echo "--task required" >&2; exit 2; }
[ -d "$RELEASE_DIR" ] || { echo "release dir missing: $RELEASE_DIR" >&2; exit 1; }
[ -f "$TASK" ] || { echo "task file missing: $TASK" >&2; exit 1; }

# Canonicalize release/source dirs: reject symlink aliases so the sandbox
# profile and the real tree can never diverge.
canonical_dir() {
  local p="$1"
  [ -d "$p" ] || { echo "no such directory: $p" >&2; exit 1; }
  (cd "$p" && pwd -P)
}
RELEASE_DIR="$(canonical_dir "$RELEASE_DIR")"
TASK_DIR="$(cd "$(dirname "$TASK")" && pwd -P)"
TASK="$TASK_DIR/$(basename "$TASK")"
[ -d "$SOURCE_DIR" ] && SOURCE_DIR="$(canonical_dir "$SOURCE_DIR")"

case "$TASK" in
  "$RELEASE_DIR"/*) : ;; # tasks normally live in the release dir
  "$SOURCE_DIR"/*)
    echo "refusing: task file lives in the source tree (would put source path in agent context)" >&2
    exit 1 ;;
esac

# Task text must be nonempty and must demand a screenshot-based outcome
# (continuation-review: no empty/answer-in-prompt tasks).
TASK_TEXT="$(cat "$TASK")"
if [ -z "${TASK_TEXT//[[:space:]]/}" ]; then
  echo "refusing: task file is empty/whitespace: $TASK" >&2
  exit 1
fi
if ! printf '%s' "$TASK_TEXT" | grep -qiE '截图|screenshot|观察|observe'; then
  echo "refusing: task text does not demand a screenshot/observe-based outcome" >&2
  echo "  (acceptance tasks must require conclusions from actual screenshots)" >&2
  exit 1
fi

# Lock-screen safety: the agent must recognize a login/lock/credential screen
# as a BLOCKED environment, never type into it (continuation-review QA D).
CLEAN_SYSTEM_PROMPT="$CLEAN_SYSTEM_PROMPT If any screenshot shows a login, lock, or credential screen, stop immediately, call computer_close, and report that the environment is blocked by a lock screen; never type credentials, passwords, or task text into such a screen."

# Release dir must not overlap the source tree in EITHER direction, and must
# not sit at a filesystem/temp root (continuation-review QA2 follow-up).
is_within_dir() {  # $1 within-or-equal $2 (canonical, no trailing slash)
  case "$1" in
    "$2"|"$2"/*) return 0 ;;
    *) return 1 ;;
  esac
}
if is_within_dir "$RELEASE_DIR" "$SOURCE_DIR" || is_within_dir "$SOURCE_DIR" "$RELEASE_DIR"; then
  echo "refusing: release dir overlaps the source tree: $RELEASE_DIR vs $SOURCE_DIR" >&2
  exit 1
fi
case "$RELEASE_DIR" in
  /|/tmp|/private/tmp|"$(cd "${TMPDIR:-/tmp}" 2>/dev/null && pwd -P)"|"$HOME")
    echo "refusing: release dir is a filesystem/temp/home root: $RELEASE_DIR" >&2
    exit 1 ;;
esac

# Release dir must be source-FREE, not merely path-disjoint: any project
# source artifact anywhere in it invalidates the black-box claim.
if find "$RELEASE_DIR" \( -name '*.rs' -o -name 'Cargo.toml' -o -name 'Cargo.lock' \
    -o -name '.git' -o -name 'CLAUDE.md' \) -print -quit | grep -q .; then
  echo "refusing: release dir contains source artifacts (.rs/Cargo/.git/CLAUDE.md): $RELEASE_DIR" >&2
  exit 1
fi

# If the release dir's path itself matches this tool family's scratch-name
# pattern (rpa-computer-*/rpa-core-*), the agent's cwd would sit under the
# scratch-copy deny and the run could not function. That name shape is
# reserved for STAGED SOURCE COPIES, so refuse it as a release destination
# (a safe-named release under a temp root is fine and must keep working).
case "$(basename "$RELEASE_DIR")" in
  rpa-computer-*|rpa-core-*)
    echo "refusing: release dir name matches the staged-source-copy scratch pattern: $RELEASE_DIR" >&2
    echo "  (choose a safe, non-rpa-* name for the release directory)" >&2
    exit 1 ;;
esac

MCP_CONFIG="$RELEASE_DIR/mcp/mcp.json"
[ -f "$MCP_CONFIG" ] || { echo "MCP config missing: $MCP_CONFIG" >&2; exit 1; }

# The MCP config must reference only release-dir binaries (no source paths).
if grep -qF "$SOURCE_DIR" "$MCP_CONFIG"; then
  echo "refusing: MCP config references the source tree" >&2
  exit 1
fi

# No symlinks anywhere in the release dir (fail closed).
if find "$RELEASE_DIR" -type l | grep -q .; then
  echo "refusing: release dir contains symlinks" >&2; exit 1
fi

CLAUDE_BIN="${CLAUDE_BIN:-claude}"
command -v "$CLAUDE_BIN" >/dev/null 2>&1 || { echo "claude CLI not found" >&2; exit 1; }

MODE="interactive"
[ "$PRINT_MODE" -eq 1 ] && MODE="print"
build_claude_args "$MCP_CONFIG" "$TASK_TEXT" "$MODE"

# Evidence capture (print mode): transcript + stderr land in the release
# dir's evidence/ area — outside the source tree, 0700, no credentials
# (tokens are never passed to claude; the MCP child reads its own token
# file).
#
# Watchdog ownership model (continuation-review QA2 follow-up): the child
# is spawned by scripts/watchdog-launch.py with start_new_session=True, so
# the child's pgid IS its pid and every descendant (sandbox-exec -> claude
# -> MCP/bridge) inherits that pgid. On budget expiry the helper SIGTERMs
# then SIGKILLs exactly that verified group — never the caller's group (a
# bash background job without job control stays in the CALLER'S group,
# which is why the previous in-bash `kill -- -$CHILD_PID` could signal the
# wrong group or nothing). The child tree reads the task from argv with
# stdin=DEVNULL, so there is no unbounded background input to drain;
# host-side stdio EOF behaviour of MCP children after CLI exit is
# separately UNVERIFIED and recorded honestly in the QA report.
EVIDENCE_DIR=""
if [ "$PRINT_MODE" -eq 1 ]; then
  RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
  EVIDENCE_DIR="$RELEASE_DIR/evidence/$RUN_ID"
  mkdir -p "$EVIDENCE_DIR"
  chmod 700 "$EVIDENCE_DIR"
fi

# Run the (possibly sandboxed) claude child in its OWN session via
# scripts/watchdog-launch.py, capture evidence, let the helper enforce the
# wall-clock budget against exactly the child's process group, then audit.
exec_and_audit() {
  local runner=("$@")
  if [ "$PRINT_MODE" -eq 0 ]; then
    cd "$RELEASE_DIR"
    exec "${runner[@]}"
  fi

  local rc=0
  ( cd "$RELEASE_DIR" && \
    python3 "$ROOT/scripts/watchdog-launch.py" --max-seconds "$MAX_SECONDS" \
      "$EVIDENCE_DIR/transcript.jsonl" "$EVIDENCE_DIR/stderr.log" -- "${runner[@]}" ) \
    || rc=$?
  printf '%s\n' "$rc" > "$EVIDENCE_DIR/exit-code.txt"

  echo ">> run finished rc=$rc; auditing transcript $EVIDENCE_DIR/transcript.jsonl"
  # The audit may fail the run, but its refusal must NOT exit through `||`
  # (that path would fire the SIGEXIT trap inside `exit` and skip reporting).
  local audit_rc=0
  python3 "$ROOT/scripts/audit-claude-transcript.py" \
    "$EVIDENCE_DIR/transcript.jsonl" \
    --expect-server computer --max-turns "$MAX_TURNS" --max-seconds "$MAX_SECONDS" \
    || audit_rc=$?
  if [ "$audit_rc" -ne 0 ]; then
    echo "refusing: transcript audit FAILED; run is invalid evidence" >&2
    [ "$rc" -eq 0 ] && rc=1
  fi
  echo ">> evidence: $EVIDENCE_DIR"
  trap - EXIT   # run the final exit outside any SIGEXIT trap suspension window
  exit "$rc"
}

# ---------------------------------------------------------------------------
# Sandbox profile (macOS sandbox-exec). ONE single generated profile is used
# for BOTH the negative probe and the final Claude process — probing a weaker
# profile than the real run was the continuation-review QA2 defect.
#
# Deny policy (exact, not temp-root carpet bans):
#   * the source tree itself (canonical subpath);
#   * KNOWN source-containing copies from this project's own prior work
#     (verified to actually hold source before listing; unknown scratch is
#     never touched or erased);
#   * staged temp copies under the rpa-computer-*/rpa-core-* names THIS
#     tool family creates (they may embed source paths or binaries);
#   * the release dir's own scratch subdirs, when it lives under a temp root
#     (test setups only; a production release never lives under a temp root,
#     so no temp root is denied and Claude's own temp usage stays functional).
# Paths are canonical (pwd -P) and free of the sandbox-language
# metacharacters " \ so a double-quoted (subpath "...") literal is exact;
# regex rules are built from fixed strings (literal slash appended) — never
# from unescaped user input.
# ---------------------------------------------------------------------------
build_sandbox_profile() {
  local profile="$1"
  case "$SOURCE_DIR" in
    *\"*|*\\*)
      echo "refusing: source dir contains sandbox-quoting metacharacters: $SOURCE_DIR" >&2
      exit 1 ;;
  esac
  cat > "$profile" <<EOF
(version 1)
;; Default allow so the agent can still use the desktop/network it needs,
;; but explicitly DENY every file read/write of the original source tree.
(allow default)
(deny file-read* file-write* (subpath "$SOURCE_DIR"))
EOF
  # Deny KNOWN source-containing copies from this project's own work
  # (continuation-review QA item 2). Only paths verified to actually hold
  # project source are listed; unknown scratch is never touched/erased.
  local known
  local seen_known=" "
  for known in /tmp/rpa-core-shim /private/tmp/rpa-core-shim; do
    if [ -e "$known/Cargo.toml" ] || [ -d "$known/src" ]; then
      known="$(cd "$known" && pwd -P)"
      case "$known" in
        "$RELEASE_DIR"|"$RELEASE_DIR"/*) continue ;;  # never deny our own cwd
      esac
      case "$seen_known" in *" $known "*) continue ;; esac
      seen_known="$seen_known$known "
      printf '(deny file-read* file-write* (subpath "%s"))\n' "$known" >> "$profile"
    fi
  done
  # Deny this tool family's own scratch copies: <root>/rpa-(computer|core)-*
  # Regex (sandbox-exec has no glob rules) anchored to canonical roots; the
  # trailing slash is appended explicitly (a previous version concatenated
  # without it and matched nothing). Roots are de-duplicated (/tmp
  # canonicalizes to /private/tmp on macOS). A root whose scratch deny
  # would cover the RELEASE DIR itself is skipped — the release dir was
  # already verified source-free, and the run cwd must stay usable (the
  # rpa-*-shaped release-dir case is refused up front).
  local root
  local seen_roots=" "
  for root in /tmp /private/tmp "${TMPDIR:-}"; do
    [ -n "$root" ] && [ -d "$root" ] || continue
    root="$(cd "$root" && pwd -P 2>/dev/null)" || continue
    case "$RELEASE_DIR" in
      "$root"/rpa-computer-*|"$root"/rpa-core-*) continue ;;
    esac
    case "$seen_roots" in *" $root "*) continue ;; esac
    seen_roots="$seen_roots$root "
    printf '(deny file-read* file-write* (regex #"^%s/rpa-(computer|core)-[^/]*"))\n' "$root" >> "$profile"
  done
  # If the release dir itself lives under a temp root (test/diagnostics
  # setups only), deny ONLY its own rpa-*-shaped scratch siblings — never
  # the whole temp root (Claude needs temp space, and a safe-named
  # source-free release under a temp root is legitimate).
  case "$RELEASE_DIR" in
    /tmp/*|/private/tmp/*|"$(cd "${TMPDIR:-/tmp}" 2>/dev/null && pwd -P)"/*)
      printf '(deny file-read* file-write* (regex #"^%s/rpa-(computer|core)-[^/]*"))\n' \
        "$RELEASE_DIR" >> "$profile"
      ;;
  esac
}

run_sandboxed_macos() {
  # ONE profile for the negative probe AND the final Claude process.
  local profile
  profile="$(mktemp -t rpa-computer-sandbox.XXXXXX.sb)"
  build_sandbox_profile "$profile"
  if ! SANDBOX_PROFILE="$profile" "$ROOT/scripts/sandbox-probe.sh" "$SOURCE_DIR" >/dev/null 2>&1; then
    echo "refusing: sandbox negative probe failed on the exact run profile; cannot prove source isolation" >&2
    rm -f "$profile"
    exit 1
  fi
  # Persist the EXACT profile the probe AND the real launch used into this
  # run's evidence dir (0600 — the profile embeds the real source path),
  # with its SHA256 + execution identity (no tokens/auth contents). Before
  # this, the generated profile was deleted with the mktemp file, so a
  # report could never show which profile a run actually ran under.
  if [ -n "$EVIDENCE_DIR" ]; then
    cp "$profile" "$EVIDENCE_DIR/sandbox.sb"
    chmod 600 "$EVIDENCE_DIR/sandbox.sb"
    shasum -a 256 "$EVIDENCE_DIR/sandbox.sb" | awk '{print $1}' > "$EVIDENCE_DIR/sandbox.sb.sha256"
    chmod 600 "$EVIDENCE_DIR/sandbox.sb.sha256"
    {
      printf 'generator-script=%s\n' "$ROOT/scripts/run-blackbox-claude.sh"
      printf 'generator-script-sha256=%s\n' "$(shasum -a 256 "$ROOT/scripts/run-blackbox-claude.sh" | awk '{print $1}')"
      printf 'claude-bin=%s\n' "$CLAUDE_BIN"
      printf 'max-seconds=%s\n' "$MAX_SECONDS"
      printf 'max-turns=%s\n' "$MAX_TURNS"
    } > "$EVIDENCE_DIR/sandbox.sb.meta"
    chmod 600 "$EVIDENCE_DIR/sandbox.sb.meta"
  fi
  exec_and_audit sandbox-exec -f "$profile" "$CLAUDE_BIN" "${CLAUDE_ARGS[@]}"
}

if [ "$USE_SANDBOX" -eq 1 ]; then
  case "$(uname -s)" in
    Darwin)
      command -v sandbox-exec >/dev/null 2>&1 || {
        echo "refusing: sandbox-exec unavailable; fix the OS or use --diagnostics --no-sandbox (invalid isolation)" >&2
        exit 1
      }
      run_sandboxed_macos
      ;;
    *)
      echo "refusing: no OS sandbox implemented for $(uname -s); use --diagnostics --no-sandbox" >&2
      exit 1
      ;;
  esac
else
  echo "WARNING: DIAGNOSTICS run WITHOUT OS filesystem isolation; this run is NOT" >&2
  echo "valid isolation evidence and must be reported as such." >&2
  exec_and_audit "$CLAUDE_BIN" "${CLAUDE_ARGS[@]}"
fi
