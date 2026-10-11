//! Synthetic export/replay arithmetic checks, never native simulation evidence.
use super::*;
use crate::{
    climate::{ClimateProject, WeatherReference},
    timeseries::SeriesWriter,
};
use tempfile::tempdir;

#[test]
fn legacy_only_replay_does_not_discover_annual_engines() {
    let temporary = tempdir().unwrap();
    let missing_engine = temporary.path().join("annual-engine-is-not-installed");
    let checks = replay_climate(
        temporary.path(),
        &[],
        Some(&missing_engine),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(checks.is_empty());
    assert!(!missing_engine.exists());
}

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
    assert!(report.contains("Source minute 60 → canonical hour-ending minute 60"));
    assert!(report.contains("Daylight-saving adjustment applied: no"));
    assert!(report.contains("Radiation policy:"));
    assert!(report.contains("Complete validated time-normalization policy"));
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

#[test]
fn report_defines_metrics_boundaries_and_actual_occupancy_denominator() {
    let mut html = Html(String::new());
    let mut settings = MetricSettings::default();
    metric_definitions(&mut html, &settings).unwrap();
    for text in [
        "Daily 08:00–18:00 local standard time",
        "start inclusive and end exclusive",
        "Occupied intervals (positive weight): 3650 of 8760",
        "W = Σ wₜ = 3650",
        "DA = 100 × Σ(wₜ × 1[Eₜ ≥ T]) / W",
        "cDA = 100 × Σ(wₜ × min(Eₜ / T, 1)) / W",
        "1[L ≤ Eₜ ≤ U]",
        "1[Eₜ &lt; L]",
        "1[Eₜ &gt; U]",
        "T = 300 lx; L = 100 lx; U = 3000 lx",
        "Nighttime occupancy remains",
    ] {
        assert!(html.0.contains(text), "Missing definition: {text}");
    }
    settings.occupancy = Occupancy::Daily {
        start_minute: 22 * 60 + 30,
        end_minute: 6 * 60 + 15,
    };
    let mut overnight = Html(String::new());
    metric_definitions(&mut overnight, &settings).unwrap();
    assert!(overnight.0.contains("22:30–06:15"));
    assert!(overnight.0.contains("crossing midnight"));
    assert!(overnight.0.contains("W = Σ wₜ = 2828.75"));
    let mut weights = vec![0.0; 8760];
    weights[0] = 0.25;
    weights[1] = 0.75;
    weights[2] = 1.0;
    settings.occupancy = Occupancy::Mask { weights };
    let mut mask = Html(String::new());
    metric_definitions(&mut mask, &settings).unwrap();
    assert!(mask
        .0
        .contains("Explicit 8,760-interval occupancy-weight mask"));
    assert!(mask
        .0
        .contains("positive weight): 3 of 8760; fractional-weight intervals: 2"));
    assert!(mask.0.contains("W = Σ wₜ = 2"));
    let mut tiny_weights = vec![0.0; 8760];
    tiny_weights[0] = 1e-310;
    tiny_weights[1] = 1e-310;
    settings.occupancy = Occupancy::Mask {
        weights: tiny_weights,
    };
    let mut tiny = Html(String::new());
    metric_definitions(&mut tiny, &settings).unwrap();
    assert!(tiny.0.contains("W = Σ wₜ = 2e-310"));
}

#[test]
fn report_weather_policy_uses_reparsed_source_specific_normalization() {
    use greenlens_core::weather::{parse_epw, synthetic_epw, WeatherSourceProfile};
    let original = synthetic_epw();
    let generic = parse_epw(&original, WeatherSourceProfile::EpwHourEnding).unwrap();
    let mut first = Html(String::new());
    weather_policy(&mut first, &generic.data).unwrap();
    assert!(first.0.contains("greenlens.epw_hour_ending_midpoint.v1"));
    assert!(first
        .0
        .contains("Source minute 60 → canonical hour-ending minute 60"));
    assert!(first.0.contains("not declared by the source"));
    assert!(first.0.contains("Daylight-saving adjustment applied: no"));
    assert!(first.0.contains("interval duration 60 minutes"));
    assert!(first.0.contains("Representative instant: -30 minutes"));
    assert!(first
        .0
        .contains("accumulation divided by the 1 h interval duration"));
    assert!(first.0.contains(&generic.data.original_sha256));
    // Synthetic bytes explicitly exercise the supported PVGIS labeling policy;
    // they are not actual PVGIS climate or a native simulation fixture.
    let pvgis = String::from_utf8(original.clone()).unwrap()
        .replace(",60,?9?9?9,", ",0,?9?9?9,")
        .replace("COMMENTS 2,Hourly interval-ending accumulations represented at midpoint; no source irradiation shift", "COMMENTS 2,Irradiance Time Offset (h):-0.5");
    let parsed = parse_epw(pvgis.as_bytes(), WeatherSourceProfile::PvgisTmy).unwrap();
    let mut second = Html(String::new());
    weather_policy(&mut second, &parsed.data).unwrap();
    assert!(second.0.contains("greenlens.pvgis_tmy_epw_midpoint.v1"));
    assert!(second
        .0
        .contains("Source minute 0 → canonical hour-ending minute 60"));
    assert!(second.0.contains("-0.5 h relative to the interval end"));
    assert!(!second.0.contains("not declared by the source"));
    assert_eq!(original, synthetic_epw());
}
