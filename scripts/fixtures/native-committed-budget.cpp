// Original GreenLens engineering fixture, Apache-2.0.
// Build x64 /MT outside the checkout. No simulation engine or runtime child.
// Original pressure modes after failed commitment: fixed pre-touched mapped
// fields, then an existing wait handle. The accounting mode adds pre-warmed
// read-only kernel queries. No JSON, heap allocation, file output or CRT cleanup.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#define PSAPI_VERSION 2
#include <windows.h>
#include <psapi.h>
#include <stddef.h>
#include <stdint.h>
#ifndef _WIN64
#error Native x64 is required.
#endif
constexpr SIZE_T MiB = 1024u * 1024u;
constexpr SIZE_T MappingBytes = 4096u;
constexpr uint32_t Magic = 0x474c4244u;
enum State : LONG { Started=1, WaitingChild=2, Completed=3, Failed=4 };
enum Mode : uint32_t { Single=1, AggregateParent=2, AggregateChild=3, ProcessCap=4, ParkChild=5, AccountingDiscriminator=6 };
struct alignas(8) Status {
    uint32_t magic, version, pid, role, mode;
    volatile LONG state;
    uint32_t failure, child_pid;
    uint64_t committed_bytes, attempted_bytes;
    uint32_t allocation_denied, child_created, child_create_error, child_wait_result;
    uint32_t allocation_error, reserved[15];
};
static_assert(sizeof(Status)==128, "Published slot size");
static_assert(offsetof(Status,state)==20 && offsetof(Status,committed_bytes)==32 &&
              offsetof(Status,allocation_error)==64, "Published field offsets");
// Separate, pre-touched diagnostic telemetry. The original four regression
// modes and their strict peak criterion are unchanged. No rejected request is
// treated as accepted memory, and the diagnostic cannot qualify containment.
struct alignas(8) AccountingSnapshot {
    uint32_t flags, active_limit;
    uint64_t job_limit, peak_job, own_private, own_peak_private, region_size;
    uint32_t region_state, error;
};
struct alignas(8) AccountingProbe {
    uint32_t magic, version;
    volatile LONG command, stage;
    uint64_t region_base, initial_bytes, attempt_bytes, attempt_address;
    AccountingSnapshot snapshots[3];
    uint32_t attempt_succeeded, attempt_error, failure, reserved[7];
};
static_assert(sizeof(AccountingSnapshot)==56 && sizeof(AccountingProbe)==256,
              "Diagnostic telemetry size");
static_assert(offsetof(AccountingProbe,snapshots)==48 &&
              offsetof(AccountingProbe,attempt_succeeded)==216, "Diagnostic offsets");
static_assert(sizeof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION)==144 &&
              offsetof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION,JobMemoryLimit)==120 &&
              offsetof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION,PeakJobMemoryUsed)==136,
              "Native x64 job limit ABI");
