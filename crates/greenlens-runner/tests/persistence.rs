//! Persistence integrity tests use explicitly synthetic text evidence.
//! They are not Radiance execution or scientific acceptance evidence.

use greenlens_runner::persistence::{
    atomic_json, check_revision, extract_package, promote_run, relative_path, seal, verify_seal,
    zip_directory, WriterLock,
};
use greenlens_runner::{example, open_project, save_project};
use std::{fs, io::Write, path::Path};
use tempfile::tempdir;

#[test]
fn growing_or_unending_evidence_stream_is_bounded_during_copy() {
    let mut output = Vec::new();
    let error =
        greenlens_runner::persistence::copy_bounded(&mut std::io::repeat(7), &mut output, 31)
            .unwrap_err();
    assert!(error.to_string().contains("exceeded"));
    assert_eq!(output.len(), 32); // one probe byte, never an unbounded copy
    let mut exact = Vec::new();
    assert_eq!(
        greenlens_runner::persistence::copy_bounded(&mut &output[..31], &mut exact, 31).unwrap(),
        31
    );
    assert_eq!(exact, output[..31]);
}

fn mock_compact_evidence(root: &Path) {
    fs::create_dir_all(root).unwrap();
    // Deliberately marked mocks: these exercise file handling only.
    fs::write(root.join("input.json"), "{\"mock\":true}\n").unwrap();
    fs::write(root.join("raw.rgb"), "# MOCK; not engine output\n").unwrap();
    fs::write(root.join("scene.rad"), "# MOCK; not a simulation scene\n").unwrap();
    seal(root, &["input.json", "raw.rgb", "scene.rad"]).unwrap();
}

#[test]
fn empty_project_roundtrip_preserves_model_and_detects_external_edits() {
    let directory = tempdir().unwrap();
    let project = example();
    let saved = save_project(directory.path(), &project, None).unwrap();
    let reopened = open_project(directory.path()).unwrap();
    assert_eq!(saved.revision, reopened.revision);
    assert_eq!(
        serde_json::to_value(saved.project).unwrap(),
        serde_json::to_value(&reopened.project).unwrap()
    );
    let mut external = reopened.project.clone();
    external.name = "External editor change".into();
    atomic_json(directory.path(), &external).unwrap();
    let error = save_project(directory.path(), &project, Some(&reopened.revision)).unwrap_err();
    assert!(error.to_string().contains("changed externally"));
    assert_eq!(
        open_project(directory.path()).unwrap().project.name,
        "External editor change"
    );
}

#[test]
fn writer_lock_lifetime_prevents_concurrent_write_and_releases_on_drop() {
    let directory = tempdir().unwrap();
    let lock = WriterLock::acquire(directory.path()).unwrap();
    assert!(WriterLock::acquire(directory.path()).is_err());
    assert!(save_project(directory.path(), &example(), None).is_err());
    drop(lock);
    assert!(save_project(directory.path(), &example(), None).is_ok());
}

#[test]
fn successful_manifest_replacement_keeps_complete_parseable_document() {
    let directory = tempdir().unwrap();
    let first = example();
    let revision = atomic_json(directory.path(), &first).unwrap();
    let mut second = first;
    second.name = "Replacement project".into();
    check_revision(directory.path(), Some(&revision)).unwrap();
    let replacement = atomic_json(directory.path(), &second).unwrap();
    assert_ne!(revision, replacement);
    assert_eq!(
        open_project(directory.path()).unwrap().project.name,
        "Replacement project"
    );
    assert!(fs::read_dir(directory.path()).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .ends_with(".tmp")));
}

#[test]
fn mocked_run_promotion_is_immutable_portable_and_independent_of_scratch() {
    let staging = tempdir().unwrap();
    let project = tempdir().unwrap();
    let copy = tempdir().unwrap();
    mock_compact_evidence(staging.path());
    let id = uuid::Uuid::new_v4().to_string();
    promote_run(staging.path(), project.path(), &id).unwrap();
    let finalized = project.path().join("runs").join(&id);
    let before = verify_seal(&finalized).unwrap().sha256;
    // Retrying a completed promotion is idempotent if its evidence matches.
    promote_run(staging.path(), project.path(), &id).unwrap();
    // Copy completed evidence elsewhere, then remove the disposable source.
    promote_run(&finalized, copy.path(), &id).unwrap();
    drop(staging);
    let copied = copy.path().join("runs").join(&id);
    assert_eq!(verify_seal(&copied).unwrap().sha256, before);
    assert_eq!(
        fs::read(copied.join("raw.rgb")).unwrap(),
        fs::read(finalized.join("raw.rgb")).unwrap()
    );
    let changed = tempdir().unwrap();
    mock_compact_evidence(changed.path());
    fs::write(changed.path().join("raw.rgb"), "# DIFFERENT MOCK\n").unwrap();
    seal(changed.path(), &["input.json", "raw.rgb", "scene.rad"]).unwrap();
    assert!(promote_run(changed.path(), project.path(), &id)
        .unwrap_err()
        .to_string()
        .contains("conflict"));
    assert_eq!(verify_seal(&finalized).unwrap().sha256, before);
}

