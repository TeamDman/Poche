// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. https://mozilla.org/MPL/2.0/
// Test infrastructure only. No game IPC, network access, or interactive clipboard writes.
using System;
using System.Collections;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Principal;
using System.Text;
using System.Windows.Forms;

internal static class PrivateClipboardProbe
{
    [StructLayout(LayoutKind.Sequential)]
    struct SecurityAttributes { public int length; public IntPtr descriptor; public int inherit; }
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    struct StartupInfo
    {
        public int cb; public string reserved, desktop, title;
        public uint x, y, width, height, charsX, charsY, fill, flags;
        public ushort show, reservedBytes; public IntPtr reservedData, input, output, error;
    }
    [StructLayout(LayoutKind.Sequential)]
    struct ProcessInfo { public IntPtr process, thread; public uint processId, threadId; }
    [StructLayout(LayoutKind.Sequential)]
    struct UserFlags { public int inherit, reserved; public uint flags; }
    [StructLayout(LayoutKind.Sequential)]
    struct JobBasic
    {
        public long processTime, jobTime; public uint flags;
        public UIntPtr minimumWorkingSet, maximumWorkingSet;
        public uint activeProcesses; public UIntPtr affinity; public uint priority, scheduling;
    }
    [StructLayout(LayoutKind.Sequential)]
    struct IoCounters { public ulong readOps, writeOps, otherOps, readBytes, writeBytes, otherBytes; }
    [StructLayout(LayoutKind.Sequential)]
    struct JobExtended
    {
        public JobBasic basic; public IoCounters io;
        public UIntPtr processMemory, jobMemory, peakProcessMemory, peakJobMemory;
    }

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern IntPtr CreateWindowStationW(string name, uint flags, uint access, ref SecurityAttributes security);
    [DllImport("user32.dll", SetLastError = true)] static extern bool CloseWindowStation(IntPtr station);
    [DllImport("user32.dll", SetLastError = true)] static extern bool SetProcessWindowStation(IntPtr station);
    [DllImport("user32.dll")] static extern IntPtr GetProcessWindowStation();
    [DllImport("user32.dll")] static extern IntPtr GetThreadDesktop(uint thread);
    [DllImport("user32.dll", SetLastError = true)] static extern bool SetThreadDesktop(IntPtr desktop);
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern IntPtr CreateDesktopW(string name, IntPtr device, IntPtr mode, uint flags, uint access, ref SecurityAttributes security);
    [DllImport("user32.dll", SetLastError = true)] static extern bool CloseDesktop(IntPtr desktop);
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool GetUserObjectInformationW(IntPtr handle, int index, StringBuilder value, int length, out int needed);
    [DllImport("user32.dll", EntryPoint = "GetUserObjectInformationW", SetLastError = true)]
    static extern bool GetUserFlags(IntPtr handle, int index, out UserFlags value, int length, out int needed);
    [DllImport("user32.dll")] static extern uint GetClipboardSequenceNumber();
    [DllImport("user32.dll")] static extern IntPtr GetShellWindow();
    [DllImport("user32.dll", SetLastError = true)]
    static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool ConvertStringSecurityDescriptorToSecurityDescriptorW(string text, uint revision, out IntPtr descriptor, IntPtr size);
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
    [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
    [DllImport("kernel32.dll")] static extern IntPtr LocalFree(IntPtr pointer);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool CloseHandle(IntPtr handle);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool CreateProcessW(string application, StringBuilder command, IntPtr processSecurity,
        IntPtr threadSecurity, bool inherit, uint flags, IntPtr environment, string directory,
        ref StartupInfo startup, out ProcessInfo process);
    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool CreateProcessWithTokenW(IntPtr token, uint logonFlags, string application,
        StringBuilder command, uint flags, IntPtr environment, string directory,
        ref StartupInfo startup, out ProcessInfo process);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern IntPtr OpenProcess(uint access, bool inherit, uint processId);
    [DllImport("advapi32.dll", SetLastError = true)]
    static extern bool OpenProcessToken(IntPtr process, uint access, out IntPtr token);
    [DllImport("advapi32.dll", SetLastError = true)]
    static extern bool GetTokenInformation(IntPtr token, int kind, out int value, int length, out int needed);
    [DllImport("kernel32.dll", SetLastError = true)] static extern uint ResumeThread(IntPtr thread);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool TerminateProcess(IntPtr process, uint code);
    [DllImport("kernel32.dll", SetLastError = true)] static extern uint WaitForSingleObject(IntPtr handle, uint milliseconds);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool GetExitCodeProcess(IntPtr process, out uint code);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern IntPtr CreateJobObjectW(IntPtr security, string name);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool SetInformationJobObject(IntPtr job, int kind, ref JobExtended value, uint size);
    [DllImport("kernel32.dll", SetLastError = true)] static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);

