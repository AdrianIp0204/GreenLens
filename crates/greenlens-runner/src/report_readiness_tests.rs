//! Report readiness is session state, not proof that a simulation ran here.
//! Control-state fixtures below contain no fabricated accepted run. The explicit
//! retained-evidence test requires genuine records and launches no engine/renderer.
use super::*;

fn controlled_application(state: JobState, pending_evidence: bool) -> Application {
    Application {
        active: Mutex::new(Some(ActiveJob {
            status: Arc::new(Mutex::new(JobStatus {
                id: "original-control-state-only".into(),
                state,
                log: vec!["No engine/result fixture; readiness control only".into()],
                error: None,
                result: None,
                climate_result: None,
                energy_result: None,
                pending_evidence,
            })),
            cancel: Arc::new(AtomicBool::new(false)),
            handle: None,
            scratch: PathBuf::from("unused-readiness-control"),
            project_id: None,
        })),
    }
}

#[test]
fn report_readiness_allows_a_cold_session_without_changing_status_semantics() {
    let app = Application::default();
    assert!(app.ensure_report_ready().is_ok());
    assert_eq!(
        app.status().unwrap_err().to_string(),
        "No simulation has been started"
    );
    assert!(app.active.lock().unwrap().is_none());
}

#[test]
fn report_readiness_rejects_live_and_pending_jobs_without_consuming_them() {
    for state in [JobState::Queued, JobState::Preparing, JobState::Running] {
        let app = controlled_application(state, false);
        let before = canonical_hash(&app.status().unwrap()).unwrap();
        assert!(app
            .ensure_report_ready()
            .unwrap_err()
            .to_string()
            .contains("Finish or cancel"));
        assert_eq!(canonical_hash(&app.status().unwrap()).unwrap(), before);
        assert!(!app
            .active
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .cancel
            .load(Ordering::SeqCst));
    }
    let app = controlled_application(JobState::Completed, true);
    let before = canonical_hash(&app.status().unwrap()).unwrap();
    assert!(app
        .ensure_report_ready()
        .unwrap_err()
        .to_string()
        .contains("waiting to be saved"));
    assert_eq!(canonical_hash(&app.status().unwrap()).unwrap(), before);
}

#[test]
fn report_readiness_allows_terminal_collected_jobs_but_never_ignores_pending_evidence() {
    for state in [JobState::Completed, JobState::Failed, JobState::Cancelled] {
        let app = controlled_application(state.clone(), false);
        let before = canonical_hash(&app.status().unwrap()).unwrap();
        app.ensure_report_ready().unwrap();
        assert_eq!(canonical_hash(&app.status().unwrap()).unwrap(), before);
        let pending = controlled_application(state, true);
        assert!(pending.ensure_report_ready().is_err());
        assert!(pending.status().unwrap().pending_evidence);
    }
}

#[test]
fn report_readiness_fails_closed_for_poisoned_job_and_status_mutexes() {
    let app = Application::default();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _held = app.active.lock().unwrap();
        panic!("controlled mutex poison; no child process");
    }));
    assert!(panic.is_err());
    let error = app.ensure_report_ready().unwrap_err().to_string();
    app.active.clear_poison(); // Existing Drop/status semantics are unchanged.
    assert!(error.contains("job mutex poisoned"));

    let app = controlled_application(JobState::Completed, false);
    let status = app.active.lock().unwrap().as_ref().unwrap().status.clone();
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _held = status.lock().unwrap();
        panic!("controlled status poison; no child process");
    }));
    assert!(panic.is_err());
    let error = app.ensure_report_ready().unwrap_err().to_string();
    status.clear_poison();
    assert!(error.contains("job status mutex poisoned"));
}

