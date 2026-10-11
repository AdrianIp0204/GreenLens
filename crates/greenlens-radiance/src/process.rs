use anyhow::{bail, ensure, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessRecord {
    pub executable: String,
    pub arguments: Vec<String>,
    pub exit_code: i32,
    pub started_utc: String,
    pub finished_utc: String,
    pub elapsed_ms: u128,
    pub stderr: String,
}

/// Annual-only telemetry. The historical ProcessRecord wire format is unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnualProcessResources {
    pub executable: String,
    pub started_utc: String,
    pub finished_utc: String,
    pub elapsed_ms: u128,
    pub peak_child_working_set_bytes: u64,
    pub peak_observed_runner_plus_child_working_set_bytes: u64,
    pub peak_scratch_bytes: u64,
    pub output_bytes: u64,
    pub samples: u64,
}

pub(crate) const ANNUAL_MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
pub(crate) const ANNUAL_MAX_SCRATCH_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub(crate) const ANNUAL_MAX_OBSERVED_WORKING_SET_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[cfg(all(test, windows))]
#[path = "process_resource_tests.rs"]
mod resource_tests;

pub(crate) fn scratch_sizes(directory: &Path) -> Result<(u64, u64)> {
    fn visit(
        directory: &Path,
        depth: usize,
        count: &mut usize,
        total: &mut u64,
        largest: &mut u64,
    ) -> Result<()> {
        ensure!(depth <= 16, "Annual scratch directory depth exceeds limit");
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            *count += 1;
            ensure!(*count <= 5000, "Annual scratch entry count exceeds limit");
            let kind = entry.file_type()?;
            let metadata = entry.metadata()?;
            ensure!(
                !kind.is_symlink(),
                "Annual scratch must not contain symbolic links"
            );
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    metadata.file_attributes() & 0x400 == 0,
                    "Annual scratch must not contain reparse points"
                );
            }
            if kind.is_dir() {
                visit(&entry.path(), depth + 1, count, total, largest)?;
            } else {
                ensure!(
                    kind.is_file(),
                    "Annual scratch contains an unsupported file type"
                );
                // Directory-entry size may remain stale while the engine holds
                // a growing output open. Query the open file handle instead.
                let size = File::open(entry.path())?.metadata()?.len();
                *total = total.checked_add(size).context("Scratch size overflow")?;
                *largest = (*largest).max(size);
            }
        }
        Ok(())
    }
    let (mut count, mut total, mut largest) = (0, 0_u64, 0);
    visit(directory, 0, &mut count, &mut total, &mut largest)?;
    Ok((total, largest))
}

#[cfg(windows)]
fn working_sets(child: &std::process::Child) -> Result<(u64, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::GetCurrentProcess,
    };
    unsafe {
        let mut child_counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        let mut runner_counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        child_counters.cb = size;
        runner_counters.cb = size;
        if GetProcessMemoryInfo(child.as_raw_handle(), &mut child_counters, size) == 0
            || GetProcessMemoryInfo(GetCurrentProcess(), &mut runner_counters, size) == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok((
            child_counters.PeakWorkingSetSize as u64,
            child_counters.WorkingSetSize as u64 + runner_counters.WorkingSetSize as u64,
        ))
    }
}

#[cfg(not(windows))]
fn working_sets(_child: &std::process::Child) -> Result<(u64, u64)> {
    bail!("Annual resource supervision currently requires native Windows")
}

impl AnnualProcessResources {
    fn sample(&mut self, child: &std::process::Child, directory: &Path) -> Result<()> {
        let (child_peak, observed_combined) = working_sets(child)?;
        let (scratch, largest) = scratch_sizes(directory)?;
        self.samples += 1;
        self.peak_child_working_set_bytes = self.peak_child_working_set_bytes.max(child_peak);
        self.peak_observed_runner_plus_child_working_set_bytes = self
            .peak_observed_runner_plus_child_working_set_bytes
            .max(observed_combined);
        self.peak_scratch_bytes = self.peak_scratch_bytes.max(scratch);
        ensure!(
            largest <= ANNUAL_MAX_FILE_BYTES,
            "Annual output exceeds 512 MiB file budget"
        );
        ensure!(
            scratch <= ANNUAL_MAX_SCRATCH_BYTES,
            "Annual scratch exceeds 2 GiB budget"
        );
        ensure!(
            observed_combined <= ANNUAL_MAX_OBSERVED_WORKING_SET_BYTES,
            "Observed runner and engine working sets exceed 2 GiB budget"
        );
        Ok(())
    }
    fn finish(&mut self, directory: &Path, output: &Path, start: Instant) -> Result<(u64, u64)> {
        self.finished_utc = Utc::now().to_rfc3339();
        self.elapsed_ms = start.elapsed().as_millis();
        self.output_bytes = output.metadata()?.len();
        let sizes = scratch_sizes(directory)?;
        self.peak_scratch_bytes = self.peak_scratch_bytes.max(sizes.0);
        fs::write(
            directory.join(format!(
                "{}.resources.json",
                self.executable.trim_end_matches(".exe")
            )),
            serde_json::to_vec_pretty(self)?,
        )?;
        // Return final sizes only after retaining telemetry. Cancellation and
        // prior failures must still write their original process diagnostics.
        Ok(sizes)
    }
}

