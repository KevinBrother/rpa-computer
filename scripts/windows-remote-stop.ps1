# windows-remote-stop.ps1 — stop the TLS remote-listen computer-host started
# by windows-remote-start.ps1, and ONLY that host.
#
#   ssh acer-win powershell -NoProfile -ExecutionPolicy Bypass -File \
#       C:\rpa-remote\windows-remote-stop.ps1 -RemoteDir C:\rpa-remote
#
# Safety properties:
#   * Reads remote-host-instance.json written by the start script and
#     verifies EVERY identity field before ANY mutation:
#       - required record fields: task_name, pid, exe, creation_date_utc,
#         session_id, listen_address, action_execute, action_arguments,
#         principal_user, principal_sid, principal_run_level;
#       - the recorded PID is a live computer-host.exe whose ExecutablePath,
#         SessionId and CreationDate (EXACT UTC ticks — PID-reuse safe, no
#         fuzzy time window) match the record; a provisional record (pid 0,
#         status 'provisional') means no process was ever verified, so only
#         the task is cleaned up;
#       - the recorded task still exists, has EXACTLY ONE action whose
#         Execute is the recorded exe and whose Arguments equal the recorded
#         argument string, and whose principal UserId resolves (name or SID)
#         to EXACTLY the recorded principal_sid, with LogonType
#         (Interactive) and RunLevel matching the record.
#   * If ANY check fails, NOTHING is stopped/unregistered (exit 1): a
#     mismatched task or process belongs to someone else.
#   * Only then: Stop-ScheduledTask for the owned task, a bounded wait for
#     the owned PID to exit, the FULL identity REVALIDATION (exe, session,
#     exact creation ticks) of the still-live PID after the wait, and only
#     then Stop-Process -Force on THAT PID as a last resort;
#     Unregister-ScheduledTask for the owned task name, and removal of the
#     instance record file (nothing else is ever deleted).
#   * Secret files, logs, and the deployment directory are left untouched.
#   * No firewall commands.
param(
    [string]$RemoteDir = 'C:\rpa-remote'
)

$ErrorActionPreference = 'Stop'
$record = Join-Path $RemoteDir 'remote-host-instance.json'

if (-not (Test-Path -LiteralPath $record -PathType Leaf)) {
    Write-Output "no instance record at $record; nothing to stop."
    exit 0
}
try {
    $rec = Get-Content -LiteralPath $record -Raw | ConvertFrom-Json
} catch {
    Write-Error "instance record $record is unparseable; refusing to stop anything."
    exit 1
}
# ALL identity fields must be present before any mutation.
foreach ($field in @('task_name','pid','exe','creation_date_utc','session_id',
                     'listen_address','action_execute','action_arguments',
                     'principal_user','principal_sid','principal_run_level')) {
    if ($null -eq $rec.$field) {
        Write-Error "instance record is missing '$field'; refusing to stop anything."
        exit 1
    }
}
foreach ($field in @('task_name','exe','action_execute','action_arguments',
                     'principal_user','principal_sid','principal_run_level',
                     'listen_address')) {
    if ([string]::IsNullOrWhiteSpace([string]$rec.$field)) {
        Write-Error "instance record has empty '$field'; refusing to stop anything."
        exit 1
    }
}

# --- 1. Verify the recorded PROCESS identity -----------------------------
# A provisional record (pid 0) means the start script never verified a
# process; there is nothing to kill — the task cleanup below is enough.
$provisional = ([int]$rec.pid -eq 0)
$recCreationTicks = 0
if (-not $provisional) {
    if ([string]::IsNullOrWhiteSpace([string]$rec.creation_date_utc)) {
        Write-Error "instance record has a pid but no creation_date_utc; refusing to stop anything."
        exit 1
    }
    $recCreationTicks = [DateTime]::Parse($rec.creation_date_utc).ToUniversalTime().Ticks
}
$proc = $null
$procOk = $false
if (-not $provisional) {
    $proc = Get-CimInstance Win32_Process -Filter "ProcessId=$([int]$rec.pid)" -ErrorAction SilentlyContinue
    if ($null -eq $proc) {
        $procOk = $false  # already exited; fine — still verify+remove the task
    } else {
        # EXACT UTC tick equality: no fuzzy window, PID-reuse safe.
        $procOk = (
            $proc.Name -eq 'computer-host.exe' -and
            $proc.ExecutablePath -eq $rec.exe -and
            $proc.SessionId -eq [int]$rec.session_id -and
            $proc.CreationDate.ToUniversalTime().Ticks -eq $recCreationTicks
        )
        if (-not $procOk) {
            Write-Error (("pid {0} exists but does NOT match the recorded identity " +
                "(exe/session/exact creation ticks; PID may have been reused). Refusing to stop anything.") -f $rec.pid)
            exit 1
        }
    }
}

