//! One supervised energy worker shared by CLI and native desktop.
use crate::{
    energy::{self, EnergyProvenance, EnergyRunRecord},
    energy_series::{EnergyChannel, EnergySeriesWriter},
    energy_weather,
    persistence::*,
};
use anyhow::{ensure, Result};
use chrono::Utc;
use greenlens_core::{canonical_hash, energy::*};
use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    id: &str,
    model: ThermalModel,
    alternative: String,
    engine_dir: Option<&Path>,
    study: EnergyStudy,
    weather_root: &Path,
    scratch: &Path,
    cancel: &AtomicBool,
    log: &dyn Fn(&str),
) -> Result<EnergyRunRecord> {
    run_expected(
        id,
        model,
        alternative,
        engine_dir,
        study,
        weather_root,
        scratch,
        cancel,
        log,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_expected(
    id: &str,
    model: ThermalModel,
    alternative: String,
    engine_dir: Option<&Path>,
    study: EnergyStudy,
    weather_root: &Path,
    scratch: &Path,
    cancel: &AtomicBool,
    log: &dyn Fn(&str),
    expected: Option<&EnergyMethodIdentity>,
) -> Result<EnergyRunRecord> {
    let started = Utc::now().to_rfc3339();
    study.validate(&model)?;
    let (asset, parsed) = energy_weather::open(weather_root)?;
    validate_weather_context(&model, &study, &parsed.data)?;
    ensure!(!cancel.load(Ordering::SeqCst), "Energy run cancelled");
    let engine = if let Some(path) = engine_dir {
        greenlens_energyplus::EngineConfig::from_directory(path)
    } else {
        greenlens_energyplus::EngineConfig::discover()?
    };
    let method = greenlens_energyplus::method_identity(&study)?;
    if let Some(expected) = expected {
        ensure!(expected==&method,"The retained energy method is not recognized by this replay adapter; no imported recipe was executed");
    }
    log("Validating thermal inputs, native engine inventory and exact numerical recipe");
    let prepared =
        greenlens_energyplus::prepare(&engine, &model, &study, &parsed.normalized_epw, scratch)?;
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
    let (retained_asset, retained_data) = energy_weather::open(&retained_weather)?;
    ensure!(
        retained_asset == asset && retained_data.normalized_epw == parsed.normalized_epw,
        "Weather changed during snapshot; calculation was not started"
    );
    let validated = validate_thermal_model(&model, &study.calendar)?;
    let mut writer = EnergySeriesWriter::create(
        &scratch.join("series"),
        prepared
            .channels
            .iter()
            .map(|c| EnergyChannel {
                id: c.id.clone(),
                label: c.label.clone(),
                unit: c.unit.clone(),
            })
            .collect(),
        60 / u16::from(study.zone_timesteps_per_hour),
    )?;
    let zone_ids = model
        .zones
        .iter()
        .filter(|z| z.cooling_system.is_some())
        .map(|z| z.id.clone())
        .collect::<Vec<_>>();
    let mut accumulator = EnergyAccumulator::with_interval_seconds(
        &prepared.ledger,
        &zone_ids,
        3600 / u32::from(study.zone_timesteps_per_hour),
    )?;
    let output = greenlens_energyplus::execute(&prepared, &engine, cancel, log, |row, values| {
        ensure!(
            !cancel.load(Ordering::SeqCst),
            "Energy result collection cancelled"
        );
        accumulator.push(row)?;
        writer.push(row.index, values)
    })?;
    ensure!(
        !cancel.load(Ordering::SeqCst),
        "Energy run cancelled before finalization"
    );
    let series = writer.finish()?;
    let totals = accumulator.finish()?;
    let physical_input_hash = study.physical_input_hash(&model)?;
    let execution_input_hash = study.execution_input_hash(&model, &prepared.method)?;
    let physical_result_hash = canonical_hash(&(
        "greenlens.energy-physical-result.v1",
        &execution_input_hash,
        series.identity()?,
    ))?;
    let processing_identity = processing_identity(&physical_result_hash, &prepared.ledger)?;
    let diagnostics: serde_json::Value = serde_json::to_value(&output.diagnostics)?;
    let warning_count = diagnostics["warning_count"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing native warning summary"))?;
    let severe_count = diagnostics["severe_count"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing native severe summary"))?;
    ensure!(
        severe_count == 0 && warning_count <= u32::MAX as u64,
        "Fatal/severe or invalid native diagnostics prevent finalization"
    );
    let messages: Vec<String> = serde_json::from_value(diagnostics["messages"].clone())?;
    let record=EnergyRunRecord{schema_version:energy::RUN_SCHEMA.into(),id:id.into(),alternative_id:alternative,model,study,validated,weather_asset:retained_asset,method:prepared.method,ledger:prepared.ledger,series,totals,
        provenance:EnergyProvenance{application_version:env!("CARGO_PKG_VERSION").into(),source_commit:option_env!("GREENLENS_BUILD_SOURCE_COMMIT").map(String::from),source_worktree_dirty:option_env!("GREENLENS_BUILD_SOURCE_DIRTY").map(|v|v=="true"),started_utc:started,finished_utc:Utc::now().to_rfc3339(),physical_input_hash,execution_input_hash,physical_result_hash,processing_identity,processes:vec![output.process],diagnostics:messages,warning_count:warning_count as u32,severe_count:severe_count as u32,raw_representation:"All requested native CSV numeric columns parsed to IEEE-754 float64 little-endian chunks; ordered labels, dimensions, time policy, original CSV digests and exact argument recipe retained. Input-derived occupancy is identified separately. Large native CSV/octrees are regenerable scratch, not needed for inspection.".into()}};
    energy::validate_record(&record)?;
    write_json(&scratch.join("input.json"), &record.model)?;
    write_json(&scratch.join("study.json"), &record.study)?;
    write_json(&scratch.join("results.json"), &record)?;
    write_json(&scratch.join("provenance.json"), &record.provenance)?;
    let files = energy::evidence_files(&record);
    seal(
        scratch,
        &files.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    energy::read_run_directory(scratch)?;
    ensure!(
        !cancel.load(Ordering::SeqCst),
        "Energy finalization cancelled"
    );
    log("Complete native outputs, units, coincident accounting and compact evidence checked; scientific qualification is separate");
    Ok(record)
}