/// No shell, no pipeline text conversion, and no inherited global RAYPATH.
pub(crate) fn execute(
    engine: &Path,
    name: &str,
    arguments: &[String],
    directory: &Path,
    input: Option<&Path>,
    output: &Path,
    cancel: &AtomicBool,
) -> Result<ProcessRecord> {
    execute_inner(
        engine, name, arguments, directory, input, output, cancel, false, None,
    )
    .map(|(record, _)| record)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_annual(
    engine: &Path,
    name: &str,
    arguments: &[String],
    directory: &Path,
    input: Option<&Path>,
    output: &Path,
    cancel: &AtomicBool,
) -> Result<(ProcessRecord, AnnualProcessResources)> {
    let (record, resources) = execute_inner(
        engine, name, arguments, directory, input, output, cancel, true, None,
    )?;
    Ok((
        record,
        resources.context("Annual resource measurement missing")?,
    ))
}

/// New-method bounded file-backed stages; historical callers keep their exact
/// execution policy. A deadline failure retains the normal failure diagnostics.
#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_annual_with_deadline(
    engine: &Path,
    name: &str,
    arguments: &[String],
    directory: &Path,
    input: Option<&Path>,
    output: &Path,
    cancel: &AtomicBool,
    deadline: Duration,
) -> Result<(ProcessRecord, AnnualProcessResources)> {
    ensure!(
        !deadline.is_zero() && deadline <= Duration::from_secs(7200),
        "Invalid annual process deadline"
    );
    let (record, resources) = execute_inner(
        engine,
        name,
        arguments,
        directory,
        input,
        output,
        cancel,
        true,
        Some(deadline),
    )?;
    Ok((
        record,
        resources.context("Annual resource measurement missing")?,
    ))
}

