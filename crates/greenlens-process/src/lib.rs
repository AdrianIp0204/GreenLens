//! Native argument-array subprocesses for trusted compiled engine/report adapters.
//! No imported recipe is executable. Windows Job membership is assigned atomically
//! before the child's first instruction; cancellation terminates all descendants.
use anyhow::{ensure, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};
#[cfg(windows)]
mod windows;

#[derive(Debug, Clone)]
pub struct Limits {
    pub timeout: Duration,
    pub max_scratch_bytes: u64,
    pub max_file_bytes: u64,
    pub max_observed_working_set_bytes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(1800),
            max_scratch_bytes: 2 * 1024 * 1024 * 1024,
            max_file_bytes: 512 * 1024 * 1024,
            max_observed_working_set_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
}
#[derive(Debug, Clone)]
pub struct NativeRequest {
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub directory: PathBuf,
    /// Child-only overrides. Never serialize the inherited environment.
    pub child_environment: BTreeMap<String, String>,
    /// Names removed case-insensitively from the child only, before overrides.
    pub child_environment_remove: Vec<String>,
    pub limits: Limits,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessOutcome {
    pub schema_version: String,
    pub executable: String,
    pub executable_sha256: String,
    pub arguments: Vec<String>,
    pub started_utc: String,
    pub finished_utc: String,
    pub elapsed_ms: u128,
    pub exit_code: Option<i32>,
    pub state: String,
    pub failure: Option<String>,
    pub process_tree_empty: bool,
    pub sampled_peak_runner_plus_job_working_set_bytes: u64,
    pub peak_observed_scratch_bytes: u64,
    pub samples: u64,
    pub stdout_sha256: String,
    pub stderr_sha256: String,
}
pub fn file_hash(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
pub fn scratch_sizes(root: &Path) -> Result<(u64, u64)> {
    scratch_sizes_impl(root, false)
}
fn scratch_sizes_impl(root: &Path, allow_removed_entries: bool) -> Result<(u64, u64)> {
    // Engines create and remove temporary files while running. A listed entry
    // may disappear before it can be measured. Only live sampling tolerates
    // NotFound; the root and the final, quiescent measurement remain strict.
    fn visit(
        root: &Path,
        depth: usize,
        count: &mut usize,
        total: &mut u64,
        max: &mut u64,
        allow_removed_entries: bool,
    ) -> Result<()> {
        ensure!(depth <= 16, "Native scratch directory depth exceeds 16");
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            *count += 1;
            ensure!(*count <= 5000, "Native scratch entry count exceeds 5000");
            let measured = (|| -> Result<()> {
                let meta = query_scratch_metadata(
                    || fs::symlink_metadata(entry.path()),
                    allow_removed_entries,
                )?;
                ensure!(
                    !meta.file_type().is_symlink(),
                    "Native scratch cannot contain links"
                );
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    ensure!(
                        meta.file_attributes() & 0x400 == 0,
                        "Native scratch cannot contain reparse points"
                    );
                }
                if meta.is_dir() {
                    visit(
                        &entry.path(),
                        depth + 1,
                        count,
                        total,
                        max,
                        allow_removed_entries,
                    )?;
                } else {
                    ensure!(meta.is_file(), "Unsupported native scratch file");
                    // Use the fresh path metadata already validated above during
                    // live sampling, not DirEntry's cached enumeration metadata.
                    // A second open can race DeleteFile on Windows and return
                    // AccessDenied for a file that is pending deletion. Do not
                    // suppress that error generally: finalized measurement still
                    // requires a successful open and handle-based size.
                    let n = if allow_removed_entries {
                        meta.len()
                    } else {
                        File::open(entry.path())?.metadata()?.len()
                    };
                    *total = total.checked_add(n).context("Scratch size overflow")?;
                    *max = (*max).max(n);
                }
                Ok(())
            })();
            accept_scratch_entry(measured, allow_removed_entries)?;
        }
        Ok(())
    }
    let (mut count, mut total, mut max) = (0, 0, 0);
    visit(
        root,
        0,
        &mut count,
        &mut total,
        &mut max,
        allow_removed_entries,
    )?;
    Ok((total, max))
}
fn accept_scratch_entry(result: Result<()>, allow_removed_entries: bool) -> Result<()> {
    match result {
        Err(error)
            if allow_removed_entries
                && error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) =>
        {
            Ok(())
        }
        other => other,
    }
}
fn query_scratch_metadata(
    mut query: impl FnMut() -> std::io::Result<fs::Metadata>,
    live: bool,
) -> std::io::Result<fs::Metadata> {
    match query() {
        // On Windows a delete-pending metadata query can return AccessDenied
        // even when the file disappears before std's enumeration fallback.
        // Re-query this exact path once without delay. Only new metadata or a
        // genuine NotFound can resolve that race; persistent denial still fails.
        Err(error) if cfg!(windows) && live && error.raw_os_error() == Some(5) => query(),
        other => other,
    }
}
fn validate(request: &NativeRequest) -> Result<()> {
    ensure!(
        request.executable.is_file() && request.directory.is_dir(),
        "Native executable or scratch directory is unavailable"
    );
    ensure!(
        request.limits.timeout > Duration::ZERO
            && request.limits.timeout <= Duration::from_secs(14400),
        "Native deadline must be within four hours"
    );
    ensure!(
        request.limits.max_file_bytes > 0
            && request.limits.max_file_bytes <= 512 * 1024 * 1024
            && request.limits.max_scratch_bytes > 0
            && request.limits.max_scratch_bytes <= 2 * 1024 * 1024 * 1024
            && request.limits.max_observed_working_set_bytes > 0
            && request.limits.max_observed_working_set_bytes <= 2 * 1024 * 1024 * 1024,
        "Native resource limits exceed supported caps"
    );
    for name in ["process-stdout.txt", "process-stderr.txt", "process.json"] {
        ensure!(
            !request.directory.join(name).exists(),
            "Native process directory already contains a receipt; use a new stage"
        );
    }
    Ok(())
}
#[cfg(not(windows))]
pub fn execute(_request: &NativeRequest, _cancel: &AtomicBool) -> Result<ProcessOutcome> {
    anyhow::bail!("Native Windows process supervision is required")
}

