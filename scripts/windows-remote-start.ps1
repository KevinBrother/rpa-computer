# windows-remote-start.ps1 — start computer-host in TLS REMOTE LISTEN mode
# in the ACTIVE interactive Windows session via a scheduled task. Direct
# client<->host TLS; SSH is used only to deploy and to invoke this script.
# Never via SSH Start-Process/WMI (Session 0 cannot drive the GUI), never
# a port forward.
#
# Invoked by the coordinator only, e.g.:
#   ssh acer-win powershell -NoProfile -ExecutionPolicy Bypass -File \
#       C:\rpa-remote\windows-remote-start.ps1 -RemoteDir C:\rpa-remote \
#       -ListenAddress 192.168.1.50 -Port 8399
#
# Expected deployed layout (fresh, owned by this deployment):
#   <RemoteDir>\bin\computer-host.exe
#   <RemoteDir>\tls\server.pem
#   <RemoteDir>\tls\server.key
#   <RemoteDir>\host.token
#
# Safety properties:
#   * Interactive session via WTSGetActiveConsoleSessionId; the scheduled
#     task principal is the OWNER of explorer.exe in EXACTLY that session
#     (LogonType Interactive, RunLevel Limited). No WMI/SSH Start-Process.
#   * The task action executes bin\computer-host.exe DIRECTLY with
#     --remote-listen/--tls-cert/--tls-key/--token-file/--log-file flags —
#     no cmd wrapper. Token/key paths appear as paths only; secret CONTENT
#     is never placed on a command line, in the instance record, or printed.
#   * -ListenAddress is parsed with [System.Net.IPAddress]::Parse (wildcard
#     and IPv6-compatible IPv4-mapped forms refused); an IPv6 literal is
#     BRACKETED so --remote-listen always receives a valid Rust SocketAddr.
#   * SECRET ACLs ARE SET HERE (files are freshly deployed): host.token and
#     tls\server.key are re-ACL'd to allow ONLY the console-user SID,
#     SYSTEM, and Administrators (inheritance disabled, all other ACEs
#     removed). Nothing outside <RemoteDir>\{host.token,tls\server.key}
#     is ever touched.
#   * IDENTITY RECORD: a PROVISIONAL record (status 'provisional', pid 0) is
#     written BEFORE the task is started, so a failed start is still owned
#     and stoppable by windows-remote-stop.ps1. On failure the record is
#     updated to status 'unhealthy' with a reason BEFORE the error exit;
#     on success it becomes status 'healthy' with the verified PID, session
#     id, exe path, process CreationDate (UTC ticks), and the full task
#     action/principal. windows-remote-stop.ps1 verifies ALL identity
#     fields before any mutation and stops ONLY the owned task/process.
#   * REFUSE-OVERWRITE (fail-closed): if the instance record exists AT
#     ALL (any status, even unparseable) OR the task name collides, this
#     script exits 1 and changes nothing — start NEVER removes or rewrites
#     a pre-existing record. Only windows-remote-stop.ps1 verifies
#     ownership and removes it. Run windows-remote-stop.ps1 first for a
#     clean re-start.
#   * POST-START VERIFY: the exact owned process (exe path, console
#     session, created at/after task start) must exist AND a TCP LISTEN on
#     <ListenAddress>:<Port> owned by that PID must appear within the
#     bounded wait; otherwise the record is marked 'unhealthy' and the
#     script exits 1 WITHOUT killing anything (no unrelated kill —
#     windows-remote-stop.ps1 performs the verified cleanup).
#   * No firewall commands, no GUI automation, no RPAD/Executor dependency.
param(
    [string]$RemoteDir = 'C:\rpa-remote',
    [string]$ListenAddress,
    [int]$Port = 8399,
    [string]$TaskName = ''
)

$ErrorActionPreference = 'Stop'

