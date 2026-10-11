//! Synthetic service arithmetic and identity tests, never climate/engine evidence.
use crate::{climate, persistence::*, studies::*, timeseries::SeriesReader};
use greenlens_core::{
    rgb_to_lux,
    study::{Occupancy, HOURS_PER_YEAR},
};
use serde_json::json;
use std::{fs, path::Path};
use tempfile::tempdir;

fn persist(root: &Path, record: &mut climate::ClimateRunRecord) {
    record.provenance["physical_input_hash"] =
        json!(record.study.physical_input_hash(&record.model).unwrap());
    record.provenance["run_fingerprint"] = json!(climate::run_fingerprint(record).unwrap());
    record.provenance["comparison_fingerprint"] =
        json!(climate::comparison_fingerprint(record).unwrap());
    let physical = climate::physical_result_fingerprint(record).unwrap();
    record.provenance["physical_result_fingerprint"] = json!(physical);
    record.provenance["metric_identity"] = if record.metrics.is_some() {
        json!(record.study.metrics.identity(&physical).unwrap())
    } else {
        serde_json::Value::Null
    };
    write_json(&root.join("results.json"), record).unwrap();
    write_json(&root.join("provenance.json"), &record.provenance).unwrap();
    write_json(&root.join("study.json"), &record.study).unwrap();
    let files = climate::evidence_files(record);
    seal(root, &files.iter().map(String::as_str).collect::<Vec<_>>()).unwrap();
}

/// Replace only owned synthetic sample bytes and rederive all dependent identity.
/// Two synthetic daylight hours carry weights 0.25 and 0.75; others are unoccupied.
fn annual_fixture(
    root: &Path,
    alternative: &str,
    first: [f64; 2],
    second: [f64; 2],
) -> climate::ClimateRunRecord {
    let mut record = climate::tests::fixture(root, true);
    record.alternative_id = alternative.into();
    assert_eq!(record.series.sensor_ids.len(), 2);
    let mut weights = vec![0.; HOURS_PER_YEAR];
    weights[8] = 0.25;
    weights[9] = 0.75;
    record.study.metrics.occupancy = Occupancy::Mask { weights };
    for chunk in &mut record.series.chunks {
        let mut bytes = Vec::with_capacity(chunk.bytes);
        for hour in chunk.start..chunk.start + chunk.intervals {
            let values = match hour {
                8 => first,
                9 => second,
                _ => [0.; 2],
            };
            for lux in values {
                let rgb = [lux / 179.; 3];
                for value in [rgb[0], rgb[1], rgb[2], rgb_to_lux(rgb).unwrap()] {
                    bytes.extend_from_slice(&value.to_le_bytes());
                }
            }
        }
        chunk.sha256 = bytes_hash(&bytes);
        fs::write(root.join("series").join(&chunk.file), bytes).unwrap();
    }
    write_json(&root.join("series/series.json"), &record.series).unwrap();
    record.metrics = Some(
        SeriesReader::open(&root.join("series"))
            .unwrap()
            .metrics(&record.study.metrics)
            .unwrap(),
    );
    persist(root, &mut record);
    climate::read_run_directory(root).unwrap();
    record
}

fn close(actual: &[f64], expected: &[f64]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }
}

#[test]
fn annual_value_queries_preserve_units_order_metric_identity_and_immutable_samples() {
    let project = tempdir().unwrap();
    let scratch = tempdir().unwrap();
    let record = annual_fixture(scratch.path(), "A", [0., 400.], [200., 400.]);
    promote_run(scratch.path(), project.path(), &record.id).unwrap();
    let summary = climate::summary(&record).unwrap();
    let first = result_values(project.path(), &summary, ResultField::Illuminance, 9, None).unwrap();
    close(&first.values, &[200., 400.]);
    assert_eq!(first.units, "lx");
    assert_eq!(first.interval_index, Some(9));
    assert!(first.metric_identity.is_none());
    assert_eq!(first.sensor_ids, record.series.sensor_ids);
    assert!(result_values(
        project.path(),
        &summary,
        ResultField::Illuminance,
        HOURS_PER_YEAR,
        None
    )
    .is_err());

    for (field, expected) in [
        (ResultField::Da, [0., 100.]),
        (ResultField::Cda, [50., 100.]),
        (ResultField::Udi, [75., 100.]),
        (ResultField::Underlit, [25., 0.]),
        (ResultField::Overlit, [0., 0.]),
    ] {
        let values = result_values(project.path(), &summary, field, 0, None).unwrap();
        close(&values.values, &expected);
        assert_eq!(values.units, "% of occupied time");
        assert!(values.interval_index.is_none());
        assert_eq!(
            values.metric_identity.as_deref(),
            record.provenance["metric_identity"].as_str()
        );
    }
    let original_seal =
        fs::read(project.path().join(&summary.path).join("checksums.json")).unwrap();
    let mut requested = record.study.metrics.clone();
    requested.da_threshold_lux = 1000.;
    let derived = derive_metrics(project.path(), &summary, requested.clone()).unwrap();
    close(
        &derived
            .metrics
            .iter()
            .map(|m| m.cda_percent)
            .collect::<Vec<_>>(),
        &[15., 40.],
    );
    let values = result_values(
        project.path(),
        &summary,
        ResultField::Cda,
        0,
        Some(requested.clone()),
    )
    .unwrap();
    close(&values.values, &[15., 40.]);
    assert_eq!(
        values.metric_identity,
        Some(derived.metric_identity.clone())
    );
    assert_ne!(
        derived.metric_identity,
        record.provenance["metric_identity"].as_str().unwrap()
    );
    assert_eq!(
        fs::read(project.path().join(&summary.path).join("checksums.json")).unwrap(),
        original_seal
    );
    assert_eq!(
        climate::read_run(project.path(), &summary)
            .unwrap()
            .study
            .metrics,
        record.study.metrics
    );
    let mut current_study = record.study.clone();
    current_study.metrics = requested;
    assert!(!is_stale(&record, &record.model, &current_study).unwrap());
    let mut model = record.model.clone();
    model.window.width_m += 0.1;
    assert!(is_stale(&record, &model, &current_study).unwrap());
    current_study.metrics.da_threshold_lux = f64::NAN;
    assert!(derive_metrics(project.path(), &summary, current_study.metrics).is_err());
}