#[allow(clippy::too_many_arguments)]
fn execute_inner(
    engine: &Path,
    name: &str,
    arguments: &[String],
    directory: &Path,
    input: Option<&Path>,
    output: &Path,
    cancel: &AtomicBool,
    annual: bool,
    deadline: Option<Duration>,
) -> Result<(ProcessRecord, Option<AnnualProcessResources>)> {
    if cancel.load(Ordering::SeqCst) {
        bail!("Simulation cancelled before {name}");
    }
    let stderr_path = directory.join(format!("{name}.stderr"));
    let mut command = Command::new(engine.join("bin").join(format!("{name}.exe")));
    command
        .args(arguments)
        .current_dir(directory)
        .env("RAYPATH", format!(".;{}", engine.join("lib").display()))
        .stdin(match input {
            Some(path) => Stdio::from(File::open(path)?),
            None => Stdio::null(),
        })
        .stdout(Stdio::from(File::create(output)?))
        .stderr(Stdio::from(File::create(&stderr_path)?));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let started_utc = Utc::now().to_rfc3339();
    let start = Instant::now();
    let mut child = command.spawn().with_context(|| {
        format!("Start {name}; check the engine installation and Microsoft runtime")
    })?;
    #[cfg(windows)]
    let job = match WindowsJob::attach(&child) {
        Ok(job) => job,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.context("Cannot supervise Radiance process tree; process stopped"));
        }
    };
    fs::write(
        directory.join(format!("{name}.started.json")),
        serde_json::to_vec_pretty(
            &serde_json::json!({"executable":format!("{name}.exe"),"pid":child.id(),"started_utc":started_utc}),
        )?,
    )?;
    let mut resources = annual.then(|| AnnualProcessResources {
        executable: format!("{name}.exe"),
        started_utc: started_utc.clone(),
        finished_utc: String::new(),
        elapsed_ms: 0,
        peak_child_working_set_bytes: 0,
        peak_observed_runner_plus_child_working_set_bytes: 0,
        peak_scratch_bytes: 0,
        output_bytes: 0,
        samples: 0,
    });
    let status = loop {
        let resource_error = resources
            .as_mut()
            .and_then(|resources| resources.sample(&child, directory).err())
            .or_else(|| {
                deadline
                    .filter(|limit| start.elapsed() >= *limit)
                    .map(|_| anyhow::anyhow!("Annual process deadline exceeded"))
            });
        if cancel.load(Ordering::SeqCst) || resource_error.is_some() {
            #[cfg(windows)]
            drop(job);
            #[cfg(not(windows))]
            child.kill()?;
            let status = child.wait()?;
            if let Some(resources) = &mut resources {
                resources.finish(directory, output, start)?;
            }
            if let Some(error) = resource_error {
                fs::write(
                    directory.join(format!("{name}.process.json")),
                    serde_json::to_vec_pretty(
                        &serde_json::json!({"executable":format!("{name}.exe"),"arguments":arguments,"exit_code":status.code(),"started_utc":started_utc,"finished_utc":Utc::now().to_rfc3339(),"resource_failure":error.to_string(),"process_tree_terminated":true}),
                    )?,
                )?;
                bail!(
                    "Annual resource supervision stopped {name}: {error}; process tree terminated"
                );
            }
            fs::write(
                directory.join(format!("{name}.process.json")),
                serde_json::to_vec_pretty(
                    &serde_json::json!({"executable":format!("{name}.exe"),"arguments":arguments,"exit_code":status.code(),"started_utc":started_utc,"finished_utc":Utc::now().to_rfc3339(),"cancelled":true,"process_tree_terminated":true}),
                )?,
            )?;
            bail!("Simulation cancelled during {name}; process tree terminated");
        }
        match child.try_wait()? {
            Some(status) => break status,
            None => thread::sleep(Duration::from_millis(20)),
        }
    };
    if let Some(resources) = &mut resources {
        let (scratch, largest) = resources.finish(directory, output, start)?;
        let resource_error = if largest > ANNUAL_MAX_FILE_BYTES {
            Some("Annual final output exceeds 512 MiB file budget")
        } else if scratch > ANNUAL_MAX_SCRATCH_BYTES {
            Some("Annual final scratch exceeds 2 GiB budget")
        } else if deadline.is_some_and(|limit| start.elapsed() >= limit) {
            Some("Annual process deadline exceeded during finalization")
        } else {
            None
        };
        if let Some(error) = resource_error {
            #[cfg(windows)]
            drop(job);
            fs::write(
                directory.join(format!("{name}.process.json")),
                serde_json::to_vec_pretty(
                    &serde_json::json!({"executable":format!("{name}.exe"),"arguments":arguments,"exit_code":status.code(),"started_utc":started_utc,"finished_utc":Utc::now().to_rfc3339(),"resource_failure":error,"process_tree_terminated":true}),
                )?,
            )?;
            bail!("Annual resource supervision stopped {name}: {error}; process tree terminated");
        }
    }
    let mut stderr = fs::read_to_string(stderr_path)?;
    for (path, replacement) in [(engine, "<engine>"), (directory, "<scratch>")] {
        let text = path.display().to_string();
        stderr = stderr
            .replace(&text, replacement)
            .replace(text.trim_start_matches(r"\\?\"), replacement);
    }
    let record = ProcessRecord {
        executable: format!("{name}.exe"),
        arguments: arguments.to_vec(),
        exit_code: status.code().unwrap_or(-1),
        started_utc,
        finished_utc: Utc::now().to_rfc3339(),
        elapsed_ms: start.elapsed().as_millis(),
        stderr,
    };
    // Preserve the complete process record, including failed exits, before an
    // error is returned. The caller never treats these diagnostics as a run.
    fs::write(
        directory.join(format!("{name}.process.json")),
        serde_json::to_vec_pretty(&record)?,
    )?;
    if !status.success() {
        bail!("{name} exited {}: {}", record.exit_code, record.stderr);
    }
    Ok((record, resources))
}

#[cfg(windows)]
pub(crate) struct WindowsJob(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl WindowsJob {
    pub(crate) fn attach(child: &std::process::Child) -> Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::*;
        // Only our trusted Radiance tools are launched, using one worker and no
        // scene shell directives. They cannot spawn children before assignment.
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(std::io::Error::last_os_error().into());
            }
            let job = Self(handle);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            ) == 0
                || AssignProcessToJobObject(handle, child.as_raw_handle()) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            Ok(job)
        }
    }
}

