//! Bounded climate application actions shared by desktop and CLI.
use crate::{climate, persistence::*, weather, Project, ProjectView};
use anyhow::{ensure, Context, Result};
use greenlens_core::{
    study::{MetricSettings, SensorMetrics, Study, StudyMode, WeatherBinding},
    weather::{ParsedWeather, WeatherHour, WeatherInterval, WeatherSourceProfile},
    Model,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use uuid::Uuid;

/// Read only the selected retained weather asset. No engine or historical run
/// scan is needed; original weather bytes and their normalized axis are verified.
pub fn project_weather(
    root: &Path,
    weather_id: &str,
) -> Result<(weather::WeatherAsset, ParsedWeather)> {
    let project: Project = read_json(&regular_child(root, "project.json")?)?;
    crate::validate_project(&project)?;
    let reference = project
        .climate
        .as_ref()
        .and_then(|extension| extension.weather.iter().find(|item| item.id == weather_id))
        .context("Weather asset is not retained in this project")?;
    regular_child(root, &format!("{}/asset.json", reference.path))?;
    let (asset, parsed) = weather::open(&root.join(&reference.path))?;
    ensure!(
        asset.binding == reference.binding,
        "Project weather identity differs from retained asset"
    );
    Ok((asset, parsed))
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeatherIntervalView {
    pub weather_id: String,
    pub binding: WeatherBinding,
    pub hour: WeatherHour,
    pub interval: WeatherInterval,
}

fn weather_interval_view(
    weather_id: &str,
    parsed: &ParsedWeather,
    index: usize,
) -> Result<WeatherIntervalView> {
    let interval = parsed.data.interval(index)?;
    Ok(WeatherIntervalView {
        weather_id: weather_id.into(),
        binding: parsed.data.binding(),
        hour: parsed.data.hours[index].clone(),
        interval,
    })
}

pub fn weather_interval(
    root: &Path,
    weather_id: &str,
    interval_index: usize,
) -> Result<WeatherIntervalView> {
    let (_, parsed) = project_weather(root, weather_id)?;
    weather_interval_view(weather_id, &parsed, interval_index)
}

/// Month/day and EPW hour-ending labels address the retained local-standard
/// axis. In particular, December 31 hour 24 is the final interval, not a request
/// for a new calendar year. UTC and midpoint mapping come from the parsed axis.
pub fn weather_find_interval(
    root: &Path,
    weather_id: &str,
    month: u8,
    day: u8,
    hour_ending: u8,
) -> Result<WeatherIntervalView> {
    ensure!(
        (1..=12).contains(&month) && (1..=31).contains(&day) && (1..=24).contains(&hour_ending),
        "Weather selection requires month 1–12, day 1–31 and EPW hour ending 1–24 in local standard time"
    );
    let (_, parsed) = project_weather(root, weather_id)?;
    let hour = parsed.data.hours.iter().find(|hour| {
        hour.month == month && hour.day == day && hour.hour_ending == hour_ending
    }).context("Date/hour is absent from the retained non-leap weather year; February 29 and invalid month/day combinations are unsupported")?;
    weather_interval_view(weather_id, &parsed, hour.index)
}

#[cfg(test)]
#[path = "weather_navigation_tests.rs"]
mod weather_navigation_tests;

/// Native import needs only an EPW file selection and its source declaration.
/// Temporary assets stay outside the synced project; portable attachment uses
/// the same revision guard and verified evidence copy as the CLI import route.
pub fn import_weather_file(
    root: &Path,
    project: &Project,
    expected: Option<&str>,
    file: &Path,
    profile: WeatherSourceProfile,
    source: weather::WeatherSource,
) -> Result<ProjectView> {
    import_weather_file_in_cache(
        root,
        project,
        expected,
        file,
        profile,
        source,
        &crate::local_root()?.join("weather-imports"),
    )
}

fn import_weather_file_in_cache(
    root: &Path,
    project: &Project,
    expected: Option<&str>,
    file: &Path,
    profile: WeatherSourceProfile,
    source: weather::WeatherSource,
    cache: &Path,
) -> Result<ProjectView> {
    crate::validate_project(project)?;
    // Fail an existing conflict before doing an unnecessary weather import.
    check_revision(root, expected)?;
    // Keep the resolved cache directory stable while its owned temporary child
    // is created and removed. The importer has its own lock inside that child.
    let _cache_lock = WriterLock::acquire(cache)?;
    let mut temporary = WeatherImportTemporary::new(cache)?;
    let asset = temporary.path.join("asset");
    let result = (|| {
        weather::import(file, &asset, profile, source)?;
        // Rechecks the revision after import; an external edit is never overwritten.
        import_weather_asset(root, project, expected, &asset)
    })();
    let cleanup = temporary.cleanup();
    match (result, cleanup) {
        (Ok(view), Ok(())) => Ok(view),
        (Ok(_), Err(error)) => Err(error.context("Weather was attached, but temporary cache cleanup failed; reopen the project before continuing")),
        (Err(error), Ok(())) => Err(error),
        (Err(error), Err(cleanup)) => Err(error.context(format!("Temporary weather cache cleanup also failed: {cleanup:#}"))),
    }
}

struct WeatherImportTemporary {
    cache: std::path::PathBuf,
    path: std::path::PathBuf,
    removed: bool,
}
impl WeatherImportTemporary {
    fn new(cache: &Path) -> Result<Self> {
        let cache = std::fs::canonicalize(cache)?;
        let path = cache.join(Uuid::new_v4().to_string());
        std::fs::create_dir(&path)?;
        Ok(Self {
            cache,
            path,
            removed: false,
        })
    }

    fn cleanup(&mut self) -> Result<()> {
        if self.removed {
            return Ok(());
        }
        // Never remove a substituted path or anything beyond this newly
        // created UUID directory. remove_dir_all does not follow child links.
        ensure!(
            self.path.parent() == Some(self.cache.as_path())
                && std::fs::canonicalize(&self.path)? == self.path
                && Uuid::parse_str(
                    self.path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .context("Temporary cache name is not UTF-8")?
                )?
                .to_string()
                    == self.path.file_name().unwrap().to_str().unwrap(),
            "Temporary weather cache path changed; automatic cleanup refused"
        );
        std::fs::remove_dir_all(&self.path).context("Remove owned temporary weather import")?;
        self.removed = true;
        Ok(())
    }
}
impl Drop for WeatherImportTemporary {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

#[cfg(test)]
mod weather_file_tests {
    use super::*;
    use greenlens_core::weather::synthetic_epw;
    use std::fs;

    fn source() -> weather::WeatherSource {
        weather::WeatherSource {
            citation: "Original synthetic test year, not climatic data".into(),
            source_url: None,
            license: "Apache-2.0".into(),
            rights_note: "Original licensed GreenLens fixture".into(),
            synthetic_non_climatic: true,
        }
    }

    #[test]
    fn direct_file_import_retains_portable_bytes_after_input_and_cache_disappear() {
        let input = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let weather_file = input.path().join("original.epw");
        let bytes = synthetic_epw();
        fs::write(&weather_file, &bytes).unwrap();
        let saved = crate::save_project(project.path(), &crate::example(), None).unwrap();
        let imported = import_weather_file_in_cache(
            project.path(),
            &saved.project,
            Some(&saved.revision),
            &weather_file,
            WeatherSourceProfile::EpwHourEnding,
            source(),
            &input.path().join("cache"),
        )
        .unwrap();
        let reference = &imported.project.climate.as_ref().unwrap().weather[0];
        assert_eq!(
            fs::read(project.path().join(&reference.path).join("original.epw")).unwrap(),
            bytes
        );
        input.close().unwrap();
        let reopened = crate::open_project(project.path()).unwrap();
        assert_eq!(reopened.revision, imported.revision);
        assert_eq!(reopened.project.climate.as_ref().unwrap().weather.len(), 1);
    }

    #[test]
    fn direct_file_import_conflict_or_invalid_weather_preserves_project() {
        let input = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let weather_file = input.path().join("invalid.epw");
        fs::write(&weather_file, b"not an EPW").unwrap();
        let saved = crate::save_project(project.path(), &crate::example(), None).unwrap();
        let before = fs::read(project.path().join("project.json")).unwrap();
        assert!(import_weather_file_in_cache(
            project.path(),
            &saved.project,
            Some(&saved.revision),
            &weather_file,
            WeatherSourceProfile::EpwHourEnding,
            source(),
            &input.path().join("cache")
        )
        .is_err());
        assert_eq!(
            fs::read(project.path().join("project.json")).unwrap(),
            before
        );
        assert!(fs::read_dir(input.path().join("cache"))
            .unwrap()
            .all(|entry| entry.unwrap().file_name() == ".greenlens.lock"));
        let mut external = saved.project.clone();
        external.name = "External edit retained".into();
        atomic_json(project.path(), &external).unwrap();
        let external_bytes = fs::read(project.path().join("project.json")).unwrap();
        let unused_cache = input.path().join("not-created");
        let error = import_weather_file_in_cache(
            project.path(),
            &saved.project,
            Some(&saved.revision),
            &weather_file,
            WeatherSourceProfile::EpwHourEnding,
            source(),
            &unused_cache,
        )
        .unwrap_err();
        assert!(error.to_string().contains("changed externally"));
        assert!(!unused_cache.exists());
        assert_eq!(
            fs::read(project.path().join("project.json")).unwrap(),
            external_bytes
        );
    }

    #[test]
    fn repeated_imports_remove_only_their_temporary_assets() {
        let input = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let cache = input.path().join("cache");
        let unrelated = cache.join("unrelated");
        fs::create_dir_all(&unrelated).unwrap();
        fs::write(unrelated.join("keep.txt"), "not owned by import").unwrap();
        let weather_file = input.path().join("source.epw");
        fs::write(&weather_file, synthetic_epw()).unwrap();
        let mut saved = crate::save_project(project.path(), &crate::example(), None).unwrap();
        for _ in 0..2 {
            saved = import_weather_file_in_cache(
                project.path(),
                &saved.project,
                Some(&saved.revision),
                &weather_file,
                WeatherSourceProfile::EpwHourEnding,
                source(),
                &cache,
            )
            .unwrap();
            assert_eq!(saved.project.climate.as_ref().unwrap().weather.len(), 1);
            assert!(fs::read_dir(&cache).unwrap().all(|entry| {
                let name = entry.unwrap().file_name();
                name == ".greenlens.lock" || name == "unrelated"
            }));
            assert_eq!(
                fs::read_to_string(unrelated.join("keep.txt")).unwrap(),
                "not owned by import"
            );
        }
    }
}

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
    project.schema_version = if project.architecture.is_some() {
        "0.6.0"
    } else if project.energy.is_some() {
        "0.4.0"
    } else {
        "0.3.0"
    }
    .into();
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
    result.schema_version = if result.architecture.is_some() {
        "0.6.0"
    } else if result.energy.is_some() {
        "0.4.0"
    } else {
        "0.3.0"
    }
    .into();
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
    climate::validate_record(record)?;
    Ok(!matches_current_physics(record, model, study)?)
}

/// The study's Shipping/Reference role is not a complete numerical identity.
/// Historical profile validation may expand without changing what a new run
/// requests. Keep that distinction outside immutable stored fingerprints.
fn matches_current_physics(
    record: &climate::ClimateRunRecord,
    model: &Model,
    study: &Study,
) -> Result<bool> {
    if record.study.physical_input_hash(&record.model)? != study.physical_input_hash(model)? {
        return Ok(false);
    }
    let profile = match study.calculation {
        StudyMode::WeatherHour { profile, .. } | StudyMode::Annual { profile, .. } => profile,
        StudyMode::StaticOvercast => return Ok(false),
    };
    let requested = match profile {
        greenlens_core::study::NumericalProfile::Shipping => {
            greenlens_radiance::annual::AnnualProfile::shipping()
        }
        greenlens_core::study::NumericalProfile::Reference => {
            greenlens_radiance::annual::AnnualProfile::reference()
        }
    };
    matches_requested_numerics(record, &requested)
}

/// Call only after validate_record has checked the retained engine's pinned
/// package, executable/library policy and fingerprints. Inspection does not
/// require an installed engine; replay separately checks its complete live map.
fn matches_requested_numerics(
    record: &climate::ClimateRunRecord,
    requested: &greenlens_radiance::annual::AnnualProfile,
) -> Result<bool> {
    use greenlens_radiance::{annual, ENGINE_VERSION};
    let engine: annual::AnnualEngineIdentity =
        serde_json::from_value(record.provenance["engine"].clone())?;
    let retained: annual::AnnualProfile =
        serde_json::from_value(record.provenance["profile"].clone())?;
    Ok(engine.version == ENGINE_VERSION
        && engine.adapter_version == annual::ADAPTER_VERSION
        && engine.method_version == annual::METHOD_VERSION
        && record.provenance["adapter_version"] == annual::ADAPTER_VERSION
        && greenlens_core::canonical_hash(&retained)? == greenlens_core::canonical_hash(requested)?)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DerivedMetrics {
    pub run_id: String,
    pub metric_identity: String,
    pub settings: MetricSettings,
    pub metrics: Vec<SensorMetrics>,
    pub assessment: crate::qualification::RunAssessment,
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
        assessment: crate::qualification::from_record(
            &record,
            None,
            crate::qualification::EvidenceIntegrity::SealedMetadataChecked,
        )?,
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
    pub assessment: crate::qualification::RunAssessment,
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
        assessment: crate::qualification::from_record(
            &record,
            None,
            crate::qualification::EvidenceIntegrity::SealedMetadataChecked,
        )?,
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
            Some(_) =>
                snapshot.project_schema_version == "0.3.0"
                    || (snapshot.project_schema_version == "0.4.0" && project.energy.is_some())
                    || (snapshot.project_schema_version == "0.6.0"
                        && project.architecture.is_some()),
            None =>
                snapshot.project_schema_version == crate::SCHEMA_VERSION
                    || (snapshot.project_schema_version == "0.4.0" && project.energy.is_some())
                    || (snapshot.project_schema_version == "0.6.0"
                        && project.architecture.is_some()),
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
        restored.schema_version = if restored.architecture.is_some() {
            "0.6.0".into()
        } else if restored.energy.is_some() {
            "0.4.0".into()
        } else {
            snapshot.project_schema_version.clone()
        };
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
    pub assessment: Option<crate::qualification::RunAssessment>,
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
        assessment: None,
    };
    if matches!(study.calculation, StudyMode::StaticOvercast) {
        return Ok(result);
    }
    let mut latest = None;
    for (index, record) in records.iter().enumerate() {
        climate::validate_record(record)?;
        if record.alternative_id != alternative || !matches_current_physics(record, model, study)? {
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
        let mut assessment = crate::qualification::from_record(
            record,
            None,
            crate::qualification::EvidenceIntegrity::ValidatedRecordOnly,
        )?;
        assessment.current_input_relation.physical = crate::qualification::InputRelation::Matches;
        assessment.current_input_relation.metrics =
            if !matches!(study.calculation, StudyMode::Annual { .. }) {
                crate::qualification::InputRelation::NotApplicable
            } else if result.metrics_current {
                crate::qualification::InputRelation::Matches
            } else {
                crate::qualification::InputRelation::Differs
            };
        result.assessment = Some(assessment);
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
    let mut result = current_from_records(model, alternative, study, &records)?;
    if let Some(assessment) = &mut result.assessment {
        assessment.evidence_integrity =
            crate::qualification::EvidenceIntegrity::SealedMetadataChecked;
    }
    Ok(result)
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
        let original_assessment = current.assessment.unwrap();
        let derived_assessment = revised.assessment.unwrap();
        assert_eq!(
            original_assessment.current_input_relation.metrics,
            crate::qualification::InputRelation::Matches
        );
        assert_eq!(
            derived_assessment.current_input_relation.physical,
            crate::qualification::InputRelation::Matches
        );
        assert_eq!(
            derived_assessment.current_input_relation.metrics,
            crate::qualification::InputRelation::Differs
        );
        assert_eq!(
            original_assessment.scientific_qualification.status,
            derived_assessment.scientific_qualification.status
        );
        assert_eq!(
            derived_assessment.evidence_integrity,
            crate::qualification::EvidenceIntegrity::ValidatedRecordOnly
        );
    }

    #[test]
    fn explicit_older_series_has_its_own_current_metric_identity() {
        // Synthetic output checks identity selection only, never engine accuracy.
        let tmp = tempfile::tempdir().unwrap();
        let old = crate::climate::tests::fixture(tmp.path(), true);
        let newer_series = tempfile::tempdir().unwrap();
        let series_path = newer_series.path().join("series");
        let mut writer = crate::timeseries::SeriesWriter::create(
            &series_path,
            old.validated
                .sensor_grid
                .sensors
                .iter()
                .map(|sensor| sensor.id.clone())
                .collect(),
            greenlens_core::study::HOURS_PER_YEAR,
        )
        .unwrap();
        for index in 0..greenlens_core::study::HOURS_PER_YEAR {
            let rgb = if index == 8 { [1.0; 3] } else { [0.0; 3] };
            writer
                .push_hour(&vec![rgb; old.validated.sensor_grid.sensors.len()])
                .unwrap();
        }
        let mut newer = old.clone();
        newer.id = Uuid::new_v4().to_string();
        newer.series = writer.finish().unwrap();
        newer.metrics = Some(
            crate::timeseries::SeriesReader::open(&series_path)
                .unwrap()
                .metrics(&newer.study.metrics)
                .unwrap(),
        );
        newer.provenance["finished_utc"] = json!("2026-01-01T03:00:00Z");
        let physical = climate::physical_result_fingerprint(&newer).unwrap();
        newer.provenance["physical_result_fingerprint"] = json!(physical);
        newer.provenance["metric_identity"] =
            json!(newer.study.metrics.identity(&physical).unwrap());
        assert_ne!(
            old.series.identity().unwrap(),
            newer.series.identity().unwrap()
        );
        let latest =
            current_from_records(&old.model, "A", &old.study, &[old.clone(), newer.clone()])
                .unwrap();
        let selected =
            current_from_records(&old.model, "A", &old.study, std::slice::from_ref(&old)).unwrap();
        assert_eq!(latest.run.unwrap().id, newer.id);
        assert_eq!(selected.run.unwrap().id, old.id);
        assert_eq!(latest.physical_input_hash, selected.physical_input_hash);
        assert!(latest.metrics_current && selected.metrics_current);
        assert_ne!(
            latest.requested_metric_identity,
            selected.requested_metric_identity
        );
        let mut edited = old.study.clone();
        edited.metrics.da_threshold_lux = 500.0;
        let selected_edited =
            current_from_records(&old.model, "A", &edited, std::slice::from_ref(&old)).unwrap();
        assert_eq!(selected_edited.run.unwrap().id, old.id);
        assert!(!selected_edited.metrics_current);
        assert_eq!(
            selected_edited.stored_metric_identity,
            selected.stored_metric_identity
        );
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

    #[test]
    fn unsupported_numerics_cannot_be_labeled_current() {
        let tmp = tempfile::tempdir().unwrap();
        let record = crate::climate::tests::fixture(tmp.path(), false);
        let mut changed_profile = record.clone();
        changed_profile.provenance["profile"]["ambient_divisions"] = json!(131073);
        let mut changed_method = record.clone();
        changed_method.provenance["engine"]["method_version"] = json!("unsupported-method/999");
        for unsupported in [changed_profile, changed_method] {
            assert!(is_stale(&unsupported, &record.model, &record.study).is_err());
            assert!(
                current_from_records(&record.model, "A", &record.study, &[unsupported],).is_err()
            );
        }
    }

    #[test]
    fn same_role_does_not_hide_changed_complete_sampling_or_method() {
        let tmp = tempfile::tempdir().unwrap();
        let record = crate::climate::tests::fixture(tmp.path(), false);
        climate::validate_record(&record).unwrap();
        let stored_bytes = serde_json::to_vec(&record).unwrap();
        let profile = greenlens_radiance::annual::AnnualProfile::shipping();
        assert!(matches_requested_numerics(&record, &profile).unwrap());
        // Model a future application's numerical request without registering
        // experimental settings or rewriting this valid historical fixture.
        let mut next_request = profile.clone();
        next_request.ambient_divisions *= 2;
        assert_eq!(next_request.name, profile.name);
        assert!(!matches_requested_numerics(&record, &next_request).unwrap());
        next_request = profile.clone();
        next_request.direct_ambient_divisions *= 2;
        assert!(!matches_requested_numerics(&record, &next_request).unwrap());
        next_request = profile.clone();
        next_request.direct_solar_samples *= 4;
        assert!(!matches_requested_numerics(&record, &next_request).unwrap());
        next_request = profile.clone();
        next_request.coefficient_primary_repetitions = Some(256);
        assert!(!matches_requested_numerics(&record, &next_request).unwrap());
        next_request = profile.clone();
        next_request.coefficient_receiver_scale = Some(1000.0);
        assert!(!matches_requested_numerics(&record, &next_request).unwrap());
        next_request = profile.clone();
        next_request.ray_depth = 16;
        assert!(!matches_requested_numerics(&record, &next_request).unwrap());
        let mut other_method = record.clone();
        other_method.provenance["engine"]["method_version"] = json!("historical-method/0");
        assert!(!matches_requested_numerics(&other_method, &profile).unwrap());
        assert_eq!(serde_json::to_vec(&record).unwrap(), stored_bytes);
        assert!(!is_stale(&record, &record.model, &record.study).unwrap());
    }

    #[test]
    fn accepted_historical_method_stays_inspectable_but_never_current() {
        use greenlens_radiance::annual;
        for method in [
            annual::LEGACY_METHOD_VERSION,
            annual::EXPLICIT_METHOD_VERSION,
            annual::REPEATED_METHOD_VERSION,
        ] {
            let temporary = tempfile::tempdir().unwrap();
            let mut historical = crate::climate::tests::fixture_for_method(
                &temporary.path().join("historical"),
                false,
                method,
            );
            let current = crate::climate::tests::fixture(&temporary.path().join("current"), false);
            // Even a later-collected historical calculation cannot displace the
            // actual current-method result with the same model/study role.
            historical.provenance["finished_utc"] = json!("2026-01-02T00:00:00Z");
            let original = serde_json::to_vec(&historical).unwrap();
            climate::validate_record(&historical).unwrap();
            assert!(is_stale(&historical, &historical.model, &historical.study).unwrap());
            assert!(!is_stale(&current, &current.model, &current.study).unwrap());
            assert_eq!(
                historical
                    .study
                    .physical_input_hash(&historical.model)
                    .unwrap(),
                current.study.physical_input_hash(&current.model).unwrap()
            );
            let selected = current_from_records(
                &current.model,
                "A",
                &current.study,
                &[current.clone(), historical.clone()],
            )
            .unwrap();
            assert_eq!(selected.run.unwrap().id, current.id);
            let history_only = current_from_records(
                &historical.model,
                "A",
                &historical.study,
                std::slice::from_ref(&historical),
            )
            .unwrap();
            assert!(history_only.run.is_none() && history_only.summary.is_none());
            assert_eq!(serde_json::to_vec(&historical).unwrap(), original);
            assert_ne!(
                climate::comparison_fingerprint(&historical).unwrap(),
                climate::comparison_fingerprint(&current).unwrap()
            );
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
