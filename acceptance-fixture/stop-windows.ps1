# stop-windows.ps1 — stop ONLY the fixture run recorded by start-windows.ps1.
#
# INVOKED BY THE COORDINATOR ONLY, with the run-record.json path produced by
# the start script. Safety properties:
#   * Re-verifies the scheduled task is OURS: exactly one action whose Execute
#     path and Arguments match the record, and the recorded principal user.
#   * Re-verifies the live process matches the record: PID + exact exe path +
#     session + creation time (PID-reuse safe).
#   * VERIFY-FIRST ordering: the script reads and verifies the task AND the
#     live process against the record BEFORE any mutation. Only when every
#     present identity matches does it stop/unregister the task and stop the
#     process; on ANY mismatch it refuses with nothing touched.
#   * On ANY mismatch it refuses: no pattern kill, no force overwrite, never
#     touches unrelated tasks/processes, never deletes the source exe, run dir
#     or any user files — the evidence dir is left intact for the coordinator.
param(
    [Parameter(Mandatory = $true)][string]$RecordPath
)
$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $RecordPath -PathType Leaf)) {
    Write-Error "record file not found: $RecordPath"; exit 1
}
$rec = Get-Content -LiteralPath $RecordPath -Raw | ConvertFrom-Json
foreach ($k in 'task_name', 'action_execute', 'action_arguments', 'principal_user',
               'pid', 'exe', 'session_id', 'creation_date_utc') {
    if (-not ($rec.PSObject.Properties.Name -contains $k)) {
        Write-Error "record is missing field '$k'; refusing to act"; exit 1
    }
}

# --- 1. READ and VERIFY everything FIRST: no mutation of any kind until
# both the task identity AND the live process identity match the record.
# A stale/reused process or an unrelated task must never cause an earlier
# task mutation (coordinator directive 2026-09-24).
$task = Get-ScheduledTask -TaskName $rec.task_name -ErrorAction SilentlyContinue
$taskVerified = $false
if ($task) {
    $actions = @($task.Actions)
    $taskVerified = ($actions.Count -eq 1) -and
                    ($actions[0].Execute -eq $rec.action_execute) -and
                    ($actions[0].Arguments -eq $rec.action_arguments) -and
                    ($task.Principal.UserId -eq $rec.principal_user)
    if (-not $taskVerified) {
        $msg = ("task '{0}' does NOT match the record (actions=[{1}] args=[{2}] principal='{3}'); " +
            "refusing to stop/unregister an unrelated task") -f `
            $rec.task_name, (($actions | ForEach-Object { $_.Execute }) -join ';'),
            (($actions | ForEach-Object { $_.Arguments }) -join ';'), $task.Principal.UserId
        Write-Error $msg
        exit 1
    }
}

$proc = Get-CimInstance Win32_Process -Filter "ProcessId=$($rec.pid)" -ErrorAction SilentlyContinue
$procVerified = $false
if ($proc) {
    $expectedCreation = [datetime]::Parse($rec.creation_date_utc).ToUniversalTime()
    $procVerified = ($proc.ExecutablePath -eq $rec.exe) -and
                    ($proc.SessionId -eq [int]$rec.session_id) -and
                    ($proc.CreationDate.ToUniversalTime() -eq $expectedCreation)
    if (-not $procVerified) {
        $msg = ("pid {0} no longer matches the record (path='{1}' session={2} created={3:o}); " +
            "refusing to stop a possibly-reused pid") -f `
            $rec.pid, $proc.ExecutablePath, $proc.SessionId, $proc.CreationDate.ToUniversalTime()
        Write-Error $msg
        exit 1
    }
}

# --- 2. All present identities verified. NOW mutate: stop/unregister ONLY
# our own task, then stop ONLY the recorded process.
if ($taskVerified) {
    Stop-ScheduledTask -TaskName $rec.task_name -ErrorAction SilentlyContinue
    Unregister-ScheduledTask -TaskName $rec.task_name -Confirm:$false
    Write-Output "task '$($rec.task_name)' stopped and unregistered (identity verified before mutation)"
} else {
    Write-Output "task '$($rec.task_name)' already gone; nothing to unregister"
}

if ($procVerified) {
    # Stopping the scheduled task may ALREADY have ended this exact process.
    # Re-check liveness first: an already-gone verified PID is a SAFE
    # success, not an error (and Stop-Process on a dead PID must not fail
    # the run under $ErrorActionPreference='Stop').
    $still = Get-CimInstance Win32_Process -Filter "ProcessId=$($rec.pid)" -ErrorAction SilentlyContinue
    if ($still) {
        Stop-Process -Id $rec.pid -Force -ErrorAction SilentlyContinue
        Write-Output "process pid=$($rec.pid) ($($rec.exe)) stopped (identity verified before stop)"
    } else {
        Write-Output "process pid=$($rec.pid) already ended with the task (identity verified)"
    }
} else {
    Write-Output "pid $($rec.pid) already exited"
}

Write-Output "NOTE: run dir, oracle evidence and record are LEFT INTACT for the coordinator: $(Split-Path -Parent $RecordPath)"
