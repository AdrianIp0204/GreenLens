// Original GreenLens research supervision helper, Apache-2.0.
// Windows 10+: job membership and the explicit inherited-handle list are applied
// by CreateProcess itself, before the executable can create a descendant.
using System;
using System.Collections;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using Microsoft.Win32.SafeHandles;

namespace GreenLensPairedProcess {
public sealed class NativeProcess : IDisposable {
    const uint EXTENDED_STARTUPINFO_PRESENT = 0x00080000;
    const uint CREATE_UNICODE_ENVIRONMENT = 0x00000400;
    const uint CREATE_NO_WINDOW = 0x08000000;
    const uint STARTF_USESTDHANDLES = 0x00000100;
    const uint HANDLE_FLAG_INHERIT = 1;
    const uint KILL_ON_JOB_CLOSE = 0x00002000;
    const uint WAIT_OBJECT_0 = 0;
    const uint WAIT_TIMEOUT = 258;
    const int ERROR_MORE_DATA = 234;
    static readonly IntPtr ATTRIBUTE_HANDLE_LIST = new IntPtr(0x00020002);
    static readonly IntPtr ATTRIBUTE_JOB_LIST = new IntPtr(0x0002000d);
    readonly SafeFileHandle job;
    readonly SafeFileHandle process;
    readonly int id;
    readonly Dictionary<uint,SafeFileHandle> observedMembers=new Dictionary<uint,SafeFileHandle>();
    long peakWorkingSet;
    bool disposed;
    public Stream Input { get; private set; }
    public Stream Output { get; private set; }
    public Stream Error { get; private set; }
    public int Id { get { Check(); return id; } }

    NativeProcess(SafeFileHandle job, SafeFileHandle process, int id,
                  Stream input, Stream output, Stream error) {
        this.job=job; this.process=process; this.id=id;
        Input=input; Output=output; Error=error;
    }
    void Check() { if(disposed) throw new ObjectDisposedException(nameof(NativeProcess)); }

    // Directory-entry metadata may remain stale while a writer still has its
    // file open. Query the file object through a freshly opened native handle.
    public static long FileSize(string path) {
        using(var handle=CreateFileW(Path.GetFullPath(path),0,7,IntPtr.Zero,3,0,IntPtr.Zero)) {
            RequireHandle(handle,"Open file for live size");
            long bytes; if(!GetFileSizeEx(handle,out bytes)) Fail("Get live file size");
            if(bytes<0) throw new InvalidDataException("Invalid live file size.");
            return bytes;
        }
    }

    // Quote according to the Windows CRT/CommandLineToArgvW backslash/quote
    // convention. No shell is involved; executable is passed separately too.
    public static string QuoteArgument(string value) {
        if(value == null || value.IndexOf('\0') >= 0)
            throw new ArgumentException("Arguments cannot be null or contain NUL.");
        var result=new StringBuilder("\""); int slashes=0;
        foreach(char c in value) {
            if(c=='\\') { slashes++; continue; }
            if(c=='\"') { result.Append('\\',checked(slashes*2+1)); result.Append(c); slashes=0; continue; }
            result.Append('\\',slashes); slashes=0; result.Append(c);
        }
        result.Append('\\',checked(slashes*2)); result.Append('"');
        return result.ToString();
    }

