# windows-start-host.ps1 — start computer-host in the INTERACTIVE Windows
# session via a scheduled task, never via SSH Start-Process/WMI (those land
# in Session 0 and cannot drive the GUI).
#
# INVOKED BY THE COORDINATOR ONLY (QA never starts GUI tests):
#   ssh acer-win powershell -NoProfile -ExecutionPolicy Bypass \
#       -File C:\rpa-computer-test\windows-start-host.ps1 -Port 8399
#
# Safety properties (defect #15 + continuation-review QA items 3/4):
#   * The interactive session is selected via WTSGetActiveConsoleSessionId
#     (the ACTIVE console session), and explorer.exe must be running in
#     EXACTLY that session — we no longer assume the SSH environment user
#     is the console user; the scheduled task principal is the OWNER of the
#     console-session explorer process.
#   * COLLISION-SAFE TASK IDENTITY: if a task with $TaskName already exists
#     we re-register it ONLY after verifying it is ours (single action whose
#     Execute path is this deployment's run-host-logged.cmd AND principal
#     UserId matches the interactive user). An unrelated task with the same
#     name is NEVER deleted/replaced — the script refuses (exit 1).
#   * After starting the task we verify the EXACT process we started:
#     a computer-host process whose ExecutablePath is our deployed exe, in
#     the console session, created at/after task start. We never "match
#     every computer-host process".
#   * The started PID, exe path, and process CreationDate are recorded to
#     host-instance.json; windows-stop-host.ps1 verifies PID + exe +
#     CreationDate before stopping (PID-reuse safe).
#   * stdout/stderr of the host are redirected to persistent diagnostic
#     logs in the deployment dir (never deleted by this script).
#   * TOKEN ACL BY SID (not display name): the token file's allow ACEs are
#     resolved to SIDs and must be a subset of {interactive user SID,
#     SYSTEM, Administrators}. Display-name matching is locale-fragile and
#     is not used. Validated BEFORE the host is started; the token is never
#     placed on a command line or printed (failures print the file path
#     only, never the token content).
#   * No firewall changes, no GUI started by QA, no RPAD/Executor dependency.
param(
    [string]$RemoteDir = 'C:\rpa-computer-test',
    [int]$Port = 8399,
    [string]$TaskName = 'RpaComputerTestHost'
)

$ErrorActionPreference = 'Stop'

$exe       = Join-Path $RemoteDir 'bin\computer-host.exe'
$tokenFile = Join-Path $RemoteDir 'host.token'
$logDir    = $RemoteDir
$stdoutLog = Join-Path $logDir 'host.stdout.log'
$stderrLog = Join-Path $logDir 'host.stderr.log'
$record    = Join-Path $RemoteDir 'host-instance.json'
$wrapper   = Join-Path $RemoteDir 'run-host-logged.cmd'

foreach ($p in @($exe, $tokenFile)) {
    if (-not (Test-Path $p)) { Write-Error "missing $p"; exit 1 }
}

# --- 1. Select the ACTIVE interactive console session ---------------------
Add-Type -Namespace Wts -Name Api -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("kernel32.dll")]
public static extern uint WTSGetActiveConsoleSessionId();
'@
$consoleSession = [Wts.Api]::WTSGetActiveConsoleSessionId()
if ($consoleSession -eq 0xFFFFFFFF -or $consoleSession -eq 0) {
    Write-Error "No active interactive console session (id=$consoleSession). Log in on the console first."
    exit 1
}

# explorer.exe must be running in EXACTLY that session; its OWNER is the
# interactive user we must run as — not $env:USERNAME (that is the SSH/
# invoking account, which may differ).
$explorers = Get-Process -Name explorer -ErrorAction SilentlyContinue |
    Where-Object { $_.SessionId -eq $consoleSession }
