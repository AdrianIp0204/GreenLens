//! Bounded generated stdin for the compiled method-5 coefficient stages.
//!
//! Windows 10+ is required: JOB_LIST assigns the child before its first instruction.
//! Only compiled, trusted callers may select the executable or record iterator.
//! Each iterator step must be bounded CPU work, with no blocking I/O. This module
//! cannot preempt arbitrary Rust code; it checks the deadline before/after each step.
//!
//! The server end uses synchronous PIPE_NOWAIT byte writes, not asynchronous I/O:
//! WriteFile returns immediately with as many bytes as fit, including zero. Keep
//! the unwritten suffix and hash only bytes reported written. There is no writer
//! thread, outstanding OVERLAPPED allocation, FlushFileBuffers, or thread join.
//! See https://learn.microsoft.com/windows/win32/ipc/named-pipe-type-read-and-wait-modes
//! and https://devblogs.microsoft.com/oldnewthing/20230209-00/?p=107812 .
use crate::annual_primary::STREAM_RESOURCE_SCHEMA_VERSION;
pub use crate::annual_primary::{InputReceipt, StreamTelemetry};
use crate::process::{self, ProcessRecord};
use anyhow::{bail, ensure, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs::{self, File},
    io::Read,
    mem::{size_of, size_of_val},
    os::windows::{ffi::OsStrExt, io::AsRawHandle},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::SECURITY_ATTRIBUTES,
    Storage::FileSystem::*,
    System::{JobObjects::*, Pipes::*, ProcessStatus::*, Threading::*},
};

const BUFFER_BYTES: usize = 64 * 1024;
pub const MAX_RECORD_BYTES: usize = 1024;
pub const MAX_RECORDS: u64 = 256 * 262_144;
const POLL: Duration = Duration::from_millis(20);
const CLEANUP: Duration = Duration::from_secs(3);
static PIPE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Limits never exceed the existing annual file/scratch/working-set budgets.
/// Input is a stream, not a retained file; its separate finite bound is required.
#[derive(Clone, Debug)]
pub struct StreamLimits {
    pub deadline: Duration,
    pub max_input_bytes: u64,
    pub max_file_bytes: u64,
    pub max_scratch_bytes: u64,
    pub max_observed_working_set_bytes: u64,
}
impl Default for StreamLimits {
    fn default() -> Self {
        Self {
            deadline: Duration::from_secs(7200),
            max_input_bytes: MAX_RECORDS * MAX_RECORD_BYTES as u64,
            max_file_bytes: process::ANNUAL_MAX_FILE_BYTES,
            max_scratch_bytes: process::ANNUAL_MAX_SCRATCH_BYTES,
            max_observed_working_set_bytes: process::ANNUAL_MAX_OBSERVED_WORKING_SET_BYTES,
        }
    }
}
#[derive(Debug)]
pub struct StreamRequest {
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub directory: PathBuf,
    /// Applied only to this child's inherited environment; never recorded wholesale.
    pub child_environment: BTreeMap<String, String>,
    pub expected_records: u64,
    pub limits: StreamLimits,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamOutcome {
    pub process: ProcessRecord,
    pub input: InputReceipt,
    pub telemetry: StreamTelemetry,
}

struct Handle(HANDLE);
impl Handle {
    fn new(raw: HANDLE, operation: &str) -> Result<Self> {
        ensure!(
            !raw.is_null() && raw != INVALID_HANDLE_VALUE,
            "{operation}: {}",
            std::io::Error::last_os_error()
        );
        Ok(Self(raw))
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
fn check(ok: i32, operation: &str) -> Result<()> {
    ensure!(ok != 0, "{operation}: {}", std::io::Error::last_os_error());
    Ok(())
}
fn wide(value: &OsStr) -> Result<Vec<u16>> {
    let mut result: Vec<_> = value.encode_wide().collect();
    ensure!(!result.contains(&0), "Windows parameter contains NUL");
    result.push(0);
    Ok(result)
}
fn quote(value: &str) -> Result<String> {
    ensure!(!value.contains('\0'), "Windows argument contains NUL");
    let mut result = String::from("\"");
    let mut slashes = 0;
    for c in value.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        result.extend(std::iter::repeat_n(
            '\\',
            if c == '"' { slashes * 2 + 1 } else { slashes },
        ));
        result.push(c);
        slashes = 0;
    }
    result.extend(std::iter::repeat_n('\\', slashes * 2));
    result.push('"');
    Ok(result)
}
struct Attributes {
    storage: Vec<usize>,
    initialized: bool,
}
impl Attributes {
    fn new() -> Result<Self> {
        let mut bytes = 0;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut bytes);
        }
        ensure!(bytes > 0, "Cannot size process attribute list");
        let mut result = Self {
            storage: vec![0; bytes.div_ceil(size_of::<usize>())],
            initialized: false,
        };
        check(
            unsafe { InitializeProcThreadAttributeList(result.ptr(), 2, 0, &mut bytes) },
            "Initialize process attributes",
        )?;
        result.initialized = true;
        Ok(result)
    }
    fn ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.storage.as_mut_ptr().cast()
    }
    fn set(&mut self, attribute: u32, values: &[HANDLE]) -> Result<()> {
        check(
            unsafe {
                UpdateProcThreadAttribute(
                    self.ptr(),
                    0,
                    attribute as usize,
                    values.as_ptr().cast(),
                    size_of_val(values),
                    null_mut(),
                    null(),
                )
            },
            "Set atomic job/handle attribute (Windows 10+ required)",
        )
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                DeleteProcThreadAttributeList(self.ptr());
            }
        }
    }
}

