//! Atomic Windows Job creation, adapted from GreenLens's retained stream supervisor.
use crate::NativeRequest;
use anyhow::{bail, ensure, Context, Result};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs::File,
    mem::{size_of, size_of_val},
    os::windows::{ffi::OsStrExt, io::AsRawHandle},
    path::Path,
    ptr::{null, null_mut},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::SECURITY_ATTRIBUTES,
    Storage::FileSystem::*,
    System::{JobObjects::*, ProcessStatus::*, Threading::*},
};
const CLEANUP: Duration = Duration::from_secs(3);
const POLL: Duration = Duration::from_millis(20);
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

pub(crate) struct Child {
    job: Handle,
    process: Handle,
    observed_members: BTreeMap<u32, Handle>,
}
impl Child {
    pub(crate) fn start(request: &NativeRequest, working_directory: &Path) -> Result<Self> {
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
        for key in &request.child_environment_remove {
            ensure!(
                !key.is_empty() && !key.contains(['=', '\0']),
                "Invalid child environment removal"
            );
            env.remove(&key.to_uppercase());
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
        let security = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: null_mut(),
            bInheritHandle: 1,
        };
        let name = wide(OsStr::new("NUL"))?;
        let read = Handle::new(
            unsafe {
                CreateFileW(
                    name.as_ptr(),
                    GENERIC_READ,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    &security,
                    OPEN_EXISTING,
                    0,
                    null_mut(),
                )
            },
            "Open empty child input",
        )?;
        let stdout = File::create(request.directory.join("process-stdout.txt"))?;
        let stderr = File::create(request.directory.join("process-stderr.txt"))?;
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
            observed_members: BTreeMap::new(),
        })
    }
    pub(crate) fn exit_code(&self) -> Result<Option<i32>> {
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
    pub(crate) fn empty(&self) -> Result<bool> {
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
    pub(crate) fn observed_working_set(&mut self) -> Result<u64> {
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
    pub(crate) fn all_observed_exited(&self) -> Result<bool> {
        for member in self.observed_members.values() {
            match unsafe { WaitForSingleObject(member.0, 0) } {
                WAIT_OBJECT_0 => {}
                WAIT_TIMEOUT => return Ok(false),
                _ => bail!("Cannot query observed owned process completion"),
            }
        }
        Ok(true)
    }
    pub(crate) fn stop(&mut self) -> Result<()> {
        check(
            unsafe { TerminateJobObject(self.job.0, 0xC000013A) },
            "Terminate owned streaming job",
        )?;
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