#[test]
fn incomplete_or_tampered_mock_evidence_is_not_promoted() {
    let source = tempdir().unwrap();
    let project = tempdir().unwrap();
    mock_compact_evidence(source.path());
    let id = uuid::Uuid::new_v4().to_string();
    fs::remove_file(source.path().join("raw.rgb")).unwrap();
    assert!(promote_run(source.path(), project.path(), &id).is_err());
    assert!(!project.path().join("runs").join(&id).exists());
    assert!(!project.path().join("project.json").exists());
}

#[test]
fn zip_roundtrip_keeps_compact_evidence_and_checksums() {
    let source = tempdir().unwrap();
    mock_compact_evidence(source.path());
    fs::write(source.path().join("project.json"), "{\"mock\":true}\n").unwrap();
    seal(
        source.path(),
        &["project.json", "input.json", "raw.rgb", "scene.rad"],
    )
    .unwrap();
    let exports = tempdir().unwrap();
    let archive = exports.path().join("evidence.zip");
    let files = [
        "project.json",
        "input.json",
        "raw.rgb",
        "scene.rad",
        "checksums.json",
    ]
    .map(str::to_string);
    zip_directory(source.path(), &files, &archive).unwrap();
    let destination = exports.path().join("unpacked");
    extract_package(&archive, &destination).unwrap();
    assert_eq!(
        verify_seal(source.path()).unwrap().sha256,
        verify_seal(&destination).unwrap().sha256
    );
    assert!(zip_directory(source.path(), &files, &archive).is_err());
}

fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let file = fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    for (name, contents) in entries {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(contents).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn package_traversal_is_rejected_before_creating_destination() {
    let directory = tempdir().unwrap();
    let archive = directory.path().join("traversal.zip");
    write_zip(&archive, &[("../escape.txt", b"not allowed")]);
    let destination = directory.path().join("unpacked");
    assert!(extract_package(&archive, &destination).is_err());
    assert!(!destination.exists());
    assert!(!directory.path().join("escape.txt").exists());
}

#[test]
fn case_aliased_zip_paths_are_rejected() {
    let directory = tempdir().unwrap();
    let archive = directory.path().join("alias.zip");
    write_zip(&archive, &[("raw.rgb", b"a"), ("RAW.RGB", b"b")]);
    let destination = directory.path().join("unpacked");
    assert!(extract_package(&archive, &destination)
        .unwrap_err()
        .to_string()
        .contains("Duplicate"));
    assert!(!destination.exists());
}

#[test]
fn malformed_and_truncated_packages_fail_closed() {
    let directory = tempdir().unwrap();
    let archive = directory.path().join("broken.zip");
    fs::write(&archive, b"PK\x03\x04truncated").unwrap();
    assert!(extract_package(&archive, &directory.path().join("unpacked")).is_err());
    for bad in [
        "../file",
        "/absolute",
        "C:/outside",
        "a\\b",
        "a\0b",
        "",
        "NUL",
        "COM1.txt",
        "a/../b",
        "a/./b",
        "a//b",
        "a.",
        "a ",
    ] {
        assert!(relative_path(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn unsealed_extra_zip_file_is_rejected() {
    let source = tempdir().unwrap();
    fs::write(source.path().join("project.json"), "{\"mock\":true}\n").unwrap();
    seal(source.path(), &["project.json"]).unwrap();
    fs::write(
        source.path().join("unsealed.txt"),
        "not part of checksum manifest",
    )
    .unwrap();
    let exports = tempdir().unwrap();
    let archive = exports.path().join("extra.zip");
    let files = ["project.json", "checksums.json", "unsealed.txt"].map(str::to_string);
    zip_directory(source.path(), &files, &archive).unwrap();
    let error = extract_package(&archive, &exports.path().join("unpacked")).unwrap_err();
    assert!(error.to_string().contains("coverage"));
}

// This record is intentionally mocked and tagged in provenance. It is used
// only to test immutable snapshot/comparison validation, never engine science.
fn mock_record(model: greenlens_core::Model, alternative: &str) -> greenlens_runner::RunRecord {
    use greenlens_core::{canonical_hash, statistics, validate_model};
    use serde_json::json;
    let validated = validate_model(&model).unwrap();
    let lux = vec![179.; validated.sensor_grid.sensors.len()];
    let profile = greenlens_radiance::Profile::shipping();
    let engine = json!({"version":greenlens_radiance::ENGINE_VERSION,"adapter_version":greenlens_radiance::ADAPTER_VERSION,
        "executable_sha256":{"gensky.exe":"0".repeat(64),"oconv.exe":"0".repeat(64),"rtrace.exe":"0".repeat(64)},
        "library_sha256":{"lib/mock-only.cal":"0".repeat(64)},"fingerprint":"MOCK: never engine acceptance"});
    let processes=["gensky.exe","oconv.exe","rtrace.exe"].into_iter().map(|executable|{
        let arguments=match executable {
            "gensky.exe"=>vec!["-ang".into(),"45".into(),"0".into(),"-c".into(),"-B".into(),(model.sky.exterior_horizontal_illuminance_lux/179.).to_string(),"-g".into(),model.sky.ground_reflectance.to_string()],
            "oconv.exe"=>vec!["-f".into(),"sky.rad".into(),"scene.rad".into()],
            _=>profile.arguments(),
        };
        json!({"executable":executable,"arguments":arguments,"exit_code":0,"started_utc":"2026-01-01T00:00:00Z","finished_utc":"2026-01-01T00:00:01Z","elapsed_ms":1000,"stderr":"MOCK fixture; no process was executed"})
    }).collect::<Vec<_>>();
    let provenance = json!({"schema_version":greenlens_runner::SCHEMA_VERSION,"state":"completed","units":"lx",
        "application_version":"0.1.0","adapter_version":greenlens_radiance::ADAPTER_VERSION,
        "calculation_label":greenlens_runner::CALCULATION_LABEL,
        "test_fixture":"MOCK; not engine execution or acceptance evidence", "engine":engine,"profile":profile,
        "started_utc":"2026-01-01T00:00:00Z","finished_utc":"2026-01-01T00:00:01Z","processes":processes,
        "input_hash":validated.normalized_input_hash,"sensor_hash":validated.sensor_hash,
        "comparison_fingerprint":canonical_hash(&json!({"conditions":validated.comparison_conditions_hash,"engine":engine,"profile":profile})).unwrap(),
        "run_fingerprint":canonical_hash(&json!({"input":validated.scientific_input_hash,"engine":engine,"profile":profile})).unwrap()});
    greenlens_runner::RunRecord {
        id: uuid::Uuid::new_v4().to_string(),
        alternative_id: alternative.into(),
        model,
        validated,
        statistics: serde_json::to_value(statistics(&lux).unwrap()).unwrap(),
        lux,
        provenance,
    }
}

#[test]
fn mocked_record_validation_rejects_corrupted_identity_values_and_provenance() {
    use greenlens_runner::validate_record;
    let valid = mock_record(greenlens_core::Model::default(), "A");
    validate_record(&valid).unwrap();
    let mut historical = valid.clone();
    historical.provenance["application_version"] = serde_json::json!("0.0.9");
    historical.provenance["adapter_version"] = serde_json::json!("0.1.0");
    validate_record(&historical).unwrap();
    for field in [
        "application_version",
        "adapter_version",
        "calculation_label",
    ] {
        let mut missing = valid.clone();
        missing.provenance.as_object_mut().unwrap().remove(field);
        assert!(validate_record(&missing)
            .unwrap_err()
            .to_string()
            .contains("application, adapter or calculation provenance"));
        for invalid in [
            serde_json::Value::Null,
            serde_json::json!(""),
            serde_json::json!(" "),
            serde_json::json!("invalid\nversion"),
            serde_json::json!(1),
        ] {
            let mut changed = valid.clone();
            changed.provenance[field] = invalid;
            assert!(validate_record(&changed)
                .unwrap_err()
                .to_string()
                .contains("application, adapter or calculation provenance"));
        }
    }
    let mut wrong_calculation = valid.clone();
    wrong_calculation.provenance["calculation_label"] = serde_json::json!("Annual daylight");
    assert!(validate_record(&wrong_calculation).is_err());
    let mut incompatible_adapter = valid.clone();
    incompatible_adapter.provenance["engine"]["adapter_version"] = serde_json::json!("unsupported");
    assert!(validate_record(&incompatible_adapter)
        .unwrap_err()
        .to_string()
        .contains("engine identity"));
    let mut corrupted = valid.clone();
    corrupted.validated.sensor_grid.sensors.swap(0, 1);
    assert!(validate_record(&corrupted).is_err());
    corrupted = valid.clone();
    corrupted.model.window.width_m = 2.;
    assert!(validate_record(&corrupted).is_err());
    corrupted = valid.clone();
    corrupted.lux.pop();
    assert!(validate_record(&corrupted).is_err());
    corrupted = valid.clone();
    corrupted.statistics["mean"] = serde_json::json!(1);
    assert!(validate_record(&corrupted).is_err());
    corrupted = valid.clone();
    corrupted.provenance["processes"][2]["exit_code"] = serde_json::json!(1);
    assert!(validate_record(&corrupted).is_err());
    corrupted = valid.clone();
    corrupted.provenance["finished_utc"] = serde_json::json!("2026-01-01T00:00:01+01:00");
    assert!(validate_record(&corrupted).is_err());
    corrupted = valid;
    corrupted.provenance["units"] = serde_json::json!("W/m2");
    assert!(validate_record(&corrupted).is_err());
}

#[test]
fn mocked_historical_inputs_remain_immutable_and_comparison_deltas_use_matching_sensors() {
    use greenlens_runner::{compare_runs, is_stale};
    let a = mock_record(greenlens_core::Model::default(), "A");
    let b = mock_record(greenlens_core::Model::alternative_b(), "B");
    let comparison = compare_runs(&a, &b).unwrap();
    assert_eq!(comparison.deltas_lux, vec![0.; 77]);
    assert_eq!(comparison.scale_max_lux, 179.);
    let snapshot = serde_json::to_value(&a.model).unwrap();
    let mut current = a.model.clone();
    current.window.width_m = 2.;
    assert!(is_stale(&a, &current).unwrap());
    assert_eq!(serde_json::to_value(&a.model).unwrap(), snapshot);
    current = a.model.clone();
    current.name = "Display name edit".into();
    assert!(!is_stale(&a, &current).unwrap());
    let mut incompatible = b.model.clone();
    incompatible.sky.exterior_horizontal_illuminance_lux = 9000.;
    let other = mock_record(incompatible, "B");
    assert!(compare_runs(&a, &other).is_err());
}

#[cfg(unix)]
#[test]
fn evidence_symlink_cannot_escape_project() {
    use std::os::unix::fs::symlink;
    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    fs::write(outside.path().join("private.txt"), "private").unwrap();
    symlink(
        outside.path().join("private.txt"),
        root.path().join("input.json"),
    )
    .unwrap();
    assert!(seal(root.path(), &["input.json"]).is_err());
}

#[cfg(windows)]
fn create_test_junction(link: &Path, target: &Path) {
    use std::os::windows::process::CommandExt;
    // Fixed test command, with paths carried only in child environment values.
    // Windows junction creation requires no Developer Mode or administrator.
    let output=std::process::Command::new("powershell.exe")
        .args(["-NoLogo","-NoProfile","-NonInteractive","-Command",
            "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path $env:GREENLENS_TEST_LINK_PATH -Value $env:GREENLENS_TEST_LINK_TARGET | Out-Null"])
        .env("GREENLENS_TEST_LINK_PATH",link)
        .env("GREENLENS_TEST_LINK_TARGET",target)
        .creation_flags(0x0800_0000)
        .output().expect("Windows PowerShell is required for the native junction fixture");
    assert!(
        output.status.success(),
        "junction fixture creation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(windows)]
#[test]
fn native_destination_runs_junction_cannot_redirect_promotion() {
    let source = tempdir().unwrap();
    let project = tempdir().unwrap();
    let outside = tempdir().unwrap();
    mock_compact_evidence(source.path());
    create_test_junction(&project.path().join("runs"), outside.path());
    let id = uuid::Uuid::new_v4().to_string();
    assert!(promote_run(source.path(), project.path(), &id).is_err());
    assert!(fs::read_dir(outside.path()).unwrap().next().is_none());
}

#[cfg(windows)]
#[test]
fn native_existing_run_junction_is_rejected_even_with_identical_mock_checksums() {
    let source = tempdir().unwrap();
    let project = tempdir().unwrap();
    let outside = tempdir().unwrap();
    mock_compact_evidence(source.path());
    mock_compact_evidence(outside.path());
    fs::create_dir(project.path().join("runs")).unwrap();
    let id = uuid::Uuid::new_v4().to_string();
    create_test_junction(&project.path().join("runs").join(&id), outside.path());
    assert!(promote_run(source.path(), project.path(), &id).is_err());
}

#[cfg(windows)]
#[test]
fn native_project_root_junction_is_rejected_before_writing_outside() {
    let source = tempdir().unwrap();
    let parent = tempdir().unwrap();
    let outside = tempdir().unwrap();
    mock_compact_evidence(source.path());
    let linked = parent.path().join("linked-project");
    create_test_junction(&linked, outside.path());
    let id = uuid::Uuid::new_v4().to_string();
    assert!(promote_run(source.path(), &linked, &id).is_err());
    assert!(fs::read_dir(outside.path()).unwrap().next().is_none());
    assert!(WriterLock::acquire(&linked).is_err());
}
