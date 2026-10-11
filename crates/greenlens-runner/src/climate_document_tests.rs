//! Synthetic document lifecycle tests only. No engine, process-tree or scientific
//! acceptance claim is made by these deliberately constructed completed records.
use super::*;
use climate::ClimateRunRecord;
use tempfile::tempdir;

fn save_fixture(root: &Path, record: &ClimateRunRecord) {
    write_json(&root.join("results.json"), record).unwrap();
    write_json(&root.join("provenance.json"), &record.provenance).unwrap();
    let names = climate::evidence_files(record);
    seal(root, &names.iter().map(String::as_str).collect::<Vec<_>>()).unwrap();
}

fn completed_app(scratch: &Path, project: &Project, run: ClimateRunRecord) -> Application {
    Application {
        active: Mutex::new(Some(ActiveJob {
            status: Arc::new(Mutex::new(JobStatus {
                id: run.id.clone(),
                state: JobState::Completed,
                log: vec!["MOCK climate lifecycle; no engine executed".into()],
                error: None,
                result: None,
                climate_result: Some(run),
                pending_evidence: true,
            })),
            cancel: Arc::new(AtomicBool::new(false)),
            handle: None,
            scratch: scratch.to_owned(),
            project_id: Some(project.id.clone()),
        })),
    }
}

#[test]
fn external_conflict_preserves_climate_evidence_until_explicit_reconcile() {
    let root = tempdir().unwrap();
    let scratch = tempdir().unwrap();
    let original = save_project(root.path(), &example(), None).unwrap();
    let record = climate::tests::fixture(scratch.path(), false);
    let immutable = canonical_hash(&record).unwrap();
    let app = completed_app(scratch.path(), &original.project, record);
    let mut external = original.project.clone();
    external.name = "Externally edited climate document".into();
    external.alternatives[0].model.window.width_m = 2.0;
    atomic_json(root.path(), &external).unwrap();
    assert!(app
        .collect(root.path(), &original.project, Some(&original.revision))
        .is_err());
    assert!(app.status().unwrap().pending_evidence);
    assert!(app.prepare_close(false).is_err());
    assert!(app
        .start(Model::default(), "B".into(), None, Profile::shipping())
        .is_err());
    assert!(open_project(root.path())
        .unwrap()
        .climate_results
        .is_empty());

    let fresh = open_project(root.path()).unwrap();
    let reconciled = app
        .reconcile_completed_run(root.path(), &fresh.revision)
        .unwrap();
    assert_eq!(reconciled.project.name, external.name);
    assert_eq!(reconciled.project.alternatives[0].model.window.width_m, 2.0);
    assert_eq!(
        canonical_hash(&reconciled.climate_results[0]).unwrap(),
        immutable
    );
    assert_eq!(reconciled.project.schema_version, "0.3.0");
    assert_eq!(
        reconciled.project.climate.as_ref().unwrap().weather.len(),
        1
    );
    assert!(!app.status().unwrap().pending_evidence);
}