# --- 0. Validate parameters -------------------------------------------------
# These strings become scheduled-task action arguments verbatim. Reject
# double-quotes and control characters outright (we never re-quote argv;
# forbidding the metacharacters is the correct handling), and reject
# whitespace in the listen address and task name.
if ([string]::IsNullOrWhiteSpace($ListenAddress)) {
    Write-Error "-ListenAddress is required. Do NOT bind 0.0.0.0; pass the machine's specific LAN address (ipconfig)."
    exit 1
}
if ($Port -lt 1 -or $Port -gt 65535) { Write-Error "-Port out of range: $Port"; exit 1 }
foreach ($v in @($RemoteDir, $ListenAddress, $TaskName)) {
    if ($v -match '[\x00-\x1f"]') {
        Write-Error "double-quote or control characters are not allowed in parameters (argv is never re-quoted)."
        exit 1
    }
}
foreach ($v in @($ListenAddress, $TaskName)) {
    if ($v -match '\s') { Write-Error "whitespace is not allowed in parameters."; exit 1 }
}

# Real IP parsing — no character regexes. Specific interface address only.
try {
    $ip = [System.Net.IPAddress]::Parse($ListenAddress)
} catch {
    Write-Error "-ListenAddress must be an IP literal parseable by System.Net.IPAddress."
    exit 1
}
if ($ip -eq [System.Net.IPAddress]::Any -or $ip -eq [System.Net.IPAddress]::IPv6Any) {
    Write-Error "-ListenAddress must be a specific interface address, not a wildcard."
    exit 1
}
if ($ip.IsIPv4MappedToIPv6) {
    Write-Error "-ListenAddress must not be an IPv4-mapped IPv6 address; pass the plain IPv4 or IPv6 literal."
    exit 1
}
$listenArg = $ListenAddress
if ($ip.AddressFamily -eq [System.Net.Sockets.AddressFamily]::InterNetworkV6) {
    # Rust SocketAddr requires brackets around IPv6 literals.
    $listenArg = "[$ListenAddress]"
}

# Omitted -TaskName gets a GUID-unique name: never shared per port, so two
# deployments on the same port can never alias each other's task.
if ([string]::IsNullOrWhiteSpace($TaskName)) {
    $TaskName = "RpaComputerRemoteHost-$([Guid]::NewGuid().ToString('N'))"
}

$exe       = Join-Path $RemoteDir 'bin\computer-host.exe'
$serverPem = Join-Path $RemoteDir 'tls\server.pem'
$serverKey = Join-Path $RemoteDir 'tls\server.key'
$tokenFile = Join-Path $RemoteDir 'host.token'
$logFile   = Join-Path $RemoteDir 'host.log'
$record    = Join-Path $RemoteDir 'remote-host-instance.json'

foreach ($f in @($exe, $serverPem, $serverKey, $tokenFile)) {
    if (-not (Test-Path -LiteralPath $f -PathType Leaf)) {
        Write-Error "missing deployed file: $f"
        exit 1
    }
    $item = Get-Item -LiteralPath $f -Force
    if ($item.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
        Write-Error "deployed file must not be a reparse point/symlink: $f"
        exit 1
    }
}
$exe = (Resolve-Path -LiteralPath $exe).Path

# --- 0b. Refuse to overwrite an existing deployment (fail-closed) -------
# An omitted -TaskName becomes a NEW GUID above, so Get-ScheduledTask here
# can NEVER find the prior deployment's task — only the record can. If the
# record exists AT ALL (healthy, unhealthy, provisional, or even
# unparseable), refuse: deleting a 'terminal-looking' record could orphan
# (or, after PID reuse, mis-attribute) the prior task/process and lose
# ownership. Only windows-remote-stop.ps1 verifies identity and removes
# the record; start NEVER mutates it. An explicitly colliding -TaskName is
# refused the same way.
$existingTask = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if (Test-Path -LiteralPath $record -PathType Leaf) {
    Write-Error (("instance record already exists at {0}. Refusing to start over an existing deployment. " +
        "Run windows-remote-stop.ps1 first (it verifies ownership and removes the record), then retry.") -f $record)
    exit 1
}
if ($null -ne $existingTask) {
    Write-Error (("scheduled task '{0}' already exists. Refusing to overwrite. " +
        "Run windows-remote-stop.ps1 first, or pass a different -TaskName.") -f $TaskName)
    exit 1
}

