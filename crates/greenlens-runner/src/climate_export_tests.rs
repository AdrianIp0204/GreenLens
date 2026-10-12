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
    let original_bytes: Vec<_> = records
        .iter()
        .map(|run| {
            let dir = source.join("runs").join(&run.id);
            let sums: Checksums = read_json(&dir.join("checksums.json")).unwrap();
            sums.sha256
                .keys()
                .map(|name| (name.clone(), fs::read(dir.join(name)).unwrap()))
                .collect::<Vec<_>>()
        })
        .collect();
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
    assert!(report.contains("GreenLens · selected evidence"));
    assert!(report.contains("Model +Y ↑; window facade ↓"));
    assert!(!report.contains("Metric settings match:"));
    assert!(!report.contains("Â"));
    assert!(!report.contains("â€"));
    assert_eq!(
        report
            .matches("<h3>Scientific qualification unassessed · Candidate profile</h3>")
            .count(),
        4
    ); // each run and each comparison side, outside collapsed details
    assert!(report.contains("Full retained evidence checks passed"));
    let supplement: Value = read_json(&extracted.join("qualification.json")).unwrap();
    assert_eq!(supplement["assessments"].as_object().unwrap().len(), 2);
    for (run, members) in records.iter().zip(&original_bytes) {
        assert_eq!(
            supplement["assessments"][&run.id]["record_sha256"],
            canonical_hash(run).unwrap()
        );
        for (name, bytes) in members {
            assert_eq!(
                &fs::read(
                    temp.path()
                        .join("source-unavailable")
                        .join("runs")
                        .join(&run.id)
                        .join(name)
                )
                .unwrap(),
                bytes,
                "original unchanged {name}"
            );
            assert_eq!(
                &fs::read(extracted.join("runs").join(&run.id).join(name)).unwrap(),
                bytes,
                "immutable {name}"
            );
        }
    }
    let comparisons: Value = read_json(&extracted.join("comparisons.json")).unwrap();
    assert_eq!(comparisons["climate"].as_array().unwrap().len(), 1);
    assert_eq!(comparisons["climate"][0]["a_id"], records[0].id);
    assert_eq!(comparisons["climate"][0]["b_id"], records[1].id);
    assert_eq!(
        comparisons["climate"][0]["a_assessment"]["scientific_qualification"]["status"],
        "unassessed"
    );
    assert_eq!(
        comparisons["climate"][0]["b_assessment"]["scientific_qualification"]["status"],
        "unassessed"
    );
    // Even a coherently re-sealed imported assessment is merely commentary.
    let mut forged = supplement;
    for assessment in forged["assessments"].as_object_mut().unwrap().values_mut() {
        assessment["scientific_qualification"]["status"] = json!("limited_qualified");
    }
    write_json(&extracted.join("qualification.json"), &forged).unwrap();
    let root_seal: Checksums = read_json(&extracted.join("checksums.json")).unwrap();
    seal(
        &extracted,
        &root_seal
            .sha256
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert!(crate::open_project(&extracted)
        .unwrap()
        .climate_assessments
        .values()
        .all(|value| value.scientific_qualification.status
            == crate::qualification::QualificationStatus::Unassessed));
    let single = temp.path().join("single.zip");
    export_selected(&extracted, &single, std::slice::from_ref(&records[0].id)).unwrap();
    let single_root = temp.path().join("single");
    extract_package(&single, &single_root).unwrap();
    let recalculated: Value = read_json(&single_root.join("qualification.json")).unwrap();
    assert_eq!(
        recalculated["assessments"][&records[0].id]["scientific_qualification"]["status"],
        "unassessed"
    );
    assert_eq!(
        crate::open_project(&single_root)
            .unwrap()
            .climate_results
            .len(),
        1
    );
}

#[test]
fn standalone_assessment_labels_all_three_statuses_without_implying_general_acceptance() {
    use crate::qualification::*;
    let temporary = tempdir().unwrap();
    let record = climate::tests::fixture(temporary.path(), false);
    let mut assessment = from_record(
        &record,
        None,
        EvidenceIntegrity::FullRetainedEvidenceChecked,
    )
    .unwrap();
    for (status, expected) in [
        (
            QualificationStatus::Failed,
            "Failed scientific qualification",
        ),
        (
            QualificationStatus::Unassessed,
            "Scientific qualification unassessed",
        ),
        (
            QualificationStatus::LimitedQualified,
            "Limited scientific qualification",
        ),
    ] {
        // Synthetic presentation input only; positive production registry is empty.
        assessment.scientific_qualification.status = status;
        assessment.scientific_qualification.reason = "<scope> only".into();
        assessment.scientific_qualification.evidence = vec![QualificationEvidence {
            id: "synthetic-fixture-only".into(),
            sha256: "0".repeat(64),
            tested_scope: "one exact synthetic condition".into(),
            outcome: status,
            reason: "Test presentation only".into(),
        }];
        let mut html = Html(String::new());
        render_assessment(&mut html, &assessment).unwrap();
        assert!(html.0.contains(expected));
        assert!(html.0.contains("&lt;scope&gt; only"));
        assert!(html.0.contains("synthetic-fixture-only"));
        assert!(html.0.contains("one exact synthetic condition"));
        assert!(!html.0.contains("<details>"));
        assert!(html.0.contains("do not establish scientific qualification"));
    }
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
fn primary_replay_compares_exact_input_without_demanding_stochastic_output_identity() {
    let temp = tempdir().unwrap();
    let record = climate::tests::fixture(temp.path(), false);
    let recipe = read_json(&temp.path().join("annual-recipe.json")).unwrap();
    let original = climate::read_primary_streams(temp.path(), &recipe)
        .unwrap()
        .unwrap();
    assert_eq!(original.sensor_count, record.series.sensor_ids.len());
    let mut regenerated = original.clone();
    regenerated.records[0].matrix_sha256 = "a".repeat(64);
    regenerated.records[0].resources.samples += 1;
    verify_primary_replay(Some(original.clone()), Some(regenerated.clone())).unwrap();
    verify_primary_replay(None, None).unwrap();
    assert!(verify_primary_replay(Some(original.clone()), None).is_err());
    assert!(verify_primary_replay(None, Some(original.clone())).is_err());
    for mutation in 0..6 {
        let mut changed = regenerated.clone();
        match mutation {
            0 => changed.records[0].input.bytes += 1,
            1 => changed.records[0].input.records += 1,
            2 => changed.records[0].input.sha256 = "b".repeat(64),
            3 => changed.contract.count *= 4,
            4 => changed.sensor_sha256 = "c".repeat(64),
            _ => changed.records.clear(),
        }
        assert!(verify_primary_replay(Some(original.clone()), Some(changed)).is_err());
    }
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