if (-not $explorers) {
    Write-Error "No explorer.exe in active console session $consoleSession; cannot identify the interactive user."
    exit 1
}
$explorer = $explorers | Select-Object -First 1
$owner = (Get-CimInstance Win32_Process -Filter "ProcessId=$($explorer.Id)" |
    Invoke-CimMethod -MethodName GetOwner)
if (-not $owner -or -not $owner.User) {
    Write-Error "Could not resolve owner of explorer pid=$($explorer.Id)"
    exit 1
}
$interactiveUser = if ($owner.Domain -and $owner.Domain -ne $env:COMPUTERNAME) {
    "$($owner.Domain)\$($owner.User)"
} else {
    $owner.User
}
# Resolve the interactive user to a SID (locale-independent identity).
# NOTE: PowerShell `if` is a STATEMENT, not an expression — assign the
# account name first, then cast (a parenthesized if-block inside a cast
# fails at parse time on PS 5.1).
$accountName = if ($owner.Domain) { "$($owner.Domain)\$($owner.User)" } else { $owner.User }
try {
    $interactiveSid = ([System.Security.Principal.NTAccount]$accountName).
        Translate([System.Security.Principal.SecurityIdentifier]).Value
} catch {
    Write-Error "Could not resolve SID for interactive user '$interactiveUser': $($_.Exception.Message)"
    exit 1
}

# --- 2. Validate token file ACL BY SID ------------------------------------
# Allowed allow-ACE SIDs: the interactive user, SYSTEM (S-1-5-18), and the
# Administrators group (S-1-5-32-544). Everything else — regardless of
# display name or locale — fails closed.
$allowedSids = @($interactiveSid, 'S-1-5-18', 'S-1-5-32-544')
$acl = Get-Acl $tokenFile
foreach ($ace in $acl.Access) {
    if ($ace.AccessControlType -ne 'Allow') { continue }
    try {
        $sid = $ace.IdentityReference.Translate([System.Security.Principal.SecurityIdentifier]).Value
    } catch {
        Write-Error "token file ACL entry '$($ace.IdentityReference.Value)' cannot be resolved to a SID; refusing to start."
        exit 1
    }
    if ($allowedSids -notcontains $sid) {
        Write-Error "token file grants access to unexpected SID $sid ($($ace.IdentityReference.Value)); tighten ACL before starting (deploy script ACLs it to the console owner only). Token content is never printed."
        exit 1
    }
}

# --- 3. Register the scheduled task with persistent logs ------------------
# The wrapper redirects host stdout/stderr to log files in the deployment
# dir so post-mortem diagnostics survive SSH disconnects.
@"
@echo off
rem Generated by windows-start-host.ps1; keeps host diagnostics on disk.
"$exe" --listen 127.0.0.1:$Port --token-file "$tokenFile" 1>> "$stdoutLog" 2>> "$stderrLog"
"@ | Set-Content -Path $wrapper -Encoding ASCII

$action = New-ScheduledTaskAction -Execute $wrapper
$principal = New-ScheduledTaskPrincipal -UserId $interactiveUser -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
    -ExecutionTimeLimit (New-TimeSpan -Hours 2)