    static void Check(bool success, string operation)
    {
        if (!success) throw new Win32Exception(Marshal.GetLastWin32Error(), operation);
    }
    static string Name(IntPtr handle)
    {
        var buffer = new StringBuilder(256); int needed;
        Check(GetUserObjectInformationW(handle, 2, buffer, buffer.Capacity * 2, out needed), "read object name");
        return buffer.ToString();
    }
    static void RequirePrivate(string stationName, string desktopName)
    {
        var station = GetProcessWindowStation();
        var actual = Name(station); UserFlags flags; int needed;
        Check(GetUserFlags(station, 1, out flags, Marshal.SizeOf(typeof(UserFlags)), out needed), "read station flags");
        if (String.IsNullOrEmpty(stationName) || stationName.Equals("WinSta0", StringComparison.OrdinalIgnoreCase)
            || !actual.Equals(stationName, StringComparison.OrdinalIgnoreCase) || (flags.flags & 1) != 0
            || !Name(GetThreadDesktop(GetCurrentThreadId())).Equals(desktopName, StringComparison.OrdinalIgnoreCase))
            throw new InvalidOperationException("Private station/desktop guard failed; clipboard untouched.");
    }

    static string Quote(string value)
    {
        // Windows argv quoting, including runs of backslashes before quotes/end.
        var output = new StringBuilder("\""); int slashes = 0;
        foreach (char c in value)
        {
            if (c == '\\') { slashes++; continue; }
            output.Append('\\', c == '"' ? slashes * 2 + 1 : slashes); slashes = 0; output.Append(c);
        }
        output.Append('\\', slashes * 2); return output.Append('"').ToString();
    }

    static void RunChild(IntPtr job, string station, string desktop, string role, string root, string sentinel)
    {
        var executable = Process.GetCurrentProcess().MainModule.FileName;
        var command = new StringBuilder();
        foreach (var argument in new[] { executable, "child", station, desktop, role, root, sentinel })
            command.Append(Quote(argument)).Append(' ');
        var startup = new StartupInfo { cb = Marshal.SizeOf(typeof(StartupInfo)), desktop = station + "\\" + desktop,
            flags = 1, show = 0 }; // STARTF_USESHOWWINDOW, SW_HIDE; never SwitchDesktop.
        ProcessInfo child;
        Check(CreateProcessW(executable, command, IntPtr.Zero, IntPtr.Zero, false,
            0x08000004, IntPtr.Zero, root, ref startup, out child), "create private child"); // NO_WINDOW | SUSPENDED
        bool finished = false;
        try
        {
            // Assign before execution; job closure kills assigned descendants.
            // Atomic create+job assignment is a remaining failure-injection gate.
            Check(AssignProcessToJobObject(job, child.process), "assign owned child");
            Check(ResumeThread(child.thread) != UInt32.MaxValue, "resume private child");
            if (WaitForSingleObject(child.process, 15000) != 0) throw new TimeoutException("Private clipboard child timed out.");
            finished = true; uint code;
            Check(GetExitCodeProcess(child.process, out code), "read child exit");
            if (code != 0) throw new InvalidOperationException("Private clipboard child failed: " + role + " (" + code + ").");
        }
        finally
        {
            if (!finished) { TerminateProcess(child.process, 124); WaitForSingleObject(child.process, 5000); }
            CloseHandle(child.thread); CloseHandle(child.process);
        }
    }

    static bool IsElevated(IntPtr token)
    {
        int elevated, needed;
        Check(GetTokenInformation(token, 20, out elevated, sizeof(int), out needed), "read token elevation");
        return elevated != 0;
    }