static wchar_t image[32768], command[32768];
static HANDLE park_event;
[[noreturn]] static void Park() { for (;;) WaitForSingleObject(park_event, INFINITE); }
[[noreturn]] static void Finish(Status* s, State state, DWORD failure=0) {
    s->failure=failure; InterlockedExchange(&s->state,state); Park();
}
static Status* OpenStatus(uint32_t slot, Mode mode) {
    HANDLE file=CreateFileW(L"status.bin",GENERIC_READ|GENERIC_WRITE,FILE_SHARE_READ|FILE_SHARE_WRITE,
        nullptr,OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL,nullptr);
    if(file==INVALID_HANDLE_VALUE) ExitProcess(101);
    LARGE_INTEGER bytes;
    if(!GetFileSizeEx(file,&bytes) || bytes.QuadPart!=static_cast<LONGLONG>(MappingBytes)) ExitProcess(102);
    HANDLE mapping=CreateFileMappingW(file,nullptr,PAGE_READWRITE,0,static_cast<DWORD>(MappingBytes),nullptr);
    if(!mapping) ExitProcess(103);
    auto base=static_cast<Status*>(MapViewOfFile(mapping,FILE_MAP_READ|FILE_MAP_WRITE,0,0,MappingBytes));
    if(!base) ExitProcess(104);
    Status* s=base+slot;
    volatile unsigned char* touched=reinterpret_cast<volatile unsigned char*>(s);
    for(SIZE_T i=0;i<sizeof(Status);++i) touched[i]=0;
    s->magic=Magic; s->version=1; s->pid=GetCurrentProcessId(); s->role=slot; s->mode=mode;
    park_event=CreateEventW(nullptr,TRUE,FALSE,nullptr);
    if(!park_event) ExitProcess(105);
    WaitForSingleObject(park_event,0); // Warm the same wait/API path before pressure.
    InterlockedExchange(&s->state,Started);
    // Retain mapping/file/event handles until supervised job termination.
    return s;
}
static void* Reserve(Status* s) {
    void* base=VirtualAlloc(nullptr,512u*MiB,MEM_RESERVE,PAGE_READWRITE);
    if(!base) Finish(s,Failed,GetLastError());
    return base;
}
static bool Commit(Status* s,void* address,SIZE_T bytes) {
    s->attempted_bytes=bytes;
    if(!VirtualAlloc(address,bytes,MEM_COMMIT,PAGE_READWRITE)) {
        s->allocation_error=GetLastError(); s->allocation_denied=1; return false;
    }
    s->committed_bytes+=bytes; return true;
}
static bool SnapshotAccounting(AccountingSnapshot* out,void* address) {
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits{};
    PROCESS_MEMORY_COUNTERS_EX memory{};
    memory.cb=static_cast<DWORD>(sizeof(memory));
    MEMORY_BASIC_INFORMATION region{};
    if(!QueryInformationJobObject(nullptr,JobObjectExtendedLimitInformation,
            &limits,static_cast<DWORD>(sizeof(limits)),nullptr) ||
       !K32GetProcessMemoryInfo(GetCurrentProcess(),
            reinterpret_cast<PROCESS_MEMORY_COUNTERS*>(&memory),memory.cb) ||
       !VirtualQuery(address,&region,sizeof(region))) {
        out->error=GetLastError(); return false;
    }
    out->flags=limits.BasicLimitInformation.LimitFlags;
    out->active_limit=limits.BasicLimitInformation.ActiveProcessLimit;
    out->job_limit=limits.JobMemoryLimit;
    out->peak_job=limits.PeakJobMemoryUsed;
    out->own_private=memory.PrivateUsage;
    out->own_peak_private=memory.PeakPagefileUsage;
    out->region_size=region.RegionSize; out->region_state=region.State;
    return true;
}
[[noreturn]] static void DiagnoseAccounting(Status* s,unsigned char* base) {
    auto probe=reinterpret_cast<AccountingProbe*>(reinterpret_cast<unsigned char*>(s)+256);
    volatile unsigned char* touched=reinterpret_cast<volatile unsigned char*>(probe);
    for(SIZE_T i=0;i<sizeof(AccountingProbe);++i) touched[i]=0;
    probe->magic=Magic; probe->version=1; probe->region_base=reinterpret_cast<uint64_t>(base);
    probe->initial_bytes=256u*MiB; probe->attempt_bytes=160u*MiB;
    auto address=base+256u*MiB; probe->attempt_address=reinterpret_cast<uint64_t>(address);
    // All three query paths and fixed stack buffers are warmed before pressure.
    if(!SnapshotAccounting(&probe->snapshots[0],address)) Finish(s,Failed,probe->snapshots[0].error);
    InterlockedExchange(&probe->stage,1);
    while(InterlockedCompareExchange(&probe->command,0,0)<1) WaitForSingleObject(park_event,20);
    if(!Commit(s,base,256u*MiB)) Finish(s,Failed,s->allocation_error);
    if(!SnapshotAccounting(&probe->snapshots[1],address)) Finish(s,Failed,probe->snapshots[1].error);
    InterlockedExchange(&probe->stage,2);
    while(InterlockedCompareExchange(&probe->command,0,0)<2) WaitForSingleObject(park_event,20);
    const bool accepted=Commit(s,address,160u*MiB);
    probe->attempt_succeeded=accepted?1u:0u; probe->attempt_error=s->allocation_error;
    if(!SnapshotAccounting(&probe->snapshots[2],address)) Finish(s,Failed,probe->snapshots[2].error);
    InterlockedExchange(&probe->stage,3);
    Finish(s,Completed);
}
static void Spawn(Status* s,const wchar_t* mode) {
    DWORD length=GetModuleFileNameW(nullptr,image,32768);
    if(!length || length>=32768) Finish(s,Failed,ERROR_FILENAME_EXCED_RANGE);
    SIZE_T count=0; command[count++]=L'"';
    for(DWORD i=0;i<length;++i) {
        if(image[i]==L'"' || count>=32700) Finish(s,Failed,ERROR_INVALID_NAME);
        command[count++]=image[i];
    }
    command[count++]=L'"'; command[count++]=L' ';
    for(SIZE_T i=0;mode[i];++i) {
        if(count>=32766) Finish(s,Failed,ERROR_FILENAME_EXCED_RANGE);
        command[count++]=mode[i];
    }
    command[count]=0;
    STARTUPINFOW startup{}; startup.cb=static_cast<DWORD>(sizeof(startup));
    PROCESS_INFORMATION child{};
    InterlockedExchange(&s->state,WaitingChild);
    const BOOL created=CreateProcessW(image,command,nullptr,nullptr,FALSE,
        CREATE_NO_WINDOW,nullptr,nullptr,&startup,&child);
    s->child_created=created?1u:0u;
    if(!created) { s->child_create_error=GetLastError(); return; }
    s->child_pid=child.dwProcessId; s->child_wait_result=WaitForSingleObject(child.hProcess,0);
    CloseHandle(child.hThread); CloseHandle(child.hProcess);
}
int wmain(int argc,wchar_t** argv) {
    if(argc!=2) return 100;
    Mode mode;
    if(lstrcmpW(argv[1],L"single")==0) mode=Single;
    else if(lstrcmpW(argv[1],L"aggregate")==0) mode=AggregateParent;
    else if(lstrcmpW(argv[1],L"aggregate-child")==0) mode=AggregateChild;
    else if(lstrcmpW(argv[1],L"process-cap")==0) mode=ProcessCap;
    else if(lstrcmpW(argv[1],L"park-child")==0) mode=ParkChild;
    else if(lstrcmpW(argv[1],L"accounting-discriminator")==0) mode=AccountingDiscriminator;
    else return 100;
    Status* s=OpenStatus(mode==AggregateChild || mode==ParkChild?1u:0u,mode);
    if(mode==ParkChild) Finish(s,Completed);
    if(mode==ProcessCap) { Spawn(s,L"park-child"); Finish(s,Completed); }
    auto base=static_cast<unsigned char*>(Reserve(s));
    if(mode==AccountingDiscriminator) DiagnoseAccounting(s,base);
    if(mode==Single) {
        for(SIZE_T i=0;i<512;++i) if(!Commit(s,base+i*MiB,MiB)) Finish(s,Completed);
        Finish(s,Failed,ERROR_INVALID_DATA);
    }
    if(!Commit(s,base,128u*MiB)) Finish(s,Failed,s->allocation_error);
    if(mode==AggregateParent) {
        Spawn(s,L"aggregate-child");
        if(!s->child_created) Finish(s,Failed,s->child_create_error);
        Finish(s,Completed);
    }
    // Child alone requests 288 MiB (<384); parent+child request 416 MiB.
    // The otherwise-identical 512 MiB control must succeed.
    Commit(s,base+128u*MiB,160u*MiB); Finish(s,Completed);
}