struct Child {
    job: Handle,
    process: Handle,
    input: Option<Handle>,
    pid: u32,
    observed_members: BTreeMap<u32, Handle>,
}
impl Child {
    fn start(request: &StreamRequest, working_directory: &Path) -> Result<Self> {
        let exe = wide(request.executable.as_os_str())?;
        let cwd = wide(working_directory.as_os_str())?;
        let mut command = quote(
            request
                .executable
                .to_str()
                .context("Executable path is not Unicode")?,
        )?;
        for argument in &request.arguments {
            command.push(' ');
            command.push_str(&quote(argument)?);
        }
        let mut command = wide(OsStr::new(&command))?;
        ensure!(command.len() <= 32767, "Windows command line exceeds limit");
        let mut env: BTreeMap<String, (String, String)> = BTreeMap::new();
        for (key, value) in std::env::vars_os() {
            let key = key
                .into_string()
                .map_err(|_| anyhow::anyhow!("Non-Unicode environment key"))?;
            let value = value
                .into_string()
                .map_err(|_| anyhow::anyhow!("Non-Unicode environment value"))?;
            env.insert(key.to_uppercase(), (key, value));
        }
        for (key, value) in &request.child_environment {
            ensure!(
                !key.is_empty() && !key.contains(['=', '\0']) && !value.contains('\0'),
                "Invalid child environment override"
            );
            env.insert(key.to_uppercase(), (key.clone(), value.clone()));
        }
        let mut environment = Vec::new();
        for (_, (key, value)) in env {
            environment.extend(wide(OsStr::new(&format!("{key}={value}")))?);
        }
        environment.push(0);
        let job = Handle::new(
            unsafe { CreateJobObjectW(null(), null()) },
            "Create owned job",
        )?;
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        check(
            unsafe {
                SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    size_of_val(&limits) as u32,
                )
            },
            "Set owned job limits",
        )?;
        let name = format!(
            r"\\.\pipe\GreenLens-stream-{}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default(),
            PIPE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        let name = wide(OsStr::new(&name))?;
        let input = Handle::new(
            unsafe {
                CreateNamedPipeW(
                    name.as_ptr(),
                    PIPE_ACCESS_OUTBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
                    1,
                    BUFFER_BYTES as u32,
                    0,
                    0,
                    null(),
                )
            },
            "Create nonblocking local input",
        )?;
        let security = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: null_mut(),
            bInheritHandle: 1,
        };
        // Client end remains a normal blocking byte reader, suitable for native stdin.
        let read = Handle::new(
            unsafe {
                CreateFileW(
                    name.as_ptr(),
                    GENERIC_READ,
                    0,
                    &security,
                    OPEN_EXISTING,
                    0,
                    null_mut(),
                )
            },
            "Connect child input",
        )?;
        let stdout = File::create(request.directory.join("stream-stdout.bin"))?;
        let stderr = File::create(request.directory.join("stream-stderr.txt"))?;
        for handle in [stdout.as_raw_handle(), stderr.as_raw_handle()] {
            check(
                unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) },
                "Set child output inheritance",
            )?;
        }
        let jobs = [job.0];
        let handles = [read.0, stdout.as_raw_handle(), stderr.as_raw_handle()];
        // Attribute values must outlive DeleteProcThreadAttributeList as well
        // as CreateProcess. Locals are dropped in reverse declaration order.
        let mut attributes = Attributes::new()?;
        attributes.set(PROC_THREAD_ATTRIBUTE_JOB_LIST, &jobs)?;
        attributes.set(PROC_THREAD_ATTRIBUTE_HANDLE_LIST, &handles)?;
        let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = read.0;
        startup.StartupInfo.hStdOutput = stdout.as_raw_handle();
        startup.StartupInfo.hStdError = stderr.as_raw_handle();
        startup.lpAttributeList = attributes.ptr();
        let mut created: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        check(
            unsafe {
                CreateProcessW(
                    exe.as_ptr(),
                    command.as_mut_ptr(),
                    null(),
                    null(),
                    1,
                    EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW,
                    environment.as_ptr().cast(),
                    cwd.as_ptr(),
                    &startup.StartupInfo,
                    &mut created,
                )
            },
            "Launch in atomic owned job (Windows 10+ required)",
        )?;
        let process = Handle::new(created.hProcess, "Created process")?;
        let _thread = Handle::new(created.hThread, "Created thread")?;
        // Drop all parent copies of the child's three handles before feeding input.
        drop((read, stdout, stderr));
        Ok(Self {
            job,
            process,
            input: Some(input),
            pid: created.dwProcessId,
            observed_members: BTreeMap::new(),
        })
    }
    fn exit_code(&self) -> Result<Option<i32>> {
        let state = unsafe { WaitForSingleObject(self.process.0, 0) };
        if state == WAIT_TIMEOUT {
            return Ok(None);
        }
        ensure!(state == WAIT_OBJECT_0, "Cannot query child exit");
        let mut code = 0;
        check(
            unsafe { GetExitCodeProcess(self.process.0, &mut code) },
            "Read child exit",
        )?;
        Ok(Some(code as i32))
    }
    fn empty(&self) -> Result<bool> {
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { std::mem::zeroed() };
        check(
            unsafe {
                QueryInformationJobObject(
                    self.job.0,
                    JobObjectBasicAccountingInformation,
                    (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                    size_of_val(&info) as u32,
                    null_mut(),
                )
            },
            "Read job accounting",
        )?;
        Ok(info.ActiveProcesses == 0)
    }
    fn observed_working_set(&mut self) -> Result<u64> {
        // Bounded job list; an unexpected process fanout fails supervision.
        let mut list = vec![0usize; 2 + 1024];
        check(
            unsafe {
                QueryInformationJobObject(
                    self.job.0,
                    JobObjectBasicProcessIdList,
                    list.as_mut_ptr().cast(),
                    (list.len() * size_of::<usize>()) as u32,
                    null_mut(),
                )
            },
            "Read owned process list",
        )?;
        let raw = list.as_ptr().cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>();
        let count = unsafe { (*raw).NumberOfProcessIdsInList } as usize;
        ensure!(
            count <= 1024 && unsafe { (*raw).NumberOfAssignedProcesses } as usize <= count,
            "Owned process list exceeds bound"
        );
        let ids = unsafe { std::slice::from_raw_parts((*raw).ProcessIdList.as_ptr(), count) };
        let mut total = process_working_set(unsafe { GetCurrentProcess() })?;
        for &id in ids {
            let handle = unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ | SYNCHRONIZE,
                    0,
                    id as u32,
                )
            };
            if handle.is_null() {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() == Some(ERROR_INVALID_PARAMETER as i32) {
                    continue;
                }
                return Err(error.into());
            }
            let member = Handle(handle);
            let mut belongs = 0;
            check(
                unsafe { IsProcessInJob(member.0, self.job.0, &mut belongs) },
                "Check owned PID identity",
            )?;
            if belongs == 0 {
                continue;
            }
            match process_working_set(member.0) {
                Ok(bytes) => total = total.checked_add(bytes).context("Working-set overflow")?,
                Err(_) if unsafe { WaitForSingleObject(member.0, 0) } == WAIT_OBJECT_0 => {}
                Err(error) => return Err(error),
            }
            self.observed_members.insert(id as u32, member);
        }
        Ok(total)
    }
    fn all_observed_exited(&self) -> Result<bool> {
        for member in self.observed_members.values() {
            match unsafe { WaitForSingleObject(member.0, 0) } {
                WAIT_OBJECT_0 => {}
                WAIT_TIMEOUT => return Ok(false),
                _ => bail!("Cannot query observed owned process completion"),
            }
        }
        Ok(true)
    }
    fn stop(&mut self) -> Result<()> {
        check(
            unsafe { TerminateJobObject(self.job.0, 0xC000013A) },
            "Terminate owned streaming job",
        )?;
        self.input.take();
        let until = Instant::now() + CLEANUP;
        while Instant::now() < until {
            if self.empty()? && self.exit_code()?.is_some() && self.all_observed_exited()? {
                return Ok(());
            }
            thread::sleep(POLL);
        }
        bail!("Owned streaming job did not become empty within bounded cleanup")
    }
}
fn process_working_set(handle: HANDLE) -> Result<u64> {
    Ok(process_memory(handle)?.WorkingSetSize as u64)
}
fn process_memory(handle: HANDLE) -> Result<PROCESS_MEMORY_COUNTERS> {
    let mut counters: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    counters.cb = size_of_val(&counters) as u32;
    check(
        unsafe { GetProcessMemoryInfo(handle, &mut counters, size_of_val(&counters) as u32) },
        "Measure process working set",
    )?;
    Ok(counters)
}

