# Private OS clipboard acceptance

This test-only launcher uses a noninteractive Windows window station to keep
acceptance tests away from the user's rich clipboard. It is **not game IPC**
or a production dependency. Its `accept` mode is the final Windows clipboard
carrier witness for the desktop Veilid slice; it is not complete evidence until
the UAC-scoped run itself passes.

Compile with the installed .NET Framework compiler (absolute backslash paths
avoid the legacy compiler's command-line parsing ambiguity):

```powershell
New-Item -ItemType Directory -Path D:\Repos\Games\poche-3\target\windows-acceptance -Force | Out-Null
& C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe /nologo /target:exe /reference:System.Windows.Forms.dll /out:D:\Repos\Games\poche-3\target\windows-acceptance\poche-private-clipboard-probe.exe D:\Repos\Games\poche-3\tools\windows-acceptance\PrivateClipboardProbe.cs
& D:\Repos\Games\poche-3\target\windows-acceptance\poche-private-clipboard-probe.exe probe D:\Repos\Games\poche-3\target\private-clipboard-fresh
```

Build the opt-in Rust test binary, copy the exact executable path printed by
Cargo, then launch the broker only with explicit approval for UAC elevation and
public Veilid traffic:

```powershell
cd D:\Repos\Games\poche-3
cargo test --locked --offline -p poche-cli --features native-input-test --lib --no-run
$pocheTestExe = 'D:\Repos\Games\poche-3\target\debug\deps\poche_cli-REPLACE_WITH_PRINTED_HASH.exe'
$pocheEvidence = 'D:\Repos\Games\poche-3\target\private-clipboard-acceptance-fresh'
$pocheBroker = 'D:\Repos\Games\poche-3\target\windows-acceptance\poche-private-clipboard-probe.exe'
$pocheRun = Start-Process -FilePath $pocheBroker -ArgumentList @('accept', $pocheTestExe, 'D:\Repos\Games\poche-3', $pocheEvidence) -Verb RunAs -WindowStyle Hidden -Wait -PassThru
if ($pocheRun.ExitCode -ne 0) { throw "Private clipboard acceptance failed with exit code $($pocheRun.ExitCode)." }
Get-Content -LiteralPath (Join-Path $pocheEvidence 'result.txt')
```

The broker alone is elevated. It obtains the existing interactive shell token,
requires that token and both child tokens to be non-elevated, and launches each
child with an explicit private `station\\desktop`. Before any Poche process, two
ordinary-integrity sentinel processes verify the actual station, noninteractive
flag, desktop and private clipboard exchange. Each Rust role consumes a one-use
attestation bound to its PID, role, nonce and evidence directory before Bevy
initializes the production clipboard backend. The creator then clicks Copy and
the joiner clicks Paste; no invitation file or log substitutes for that carrier.
The processes are windowless and owned by a kill-on-close job.

The evidence directory must not exist. The intended test creates a station
with a current-user-only DACL, creates a private desktop, and runs separate
writer/reader children with explicit `STARTUPINFO.lpDesktop`. Every child checks
its actual station name, noninteractive flag and desktop before accessing the
clipboard. Children start suspended, are assigned to a kill-on-close job, then
run with a 15-second limit. No visible-desktop switch or interactive clipboard
content read/write exists in the launcher. It only compares the original
clipboard's sequence number before/after; unrelated user clipboard activity
would invalidate that unchanged-sequence observation, not trigger restoration.

## Current evidence and boundary

On September 10, the original smoke-probe compilation succeeded, but an ordinary-user run could not
create a named station (AccessDenied). The create-only unnamed alternative
failed with Win32 183 (AlreadyExists). The probe refused to open that existing
station and stopped before any child or clipboard access. Explicit `child
WinSta0 ...` invocation also failed its guard before clipboard access.

The broker/ordinary-child split is now implemented. The C# source compiles; its
ordinary invocation rejects `accept` before station creation with “requires
one-time UAC elevation”; and the Rust native UI/CLI suites compile and pass with
the PID-bound path present. The actual elevated broker run has not been approved
or executed, so **positive private-station isolation, job cleanup and genuine
Bevy Copy/Paste remain unproven**. Running the entire game as administrator is
not a substitute for ordinary-user evidence. Do not remove create-only/WinSta0
guards or substitute text-only backup/restore.

References: Microsoft's [window stations](https://learn.microsoft.com/en-us/windows/win32/winstation/window-stations),
[creation API](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-createwindowstationw),
[process connection rules](https://learn.microsoft.com/en-us/windows/win32/winstation/process-connection-to-a-window-station),
and [STARTUPINFO](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/ns-processthreadsapi-startupinfow).
The ordinary-child launch additionally follows Microsoft
[`CreateProcessWithTokenW`](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createprocesswithtokenw).
