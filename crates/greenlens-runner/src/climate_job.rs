//! One shared worker for desktop and CLI climate studies.
use crate::{
    climate::{self, ClimateRunRecord},
    persistence::*,
    timeseries::{SeriesReader, SeriesWriter},
    weather,
};
use anyhow::{ensure, Result};
use chrono::Utc;
use greenlens_core::{
    study::{NumericalProfile, Study, StudyMode},
    validate_model, Model,
};
use greenlens_radiance::{
    annual::{self, AnnualProfile},
    Engine,
};
use serde_json::{json, Value};
use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    id: &str,
    model: Model,
    alternative: String,
    engine_dir: Option<&Path>,
    study: Study,
    weather_root: &Path,
    scratch: &Path,
    cancel: &AtomicBool,
    log: &dyn Fn(&str),
) -> Result<ClimateRunRecord> {
    let started = Utc::now().to_rfc3339();
    study.validate(&model)?;
    let (expected_weather, selected, numerical) = match &study.calculation {
        StudyMode::WeatherHour {
            weather,
            interval_index,
            profile,
        } => (weather, Some(*interval_index), *profile),
        StudyMode::Annual { weather, profile } => (weather, None, *profile),
        StudyMode::StaticOvercast => anyhow::bail!("Static studies use the legacy worker"),
    };
    ensure!(!cancel.load(Ordering::SeqCst), "Cancelled");
    let (asset, weather_data) = weather::open(weather_root)?;
    ensure!(
        &asset.binding == expected_weather,
        "Study weather identity differs from imported asset"
    );
    // Snapshot every required file before invoking the engine. Later source edits
    // cannot alter the immutable weather used for this job or its provenance.
    let retained_weather = scratch.join("weather");
    fs::create_dir(&retained_weather)?;
    for name in [
        "asset.json",
        "weather.json",
        "original.epw",
        "normalized.epw",
        "checksums.json",
    ] {
        fs::copy(
            regular_child(weather_root, name)?,
            retained_weather.join(name),
        )?;
    }
    let (retained_asset, retained_data) = weather::open(&retained_weather)?;
    ensure!(
        asset == retained_asset && retained_data.normalized_epw == weather_data.normalized_epw,
        "Weather changed during snapshot; no simulation was started"
    );
    let validated = validate_model(&model)?;
    let engine = Engine::discover(engine_dir)?;
    let profile = match numerical {
        NumericalProfile::Shipping => AnnualProfile::shipping(),
        NumericalProfile::Reference => AnnualProfile::reference(),
    };
    let mut output = annual::simulate(
        &engine,
        &validated,
        &retained_data.normalized_epw,
        selected,
        &profile,
        scratch,
        cancel,
        log,
    )?;
    ensure!(!cancel.load(Ordering::SeqCst), "Cancelled");
    log("Validating and retaining complete hourly results");
    let expected_indices =
        selected.map_or_else(|| (0..8760).collect::<Vec<_>>(), |index| vec![index]);
    ensure!(
        output.interval_indices == expected_indices
            && output.sensor_count == validated.sensor_grid.sensors.len(),
        "Climate output time/sensor shape differs from study"
    );
    let mut writer = SeriesWriter::create(
        &scratch.join("series"),
        validated
            .sensor_grid
            .sensors
            .iter()
            .map(|s| s.id.clone())
            .collect(),
        expected_indices.len(),
    )?;
    output.visit_hours(|interval, rgb| {
        ensure!(!cancel.load(Ordering::SeqCst), "Cancelled");
        climate::validate_zero_solar_output(
            &retained_data.data,
            interval,
            rgb.iter().flatten().copied(),
        )?;
        writer.push_hour(rgb)
    })?;
    let series = writer.finish()?;
    let metrics = if selected.is_none() {
        Some(SeriesReader::open(&scratch.join("series"))?.metrics(&study.metrics)?)
    } else {
        None
    };
    let provenance = json!({
        "schema_version":climate::RUN_SCHEMA,"application_version":env!("CARGO_PKG_VERSION"),
        "source_commit":option_env!("GREENLENS_BUILD_SOURCE_COMMIT"),
        "source_worktree_dirty":option_env!("GREENLENS_BUILD_SOURCE_DIRTY").map(|value| value == "true"),
        "adapter_version":annual::ADAPTER_VERSION,"calculation_label":if selected.is_none() {climate::ANNUAL_LABEL} else {climate::HOUR_LABEL},
        "state":"completed","units":"lx","started_utc":started,"finished_utc":Utc::now().to_rfc3339(),
        "engine":output.identity,"profile":profile,"processes":output.processes,
        "input_hash":validated.normalized_input_hash,"sensor_hash":validated.sensor_hash,
        "physical_input_hash":study.physical_input_hash(&model)?,"time_axis_sha256":asset.binding.time_axis_sha256,
        "child_environment":{"RAYPATH":".;<compatible-engine>/lib"},
        "recipe_sha256":crate::persistence::bytes_hash(&std::fs::read(scratch.join("annual-recipe.json"))?),
        "solar_sources_sha256":crate::persistence::bytes_hash(&std::fs::read(scratch.join("annual-suns.rad"))?),
        "solar_modifiers_sha256":crate::persistence::bytes_hash(&std::fs::read(scratch.join("annual-sun-modifiers.txt"))?),
        "recipe":"Regenerate disposable matrices and octrees through the supported adapter. Never execute imported recipe arguments.",
        "raw_representation":"Parsed ASCII engine RGB retained as float64 little-endian in time/sensor/channel chunks; original large text matrices are regenerable."
    });
    let mut record = ClimateRunRecord {
        schema_version: climate::RUN_SCHEMA.into(),
        id: id.into(),
        alternative_id: alternative,
        model,
        validated,
        study,
        weather_asset: asset,
        series,
        metrics,
        provenance,
    };
    record.provenance["run_fingerprint"] = json!(climate::run_fingerprint(&record)?);
    record.provenance["comparison_fingerprint"] = json!(climate::comparison_fingerprint(&record)?);
    let result_fingerprint = climate::physical_result_fingerprint(&record)?;
    record.provenance["physical_result_fingerprint"] = json!(result_fingerprint);
    record.provenance["metric_identity"] = if selected.is_none() {
        json!(record.study.metrics.identity(&result_fingerprint)?)
    } else {
        Value::Null
    };
    climate::validate_record(&record)?;
    write_json(&scratch.join("input.json"), &record.model)?;
    write_json(&scratch.join("study.json"), &record.study)?;
    write_json(&scratch.join("results.json"), &record)?;
    write_json(&scratch.join("provenance.json"), &record.provenance)?;
    output.resources.record_final_scratch(scratch)?;
    let files = climate::evidence_files(&record);
    seal(
        scratch,
        &files.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    climate::read_run_directory(scratch)?;
    ensure!(!cancel.load(Ordering::SeqCst), "Cancelled");
    Ok(record)
}