fn member_hashes(root: &Path) -> std::collections::BTreeMap<String, String> {
    fn visit(root: &Path, here: &Path, members: &mut std::collections::BTreeMap<String, String>) {
        for entry in fs::read_dir(here).unwrap() {
            let entry = entry.unwrap();
            assert!(!entry.file_type().unwrap().is_symlink());
            if entry.file_type().unwrap().is_dir() {
                visit(root, &entry.path(), members);
            } else if entry.file_name() != ".greenlens.lock" {
                members.insert(
                    entry
                        .path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    persistence::bytes_hash(&fs::read(entry.path()).unwrap()),
                );
            }
        }
    }
    let mut members = std::collections::BTreeMap::new();
    visit(root, root, &mut members);
    members
}

#[test]
#[ignore = "Requires genuine retained A/B evidence; no engine/render execution. Set GREENLENS_TEST_RETAINED_ENERGY_PROJECT and exact A/B run IDs"]
fn cold_retained_report_preparation_and_genuine_pending_collection() {
    let root = PathBuf::from(
        std::env::var("GREENLENS_TEST_RETAINED_ENERGY_PROJECT")
            .expect("Required genuine retained energy project is missing"),
    );
    let a = std::env::var("GREENLENS_TEST_RETAINED_ENERGY_A")
        .expect("Declare the exact retained baseline A run ID");
    let b = std::env::var("GREENLENS_TEST_RETAINED_ENERGY_B")
        .expect("Declare the exact retained proposed B run ID");
    let before = member_hashes(&root);
    let view = open_project(&root).expect("Genuine retained project must pass full verification");
    let app = Application::default();
    app.ensure_report_ready().unwrap();
    let content = design_report_service::prepare(
        &root, &view.project, &a, &b,
        "Original retained-evidence regression; supported analytical scope only, no physical qualification claimed.".into(),
        None, false, Arc::new(AtomicBool::new(false)),
    ).expect("Cold session must prepare exact retained A/B report data without starting a job");
    assert_eq!(content.workbench.base.baseline.run_id, a);
    assert_eq!(content.workbench.base.proposed.run_id, b);
    assert!(app.active.lock().unwrap().is_none());

    let run = view
        .energy_results
        .iter()
        .find(|run| run.id == b)
        .expect("Declared genuine proposed record was verified")
        .clone();
    // Restore a completed-job control state from that exact verified record.
    // This is evidence-preservation coverage, not a new native calculation.
    let pending = controlled_application(JobState::Completed, true);
    {
        let mut active = pending.active.lock().unwrap();
        let job = active.as_mut().unwrap();
        job.scratch = root.join("runs").join(&run.id);
        job.project_id = Some(view.project.id.clone());
        let mut status = job.status.lock().unwrap();
        status.id = run.id.clone();
        status.energy_result = Some(run.clone());
    }
    let status_before = canonical_hash(&pending.status().unwrap()).unwrap();
    assert!(pending.ensure_report_ready().is_err());
    assert_eq!(
        canonical_hash(&pending.status().unwrap()).unwrap(),
        status_before
    );
    let verification = local_root().unwrap().join("verification");
    fs::create_dir_all(&verification).unwrap();
    let temporary = tempfile::Builder::new()
        .prefix("report-readiness-")
        .tempdir_in(verification)
        .unwrap();
    let destination = temporary.path().join("complete retained copy");
    let copied = save_project_as(
        Some(&root),
        &destination,
        &view.project,
        Some(&view.revision),
    )
    .unwrap();
    let collected = pending
        .collect(&destination, &copied.project, Some(&copied.revision))
        .unwrap();
    pending.ensure_report_ready().unwrap();
    assert!(!pending.status().unwrap().pending_evidence);
    assert_eq!(
        canonical_hash(collected.energy_results.iter().find(|r| r.id == b).unwrap()).unwrap(),
        canonical_hash(&run).unwrap()
    );
    for (name, hash) in &before {
        if name != "project.json" {
            assert_eq!(
                persistence::bytes_hash(&fs::read(destination.join(name)).unwrap()),
                *hash
            );
        }
    }
    assert_eq!(member_hashes(&root), before);
}
