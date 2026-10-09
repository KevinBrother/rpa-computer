# Build the Windows acceptance fixture (.NET Framework WinForms, no NuGet).
# Run this ON the Windows machine (e.g. via ssh acer-win).
# Usage: powershell -ExecutionPolicy Bypass -File build-windows.ps1 [-OutDir <dir>]
# Does not delete or overwrite anything outside the chosen output directory.
param(
    [string]$OutDir = "$PSScriptRoot\build\windows"
)

$ErrorActionPreference = 'Stop'

$src    = Join-Path $PSScriptRoot 'windows\MainForm.cs'
$srcCases = Join-Path $PSScriptRoot 'windows\Cases.cs'
$gestureSources = @(Get-ChildItem -LiteralPath (Join-Path $PSScriptRoot 'windows') -Filter 'Gesture*.cs' | ForEach-Object { $_.FullName })
$basicSources = @(Get-ChildItem -LiteralPath (Join-Path $PSScriptRoot 'windows') -Filter 'Basic*.cs' | ForEach-Object { $_.FullName })
$focusSources = @(Get-ChildItem -LiteralPath (Join-Path $PSScriptRoot 'windows') -Filter 'Focus*.cs' | ForEach-Object { $_.FullName })
$nativeTextSources = @(Get-ChildItem -LiteralPath (Join-Path $PSScriptRoot 'windows') -Filter 'NativeText*.cs' | ForEach-Object { $_.FullName })
$geometrySources = @(Get-ChildItem -LiteralPath (Join-Path $PSScriptRoot 'windows') -Filter 'Geometry*.cs' | ForEach-Object { $_.FullName })
$focusTestSource = Join-Path $PSScriptRoot 'windows\FocusSelfTest.cs'
$exe    = Join-Path $OutDir 'ComputerUseAcceptance.exe'
$csc    = 'C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe'

if (-not (Test-Path $csc)) { throw "csc.exe not found at $csc" }
if (-not (Test-Path $src)) { throw "source not found at $src" }
if (-not (Test-Path $srcCases)) { throw "source not found at $srcCases" }
if (-not (Test-Path $focusTestSource)) { throw "source not found at $focusTestSource" }
if (Test-Path $exe) { throw "$exe already exists; remove it yourself if you want to rebuild there" }

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

& $csc /nologo /langversion:5 /codepage:65001 /target:winexe /platform:anycpu /utf8output /optimize+ `
    /out:"$exe" "$src" "$srcCases" $gestureSources $basicSources $focusSources $nativeTextSources $geometrySources `
    /reference:System.dll,System.Drawing.dll,System.Windows.Forms.dll,System.Web.Extensions.dll
if ($LASTEXITCODE -ne 0) { throw "csc failed with exit code $LASTEXITCODE" }

Write-Host "built: $exe"
Get-FileHash -Algorithm SHA256 $exe | Format-List
