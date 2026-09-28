# windows-stop-host.ps1 — stop the test host and remove the scheduled task.
#
# SAFETY (defect #15 + continuation-review QA item 3): this script NEVER
# pattern-matches and kills every 'computer-host' process, and NEVER
# touches a scheduled task that is not verifiably ours. It stops ONLY:
#   1. the scheduled task recorded in host-instance.json (created by
#      windows-start-host.ps1) — and only after verifying the live task's
#      action path and principal still match the record; -TaskName may
#      ONLY narrow the record (a different override name is refused), and
#   2. the exact PID recorded there — after verifying the live process at
#      that PID still has the SAME executable path AND (when recorded) the
#      SAME creation date as the record (guards against PID reuse pointing
#      at an unrelated or newer host process).
# If no valid record exists, nothing is killed and the script says so.
# Invoked by the coordinator. Safe to run when nothing is running.
param(
    [string]$RemoteDir = 'C:\rpa-computer-test',
    [string]$TaskName = ''        # may only CONFIRM the recorded name, never override it
)

$ErrorActionPreference = 'Continue'

$record = Join-Path $RemoteDir 'host-instance.json'

if (-not (Test-Path $record)) {
    Write-Output "no instance record at $record; nothing owned by this tool to stop."
    Write-Output "(refusing to pattern-kill unrelated computer-host processes)"
    exit 0
}

# VERIFY-FIRST ordering (coordinator directive 2026-09-24): the script
# reads and verifies the task AND the live process against the record
# BEFORE any mutation. Only when every present identity matches does it
# stop/unregister the task, stop the process, and remove the record. On ANY
# mismatch it refuses with a NON-ZERO exit and leaves the record (and
# everything else) intact.
$instance = Get-Content $record -Raw | ConvertFrom-Json
$task = [string]$instance.task_name
if ($TaskName -and $TaskName -ne $task) {
    $msg = ("-TaskName '{0}' does not match the recorded owned task '{1}'; " +
        "refusing to stop/unregister a task this tool did not record.") -f $TaskName, $task
    Write-Error $msg
    exit 1
}
$pidToStop = [int]$instance.pid
$exeExpected = [string]$instance.exe
$userExpected = [string]$instance.interactive_user
$sessionExpected = $null
if ($instance.PSObject.Properties['session_id'] -and $null -ne $instance.session_id) {
    $sessionExpected = [int]$instance.session_id
}
# The creation date is REQUIRED for PID-reuse safety; a record without it
# cannot authorize stopping anything.
$createdExpected = $null
if ($instance.PSObject.Properties['creation_date_utc'] -and $instance.creation_date_utc) {
    try { $createdExpected = [datetime]::Parse([string]$instance.creation_date_utc).ToUniversalTime() } catch { $createdExpected = $null }
}

# --- 1. READ + VERIFY the task identity (no mutation yet) -----------------
$liveTask = Get-ScheduledTask -TaskName $task -ErrorAction SilentlyContinue
$taskVerified = $false
if ($liveTask) {
    $liveActions = @($liveTask.Actions)
    $taskVerified = ($liveActions.Count -eq 1) -and
                    ($liveActions[0].Execute -eq (Join-Path $RemoteDir 'run-host-logged.cmd')) -and
                    ($liveTask.Principal.UserId -eq $userExpected)
    if (-not $taskVerified) {
        Write-Error ("task '{0}' no longer matches the recorded owned task (actions=[{1}] principal='{2}'); " +
            "refusing to stop/unregister an unrelated task. Record left intact: {3}") -f `
            $task, (($liveActions | ForEach-Object { $_.Execute }) -join ';'), $liveTask.Principal.UserId, $record
        exit 1
    }
}

# --- 2. READ + VERIFY the process identity (no mutation yet) --------------
$proc = Get-CimInstance Win32_Process -Filter "ProcessId=$pidToStop" -ErrorAction SilentlyContinue
$procVerified = $false
if ($proc) {
    if (-not $createdExpected) {
        Write-Error ("record has no usable creation_date_utc; cannot prove pid {0} is ours " +
            "(PID-reuse unsafe). Refusing to stop. Record left intact: {1}") -f $pidToStop, $record
        exit 1
    }
    $procVerified = ($proc.ExecutablePath -eq $exeExpected) -and ($proc.Name -eq 'computer-host.exe')
    if ($procVerified -and $null -ne $sessionExpected) {
        $procVerified = ($proc.SessionId -eq $sessionExpected)
    }
    if ($procVerified) {
        try {
            $createdLive = $proc.CreationDate.ToUniversalTime()
            $skew = [math]::Abs(($createdLive - $createdExpected).TotalSeconds)
            if ($skew -gt 2) { $procVerified = $false }
        } catch { $procVerified = $false }
    }
    if (-not $procVerified) {
        Write-Error ("pid {0} no longer matches the record (path='{1}' name='{2}' creation mismatch); " +
            "refusing to stop a possibly-reused pid. Record left intact: {3}") -f `
            $pidToStop, $proc.ExecutablePath, $proc.Name, $record
        exit 1
    }
}

# --- 3. All present identities verified. NOW mutate. ----------------------
if ($taskVerified) {
    Stop-ScheduledTask -TaskName $task -ErrorAction SilentlyContinue
    Unregister-ScheduledTask -TaskName $task -Confirm:$false -ErrorAction SilentlyContinue
    Write-Output "stopped+unregistered owned scheduled task '$task' (identity verified before mutation)"
} else {
    Write-Output "recorded task '$task' already gone"
}

if ($procVerified) {
    # Stopping the scheduled task may ALREADY have ended this exact process.
    # Re-check liveness first: an already-gone verified PID is a SAFE
    # success, not an error.
    $still = Get-CimInstance Win32_Process -Filter "ProcessId=$pidToStop" -ErrorAction SilentlyContinue
    if ($still) {
        Write-Output ("stopping owned host pid={0} session={1} (identity verified before stop)" -f $proc.ProcessId, $proc.SessionId)
        Stop-Process -Id $proc.ProcessId -Force -ErrorAction SilentlyContinue
    } else {
        Write-Output "host pid=$pidToStop already ended with the task (identity verified)"
    }
} else {
    Write-Output "recorded pid $pidToStop already exited"
}

# Only remove the record once every owned artifact is actually gone.
$taskLeft = Get-ScheduledTask -TaskName $task -ErrorAction SilentlyContinue
$procLeft = Get-CimInstance Win32_Process -Filter "ProcessId=$pidToStop" -ErrorAction SilentlyContinue
if ($taskLeft -or $procLeft) {
    Write-Error ("cleanup incomplete (task or pid {0} still present); record PRESERVED for retry: {1}" -f $pidToStop, $record)
    exit 1
}
Remove-Item $record -Force -ErrorAction SilentlyContinue
Write-Output 'OK: owned host stopped and task removed (no unrelated processes touched)'
