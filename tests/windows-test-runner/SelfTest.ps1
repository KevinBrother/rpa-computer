# Windows-only SOURCE. Run only by the authorized CC test owner.
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Windows required' }
# The per-invocation watchdog is independent of the runner being tested.
Add-Type -TypeDefinition @'
using System;
using System.Diagnostics;
using System.IO;
using System.Threading.Tasks;
public static class RunnerSelfTestWatchdog {
    public static int Invoke(string runner, string config, string output, string error) {
        string body = "& '" + runner.Replace("'", "''") + "' -ConfigPath '" + config.Replace("'", "''") + "'";
        string command = Convert.ToBase64String(System.Text.Encoding.Unicode.GetBytes(body));
        using(var p = new Process()) {
            p.StartInfo = new ProcessStartInfo(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.System), "WindowsPowerShell\\v1.0\\powershell.exe"), "-NoProfile -EncodedCommand " + command) {UseShellExecute=false,RedirectStandardOutput=true,RedirectStandardError=true,CreateNoWindow=true};
            p.Start(); IntPtr handle=p.Handle;
            // Harness output is bounded by small config/summary records, not child stress output.
            var stdout=p.StandardOutput.ReadToEndAsync(); var stderr=p.StandardError.ReadToEndAsync();
            if(!p.WaitForExit(30000)) {p.Kill(); p.WaitForExit(2000); throw new Exception("Independent 30s watchdog expired; only owned harness root kill requested, descendants unknown");}
            if(!Task.WaitAll(new Task[]{stdout,stderr},2000)) throw new Exception("Harness drain watchdog expired");
            File.WriteAllText(output,stdout.Result); File.WriteAllText(error,stderr.Result);
            return p.ExitCode;
        }
    }
}
'@
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$runner = Join-Path $root 'scripts/windows-test-runner.ps1'
$temp = Join-Path $env:TEMP ('windows-runner-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($temp) | Out-Null
$csc = Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
if (!(Test-Path $csc)) { $csc = Join-Path $env:WINDIR 'Microsoft.NET/Framework/v4.0.30319/csc.exe' }
$child = Join-Path $temp 'child fixture.exe'
& $csc /nologo /target:exe "/out:$child" (Join-Path $PSScriptRoot 'Child.cs')
if ($LASTEXITCODE -ne 0) { throw 'Fixture compilation failed' }
$script:count = 0
function Assert($ok, $message) { if (!$ok) { throw $message } }
function Case($name, $arguments, $timeout = 15, $change = $null, $raw = $null) {
    $dir = Join-Path $temp $name
    $cfg = [ordered]@{executablePath=$child; arguments=@($arguments); workingDirectory=$temp; evidenceDirectory=$dir; timeoutSeconds=$timeout}
    if ($change) { & $change $cfg }
    $path = Join-Path $temp ($name + '.json')
    if ($null -eq $raw) { $raw = $cfg | ConvertTo-Json -Depth 5 -Compress }
    [IO.File]::WriteAllText($path, $raw, [Text.UTF8Encoding]::new($false))
    $callOut = Join-Path $temp ($name + ".harness-stdout.log")
    $callErr = Join-Path $temp ($name + ".harness-stderr.log")
    $status = [RunnerSelfTestWatchdog]::Invoke($runner, $path, $callOut, $callErr)
    $summary = if (Test-Path (Join-Path $dir 'summary.json')) { Get-Content (Join-Path $dir 'summary.json') -Raw | ConvertFrom-Json } else { Get-Content $callOut -Raw | ConvertFrom-Json }
    $script:count++
    return @{status=$status; summary=$summary; dir=$dir}
}
$r = Case 'zero' @('exit','0'); Assert ($r.status -eq 0 -and $r.summary.native_exit_code -ceq 0) 'native zero'
Assert ($r.summary.root_pid -gt 0 -and $r.summary.root_session_id -ge 0 -and $r.summary.root_start_utc -and $r.summary.root_executable -eq $child -and $r.summary.runner_source_sha256 -and $r.summary.runner_module_sha256 -and $r.summary.executable_sha256) 'source identity'
$r = Case 'nonzero' @('exit','23'); Assert ($r.status -ne 0 -and $r.summary.native_exit_code -eq 23 -and $r.summary.outcome -eq 'native_failure') 'native failure'
Assert ((Get-Content (Join-Path $r.dir 'stdout.log') -Raw).Contains('retained-output')) 'failure stdout retained'
Assert ((Get-Content (Join-Path $r.dir 'stderr.log') -Raw).Contains('retained-error')) 'failure stderr retained'
$argv = @('', 'a b', 'a"b', 'tail\', 'space tail\', '\\"quoted', "tab`targ", '中文')
$r = Case 'argv' (@('argv') + $argv); Assert ($r.status -eq 0) 'argv execution'
$actual = @(Get-Content (Join-Path $r.dir 'stdout.log'))
Assert ($actual.Count -eq $argv.Count) 'argv count'
for ($i=0;$i -lt $argv.Count;$i++) { Assert ($actual[$i] -ceq [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($argv[$i]))) "argv $i" }
$r = Case 'stress' @('stress'); Assert ($r.status -eq 0 -and $r.summary.stdout_truncated -and $r.summary.stderr_truncated) 'stress drained and capped'
Assert ((Get-Item (Join-Path $r.dir 'stdout.log')).Length -eq 4194304 -and (Get-Item (Join-Path $r.dir 'stderr.log')).Length -eq 4194304) 'caps'
$r = Case 'timeout' @('timeout') 1; Assert ($r.status -ne 0 -and $r.summary.outcome -eq 'timeout' -and $r.summary.timed_out) 'timeout'
Assert (Test-Path (Join-Path $r.dir 'stdout.log')) 'timeout retained'
$r = Case 'missing' @('exit','0') 15 {param($c) $c.executablePath = Join-Path $temp 'missing.exe'}; Assert ($r.status -ne 0 -and !$r.summary.root_pid) 'missing fails before launch'
$existing = Join-Path $temp 'existing'; [IO.Directory]::CreateDirectory($existing) | Out-Null
[IO.File]::WriteAllText((Join-Path $existing 'sentinel'), 'unchanged')
$r = Case 'existing' @('exit','0'); Assert ($r.status -ne 0 -and !(Test-Path (Join-Path $existing 'summary.json')) -and [IO.File]::ReadAllText((Join-Path $existing 'sentinel')) -ceq 'unchanged') 'no overwrite'
$r = Case 'nullarg' @('exit','0') 15 {param($c) $c.arguments = @('exit',$null)}; Assert ($r.status -ne 0 -and !$r.summary.root_pid) 'null arg rejected'
$r = Case 'unknown' @('exit','0') 15 {param($c) $c['extra'] = 'no'}; Assert ($r.status -ne 0 -and !$r.summary.root_pid) 'unknown rejected'
$r = Case 'bound' @('exit','0') 0; Assert ($r.status -ne 0 -and !$r.summary.root_pid) 'bound rejected'
$r = Case 'duplicate' @() 15 $null '{"timeoutSeconds":1,"timeoutSeconds":2}'; Assert ($r.status -ne 0 -and !$r.summary.root_pid) 'duplicate rejected'
$r = Case 'nonstring' @('exit',3); Assert ($r.status -ne 0 -and !$r.summary.root_pid) 'nonstring rejected'
$badExe = Join-Path $temp 'not-executable.exe'; [IO.File]::WriteAllText($badExe, 'not a PE image')
$r = Case 'launch' @('exit','0') 15 {param($c) $c.executablePath=$badExe}; Assert ($r.status -ne 0 -and $r.summary.outcome -eq 'launch_failure' -and !$r.summary.root_pid -and (Test-Path (Join-Path $r.dir 'summary.json'))) 'launch failure retained'
$r = Case 'cwd' @('exit','0') 15 {param($c) $c.workingDirectory=Join-Path $temp 'absent-cwd'}; Assert ($r.status -ne 0 -and !$r.summary.root_pid -and (Test-Path (Join-Path $r.dir 'config.json'))) 'bad cwd retained'
$r = Case 'nullarray' @('exit','0') 15 {param($c) $c.arguments=$null}; Assert ($r.status -ne 0 -and !$r.summary.root_pid) 'null array rejected'
$r = Case 'malformed' @() 15 $null '{'; Assert ($r.status -ne 0 -and !$r.summary.root_pid) 'malformed rejected'
[IO.File]::WriteAllText((Join-Path $temp 'selftest-result.json'), (@{cases=$script:count; outcome='passed'; evidence=$temp} | ConvertTo-Json))
Write-Output "PASS $script:count cases; retained evidence: $temp"
