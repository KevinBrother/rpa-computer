#!/usr/bin/env bash
# deploy-windows-test.sh — copy ONLY release artifacts to the Windows test
# machine (acer-win) and stage an interactive-session startup task.
#
# What this script DOES:
#   * copies bin/computer-host.exe, an MCP/bridge config, a token file
#     (locally generated, 0600), the acceptance task prompts and the
#     start/stop PowerShell scripts to C:\rpa-computer-test\ on acer-win
#   * writes a sha256 manifest of everything deployed (MANIFEST.sha256
#     itself is EXCLUDED from the file list — continuation-review QA item 4:
#     shell redirection creates the file before find runs, so including it
#     would hash a half-written file)
#   * verifies byte-identical transfer by comparing hashes over SSH
#   * remote PowerShell invocations use -EncodedCommand (base64/UTF-16LE):
#     no nested-quote gymnastics, no shell re-parsing (QA item 4)
#   * PARSE-CHECKS the staged .ps1 scripts on the remote (PSPARSER only —
#     nothing is executed, no host/GUI is started)
#
# What this script REFUSES to do (by policy; coordinator does these):
#   * start the host GUI process (must be an INTERACTIVE scheduled task in
#     the explorer session, launched by the coordinator — see
#     scripts/windows-start-host.ps1 which the COORDINATOR invokes)
#   * change firewall/network settings, install toolchains, touch RPAD/Executor
#
# The token is generated locally, transferred in the scp batch, ACL'd on the
# remote to the console-session owner, and is NEVER printed — including in
# error paths (QA item 4).
#
# Usage:
#   scripts/deploy-windows-test.sh --host-ssh acer-win \
#       --binary target/x86_64-pc-windows-msvc/release/computer-host.exe \
#       [--remote-dir 'C:\rpa-computer-test'] [--port 8399]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

SSH_HOST=""
BINARY=""
REMOTE_DIR='C:\rpa-computer-test'
PORT=8399

while [ $# -gt 0 ]; do
  case "$1" in
    --host-ssh)   SSH_HOST="$2"; shift 2 ;;
    --binary)     BINARY="$2"; shift 2 ;;
    --remote-dir) REMOTE_DIR="$2"; shift 2 ;;
    --port)       PORT="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

[ -n "$SSH_HOST" ] || { echo "--host-ssh required" >&2; exit 2; }
[ -n "$BINARY" ] || { echo "--binary required" >&2; exit 2; }
[ -f "$BINARY" ] || { echo "binary missing: $BINARY (build for windows first)" >&2; exit 1; }

# ---------------------------------------------------------------------------
# Remote PowerShell helper: send the script as -EncodedCommand so NOTHING is
# re-parsed by a shell layer. $1 = PowerShell script text.
#
# Use printf '%s' "$script" (never echo) so backtick/escape sequences in the
# PS text are not reinterpreted, and pipe through `iconv | base64 | tr` —
# the EncodedCommand transport expects UTF-16LE base64 with no newlines.
# Callers pass SINGLE-QUOTED heredoc-style literals; '$' inside them is
# literal PS syntax and is NOT expanded locally (bash does not expand
# single-quoted strings). The old blanket "reject any '$'" rule made the
# parse-check/ACL blocks impossible to send and is removed; local expansion
# safety comes from the single-quoted call sites, which are reviewed.
# ---------------------------------------------------------------------------
ps_remote() {
  local script="$1" enc
  enc="$(printf '%s' "$script" | iconv -f UTF-8 -t UTF-16LE | base64 | tr -d '\n')"
  ssh "$SSH_HOST" "powershell -NoProfile -ExecutionPolicy Bypass -EncodedCommand $enc"
}

TOKEN="$(head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n')"
STAGE="$(mktemp -d -t rpa-computer-win-deploy.XXXXXX)"
trap 'rm -rf "$STAGE"' EXIT

mkdir -p "$STAGE/bin" "$STAGE/tasks"
cp -p "$BINARY" "$STAGE/bin/computer-host.exe"
printf '%s' "$TOKEN" > "$STAGE/host.token"
unset TOKEN  # never again referenced: cannot leak via set -x / error paths
chmod 600 "$STAGE/host.token"