    static IntPtr OpenOrdinaryShellToken()
    {
        var shell = GetShellWindow();
        if (shell == IntPtr.Zero) throw new InvalidOperationException("Interactive shell window is unavailable.");
        uint processId;
        if (GetWindowThreadProcessId(shell, out processId) == 0 || processId == 0)
            throw new Win32Exception(Marshal.GetLastWin32Error(), "find interactive shell process");
        var process = OpenProcess(0x1000, false, processId); // PROCESS_QUERY_LIMITED_INFORMATION
        Check(process != IntPtr.Zero, "open interactive shell process");
        try
        {
            IntPtr token;
            Check(OpenProcessToken(process, 0x000b, out token), "open interactive shell token");
            try
            {
                using (var shellIdentity = new WindowsIdentity(token))
                {
                    if (shellIdentity.User == null || WindowsIdentity.GetCurrent().User == null
                        || !shellIdentity.User.Equals(WindowsIdentity.GetCurrent().User))
                        throw new InvalidOperationException("Shell token belongs to a different user.");
                }
                if (IsElevated(token))
                    throw new InvalidOperationException("Shell token is elevated; ordinary-player evidence unavailable.");
                return token;
            }
            catch
            {
                CloseHandle(token);
                throw;
            }
        }
        finally { CloseHandle(process); }
    }

    static void RequireElevatedBroker()
    {
        IntPtr token;
        Check(OpenProcessToken(GetCurrentProcess(), 0x0008, out token), "open broker token");
        try
        {
            if (!IsElevated(token))
                throw new InvalidOperationException("Acceptance broker requires one-time UAC elevation.");
        }
        finally { CloseHandle(token); }
    }

    static IntPtr EnvironmentBlock(IDictionary<string, string> overrides)
    {
        var values = new SortedDictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        foreach (DictionaryEntry entry in Environment.GetEnvironmentVariables())
        {
            var key = entry.Key as string; var value = entry.Value as string;
            if (!String.IsNullOrEmpty(key) && value != null && key.IndexOf('\0') < 0
                && key.IndexOf('=') < 0 && value.IndexOf('\0') < 0)
                values[key] = value;
        }
        foreach (var entry in overrides) values[entry.Key] = entry.Value;
        var block = new StringBuilder();
        foreach (var entry in values) block.Append(entry.Key).Append('=').Append(entry.Value).Append('\0');
        block.Append('\0');
        return Marshal.StringToHGlobalUni(block.ToString());
    }

    static ProcessInfo CreateOrdinaryChild(IntPtr token, IntPtr job, string station, string desktop,
        string application, IEnumerable<string> arguments, string directory,
        IDictionary<string, string> environment)
    {
        var command = new StringBuilder(Quote(application)).Append(' ');
        foreach (var argument in arguments) command.Append(Quote(argument)).Append(' ');
        var startup = new StartupInfo { cb = Marshal.SizeOf(typeof(StartupInfo)),
            desktop = station + "\\" + desktop, flags = 1, show = 0 };
        var block = EnvironmentBlock(environment); ProcessInfo child;
        try
        {
            Check(CreateProcessWithTokenW(token, 0, application, command,
                0x08000404, block, directory, ref startup, out child),
                "create ordinary private child"); // NO_WINDOW | UNICODE_ENVIRONMENT | SUSPENDED
        }
        finally { Marshal.FreeHGlobal(block); }
        try
        {
            IntPtr childToken;
            Check(OpenProcessToken(child.process, 0x0008, out childToken), "open child token");
            try
            {
                if (IsElevated(childToken))
                    throw new InvalidOperationException("Private child unexpectedly retained elevation.");
            }
            finally { CloseHandle(childToken); }
            Check(AssignProcessToJobObject(job, child.process), "assign ordinary child");
            return child;
        }
        catch
        {
            TerminateProcess(child.process, 125);
            CloseHandle(child.thread); CloseHandle(child.process);
            throw;
        }
    }

    static void Resume(ProcessInfo child)
    {
        Check(ResumeThread(child.thread) != UInt32.MaxValue, "resume ordinary private child");
    }

    static void CloseProcessInfo(ProcessInfo child)
    {
        if (child.thread != IntPtr.Zero) CloseHandle(child.thread);
        if (child.process != IntPtr.Zero) CloseHandle(child.process);
    }

    static uint WaitForExit(ProcessInfo child, uint timeout, string role)
    {
        if (WaitForSingleObject(child.process, timeout) != 0)
            throw new TimeoutException("Private child timed out: " + role + ".");
        uint code; Check(GetExitCodeProcess(child.process, out code), "read private child exit");
        return code;
    }

