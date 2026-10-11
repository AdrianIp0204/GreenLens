//! Synthetic export/replay arithmetic checks, never native simulation evidence.
use super::*;
use crate::{
    climate::{ClimateProject, WeatherReference},
    timeseries::SeriesWriter,
};
use tempfile::tempdir;

#[test]
fn explicit_selection_rejects_ambiguous_ids_and_preserves_current_studies() {
    let mut project = crate::example();
    let id = Uuid::new_v4().to_string();
    project.runs.push(crate::RunSummary {
        id: id.clone(),
        alternative_id: "A".into(),
        path: format!("runs/{id}"),
        scientific_input_hash: "0".repeat(64),
        comparison_fingerprint: "1".repeat(64),
    });
    let mut extension = ClimateProject::default();
    let climate_id = Uuid::new_v4().to_string();
    extension.runs.push(climate::ClimateRunSummary {
        id: climate_id.clone(),
        alternative_id: "B".into(),
        path: format!("runs/{climate_id}"),
        physical_input_hash: "2".repeat(64),
        comparison_fingerprint: "3".repeat(64),
        record_sha256: "4".repeat(64),
    });
    project.climate = Some(extension);
    project.schema_version = "0.3.0".into();
    assert!(reduced_project(project.clone(), &[]).is_err());
    assert!(reduced_project(project.clone(), &[id.clone(), id.clone()]).is_err());
    assert!(reduced_project(project.clone(), &[Uuid::new_v4().to_string()]).is_err());
    let selected = reduced_project(project.clone(), std::slice::from_ref(&climate_id)).unwrap();
    assert!(selected.runs.is_empty());
    assert_eq!(selected.climate.unwrap().runs.len(), 1);
    let mixed = reduced_project(project, &[id, climate_id]).unwrap();
    assert_eq!(mixed.runs.len(), 1);
    assert_eq!(mixed.climate.unwrap().studies.len(), 2);
}

fn fixture_project(root: &Path) -> (Project, Vec<ClimateRunRecord>) {
    fs::create_dir_all(root).unwrap();
    let temp = tempdir().unwrap();
    let mut project = crate::example();
    project.name = "<script>should be escaped</script>".into();
    project.schema_version = "0.3.0".into();
    let mut extension = ClimateProject::default();
    let mut records = Vec::new();
    for alternative in ["A", "B"] {
        let scratch = temp.path().join(alternative);
        let mut record = climate::tests::fixture(&scratch, false);
        record.alternative_id = alternative.into();
        write_json(&scratch.join("results.json"), &record).unwrap();
        let names = climate::evidence_files(&record);
        seal(
            &scratch,
            &names.iter().map(String::as_str).collect::<Vec<_>>(),
        )
        .unwrap();
        if extension.weather.is_empty() {
            let reference: WeatherReference = climate::import_project_weather(
                &scratch.join("weather"),
                root,
                &Uuid::new_v4().to_string(),
            )
            .unwrap();
            extension.weather.push(reference);
        }
        extension
            .studies
            .insert(alternative.into(), record.study.clone());
        promote_run(&scratch, root, &record.id).unwrap();
        climate::append_run(&mut extension, &record).unwrap();
        records.push(record);
    }
    project.climate = Some(extension);
    atomic_json(root, &project).unwrap();
    (project, records)
}

