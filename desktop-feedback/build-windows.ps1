param([string]$Output = "")
$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
if (!$Output) { $Output = Join-Path $root 'build\desktop-feedback-windows.exe' }
$Output = [IO.Path]::GetFullPath($Output)
$null = New-Item -ItemType Directory -Force -Path ([IO.Path]::GetDirectoryName($Output))
# Required compiler: no dotnet SDK / NuGet / Rust and no silent tool fallback.
$csc = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
if (!(Test-Path -LiteralPath $csc)) { throw 'Framework64 v4 csc.exe not found; install/enable .NET Framework explicitly.' }
$sources = @('JsonValue.cs', 'Protocol.cs', 'Model.cs', 'IPC.cs', 'Native.cs', 'Windows.cs', 'SelfTests.cs', 'Program.cs') | ForEach-Object { Join-Path $root "windows\$_" }
& $csc /nologo /langversion:5 /target:winexe /platform:x64 /optimize+ /warnaserror+ "/out:$Output" "/win32manifest:$(Join-Path $root 'windows\app.manifest')" /reference:System.dll /reference:System.Core.dll /reference:System.Drawing.dll /reference:System.Windows.Forms.dll $sources
if ($LASTEXITCODE -ne 0) { throw "csc compilation failed ($LASTEXITCODE)" }
Copy-Item -LiteralPath (Join-Path $root 'windows\desktop-feedback-windows.exe.config') -Destination "$Output.config" -Force
Write-Output "Built $Output (not launched; not acceptance)"