/// Execute one continuous, finite record stream. Iterator calls must be bounded
/// and trusted; caller records are opaque bytes, with no numerical policy here.
/// Output and failure diagnostics live only in the caller's owned scratch dir.
pub fn execute<I>(request: &StreamRequest, records: I, cancel: &AtomicBool) -> Result<StreamOutcome>
where
    I: Iterator<Item = Result<Vec<u8>>>,
{
    execute_scoped(request, records, cancel, &request.directory)
}

/// Production stage diagnostics stay below the owned run root; the child and
/// resource scanner use the whole root. No sibling scratch escapes the budget.
pub fn execute_in_run<I>(
    request: &StreamRequest,
    records: I,
    cancel: &AtomicBool,
    run_root: &Path,
) -> Result<StreamOutcome>
where
    I: Iterator<Item = Result<Vec<u8>>>,
{
    ensure!(
        run_root.is_absolute() && run_root.is_dir(),
        "Run root must be an existing absolute directory"
    );
    for path in [run_root, request.directory.as_path()] {
        for ancestor in path.ancestors() {
            let metadata = fs::symlink_metadata(ancestor)?;
            use std::os::windows::fs::MetadataExt;
            ensure!(
                !metadata.file_type().is_symlink() && metadata.file_attributes() & 0x400 == 0,
                "Run/stage path must not traverse a reparse point"
            );
        }
    }
    let root = fs::canonicalize(run_root)?;
    let stage = fs::canonicalize(&request.directory)?;
    ensure!(
        stage != root && stage.starts_with(&root),
        "Stage must be strictly below the owned run root"
    );
    execute_scoped(request, records, cancel, run_root)
}

