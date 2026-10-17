// Original GreenLens source, Apache-2.0. Observation only: no process creation.
// Query the kernel's executable path through the held process handle, avoiding
// Process.MainModule's nullable module enumeration during short process lives.
// https://learn.microsoft.com/windows/win32/api/winbase/nf-winbase-queryfullprocessimagenamew
// https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes
using System;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32.SafeHandles;

namespace GreenLensObservation {
    public sealed class ProcessImage {
        public int ProcessId { get; internal set; }
        public long StartedUtcTicks { get; internal set; }
        public string ImagePath { get; internal set; }
        public string Name { get { return Path.GetFileNameWithoutExtension(ImagePath); } }
    }

    public static class HeldProcessImage {
        [StructLayout(LayoutKind.Sequential)]
        struct FileTime { public uint Low; public uint High; }
        [DllImport("kernel32.dll", SetLastError = true)]
        static extern uint GetProcessId(SafeProcessHandle process);
        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        static extern bool GetProcessTimes(SafeProcessHandle process, out FileTime created,
            out FileTime exited, out FileTime kernel, out FileTime user);
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        static extern bool QueryFullProcessImageNameW(SafeProcessHandle process, uint flags,
            StringBuilder path, ref uint length);

        public static void ValidateBinding(SafeProcessHandle process, int expectedPid, long expectedStartedUtcTicks) {
            if (process == null || process.IsInvalid || process.IsClosed)
                throw new InvalidOperationException("Observed process handle is unavailable; executable identity is unverified.");
            uint actualPid = GetProcessId(process);
            if (actualPid == 0) throw new Win32Exception(Marshal.GetLastWin32Error(), "Observed process handle identity query failed.");
            FileTime created, exited, kernel, user;
            if (!GetProcessTimes(process, out created, out exited, out kernel, out user))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Observed process creation-time query failed.");
            long started = DateTime.FromFileTimeUtc(unchecked((long)(((ulong)created.High << 32) | created.Low))).Ticks;
            if (expectedPid <= 0 || actualPid != (uint)expectedPid || started != expectedStartedUtcTicks)
                throw new InvalidOperationException("Observed PID/creation-time binding changed; no cached or supplied executable identity is accepted.");
        }

        public static ProcessImage Read(SafeProcessHandle process, int expectedPid, long expectedStartedUtcTicks) {
            ValidateBinding(process, expectedPid, expectedStartedUtcTicks);
            // One fixed bounded query; no polling, timeout extension, path fallback,
            // supplied expected-path substitution, or module enumeration.
            const int maximumCharacters = 32768;
            var path = new StringBuilder(maximumCharacters);
            uint length = maximumCharacters;
            if (!QueryFullProcessImageNameW(process, 0, path, ref length))
                throw new Win32Exception(Marshal.GetLastWin32Error(), "Observed executable image query failed; identity remains unverified.");
            string image = path.ToString();
            if (length == 0 || length >= maximumCharacters || image.Length != length ||
                String.IsNullOrWhiteSpace(image) || !Path.IsPathFullyQualified(image) ||
                String.IsNullOrWhiteSpace(Path.GetFileNameWithoutExtension(image)))
                throw new InvalidOperationException("Observed executable image path is empty or invalid; identity remains unverified.");
            return new ProcessImage { ProcessId = expectedPid, StartedUtcTicks = expectedStartedUtcTicks, ImagePath = image };
        }
    }
}
