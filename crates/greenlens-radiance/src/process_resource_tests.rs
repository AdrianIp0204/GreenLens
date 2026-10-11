//! Real Windows supervision with synthetic processes; no Radiance science claim.
use super::*;
use std::{ffi::c_void, os::windows::io::AsRawHandle, path::PathBuf};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn DeviceIoControl(
        handle: *mut c_void,
        code: u32,
        input: *const c_void,
        input_size: u32,
        output: *mut c_void,
        output_size: u32,
        returned: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
}

#[test]
#[ignore = "synthetic child entry point invoked by required resource-cap test"]
fn resource_child_fixture() {
    let directory = std::env::current_dir().unwrap();
    // Wait until the actual supervisor has assigned this process to its job.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !directory.join("helper.started.json").exists() {
        assert!(
            Instant::now() < deadline,
            "Supervisor did not attach fixture"
        );
        thread::sleep(Duration::from_millis(5));
    }
    fs::write(directory.join("child.pid"), std::process::id().to_string()).unwrap();
    use std::os::windows::process::CommandExt;
    let mut descendant = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "process::resource_tests::resource_grandchild_fixture",
            "--nocapture",
        ])
        .creation_flags(0x0800_0000)
        .spawn()
        .unwrap();
    while !directory.join("grandchild.pid").exists() {
        assert!(
            Instant::now() < deadline,
            "Fixture descendant did not start"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let oversized = File::create(directory.join("oversized-sparse.fixture")).unwrap();
    let mut returned = 0;
    // FSCTL_SET_SPARSE, as declared in the pinned Windows SDK bindings. Only
    // logical length exceeds the real production cap; no 512 MiB payload is
    // allocated, generated, downloaded, or retained by this fixture.
    let sparse = unsafe {
        DeviceIoControl(
            oversized.as_raw_handle(),
            0x0009_00c4,
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(sparse, 0, "Sparse fixture requires a supporting filesystem");
    oversized.set_len(ANNUAL_MAX_FILE_BYTES + 1).unwrap();
    drop(oversized);
    thread::sleep(Duration::from_secs(15));
    // Reaching this sentinel means the resource supervisor failed to intervene.
    fs::write(
        directory.join("fixture-completed.txt"),
        "unexpected completion",
    )
    .unwrap();
    descendant.kill().unwrap();
    descendant.wait().unwrap();
}

#[test]
#[ignore = "synthetic descendant entry point invoked by required resource-cap test"]
fn resource_grandchild_fixture() {
    fs::write("grandchild.pid", std::process::id().to_string()).unwrap();
    thread::sleep(Duration::from_secs(30));
}

fn active(pid: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
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
fn annual_file_cap_terminates_tree_without_success_record() {
    let base = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap()).join(format!(
        "GreenLens/verification/resource-cap 空間 {}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap()
    ));
    let engine = base.join("fixture-engine");
    let scratch = base.join("job");
    fs::create_dir_all(engine.join("bin")).unwrap();
    fs::create_dir_all(&scratch).unwrap();
    fs::copy(
        std::env::current_exe().unwrap(),
        engine.join("bin/helper.exe"),
    )
    .unwrap();
    let result = execute_annual(
        &engine,
        "helper",
        &[
            "--ignored".into(),
            "--exact".into(),
            "process::resource_tests::resource_child_fixture".into(),
            "--nocapture".into(),
        ],
        &scratch,
        None,
        &scratch.join("fixture.stdout"),
        &AtomicBool::new(false),
    );
    let error = result.expect_err("Resource failure cannot return a successful process/result");
    assert!(
        error.to_string().contains("512 MiB file budget"),
        "{error:#}"
    );
    assert!(!scratch.join("fixture-completed.txt").exists());
    for file in ["child.pid", "grandchild.pid"] {
        let pid = fs::read_to_string(scratch.join(file))
            .unwrap()
            .parse()
            .unwrap();
        assert!(
            !active(pid),
            "Process {pid} survived resource-limit failure"
        );
    }
    let bytes = fs::read(scratch.join("helper.process.json")).unwrap();
    let failure: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(failure["process_tree_terminated"], true);
    assert!(failure["resource_failure"]
        .as_str()
        .unwrap()
        .contains("512 MiB"));
    assert!(serde_json::from_slice::<ProcessRecord>(&bytes).is_err());
    let resources: AnnualProcessResources =
        serde_json::from_slice(&fs::read(scratch.join("helper.resources.json")).unwrap()).unwrap();
    assert!(resources.samples > 0);
    assert!(resources.peak_scratch_bytes > ANNUAL_MAX_FILE_BYTES);
    assert!(resources.peak_observed_runner_plus_child_working_set_bytes > 0);
    // Remove only the exact owned sparse fixture after retaining actual observed
    // failure/peaks. No directory recursion or unrelated cache is touched.
    let payload = scratch
        .join("oversized-sparse.fixture")
        .canonicalize()
        .unwrap();
    assert_eq!(payload.parent().unwrap(), scratch.canonicalize().unwrap());
    fs::remove_file(payload).unwrap();
}
