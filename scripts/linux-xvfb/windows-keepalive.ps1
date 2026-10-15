#Requires -Version 5.1
[CmdletBinding()]
param(
    [Parameter(Position = 0, Mandatory = $true)]
    [ValidateSet('start', 'status', 'stop', 'remove')]
    [string]$Command
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$TaskName = 'RpaLinuxXvfb-Keepalive-20261015'
$TaskPath = '\'
$Marker = 'rpa-linux-xvfb-keepalive:20261015:v1'
$Executable = Join-Path $env:SystemRoot 'System32\wsl.exe'
$Arguments = '-d Ubuntu --exec /bin/sleep infinity'
$Identity = [System.Security.Principal.WindowsIdentity]::GetCurrent()
$UserSid = $Identity.User.Value
if ($Identity.IsSystem -or $UserSid -eq 'S-1-5-18') {
    throw 'Run as the current Windows user, not SYSTEM. No credentials are accepted.'
}
if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
    throw "Missing WSL executable: $Executable"
}

function Get-OwnedTask {
    # Enumerate the exact task folder; do not suppress permission/service errors.
    $Matches = @(Get-ScheduledTask -TaskPath $TaskPath -ErrorAction Stop |
        Where-Object { $_.TaskName -eq $TaskName })
    if ($Matches.Count -eq 0) { return $null }
    if ($Matches.Count -ne 1) { throw 'Ambiguous task identity; no changes made.' }
    $Task = $Matches[0]
    if ($Task.Description -cne $Marker) {
        throw 'Existing task has a different ownership marker; no changes made.'
    }
    $Actions = @($Task.Actions)
    if ($Actions.Count -ne 1 -or
        $Actions[0].CimClass.CimClassName -ne 'MSFT_TaskExecAction') {
        throw 'Existing task has an unexpected action type/count; no changes made.'
    }
    if ($Actions[0].Execute -ine $Executable -or
        $Actions[0].Arguments -cne $Arguments -or
        -not [string]::IsNullOrEmpty($Actions[0].WorkingDirectory)) {
        throw 'Existing task action differs from the keepalive command; no changes made.'
    }
    $Principal = $Task.Principal
    $PrincipalSid = [string]$Principal.UserId
    if ($PrincipalSid -notmatch '^S-1-') {
        $Account = New-Object System.Security.Principal.NTAccount($PrincipalSid)
        $PrincipalSid = $Account.Translate([System.Security.Principal.SecurityIdentifier]).Value
    }
    if ($PrincipalSid -ne $UserSid -or
        [string]$Principal.LogonType -ne 'Interactive' -or
        [string]$Principal.RunLevel -ne 'Limited' -or
        -not [string]::IsNullOrEmpty($Principal.GroupId)) {
        throw 'Existing task principal is not this user / Interactive / Limited; no changes made.'
    }
    $Triggers = @($Task.Triggers | Where-Object { $null -ne $_ })
    if ($Triggers.Count -ne 0 -or
        [System.Xml.XmlConvert]::ToTimeSpan($Task.Settings.ExecutionTimeLimit) -ne [TimeSpan]::Zero -or
        $Task.Settings.DisallowStartIfOnBatteries -or $Task.Settings.StopIfGoingOnBatteries -or
        [string]$Task.Settings.MultipleInstances -ne 'IgnoreNew') {
        throw 'Existing task settings differ from manual-only keepalive; no changes made.'
    }
    return $Task
}

$Task = Get-OwnedTask
if ($Command -eq 'start') {
    if ($null -eq $Task) {
        $Action = New-ScheduledTaskAction -Execute $Executable -Argument $Arguments
        $Principal = New-ScheduledTaskPrincipal -UserId $UserSid -LogonType Interactive -RunLevel Limited
        $Settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) `
            -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -MultipleInstances IgnoreNew
        $Settings.ExecutionTimeLimit = 'PT0S'
        # No trigger, password, elevation or -Force: never replace an existing task.
        $Definition = New-ScheduledTask -Action $Action -Principal $Principal `
            -Settings $Settings -Description $Marker
        Register-ScheduledTask -TaskName $TaskName -TaskPath $TaskPath `
            -InputObject $Definition | Out-Null
        $Task = Get-OwnedTask
        if ($null -eq $Task) { throw 'Registered task could not be read back; not started.' }
    }
    if ([string]$Task.State -eq 'Running') {
        Write-Output 'Keepalive task is already running; no changes made.'
    } else {
        Start-ScheduledTask -InputObject $Task
        Write-Output 'Keepalive start requested; use status and independently verify WSL/Docker readiness.'
    }
} elseif ($null -eq $Task) {
    Write-Output "Task $TaskName does not exist; no changes made."
} elseif ($Command -eq 'status') {
    $Info = Get-ScheduledTaskInfo -InputObject $Task
    [pscustomobject]@{
        TaskName = $Task.TaskName
        State = [string]$Task.State
        UserId = $Task.Principal.UserId
        LastRunTime = $Info.LastRunTime
        LastTaskResult = $Info.LastTaskResult
    }
} elseif ($Command -eq 'stop') {
    Stop-ScheduledTask -InputObject $Task
    Write-Output 'Keepalive stop requested. No WSL shutdown or Docker command was issued.'
} elseif ($Command -eq 'remove') {
    Stop-ScheduledTask -InputObject $Task
    Unregister-ScheduledTask -InputObject $Task -Confirm:$false
    Write-Output 'Owned keepalive task removed. No WSL shutdown or Docker command was issued.'
}