    static void RunOrdinarySentinelChild(IntPtr token, IntPtr job, string station, string desktop,
        string role, string root, string sentinel)
    {
        var executable = Process.GetCurrentProcess().MainModule.FileName;
        var child = CreateOrdinaryChild(token, job, station, desktop, executable,
            new[] { "child", station, desktop, role, root, sentinel }, root,
            new Dictionary<string, string>());
        try
        {
            Resume(child);
            var code = WaitForExit(child, 15000, "sentinel-" + role);
            if (code != 0) throw new InvalidOperationException("Ordinary private sentinel failed: " + role + ".");
        }
        finally { CloseProcessInfo(child); }
    }

    static ProcessInfo StartRustRole(IntPtr token, IntPtr job, string station, string desktop,
        string executable, string repository, string root, string evidence, string suffix, string role)
    {
        string nonce = Guid.NewGuid().ToString("N") + Guid.NewGuid().ToString("N");
        string attestation = Path.Combine(root, role + "-attestation.txt");
        var environment = new Dictionary<string, string> {
            { "POCHE_ALLOW_VEILID_PUBLIC_TEST", "I_ACCEPT_PUBLIC_NETWORK_TRAFFIC" },
            { "POCHE_PROCESS_PROBE_ROOT", root },
            { "POCHE_PROCESS_PROBE_ROLE", role },
            { "POCHE_PROCESS_PROBE_SUFFIX", suffix },
            { "POCHE_MENU_EVIDENCE_ROOT", evidence },
            { "POCHE_PRIVATE_CLIPBOARD_STATION", station },
            { "POCHE_PRIVATE_CLIPBOARD_DESKTOP", desktop },
            { "POCHE_PRIVATE_CLIPBOARD_NONCE", nonce },
            { "POCHE_PRIVATE_CLIPBOARD_ATTESTATION", attestation },
            { "WGPU_BACKEND", "dx12" }
        };
        var child = CreateOrdinaryChild(token, job, station, desktop, executable,
            new[] { "protected_desktop_private_clipboard_role", "--ignored", "--nocapture", "--test-threads=1" },
            repository, environment);
        try
        {
            File.WriteAllText(attestation,
                "version=1\r\npid=" + child.processId + "\r\nstation=" + station
                + "\r\ndesktop=" + desktop + "\r\nrole=" + role + "\r\nnonce=" + nonce
                + "\r\nelevated=false\r\n");
            Resume(child);
            return child;
        }
        catch
        {
            TerminateProcess(child.process, 125);
            CloseProcessInfo(child);
            throw;
        }
    }