    public static NativeProcess Start(string executable, string[] arguments,
                                      string workingDirectory, string rayPath) {
        return StartInternal(executable,arguments,workingDirectory,rayPath,0,0);
    }
    // Opt-in engineering supervision; the historical Start behavior is unchanged.
    // Kernel limits apply atomically at creation to the whole non-breakaway job,
    // including descendants and nested jobs. Memory is committed memory, not RSS.
    public static NativeProcess StartBounded(string executable, string[] arguments,
        string workingDirectory, string rayPath, long jobCommittedBytes, uint activeProcesses) {
        if(jobCommittedBytes<=0 || activeProcesses==0 || activeProcesses>16)
            throw new ArgumentOutOfRangeException("Positive committed-memory and 1..16 process bounds required.");
        return StartInternal(executable,arguments,workingDirectory,rayPath,jobCommittedBytes,activeProcesses);
    }
    static NativeProcess StartInternal(string executable, string[] arguments,
        string workingDirectory, string rayPath, long jobCommittedBytes, uint activeProcesses) {
        if(!OperatingSystem.IsWindowsVersionAtLeast(10))
            throw new PlatformNotSupportedException("Atomic job launch requires Windows 10 or newer.");
        if(arguments==null) throw new ArgumentNullException(nameof(arguments));
        if(string.IsNullOrWhiteSpace(executable) || executable.IndexOf('\0')>=0 ||
           string.IsNullOrWhiteSpace(workingDirectory) || workingDirectory.IndexOf('\0')>=0 ||
           rayPath==null || rayPath.IndexOf('\0')>=0)
            throw new ArgumentException("Invalid executable, directory, or child RAYPATH.");
        executable=Path.GetFullPath(executable); workingDirectory=Path.GetFullPath(workingDirectory);
        if(!File.Exists(executable)) throw new FileNotFoundException("Required executable is unavailable.",executable);
        if(!Directory.Exists(workingDirectory)) throw new DirectoryNotFoundException("Required working directory is unavailable.");
        var command=new StringBuilder(QuoteArgument(executable));
        foreach(string argument in arguments) command.Append(' ').Append(QuoteArgument(argument));
        if(command.Length>=32767) throw new ArgumentException("Windows command line exceeds its declared limit.");

        SafeFileHandle job=null, process=null, thread=null;
        SafeFileHandle inputRead=null, inputWrite=null, outputRead=null, outputWrite=null, errorRead=null, errorWrite=null;
        Stream input=null, output=null, error=null;
        IntPtr attributes=IntPtr.Zero, jobs=IntPtr.Zero, handles=IntPtr.Zero, environment=IntPtr.Zero;
        bool attributesReady=false, success=false;
        try {
            job=CreateJobObjectW(IntPtr.Zero,null);
            RequireHandle(job,"CreateJobObject");
            var limits=new ExtendedLimits(); limits.Basic.LimitFlags=KILL_ON_JOB_CLOSE;
            if(jobCommittedBytes>0) {
                limits.Basic.LimitFlags|=0x00000200|0x00000008; // JOB_MEMORY | ACTIVE_PROCESS
                limits.jobMemory=new UIntPtr(checked((ulong)jobCommittedBytes));
                limits.Basic.activeProcessLimit=activeProcesses;
            }
            if(!SetInformationJobObject(job,9,ref limits,(uint)Marshal.SizeOf<ExtendedLimits>())) Fail("SetInformationJobObject");

            Pipe(out inputRead,out inputWrite); Uninherit(inputWrite);
            Pipe(out outputRead,out outputWrite); Uninherit(outputRead);
            Pipe(out errorRead,out errorWrite); Uninherit(errorRead);
            IntPtr attributeBytes=IntPtr.Zero;
            InitializeProcThreadAttributeList(IntPtr.Zero,2,0,ref attributeBytes);
            if(attributeBytes==IntPtr.Zero) Fail("InitializeProcThreadAttributeList size");
            attributes=Marshal.AllocHGlobal(attributeBytes);
            if(!InitializeProcThreadAttributeList(attributes,2,0,ref attributeBytes)) Fail("InitializeProcThreadAttributeList");
            attributesReady=true;
            jobs=Marshal.AllocHGlobal(IntPtr.Size); Marshal.WriteIntPtr(jobs,job.DangerousGetHandle());
            if(!UpdateProcThreadAttribute(attributes,0,ATTRIBUTE_JOB_LIST,jobs,new IntPtr(IntPtr.Size),IntPtr.Zero,IntPtr.Zero)) Fail("Set atomic job list");
            handles=Marshal.AllocHGlobal(IntPtr.Size*3);
            Marshal.WriteIntPtr(handles,0,inputRead.DangerousGetHandle());
            Marshal.WriteIntPtr(handles,IntPtr.Size,outputWrite.DangerousGetHandle());
            Marshal.WriteIntPtr(handles,IntPtr.Size*2,errorWrite.DangerousGetHandle());
            if(!UpdateProcThreadAttribute(attributes,0,ATTRIBUTE_HANDLE_LIST,handles,new IntPtr(IntPtr.Size*3),IntPtr.Zero,IntPtr.Zero)) Fail("Set inherited handle list");

            var environmentValues=new SortedDictionary<string,string>(StringComparer.OrdinalIgnoreCase);
            foreach(DictionaryEntry item in Environment.GetEnvironmentVariables())
                environmentValues[(string)item.Key]=(string)item.Value;
            environmentValues["RAYPATH"]=rayPath;
            var environmentText=new StringBuilder();
            foreach(var item in environmentValues) {
                if(item.Key.IndexOf('\0')>=0 || item.Value.IndexOf('\0')>=0) throw new ArgumentException("Invalid child environment value.");
                environmentText.Append(item.Key).Append('=').Append(item.Value).Append('\0');
            }
            environmentText.Append('\0'); environment=Marshal.StringToHGlobalUni(environmentText.ToString());

            var startup=new StartupInfoEx();
            startup.Startup.cb=Marshal.SizeOf<StartupInfoEx>();
            startup.Startup.flags=STARTF_USESTDHANDLES;
            startup.Startup.stdin=inputRead.DangerousGetHandle();
            startup.Startup.stdout=outputWrite.DangerousGetHandle();
            startup.Startup.stderr=errorWrite.DangerousGetHandle();
            startup.attributes=attributes;
            ProcessInformation created;
            if(!CreateProcessW(executable,command,IntPtr.Zero,IntPtr.Zero,true,
                               EXTENDED_STARTUPINFO_PRESENT|CREATE_UNICODE_ENVIRONMENT|CREATE_NO_WINDOW,
                               environment,workingDirectory,ref startup,out created)) Fail("CreateProcess in owned job");
            process=new SafeFileHandle(created.process,true); thread=new SafeFileHandle(created.thread,true);
            // Close the supervisor's copies of every child-side pipe immediately:
            // otherwise it would itself prevent EOF after all job members exit.
            inputRead.Dispose(); inputRead=null; outputWrite.Dispose(); outputWrite=null; errorWrite.Dispose(); errorWrite=null;
            thread.Dispose(); thread=null;
            // Keep stdin unbuffered: a completed WriteAsync must not leave a
            // buffered tail whose synchronous Close/Flush could bypass polling.
            input=new FileStream(inputWrite,FileAccess.Write,1,false); inputWrite=null;
            output=new FileStream(outputRead,FileAccess.Read,65536,false); outputRead=null;
            error=new FileStream(errorRead,FileAccess.Read,65536,false); errorRead=null;
            var result=new NativeProcess(job,process,checked((int)created.processId),input,output,error);
            success=true; return result;
        } finally {
            if(attributesReady) DeleteProcThreadAttributeList(attributes);
            if(attributes!=IntPtr.Zero) Marshal.FreeHGlobal(attributes);
            if(jobs!=IntPtr.Zero) Marshal.FreeHGlobal(jobs);
            if(handles!=IntPtr.Zero) Marshal.FreeHGlobal(handles);
            if(environment!=IntPtr.Zero) Marshal.FreeHGlobal(environment);
            if(!success) {
                // Job close terminates any successfully created process even if
                // managed stream construction fails after CreateProcess returns.
                job?.Dispose(); QuietDispose(input); QuietDispose(output); QuietDispose(error); process?.Dispose();
            }
            thread?.Dispose(); inputRead?.Dispose(); inputWrite?.Dispose();
            outputRead?.Dispose(); outputWrite?.Dispose(); errorRead?.Dispose(); errorWrite?.Dispose();
        }
    }
    static void QuietDispose(IDisposable value) { try { value?.Dispose(); } catch(IOException) {} }
    static void Pipe(out SafeFileHandle read, out SafeFileHandle write) {
        var security=new SecurityAttributes { length=Marshal.SizeOf<SecurityAttributes>(), inherit=1 };
        if(!CreatePipe(out read,out write,ref security,65536)) Fail("CreatePipe");
    }
    static void Uninherit(SafeFileHandle handle) {
        if(!SetHandleInformation(handle,HANDLE_FLAG_INHERIT,0)) Fail("SetHandleInformation");
    }
    static void RequireHandle(SafeFileHandle handle,string operation) {
        if(handle==null || handle.IsInvalid) Fail(operation);
    }
    static void Fail(string operation) { throw new Win32Exception(Marshal.GetLastWin32Error(),operation); }

