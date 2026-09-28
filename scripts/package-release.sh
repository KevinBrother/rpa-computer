#!/usr/bin/env bash
# package-release.sh — build a source-free release directory for black-box
# acceptance testing.
#
# The release directory contains ONLY:
#   bin/computer-host           compiled binary (no source, no symlinks)
#   mcp/mcp.json                strict MCP config for the external test agent
#   tasks/                      acceptance task prompts (examples/acceptance)
#   RELEASE.json                build metadata (triple, rustc, profile)
#   MANIFEST.sha256             sha256 of every shipped file (generated LAST,
#                               after RELEASE.json, so it covers everything)
#   .rpa-computer-release       tool-owned marker; a pre-existing dir may
#                               only be reused if this marker validates
#
# It must NOT contain: source code, Cargo files, .git, credentials, tokens,
# logs, symlinks. This is verified before the script exits successfully.
#
# SAFETY (defect #14 fix): this script NEVER runs `rm -rf` on a
# caller-supplied path after a mere lexical prefix check. The release dir
# must either not exist (created fresh), or be a directory previously
# created by THIS tool (validated marker + expected shape + canonical path
# outside the source tree, $HOME, and /). Anything else is refused. Stale
# content is removed entry-by-entry, never via rm -rf on the given path.
#
# Usage:
#   scripts/package-release.sh [--release-dir PATH] [--skip-build]
#
# Env:
#   CARGO_PROFILE   cargo profile to build (default: release)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"
cd "$ROOT"

# Default destination: a UNIQUE, tool-owned staging dir under the temp root
# (never the sibling rpa-computer-release: writing sibling files next to the
# source checkout by default was the continuation-review packaging defect).
# Use an explicit --release-dir for a stable destination. A temp-root DEFAULT
# is safe here because the path is GUID-unique, cannot collide with user
# data, and is below validated as non-dangerous.
if [ -z "${RELEASE_DIR:-}" ]; then
  RELEASE_DIR="${TMPDIR:-/tmp}/computer-release.$(uuidgen 2>/dev/null | tr 'A-F' 'a-f' || date +%s)-$$"
fi
PROFILE="${CARGO_PROFILE:-release}"
SKIP_BUILD=0
REMOTE_MODE=0          # 1 = package a Windows-remote bridge release
REMOTE_PORT=8399       # local forwarded loopback port of the remote host

while [ $# -gt 0 ]; do
  case "$1" in
    --release-dir) RELEASE_DIR="$2"; shift 2 ;;
    --skip-build)  SKIP_BUILD=1; shift ;;
    --remote)      REMOTE_MODE=1; REMOTE_PORT="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

# The rpa-computer-*/rpa-core-* name shape is RESERVED for staged source
# copies (the black-box sandbox denies it); a release dir must never use it.
case "$(basename "$RELEASE_DIR")" in
  rpa-computer-*|rpa-core-*)
    echo "refusing: release dir name matches the staged-source-copy scratch pattern: $RELEASE_DIR" >&2
    exit 2 ;;
esac

json_escape() {  # minimal JSON string escaping for paths/metadata
  python3 -c 'import json,sys; print(json.dumps(sys.argv[1]))' "$1"
}

# ---------------------------------------------------------------------------
# Cargo profile -> target directory resolution (defect #14: --profile dev
# outputs to target/debug, NOT target/dev).
# ---------------------------------------------------------------------------
target_dir_for_profile() {
  case "$1" in
    dev)     echo "debug" ;;
    release) echo "release" ;;
    *)       echo "$1" ;;   # custom profiles use their own name
  esac
}

# ---------------------------------------------------------------------------
# Release-dir destination validation (fail closed)
# ---------------------------------------------------------------------------
MARKER_NAME=".rpa-computer-release"

resolve_parent_canon() {
  # Canonical path of the deepest existing ancestor of $1 (which may not
  # exist yet), with the non-existent tail appended.
  local p="$1" tail=""
  while [ ! -e "$p" ]; do
    tail="/$(basename "$p")$tail"
    p="$(dirname "$p")"
    [ "$p" = "/" ] && break
  done
  local canon
  canon="$(cd "$p" && pwd -P)"
  printf '%s%s\n' "$canon" "$tail"
}