# --- 2. Verify the recorded TASK identity --------------------------------
# Resolve an account name or SID string to its canonical SID string.
# FAILS CLOSED: throws (caught by the caller -> refuse) when the identity
# cannot be resolved to a SecurityIdentifier. Never trust name text alone:
# ScheduledTask canonicalizes 'NODE1\Administrator' to 'Administrator', so
# literal name equality is unreliable; exact SID equivalence is the only
# authoritative identity comparison.
function Resolve-ToSidString([string]$Identity) {
    if ([string]::IsNullOrWhiteSpace($Identity)) { throw "empty identity" }
    if ($Identity -match '^S-1-') {
        return ([System.Security.Principal.SecurityIdentifier]$Identity).Value
    }
    return ([System.Security.Principal.NTAccount]$Identity).Translate(
        [System.Security.Principal.SecurityIdentifier]).Value
}

# The record itself must be internally consistent: the recorded
# principal_user must resolve to EXACTLY the recorded principal_sid.
try {
    $recUserSid = Resolve-ToSidString ([string]$rec.principal_user)
} catch {
    Write-Error "recorded principal_user '$($rec.principal_user)' cannot be resolved to a SID; refusing to stop anything."
    exit 1
}
if ($recUserSid -ne [string]$rec.principal_sid) {
    Write-Error (("recorded principal_user '{0}' resolves to SID '{1}' but the record claims principal_sid '{2}'; " +
        "inconsistent record, refusing to stop anything.") -f $rec.principal_user, $recUserSid, $rec.principal_sid)
    exit 1
}

$task = Get-ScheduledTask -TaskName $rec.task_name -ErrorAction SilentlyContinue
$taskOk = $false
if ($null -ne $task) {
    $actions = @($task.Actions)
    # Strict SID equivalence for the task principal: resolve whatever
    # UserId the task scheduler reports (canonicalized short name or SID)
    # to a SecurityIdentifier and compare EXACTLY to the recorded
    # principal_sid. Resolution failure -> refuse (fail closed).
    $taskPrincipalSid = $null
    try { $taskPrincipalSid = Resolve-ToSidString ([string]$task.Principal.UserId) } catch { $taskPrincipalSid = $null }
    $taskOk = (
        $actions.Count -eq 1 -and
        $actions[0].Execute -eq $rec.action_execute -and
        $actions[0].Arguments -eq $rec.action_arguments -and
        $null -ne $taskPrincipalSid -and
        $taskPrincipalSid -eq [string]$rec.principal_sid -and
        $task.Principal.LogonType -eq 'Interactive' -and
        $task.Principal.RunLevel -eq $rec.principal_run_level
    )
    if (-not $taskOk) {
        Write-Error (("task '{0}' exists but its action/principal do NOT match the record " +
            "(execute='{1}' arguments='{2}' principal='{3}' principal_sid='{4}'). It is not ours; refusing to touch it.") -f
            $rec.task_name, $actions[0].Execute, $actions[0].Arguments, $task.Principal.UserId, $taskPrincipalSid)
        exit 1
    }
}

# --- 3. Stop ONLY the verified-owned task/process ------------------------
if ($taskOk) {
    Stop-ScheduledTask -TaskName $rec.task_name -ErrorAction SilentlyContinue
}
if (-not $provisional -and $procOk -and $null -ne $proc) {
    $deadline = (Get-Date).AddSeconds(15)
    while ((Get-Date) -lt $deadline) {
        if (-not (Get-Process -Id ([int]$rec.pid) -ErrorAction SilentlyContinue)) { break }
        Start-Sleep -Milliseconds 500
    }
    if (Get-Process -Id ([int]$rec.pid) -ErrorAction SilentlyContinue) {
        # REVALIDATE the full identity after the wait: the PID may have
        # exited and been reused during the 15s window. Kill only if the
        # live process still matches exe + session + EXACT creation ticks.
        $live = Get-CimInstance Win32_Process -Filter "ProcessId=$([int]$rec.pid)" -ErrorAction SilentlyContinue
        $stillOwned = ($null -ne $live -and
            $live.Name -eq 'computer-host.exe' -and
            $live.ExecutablePath -eq $rec.exe -and
            $live.SessionId -eq [int]$rec.session_id -and
            $live.CreationDate.ToUniversalTime().Ticks -eq $recCreationTicks)
        if ($stillOwned) {
            # Last resort: kill the revalidated-owned PID only.
            Stop-Process -Id ([int]$rec.pid) -Force
        } else {
            Write-Error (("pid {0} survived the stop wait but no longer matches the recorded identity " +
                "(revalidate after wait failed; PID may have been reused). Refusing to force-kill; " +
                "the task is left registered for inspection.") -f $rec.pid)
            exit 1
        }
    }
}
if ($taskOk) {
    Unregister-ScheduledTask -TaskName $rec.task_name -Confirm:$false
}
Remove-Item -LiteralPath $record -Force

Write-Output (("stopped remote host: task '{0}', pid {1}; record removed. Logs and secrets left in place.") -f
    $rec.task_name, $rec.pid)
exit 0
