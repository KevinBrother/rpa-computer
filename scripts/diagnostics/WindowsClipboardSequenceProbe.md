# Windows clipboard sequence probe

`WindowsClipboardSequenceProbe.cs` is a standalone .NET Framework console diagnostic, independent of the product. It reports metadata from its **own process**, not another host process.

## Build and operator-controlled execution

Compile with an installed .NET Framework C# compiler in an approved staging directory:

```powershell
$compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
& $compiler /nologo /target:exe /platform:x64 /out:WindowsClipboardSequenceProbe.exe WindowsClipboardSequenceProbe.cs
if ($LASTEXITCODE -ne 0) { throw 'Diagnostic compile failed' }
Get-FileHash .\WindowsClipboardSequenceProbe.exe -Algorithm SHA256
```

Only after the desktop operator authorizes a quiet diagnostic window, run the exact hash-verified executable once in the intended interactive user/session/desktop context. Do not substitute an SSH/service session. Record the actual user, elevation, and session from the report rather than assuming they match another process. The executable is short-lived: capture stdout, stderr, native exit status, and executable hash; do not require it to stay running. This directory does not provide a deployment or scheduled-task launcher.

## Scope and safety

- Reports PID/TID, executable path, session, station/desktop names, actual handle GrantedAccess when queryable, and token metadata. Keep identity metadata in local evidence.
- Calls GetClipboardSequenceNumber and performs exactly one OpenClipboard(NULL)/CloseClipboard pair. While locked, it only samples sequence and LastError; release is in finally.
- No clipboard contents or format enumeration, EmptyClipboard/SetClipboardData, input injection, window creation, permission changes, retries, or clipboard seeding.
- LastError after a sequence query is observational: that API has no documented LastError contract. Token/query errors remain unknown metadata, not successful checks.
- Exit 2 reports a failed clipboard close. Exit 0 does **not** establish successful open, paste, or matching host context; inspect the individual fields.

## Interpretation

Successful open with locked sequence zero and successful close demonstrates usable clipboard-open access with zero **for this probe at that time**. It does not establish another process's handle rights, the earlier failure's precise state, or the counter's initialization history.

A successful station GrantedAccess query containing WINSTA_ACCESSCLIPBOARD (0x0004) is supporting access metadata. Failed NtQueryObject, including unsupported-handle failures, leaves that metadata unknown; successful open with locked zero remains useful independently. Open failure may reflect contention or unavailability and is not by itself proof of access denial. Preserve contradictions for review.

Nonzero later samples cannot reconstruct an earlier zero. Neither a raw zero nor unchanged sequence alone authorizes a production clipboard write; access, focus, ownership, and stage-specific safety checks are separate concerns.

## Official API references

```text
https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getclipboardsequencenumber
https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-openclipboard
https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntqueryobject
https://learn.microsoft.com/en-us/windows/win32/winstation/window-station-security-and-access-rights
```