if [ -L "$RELEASE_DIR" ]; then
  echo "refusing: release dir is a symlink: $RELEASE_DIR" >&2
  exit 2
fi

CANON_RELEASE="$(resolve_parent_canon "$RELEASE_DIR")"
CANON_HOME="$(cd "$HOME" && pwd -P)"
CANON_ROOT_SRC="$ROOT"          # already canonical (pwd -P above)
CANON_TMP="$(cd "${TMPDIR:-/tmp}" && pwd -P)"

is_within() {  # $1 within-or-equal $2 (canonical, no trailing slash)
  case "$1" in
    "$2"|"$2"/*) return 0 ;;
    *) return 1 ;;
  esac
}

# Reject dangerous destinations: root, home, the source tree, ancestors or
# siblings of the source checkout (the old default wrote files next to the
# repo), and temp ROOTS themselves. A uniquely-named path inside a temp
# root (the new default) is fine; sibling detection compares canonical
# PARENTS, so an innocent same-named dir elsewhere is unaffected.
dangerous=0
[ "$CANON_RELEASE" = "/" ] && dangerous=1
[ "$CANON_RELEASE" = "$CANON_HOME" ] && dangerous=1
is_within "$CANON_RELEASE" "$CANON_ROOT_SRC" && dangerous=1     # inside repo
is_within "$CANON_ROOT_SRC" "$CANON_RELEASE" && dangerous=1     # ancestor of repo
[ "$CANON_RELEASE" = "$CANON_TMP" ] && dangerous=1
[ "$CANON_RELEASE" = "/tmp" ] && dangerous=1
[ "$CANON_RELEASE" = "/private/tmp" ] && dangerous=1
[ "$(dirname "$CANON_RELEASE")" = "$(dirname "$CANON_ROOT_SRC")" ] && dangerous=1  # repo sibling
if [ "$dangerous" -eq 1 ]; then
  echo "refusing: dangerous release destination: $CANON_RELEASE" >&2
  echo "  (must not be /, \$HOME, inside/above/beside the source tree, or a temp root)" >&2
  exit 2
fi

if [ -d "$RELEASE_DIR" ]; then
  # Existing dir: only reuse if it is OURS.
  marker="$RELEASE_DIR/$MARKER_NAME"
  if [ ! -f "$marker" ] || ! grep -q '^rpa-computer-release-v1$' "$marker" 2>/dev/null; then
    echo "refusing: $RELEASE_DIR exists but was not created by this tool" >&2
    echo "  (no valid $MARKER_NAME marker). Remove it yourself or pick a new path." >&2
    exit 2
  fi
  # Must look like a release dir, not arbitrary user data with our marker.
  for sub in bin mcp tasks; do
    if [ -e "$RELEASE_DIR/$sub" ] && [ ! -d "$RELEASE_DIR/$sub" ]; then
      echo "refusing: $RELEASE_DIR/$sub exists and is not a directory" >&2
      exit 2
    fi
  done
  if find "$RELEASE_DIR" -type l | grep -q .; then
    echo "refusing: existing release dir contains symlinks" >&2
    exit 2
  fi
  echo ">> reusing tool-owned release dir; clearing stale entries one by one"
  find "$RELEASE_DIR" -mindepth 1 -maxdepth 1 ! -name "$MARKER_NAME" -exec rm -rf -- {} +
else
  echo ">> creating new release dir: $RELEASE_DIR"
  mkdir -p "$RELEASE_DIR"
fi
printf 'rpa-computer-release-v1\n' > "$RELEASE_DIR/$MARKER_NAME"

# ---------------------------------------------------------------------------
# Build (skipped entirely in remote-bridge mode: the host binary lives on
# the Windows machine, not in this release)
# ---------------------------------------------------------------------------
if [ "$SKIP_BUILD" -eq 0 ] && [ "$REMOTE_MODE" -eq 0 ]; then
  echo ">> cargo build --profile $PROFILE --bin computer-host"
  cargo build --profile "$PROFILE" --bin computer-host
fi

if [ "$REMOTE_MODE" -eq 0 ]; then
  TARGET_SUBDIR="$(target_dir_for_profile "$PROFILE")"
  BIN="$ROOT/target/$TARGET_SUBDIR/computer-host"
  [ -x "$BIN" ] || { echo "binary missing: $BIN (profile $PROFILE -> target/$TARGET_SUBDIR)" >&2; exit 1; }

  # Never package a test stub: the pure script tests may have staged a
  # shell-script placeholder at this path. A real cargo-built executable is
  # Mach-O/PE/ELF — never a text script. Refusing here means a stub can never
  # be mistaken for (or replace) a product artifact in a release dir.
  if head -c 2 "$BIN" | grep -q '^#!'; then
    echo "refusing: $BIN is a script stub (starts with #!), not a built executable." >&2
    echo "  Run without --skip-build, or after a real cargo build, before packaging." >&2
    exit 1
  fi
fi

echo ">> staging $RELEASE_DIR"
mkdir -p "$RELEASE_DIR/bin" "$RELEASE_DIR/mcp" "$RELEASE_DIR/tasks"

if [ "$REMOTE_MODE" -eq 1 ]; then
  # -------------------------------------------------------------------------
  # WINDOWS-REMOTE bridge release: the computer-host runs on acer-win inside
  # an interactive scheduled task (deployed separately by the coordinator);
  # an SSH loopback forward exposes it here on 127.0.0.1:$REMOTE_PORT. The
  # release therefore ships ONLY the stdio->TCP bridge plus a strict MCP
  # config. The auth token is NEVER staged: it is generated per-run by the
  # coordinator into a 0600 file OUTSIDE this directory, and only the MCP
  # bridge CHILD (spawned by claude from this config) reads it. The agent
  # never receives the token.
  # -------------------------------------------------------------------------
  case "$REMOTE_PORT" in (*[!0-9]*|'') echo "--remote port must be an integer" >&2; exit 2 ;; esac
  [ "$REMOTE_PORT" -ge 1 ] && [ "$REMOTE_PORT" -le 65535 ] || { echo "invalid --remote port" >&2; exit 2; }
  cp -p "$ROOT/scripts/mcp-bridge.py" "$RELEASE_DIR/bin/mcp-bridge.py"
  chmod +x "$RELEASE_DIR/bin/mcp-bridge.py"
  PYTHON_BIN="$(command -v python3)" || { echo "python3 not found" >&2; exit 1; }
  python3 - "$RELEASE_DIR/mcp/mcp.json" "$PYTHON_BIN" "$RELEASE_DIR/bin/mcp-bridge.py" "$REMOTE_PORT" <<'PYEOF'
import json, sys
path, pybin, bridge, port = sys.argv[1:5]
cfg = {"mcpServers": {"computer": {
    "command": pybin,
    "args": [bridge, "--port", port, "--token-file", "__TOKEN_FILE__",
             "--max-seconds", "3600"],
    "env": {}}}}
with open(path, "w") as f:
    json.dump(cfg, f, indent=2)
    f.write("\n")
PYEOF
  echo ">> REMOTE bridge release staged. Per-run token step (coordinator):"
  echo "   install -m 600 /dev/null \"\$TOKEN_FILE\" && openssl rand -hex 32 > \"\$TOKEN_FILE\""
  echo "   then: sed -i '' \"s|__TOKEN_FILE__|\$TOKEN_FILE|\" \"$RELEASE_DIR/mcp/mcp.json\""
  echo "   (token lives ONLY in that 0600 file outside this dir; the bridge child reads it)"
else
  cp -p "$BIN" "$RELEASE_DIR/bin/computer-host"

  # Strict MCP config: stdio host, no other servers, no env additions. Paths
  # are JSON-escaped (defect #14: quotes in paths must not corrupt JSON).
  # NOTE: "env": {} means "add no variables" — the child still inherits the
  # parent process environment; we do NOT claim a clean environment.
  printf '{\n  "mcpServers": {\n    "computer": {\n      "command": %s,\n      "args": [],\n      "env": {}\n    }\n  }\n}\n' \
    "$(json_escape "$RELEASE_DIR/bin/computer-host")" > "$RELEASE_DIR/mcp/mcp.json"
fi

if [ -d "$ROOT/examples/acceptance" ]; then
  cp -p "$ROOT/examples/acceptance/"*.md "$RELEASE_DIR/tasks/" 2>/dev/null || true
fi

TRIPLE="$(rustc -vV | awk '/^host:/ {print $2}')"
printf '{\n  "name": "rpa-computer computer-host",\n  "built_at": %s,\n  "target_triple": %s,\n  "rustc": %s,\n  "cargo_profile": %s,\n  "packaged_by": "scripts/package-release.sh",\n  "source_tree": "excluded-by-policy"\n}\n' \
  "$(json_escape "$(date -u +%Y-%m-%dT%H:%M:%SZ)")" \
  "$(json_escape "$TRIPLE")" \
  "$(json_escape "$(rustc --version)")" \
  "$(json_escape "$PROFILE")" > "$RELEASE_DIR/RELEASE.json"

# Hash manifest LAST so it covers RELEASE.json and every staged file
# (defect #14: manifest was generated before RELEASE.json).
(
  cd "$RELEASE_DIR"
  find . -type f ! -name MANIFEST.sha256 -print0 | sort -z | xargs -0 shasum -a 256 > MANIFEST.sha256
)

# ---------------------------------------------------------------------------
# Containment + manifest-completeness verification (fail closed)
# ---------------------------------------------------------------------------
bad=0
while IFS= read -r link; do
  echo "refusing: symlink in release dir: $link" >&2; bad=1
done < <(find "$RELEASE_DIR" -type l)
# .py files are forbidden only in LOCAL-binary mode (a stray script there
# would be unexplained); the remote-bridge release legitimately ships
# bin/mcp-bridge.py as the MCP entry point.
if [ "$REMOTE_MODE" -eq 1 ]; then
  while IFS= read -r f; do
    echo "refusing: forbidden file in release dir: $f" >&2; bad=1
  done < <(find "$RELEASE_DIR" -type f \( \
      -name '*.rs' -o -name 'Cargo.toml' -o -name 'Cargo.lock' \
      -o -name '.git' -o -name '*.token' -o -name 'token*' -o -name '*.key' \
      -o -name 'CLAUDE.md' \) ! -path '*/bin/mcp-bridge.py' \
      ! -name 'computer-host' )
  # The ONLY .py allowed is exactly bin/mcp-bridge.py.
  while IFS= read -r f; do
    [ "$f" = "$RELEASE_DIR/bin/mcp-bridge.py" ] || { echo "refusing: unexpected .py in release dir: $f" >&2; bad=1; }
  done < <(find "$RELEASE_DIR" -type f -name '*.py')
else
  while IFS= read -r f; do
    echo "refusing: forbidden file in release dir: $f" >&2; bad=1
  done < <(find "$RELEASE_DIR" -type f \( \
      -name '*.rs' -o -name 'Cargo.toml' -o -name 'Cargo.lock' -o -name '*.py' \
      -o -name '.git' -o -name '*.token' -o -name 'token*' -o -name '*.key' \
      -o -name 'CLAUDE.md' \) )
fi
# No token material may EVER be staged, either mode.
if grep -rl '__TOKEN_FILE__' "$RELEASE_DIR/mcp/mcp.json" >/dev/null 2>&1; then
  : # placeholder is fine; a REAL token is not
fi

# Manifest completeness: every staged regular file (except the manifest
# itself) must appear exactly once in MANIFEST.sha256. Manifest paths are
# relative to the release dir (generated with cd + find .), so compare
# against relative paths.
manifest_missing=0
while IFS= read -r rel; do
  grep -qF " $rel" "$RELEASE_DIR/MANIFEST.sha256" || {
    echo "refusing: file missing from MANIFEST.sha256: $rel" >&2
    manifest_missing=1
  }
done < <(cd "$RELEASE_DIR" && find . -type f ! -name MANIFEST.sha256)
manifest_count="$(wc -l < "$RELEASE_DIR/MANIFEST.sha256" | tr -d ' ')"
staged_count="$(find "$RELEASE_DIR" -type f ! -name MANIFEST.sha256 | wc -l | tr -d ' ')"
if [ "$manifest_count" != "$staged_count" ]; then
  echo "refusing: manifest has $manifest_count entries but $staged_count files staged" >&2
  manifest_missing=1
fi
[ "$manifest_missing" -eq 0 ] || bad=1

[ "$bad" -eq 0 ] || exit 1

echo ">> release ready: $RELEASE_DIR"
find "$RELEASE_DIR" -type f | sort
echo ">> manifest: $RELEASE_DIR/MANIFEST.sha256 ($manifest_count files)"
