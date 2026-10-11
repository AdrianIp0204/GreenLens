//! Native synthetic child tests; no Radiance/scientific acceptance claim.
use super::*;
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
#[ignore = "synthetic subprocess entry, invoked by native stream supervisor tests"]
fn child_fixture() {
    let mode = std::env::var("GREENLENS_STREAM_TEST_MODE").unwrap();
    if mode != "descendant" {
        fs::write("fixture-parent.pid", std::process::id().to_string()).unwrap();
    }
    match mode.as_str() {
        "parent" => {
            let descendant = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "annual_stream::tests::child_fixture",
                    "--nocapture",
                ])
                .env("GREENLENS_STREAM_TEST_MODE", "descendant")
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap();
            // No wait: the parent intentionally exits while its child owns stdin.
            std::mem::forget(descendant);
        }
        "descendant" => {
            fs::write("fixture-descendant.pid", std::process::id().to_string()).unwrap();
            thread::sleep(Duration::from_secs(60));
        }
        "hold" => thread::sleep(Duration::from_secs(60)),
        "early" => {}
        "output" => {
            std::io::stdout()
                .write_all(&vec![b'x'; 128 * 1024])
                .unwrap();
            thread::sleep(Duration::from_secs(60));
        }
        "final_output" => {
            std::io::copy(&mut std::io::stdin().lock(), &mut std::io::sink()).unwrap();
            std::io::stdout()
                .write_all(&vec![b'x'; 128 * 1024])
                .unwrap();
            // The final write and exit may occur between two live resource samples.
        }
        "echo" | "slow" => {
            let mut input = std::io::stdin().lock();
            let mut result = File::create("fixture-input.bin").unwrap();
            let mut buffer = [0; 511];
            loop {
                let size = input.read(&mut buffer).unwrap();
                if size == 0 {
                    break;
                }
                result.write_all(&buffer[..size]).unwrap();
                if mode == "slow" {
                    thread::sleep(Duration::from_millis(1));
                }
            }
            fs::write(
                "fixture-arguments.json",
                serde_json::to_vec(&std::env::args().collect::<Vec<_>>()).unwrap(),
            )
            .unwrap();
            fs::write(
                "fixture-environment.txt",
                std::env::var("GREENLENS_STREAM_TEST_VALUE").unwrap(),
            )
            .unwrap();
        }
        _ => panic!("unknown fixture"),
    }
}