# --- 1. Active console session + its interactive user --------------------
Add-Type -Namespace Wts -Name Api -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("kernel32.dll")]
public static extern uint WTSGetActiveConsoleSessionId();
'@
$consoleSession = [Wts.Api]::WTSGetActiveConsoleSessionId()
if ($consoleSession -eq 0xFFFFFFFF) {
    Write-Error "No active console session; cannot start a GUI-capable host."
    exit 1
}
$explorers = Get-Process -Name explorer -ErrorAction SilentlyContinue |
    Where-Object { $_.SessionId -eq [int]$consoleSession }
if (-not $explorers) {
    Write-Error "No explorer.exe in active console session $consoleSession; cannot identify the interactive user."
    exit 1
}
$explorer = $explorers | Select-Object -First 1
$owner = (Get-CimInstance Win32_Process -Filter "ProcessId=$($explorer.Id)" |
    Invoke-CimMethod -MethodName GetOwner)
if ($owner.ReturnValue -ne 0) {
    Write-Error "Could not resolve owner of explorer pid=$($explorer.Id)"
    exit 1
}
$interactiveUser = "$($owner.Domain)\$($owner.User)"
try {
    $interactiveSid = ([System.Security.Principal.NTAccount]$interactiveUser).
        Translate([System.Security.Principal.SecurityIdentifier]).Value
} catch {
    Write-Error "Could not translate '$interactiveUser' to a SID."
    exit 1
}

# --- 2. SET secret ACLs (freshly deployed files; we own them) ------------
# Allow ONLY: console-user SID, SYSTEM (S-1-5-18), Administrators (S-1-5-32-544).
$allowedSids = @(
    $interactiveSid,
    'S-1-5-18',
    'S-1-5-32-544'
)
foreach ($secret in @($tokenFile, $serverKey)) {
    $acl = New-Object System.Security.AccessControl.FileSecurity
    $acl.SetAccessRuleProtection($true, $false)  # disable inheritance, drop inherited ACEs
    foreach ($sid in $allowedSids) {
        $rule = New-Object System.Security.AccessControl.FileSystemAccessRule(
            ([System.Security.Principal.SecurityIdentifier]$sid),
            'FullControl', 'Allow')
        $acl.AddAccessRule($rule)
    }
    Set-Acl -LiteralPath $secret -AclObject $acl
    # Verify what we just set: every allow ACE must resolve to an allowed SID.
    $check = Get-Acl -LiteralPath $secret
    foreach ($ace in $check.Access) {
        if ($ace.AccessControlType -ne 'Allow') { continue }
        try {
            $sid = $ace.IdentityReference.Translate([System.Security.Principal.SecurityIdentifier]).Value
        } catch {
            Write-Error "ACL entry on $secret cannot be resolved to a SID after Set-Acl; refusing to start."
            exit 1
        }
        if ($allowedSids -notcontains $sid) {
            Write-Error "ACL on $secret still grants access to unexpected SID $sid; refusing to start. (Secret content is never printed.)"
            exit 1
        }
    }
}

# --- 3. Register the task: DIRECT exe action, no cmd wrapper -------------
# RemoteDir may contain spaces, so every PATH argument must be quoted in
# the scheduled-task argument string (double-quote/control characters are
# already rejected above, so plain wrapping is injection-safe). The exact
# same string is recorded as action_arguments and verified by stop.
$argList = @(
    '--remote-listen', "${listenArg}:${Port}",
    '--tls-cert', ('"{0}"' -f $serverPem),
    '--tls-key', ('"{0}"' -f $serverKey),
    '--token-file', ('"{0}"' -f $tokenFile),
    '--log-file', ('"{0}"' -f $logFile)
) -join ' '
$action = New-ScheduledTaskAction -Execute $exe -Argument $argList
$principal = New-ScheduledTaskPrincipal -UserId $interactiveUser -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
    -ExecutionTimeLimit ([TimeSpan]::Zero)
Register-ScheduledTask -TaskName $TaskName -Action $action -Principal $principal -Settings $settings | Out-Null