fn execute_scoped<I>(
    request: &StreamRequest,
    mut records: I,
    cancel: &AtomicBool,
    run_root: &Path,
) -> Result<StreamOutcome>
where
    I: Iterator<Item = Result<Vec<u8>>>,
{
    let limits = &request.limits;
    ensure!(
        (1..=MAX_RECORDS).contains(&request.expected_records),
        "Invalid expected input record count"
    );
    ensure!(
        !limits.deadline.is_zero() && limits.deadline <= Duration::from_secs(24 * 3600),
        "Invalid streaming deadline"
    );
    ensure!(
        limits.max_input_bytes > 0
            && limits.max_input_bytes <= MAX_RECORDS * MAX_RECORD_BYTES as u64,
        "Invalid streaming input bound"
    );
    ensure!(
        limits.max_file_bytes > 0
            && limits.max_file_bytes <= process::ANNUAL_MAX_FILE_BYTES
            && limits.max_scratch_bytes > 0
            && limits.max_scratch_bytes <= process::ANNUAL_MAX_SCRATCH_BYTES
            && limits.max_observed_working_set_bytes > 0
            && limits.max_observed_working_set_bytes
                <= process::ANNUAL_MAX_OBSERVED_WORKING_SET_BYTES,
        "Invalid streaming resource bounds"
    );
    ensure!(
        request.executable.is_absolute()
            && request.executable.is_file()
            && request.directory.is_absolute()
            && request.directory.is_dir(),
        "Streaming requires existing absolute executable and owned scratch paths"
    );
    for name in [
        "stream-stdout.bin",
        "stream-stderr.txt",
        "stream-success.json",
        "stream-failure.json",
    ] {
        ensure!(
            !request.directory.join(name).exists(),
            "Streaming diagnostics already exist; use a fresh owned execution directory"
        );
    }
    let start = Instant::now();
    let started_utc = Utc::now().to_rfc3339();
    let check_deadline = || -> Result<()> {
        ensure!(!cancel.load(Ordering::SeqCst), "Streaming cancelled");
        ensure!(
            start.elapsed() < limits.deadline,
            "Streaming deadline exceeded"
        );
        Ok(())
    };
    check_deadline()?;
    let mut child = Child::start(request, run_root)?;
    let mut telemetry = StreamTelemetry {
        schema_version: STREAM_RESOURCE_SCHEMA_VERSION.into(),
        sampled_peak_job_plus_runner_working_set_bytes: 0,
        peak_child_working_set_bytes: 0,
        sampled_peak_runner_plus_child_working_set_bytes: 0,
        peak_scratch_bytes: 0,
        samples: 0,
        zero_progress_writes: 0,
        partial_writes: 0,
        atomic_job_assignment: true,
    };
    let mut written = 0u64;
    let mut generated = 0u64;
    let mut hashed = Sha256::new();
    let result = (|| -> Result<i32> {
        let mut buffer = Vec::with_capacity(BUFFER_BYTES);
        let mut offset = 0;
        let mut eof = false;
        loop {
            check_deadline()?;
            let (scratch, largest) = process::scratch_sizes(run_root)?;
            let working_set = child.observed_working_set()?;
            let memory = process_memory(child.process.0)?;
            let child_combined = (memory.WorkingSetSize as u64)
                .checked_add(process_working_set(unsafe { GetCurrentProcess() })?)
                .context("Working set overflow")?;
            telemetry.peak_child_working_set_bytes = telemetry
                .peak_child_working_set_bytes
                .max(memory.PeakWorkingSetSize as u64);
            telemetry.sampled_peak_runner_plus_child_working_set_bytes = telemetry
                .sampled_peak_runner_plus_child_working_set_bytes
                .max(child_combined);
            check_deadline()?;
            telemetry.samples += 1;
            telemetry.peak_scratch_bytes = telemetry.peak_scratch_bytes.max(scratch);
            telemetry.sampled_peak_job_plus_runner_working_set_bytes = telemetry
                .sampled_peak_job_plus_runner_working_set_bytes
                .max(working_set);
            ensure!(
                largest <= limits.max_file_bytes,
                "Streaming file resource limit exceeded"
            );
            ensure!(
                scratch <= limits.max_scratch_bytes,
                "Streaming scratch resource limit exceeded"
            );
            ensure!(
                working_set <= limits.max_observed_working_set_bytes
                    && child_combined <= limits.max_observed_working_set_bytes
                    && telemetry.peak_child_working_set_bytes
                        <= limits.max_observed_working_set_bytes,
                "Streaming working-set resource limit exceeded"
            );
            ensure!(
                File::open(request.directory.join("stream-stderr.txt"))?
                    .metadata()?
                    .len()
                    <= 1024 * 1024,
                "Streaming diagnostic text exceeds bound"
            );
            let exited = child.exit_code()?;
            if let Some(code) = exited {
                ensure!(
                    eof && offset == buffer.len(),
                    "Child exited before complete input receipt"
                );
                ensure!(code == 0, "Streaming child exited {code}");
                if child.empty()? && child.all_observed_exited()? {
                    // The last live scan can precede the child's final writes.
                    // Recheck only after every owned writer has exited, inside
                    // the failure-handled closure so excess output is retained
                    // as a failed diagnostic rather than a successful receipt.
                    let (scratch, largest) = process::scratch_sizes(run_root)?;
                    telemetry.peak_scratch_bytes = telemetry.peak_scratch_bytes.max(scratch);
                    telemetry.peak_child_working_set_bytes = telemetry
                        .peak_child_working_set_bytes
                        .max(process_memory(child.process.0)?.PeakWorkingSetSize as u64);
                    ensure!(
                        telemetry.peak_child_working_set_bytes
                            <= limits.max_observed_working_set_bytes,
                        "Streaming final child working-set resource limit exceeded"
                    );
                    ensure!(
                        largest <= limits.max_file_bytes,
                        "Streaming final file resource limit exceeded"
                    );
                    ensure!(
                        scratch <= limits.max_scratch_bytes,
                        "Streaming final scratch resource limit exceeded"
                    );
                    ensure!(
                        File::open(request.directory.join("stream-stderr.txt"))?
                            .metadata()?
                            .len()
                            <= 1024 * 1024,
                        "Streaming final diagnostic text exceeds bound"
                    );
                    check_deadline()?;
                    return Ok(code);
                }
            }
            if offset == buffer.len() && !eof {
                buffer.clear();
                offset = 0;
                while buffer.len() <= BUFFER_BYTES - MAX_RECORD_BYTES {
                    check_deadline()?;
                    let next = records.next();
                    check_deadline()?;
                    let Some(record) = next else {
                        ensure!(
                            generated == request.expected_records,
                            "Truncated generated input records"
                        );
                        eof = true;
                        break;
                    };
                    let record = record.context("Input producer failed")?;
                    ensure!(
                        !record.is_empty() && record.len() <= MAX_RECORD_BYTES,
                        "Generated record size exceeds bound"
                    );
                    generated += 1;
                    ensure!(
                        generated <= request.expected_records,
                        "Excess generated input records"
                    );
                    ensure!(
                        written + buffer.len() as u64 + record.len() as u64
                            <= limits.max_input_bytes,
                        "Generated input byte bound exceeded"
                    );
                    buffer.extend_from_slice(&record);
                    if generated == request.expected_records {
                        check_deadline()?;
                        let excess = records.next();
                        check_deadline()?;
                        ensure!(
                            excess.transpose()?.is_none(),
                            "Excess generated input records"
                        );
                        eof = true;
                        break;
                    }
                }
            }
            if offset < buffer.len() {
                let input = child
                    .input
                    .as_ref()
                    .context("Streaming stdin closed prematurely")?;
                let mut count = 0;
                check(
                    unsafe {
                        WriteFile(
                            input.0,
                            buffer[offset..].as_ptr(),
                            (buffer.len() - offset) as u32,
                            &mut count,
                            null_mut(),
                        )
                    },
                    "Write nonblocking generated input",
                )?;
                let count = count as usize;
                ensure!(count <= buffer.len() - offset, "Invalid pipe write count");
                if count == 0 {
                    telemetry.zero_progress_writes += 1;
                } else if count < buffer.len() - offset {
                    telemetry.partial_writes += 1;
                }
                hashed.update(&buffer[offset..offset + count]);
                offset += count;
                written += count as u64;
            }
            if eof && offset == buffer.len() {
                child.input.take();
            }
            // Poll all lifetimes through the same deadline, including after parent exit.
            thread::sleep(POLL);
        }
    })();
    let exit_code = match result {
        Ok(code) => code,
        Err(error) => {
            let cleanup = child.stop();
            let diagnostic = serde_json::json!({"schema_version":"greenlens.stream-failure.v1","state":"failed","error":format!("{error:#}"),"pid":child.pid,"written_bytes":written,"generated_records":generated,"process_tree_terminated":cleanup.is_ok(),"cleanup_error":cleanup.as_ref().err().map(ToString::to_string),"telemetry":telemetry});
            fs::write(
                request.directory.join("stream-failure.json"),
                serde_json::to_vec_pretty(&diagnostic)?,
            )?;
            cleanup.context("Streaming failure cleanup")?;
            return Err(error);
        }
    };
    check_deadline()?;
    let mut stderr = Vec::new();
    File::open(request.directory.join("stream-stderr.txt"))?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut stderr)?;
    ensure!(
        stderr.len() <= 1024 * 1024,
        "Streaming diagnostic text exceeds bound"
    );
    let mut stderr = String::from_utf8_lossy(&stderr).into_owned();
    for (path, label) in [
        (request.directory.as_path(), "<stage>"),
        (run_root, "<scratch>"),
        (request.executable.as_path(), "<executable>"),
    ] {
        stderr = stderr.replace(path.to_string_lossy().as_ref(), label);
    }
    let outcome = StreamOutcome {
        process: ProcessRecord {
            executable: request
                .executable
                .file_name()
                .context("Missing executable name")?
                .to_string_lossy()
                .into_owned(),
            arguments: request.arguments.clone(),
            exit_code,
            started_utc,
            finished_utc: Utc::now().to_rfc3339(),
            elapsed_ms: start.elapsed().as_millis(),
            stderr,
        },
        input: InputReceipt {
            bytes: written,
            records: generated,
            sha256: format!("{:x}", hashed.finalize()),
        },
        telemetry,
    };
    fs::write(
        request.directory.join("stream-success.json"),
        serde_json::to_vec_pretty(&outcome)?,
    )?;
    Ok(outcome)
}

#[cfg(test)]
#[path = "annual_stream_tests.rs"]
mod tests;
