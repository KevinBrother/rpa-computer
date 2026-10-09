#!/usr/bin/env bash
# package-remote-client.sh — stage a source-free, credential-free release
# directory for the REMOTE computer-client (Mac Claude -> local MCP client
# -> TLS -> Windows host).
#
# The release directory contains ONLY:
#   bin/computer-client         the compiled client binary (--binary PATH)
#   mcp/mcp.json                strict MCP config referencing EXTERNAL
#                               --ca-cert / --token-file paths; the token
#                               and CA cert are NEVER copied into the release
#   tasks/                      acceptance task prompts (examples/acceptance)
#                               for launcher compatibility with
#                               scripts/run-blackbox-claude.sh
#   RELEASE.json                packaging metadata (no secrets; JSON-escaped)
#   MANIFEST.sha256             sha256 of every shipped file (generated last)
#
# It must NOT contain: source code, Cargo files, .git, tokens, private keys,
# certificates, symlinks. Verified before exit.
#
# SAFETY: --release-dir must NOT already exist (fresh directory only; this
# tool never deletes or reuses anything), must not overlap the source tree,
# and no ancestor may be a symlink. All copied payloads are regular files
# (symlinks refused). Every path in mcp.json is ABSOLUTE so the MCP client
# can be launched from any cwd. Both mcp.json and RELEASE.json are emitted
# as JSON via python3 — no sed interpolation of user-controlled paths.
#
# Usage:
#   scripts/package-remote-client.sh \
#       --binary PATH --release-dir PATH \
#       --connect HOST:PORT --server-name DNS_NAME \
#       --ca-cert PATH --token-file PATH
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd -P)"

BINARY=""
RELEASE_DIR=""
CONNECT=""
SERVER_NAME=""
CA_CERT=""
TOKEN_FILE=""

while [ $# -gt 0 ]; do
  case "$1" in
    --binary)      BINARY="$2"; shift 2 ;;
    --release-dir) RELEASE_DIR="$2"; shift 2 ;;
    --connect)     CONNECT="$2"; shift 2 ;;
    --server-name) SERVER_NAME="$2"; shift 2 ;;
    --ca-cert)     CA_CERT="$2"; shift 2 ;;
    --token-file)  TOKEN_FILE="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

fail() { echo "error: $*" >&2; exit 2; }

[ -n "$BINARY" ]      || fail "--binary is required"
[ -n "$RELEASE_DIR" ] || fail "--release-dir is required"
[ -n "$CONNECT" ]     || fail "--connect is required"
[ -n "$SERVER_NAME" ] || fail "--server-name is required"
[ -n "$CA_CERT" ]     || fail "--ca-cert is required"
[ -n "$TOKEN_FILE" ]  || fail "--token-file is required"

# Resolve every input to an absolute, canonical path (secrets stay OUTSIDE
# the release; absolute paths keep the MCP config valid from any cwd).
# Relative paths resolve against the CURRENT cwd, never against the repo.
# Symlinks are refused BEFORE canonicalization — realpath would otherwise
# silently resolve the link away.
canonical_file() {
  python3 -c 'import os,sys
p = os.path.abspath(sys.argv[1])
if os.path.islink(p):
    print(f"error: input must not be a symlink: {p}", file=sys.stderr)
    sys.exit(2)
print(os.path.realpath(p))' "$1" || exit 2
}
BINARY="$(canonical_file "$BINARY")"
CA_CERT="$(canonical_file "$CA_CERT")"
TOKEN_FILE="$(canonical_file "$TOKEN_FILE")"