#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::sync::Arc;

    // These ignored test-harness entry points are deliberately invoked by the
    // required tree test below; they are synthetic process fixtures, not science.
    #[test]
    #[ignore = "child process fixture, invoked by cancellation_terminates_process_tree"]
    fn child_fixture() {
        let dir = std::env::current_dir().unwrap();
        fs::write(dir.join("child.pid"), std::process::id().to_string()).unwrap();
        use std::os::windows::process::CommandExt;
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "process::tests::grandchild_fixture",
                "--nocapture",
            ])
            .creation_flags(0x0800_0000)
            .spawn()
            .unwrap();
        thread::sleep(Duration::from_secs(60));
        child.kill().unwrap();
        child.wait().unwrap();
    }

    #[test]
    #[ignore = "grandchild process fixture, invoked by cancellation_terminates_process_tree"]
    fn grandchild_fixture() {
        fs::write(
            std::env::current_dir().unwrap().join("grandchild.pid"),
            std::process::id().to_string(),
        )
        .unwrap();
        thread::sleep(Duration::from_secs(60));
    }

    fn active(pid: u32) -> bool {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::Threading::{
                GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            },
        };
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return false;
            }
            let mut status = 0;
            let ok = GetExitCodeProcess(handle, &mut status);
            CloseHandle(handle);
            ok != 0 && status == 259
        }
    }

    #[test]
    fn cancellation_terminates_process_tree() {
        check_cancellation_tree(false);
    }

    #[test]
    fn annual_cancellation_terminates_process_tree_and_retains_measurements() {
        check_cancellation_tree(true);
    }

    fn check_cancellation_tree(annual: bool) {
        // A recycled Windows PID must not make old fixture marker files look
        // like freshly launched descendants in a later test invocation.
        let base =
            std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap()).join(format!(
                "GreenLens/verification/process-tree-{}-{}",
                std::process::id(),
                Utc::now().timestamp_micros()
            ));
        fs::create_dir_all(base.join("bin")).unwrap();
        fs::copy(
            std::env::current_exe().unwrap(),
            base.join("bin/helper.exe"),
        )
        .unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let thread_cancel = cancel.clone();
        let watch_dir = base.clone();
        let watcher = thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(10);
            while Instant::now() < until
                && !(watch_dir.join("child.pid").exists()
                    && watch_dir.join("grandchild.pid").exists())
            {
                thread::sleep(Duration::from_millis(20));
            }
            thread_cancel.store(true, Ordering::SeqCst);
        });
        let error = execute_inner(
            &base,
            "helper",
            &[
                "--ignored".into(),
                "--exact".into(),
                "process::tests::child_fixture".into(),
                "--nocapture".into(),
            ],
            &base,
            None,
            &base.join("fixture.stdout"),
            &cancel,
            annual,
            None,
        )
        .unwrap_err();
        watcher.join().unwrap();
        assert!(error.to_string().contains("cancelled"));
        for file in ["child.pid", "grandchild.pid"] {
            let pid: u32 = fs::read_to_string(base.join(file))
                .expect("fixture started both child and grandchild")
                .parse()
                .unwrap();
            assert!(!active(pid), "Process {pid} survived job cancellation");
        }
        if annual {
            let resources: AnnualProcessResources =
                serde_json::from_slice(&fs::read(base.join("helper.resources.json")).unwrap())
                    .unwrap();
            assert!(resources.samples > 0);
            assert!(resources.peak_child_working_set_bytes > 0);
            assert!(resources.peak_observed_runner_plus_child_working_set_bytes > 0);
            assert!(resources.peak_scratch_bytes > 0);
        }
    }

    #[test]
    fn scratch_measurement_includes_nested_files() {
        let base =
            std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap()).join(format!(
                "GreenLens/verification/annual-resource-shape-{}-{}",
                std::process::id(),
                Utc::now().timestamp_micros()
            ));
        fs::create_dir_all(base.join("weather/series")).unwrap();
        fs::write(base.join("weather/original.epw"), [0; 13]).unwrap();
        fs::write(base.join("weather/series/chunk"), [0; 17]).unwrap();
        fs::write(base.join("scene.rad"), [0; 7]).unwrap();
        assert_eq!(scratch_sizes(&base).unwrap(), (37, 17));
    }
}