    public bool HasExited { get { return WaitForExit(0); } }
    public bool WaitForExit(int milliseconds) {
        Check(); if(milliseconds<0) throw new ArgumentOutOfRangeException(nameof(milliseconds),"Waits must be bounded.");
        uint result=WaitForSingleObject(process,(uint)milliseconds);
        if(result==WAIT_OBJECT_0) return true;
        if(result==WAIT_TIMEOUT) return false;
        Fail("WaitForSingleObject"); return false;
    }
    public int ExitCode {
        get {
            Check(); if(!HasExited) throw new InvalidOperationException("Process has not exited.");
            uint code; if(!GetExitCodeProcess(process,out code)) Fail("GetExitCodeProcess");
            return unchecked((int)code);
        }
    }
    public bool JobEmpty {
        get {
            Check(); BasicAccounting accounting;
            if(!QueryInformationJobObjectAccounting(job,1,out accounting,(uint)Marshal.SizeOf<BasicAccounting>(),IntPtr.Zero)) Fail("Query job accounting");
            if(accounting.activeProcesses!=0) return false;
            // Job accounting can become empty just before the process object
            // signals. Retain observed identities through bounded cleanup.
            foreach(var member in observedMembers.Values) {
                uint state=WaitForSingleObject(member,0);
                if(state==WAIT_TIMEOUT) return false;
                if(state!=WAIT_OBJECT_0) Fail("Wait for observed owned member");
            }
            return true;
        }
    }
    public long WorkingSetBytes {
        get {
            Check(); long total=0;
            foreach(uint pid in ProcessIds()) {
                SafeFileHandle member;
                if(!observedMembers.TryGetValue(pid,out member)) {
                    member=OpenProcess(0x1000|0x0010|0x00100000,false,pid);
                    if(member.IsInvalid) {
                        int code=Marshal.GetLastWin32Error();
                        member.Dispose();
                        if(code==87) continue; // Member exited between enumeration and open.
                        throw new Win32Exception(code,"Open owned job member for memory observation");
                    }
                    bool belongs;
                    if(!IsProcessInJob(member,job,out belongs)) {
                        int code=Marshal.GetLastWin32Error();
                        bool exited=WaitForSingleObject(member,0)==WAIT_OBJECT_0;
                        member.Dispose();
                        if(exited) continue;
                        throw new Win32Exception(code,"Verify observed job membership");
                    }
                    if(!belongs) { member.Dispose(); continue; } // Recycled PID after snapshot.
                    if(observedMembers.Count>=65536) {
                        member.Dispose(); throw new InvalidOperationException("Owned member observation exceeds bounded capacity.");
                    }
                    observedMembers.Add(pid,member);
                }
                var counters=new ProcessMemoryCounters(); counters.cb=(uint)Marshal.SizeOf<ProcessMemoryCounters>();
                if(!GetProcessMemoryInfo(member,ref counters,counters.cb)) {
                    if(WaitForSingleObject(member,0)==WAIT_OBJECT_0) continue;
                    Fail("Get owned job memory");
                }
                total=checked(total+(long)counters.workingSet.ToUInt64());
            }
            long old;
            do { old=Interlocked.Read(ref peakWorkingSet); if(total<=old) break; }
            while(Interlocked.CompareExchange(ref peakWorkingSet,total,old)!=old);
            return total;
        }
    }
    // Observed peak of the summed live job-member working sets; this is not a
    // claim to have sampled each process's lifetime maximum simultaneously.
    public long PeakWorkingSetBytes { get { long current=WorkingSetBytes; return Interlocked.Read(ref peakWorkingSet); } }
    // Kernel-reported PeakJobMemoryUsed. Preserve this raw high-water reading;
    // rejected commitment/accounting behavior is investigated separately. Never
    // substitute working set or silently subtract a denied allocation from it.
    public long PeakJobCommittedBytes {
        get {
            Check(); ExtendedLimits limits;
            if(!QueryInformationJobObjectLimits(job,9,out limits,(uint)Marshal.SizeOf<ExtendedLimits>(),IntPtr.Zero)) Fail("Query committed job memory");
            return checked((long)limits.peakJobMemory.ToUInt64());
        }
    }
    public long[] CommittedMemoryLimitSnapshot {
        get {
            Check();
            if(IntPtr.Size!=8 || Marshal.SizeOf<ExtendedLimits>()!=144 ||
               Marshal.OffsetOf<ExtendedLimits>("jobMemory").ToInt64()!=120 ||
               Marshal.OffsetOf<ExtendedLimits>("peakJobMemory").ToInt64()!=136)
                throw new PlatformNotSupportedException("Audited x64 job-memory ABI required.");
            ExtendedLimits limits;
            if(!QueryInformationJobObjectLimits(job,9,out limits,(uint)Marshal.SizeOf<ExtendedLimits>(),IntPtr.Zero)) Fail("Query held job limits");
            return new long[] { limits.Basic.LimitFlags, limits.Basic.activeProcessLimit,
                checked((long)limits.jobMemory.ToUInt64()), checked((long)limits.peakJobMemory.ToUInt64()) };
        }
    }
    public uint[] MemberIds { get { Check(); return ProcessIds(); } }
    public uint ActiveProcesses {
        get {
            Check(); BasicAccounting accounting;
            if(!QueryInformationJobObjectAccounting(job,1,out accounting,(uint)Marshal.SizeOf<BasicAccounting>(),IntPtr.Zero)) Fail("Query job accounting");
            return accounting.activeProcesses;
        }
    }
    uint[] ProcessIds() {
        for(int capacity=16;capacity<=65536;capacity*=2) {
            int bytes=checked(8+capacity*IntPtr.Size); IntPtr buffer=Marshal.AllocHGlobal(bytes);
            try {
                if(!QueryInformationJobObject(job,3,buffer,(uint)bytes,IntPtr.Zero)) {
                    int code=Marshal.GetLastWin32Error(); if(code==ERROR_MORE_DATA) continue;
                    throw new Win32Exception(code,"Query owned job process list");
                }
                int assigned=Marshal.ReadInt32(buffer,0), count=Marshal.ReadInt32(buffer,4);
                if(assigned<0 || count<0 || count>capacity) throw new InvalidDataException("Invalid job-member count.");
                if(assigned>count) continue;
                var result=new uint[count];
                for(int i=0;i<count;i++) result[i]=checked((uint)Marshal.ReadIntPtr(buffer,8+i*IntPtr.Size).ToInt64());
                return result;
            } finally { Marshal.FreeHGlobal(buffer); }
        }
        throw new InvalidOperationException("Owned process count exceeds bounded observation capacity.");
    }
    public void Kill() { Check(); if(!TerminateJobObject(job,0xC000013A)) Fail("Terminate owned job"); }
    public void Dispose() {
        if(disposed) return; disposed=true;
        // Close the non-inherited, final owned job handle first. Descendants
        // cannot keep either job lifetime or redirected pipe ownership alive.
        job.Dispose(); QuietDispose(Input); QuietDispose(Output); QuietDispose(Error); process.Dispose();
        foreach(var member in observedMembers.Values) member.Dispose();
        observedMembers.Clear();
    }

