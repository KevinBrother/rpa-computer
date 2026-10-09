[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$ConfigPath)
$ErrorActionPreference = 'Stop'
try {
    if ($env:OS -ne 'Windows_NT') { throw 'This runner requires Windows .NET Framework PowerShell 5.1' }
    $module = Join-Path $PSScriptRoot 'windows-test-runner/Runner.cs'
    Add-Type -Path $module -ReferencedAssemblies 'System.dll','System.Core.dll','System.Web.Extensions.dll'
    $status = [BoundedWindowsTests.Runner]::Run($ConfigPath, $PSCommandPath, $module)
    exit $status
} catch {
    # Bootstrap failures cannot safely reserve an unvalidated evidence destination.
    [Console]::Error.WriteLine($_.ToString())
    exit 70
}