cp -p "$ROOT/scripts/windows-start-host.ps1" "$STAGE/"
cp -p "$ROOT/scripts/windows-stop-host.ps1" "$STAGE/"
if [ -d "$ROOT/examples/acceptance" ]; then
  cp -p "$ROOT/examples/acceptance/"*.md "$STAGE/tasks/" 2>/dev/null || true
fi

cat > "$STAGE/start-host.cmd" <<EOF
@echo off
rem Helper for the INTERACTIVE scheduled task. The host listens on loopback
rem only and requires the token as its first line. Logs go to stderr file.
"$REMOTE_DIR\bin\computer-host.exe" --listen 127.0.0.1:$PORT --token-file "$REMOTE_DIR\host.token" 2>> "$REMOTE_DIR\host.stderr.log"
EOF

# Manifest of the payload, EXCLUDING MANIFEST.sha256 itself (the redirect
# creates the file before find runs — it must not hash itself).
(
  cd "$STAGE"
  find . -type f ! -name MANIFEST.sha256 -print0 | sort -z | xargs -0 shasum -a 256 > MANIFEST.sha256
)

echo ">> deploying to $SSH_HOST:$REMOTE_DIR"
ps_remote "New-Item -ItemType Directory -Force -Path '$REMOTE_DIR','$REMOTE_DIR\\bin','$REMOTE_DIR\\tasks' | Out-Null"
scp -q -r "$STAGE/." "$SSH_HOST:$(echo "$REMOTE_DIR" | sed 's|\\|/|g')/"

echo ">> verifying remote hashes (actual remote Get-FileHash, not the uploaded manifest)"
LOCAL_SUMS="$(cd "$STAGE" && find . -type f ! -name MANIFEST.sha256 -print0 | sort -z | xargs -0 shasum -a 256 | awk '{print $1" "$2}')"
FAIL=0
# Hash every payload file ON the remote with Get-FileHash and compare
# against the local hashes. The uploaded MANIFEST.sha256 is informational
# only — trusting it would let a corrupted transfer validate itself. The
# remote script is built locally with '\n' joins (no '$' interpolation).
REMOTE_HASH_SCRIPT="(Get-ChildItem -Recurse -File '$REMOTE_DIR' | Where-Object { \$_.Name -ne 'MANIFEST.sha256' } | ForEach-Object { \$h = (Get-FileHash -Algorithm SHA256 -LiteralPath \$_.FullName).Hash; \$rel = \$_.FullName.Substring('$REMOTE_DIR'.Length).Replace('\\','/'); if (\$rel.StartsWith('/')) { \$rel = \$rel.Substring(1) }; Write-Output (\$h + ' ./' + \$rel) }) -join [char]10"
REMOTE_SUMS_RAW="$(mktemp -t rpa-computer-win-remote-sums.XXXXXX)"
ps_remote "$REMOTE_HASH_SCRIPT" > "$REMOTE_SUMS_RAW"
# Normalize CRLF -> LF: remote PowerShell writes \r\n, which otherwise
# poisons trailing fields (the historical "REMOTE MANIFEST MISSING ENTRY"
# was awk comparing a CR-suffixed path).
tr -d '\r' < "$REMOTE_SUMS_RAW" > "$REMOTE_SUMS_RAW.lf"
mv "$REMOTE_SUMS_RAW.lf" "$REMOTE_SUMS_RAW"
while read -r hash path; do
  rel="${path#./}"
  # Get-FileHash prints UPPERCASE hex; local shasum prints lowercase.
  rhash="$(awk -v p="$path" '{if ($2 == p) {print tolower($1)}}' "$REMOTE_SUMS_RAW")"
  if [ -z "$rhash" ]; then
    echo "REMOTE HASH MISSING ENTRY: $rel" >&2
    FAIL=1
  elif [ "$hash" != "$rhash" ]; then
    echo "HASH MISMATCH: $rel local=$hash remote=$rhash" >&2
    FAIL=1
  fi