#[cfg(windows)]
pub fn execute(request: &NativeRequest, cancel: &AtomicBool) -> Result<ProcessOutcome> {
    validate(request)?;
    ensure!(
        !cancel.load(Ordering::SeqCst),
        "Cancelled before process launch"
    );
    let started = Instant::now();
    let mut receipt = ProcessOutcome {
        schema_version: "greenlens.native-process.v0.4".into(),
        executable: request
            .executable
            .file_name()
            .context("Missing executable name")?
            .to_string_lossy()
            .into(),
        executable_sha256: file_hash(&request.executable)?,
        arguments: request.arguments.clone(),
        started_utc: Utc::now().to_rfc3339(),
        finished_utc: String::new(),
        elapsed_ms: 0,
        exit_code: None,
        state: "running".into(),
        failure: None,
        process_tree_empty: false,
        sampled_peak_runner_plus_job_working_set_bytes: 0,
        peak_observed_scratch_bytes: 0,
        samples: 0,
        stdout_sha256: String::new(),
        stderr_sha256: String::new(),
    };
    let mut child = windows::Child::start(request, &request.directory)?;
    let result = (|| -> Result<()> {
        loop {
            ensure!(
                !cancel.load(Ordering::SeqCst),
                "Cancelled; native process tree terminated"
            );
            ensure!(
                started.elapsed() < request.limits.timeout,
                "Native process deadline exceeded"
            );
            let memory = child.observed_working_set()?;
            let (scratch, largest) = scratch_sizes_impl(&request.directory, true)
                .context("Sample active native scratch")?;
            receipt.samples += 1;
            receipt.sampled_peak_runner_plus_job_working_set_bytes = receipt
                .sampled_peak_runner_plus_job_working_set_bytes
                .max(memory);
            receipt.peak_observed_scratch_bytes = receipt.peak_observed_scratch_bytes.max(scratch);
            ensure!(
                memory <= request.limits.max_observed_working_set_bytes,
                "Native working-set budget exceeded"
            );
            ensure!(
                scratch <= request.limits.max_scratch_bytes
                    && largest <= request.limits.max_file_bytes,
                "Native scratch/file budget exceeded"
            );
            if let Some(code) = child.exit_code()? {
                receipt.exit_code = Some(code);
                ensure!(code == 0, "Native process exited with code {code}");
                if child.empty()? && child.all_observed_exited()? {
                    // A child can finish writing between the previous sample and
                    // Job closure. Enforce file limits once streams are closed.
                    let (scratch, largest) = scratch_sizes(&request.directory)
                        .context("Measure finalized native scratch")?;
                    receipt.peak_observed_scratch_bytes =
                        receipt.peak_observed_scratch_bytes.max(scratch);
                    ensure!(
                        scratch <= request.limits.max_scratch_bytes
                            && largest <= request.limits.max_file_bytes,
                        "Native final scratch/file budget exceeded"
                    );
                    break;
                }
            }
            thread::sleep(Duration::from_millis(50));
        }
        Ok(())
    })();
    if let Err(error) = &result {
        receipt.state = if cancel.load(Ordering::SeqCst) {
            "cancelled"
        } else {
            "failed"
        }
        .into();
        receipt.failure = Some(format!("{error:#}"));
        if let Err(cleanup) = child.stop() {
            receipt.failure = Some(format!("{error:#}; cleanup: {cleanup:#}"));
        }
    } else {
        receipt.state = "completed".into();
    }
    receipt.process_tree_empty = child.empty()? && child.all_observed_exited()?;
    receipt.exit_code = child.exit_code()?;
    receipt.finished_utc = Utc::now().to_rfc3339();
    receipt.elapsed_ms = started.elapsed().as_millis();
    receipt.stdout_sha256 = file_hash(&request.directory.join("process-stdout.txt"))?;
    receipt.stderr_sha256 = file_hash(&request.directory.join("process-stderr.txt"))?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(request.directory.join("process.json"))?;
    file.write_all(&serde_json::to_vec_pretty(&receipt)?)?;
    file.sync_all()?;
    ensure!(
        receipt.process_tree_empty,
        "Native process tree cleanup incomplete; inspect retained process.json"
    );
    result?;
    Ok(receipt)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::{process::Command, sync::Arc};
    fn request(root: &Path, mode: &str) -> NativeRequest {
        NativeRequest {
            executable: std::env::current_exe().unwrap(),
            arguments: vec![
                "--ignored".into(),
                "--exact".into(),
                "tests::native_child_fixture".into(),
                "--nocapture".into(),
            ],
            directory: root.into(),
            child_environment_remove: vec![],
            child_environment: BTreeMap::from([("GREENLENS_PROCESS_FIXTURE".into(), mode.into())]),
            limits: Limits::default(),
        }
    }
    #[test]
    #[ignore = "Invoked by native supervisor parent tests"]
    // This fixture deliberately leaves a descendant so the parent can verify Job cleanup.
    #[allow(clippy::zombie_processes)]
    fn native_child_fixture() {
        let mode = std::env::var("GREENLENS_PROCESS_FIXTURE").unwrap();
        if mode == "grandchild" {
            fs::write("grandchild.pid", std::process::id().to_string()).unwrap();
            loop {
                thread::sleep(Duration::from_millis(100));
            }
        }
        if mode == "tree" || mode == "orphan" {
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "tests::native_child_fixture",
                    "--nocapture",
                ])
                .env("GREENLENS_PROCESS_FIXTURE", "grandchild")
                .spawn()
                .unwrap();
            if mode == "orphan" {
                return;
            }
            child.wait().unwrap();
        }
        if mode == "fail" {
            std::process::exit(9);
        }
        if mode == "oversize" {
            let file = File::create("large.bin").unwrap();
            file.set_len(100_000).unwrap();
            thread::sleep(Duration::from_secs(10));
        }
        if mode == "open-growing-file" {
            let mut file = File::create("growing.bin").unwrap();
            file.write_all(&vec![0; 131_088]).unwrap();
            file.flush().unwrap();
            thread::sleep(Duration::from_secs(10));
        }
        if mode == "fast-oversize" {
            fs::write("large.bin", vec![0; 100_000]).unwrap();
        }
        if mode == "environment-remove" {
            assert!(std::env::var_os("PATH").is_none());
        }
        if mode == "temporary-files" {
            let until = Instant::now() + Duration::from_millis(500);
            while Instant::now() < until {
                fs::write("temporary.bin", [0; 4096]).unwrap();
                fs::remove_file("temporary.bin").unwrap();
            }
            fs::write("final.bin", [0; 31]).unwrap();
        }
        println!("Native fixture: 空間 complete");
    }
    fn retained(root: &Path) -> ProcessOutcome {
        serde_json::from_slice(&fs::read(root.join("process.json")).unwrap()).unwrap()
    }
    #[test]
    fn live_scratch_removal_is_not_a_failed_process_or_a_final_measurement_skip() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("temporary.bin");
        fs::write(&file, [0; 31]).unwrap();
        let listed = fs::read_dir(tmp.path()).unwrap().next().unwrap().unwrap();
        fs::remove_file(&file).unwrap();
        let measure = || {
            fs::symlink_metadata(listed.path())
                .map(|_| ())
                .map_err(Into::into)
        };
        assert!(accept_scratch_entry(measure(), true).is_ok());
        assert!(accept_scratch_entry(measure(), false).is_err());
        assert!(accept_scratch_entry(
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied).into()),
            true
        )
        .is_err());
        assert!(accept_scratch_entry(
            Err(anyhow::anyhow!("Native scratch cannot contain links")),
            true
        )
        .is_err());
        assert!(scratch_sizes_impl(&tmp.path().join("missing-root"), true).is_err());
        let r = request(tmp.path(), "temporary-files");
        let outcome = execute(&r, &AtomicBool::new(false)).unwrap();
        assert_eq!(outcome.state, "completed");
        assert_eq!(outcome.exit_code, Some(0));
        assert!(outcome.process_tree_empty);
        assert!(!file.exists());
        assert!(scratch_sizes(tmp.path()).unwrap().0 >= 31);
    }
    #[test]
    fn win32_deleted_open_file_is_measured_or_absent_but_final_open_remains_strict() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::DeleteFileW;

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("delete-pending.bin");
        fs::write(&path, [0; 31]).unwrap();
        // Rust's remove_file may use POSIX deletion. Use the Win32 operation
        // with an outstanding share-delete handle. Windows/filesystem versions
        // may leave a delete-pending name or unlink it immediately; both states
        // must be handled without treating persistent denial as disappearance.
        let held = File::open(&path).unwrap();
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        assert_ne!(unsafe { DeleteFileW(wide.as_ptr()) }, 0);
        assert_eq!(held.metadata().unwrap().len(), 31);
        match File::open(&path).unwrap_err().raw_os_error() {
            Some(5) => {
                assert_eq!(fs::symlink_metadata(&path).unwrap().len(), 31);
                assert_eq!(scratch_sizes_impl(tmp.path(), true).unwrap(), (31, 31));
                let final_error = scratch_sizes(tmp.path()).unwrap_err();
                assert_eq!(
                    final_error
                        .downcast_ref::<std::io::Error>()
                        .unwrap()
                        .raw_os_error(),
                    Some(5)
                );
            }
            Some(2) => {
                assert_eq!(
                    fs::symlink_metadata(&path).unwrap_err().kind(),
                    std::io::ErrorKind::NotFound
                );
                assert_eq!(scratch_sizes_impl(tmp.path(), true).unwrap(), (0, 0));
                assert_eq!(scratch_sizes(tmp.path()).unwrap(), (0, 0));
            }
            other => panic!("Unexpected deleted-open-file error: {other:?}"),
        }
        drop(held);
        assert_eq!(scratch_sizes(tmp.path()).unwrap(), (0, 0));
    }
    #[test]
    fn access_denied_metadata_gets_only_one_live_same_path_recheck() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("file.bin");
        fs::write(&path, [0; 31]).unwrap();
        let mut calls = 0;
        let metadata = query_scratch_metadata(
            || {
                calls += 1;
                if calls == 1 {
                    Err(std::io::Error::from_raw_os_error(5))
                } else {
                    fs::symlink_metadata(&path)
                }
            },
            true,
        )
        .unwrap();
        assert_eq!(calls, 2);
        assert_eq!(metadata.len(), 31);
        fs::remove_file(&path).unwrap();
        for live in [true, false] {
            let mut calls = 0;
            let error = query_scratch_metadata(
                || {
                    calls += 1;
                    if calls == 1 {
                        Err(std::io::Error::from_raw_os_error(5))
                    } else {
                        fs::symlink_metadata(&path)
                    }
                },
                live,
            )
            .unwrap_err();
            assert_eq!(calls, if live { 2 } else { 1 });
            assert_eq!(error.raw_os_error(), Some(if live { 2 } else { 5 }));
        }
        let mut calls = 0;
        let error = query_scratch_metadata(
            || {
                calls += 1;
                Err(std::io::Error::from_raw_os_error(5))
            },
            true,
        )
        .unwrap_err();
        assert_eq!(calls, 2);
        assert_eq!(error.raw_os_error(), Some(5));
        // Other failures do not cause a retry, and no predicate is weakened.
        let mut calls = 0;
        let error = query_scratch_metadata(
            || {
                calls += 1;
                Err(std::io::Error::from_raw_os_error(32))
            },
            true,
        )
        .unwrap_err();
        assert_eq!(calls, 1);
        assert_eq!(error.raw_os_error(), Some(32));
    }
    #[test]
    fn live_scratch_uses_fresh_size_while_writer_is_still_open() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("growing.bin");
        let mut held = File::create(&path).unwrap();
        assert_eq!(scratch_sizes_impl(tmp.path(), true).unwrap(), (0, 0));
        held.write_all(&vec![0; 131_088]).unwrap();
        held.flush().unwrap();
        // No writer close or directory enumeration cache is needed to observe
        // the live size. The end-to-end live budget test uses the same sampler.
        assert_eq!(
            scratch_sizes_impl(tmp.path(), true).unwrap(),
            (131_088, 131_088)
        );
        assert_eq!(scratch_sizes(tmp.path()).unwrap(), (131_088, 131_088));
    }
    #[test]
    fn open_writer_exceeding_file_cap_is_stopped_before_it_closes() {
        let tmp = tempfile::tempdir().unwrap();
        let mut r = request(tmp.path(), "open-growing-file");
        r.limits.max_file_bytes = 4096;
        r.limits.timeout = Duration::from_secs(3);
        assert!(execute(&r, &AtomicBool::new(false)).is_err());
        let record = retained(tmp.path());
        assert_eq!(record.state, "failed");
        assert!(record.failure.unwrap().contains("scratch/file budget"));
        assert!(record.peak_observed_scratch_bytes >= 131_088);
        assert!(record.elapsed_ms < 3000);
        assert!(record.process_tree_empty);
    }
    #[test]
    fn inaccessible_directory_is_not_a_removed_live_entry() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Security::{
            GetFileSecurityW, InitializeAcl, InitializeSecurityDescriptor, SetFileSecurityW,
            SetSecurityDescriptorDacl, ACL, ACL_REVISION, DACL_SECURITY_INFORMATION,
            PROTECTED_DACL_SECURITY_INFORMATION, SECURITY_DESCRIPTOR,
        };

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("inaccessible");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("retained.bin"), [0; 31]).unwrap();
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut needed = 0;
        unsafe {
            GetFileSecurityW(
                wide.as_ptr(),
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                0,
                &mut needed,
            );
        }
        assert!(needed > 0);
        // Save this fresh test directory's DACL and restore it even if an
        // assertion unwinds. No production path or inherited ACL is modified.
        struct RestoreAcl(Vec<u16>, Vec<u64>);
        impl Drop for RestoreAcl {
            fn drop(&mut self) {
                unsafe {
                    SetFileSecurityW(
                        self.0.as_ptr(),
                        DACL_SECURITY_INFORMATION,
                        self.1.as_mut_ptr().cast(),
                    );
                }
            }
        }
        let mut restore = RestoreAcl(wide, vec![0; (needed as usize).div_ceil(8)]);
        assert_ne!(
            unsafe {
                GetFileSecurityW(
                    restore.0.as_ptr(),
                    DACL_SECURITY_INFORMATION,
                    restore.1.as_mut_ptr().cast(),
                    needed,
                    &mut needed,
                )
            },
            0
        );
        let mut descriptor: SECURITY_DESCRIPTOR = unsafe { std::mem::zeroed() };
        let mut acl: ACL = unsafe { std::mem::zeroed() };
        assert_ne!(
            unsafe {
                InitializeSecurityDescriptor(
                    (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast(),
                    1,
                )
            },
            0
        );
        assert_ne!(
            unsafe { InitializeAcl(&mut acl, std::mem::size_of::<ACL>() as u32, ACL_REVISION) },
            0
        );
        assert_ne!(
            unsafe {
                SetSecurityDescriptorDacl(
                    (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast(),
                    1,
                    &acl,
                    0,
                )
            },
            0
        );
        assert_ne!(
            unsafe {
                SetFileSecurityW(
                    restore.0.as_ptr(),
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    (&mut descriptor as *mut SECURITY_DESCRIPTOR).cast(),
                )
            },
            0
        );
        assert_eq!(fs::read_dir(&path).unwrap_err().raw_os_error(), Some(5));
        for live in [true, false] {
            let error = scratch_sizes_impl(tmp.path(), live).unwrap_err();
            assert_eq!(
                error
                    .downcast_ref::<std::io::Error>()
                    .unwrap()
                    .raw_os_error(),
                Some(5)
            );
        }
        drop(restore);
        assert_eq!(scratch_sizes(tmp.path()).unwrap(), (31, 31));
    }
    #[test]
    fn native_spaces_unicode_success_and_nonzero_exit_are_distinct() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("native stage 空間");
        fs::create_dir(&root).unwrap();
        let exe = tmp.path().join("fixture engine 空間.exe");
        fs::copy(std::env::current_exe().unwrap(), &exe).unwrap();
        let mut r = request(&root, "success");
        r.executable = exe;
        let outcome = execute(&r, &AtomicBool::new(false)).unwrap();
        assert_eq!(outcome.exit_code, Some(0));
        assert!(outcome.process_tree_empty);
        assert_eq!(outcome.state, "completed");
        assert!(fs::read_to_string(root.join("process-stdout.txt"))
            .unwrap()
            .contains("空間"));
        let failed = tmp.path().join("failed");
        fs::create_dir(&failed).unwrap();
        assert!(execute(&request(&failed, "fail"), &AtomicBool::new(false)).is_err());
        assert_eq!(retained(&failed).exit_code, Some(9));
        assert_eq!(retained(&failed).state, "failed");
    }
    #[test]
    fn cancellation_kills_descendants_and_records_no_success() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let r = request(&root, "tree");
        let cancel = Arc::new(AtomicBool::new(false));
        let signal = cancel.clone();
        let observer = root.clone();
        let handle = thread::spawn(move || {
            let start = Instant::now();
            while !observer.join("grandchild.pid").exists()
                && start.elapsed() < Duration::from_secs(10)
            {
                thread::sleep(Duration::from_millis(20));
            }
            signal.store(true, Ordering::SeqCst);
        });
        assert!(execute(&r, &cancel).is_err());
        handle.join().unwrap();
        assert!(root.join("grandchild.pid").exists());
        let record = retained(&root);
        assert_eq!(record.state, "cancelled");
        assert!(record.process_tree_empty);
    }
    #[test]
    fn root_exit_does_not_abandon_a_live_descendant() {
        let tmp = tempfile::tempdir().unwrap();
        let mut r = request(tmp.path(), "orphan");
        r.limits.timeout = Duration::from_secs(2);
        assert!(execute(&r, &AtomicBool::new(false)).is_err());
        let record = retained(tmp.path());
        assert_eq!(record.state, "failed");
        assert!(record.failure.unwrap().contains("deadline"));
        assert!(record.process_tree_empty);
        assert!(tmp.path().join("grandchild.pid").exists());
    }
    #[test]
    fn file_budget_failure_retains_diagnostics_and_terminates_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let mut r = request(tmp.path(), "oversize");
        r.limits.max_file_bytes = 50_000;
        assert!(execute(&r, &AtomicBool::new(false)).is_err());
        let record = retained(tmp.path());
        assert_eq!(record.state, "failed");
        assert!(record.failure.unwrap().contains("budget"));
        assert!(record.process_tree_empty);
    }
    #[test]
    fn fast_closed_output_is_bounded_and_child_environment_removal_is_local() {
        let parent_path = std::env::var_os("PATH");
        assert!(parent_path.is_some());
        let tmp = tempfile::tempdir().unwrap();
        let mut r = request(tmp.path(), "environment-remove");
        r.child_environment_remove = vec!["pAtH".into()];
        execute(&r, &AtomicBool::new(false)).unwrap();
        assert_eq!(std::env::var_os("PATH"), parent_path);
        let tmp = tempfile::tempdir().unwrap();
        let mut r = request(tmp.path(), "fast-oversize");
        r.limits.max_file_bytes = 50_000;
        assert!(execute(&r, &AtomicBool::new(false)).is_err());
        assert_eq!(retained(tmp.path()).state, "failed");
        assert!(retained(tmp.path()).process_tree_empty);
    }
}