fn request(mode: &str, expected_records: u64) -> StreamRequest {
    let root = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
        .join("GreenLens/verification")
        .join(format!(
            "stream synthetic 空間 {}-{}-{}",
            std::process::id(),
            Utc::now().timestamp_micros(),
            TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(&root).unwrap();
    StreamRequest {
        executable: std::env::current_exe().unwrap(),
        arguments: vec![
            "--ignored".into(),
            "--exact".into(),
            "annual_stream::tests::child_fixture".into(),
            "--nocapture".into(),
        ],
        directory: root,
        child_environment: BTreeMap::from([
            ("GREENLENS_STREAM_TEST_MODE".into(), mode.into()),
            (
                "GREENLENS_STREAM_TEST_VALUE".into(),
                "space 空間 \"quoted\" C:\\tail\\".into(),
            ),
        ]),
        expected_records,
        limits: StreamLimits {
            deadline: Duration::from_secs(8),
            ..StreamLimits::default()
        },
    }
}
fn input(count: usize) -> impl Iterator<Item = Result<Vec<u8>>> {
    (0..count).map(|index| Ok(format!("{index:06} {}\n", "x".repeat(120)).into_bytes()))
}
fn check_dead(request: &StreamRequest) {
    for name in ["fixture-parent.pid", "fixture-descendant.pid"] {
        if let Ok(text) = fs::read_to_string(request.directory.join(name)) {
            let pid: u32 = text.parse().unwrap();
            let handle = unsafe { OpenProcess(SYNCHRONIZE, 0, pid) };
            if !handle.is_null() {
                let handle = Handle(handle);
                assert_eq!(
                    unsafe { WaitForSingleObject(handle.0, 0) },
                    WAIT_OBJECT_0,
                    "surviving child {pid}"
                );
            }
        }
    }
    assert!(!request.directory.join("stream-success.json").exists());
    let diagnostic: serde_json::Value =
        serde_json::from_slice(&fs::read(request.directory.join("stream-failure.json")).unwrap())
            .unwrap();
    assert_eq!(diagnostic["process_tree_terminated"], true);
}

#[test]
fn native_partial_nonblocking_stream_preserves_bytes_count_digest_and_arguments() {
    let mut req = request("slow", 1500);
    let tricky = "not-a-test 空間 \"quote\" C:\\path with spaces\\";
    req.arguments.extend(["--skip".into(), tricky.into()]);
    let expected: Vec<_> = input(1500).flat_map(|r| r.unwrap()).collect();
    let before = std::env::var_os("GREENLENS_STREAM_TEST_VALUE");
    let outcome = execute(&req, input(1500), &AtomicBool::new(false)).unwrap();
    outcome.telemetry.validate().unwrap();
    assert_eq!(
        fs::read(req.directory.join("fixture-input.bin")).unwrap(),
        expected
    );
    assert_eq!(
        outcome.input,
        InputReceipt {
            bytes: expected.len() as u64,
            records: 1500,
            sha256: format!("{:x}", Sha256::digest(&expected))
        }
    );
    let arguments: Vec<String> =
        serde_json::from_slice(&fs::read(req.directory.join("fixture-arguments.json")).unwrap())
            .unwrap();
    assert!(arguments.iter().any(|a| a == tricky));
    assert_eq!(
        fs::read_to_string(req.directory.join("fixture-environment.txt")).unwrap(),
        req.child_environment["GREENLENS_STREAM_TEST_VALUE"]
    );
    assert_eq!(std::env::var_os("GREENLENS_STREAM_TEST_VALUE"), before);
    assert!(outcome.telemetry.partial_writes > 0 || outcome.telemetry.zero_progress_writes > 0);
    assert!(outcome.telemetry.atomic_job_assignment);
    assert!(!req.directory.join("stream-failure.json").exists());
}

#[test]
fn native_run_scope_uses_root_cwd_and_includes_sibling_files_in_budget() {
    let mut req = request("echo", 2);
    let root = req.directory.clone();
    req.directory = root.join("stage");
    fs::create_dir(&req.directory).unwrap();
    let outcome = execute_in_run(&req, input(2), &AtomicBool::new(false), &root).unwrap();
    assert_eq!(outcome.input.records, 2);
    outcome.telemetry.validate().unwrap();
    assert!(root.join("fixture-input.bin").is_file());
    assert!(!req.directory.join("fixture-input.bin").exists());
    assert!(req.directory.join("stream-success.json").is_file());

    let mut req = request("hold", 2);
    let root = req.directory.clone();
    req.directory = root.join("stage");
    fs::create_dir(&req.directory).unwrap();
    fs::write(root.join("prior-stage.mtx"), [0; 32768]).unwrap();
    req.limits.max_scratch_bytes = 16384;
    let error = execute_in_run(&req, input(2), &AtomicBool::new(false), &root).unwrap_err();
    assert!(error.to_string().contains("scratch resource"), "{error:#}");
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(req.directory.join("stream-failure.json")).unwrap())
            .unwrap();
    assert_eq!(report["process_tree_terminated"], true);
    assert!(!req.directory.join("stream-success.json").exists());
    assert!(execute_in_run(&req, input(2), &AtomicBool::new(false), &req.directory).is_err());
}

#[test]
fn native_parent_exit_does_not_lose_descendant_pipe_holder() {
    let mut req = request("parent", 1);
    req.limits.deadline = Duration::from_millis(450);
    let start = Instant::now();
    let error = execute(&req, input(1), &AtomicBool::new(false)).unwrap_err();
    assert!(error.to_string().contains("deadline"), "{error:#}");
    assert!(start.elapsed() < Duration::from_secs(5));
    assert!(req.directory.join("fixture-descendant.pid").exists());
    check_dead(&req);
}

#[test]
fn native_early_stdin_exit_and_producer_fault_cannot_complete() {
    let req = request("early", 100_000);
    let start = Instant::now();
    assert!(execute(&req, input(100_000), &AtomicBool::new(false)).is_err());
    assert!(start.elapsed() < Duration::from_secs(5));
    check_dead(&req);
    let req = request("hold", 3);
    let records = vec![
        Ok(b"one\n".to_vec()),
        Err(anyhow::anyhow!("intentional producer fault")),
    ];
    let error = execute(&req, records.into_iter(), &AtomicBool::new(false)).unwrap_err();
    assert!(format!("{error:#}").contains("intentional producer fault"));
    check_dead(&req);
}

#[test]
fn native_cancel_stalled_writer_terminates_job_without_join() {
    let req = request("hold", 100_000);
    let cancel = AtomicBool::new(false);
    let start = Instant::now();
    thread::scope(|scope| {
        scope.spawn(|| {
            thread::sleep(Duration::from_millis(250));
            cancel.store(true, Ordering::SeqCst);
        });
        let error = execute(&req, input(100_000), &cancel).unwrap_err();
        assert!(error.to_string().contains("cancelled"));
    });
    assert!(start.elapsed() < Duration::from_secs(5));
    check_dead(&req);
}

#[test]
fn native_resource_and_input_shape_failures_do_not_complete() {
    for (mode, memory, file) in [
        ("hold", 1, 512 * 1024 * 1024),
        ("output", 2 * 1024 * 1024 * 1024, 4096),
        ("final_output", 2 * 1024 * 1024 * 1024, 4096),
    ] {
        let count = if mode == "final_output" { 1 } else { 100_000 };
        let mut req = request(mode, count as u64);
        req.limits.max_observed_working_set_bytes = memory;
        req.limits.max_file_bytes = file;
        let error = execute(&req, input(count), &AtomicBool::new(false)).unwrap_err();
        assert!(error.to_string().contains("resource limit"), "{error:#}");
        check_dead(&req);
    }
    for (expected, actual) in [(2, 1), (1, 2)] {
        let req = request("hold", expected);
        let error = execute(&req, input(actual), &AtomicBool::new(false)).unwrap_err();
        assert!(error.to_string().contains("input records"), "{error:#}");
        check_dead(&req);
    }
    let mut req = request("hold", 1);
    req.limits.max_input_bytes = 1;
    assert!(execute(&req, input(1), &AtomicBool::new(false))
        .unwrap_err()
        .to_string()
        .contains("byte bound"));
    check_dead(&req);
}

#[test]
fn validation_rejects_unbounded_or_nonexistent_launch_before_creating_diagnostics() {
    let mut req = request("echo", MAX_RECORDS + 1);
    assert!(execute(&req, input(1), &AtomicBool::new(false)).is_err());
    req.expected_records = 1;
    req.limits.deadline = Duration::ZERO;
    assert!(execute(&req, input(1), &AtomicBool::new(false)).is_err());
    req.limits.deadline = Duration::from_secs(1);
    req.executable = req.directory.join("missing.exe");
    assert!(execute(&req, input(1), &AtomicBool::new(false)).is_err());
    assert!(fs::read_dir(req.directory).unwrap().next().is_none());
}