#[test]
fn save_as_copies_current_weather_history_pending_evidence_and_unsaved_models() {
    let source = tempdir().unwrap();
    let historical_scratch = tempdir().unwrap();
    let explicit_scratch = tempdir().unwrap();
    let repeated_scratch = tempdir().unwrap();
    let pending_scratch = tempdir().unwrap();
    let destination = tempdir().unwrap();
    let initial = save_project(source.path(), &example(), None).unwrap();
    let a = climate::tests::fixture_for_method(
        historical_scratch.path(),
        false,
        greenlens_radiance::annual::LEGACY_METHOD_VERSION,
    );
    let first = completed_app(historical_scratch.path(), &initial.project, a.clone());
    let saved = first
        .collect(source.path(), &initial.project, Some(&initial.revision))
        .unwrap();
    drop(first);
    let historical_explicit = climate::tests::fixture_for_method(
        explicit_scratch.path(),
        false,
        greenlens_radiance::annual::EXPLICIT_METHOD_VERSION,
    );
    let second = completed_app(
        explicit_scratch.path(),
        &saved.project,
        historical_explicit.clone(),
    );
    let saved = second
        .collect(source.path(), &saved.project, Some(&saved.revision))
        .unwrap();
    drop(second);
    let historical_repeated = climate::tests::fixture_for_method(
        repeated_scratch.path(),
        false,
        greenlens_radiance::annual::REPEATED_METHOD_VERSION,
    );
    let third = completed_app(
        repeated_scratch.path(),
        &saved.project,
        historical_repeated.clone(),
    );
    let saved = third
        .collect(source.path(), &saved.project, Some(&saved.revision))
        .unwrap();
    drop(third);
    let mut b = climate::tests::fixture(pending_scratch.path(), false);
    b.alternative_id = "B".into();
    save_fixture(pending_scratch.path(), &b);
    let app = completed_app(pending_scratch.path(), &saved.project, b.clone());
    let mut local = studies::configure(&saved.project, "A", a.study.clone()).unwrap();
    local = studies::configure(&local, "B", b.study.clone()).unwrap();
    local.name = "Unsaved local climate edits".into();
    local.alternatives[0].model.window.width_m = 2.1;
    let mut external = saved.project.clone();
    external.name = "External source name remains untouched".into();
    atomic_json(source.path(), &external).unwrap();

    let copied = app
        .save_as(Some(source.path()), destination.path(), &local, true)
        .unwrap();
    assert_eq!(copied.project.name, local.name);
    assert_eq!(copied.project.alternatives[0].model.window.width_m, 2.1);
    assert_eq!(copied.climate_results.len(), 4);
    assert_eq!(
        copied.project.climate.as_ref().unwrap().studies,
        local.climate.as_ref().unwrap().studies
    );
    assert_eq!(
        open_project(source.path()).unwrap().project.name,
        external.name
    );
    assert_eq!(
        open_project(source.path()).unwrap().climate_results.len(),
        3
    );
    for (scratch, record) in [
        (historical_scratch.path(), &a),
        (explicit_scratch.path(), &historical_explicit),
        (repeated_scratch.path(), &historical_repeated),
        (pending_scratch.path(), &b),
    ] {
        for relative in climate::evidence_files(record)
            .into_iter()
            .chain(["checksums.json".into()])
        {
            assert_eq!(
                fs::read(scratch.join(&relative)).unwrap(),
                fs::read(
                    destination
                        .path()
                        .join("runs")
                        .join(&record.id)
                        .join(&relative)
                )
                .unwrap(),
                "Immutable climate evidence must be copied byte-for-byte: {relative}"
            );
        }
    }
    assert!(!app.status().unwrap().pending_evidence);
    drop(app);
    drop(source);
    drop(historical_scratch);
    drop(explicit_scratch);
    drop(repeated_scratch);
    drop(pending_scratch);

    let reopened = open_project(destination.path()).unwrap();
    assert_eq!(reopened.climate_results.len(), 4);
    for (actual, expected) in
        reopened
            .climate_results
            .iter()
            .zip([&a, &historical_explicit, &historical_repeated, &b])
    {
        assert_eq!(
            canonical_hash(actual).unwrap(),
            canonical_hash(expected).unwrap()
        );
    }
    let extension = reopened.project.climate.as_ref().unwrap();
    for reference in &extension.weather {
        assert_eq!(
            weather::open(&destination.path().join(&reference.path))
                .unwrap()
                .0
                .binding,
            reference.binding
        );
    }
    let output = tempdir().unwrap();
    let archive = output.path().join("portable-climate.zip");
    climate_export::export_selected(
        destination.path(),
        &archive,
        &[
            a.id.clone(),
            historical_explicit.id.clone(),
            historical_repeated.id.clone(),
            b.id.clone(),
        ],
    )
    .unwrap();
    let extracted = output.path().join("extracted");
    extract_package(&archive, &extracted).unwrap();
    let exported = open_project(&extracted).unwrap();
    assert_eq!(exported.climate_results.len(), 4);
    for (actual, expected) in
        exported
            .climate_results
            .iter()
            .zip([&a, &historical_explicit, &historical_repeated, &b])
    {
        assert_eq!(
            canonical_hash(actual).unwrap(),
            canonical_hash(expected).unwrap()
        );
        for relative in climate::evidence_files(expected)
            .into_iter()
            .chain(["checksums.json".into()])
        {
            assert_eq!(
                fs::read(
                    destination
                        .path()
                        .join("runs")
                        .join(&expected.id)
                        .join(&relative)
                )
                .unwrap(),
                fs::read(extracted.join("runs").join(&expected.id).join(&relative)).unwrap(),
            );
        }
    }
}