done <<< "$LOCAL_SUMS"
rm -f "$REMOTE_SUMS_RAW"

[ "$FAIL" -eq 0 ] || { echo "deployment verification FAILED (token content is never printed)" >&2; exit 1; }

# ---------------------------------------------------------------------------
# Parse-check (NOT execute) the deployed PowerShell scripts remotely. Uses
# PSParser::Tokenize; a parse error fails the deployment. No GUI, no host.
# ---------------------------------------------------------------------------
echo ">> parse-checking deployed .ps1 scripts (parse only, nothing executed)"
ps_remote '
$bad = $false
foreach ($f in @("windows-start-host.ps1","windows-stop-host.ps1")) {
  $p = Join-Path "'"$REMOTE_DIR"'" $f
  $errs = $null
  [void][System.Management.Automation.PSParser]::Tokenize((Get-Content $p -Raw), [ref]$errs)
  if ($errs -and $errs.Count -gt 0) {
    $bad = $true
    foreach ($e in $errs) { Write-Output ("PARSE ERROR {0}: {1} (line {2})" -f $f, $e.Message, $e.Token.StartLine) }
  } else {
    Write-Output ("PARSE OK {0}" -f $f)
  }
}
if ($bad) { exit 1 }
'

# ---------------------------------------------------------------------------
# ACL the token file BY SID: allow-ACEs must resolve to a subset of
# {console explorer owner, SYSTEM S-1-5-18, Administrators S-1-5-32-544}.
# Display-name matching is locale-fragile; SIDs are not. The token content
# is never read back or printed.
# ---------------------------------------------------------------------------
echo ">> restricting remote token ACL (SID allowlist: console owner + SYSTEM + Administrators)"
ps_remote '
Add-Type -Namespace WtsX -Name ApiX -MemberDefinition @"
[System.Runtime.InteropServices.DllImport("kernel32.dll")]
public static extern uint WTSGetActiveConsoleSessionId();
"@
$sess = [WtsX.ApiX]::WTSGetActiveConsoleSessionId()
$exp = Get-Process explorer -ErrorAction SilentlyContinue | Where-Object { $_.SessionId -eq $sess } | Select-Object -First 1
if (-not $exp) { Write-Error "no console explorer on session $sess"; exit 1 }
$o = (Get-CimInstance Win32_Process -Filter "ProcessId=$($exp.Id)" | Invoke-CimMethod -MethodName GetOwner)
$acct = if ($o.Domain) { "$($o.Domain)\$($o.User)" } else { $o.User }
$ownerSid = ([System.Security.Principal.NTAccount]$acct).Translate([System.Security.Principal.SecurityIdentifier]).Value
$p = "'"$REMOTE_DIR"'\host.token"
$acl = Get-Acl $p
$acl.SetAccessRuleProtection($true, $false)
foreach ($sid in @($ownerSid, "S-1-5-18", "S-1-5-32-544")) {
  $rule = New-Object System.Security.AccessControl.FileSystemAccessRule(
    (New-Object System.Security.Principal.SecurityIdentifier($sid)), "FullControl", "Allow")
  $acl.AddAccessRule($rule)
}
Set-Acl $p $acl
foreach ($ace in (Get-Acl $p).Access) {
  if ($ace.AccessControlType -ne "Allow") { continue }
  $aceSid = $ace.IdentityReference.Translate([System.Security.Principal.SecurityIdentifier]).Value
  if (@($ownerSid, "S-1-5-18", "S-1-5-32-544") -notcontains $aceSid) {
    Write-Error "token ACL still allows unexpected SID $aceSid"; exit 1
  }
}
Write-Output "token ACL OK (owner=$acct; SID allowlist verified)"
'

echo ">> deployment OK. Manifest: MANIFEST.sha256 (excludes itself; covers every payload file)."
echo ">> NEXT (coordinator only): schedule interactive host start, e.g."
echo "   ssh $SSH_HOST powershell -File $REMOTE_DIR\\windows-start-host.ps1 -RemoteDir '$REMOTE_DIR' -Port $PORT"