# --connect must be exactly HOST:PORT (no scheme, no slash, no whitespace);
# IPv6 literals must be bracketed ([v6]:port) so they form a valid
# SocketAddr for the Rust client. The host is parsed as an IP or validated
# as a DNS name by python3 — no character-class guessing.
CONNECT_CHECK="$(python3 - "$CONNECT" <<'PYEOF'
import ipaddress, re, sys
connect = sys.argv[1]
dns_re = re.compile(
    r"^(?=.{1,253}$)[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?"
    r"(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?)*$"
)
try:
    if connect.startswith("["):
        end = connect.find("]")
        if end < 0 or end + 1 >= len(connect) or connect[end + 1] != ":":
            raise ValueError("bracketed IPv6 must be [addr]:port")
        ipaddress.IPv6Address(connect[1:end])
        port_s = connect[end + 2:]
        host = connect[:end + 1]
    else:
        if connect.count(":") != 1:
            raise ValueError("unbracketed target must have exactly one colon")
        host, port_s = connect.rsplit(":", 1)
        try:
            ipaddress.IPv4Address(host)
        except ValueError:
            try:
                ipaddress.IPv6Address(host)
            except ValueError:
                pass
            else:
                raise ValueError("IPv6 literals must be bracketed: [addr]:port")
            if not dns_re.match(host):
                raise ValueError("host is neither an IP nor a valid DNS name")
    port = int(port_s, 10)
    if not (1 <= port <= 65535):
        raise ValueError("port out of range")
    print(host)
    print(port)
except ValueError as e:
    print(f"error: --connect must be HOST:PORT ({e}), got: {connect}",
          file=sys.stderr)
    sys.exit(2)
PYEOF
)" || exit 2
CONNECT_HOST="$(printf '%s\n' "$CONNECT_CHECK" | sed -n 1p)"
CONNECT_PORT="$(printf '%s\n' "$CONNECT_CHECK" | sed -n 2p)"

# --server-name: same conservative DNS-name shape as remote-credentials.py
# (which issues DNS-SAN certs only), so reject IP literals here too.
case "$SERVER_NAME" in
  "" | *[!A-Za-z0-9.-]* | .* | *..* | -* | *- | *.-* | *-.* )
    fail "--server-name must be a plain DNS name, got: $SERVER_NAME" ;;
esac
python3 -c 'import ipaddress,sys
try: ipaddress.ip_address(sys.argv[1])
except ValueError: sys.exit(0)
sys.exit(1)' "$SERVER_NAME" \
  || fail "--server-name must be a DNS name, not an IP literal: $SERVER_NAME"

# Inputs must be regular files (never symlinks; refuse arbitrary-clobber
# tricks and make manifest hashes meaningful).
[ -f "$BINARY" ] && [ ! -L "$BINARY" ] || fail "--binary is not a regular file: $BINARY"
[ -f "$CA_CERT" ] && [ ! -L "$CA_CERT" ] || fail "--ca-cert is not a regular file: $CA_CERT"
[ -f "$TOKEN_FILE" ] && [ ! -L "$TOKEN_FILE" ] || fail "--token-file is not a regular file: $TOKEN_FILE"

# The release dir must be NEW, must not overlap the source tree, and no
# ancestor may be a symlink. We never delete or overwrite anything.
if [ -e "$RELEASE_DIR" ] || [ -L "$RELEASE_DIR" ]; then
  fail "--release-dir already exists (refusing to clobber): $RELEASE_DIR"
fi
# The rpa-computer-*/rpa-core-* name shape is RESERVED for staged source
# copies (the black-box sandbox denies it); releases must never use it.
case "$(basename "$RELEASE_DIR")" in
  rpa-computer-* | rpa-core-* ) fail "release dir name is reserved for staged source copies" ;;
esac

# Canonicalize the release dir: absolute, no symlink ancestors (every
# component must already be canonical), and refuse any overlap with ROOT.
RELEASE_DIR="$(python3 - "$RELEASE_DIR" "$ROOT" <<'PYEOF'
import os, sys
release, root = sys.argv[1], sys.argv[2]
if not os.path.isabs(release):
    release = os.path.join(os.getcwd(), release)
release = os.path.normpath(release)
parent = os.path.dirname(release) or os.sep
if not os.path.isdir(parent):
    print(f"error: parent directory of --release-dir does not exist: {parent}",
          file=sys.stderr)
    sys.exit(2)
# realpath refuses any non-canonical ancestor (symlink, alias) and
# canonicalizes macOS /var -> /private/var style system aliases.
if os.path.realpath(release) != release:
    print("error: --release-dir must be canonical (no symlink ancestors); "
          f"resolves to: {os.path.realpath(release)}", file=sys.stderr)
    sys.exit(2)
# No overlap with the source tree in either direction.
def is_within(p, base):
    try:
        return os.path.commonpath([p, base]) == base
    except ValueError:
        return False