    [StructLayout(LayoutKind.Sequential)] struct SecurityAttributes { public int length; public IntPtr descriptor; public int inherit; }
    [StructLayout(LayoutKind.Sequential)] struct StartupInfo {
        public int cb; public IntPtr reserved,desktop,title;
        public uint x,y,xSize,ySize,xChars,yChars,fillAttribute,flags;
        public ushort showWindow,reservedBytes; public IntPtr reserved2,stdin,stdout,stderr;
    }
    [StructLayout(LayoutKind.Sequential)] struct StartupInfoEx { public StartupInfo Startup; public IntPtr attributes; }
    [StructLayout(LayoutKind.Sequential)] struct ProcessInformation { public IntPtr process,thread; public uint processId,threadId; }
    [StructLayout(LayoutKind.Sequential)] struct BasicLimits {
        public long processTime,jobTime; public uint LimitFlags; public UIntPtr minimumWorkingSet,maximumWorkingSet;
        public uint activeProcessLimit; public UIntPtr affinity; public uint priorityClass,schedulingClass;
    }
    [StructLayout(LayoutKind.Sequential)] struct IoCounters { public ulong readOperations,writeOperations,otherOperations,readBytes,writeBytes,otherBytes; }
    [StructLayout(LayoutKind.Sequential)] struct ExtendedLimits { public BasicLimits Basic; public IoCounters io; public UIntPtr processMemory,jobMemory,peakProcessMemory,peakJobMemory; }
    [StructLayout(LayoutKind.Sequential)] struct BasicAccounting {
        public long userTime,kernelTime,periodUserTime,periodKernelTime;
        public uint pageFaults,totalProcesses,activeProcesses,terminatedProcesses;
    }
    [StructLayout(LayoutKind.Sequential)] struct ProcessMemoryCounters {
        public uint cb,pageFaults; public UIntPtr peakWorkingSet,workingSet,peakPagedPool,pagedPool,peakNonPagedPool,nonPagedPool,pagefile,peakPagefile;
    }
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern SafeFileHandle CreateJobObjectW(IntPtr attributes,string name);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern SafeFileHandle CreateFileW(string path,uint access,uint sharing,IntPtr security,uint creation,uint flags,IntPtr template);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetFileSizeEx(SafeFileHandle file,out long bytes);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool SetInformationJobObject(SafeFileHandle job,int kind,ref ExtendedLimits value,uint bytes);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool CreatePipe(out SafeFileHandle read,out SafeFileHandle write,ref SecurityAttributes attributes,uint size);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool SetHandleInformation(SafeFileHandle handle,uint mask,uint flags);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool InitializeProcThreadAttributeList(IntPtr list,int count,uint flags,ref IntPtr bytes);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool UpdateProcThreadAttribute(IntPtr list,uint flags,IntPtr attribute,IntPtr value,IntPtr bytes,IntPtr previous,IntPtr returned);
    [DllImport("kernel32.dll")] static extern void DeleteProcThreadAttributeList(IntPtr list);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool CreateProcessW(string application,StringBuilder command,IntPtr processAttributes,IntPtr threadAttributes,bool inherit,uint flags,IntPtr environment,string directory,ref StartupInfoEx startup,out ProcessInformation process);
    [DllImport("kernel32.dll",SetLastError=true)] static extern uint WaitForSingleObject(SafeFileHandle handle,uint milliseconds);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool GetExitCodeProcess(SafeFileHandle process,out uint code);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool QueryInformationJobObject(SafeFileHandle job,int kind,IntPtr value,uint bytes,IntPtr returned);
    [DllImport("kernel32.dll",EntryPoint="QueryInformationJobObject",SetLastError=true)] static extern bool QueryInformationJobObjectAccounting(SafeFileHandle job,int kind,out BasicAccounting value,uint bytes,IntPtr returned);
    [DllImport("kernel32.dll",EntryPoint="QueryInformationJobObject",SetLastError=true)] static extern bool QueryInformationJobObjectLimits(SafeFileHandle job,int kind,out ExtendedLimits value,uint bytes,IntPtr returned);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool TerminateJobObject(SafeFileHandle job,uint code);
    [DllImport("kernel32.dll",SetLastError=true)] static extern SafeFileHandle OpenProcess(uint access,bool inherit,uint processId);
    [DllImport("kernel32.dll",SetLastError=true)] static extern bool IsProcessInJob(SafeFileHandle process,SafeFileHandle job,out bool result);
    [DllImport("psapi.dll",SetLastError=true)] static extern bool GetProcessMemoryInfo(SafeFileHandle process,ref ProcessMemoryCounters counters,uint bytes);
}
}
