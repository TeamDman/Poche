# Private OS clipboard acceptance

This test-only launcher investigates using a noninteractive Windows window
station to keep acceptance tests away from the user's rich clipboard.
It is **not game IPC**, a production dependency, or completed Poche acceptance.

Compile with the installed .NET Framework compiler (absolute backslash paths
avoid the legacy compiler's command-line parsing ambiguity):

```powershell
New-Item -ItemType Directory -Path D:\Repos\Games\poche-3\target\windows-acceptance -Force | Out-Null
& C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe /nologo /target:exe /reference:System.Windows.Forms.dll /out:D:\Repos\Games\poche-3\target\windows-acceptance\poche-private-clipboard-probe.exe D:\Repos\Games\poche-3\tools\windows-acceptance\PrivateClipboardProbe.cs
& D:\Repos\Games\poche-3\target\windows-acceptance\poche-private-clipboard-probe.exe probe D:\Repos\Games\poche-3\target\private-clipboard-fresh
```

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

On September 10, compilation succeeded, but an ordinary-user run could not
create a named station (AccessDenied). The create-only unnamed alternative
failed with Win32 183 (AlreadyExists). The probe refused to open that existing
station and stopped before any child or clipboard access. Explicit `child
WinSta0 ...` invocation also failed its guard before clipboard access.

Thus **positive isolation, child-job cleanup on timeout, and Bevy clipboard
integration are not yet proven**. No administrator launch has been performed.
A proposed one-time broker would create/hold a fresh station while test
processes remain unelevated; that needs user approval and implementation.
Running this entire probe elevated is not evidence for an ordinary-user game.
Do not remove create-only/WinSta0 guards or substitute text-only backup/restore.

References: Microsoft's [window stations](https://learn.microsoft.com/en-us/windows/win32/winstation/window-stations),
[creation API](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-createwindowstationw),
[process connection rules](https://learn.microsoft.com/en-us/windows/win32/winstation/process-connection-to-a-window-station),
and [STARTUPINFO](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/ns-processthreadsapi-startupinfow).