    static void Accept(string testExecutable, string repository, string directory)
    {
        RequireElevatedBroker();
        string executable = Path.GetFullPath(testExecutable);
        string workingDirectory = Path.GetFullPath(repository);
        string root = Path.GetFullPath(directory);
        if (!File.Exists(executable)) throw new FileNotFoundException("Rust test executable is missing.", executable);
        if (!Directory.Exists(workingDirectory)) throw new DirectoryNotFoundException(workingDirectory);
        if (Directory.Exists(root) || File.Exists(root)) throw new IOException("A fresh evidence directory is required.");
        Directory.CreateDirectory(root);
        Directory.CreateDirectory(Path.Combine(root, "rendered"));
        IntPtr station = IntPtr.Zero, desktop = IntPtr.Zero, descriptor = IntPtr.Zero;
        IntPtr job = IntPtr.Zero, shellToken = IntPtr.Zero;
        ProcessInfo creator = new ProcessInfo(), joiner = new ProcessInfo();
        var originalStation = GetProcessWindowStation(); var originalDesktop = GetThreadDesktop(GetCurrentThreadId());
        var originalName = Name(originalStation); var originalSequence = GetClipboardSequenceNumber();
        try
        {
            shellToken = OpenOrdinaryShellToken();
            string sid = WindowsIdentity.GetCurrent().User.Value;
            Check(ConvertStringSecurityDescriptorToSecurityDescriptorW("D:P(A;;GA;;;" + sid + ")", 1,
                out descriptor, IntPtr.Zero), "make current-user-only ACL");
            var security = new SecurityAttributes { length = Marshal.SizeOf(typeof(SecurityAttributes)),
                descriptor = descriptor, inherit = 0 };
            station = CreateWindowStationW("Poche-" + Guid.NewGuid().ToString("N"), 1, 0x0002037f, ref security);
            Check(station != IntPtr.Zero, "create named private station");
            string stationName = Name(station);
            if (stationName.Equals(originalName, StringComparison.OrdinalIgnoreCase)
                || stationName.Equals("WinSta0", StringComparison.OrdinalIgnoreCase))
                throw new InvalidOperationException("Station is not isolated.");
            try
            {
                Check(SetProcessWindowStation(station), "select private station for desktop creation");
                desktop = CreateDesktopW("Poche", IntPtr.Zero, IntPtr.Zero, 0, 0x0083, ref security);
                Check(desktop != IntPtr.Zero, "create private desktop");
            }
            finally
            {
                Check(SetProcessWindowStation(originalStation), "restore broker station");
                Check(SetThreadDesktop(originalDesktop), "restore broker desktop");
            }
            job = CreateJobObjectW(IntPtr.Zero, null); Check(job != IntPtr.Zero, "create owned job");
            var limits = new JobExtended { basic = new JobBasic { flags = 0x2000 } };
            Check(SetInformationJobObject(job, 9, ref limits, (uint)Marshal.SizeOf(typeof(JobExtended))),
                "set owned job lifetime");

            string sentinel = "poche-private-station-" + Guid.NewGuid().ToString("N");
            RunOrdinarySentinelChild(shellToken, job, stationName, "Poche", "write", root, sentinel);
            RunOrdinarySentinelChild(shellToken, job, stationName, "Poche", "read", root, sentinel);

            string suffix = DateTime.UtcNow.Ticks.ToString() + "-" + Guid.NewGuid().ToString("N").Substring(0, 8);
            string rendered = Path.Combine(root, "rendered");
            creator = StartRustRole(shellToken, job, stationName, "Poche", executable,
                workingDirectory, root, rendered, suffix, "creator");
            joiner = StartRustRole(shellToken, job, stationName, "Poche", executable,
                workingDirectory, root, rendered, suffix, "joiner");

            uint creatorCode = WaitForExit(creator, 300000, "creator");
            uint joinerCode = WaitForExit(joiner, 300000, "joiner");
            if (creatorCode != 0 || joinerCode != 0)
                throw new InvalidOperationException("Rust clipboard roles failed: creator=" + creatorCode
                    + ", joiner=" + joinerCode + ". See redacted role result files.");
            if (!File.Exists(Path.Combine(root, "private-clipboard-ready"))
                || !File.Exists(Path.Combine(root, "private-clipboard-joined"))
                || !File.Exists(Path.Combine(root, "private-clipboard-observed")))
                throw new InvalidOperationException("Rust clipboard acceptance markers are incomplete.");
            if (Name(GetProcessWindowStation()) != originalName || GetClipboardSequenceNumber() != originalSequence)
                throw new InvalidOperationException("Interactive clipboard sequence changed; no restoration attempted.");
            File.WriteAllText(Path.Combine(root, "result.txt"),
                "PASS: ordinary-integrity rendered Poche processes used Copy/Paste on an isolated OS clipboard and joined over public Veilid; interactive clipboard sequence unchanged.\r\n");
            Console.WriteLine("PASS: rendered private OS clipboard exchange; interactive clipboard unchanged.");
        }
        finally
        {
            CloseProcessInfo(creator); CloseProcessInfo(joiner);
            if (job != IntPtr.Zero) CloseHandle(job);
            if (shellToken != IntPtr.Zero) CloseHandle(shellToken);
            if (desktop != IntPtr.Zero) CloseDesktop(desktop);
            if (station != IntPtr.Zero) CloseWindowStation(station);
            if (descriptor != IntPtr.Zero) LocalFree(descriptor);
        }
    }

