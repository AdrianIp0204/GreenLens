//! Required explicit native test, not run by the dependency-free unit suite.
//! One full A run tests conflict recovery, followed by one cancelled process.
use greenlens_runner::{energy, energy_services as service, Application, JobState};
use std::{
    fs, thread,
    time::{Duration, Instant},
};

#[test]
#[ignore = "Requires pinned native EnergyPlus; release CI invokes explicitly"]
fn native_energy_conflict_copy_cancel_and_staleness() {
    greenlens_energyplus::EngineConfig::discover()
        .expect("Required EnergyPlus unavailable")
        .verify()
        .unwrap();
    let root = greenlens_runner::local_root()
        .unwrap()
        .join("verification")
        .join(format!("energy lifecycle {} 空間", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let original = root.join("original");
    let initial = service::create_example(&original).unwrap();
    let app = Application::default();
    service::start(&app, &original, &initial.project, "A", None).unwrap();
    assert!(service::start(&app, &original, &initial.project, "B", None).is_err());
    let mut external = initial.project.clone();
    external.name = "External writer title must survive".into();
    let edited =
        greenlens_runner::save_project(&original, &external, Some(&initial.revision)).unwrap();
    let disk_before = fs::read(original.join("project.json")).unwrap();
    let completed = app.wait().unwrap();
    assert_eq!(
        completed.state,
        JobState::Completed,
        "{:?}",
        completed.error
    );
    assert!(completed.pending_evidence);
    let run = completed.energy_result.as_ref().unwrap();
    assert_eq!(run.series.interval_count, 52_560);
    assert!(app
        .collect(&original, &initial.project, Some(&initial.revision))
        .is_err());
    assert_eq!(
        disk_before,
        fs::read(original.join("project.json")).unwrap()
    );
    assert!(app.status().unwrap().pending_evidence);
    assert!(app.prepare_close(false).is_err());
    assert!(service::start(&app, &original, &initial.project, "B", None).is_err());

    let copy = root.join("recovered copy");
    let recovered = app
        .save_as(Some(&original), &copy, &initial.project, true)
        .unwrap();
    assert!(!app.status().unwrap().pending_evidence);
    assert_eq!(recovered.energy_results.len(), 1);
    // Publication must not relabel a pre-publication verification receipt as an
    // opening of the new manifest. Inspect the published copy explicitly.
    assert!(recovered.project_verification.is_none());
    let reopened = greenlens_runner::open_project(&copy).unwrap();
    let opening = reopened
        .project_verification
        .as_ref()
        .expect("Verified open receipt");
    assert_eq!(opening.progress.numeric_passes.len(), 1);
    assert_eq!(opening.progress.numeric_passes.get(&run.id), Some(&1));
    assert_eq!(
        disk_before,
        fs::read(original.join("project.json")).unwrap()
    );
    let reconciled = app
        .reconcile_completed_run(&original, &edited.revision)
        .unwrap();
    assert_eq!(reconciled.project.name, external.name);
    assert_eq!(reconciled.energy_results.len(), 1);
    let sealed = fs::read(copy.join("runs").join(&run.id).join("results.json")).unwrap();
    assert_eq!(
        sealed,
        fs::read(original.join("runs").join(&run.id).join("results.json")).unwrap()
    );
    let extension = recovered.project.energy.as_ref().unwrap();
    let original_design = extension.designs["A"].clone();
    let mut design = original_design.clone();
    assert!(
        energy::current_result(&design, &recovered.energy_results, "A")
            .unwrap()
            .is_some()
    );
    design.model.zones[0].gains.lighting_w_per_floor_m2 += 1.;
    assert!(
        energy::current_result(&design, &recovered.energy_results, "A")
            .unwrap()
            .is_none()
    );
    assert!(
        energy::current_result(&original_design, &recovered.energy_results, "A")
            .unwrap()
            .is_some()
    );
    let rows = service::rows(&copy, &recovered.project, &run.id, 100, 6).unwrap();
    assert_eq!(rows["rows"].as_array().unwrap().len(), 6);
    assert_eq!(rows["demand_10min"].as_array().unwrap().len(), 6);
    assert!(service::rows(&copy, &recovered.project, &run.id, 52_560, 1).is_err());

    let cancelled_app = Application::default();
    service::start(&cancelled_app, &copy, &recovered.project, "B", None).unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let s = cancelled_app.status().unwrap();
        if s.log
            .iter()
            .any(|line| line.contains("Executing the declared EnergyPlus"))
        {
            break;
        }
        assert!(
            !matches!(
                s.state,
                JobState::Failed | JobState::Completed | JobState::Cancelled
            ),
            "{s:?}"
        );
        assert!(Instant::now() < deadline, "EnergyPlus did not start");
        thread::sleep(Duration::from_millis(20));
    }
    thread::sleep(Duration::from_millis(250));
    cancelled_app.cancel().unwrap();
    let cancelled = cancelled_app.wait().unwrap();
    assert_eq!(cancelled.state, JobState::Cancelled);
    assert!(cancelled.energy_result.is_none() && !cancelled.pending_evidence);
    assert!(cancelled_app
        .collect(&copy, &recovered.project, Some(&recovered.revision))
        .is_err());
    assert!(!copy.join("runs").join(&cancelled.id).exists());
    let process: serde_json::Value = serde_json::from_slice(
        &fs::read(
            greenlens_runner::local_root()
                .unwrap()
                .join("jobs")
                .join(&cancelled.id)
                .join("engine/process.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(process["process_tree_empty"], true, "{process}");
    let missing = Application::default();
    service::start(
        &missing,
        &copy,
        &recovered.project,
        "B",
        Some(root.join("missing-engine")),
    )
    .unwrap();
    let failed = missing.wait().unwrap();
    assert_eq!(failed.state, JobState::Failed);
    assert!(failed.energy_result.is_none() && !failed.pending_evidence);
    let evidence = serde_json::json!({"schema":"greenlens.energy-native-lifecycle.v1","completed_run_id":run.id,"completed_method":run.method,"completed_physical_input_hash":run.provenance.physical_input_hash,"source_commit":run.provenance.source_commit,"source_dirty":run.provenance.source_worktree_dirty,"conflict_preserved":true,"pending_close_rejected":true,"pending_save_as_complete":true,"external_title_reconciled":true,"immutable_run_bytes_equal":true,"edit_stale_undo_current":true,"bounded_native_demand_rows":true,"cancelled_id":cancelled.id,"cancelled_process":process,"missing_engine_failed":true,"scientific_qualification_asserted":false});
    fs::write(
        root.join("lifecycle.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
    println!("{}", root.display());
}