# --- 3b. provisional_record: write the instance record BEFORE starting --
# If the start fails at any later step, this record lets
# windows-remote-stop.ps1 verify and clean up the owned task. pid 0 +
# status 'provisional' means "no verified process yet".
$recordObj = [ordered]@{
    task_name            = $TaskName
    pid                  = 0
    session_id           = [int]$consoleSession
    exe                  = $exe
    creation_date_utc    = ''
    creation_ticks_utc   = 0
    listen_address       = $ListenAddress
    port                 = $Port
    log_file             = $logFile
    action_execute       = $exe
    action_arguments     = $argList
    principal_user       = $interactiveUser
    principal_sid        = $interactiveSid
    principal_logon_type = 'Interactive'
    principal_run_level  = 'Limited'
    listen_verified      = $false
    started_utc          = (Get-Date).ToUniversalTime().ToString('o')
    provisional_record   = $true
    status               = 'provisional'
    unhealthy_reason     = ''
}
$recordObj | ConvertTo-Json | Set-Content -LiteralPath $record -Encoding UTF8

$startMark = (Get-Date).ToUniversalTime()
Start-ScheduledTask -TaskName $TaskName

# --- 4. Verify the EXACT owned process + its TCP listener (bounded) ------
$proc = $null
$deadline = (Get-Date).AddSeconds(20)
while ((Get-Date) -lt $deadline) {
    $candidates = Get-CimInstance Win32_Process -Filter "Name='computer-host.exe'" -ErrorAction SilentlyContinue |
        Where-Object {
            # Strict identity predicate: exact exe, exact console session,
            # created at/after task start. try/catch as a STATEMENT (a
            # parenthesized (try ...) is parsed as a command invocation and
            # fails at runtime); an unreadable CreationDate FAILS CLOSED.
            try {
                ($_.ExecutablePath -eq $exe) -and
                ($_.SessionId -eq [int]$consoleSession) -and
                ($_.CreationDate.ToUniversalTime() -ge $startMark.AddSeconds(-2))
            } catch {
                $false
            }
        }
    if ($candidates) { $proc = $candidates | Select-Object -First 1; break }
    Start-Sleep -Milliseconds 500
}
$unhealthyReason = ''
if (-not $proc) {
    # process_not_found: record the failure reason BEFORE the unhealthy
    # update and exit. NOTHING is killed here — windows-remote-stop.ps1
    # performs the verified cleanup of the owned task.
    $unhealthyReason = 'process_not_found'
} else {
    $recordObj.pid = $proc.ProcessId
    $recordObj.creation_date_utc = $proc.CreationDate.ToUniversalTime().ToString('o')
    $recordObj.creation_ticks_utc = $proc.CreationDate.ToUniversalTime().Ticks

    $listenFound = $false
    $deadline = (Get-Date).AddSeconds(20)
    while ((Get-Date) -lt $deadline -and -not $listenFound) {
        $conns = Get-NetTCPConnection -State Listen -LocalPort $Port -ErrorAction SilentlyContinue |
            Where-Object { $_.LocalAddress -eq $ListenAddress -and $_.OwningProcess -eq $proc.ProcessId }
        if ($conns) { $listenFound = $true; break }
        Start-Sleep -Milliseconds 500
    }
    if ($listenFound) {
        $recordObj.listen_verified = $true
        $recordObj.status = 'healthy'
    } else {
        # no_tcp_listen: the port never listened. Record the failure reason,
        # then mark the record unhealthy BEFORE the failure exit. No
        # firewall policy is changed by this script; no kill is attempted.
        $unhealthyReason = 'no_tcp_listen'
    }
}
if ($unhealthyReason -ne '') {
    # mark_unhealthy: persist the failure so stop can clean up precisely.
    $recordObj.status = 'unhealthy'
    $recordObj.unhealthy_reason = $unhealthyReason
    $recordObj | ConvertTo-Json | Set-Content -LiteralPath $record -Encoding UTF8
    Write-Error (("start verification failed ({0}); instance record PRESERVED as 'unhealthy' at {1}. " +
        "Run windows-remote-stop.ps1 to clean up. See log: {2}") -f $unhealthyReason, $record, $logFile)
    exit 1
}

# --- 5. Final identity record (verified by windows-remote-stop.ps1) ------
$recordObj | ConvertTo-Json | Set-Content -LiteralPath $record -Encoding UTF8

Write-Output (("remote host listening on {0}:{1} (pid {2}, session {3}, task '{4}', user {5}); log: {6}") -f
    $ListenAddress, $Port, $proc.ProcessId, $consoleSession, $TaskName, $interactiveUser, $logFile)
exit 0