    static void Probe(string directory)
    {
        string root = Path.GetFullPath(directory);
        if (Directory.Exists(root) || File.Exists(root)) throw new IOException("A fresh evidence directory is required.");
        Directory.CreateDirectory(root);
        IntPtr station = IntPtr.Zero, desktop = IntPtr.Zero, descriptor = IntPtr.Zero, job = IntPtr.Zero;
        var originalStation = GetProcessWindowStation(); var originalDesktop = GetThreadDesktop(GetCurrentThreadId());
        var originalName = Name(originalStation); var originalSequence = GetClipboardSequenceNumber();
        try
        {
            // Avoid the API's permissive/variable default security descriptor.
            string sid = WindowsIdentity.GetCurrent().User.Value;
            Check(ConvertStringSecurityDescriptorToSecurityDescriptorW("D:P(A;;GA;;;" + sid + ")", 1,
                out descriptor, IntPtr.Zero), "make current-user-only ACL");
            var security = new SecurityAttributes { length = Marshal.SizeOf(typeof(SecurityAttributes)), descriptor = descriptor, inherit = 0 };
            station = CreateWindowStationW("Poche-" + Guid.NewGuid().ToString("N"), 1, 0x0002037f, ref security);
            if (station == IntPtr.Zero && Marshal.GetLastWin32Error() == 5)
                station = CreateWindowStationW(null, 1, 0x0002037f, ref security);
            Check(station != IntPtr.Zero, "create-only private station");
            string stationName = Name(station);
            if (stationName.Equals(originalName, StringComparison.OrdinalIgnoreCase)
                || stationName.Equals("WinSta0", StringComparison.OrdinalIgnoreCase))
                throw new InvalidOperationException("Station is not isolated.");
            try
            {
                Check(SetProcessWindowStation(station), "select private station for desktop creation");
                desktop = CreateDesktopW("Poche", IntPtr.Zero, IntPtr.Zero, 0, 0x0083, ref security);
                Check(desktop != IntPtr.Zero, "create private desktop");
            }
            finally
            {
                Check(SetProcessWindowStation(originalStation), "restore launcher station");
                Check(SetThreadDesktop(originalDesktop), "restore launcher thread desktop");
            }
            job = CreateJobObjectW(IntPtr.Zero, null); Check(job != IntPtr.Zero, "create owned job");
            var limits = new JobExtended { basic = new JobBasic { flags = 0x2000 } }; // KILL_ON_JOB_CLOSE
            Check(SetInformationJobObject(job, 9, ref limits, (uint)Marshal.SizeOf(typeof(JobExtended))), "set owned job lifetime");
            string sentinel = "poche-clipboard-probe-" + Guid.NewGuid().ToString("N");
            RunChild(job, stationName, "Poche", "write", root, sentinel);
            RunChild(job, stationName, "Poche", "read", root, sentinel);
            if (Name(GetProcessWindowStation()) != originalName || GetClipboardSequenceNumber() != originalSequence)
                throw new InvalidOperationException("Interactive clipboard sequence changed; no restoration attempted.");
            File.WriteAllText(Path.Combine(root, "result.txt"), "PASS: two private child processes exchanged OS clipboard text; original clipboard sequence unchanged.\r\n");
            Console.WriteLine("PASS: private clipboard exchange; original clipboard sequence unchanged.");
        }
        finally
        {
            if (job != IntPtr.Zero) CloseHandle(job);
            if (desktop != IntPtr.Zero) CloseDesktop(desktop);
            if (station != IntPtr.Zero) CloseWindowStation(station);
            if (descriptor != IntPtr.Zero) LocalFree(descriptor);
        }
    }

    [STAThread]
    static int Main(string[] args)
    {
        try
        {
            if (args.Length == 2 && args[0] == "probe") { Probe(args[1]); return 0; }
            if (args.Length == 4 && args[0] == "accept")
            {
                Accept(args[1], args[2], args[3]); return 0;
            }
            if (args.Length != 6 || args[0] != "child") throw new ArgumentException("Expected probe <fresh evidence directory>.");
            // Guard runs before ANY Windows Forms/OLE clipboard operation.
            RequirePrivate(args[1], args[2]);
            if (args[3] == "write") Clipboard.SetText(args[5], TextDataFormat.UnicodeText);
            else if (args[3] == "read")
            {
                if (Clipboard.GetText(TextDataFormat.UnicodeText) != args[5]) throw new InvalidOperationException("Private clipboard content mismatch.");
            }
            else throw new ArgumentException("Unknown private child role.");
            File.WriteAllText(Path.Combine(args[4], args[3] + ".txt"), "PASS: private station verified before clipboard access.\r\n");
            return 0;
        }
        catch (Exception error)
        {
            var native = error as Win32Exception;
            Console.Error.WriteLine(error.GetType().Name + ": " + error.Message
                + (native == null ? "" : " (Win32 " + native.NativeErrorCode + ")"));
            return 1;
        }
    }
}
