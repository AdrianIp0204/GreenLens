//! Bounded climate application actions shared by desktop and CLI.
use crate::{climate, persistence::*, weather, Project, ProjectView};
use anyhow::{ensure, Context, Result};
use greenlens_core::{
    study::{MetricSettings, SensorMetrics, Study, StudyMode},
    Model,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use uuid::Uuid;

pub fn import_weather_asset(
    root: &Path,
    project: &Project,
    expected: Option<&str>,
    asset: &Path,
) -> Result<ProjectView> {
    crate::validate_project(project)?;
    let _lock = WriterLock::acquire(root)?;
    check_revision(root, expected)?;
    let (source, _) = weather::open(asset)?;
    let mut project = project.clone();
    let extension = project
        .climate
        .get_or_insert_with(climate::ClimateProject::default);
    if !extension
        .weather
        .iter()
        .any(|reference| reference.binding == source.binding)
    {
        extension.weather.push(climate::import_project_weather(
            asset,
            root,
            &Uuid::new_v4().to_string(),
        )?);
    }
    project.schema_version = "0.3.0".into();
    crate::validate_project(&project)?;
    if let Some(extension) = &project.climate {
        climate::verify_project_evidence(root, extension)?;
    }
    check_revision(root, expected)?;
    atomic_json(root, &project)?;
    crate::open_project(root)
}

/// Pure validated transaction; caller owns undo and persists with the revision guard.
pub fn configure(project: &Project, alternative: &str, study: Study) -> Result<Project> {
    let model = &project
        .alternatives
        .iter()
        .find(|value| value.id == alternative)
        .context("Alternative must be A or B")?
        .model;
    study.validate(model)?;
    let mut result = project.clone();
    result
        .climate
        .get_or_insert_with(climate::ClimateProject::default)
        .studies
        .insert(alternative.into(), study);
    result.schema_version = "0.3.0".into();
    crate::validate_project(&result)?;
    Ok(result)
}
pub fn weather_path(
    root: &Path,
    project: &Project,
    study: &Study,
) -> Result<Option<std::path::PathBuf>> {
    let binding = match &study.calculation {
        StudyMode::StaticOvercast => return Ok(None),
        StudyMode::WeatherHour { weather, .. } | StudyMode::Annual { weather, .. } => weather,
    };
    let reference = project
        .climate
        .as_ref()
        .context("Import weather first")?
        .weather
        .iter()
        .find(|reference| &reference.binding == binding)
        .context("Study weather is not retained in the project")?;
    regular_child(root, &format!("{}/asset.json", reference.path))?;
    Ok(Some(root.join(&reference.path)))
}
pub fn is_stale(record: &climate::ClimateRunRecord, model: &Model, study: &Study) -> Result<bool> {
    Ok(record.study.physical_input_hash(&record.model)? != study.physical_input_hash(model)?)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DerivedMetrics {
    pub run_id: String,
    pub metric_identity: String,
    pub settings: MetricSettings,
    pub metrics: Vec<SensorMetrics>,
}
pub fn derive_metrics(
    root: &Path,
    summary: &climate::ClimateRunSummary,
    settings: MetricSettings,
) -> Result<DerivedMetrics> {
    let (record, series) = climate::open_series_for_run(root, summary)?;
    ensure!(
        record.series.interval_count == 8760,
        "Annual metrics require a complete year"
    );
    Ok(DerivedMetrics {
        run_id: record.id.clone(),
        metric_identity: settings.identity(&climate::physical_result_fingerprint(&record)?)?,
        metrics: series.metrics(&settings)?,
        settings,
    })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultField {
    Illuminance,
    Da,
    Cda,
    Udi,
    Underlit,
    Overlit,
}
impl ResultField {
    pub fn metric(self, value: &SensorMetrics) -> Result<f64> {
        Ok(match self {
            Self::Da => value.da_percent,
            Self::Cda => value.cda_percent,
            Self::Udi => value.udi_percent,
            Self::Underlit => value.underlit_percent,
            Self::Overlit => value.overlit_percent,
            Self::Illuminance => anyhow::bail!("Hourly illuminance requires a weather interval"),
        })
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ClimateValues {
    pub run_id: String,
    pub field: ResultField,
    pub units: String,
    pub interval_index: Option<usize>,
    pub metric_identity: Option<String>,
    pub sensor_ids: Vec<String>,
    pub values: Vec<f64>,
    pub statistics: Value,
}
pub fn result_values(
    root: &Path,
    summary: &climate::ClimateRunSummary,
    field: ResultField,
    interval: usize,
    settings: Option<MetricSettings>,
) -> Result<ClimateValues> {
    let (record, series) = climate::open_series_for_run(root, summary)?;
    let (values, interval_index, metric_identity) = match field {
        ResultField::Illuminance => {
            let position = match record.study.calculation {
                StudyMode::WeatherHour { interval_index, .. } => {
                    ensure!(
                        interval == interval_index,
                        "Selected-hour run contains only weather interval {interval_index}"
                    );
                    0
                }
                StudyMode::Annual { .. } => interval,
                StudyMode::StaticOvercast => anyhow::bail!("Use the legacy static result reader"),
            };
            (series.hour(position)?, Some(interval), None)
        }
        _ => {
            ensure!(
                record.series.interval_count == 8760,
                "Annual metrics require all 8,760 intervals"
            );
            let settings = settings.unwrap_or_else(|| record.study.metrics.clone());
            let metrics = if settings == record.study.metrics {
                record.metrics.clone().context("Missing annual metrics")?
            } else {
                series.metrics(&settings)?
            };
            let values = metrics
                .iter()
                .map(|value| field.metric(value))
                .collect::<Result<Vec<_>>>()?;
            (
                values,
                None,
                Some(settings.identity(&climate::physical_result_fingerprint(&record)?)?),
            )
        }
    };
    Ok(ClimateValues {
        run_id: record.id,
        field,
        units: if matches!(field, ResultField::Illuminance) {
            "lx"
        } else {
            "% of occupied time"
        }
        .into(),
        interval_index,
        metric_identity,
        sensor_ids: record.series.sensor_ids,
        statistics: serde_json::to_value(greenlens_core::statistics(&values)?)?,
        values,
    })
}
#[derive(Debug, Serialize, Deserialize)]
pub struct ClimateComparison {
    pub a: ClimateValues,
    pub b: ClimateValues,
    pub deltas: Vec<f64>,
    pub delta_units: String,
    pub delta_statistics: Value,
    pub scale_maximum: f64,
    pub conditions: Value,
    pub design_differences: Value,
}
pub fn compare(
    root: &Path,
    a: &climate::ClimateRunSummary,
    b: &climate::ClimateRunSummary,
    field: ResultField,
    interval: usize,
) -> Result<ClimateComparison> {
    let record_a = climate::read_result_metadata(root, a)?;
    let record_b = climate::read_result_metadata(root, b)?;
    ensure!(
        record_a.alternative_id == "A" && record_b.alternative_id == "B",
        "Choose an A snapshot followed by a B snapshot"
    );
    ensure!(climate::comparison_fingerprint(&record_a)? == climate::comparison_fingerprint(&record_b)?, "Incompatible climate snapshots: weather, time axis, occupancy, thresholds, sensors, environment, sky method, engine or numerical profile differ. No resampling is performed");
    let a = result_values(root, a, field, interval, None)?;
    let b = result_values(root, b, field, interval, None)?;
    ensure!(
        a.sensor_ids == b.sensor_ids && a.values.len() == b.values.len(),
        "Comparison sensor definitions differ"
    );
    let deltas = a
        .values
        .iter()
        .zip(&b.values)
        .map(|(a, b)| b - a)
        .collect::<Vec<_>>();
    let scale_maximum = if matches!(field, ResultField::Illuminance) {
        a.values
            .iter()
            .chain(&b.values)
            .copied()
            .fold(0.0, f64::max)
    } else {
        100.0
    };
    Ok(ClimateComparison {
        a,
        b,
        delta_units: if matches!(field, ResultField::Illuminance) {
            "lx"
        } else {
            "percentage points"
        }
        .into(),
        delta_statistics: serde_json::to_value(greenlens_core::statistics(&deltas)?)?,
        deltas,
        scale_maximum,
        conditions: json!({"fingerprint":record_a.provenance["comparison_fingerprint"],"delta":"B minus A","percentage_delta":"omitted","summary":"unweighted sensor statistics, not a floor-area percentage"}),
        design_differences: json!({"a":{"window":record_a.model.window,"glazing":record_a.model.materials.visible_transmittance,"overhang":record_a.model.overhang},"b":{"window":record_b.model.window,"glazing":record_b.model.materials.visible_transmittance,"overhang":record_b.model.overhang}}),
    })
}

/// A history transaction stores one study and a legacy-absence marker, never
/// whole project evidence. Values are validated again when restored.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StudyHistorySnapshot {
    pub project_schema_version: String,
    pub study: Option<Study>,
}

pub fn restore_study(
    project: &Project,
    alternative: &str,
    snapshot: &StudyHistorySnapshot,
) -> Result<Project> {
    crate::validate_project(project)?;
    ensure!(
        match &snapshot.study {
            Some(_) => snapshot.project_schema_version == "0.3.0",
            None => snapshot.project_schema_version == crate::SCHEMA_VERSION,
        },
        "Study history schema and legacy absence marker differ"
    );
    let mut restored = configure(
        project,
        alternative,
        snapshot.study.clone().unwrap_or_default(),
    )?;
    if snapshot.study.is_none()
        && restored.climate.as_ref().is_some_and(|extension| {
            extension.weather.is_empty()
                && extension.runs.is_empty()
                && extension
                    .studies
                    .values()
                    .all(|study| study == &Study::default())
        })
    {
        restored.climate = None;
        restored.schema_version = snapshot.project_schema_version.clone();
    }
    // Preserve later weather, completed history and the other study. Their
    // presence legitimately prevents returning to an unchanged legacy document.
    crate::validate_project(&restored)?;
    Ok(restored)
}

/// Current physical evidence and requested metric identity are separate. A
/// metric-only edit may reuse `run`, but never marks its stored metrics current.
#[derive(Debug, Serialize, Deserialize)]
pub struct CurrentClimateResult {
    pub physical_input_hash: String,
    pub summary: Option<climate::ClimateRunSummary>,
    pub run: Option<climate::ClimateRunRecord>,
    pub stored_metric_identity: Option<String>,
    pub requested_metric_identity: Option<String>,
    pub metrics_current: bool,
}

/// Choose by verified completion time, then by manifest order for an exact tie.
/// This pure rule also supports restored models after undo. It does not replace
/// the explicit historical result selector, and accepts no unvalidated record.
pub fn current_from_records(
    model: &Model,
    alternative: &str,
    study: &Study,
    records: &[climate::ClimateRunRecord],
) -> Result<CurrentClimateResult> {
    ensure!(["A", "B"].contains(&alternative), "Unknown alternative");
    let physical_input_hash = study.physical_input_hash(model)?;
    let mut result = CurrentClimateResult {
        physical_input_hash,
        summary: None,
        run: None,
        stored_metric_identity: None,
        requested_metric_identity: None,
        metrics_current: false,
    };
    if matches!(study.calculation, StudyMode::StaticOvercast) {
        return Ok(result);
    }
    let mut latest = None;
    for (index, record) in records.iter().enumerate() {
        climate::validate_record(record)?;
        if record.alternative_id != alternative
            || record.study.physical_input_hash(&record.model)? != result.physical_input_hash
        {
            continue;
        }
        let finished = chrono::DateTime::parse_from_rfc3339(
            record.provenance["finished_utc"]
                .as_str()
                .context("Missing climate completion timestamp")?,
        )?;
        if latest
            .as_ref()
            .is_none_or(|(time, order, _)| (finished, index) > (*time, *order))
        {
            latest = Some((finished, index, record));
        }
    }
    if let Some((_, _, record)) = latest {
        if matches!(study.calculation, StudyMode::Annual { .. }) {
            let physical = climate::physical_result_fingerprint(record)?;
            result.stored_metric_identity = Some(record.study.metrics.identity(&physical)?);
            result.requested_metric_identity = Some(study.metrics.identity(&physical)?);
            result.metrics_current =
                result.stored_metric_identity == result.requested_metric_identity;
        }
        result.summary = Some(climate::summary(record)?);
        result.run = Some(record.clone());
    }
    Ok(result)
}

/// Saved-result lookup. Pending completed jobs remain visible through the
/// shared Application status until collected into their original project.
pub fn current_result(
    root: &Path,
    project: &Project,
    alternative: &str,
    study: &Study,
) -> Result<CurrentClimateResult> {
    crate::validate_project(project)?;
    let model = &project
        .alternatives
        .iter()
        .find(|item| item.id == alternative)
        .context("Alternative must be A or B")?
        .model;
    let physical = study.physical_input_hash(model)?;
    let records = project
        .climate
        .as_ref()
        .map(|extension| {
            extension
                .runs
                .iter()
                .filter(|summary| {
                    summary.alternative_id == alternative && summary.physical_input_hash == physical
                })
                .map(|summary| climate::read_result_metadata(root, summary))
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    current_from_records(model, alternative, study, &records)
}

#[cfg(test)]
mod current_result_tests {
    use super::*;

    #[test]
    fn newest_completion_wins_even_when_collection_order_is_reversed() {
        let tmp = tempfile::tempdir().unwrap();
        let old = crate::climate::tests::fixture(tmp.path(), true);
        let mut newest = old.clone();
        newest.id = Uuid::new_v4().to_string();
        newest.provenance["finished_utc"] = json!("2026-01-01T03:00:00Z");
        let selected =
            current_from_records(&old.model, "A", &old.study, &[newest.clone(), old.clone()])
                .unwrap();
        assert_eq!(selected.run.unwrap().id, newest.id);
        let mut tie = newest.clone();
        tie.id = Uuid::new_v4().to_string();
        let selected = current_from_records(
            &old.model,
            "A",
            &old.study,
            &[newest, old.clone(), tie.clone()],
        )
        .unwrap();
        assert_eq!(selected.run.unwrap().id, tie.id);
    }

    #[test]
    fn restored_geometry_recovers_matching_history_without_new_calculation() {
        let tmp = tempfile::tempdir().unwrap();
        let record = crate::climate::tests::fixture(tmp.path(), false);
        let records = std::slice::from_ref(&record);
        let mut edited = record.model.clone();
        edited.window.width_m += 0.1;
        let stale = current_from_records(&edited, "A", &record.study, records).unwrap();
        assert!(stale.run.is_none());
        assert!(stale.requested_metric_identity.is_none());
        assert!(!stale.metrics_current);
        let restored = current_from_records(&record.model, "A", &record.study, records).unwrap();
        assert_eq!(restored.run.unwrap().id, record.id);
        assert!(restored.stored_metric_identity.is_none());
        let other = current_from_records(&record.model, "B", &record.study, records).unwrap();
        assert!(other.run.is_none());
    }

    #[test]
    fn metric_changes_reuse_physical_result_but_invalidate_stored_metric_identity() {
        let tmp = tempfile::tempdir().unwrap();
        let record = crate::climate::tests::fixture(tmp.path(), true);
        let records = std::slice::from_ref(&record);
        let current = current_from_records(&record.model, "A", &record.study, records).unwrap();
        assert!(current.metrics_current);
        let mut changed = record.study.clone();
        changed.metrics.da_threshold_lux = 500.0;
        let revised = current_from_records(&record.model, "A", &changed, records).unwrap();
        assert_eq!(revised.physical_input_hash, current.physical_input_hash);
        assert_eq!(revised.run.unwrap().id, record.id);
        assert_ne!(
            revised.stored_metric_identity,
            revised.requested_metric_identity
        );
        assert!(!revised.metrics_current);
    }

    #[test]
    fn false_or_non_utc_completion_cannot_influence_current_selection() {
        let tmp = tempfile::tempdir().unwrap();
        let record = crate::climate::tests::fixture(tmp.path(), false);
        for timestamp in ["not-a-date", "2026-01-01T09:00:00+08:00"] {
            let mut forged = record.clone();
            forged.provenance["finished_utc"] = json!(timestamp);
            assert!(current_from_records(&record.model, "A", &record.study, &[forged]).is_err());
        }
    }
}

#[cfg(test)]
mod study_history_tests {
    use super::*;

    fn absent() -> StudyHistorySnapshot {
        StudyHistorySnapshot {
            project_schema_version: crate::SCHEMA_VERSION.into(),
            study: None,
        }
    }

    #[test]
    fn first_study_edit_undo_restores_exact_legacy_absence() {
        let original = crate::example();
        let mut study = Study::default();
        study.metrics.da_threshold_lux = 500.0;
        let edited = configure(&original, "A", study.clone()).unwrap();
        let restored = restore_study(&edited, "A", &absent()).unwrap();
        assert_eq!(
            serde_json::to_value(&restored).unwrap(),
            serde_json::to_value(original).unwrap()
        );
        let redone = restore_study(
            &restored,
            "A",
            &StudyHistorySnapshot {
                project_schema_version: "0.3.0".into(),
                study: Some(study),
            },
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(redone).unwrap(),
            serde_json::to_value(edited).unwrap()
        );
    }

    #[test]
    fn undo_does_not_erase_later_weather_run_or_other_alternative_study() {
        let temporary = tempfile::tempdir().unwrap();
        let record = crate::climate::tests::fixture(temporary.path(), false);
        let original = crate::example();
        let mut edited = configure(&original, "A", Study::default()).unwrap();
        let extension = edited.climate.as_mut().unwrap();
        let weather_id = Uuid::new_v4().to_string();
        extension.weather.push(climate::WeatherReference {
            path: format!("weather/{weather_id}"),
            id: weather_id,
            binding: record.weather_asset.binding.clone(),
        });
        extension.runs.push(climate::summary(&record).unwrap());
        extension.studies.insert("B".into(), record.study.clone());
        let expected = serde_json::to_value(&edited.climate).unwrap();
        let restored = restore_study(&edited, "A", &absent()).unwrap();
        assert_eq!(restored.schema_version, "0.3.0");
        assert_eq!(serde_json::to_value(&restored.climate).unwrap(), expected);
        assert_eq!(serde_json::to_value(&edited.climate).unwrap(), expected);
    }

    #[test]
    fn malformed_history_or_unretained_weather_is_rejected_without_mutation() {
        let original = crate::example();
        let signature = serde_json::to_value(&original).unwrap();
        assert!(restore_study(
            &original,
            "A",
            &StudyHistorySnapshot {
                project_schema_version: "0.3.0".into(),
                study: None,
            }
        )
        .is_err());
        let temporary = tempfile::tempdir().unwrap();
        let record = crate::climate::tests::fixture(temporary.path(), false);
        assert!(restore_study(
            &original,
            "A",
            &StudyHistorySnapshot {
                project_schema_version: "0.3.0".into(),
                study: Some(record.study),
            }
        )
        .is_err());
        assert_eq!(serde_json::to_value(original).unwrap(), signature);
    }
}
