//! Explicitly synthetic lifecycle records. These tests are never engine or
//! independent scientific acceptance evidence.
use super::*;
use tempfile::tempdir;

fn mock_record(model: Model, alternative: &str) -> RunRecord {
    let validated = validate_model(&model).unwrap();
    let profile = Profile::shipping();
    let engine = json!({"version":greenlens_radiance::ENGINE_VERSION,"adapter_version":greenlens_radiance::ADAPTER_VERSION,
        "executable_sha256":{"gensky.exe":"0".repeat(64),"oconv.exe":"0".repeat(64),"rtrace.exe":"0".repeat(64)},
        "library_sha256":{"lib/mock.cal":"0".repeat(64)},"fingerprint":"MOCK: never engine acceptance"});
    let processes = ["gensky.exe", "oconv.exe", "rtrace.exe"].map(|name| json!({"executable":name,"arguments":if name=="rtrace.exe" {profile.arguments()} else {vec![]},"exit_code":0,"started_utc":"2026-01-01T00:00:00Z","finished_utc":"2026-01-01T00:00:01Z","elapsed_ms":1000,"stderr":"MOCK: no process was executed"}));
    let provenance = json!({"schema_version":SCHEMA_VERSION,"application_version":"0.1.0","adapter_version":greenlens_radiance::ADAPTER_VERSION,"calculation_label":CALCULATION_LABEL,
        "test_fixture":"MOCK; lifecycle only","state":"completed","units":"lx","engine":engine,"profile":profile,"processes":processes,
        "started_utc":"2026-01-01T00:00:00Z","finished_utc":"2026-01-01T00:00:01Z","input_hash":validated.normalized_input_hash,"sensor_hash":validated.sensor_hash,
        "run_fingerprint":canonical_hash(&json!({"input":validated.scientific_input_hash,"engine":engine,"profile":profile})).unwrap(),
        "comparison_fingerprint":canonical_hash(&json!({"conditions":validated.comparison_conditions_hash,"engine":engine,"profile":profile})).unwrap()});
    let lux = vec![179.; validated.sensor_grid.sensors.len()];
    RunRecord {
        id: Uuid::new_v4().to_string(),
        alternative_id: alternative.into(),
        model,
        validated,
        statistics: serde_json::to_value(statistics(&lux).unwrap()).unwrap(),
        lux,
        provenance,
    }
}
fn write_mock_run(root: &Path, run: &RunRecord) {
    fs::create_dir_all(root).unwrap();
    write_json(&root.join("input.json"), &run.model).unwrap();
    write_json(&root.join("results.json"), run).unwrap();
    write_json(&root.join("provenance.json"), &run.provenance).unwrap();
    write_json(&root.join("logs.json"), &run.provenance["processes"]).unwrap();
    for name in [
        "scene.rad",
        "sky.rad",
        "sensors.pts",
        "results.csv",
        "recipe.json",
    ] {
        fs::write(root.join(name), "MOCK fixture; never executed\n").unwrap();
    }
    fs::write(root.join("raw.rgb"), "1 1 1\n".repeat(run.lux.len())).unwrap();
    seal(
        root,
        &[
            "input.json",
            "results.json",
            "provenance.json",
            "logs.json",
            "scene.rad",
            "sky.rad",
            "sensors.pts",
            "results.csv",
            "recipe.json",
            "raw.rgb",
        ],
    )
    .unwrap();
}
fn completed_app(scratch: &Path, project: &Project, run: RunRecord) -> Application {
    write_mock_run(scratch, &run);
    Application {
        active: Mutex::new(Some(ActiveJob {
            status: Arc::new(Mutex::new(JobStatus {
                id: run.id.clone(),
                state: JobState::Completed,
                log: vec!["MOCK completed lifecycle".into()],
                error: None,
                result: Some(run),
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
fn external_revision_conflict_preserves_pending_evidence_until_explicit_reconciliation() {
    let project_dir = tempdir().unwrap();
    let scratch = tempdir().unwrap();
    let original = save_project(project_dir.path(), &example(), None).unwrap();
    let run = mock_record(original.project.alternatives[0].model.clone(), "A");
    let immutable = serde_json::to_value(&run).unwrap();
    let app = completed_app(scratch.path(), &original.project, run);
    let mut external = original.project.clone();
    external.name = "Externally edited project".into();
    external.alternatives[0].model.window.width_m = 2.;
    atomic_json(project_dir.path(), &external).unwrap();
    assert!(app
        .collect(
            project_dir.path(),
            &original.project,
            Some(&original.revision)
        )
        .is_err());
    assert!(app.status().unwrap().pending_evidence);
    assert!(app
        .start(Model::default(), "B".into(), None, Profile::shipping())
        .is_err());
    assert!(app.prepare_close(false).is_err());
    let fresh = open_project(project_dir.path()).unwrap();
    let reconciled = app
        .reconcile_completed_run(project_dir.path(), &fresh.revision)
        .unwrap();
    assert_eq!(reconciled.project.name, external.name);
    assert_eq!(reconciled.project.alternatives[0].model.window.width_m, 2.);
    assert_eq!(
        serde_json::to_value(&reconciled.results[0]).unwrap(),
        immutable
    );
    assert!(!app.status().unwrap().pending_evidence);
    assert!(is_stale(&reconciled.results[0], &external.alternatives[0].model).unwrap());
}

#[test]
fn save_as_recovers_unsaved_models_all_referenced_runs_and_pending_run_after_external_edit() {
    let source = tempdir().unwrap();
    let old_scratch = tempdir().unwrap();
    let pending_scratch = tempdir().unwrap();
    let destination = tempdir().unwrap();
    let mut project = example();
    let a = mock_record(Model::default(), "A");
    write_mock_run(old_scratch.path(), &a);
    promote_run(old_scratch.path(), source.path(), &a.id).unwrap();
    append_run(&mut project, &a).unwrap();
    let saved = save_project(source.path(), &project, None).unwrap();
    let b = mock_record(Model::alternative_b(), "B");
    let app = completed_app(pending_scratch.path(), &project, b.clone());
    let mut external = project.clone();
    external.name = "External name must remain untouched".into();
    atomic_json(source.path(), &external).unwrap();
    project.name = "Local unsaved name".into();
    project.alternatives[0].model.window.width_m = 2.1;
    let copied = app
        .save_as(Some(source.path()), destination.path(), &project, true)
        .unwrap();
    assert_ne!(copied.revision, saved.revision);
    assert_eq!(copied.project.name, "Local unsaved name");
    assert_eq!(copied.project.alternatives[0].model.window.width_m, 2.1);
    assert_eq!(copied.results.len(), 2);
    assert_eq!(
        open_project(source.path()).unwrap().project.name,
        external.name
    );
    assert_eq!(open_project(source.path()).unwrap().project.runs.len(), 1);
    assert_eq!(
        serde_json::to_value(&copied.results[0]).unwrap(),
        serde_json::to_value(&a).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&copied.results[1]).unwrap(),
        serde_json::to_value(&b).unwrap()
    );
    for (original, run) in [(old_scratch.path(), &a), (pending_scratch.path(), &b)] {
        let copied_run = destination.path().join("runs").join(&run.id);
        for entry in fs::read_dir(original).unwrap() {
            let entry = entry.unwrap();
            assert_eq!(
                fs::read(entry.path()).unwrap(),
                fs::read(copied_run.join(entry.file_name())).unwrap(),
                "Save As must preserve every immutable evidence file byte-for-byte"
            );
        }
    }
    assert!(!app.status().unwrap().pending_evidence);
    drop(source);
    drop(old_scratch);
    drop(pending_scratch);
    let reopened = open_project(destination.path()).unwrap();
    assert_eq!(reopened.results.len(), 2);
    let output = tempdir().unwrap();
    let archive = output.path().join("portable.zip");
    export_project(destination.path(), &archive).unwrap();
    let extracted = output.path().join("unpacked");
    extract_package(&archive, &extracted).unwrap();
    assert_eq!(open_project(&extracted).unwrap().results.len(), 2);
}

#[test]
fn failed_save_as_or_foreign_reconciliation_never_discards_completed_evidence() {
    let source = tempdir().unwrap();
    let scratch = tempdir().unwrap();
    let blocked = tempdir().unwrap();
    let saved = save_project(source.path(), &example(), None).unwrap();
    let app = completed_app(
        scratch.path(),
        &saved.project,
        mock_record(Model::default(), "A"),
    );
    fs::write(blocked.path().join("unrelated.txt"), "preserve me").unwrap();
    assert!(app
        .save_as(Some(source.path()), blocked.path(), &saved.project, true)
        .is_err());
    assert_eq!(
        fs::read_to_string(blocked.path().join("unrelated.txt")).unwrap(),
        "preserve me"
    );
    assert!(app.status().unwrap().pending_evidence);
    let foreign = tempdir().unwrap();
    let view = save_project(foreign.path(), &example(), None).unwrap();
    assert!(app
        .reconcile_completed_run(foreign.path(), &view.revision)
        .is_err());
    assert!(app.status().unwrap().pending_evidence);
    app.discard_job().unwrap();
    assert!(app.status().is_err());
}

#[test]
fn historical_comparisons_select_latest_compatible_pair_in_linear_groups() {
    let mut records = Vec::new();
    for _ in 0..20 {
        records.push(mock_record(Model::default(), "A"));
        records.push(mock_record(Model::alternative_b(), "B"));
    }
    let comparisons = latest_compatible_comparisons(&records).unwrap();
    assert_eq!(comparisons.len(), 1);
    assert_eq!(comparisons[0].a_id, records[38].id);
    assert_eq!(comparisons[0].b_id, records[39].id);
}

#[test]
fn legacy_project_display_roundtrip_never_changes_scientific_inputs_or_scene() {
    let directory = tempdir().unwrap();
    let original = example();
    let legacy = serde_json::to_value(&original).unwrap();
    assert!(legacy.get("display_state").is_none());
    let mut project: Project = serde_json::from_value(legacy).unwrap();
    let before = validate_model(&project.alternatives[0].model).unwrap();
    let scene = greenlens_radiance::scene_text(&before).unwrap();
    let layers = [
        "room",
        "window",
        "overhang",
        "obstruction",
        "sensors",
        "analysis",
    ]
    .into_iter()
    .map(|id| {
        (
            id.to_string(),
            json!({"visible":false,"locked":true,"color":"#123456"}),
        )
    })
    .collect::<serde_json::Map<_, _>>();
    project.display_state = Some(serde_json::from_value(json!({"schema_version":display::DISPLAY_SCHEMA_VERSION,"active_view":"top","maximized_view":"top","mode":"wireframe","cameras":{},"layers":layers,"objects":{"window":{"visible":false,"locked":true},"sensor:s000001":{"visible":false}},"grid_visible":true,"grid_snap":true,"object_snap":true,"ortho":true,"gumball":false})).unwrap());
    assert!(project.display_state.as_ref().unwrap().cutaway);
    let saved = save_project(directory.path(), &project, None).unwrap();
    assert_eq!(saved.project.display_state, project.display_state);
    let after = validate_model(&saved.project.alternatives[0].model).unwrap();
    assert_eq!(before.scientific_input_hash, after.scientific_input_hash);
    assert_eq!(scene, greenlens_radiance::scene_text(&after).unwrap());
    project.display_state.as_mut().unwrap().cutaway = false;
    let opaque = save_project(directory.path(), &project, Some(&saved.revision)).unwrap();
    assert!(!opaque.project.display_state.as_ref().unwrap().cutaway);
    let after_cutaway = validate_model(&opaque.project.alternatives[0].model).unwrap();
    assert_eq!(
        before.scientific_input_hash,
        after_cutaway.scientific_input_hash
    );
    assert_eq!(
        scene,
        greenlens_radiance::scene_text(&after_cutaway).unwrap()
    );
    let state = project.display_state.as_mut().unwrap();
    state.objects.insert(
        "arbitrary-mesh:1".into(),
        display::ObjectDisplayState {
            visible: Some(false),
            locked: None,
            color: None,
        },
    );
    assert!(state.validate().is_err());
    state.objects.remove("arbitrary-mesh:1");
    state.cameras.insert(
        display::ViewId::Top,
        display::CameraState {
            position: [0., 0., 1.],
            target: [0., 0., 0.],
            up: [0., 0., 1.],
            zoom: 1.,
        },
    );
    assert!(state.validate().is_err());
}