if release == root or is_within(release, root) or is_within(root, release):
    print("error: --release-dir must not overlap the source tree: "
          f"{release}", file=sys.stderr)
    sys.exit(2)
print(release)
PYEOF
)" || exit 2

mkdir "$RELEASE_DIR"
mkdir "$RELEASE_DIR/bin" "$RELEASE_DIR/mcp" "$RELEASE_DIR/tasks"

# Ship ONLY the compiled client binary (no source, no credentials).
cp "$BINARY" "$RELEASE_DIR/bin/computer-client"
chmod 0755 "$RELEASE_DIR/bin/computer-client"

# Acceptance task prompts for the black-box launcher (task text only).
cp "$ROOT"/examples/acceptance/*.md "$RELEASE_DIR/tasks/"

# Strict MCP config AND RELEASE.json metadata: both are generated with
# python3 json.dump so every path is properly escaped (a quote or backslash
# in a path can never break the JSON). --server-name is always passed
# explicitly. The CA/token paths point OUTSIDE the release — secrets are
# referenced, never copied. RELEASE.json carries path + SHA-256 only,
# never secret content.
export MCP_CONFIG="$RELEASE_DIR/mcp/mcp.json"
export MCP_BIN="$RELEASE_DIR/bin/computer-client"
export MCP_CONNECT="$CONNECT"
export MCP_SERVER_NAME="$SERVER_NAME"
export MCP_CA="$CA_CERT"
export MCP_TOKEN="$TOKEN_FILE"
export MCP_RELEASE_META="$RELEASE_DIR/RELEASE.json"
python3 - <<'PYEOF'
import datetime
import hashlib
import json
import os

def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()

cfg = {
    "mcpServers": {
        "computer": {
            "command": os.environ["MCP_BIN"],
            "args": [
                "--connect", os.environ["MCP_CONNECT"],
                "--ca-cert", os.environ["MCP_CA"],
                "--server-name", os.environ["MCP_SERVER_NAME"],
                "--token-file", os.environ["MCP_TOKEN"],
            ],
            "env": {},
        }
    }
}
with open(os.environ["MCP_CONFIG"], "w") as f:
    json.dump(cfg, f, indent=2)
    f.write("\n")

meta = {
    "kind": "computer-remote-client",
    "created_utc": datetime.datetime.now(datetime.timezone.utc)
        .strftime("%Y-%m-%dT%H:%M:%SZ"),
    "connect": os.environ["MCP_CONNECT"],
    "server_name": os.environ["MCP_SERVER_NAME"],
    "ca_cert_path": os.environ["MCP_CA"],
    "ca_cert_sha256": sha256(os.environ["MCP_CA"]),
    "token_file_path": os.environ["MCP_TOKEN"],
    "token_file_sha256": sha256(os.environ["MCP_TOKEN"]),
    "note": "credentials are referenced externally and are NOT part of this release",
}
with open(os.environ["MCP_RELEASE_META"], "w") as f:
    json.dump(meta, f, indent=2)
    f.write("\n")
PYEOF

# Verify the shipped tree BEFORE writing the manifest: no symlinks, no
# source/cargo/git, no credential material inside the release.
if find "$RELEASE_DIR" -type l | grep -q .; then
  fail "symlink found in release dir"
fi
if find "$RELEASE_DIR" \( -name '*.rs' -o -name 'Cargo.toml' -o -name '.git' \
     -o -name '*.token' -o -name '*.key' -o -name '*.pem' \) | grep -q .; then
  fail "source or credential material found in release dir"
fi

# Manifest LAST so it covers every shipped file except itself.
( cd "$RELEASE_DIR" && find . -type f ! -name MANIFEST.sha256 -print0 \
    | sort -z | xargs -0 shasum -a 256 > MANIFEST.sha256 )

echo "packaged remote client release: $RELEASE_DIR"
echo "  bin:     $RELEASE_DIR/bin/computer-client"
echo "  mcp:     $MCP_CONFIG"
echo "  tasks:   $RELEASE_DIR/tasks/"
echo "  connect: $CONNECT (server-name: $SERVER_NAME)"
echo "  ca-cert: $CA_CERT (external, not copied)"
echo "  token:   $TOKEN_FILE (external, not copied)"