# COLLISION SAFETY: re-register only OUR OWN task. An existing task with
# the same name must have exactly one action executing THIS deployment's
# wrapper and the interactive user's principal; otherwise we refuse rather
# than delete someone else's task.
$existing = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if ($existing) {
    $existingActions = @($existing.Actions)
    $owned = ($existingActions.Count -eq 1) -and
             ($existingActions[0].Execute -eq $wrapper) -and
             ($existing.Principal.UserId -eq $interactiveUser)
    if (-not $owned) {
        Write-Error ("scheduled task '{0}' exists but is NOT owned by this tool " +
            "(actions=[{1}] principal='{2}'); refusing to delete/replace an unrelated task. " +
            "Pick a different -TaskName or remove it yourself.") -f `
            $TaskName, (($existingActions | ForEach-Object { $_.Execute }) -join ';'), $existing.Principal.UserId
        exit 1
    }
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
}
Register-ScheduledTask -TaskName $TaskName -Action $action -Principal $principal -Settings $settings -Force | Out-Null

$startMark = Get-Date
Start-ScheduledTask -TaskName $TaskName

# --- 4. Verify the EXACT started process ---------------------------------
$proc = $null
foreach ($attempt in 1..10) {
    Start-Sleep -Seconds 1
    $candidates = Get-CimInstance Win32_Process -Filter "Name='computer-host.exe'" -ErrorAction SilentlyContinue |
        Where-Object {
            $_.ExecutablePath -eq $exe -and
            $_.SessionId -eq $consoleSession -and
            $_.CommandLine -notmatch 'run-host-logged' # exclude the wrapper shell itself
        }
    if ($candidates) {
        # The process must have been created at/after our task start.
        $proc = $candidates | Where-Object {
            try { $_.CreationDate -ge $startMark.AddSeconds(-2) } catch { $true }
        } | Select-Object -First 1
        if ($proc) { break }
    }
}
if (-not $proc) {
    Write-Error ("computer-host did not start in session {0}; see {1} and scheduled task last run result." -f $consoleSession, $stderrLog)
    exit 1
}

# --- 4b. Prove the OWNED pid is actually LISTENING on 127.0.0.1:$Port -----
# A live process is not proof of service: confirm a loopback TCP listener
# on the requested port owned by exactly this PID before declaring OK. If
# the process is up but NOT listening, still record the instance identity
# (marked unhealthy) so the coordinator can SAFELY clean up the started
# resources — never lose track of a process/task this tool launched.
$listening = $false
foreach ($attempt in 1..10) {
    $conns = Get-NetTCPConnection -State Listen -LocalAddress '127.0.0.1' `
        -LocalPort $Port -ErrorAction SilentlyContinue
    if ($conns | Where-Object { $_.OwningProcess -eq $proc.ProcessId }) {
        $listening = $true; break
    }
    Start-Sleep -Seconds 1
}
if (-not $listening) {
    $unhealthy = [ordered]@{
        task_name            = $TaskName
        pid                  = [int]$proc.ProcessId
        exe                  = $exe
        creation_date_utc    = $proc.CreationDate.ToUniversalTime().ToString('o')
        session_id           = [int]$consoleSession
        interactive_user     = $interactiveUser
        interactive_user_sid = $interactiveSid
        port                 = $Port
        healthy              = $false
        note                 = "process up but NOT listening on 127.0.0.1:$Port (owned pid); recorded for safe coordinator cleanup"
        started_utc          = (Get-Date).ToUniversalTime().ToString('o')
    }
    $unhealthy | ConvertTo-Json | Set-Content -Path $record -Encoding UTF8
    Write-Error ("host pid={0} is up but NOT listening on 127.0.0.1:{1} (owned pid); " +
        "instance identity recorded (unhealthy) at {2} for safe cleanup. See {3}.") -f `
        $proc.ProcessId, $Port, $record, $stderrLog
    exit 1
}

# Record instance identity; the stop script ONLY stops what is recorded
# here, and re-verifies PID + exe path + creation date before acting.
$instance = [ordered]@{
    task_name            = $TaskName
    pid                  = [int]$proc.ProcessId
    exe                  = $exe
    creation_date_utc    = $proc.CreationDate.ToUniversalTime().ToString('o')
    session_id           = [int]$consoleSession
    interactive_user     = $interactiveUser
    interactive_user_sid = $interactiveSid
    port                 = $Port
    healthy              = $true
    started_utc          = (Get-Date).ToUniversalTime().ToString('o')
}
$instance | ConvertTo-Json | Set-Content -Path $record -Encoding UTF8

Write-Output ("OK: host pid={0} session={1} user={2} listening 127.0.0.1:{3}" -f `
    $proc.ProcessId, $consoleSession, $interactiveUser, $Port)
Write-Output ("diagnostics: {0} , {1} ; instance record: {2}" -f $stdoutLog, $stderrLog, $record)
