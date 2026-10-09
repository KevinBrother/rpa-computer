# start-windows.ps1 — launch the PRE-COMPILED ComputerUseAcceptance fixture in
# the ACTIVE interactive Windows session via a NEW one-off scheduled task.
# Never via SSH Start-Process/WMI: those land in Session 0 and cannot show GUI.
#
# INVOKED BY THE COORDINATOR ONLY. A live PID/session is NOT proof of GUI
# success — the coordinator must still verify screenshots against the oracle.
#
# Safety properties:
#   * Requires the EXACT source exe path and an ALREADY EXISTING coordinator
#     evidence directory; verifies the exe SHA-256 before staging.
#   * Creates a NEW GUID-unique run dir + task name per invocation; never
#     deletes or overwrites existing dirs/tasks (refuses on collision).
#   * Copies ONLY the exe (never source) into the run dir; the oracle
#     (answer-bearing evidence JSONL) lives inside the controlled run dir.
#   * Console session via WTSGetActiveConsoleSessionId; the task principal is
#     the owner of explorer.exe in EXACTLY that session (not the SSH user),
#     LogonType Interactive, RunLevel Limited. No privilege/security changes.
#   * The task action directly executes the staged fixture exe with
#     --evidence-file <oracle>; no shells, no arbitrary commands.
#   * Uses -Argument (the parameter actually installed on the target host;
#     (Get-Command New-ScheduledTaskAction).Parameters.Keys lists Argument,
#     NOT Arguments) and -Principal (typed ScheduledTaskPrincipal object)
#     on Register-ScheduledTask.
#   * Verifies the EXACT started process (exe path + session + creation time)
#     and records full task/process identity for stop-windows.ps1.
#   * ExecutionTimeLimit (30 min) bounds leftovers.
param(
    [string]$ExePath = 'C:\Temp\accfix\ComputerUseAcceptance.exe',
    [Parameter(Mandatory = $true)][string]$EvidenceDir,
    # ValidateSet-guarded: only a fixed suite enum is ever passed to the exe
    # (as `--suite <name>`), never an arbitrary command.
    [ValidateSet('legacy', 'baseline', 'punctuation', 'emoji', 'known-input', 'multiclick', 'drag', 'scroll', 'pointer', 'keyboard', 'focus')]
    [string]$Suite = 'legacy',
    [string]$ExpectedHash = '2A80F568144C68B7370B8DE076F92ECB846F8BEBAF2361AF642AABE9B4ED1F32'
)
$ErrorActionPreference = 'Stop'

# --- 1. Validate inputs: exact source exe + existing evidence dir ---------
if (-not (Test-Path -LiteralPath $ExePath -PathType Leaf)) {
    Write-Error "missing fixture exe: $ExePath"; exit 1
}
if (-not (Test-Path -LiteralPath $EvidenceDir -PathType Container)) {
    Write-Error "evidence dir must already exist (coordinator-owned): $EvidenceDir"; exit 1
}
if ($ExpectedHash) {
    $hash = (Get-FileHash -LiteralPath $ExePath -Algorithm SHA256).Hash
    if ($hash -ne $ExpectedHash) {
        Write-Error "fixture exe SHA-256 mismatch: got $hash expected $ExpectedHash; refusing to stage"
        exit 1
    }
}

# --- 2. New GUID-unique run dir + task name; never overwrite --------------
$runId    = [guid]::NewGuid().ToString('N')
$runDir   = Join-Path $EvidenceDir "run-$runId"
$taskName = "AccFixture-$runId"
if (Test-Path -LiteralPath $runDir) { Write-Error "run dir already exists: $runDir"; exit 1 }
if (Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue) {
    Write-Error "scheduled task already exists: $taskName"; exit 1
}
New-Item -ItemType Directory -Path $runDir | Out-Null

# Stage ONLY the exe (never the source tree) into the controlled run dir.
$stagedExe = Join-Path $runDir 'ComputerUseAcceptance.exe'
Copy-Item -LiteralPath $ExePath -Destination $stagedExe
$oracle = Join-Path $runDir 'oracle-evidence.jsonl' # coordinator-only, contains answers
$record = Join-Path $runDir 'run-record.json'