#[test]
fn climate_comparison_is_b_minus_a_with_shared_scale_and_metric_percentage_points() {
    let project = tempdir().unwrap();
    let a_scratch = tempdir().unwrap();
    let b_scratch = tempdir().unwrap();
    let a = annual_fixture(a_scratch.path(), "A", [0., 400.], [200., 400.]);
    let b = annual_fixture(b_scratch.path(), "B", [400., 120.], [400., 0.]);
    promote_run(a_scratch.path(), project.path(), &a.id).unwrap();
    promote_run(b_scratch.path(), project.path(), &b.id).unwrap();
    let a_summary = climate::summary(&a).unwrap();
    let b_summary = climate::summary(&b).unwrap();
    let hour = compare(
        project.path(),
        &a_summary,
        &b_summary,
        ResultField::Illuminance,
        9,
    )
    .unwrap();
    close(&hour.deltas, &[200., -400.]);
    assert_eq!(hour.delta_units, "lx");
    assert!((hour.scale_maximum - 400.).abs() < 1e-9);
    assert_eq!(hour.a.sensor_ids, hour.b.sensor_ids);
    assert_eq!(hour.conditions["delta"], "B minus A");
    assert_eq!(hour.conditions["percentage_delta"], "omitted");
    let metrics = compare(project.path(), &a_summary, &b_summary, ResultField::Cda, 0).unwrap();
    close(&metrics.a.values, &[50., 100.]);
    close(&metrics.b.values, &[100., 10.]);
    close(&metrics.deltas, &[50., -90.]);
    assert_eq!(metrics.delta_units, "percentage points");
    assert_eq!(metrics.scale_maximum, 100.);
    assert!(compare(project.path(), &b_summary, &a_summary, ResultField::Da, 0).is_err());
    assert_ne!(
        a.provenance["physical_result_fingerprint"],
        b.provenance["physical_result_fingerprint"]
    );
    assert_ne!(
        a.provenance["metric_identity"],
        b.provenance["metric_identity"]
    );
    assert_eq!(
        a.provenance["comparison_fingerprint"],
        b.provenance["comparison_fingerprint"]
    );
}

#[test]
fn metric_condition_changes_reject_comparison_without_invalidating_physical_result() {
    let project = tempdir().unwrap();
    let a_scratch = tempdir().unwrap();
    let b_scratch = tempdir().unwrap();
    let a = annual_fixture(a_scratch.path(), "A", [0., 400.], [200., 400.]);
    let mut b = annual_fixture(b_scratch.path(), "B", [0., 400.], [200., 400.]);
    let original_physical = climate::physical_result_fingerprint(&b).unwrap();
    b.study.metrics.da_threshold_lux = 500.;
    b.metrics = Some(
        SeriesReader::open(&b_scratch.path().join("series"))
            .unwrap()
            .metrics(&b.study.metrics)
            .unwrap(),
    );
    persist(b_scratch.path(), &mut b);
    climate::read_run_directory(b_scratch.path()).unwrap();
    assert_eq!(
        climate::physical_result_fingerprint(&b).unwrap(),
        original_physical
    );
    assert_ne!(
        climate::comparison_fingerprint(&a).unwrap(),
        climate::comparison_fingerprint(&b).unwrap()
    );
    promote_run(a_scratch.path(), project.path(), &a.id).unwrap();
    promote_run(b_scratch.path(), project.path(), &b.id).unwrap();
    let error = compare(
        project.path(),
        &climate::summary(&a).unwrap(),
        &climate::summary(&b).unwrap(),
        ResultField::Da,
        0,
    )
    .unwrap_err();
    assert!(error.to_string().contains("Incompatible climate snapshots"));
}

#[test]
fn selected_hour_rejects_other_intervals_and_all_annual_metrics() {
    let project = tempdir().unwrap();
    let scratch = tempdir().unwrap();
    let record = climate::tests::fixture(scratch.path(), false);
    promote_run(scratch.path(), project.path(), &record.id).unwrap();
    let summary = climate::summary(&record).unwrap();
    let hour = result_values(project.path(), &summary, ResultField::Illuminance, 0, None).unwrap();
    close(&hour.values, &[0., 0.]);
    assert_eq!(hour.interval_index, Some(0));
    assert!(result_values(project.path(), &summary, ResultField::Illuminance, 1, None).is_err());
    for field in [
        ResultField::Da,
        ResultField::Cda,
        ResultField::Udi,
        ResultField::Underlit,
        ResultField::Overlit,
    ] {
        assert!(result_values(project.path(), &summary, field, 0, None).is_err());
    }
    assert!(derive_metrics(project.path(), &summary, record.study.metrics).is_err());
}
