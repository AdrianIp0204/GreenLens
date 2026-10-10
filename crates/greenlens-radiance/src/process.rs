use anyhow::{bail, Context, Result};
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
    let status = loop {
        if cancel.load(Ordering::SeqCst) {
            #[cfg(windows)]
            drop(job);
            #[cfg(not(windows))]
            child.kill()?;
            let status = child.wait()?;
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
    Ok(record)
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
        let base = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap()).join(
            format!("GreenLens/verification/process-tree-{}", std::process::id()),
        );
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
        let error = execute(
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
    }
}