# --- 3. Active console session; explorer owner there = task principal -----
Add-Type -Namespace Wts -Name Api -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("kernel32.dll")]
public static extern uint WTSGetActiveConsoleSessionId();
'@
$consoleSession = [Wts.Api]::WTSGetActiveConsoleSessionId()
if ($consoleSession -eq 0xFFFFFFFF -or $consoleSession -eq 0) {
    Write-Error "no active interactive console session (id=$consoleSession); log in on the console first"
    exit 1
}
$explorer = Get-Process -Name explorer -ErrorAction SilentlyContinue |
    Where-Object { $_.SessionId -eq $consoleSession } | Select-Object -First 1
if (-not $explorer) {
    Write-Error "no explorer.exe in console session $consoleSession; cannot identify the interactive user"
    exit 1
}
$owner = Get-CimInstance Win32_Process -Filter "ProcessId=$($explorer.Id)" |
    Invoke-CimMethod -MethodName GetOwner
if (-not $owner -or -not $owner.User) {
    Write-Error "could not resolve owner of explorer pid=$($explorer.Id)"; exit 1
}
$interactiveUser = if ($owner.Domain -and $owner.Domain -ne $env:COMPUTERNAME) {
    "$($owner.Domain)\$($owner.User)"
} else { $owner.User }

# --- 4. One-off task: directly executes the staged fixture exe ------------
# Only the fixed suite enum (ValidateSet) is forwarded — no arbitrary args.
$taskArgs = "--evidence-file `"$oracle`""
if ($Suite -ne 'legacy') { $taskArgs += " --suite $Suite" }
$action     = New-ScheduledTaskAction -Execute $stagedExe -Argument $taskArgs
$principal  = New-ScheduledTaskPrincipal -UserId $interactiveUser -LogonType Interactive -RunLevel Limited
$settings   = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
    -ExecutionTimeLimit (New-TimeSpan -Minutes 30)
# Register ONLY after the staged exe and oracle paths exist; the principal is
# a typed object (not a display-name string) so the task is owned by exactly
# the explorer-owner identity resolved above.
Register-ScheduledTask -TaskName $taskName -Action $action -Principal $principal -Settings $settings | Out-Null
$startMark = Get-Date
Start-ScheduledTask -TaskName $taskName

# --- 5. Verify the EXACT started process (path + session + creation) ------
$proc = $null
foreach ($attempt in 1..10) {
    Start-Sleep -Seconds 1
    $proc = Get-CimInstance Win32_Process -Filter "Name='ComputerUseAcceptance.exe'" -ErrorAction SilentlyContinue |
        Where-Object {
            $_.ExecutablePath -eq $stagedExe -and
            $_.SessionId -eq $consoleSession -and
            $_.CreationDate -ge $startMark.AddSeconds(-2)
        } | Select-Object -First 1
    if ($proc) { break }
}
if (-not $proc) {
    Write-Error "fixture did not start in session $consoleSession; check task last-run result. Run dir left intact: $runDir"
    exit 1
}

# Record full identity; stop-windows.ps1 re-verifies all of it before acting.
$rec = [ordered]@{
    run_id             = $runId
    task_name          = $taskName
    action_execute     = $stagedExe
    action_arguments   = $taskArgs
    suite              = $Suite
    principal_user     = $interactiveUser
    pid                = [int]$proc.ProcessId
    exe                = $stagedExe
    session_id         = [int]$consoleSession
    creation_date_utc  = $proc.CreationDate.ToUniversalTime().ToString('o')
    oracle_path        = $oracle
    started_utc        = (Get-Date).ToUniversalTime().ToString('o')
}
$rec | ConvertTo-Json | Set-Content -Path $record -Encoding UTF8

Write-Output "OK: fixture pid=$($proc.ProcessId) session=$consoleSession user=$interactiveUser"
Write-Output "oracle (coordinator-only, contains answers; never shared with the agent): $oracle"
Write-Output "record: $record"
Write-Output "NOTE: a live PID is NOT proof of GUI success; coordinator must verify screenshots + oracle."
