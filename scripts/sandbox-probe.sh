#!/usr/bin/env bash
# sandbox-probe.sh — NEGATIVE probe for the macOS sandbox profile used by
# run-blackbox-claude.sh.
#
# Verifies that a process running under the deny-profile CANNOT read the
# source tree, and CAN still do basic benign work (so we know the profile
# itself loads). Exit 0 only if isolation works. This is the evidence that
# file isolation is real, not prompt-theater.
#
# Usage:
#   scripts/sandbox-probe.sh [SOURCE_DIR]            # probe a fresh profile
#   SANDBOX_PROFILE=path.sb scripts/sandbox-probe.sh [SOURCE_DIR]
#                                                    # probe THE SAME profile
#                                                    # the real run will use
#                                                    # (used by run-blackbox)
set -euo pipefail

SOURCE_DIR="${1:-$(cd "$(dirname "$0")/.." && pwd)}"
[ -d "$SOURCE_DIR" ] || { echo "no such source dir: $SOURCE_DIR" >&2; exit 2; }

command -v sandbox-exec >/dev/null 2>&1 || { echo "sandbox-exec not available" >&2; exit 1; }

# Canonicalize: deny rules and the probe must agree on the real path, never
# on a symlink alias.
SOURCE_DIR="$(cd "$SOURCE_DIR" && pwd -P)"

PROFILE=""
CLEANUP=0
if [ -n "${SANDBOX_PROFILE:-}" ]; then
  PROFILE="$SANDBOX_PROFILE"
  [ -f "$PROFILE" ] || { echo "no such profile: $PROFILE" >&2; exit 2; }
else
  PROFILE="$(mktemp -t rpa-computer-probe.XXXXXX.sb)"
  CLEANUP=1
  case "$SOURCE_DIR" in
    *\"*|*\\*) echo "source dir has sandbox-quoting metacharacters" >&2; exit 2 ;;
  esac
  {
    cat <<EOF
(version 1)
(allow default)
(deny file-read* file-write* (subpath "$SOURCE_DIR"))
EOF
    # Same scratch-regex + known-source-copy deny set the run-blackbox
    # probe profile carries, so standalone probes exercise 4b/4c too.
    root=""
    seen=" "
    for root in /tmp /private/tmp "${TMPDIR:-}"; do
      [ -n "$root" ] && [ -d "$root" ] || continue
      root="$(cd "$root" && pwd -P 2>/dev/null)" || continue
      case "$seen" in *" $root "*) continue ;; esac
      seen="$seen$root "
      printf '(deny file-read* file-write* (regex #"^%s/rpa-(computer|core)-[^/]*"))\n' "$root"
    done
    for known in /tmp/rpa-core-shim /private/tmp/rpa-core-shim; do
      if [ -e "$known/Cargo.toml" ] || [ -d "$known/src" ]; then
        printf '(deny file-read* file-write* (subpath "%s"))\n' "$(cd "$known" && pwd -P)"
      fi
    done
  } > "$PROFILE"
fi
[ "$CLEANUP" -eq 0 ] || trap 'rm -f "$PROFILE"' EXIT

probe_file="$SOURCE_DIR/Cargo.toml"
[ -f "$probe_file" ] || probe_file="$(find "$SOURCE_DIR" -maxdepth 1 -type f | head -1)"

# 1) Profile loads and benign command works.
if ! sandbox-exec -f "$PROFILE" /usr/bin/true; then
  echo "FAIL: sandbox profile does not load" >&2
  exit 1
fi

# 2) Reading a source file under the sandbox must FAIL.
if sandbox-exec -f "$PROFILE" /bin/cat "$probe_file" >/dev/null 2>&1; then
  echo "FAIL: source file WAS readable under sandbox: $probe_file" >&2
  exit 1
fi

# 3) Listing the source dir under the sandbox must FAIL.
if sandbox-exec -f "$PROFILE" /bin/ls "$SOURCE_DIR" >/dev/null 2>&1; then
  echo "FAIL: source dir WAS listable under sandbox: $SOURCE_DIR" >&2
  exit 1
fi

# 4) Writing into the source tree must also FAIL (release safety). Skip
# silently when this probe's fresh profile only denies reads (standalone
# diagnostic use); the run profile always denies writes and is exercised.
if grep -q 'file-write' "$PROFILE"; then
  if sandbox-exec -f "$PROFILE" /usr/bin/touch "$SOURCE_DIR/.qa-sandbox-write-probe" >/dev/null 2>&1; then
    rm -f "$SOURCE_DIR/.qa-sandbox-write-probe"
    echo "FAIL: source tree WAS writable under sandbox: $SOURCE_DIR" >&2
    exit 1
  fi
fi

# 4b) If the profile carries a KNOWN source-copy deny (subpath outside the
# main source tree, e.g. /tmp/rpa-core-shim), probe that too — the denial
# list is only as good as its weakest entry. Unknown scratch is never
# touched (read-only cat probe, no deletion).
while IFS= read -r copy; do
  [ -d "$copy" ] || { echo "FAIL: denied source copy missing: $copy" >&2; exit 1; }
  copy_probe="$(find "$copy" -name '*.rs' -type f 2>/dev/null | head -1)"
  [ -n "$copy_probe" ] || copy_probe="$copy/Cargo.toml"
  if sandbox-exec -f "$PROFILE" /bin/cat "$copy_probe" >/dev/null 2>&1; then
    echo "FAIL: known source copy WAS readable under sandbox: $copy_probe" >&2
    exit 1
  fi
done < <(sed -n 's/^.*(subpath "\([^"]*\)")).*$/\1/p' "$PROFILE" | grep -v -x "$SOURCE_DIR")

# 4c) Regex scratch denies must actually match: a temp dir shaped like the
# QA scratch pattern must be unreadable under the profile. Uses its own
# rpa-computer-* mktemp dir (covered by the run profile's own regex).
if grep -q 'regex' "$PROFILE"; then
  scratch="$(mktemp -d -t rpa-computer-probe-regex.XXXXXX)"
  printf 'probe\n' > "$scratch/probe.txt"
  if sandbox-exec -f "$PROFILE" /bin/cat "$scratch/probe.txt" >/dev/null 2>&1; then
    rm -rf "$scratch"
    echo "FAIL: rpa-computer scratch dir WAS readable under sandbox: $scratch" >&2
    exit 1
  fi
  rm -rf "$scratch"
fi

# 5) Control: reading a benign file outside the tree must still work.
if ! sandbox-exec -f "$PROFILE" /bin/cat /etc/hosts >/dev/null 2>&1; then
  echo "FAIL: sandbox broke benign reads outside source tree (profile too broad)" >&2
  exit 1
fi

echo "OK: sandbox denies reads/writes of $SOURCE_DIR (+ any known source copies and rpa-* scratch; probe file: $probe_file)"