#[test]
fn selected_export_roundtrips_reader_with_retained_weather_and_offline_report() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("source");
    let (_, records) = fixture_project(&source);
    let package = temp.path().join("selected.zip");
    export_selected(
        &source,
        &package,
        &[records[0].id.clone(), records[1].id.clone()],
    )
    .unwrap();
    let extracted = temp.path().join("extracted");
    extract_package(&package, &extracted).unwrap();
    fs::rename(&source, temp.path().join("source-unavailable")).unwrap();
    let restored = crate::open_project(&extracted).unwrap();
    assert_eq!(restored.climate_results.len(), 2);
    assert_eq!(
        restored.climate_results[0].provenance,
        records[0].provenance
    );
    let report = fs::read_to_string(extracted.join("report.html")).unwrap();
    assert!(report.contains("&lt;script&gt;should be escaped&lt;/script&gt;"));
    assert!(!report.contains("<script>"));
    assert!(report.contains(climate::HOUR_LABEL));
    assert!(report.contains("Original synthetic fixture"));
    let comparisons: Value = read_json(&extracted.join("comparisons.json")).unwrap();
    assert_eq!(comparisons["climate"].as_array().unwrap().len(), 1);
    assert_eq!(comparisons["climate"][0]["a_id"], records[0].id);
    assert_eq!(comparisons["climate"][0]["b_id"], records[1].id);
    let single = temp.path().join("single.zip");
    export_selected(&extracted, &single, std::slice::from_ref(&records[0].id)).unwrap();
    let single_root = temp.path().join("single");
    extract_package(&single, &single_root).unwrap();
    assert_eq!(
        crate::open_project(&single_root)
            .unwrap()
            .climate_results
            .len(),
        1
    );
}

#[test]
fn source_budget_rejects_oversized_member_before_creating_public_zip() {
    let temp = tempdir().unwrap();
    let source = temp.path().join("source");
    let (_, records) = fixture_project(&source);
    let resources = source
        .join("runs")
        .join(&records[0].id)
        .join("annual-resources.json");
    File::options()
        .write(true)
        .open(resources)
        .unwrap()
        .set_len(MAX_EVIDENCE_FILE_BYTES + 1)
        .unwrap();
    let package = temp.path().join("must-not-exist.zip");
    assert!(export_selected(&source, &package, std::slice::from_ref(&records[0].id)).is_err());
    assert!(!package.exists());
}

#[test]
fn linear_pair_groups_do_not_expand_all_historical_combinations() {
    let temp = tempdir().unwrap();
    let root = temp.path().join("project");
    let (_, records) = fixture_project(&root);
    let mut history = vec![
        records[0].clone(),
        records[1].clone(),
        records[0].clone(),
        records[1].clone(),
    ];
    assert_eq!(comparison_pairs(&root, &history).unwrap().len(), 1);
    history[3].study.metrics.da_threshold_lux = 500.0;
    // Pure grouping uses scientific conditions; it never substitutes an
    // incompatible latest B for the compatible retained pair.
    assert_eq!(comparison_pairs(&root, &history).unwrap().len(), 1);
}

#[test]
fn streaming_replay_checks_every_value_tolerance_shape_and_cancellation() {
    let temp = tempdir().unwrap();
    let series = temp.path().join("series");
    let mut writer = SeriesWriter::create(&series, vec!["s1".into(), "s2".into()], 8760).unwrap();
    let mut actual = Vec::new();
    for _ in 0..8760 {
        writer.push_hour(&[[0.0; 3], [10.0; 3]]).unwrap();
        for rgb in [[0.0; 3], [10.0; 3]] {
            for channel in rgb {
                actual.extend_from_slice(&f64::to_le_bytes(channel));
            }
        }
    }
    writer.finish().unwrap();
    let output = temp.path().join("combined.f64");
    fs::write(&output, &actual).unwrap();
    let reader = SeriesReader::open(&series).unwrap();
    let result = compare_stream(&reader, &output, &AtomicBool::new(false)).unwrap();
    assert_eq!(result["values_checked"], 17520);
    assert_eq!(result["passed"], true);
    assert_eq!(result["max_absolute_error_lux"], 0.0);
    // Change only the final sensor: traversal must not stop after early success.
    let end = actual.len();
    actual[end - 8..end].copy_from_slice(&20.0_f64.to_le_bytes());
    fs::write(&output, &actual).unwrap();
    assert_eq!(
        compare_stream(&reader, &output, &AtomicBool::new(false)).unwrap()["passed"],
        false
    );
    assert!(compare_stream(&reader, &output, &AtomicBool::new(true)).is_err());
    fs::write(&output, [0_u8; 7]).unwrap();
    assert!(compare_stream(&reader, &output, &AtomicBool::new(false)).is_err());
    assert!(
        replay_climate(temp.path(), &[], None, &AtomicBool::new(false))
            .unwrap()
            .is_empty()
    );
}