#[test]
fn coherently_resealed_pending_record_cannot_replace_completed_job_snapshot() {
    let root = tempdir().unwrap();
    let scratch = tempdir().unwrap();
    let destination = tempdir().unwrap();
    let saved = save_project(root.path(), &example(), None).unwrap();
    let record = climate::tests::fixture(scratch.path(), false);
    let app = completed_app(scratch.path(), &saved.project, record.clone());
    let mut changed = record;
    changed.provenance["test_fixture"] = json!("MOCK evidence altered after completion");
    save_fixture(scratch.path(), &changed);
    climate::read_run_directory(scratch.path()).unwrap();
    let original_manifest = fs::read(root.path().join("project.json")).unwrap();
    let failure = app
        .collect(root.path(), &saved.project, Some(&saved.revision))
        .unwrap_err();
    assert!(failure
        .to_string()
        .contains("does not match the completed job"));
    assert_eq!(
        fs::read(root.path().join("project.json")).unwrap(),
        original_manifest
    );
    assert!(open_project(root.path())
        .unwrap()
        .climate_results
        .is_empty());
    assert!(app
        .save_as(Some(root.path()), destination.path(), &saved.project, true)
        .is_err());
    assert!(!destination.path().join("project.json").exists());
    assert!(app.status().unwrap().pending_evidence);
}

#[test]
fn foreign_project_or_failed_destination_preserves_pending_climate_record() {
    let source = tempdir().unwrap();
    let foreign = tempdir().unwrap();
    let scratch = tempdir().unwrap();
    let destination = tempdir().unwrap();
    let saved = save_project(source.path(), &example(), None).unwrap();
    let unrelated = save_project(foreign.path(), &example(), None).unwrap();
    let record = climate::tests::fixture(scratch.path(), false);
    let app = completed_app(scratch.path(), &saved.project, record);
    assert!(app
        .reconcile_completed_run(foreign.path(), &unrelated.revision)
        .is_err());
    assert!(app
        .save_as(
            Some(foreign.path()),
            destination.path(),
            &unrelated.project,
            true
        )
        .is_err());
    fs::write(destination.path().join("unrelated.txt"), "retain this file").unwrap();
    assert!(app
        .save_as(
            Some(source.path()),
            destination.path(),
            &saved.project,
            true
        )
        .is_err());
    assert_eq!(
        fs::read_to_string(destination.path().join("unrelated.txt")).unwrap(),
        "retain this file"
    );
    assert!(app.status().unwrap().pending_evidence);
    assert!(open_project(foreign.path())
        .unwrap()
        .climate_results
        .is_empty());
    app.prepare_close(true).unwrap();
    assert!(app.status().is_err());
}

#[test]
fn orderly_cancellation_joins_synthetic_worker_without_publishing_evidence() {
    let root = tempdir().unwrap();
    let scratch = tempdir().unwrap();
    let saved = save_project(root.path(), &example(), None).unwrap();
    let status = Arc::new(Mutex::new(JobStatus {
        id: Uuid::new_v4().to_string(),
        state: JobState::Running,
        log: vec!["MOCK worker; no engine subprocess exists".into()],
        error: None,
        result: None,
        climate_result: None,
        pending_evidence: false,
    }));
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    let worker_status = status.clone();
    let handle = thread::spawn(move || {
        while !worker_cancel.load(Ordering::SeqCst) {
            thread::park_timeout(std::time::Duration::from_millis(1));
        }
        worker_status.lock().unwrap().state = JobState::Cancelled;
    });
    let app = Application {
        active: Mutex::new(Some(ActiveJob {
            status: status.clone(),
            cancel: cancel.clone(),
            handle: Some(handle),
            scratch: scratch.path().to_owned(),
            project_id: Some(saved.project.id.clone()),
        })),
    };
    assert!(app
        .collect(root.path(), &saved.project, Some(&saved.revision))
        .is_err());
    app.prepare_close(false).unwrap();
    assert!(cancel.load(Ordering::SeqCst));
    let final_status = status.lock().unwrap();
    assert_eq!(final_status.state, JobState::Cancelled);
    assert!(final_status.result.is_none() && final_status.climate_result.is_none());
    assert!(!final_status.pending_evidence);
    assert!(open_project(root.path())
        .unwrap()
        .climate_results
        .is_empty());
    assert!(app.status().is_err());
}
