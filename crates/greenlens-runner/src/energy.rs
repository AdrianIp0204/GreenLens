//! Energy project extension and immutable compact run evidence. Inspection needs
//! no simulation engine; replay dispatch belongs to the trusted compiled adapter.
use crate::{
    energy_series::{EnergySeriesManifest, EnergySeriesReader},
    energy_weather::{self, EnergyWeatherAsset},
    persistence::*,
    weather::WeatherSource,
};
use anyhow::{ensure, Result};
use greenlens_core::{
    canonical_hash,
    energy::{
        validate_thermal_model, CarbonFactor, EnergyMethodIdentity, EnergyStudy, EnergyTotals,
        EnergyWeatherBinding, MeterLedger, ThermalModel, ValidatedThermalModel,
    },
};
use greenlens_process::ProcessOutcome;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use uuid::Uuid;
pub const PROJECT_SCHEMA: &str = "greenlens.energy-project.v0.4";
pub const RUN_SCHEMA: &str = "greenlens.energy-run.v0.4";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnergyDesign {
    pub model: ThermalModel,
    pub study: Option<EnergyStudy>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyWeatherReference {
    pub id: String,
    pub path: String,
    pub binding: EnergyWeatherBinding,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyRunSummary {
    pub id: String,
    pub alternative_id: String,
    pub path: String,
    pub physical_input_hash: String,
    pub record_sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnergyProject {
    pub schema_version: String,
    pub designs: BTreeMap<String, EnergyDesign>,
    pub weather: Vec<EnergyWeatherReference>,
    pub runs: Vec<EnergyRunSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<crate::energy_attachments::EnergyAttachment>,
    pub carbon_factor: Option<CarbonFactor>,
    /// An explicit evidence declaration; reduced LPD never implies equivalence.
    pub lighting_service_evidence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workbench: Option<crate::energy_workbench::EnergyWorkbench>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnergyProvenance {
    pub application_version: String,
    pub source_commit: Option<String>,
    pub source_worktree_dirty: Option<bool>,
    pub started_utc: String,
    pub finished_utc: String,
    pub physical_input_hash: String,
    pub execution_input_hash: String,
    pub physical_result_hash: String,
    pub processing_identity: String,
    pub processes: Vec<ProcessOutcome>,
    pub diagnostics: Vec<String>,
    pub warning_count: u32,
    pub severe_count: u32,
    pub raw_representation: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnergyRunRecord {
    pub schema_version: String,
    pub id: String,
    pub alternative_id: String,
    pub model: ThermalModel,
    pub study: EnergyStudy,
    pub validated: ValidatedThermalModel,
    pub weather_asset: EnergyWeatherAsset,
    pub method: EnergyMethodIdentity,
    pub ledger: MeterLedger,
    pub series: EnergySeriesManifest,
    pub totals: EnergyTotals,
    pub provenance: EnergyProvenance,
}
fn uuid(id: &str) -> Result<()> {
    ensure!(
        Uuid::parse_str(id)?.to_string() == id,
        "Energy identifiers require canonical lowercase UUIDs"
    );
    Ok(())
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn validate_extension(project: &EnergyProject) -> Result<()> {
    crate::energy_workbench::validate_state(project)?;
    ensure!(
        project.schema_version == PROJECT_SCHEMA,
        "Unsupported thermal project extension"
    );
    ensure!(
        project.designs.keys().map(String::as_str).eq(["A", "B"]),
        "Energy designs must contain exactly A and B"
    );
    let mut ids = BTreeSet::new();
    for item in &project.weather {
        uuid(&item.id)?;
        ensure!(
            ids.insert(&item.id) && item.path == format!("weather/{}", item.id),
            "Invalid/duplicate energy weather reference"
        );
        item.binding.validate()?;
    }
    for design in project.designs.values() {
        if let Some(study) = &design.study {
            study.validate(&design.model)?;
            ensure!(
                project.weather.iter().any(|w| w.binding == study.weather),
                "Energy study weather missing from portable assets"
            );
        } else {
            // Geometry and assignments are editable before weather import.
            let calendar = greenlens_core::energy::EnergyCalendar {
                start_weekday: greenlens_core::energy::Weekday::Monday,
                holidays: design
                    .model
                    .schedules
                    .iter()
                    .flat_map(|s| s.holidays.iter().map(|h| h.date))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                utc_offset_minutes: 0,
            };
            validate_thermal_model(&design.model, &calendar)?;
        }
    }
    if let Some(factor) = &project.carbon_factor {
        factor.validate()?;
    }
    if let Some(evidence) = &project.lighting_service_evidence {
        ensure!(
            !evidence.trim().is_empty() && evidence.len() <= 16000,
            "Lighting-service evidence must be bounded nonempty text"
        );
    }
    for item in &project.attachments {
        item.validate()?;
        ensure!(ids.insert(&item.id), "Duplicate energy attachment identity");
    }
    for run in &project.runs {
        uuid(&run.id)?;
        ensure!(
            ids.insert(&run.id)
                && ["A", "B"].contains(&run.alternative_id.as_str())
                && run.path == format!("runs/{}", run.id)
                && hash(&run.physical_input_hash)
                && hash(&run.record_sha256),
            "Invalid energy run reference"
        );
    }
    Ok(())
}
pub fn validate_record(run: &EnergyRunRecord) -> Result<()> {
    ensure!(
        run.schema_version == RUN_SCHEMA,
        "Unsupported energy run version"
    );
    uuid(&run.id)?;
    ensure!(
        ["A", "B"].contains(&run.alternative_id.as_str()),
        "Invalid energy alternative"
    );
    run.study.validate(&run.model)?;
    run.method.validate()?;
    run.ledger.validate()?;
    run.series.validate()?;
    ensure!(
        canonical_hash(&validate_thermal_model(&run.model, &run.study.calendar)?)?
            == canonical_hash(&run.validated)?,
        "Retained thermal validation differs from authoritative inputs"
    );
    run.weather_asset.source.validate()?;
    ensure!(
        run.weather_asset.schema_version == energy_weather::ASSET_SCHEMA
            && run.weather_asset.binding == run.study.weather,
        "Energy run weather identity mismatch"
    );
    ensure!(
        run.series.interval_minutes as usize == 60 / run.study.zone_timesteps_per_hour as usize
            && run.series.interval_count == 8760 * run.study.zone_timesteps_per_hour as usize,
        "Energy result interval shape differs from study"
    );
    let p = &run.provenance;
    ensure!(
        p.severe_count == 0,
        "Severe energy errors prevent completed evidence"
    );
    ensure!(
        p.physical_input_hash == run.study.physical_input_hash(&run.model)?
            && p.execution_input_hash == run.study.execution_input_hash(&run.model, &run.method)?,
        "Energy input identity mismatch"
    );
    ensure!(
        p.physical_result_hash
            == canonical_hash(&(
                "greenlens.energy-physical-result.v1",
                &p.execution_input_hash,
                run.series.identity()?
            ))?,
        "Energy result identity mismatch"
    );
    ensure!(
        p.processing_identity
            == greenlens_core::energy::processing_identity(&p.physical_result_hash, &run.ledger)?,
        "Energy processing identity mismatch"
    );
    ensure!(
        !p.application_version.is_empty()
            && p.application_version.len() <= 120
            && !p.raw_representation.is_empty(),
        "Missing energy application/raw provenance"
    );
    let started = chrono::DateTime::parse_from_rfc3339(&p.started_utc)?;
    let finished = chrono::DateTime::parse_from_rfc3339(&p.finished_utc)?;
    ensure!(
        started.offset().local_minus_utc() == 0
            && finished.offset().local_minus_utc() == 0
            && finished >= started,
        "Invalid energy UTC provenance"
    );
    ensure!(
        !p.processes.is_empty() && p.processes.len() <= 16,
        "Missing/excess energy process receipts"
    );
    for process in &p.processes {
        ensure!(
            process.schema_version == "greenlens.native-process.v0.4"
                && process.state == "completed"
                && process.exit_code == Some(0)
                && process.process_tree_empty
                && process.failure.is_none()
                && hash(&process.executable_sha256)
                && hash(&process.stdout_sha256)
                && hash(&process.stderr_sha256),
            "Energy evidence requires complete successful process trees"
        );
    }
    ensure!(
        run.totals.interval_count == run.series.interval_count
            && run.totals.interval_seconds == u32::from(run.series.interval_minutes) * 60,
        "Energy totals/time-series shape mismatch"
    );
    Ok(())
}
pub fn evidence_files(run: &EnergyRunRecord) -> Vec<String> {
    let mut files = [
        "input.json",
        "study.json",
        "results.json",
        "provenance.json",
        "model.epJSON",
        "recipe.json",
        "diagnostics.json",
        "series/series.json",
        "weather/asset.json",
        "weather/weather.json",
        "weather/original.epw",
        "weather/normalized.epw",
        "weather/checksums.json",
        "engine/process.json",
        "engine/process-stdout.txt",
        "engine/process-stderr.txt",
        "engine/eplusout.err",
        "engine/eplusout.end",
    ]
    .map(String::from)
    .to_vec();
    files.extend(
        run.series
            .chunks
            .iter()
            .map(|c| format!("series/{}", c.file)),
    );
    files
}
pub fn read_run_directory(root: &Path) -> Result<EnergyRunRecord> {
    let checksums = verify_seal(root)?;
    let run: EnergyRunRecord = read_json(&regular_child(root, "results.json")?)?;
    validate_record(&run)?;
    ensure!(
        checksums.sha256.keys().cloned().collect::<BTreeSet<_>>()
            == evidence_files(&run).into_iter().collect(),
        "Energy evidence coverage differs from required compact files"
    );
    ensure!(
        canonical_hash(&run.model)?
            == canonical_hash(&read_json::<ThermalModel>(&regular_child(
                root,
                "input.json"
            )?)?)?
            && canonical_hash(&run.study)?
                == canonical_hash(&read_json::<EnergyStudy>(&regular_child(
                    root,
                    "study.json"
                )?)?)?,
        "Energy input/study snapshots disagree"
    );
    ensure!(
        canonical_hash(&run.provenance)?
            == canonical_hash(&read_json::<EnergyProvenance>(&regular_child(
                root,
                "provenance.json"
            )?)?)?,
        "Energy provenance snapshot mismatch"
    );
    let (asset, _) = energy_weather::open(&root.join("weather"))?;
    ensure!(
        asset == run.weather_asset,
        "Retained energy weather differs from run"
    );
    let series = EnergySeriesReader::open(&root.join("series"))?;
    ensure!(
        series.manifest() == &run.series,
        "Energy series manifest differs from run"
    );
    let channels = run
        .series
        .channels
        .iter()
        .map(|c| greenlens_energyplus::Channel {
            id: c.id.clone(),
            label: c.label.clone(),
            unit: c.unit.clone(),
        })
        .collect::<Vec<_>>();
    greenlens_energyplus::validate_retained_contract(
        &run.model,
        &run.study,
        &run.method,
        &run.ledger,
        &channels,
        &std::fs::read(regular_child(root, "model.epJSON")?)?,
        &std::fs::read(regular_child(root, "recipe.json")?)?,
    )?;
    let decoder = greenlens_energyplus::Decoder::new(&run.model, &run.study)?;
    let zones = run
        .model
        .zones
        .iter()
        .filter(|z| z.cooling_system.is_some())
        .map(|z| z.id.clone())
        .collect::<Vec<_>>();
    let mut totals = greenlens_core::energy::EnergyAccumulator::with_interval_seconds(
        &run.ledger,
        &zones,
        3600 / u32::from(run.study.zone_timesteps_per_hour),
    )?;
    crate::project_open::numeric_pass(&run.id)?;
    series.visit(|index, values| {
        totals.push(&decoder.decode(index, values)?)?;
        Ok(())
    })?;
    ensure!(
        canonical_hash(&totals.finish()?)? == canonical_hash(&run.totals)?,
        "Retained energy totals/comfort differ from independently reprocessed native channels"
    );
    let receipt: ProcessOutcome = read_json(&regular_child(root, "engine/process.json")?)?;
    ensure!(
        run.provenance.processes.len() == 1
            && canonical_hash(&receipt)? == canonical_hash(&run.provenance.processes[0])?,
        "Energy process receipt differs from provenance"
    );
    ensure!(
        receipt.executable_sha256 == run.method.engine_executable_sha256
            && greenlens_process::file_hash(&regular_child(root, "engine/process-stdout.txt")?)?
                == receipt.stdout_sha256
            && greenlens_process::file_hash(&regular_child(root, "engine/process-stderr.txt")?)?
                == receipt.stderr_sha256,
        "Energy executable or stream provenance mismatch"
    );
    let diagnostics: serde_json::Value = read_json(&regular_child(root, "diagnostics.json")?)?;
    ensure!(
        diagnostics["warning_count"].as_u64() == Some(u64::from(run.provenance.warning_count))
            && diagnostics["severe_count"].as_u64() == Some(u64::from(run.provenance.severe_count))
            && diagnostics["messages"] == serde_json::to_value(&run.provenance.diagnostics)?,
        "Retained native diagnostics mismatch"
    );
    ensure!(
        bytes_hash(&std::fs::read(regular_child(root, "recipe.json")?)?)
            == run.method.recipe_sha256,
        "Energy recipe identity mismatch"
    );
    Ok(run)
}
pub fn read_run(root: &Path, summary: &EnergyRunSummary) -> Result<EnergyRunRecord> {
    uuid(&summary.id)?;
    ensure!(
        summary.path == format!("runs/{}", summary.id),
        "Invalid energy run path"
    );
    let record = read_run_directory(&root.join(&summary.path))?;
    validate_summary_binding(&record, summary)?;
    Ok(record)
}
pub(crate) fn validate_summary_binding(
    record: &EnergyRunRecord,
    summary: &EnergyRunSummary,
) -> Result<()> {
    ensure!(
        record.id == summary.id
            && record.alternative_id == summary.alternative_id
            && record.provenance.physical_input_hash == summary.physical_input_hash
            && canonical_hash(record)? == summary.record_sha256,
        "Energy manifest/result binding mismatch"
    );
    Ok(())
}
pub fn load_project_evidence(root: &Path, project: &EnergyProject) -> Result<Vec<EnergyRunRecord>> {
    verify_project_assets(root, project)?;
    project.runs.iter().map(|run| read_run(root, run)).collect()
}
pub(crate) fn verify_project_assets(root: &Path, project: &EnergyProject) -> Result<()> {
    validate_extension(project)?;
    for weather in &project.weather {
        let (asset, _) = energy_weather::open(&root.join(&weather.path))?;
        ensure!(
            asset.binding == weather.binding,
            "Energy project weather binding mismatch"
        );
    }
    for item in &project.attachments {
        crate::energy_attachments::verify(root, item)?;
    }
    Ok(())
}
pub fn verify_project_evidence(root: &Path, project: &EnergyProject) -> Result<()> {
    load_project_evidence(root, project)?;
    Ok(())
}
/// Document writes may reuse only Rust-validated records whose exact retained
/// bytes still match. Explicit open/export readers keep their full validation
/// policy; save_project also performs its final full open after publication.
pub(crate) fn verify_project_evidence_for_save(root: &Path, project: &EnergyProject) -> Result<()> {
    verify_project_assets(root, project)?;
    for summary in &project.runs {
        crate::energy_verified_cache::read_run(root, summary)?;
    }
    Ok(())
}
pub fn copy_project_evidence(
    source: &Path,
    destination: &Path,
    project: &EnergyProject,
) -> Result<()> {
    verify_project_assets(source, project)?;
    for weather in &project.weather {
        promote_weather(&source.join(&weather.path), destination, &weather.id)?;
    }
    for item in &project.attachments {
        crate::energy_attachments::copy(source, destination, item)?;
    }
    for run in &project.runs {
        crate::energy_verified_cache::copy_run(source, destination, run)?;
    }
    verify_project_assets(destination, project)
}
pub fn append_run(project: &mut EnergyProject, run: &EnergyRunRecord) -> Result<()> {
    validate_record(run)?;
    let summary = EnergyRunSummary {
        id: run.id.clone(),
        alternative_id: run.alternative_id.clone(),
        path: format!("runs/{}", run.id),
        physical_input_hash: run.provenance.physical_input_hash.clone(),
        record_sha256: canonical_hash(run)?,
    };
    if let Some(old) = project.runs.iter().find(|r| r.id == run.id) {
        ensure!(old == &summary, "Energy run identity conflict");
    } else {
        project.runs.push(summary);
    }
    Ok(())
}
pub fn import_project_weather(
    asset_root: &Path,
    project_root: &Path,
    id: &str,
) -> Result<EnergyWeatherReference> {
    uuid(id)?;
    let (asset, _) = energy_weather::open(asset_root)?;
    promote_weather(asset_root, project_root, id)?;
    Ok(EnergyWeatherReference {
        id: id.into(),
        path: format!("weather/{id}"),
        binding: asset.binding,
    })
}
pub fn current_result<'a>(
    design: &EnergyDesign,
    runs: &'a [EnergyRunRecord],
    alternative: &str,
) -> Result<Option<&'a EnergyRunRecord>> {
    let Some(study) = &design.study else {
        return Ok(None);
    };
    let physical = study.physical_input_hash(&design.model)?;
    let method = greenlens_energyplus::method_identity(study)?;
    Ok(runs
        .iter()
        .filter(|r| {
            r.alternative_id == alternative
                && r.provenance.physical_input_hash == physical
                && r.method == method
        })
        .max_by_key(|r| chrono::DateTime::parse_from_rfc3339(&r.provenance.finished_utc).ok()))
}
pub fn synthetic_source() -> WeatherSource {
    WeatherSource {
        citation: "GreenLens original non-climatic thermal input".into(),
        source_url: None,
        license: "Apache-2.0".into(),
        rights_note: "Original synthetic fixture; no measured or typical-climate claim".into(),
        synthetic_non_climatic: true,
    }
}

/// Inventory validation only; actual native execution is separately evidenced.
pub fn doctor(engine_dir: Option<&Path>) -> Result<serde_json::Value> {
    let config = if let Some(path) = engine_dir {
        greenlens_energyplus::EngineConfig::from_directory(path)
    } else {
        greenlens_energyplus::EngineConfig::discover()?
    };
    config.verify()?;
    Ok(
        serde_json::json!({"engine_version":greenlens_energyplus::ENGINE_VERSION,"method":greenlens_energyplus::METHOD_ID,"status":"Pinned executable/schema/runtime hashes match; execution and qualification are separate checks"}),
    )
}

/// Bounded navigation checks exact record identity and the selected chunks.
/// Opening/exporting a project still verifies and reprocesses all retained data.
pub fn read_result_metadata(root: &Path, summary: &EnergyRunSummary) -> Result<EnergyRunRecord> {
    uuid(&summary.id)?;
    ensure!(
        summary.path == format!("runs/{}", summary.id),
        "Invalid energy run path"
    );
    let run: EnergyRunRecord = read_json(&regular_child(
        root,
        &format!("{}/results.json", summary.path),
    )?)?;
    validate_record(&run)?;
    ensure!(
        run.id == summary.id
            && run.alternative_id == summary.alternative_id
            && run.provenance.physical_input_hash == summary.physical_input_hash
            && canonical_hash(&run)? == summary.record_sha256,
        "Energy metadata differs from retained document binding"
    );
    let directory = root.join(&summary.path);
    let channels = run
        .series
        .channels
        .iter()
        .map(|c| greenlens_energyplus::Channel {
            id: c.id.clone(),
            label: c.label.clone(),
            unit: c.unit.clone(),
        })
        .collect::<Vec<_>>();
    greenlens_energyplus::validate_retained_contract(
        &run.model,
        &run.study,
        &run.method,
        &run.ledger,
        &channels,
        &std::fs::read(regular_child(&directory, "model.epJSON")?)?,
        &std::fs::read(regular_child(&directory, "recipe.json")?)?,
    )?;
    Ok(run)
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    // Deliberately incomplete records exercise selection only. They cannot pass
    // validate_record or count as engine/integrity acceptance.
    fn selection_record(
        case: &greenlens_core::energy::EnergyReportCase,
        finished: &str,
    ) -> EnergyRunRecord {
        EnergyRunRecord {
            schema_version: RUN_SCHEMA.into(),
            id: Uuid::new_v4().to_string(),
            alternative_id: "A".into(),
            model: case.model.clone(),
            study: case.study.clone(),
            validated: validate_thermal_model(&case.model, &case.study.calendar).unwrap(),
            weather_asset: EnergyWeatherAsset {
                schema_version: energy_weather::ASSET_SCHEMA.into(),
                binding: case.study.weather.clone(),
                source: synthetic_source(),
            },
            method: greenlens_energyplus::method_identity(&case.study).unwrap(),
            ledger: case.meter_ledger.clone(),
            series: EnergySeriesManifest {
                schema_version: crate::energy_series::SCHEMA.into(),
                encoding: "float64_little_endian".into(),
                ordering: "time_channel".into(),
                time_policy: "non_leap_local_standard_time_interval_end_excludes_warmup_sizing"
                    .into(),
                interval_minutes: 10,
                interval_count: 52560,
                channels: vec![],
                chunks: vec![],
            },
            totals: case.totals.clone(),
            provenance: EnergyProvenance {
                application_version: "synthetic selection fixture only".into(),
                source_commit: None,
                source_worktree_dirty: None,
                started_utc: "2026-01-01T00:00:00Z".into(),
                finished_utc: finished.into(),
                physical_input_hash: case.physical_input_sha256.clone(),
                execution_input_hash: "0".repeat(64),
                physical_result_hash: "0".repeat(64),
                processing_identity: "0".repeat(64),
                processes: vec![],
                diagnostics: vec![],
                warning_count: 0,
                severe_count: 0,
                raw_representation: "No output; selection test only".into(),
            },
        }
    }
    #[test]
    fn verified_provider_cannot_reuse_another_summary_or_modified_record() {
        // Identity-binding test only: these intentionally incomplete records
        // are never admitted as scientific or process evidence.
        let case = greenlens_core::energy::synthetic_energy_report()
            .unwrap()
            .baseline;
        let run = selection_record(&case, "2026-01-03T00:00:00Z");
        let mut summary = EnergyRunSummary {
            id: run.id.clone(),
            alternative_id: run.alternative_id.clone(),
            path: format!("runs/{}", run.id),
            physical_input_hash: run.provenance.physical_input_hash.clone(),
            record_sha256: canonical_hash(&run).unwrap(),
        };
        validate_summary_binding(&run, &summary).unwrap();
        summary.record_sha256 = "f".repeat(64);
        assert!(validate_summary_binding(&run, &summary).is_err());
        summary.record_sha256 = canonical_hash(&run).unwrap();
        summary.alternative_id = "B".into();
        assert!(validate_summary_binding(&run, &summary).is_err());
    }
    #[test]
    fn restored_energy_inputs_find_newest_compatible_completion_independent_of_collection_order() {
        let case = greenlens_core::energy::synthetic_energy_report()
            .unwrap()
            .baseline;
        let mut design = EnergyDesign {
            model: case.model.clone(),
            study: Some(case.study.clone()),
        };
        let newer = selection_record(&case, "2026-01-03T00:00:00Z");
        let expected = newer.id.clone();
        let older = selection_record(&case, "2026-01-02T00:00:00Z");
        let mut wrong = selection_record(&case, "2026-01-05T00:00:00Z");
        wrong.method.recipe_sha256 = "f".repeat(64);
        let records = vec![newer, wrong, older];
        let before = serde_json::to_vec(&records).unwrap();
        assert_eq!(
            current_result(&design, &records, "A").unwrap().unwrap().id,
            expected
        );
        design.model.zones[0].gains.lighting_w_per_floor_m2 += 1.;
        assert!(current_result(&design, &records, "A").unwrap().is_none());
        design.model = case.model;
        assert_eq!(
            current_result(&design, &records, "A").unwrap().unwrap().id,
            expected
        );
        assert_eq!(before, serde_json::to_vec(&records).unwrap());
    }
}
