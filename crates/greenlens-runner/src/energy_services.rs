//! Shared bounded energy document actions. No frontend calculations or shell.
use crate::{
    energy::*, energy_series::EnergySeriesReader, energy_weather, persistence::*, Project,
    ProjectView,
};
use anyhow::{ensure, Context, Result};
use greenlens_core::{canonical_hash, energy::*};
use std::{collections::BTreeMap, fs, path::Path};
use uuid::Uuid;

pub fn example_document() -> Project {
    let mut project = crate::example();
    project.name = "Synthetic four-zone energy building".into();
    project.schema_version = "0.4.0".into();
    project.energy = Some(EnergyProject {
        schema_version: PROJECT_SCHEMA.into(),
        designs: [("A", false), ("B", true)]
            .into_iter()
            .map(|(id, proposed)| {
                (
                    id.into(),
                    EnergyDesign {
                        model: synthetic_four_zone_model(proposed),
                        study: None,
                    },
                )
            })
            .collect(),
        weather: vec![],
        runs: vec![],
        attachments: vec![],
        carbon_factor: None,
        lighting_service_evidence: None,
    });
    project
}

/// Create an original, explicitly non-climatic portable example. Existing files
/// are never replaced. The original and normalized EPW live in the project.
pub fn create_example(root: &Path) -> Result<ProjectView> {
    ensure!(
        !root.exists() || fs::read_dir(root)?.next().is_none(),
        "Example destination must be empty"
    );
    let saved = crate::save_project(root, &example_document(), None)?;
    let scratch = crate::local_root()?
        .join("energy-imports")
        .join(Uuid::new_v4().to_string());
    fs::create_dir_all(&scratch)?;
    let weather = scratch.join("original.epw");
    fs::write(&weather, weather::synthetic_energy_epw())?;
    let asset = scratch.join("asset");
    energy_weather::import(&weather, &asset, synthetic_source())?;
    attach_weather(
        root,
        &saved.project,
        Some(&saved.revision),
        &asset,
        synthetic_energy_calendar(),
        true,
    )
}

pub fn attach_weather(
    root: &Path,
    project: &Project,
    expected: Option<&str>,
    asset: &Path,
    calendar: EnergyCalendar,
    apply_location: bool,
) -> Result<ProjectView> {
    crate::validate_project(project)?;
    let _lock = WriterLock::acquire(root)?;
    check_revision(root, expected)?;
    let (weather_asset, parsed) = energy_weather::open(asset)?;
    let mut next = project.clone();
    let extension = next
        .energy
        .as_mut()
        .context("Create an energy document before attaching thermal weather")?;
    let reference = if let Some(reference) = extension
        .weather
        .iter()
        .find(|w| w.binding == weather_asset.binding)
    {
        reference.clone()
    } else {
        let reference = import_project_weather(asset, root, &Uuid::new_v4().to_string())?;
        extension.weather.push(reference.clone());
        reference
    };
    for design in extension.designs.values_mut() {
        if apply_location {
            design.model.site = ThermalSite {
                latitude_deg_north: parsed.data.location.latitude_deg_north,
                longitude_deg_east: parsed.data.location.longitude_deg_east,
                elevation_m: parsed.data.location.elevation_m,
            };
        }
        let study = EnergyStudy {
            schema_version: ENERGY_STUDY_SCHEMA.into(),
            weather: reference.binding.clone(),
            calendar: calendar.clone(),
            zone_timesteps_per_hour: 6,
        };
        validate_weather_context(&design.model, &study, &parsed.data)?;
        design.study = Some(study);
    }
    verify_project_evidence(root, extension)?;
    crate::validate_project(&next)?;
    check_revision(root, expected)?;
    atomic_json(root, &next)?;
    crate::open_project(root)
}

pub fn import_weather(
    root: &Path,
    project: &Project,
    expected: Option<&str>,
    file: &Path,
    source: crate::weather::WeatherSource,
    calendar: EnergyCalendar,
    apply_location: bool,
) -> Result<ProjectView> {
    check_revision(root, expected)?;
    let scratch = crate::local_root()?
        .join("energy-imports")
        .join(Uuid::new_v4().to_string());
    fs::create_dir_all(&scratch)?;
    let asset = scratch.join("asset");
    energy_weather::import(file, &asset, source)?;
    attach_weather(root, project, expected, &asset, calendar, apply_location)
}

pub fn edit(
    project: &Project,
    alternative: &str,
    model: ThermalModel,
    study: Option<EnergyStudy>,
) -> Result<Project> {
    let mut next = project.clone();
    let design = next
        .energy
        .as_mut()
        .context("Energy document is unavailable")?
        .designs
        .get_mut(alternative)
        .context("Select energy alternative A or B")?;
    *design = EnergyDesign { model, study };
    crate::validate_project(&next)?;
    Ok(next)
}

pub fn start(
    app: &crate::Application,
    root: &Path,
    project: &Project,
    alternative: &str,
    engine_dir: Option<std::path::PathBuf>,
) -> Result<crate::JobStatus> {
    crate::validate_project(project)?;
    let extension = project
        .energy
        .as_ref()
        .context("Energy document is unavailable")?;
    let design = extension
        .designs
        .get(alternative)
        .context("Select energy alternative A or B")?;
    let study = design
        .study
        .clone()
        .context("Import suitable thermal weather and configure the calendar first")?;
    let weather = extension
        .weather
        .iter()
        .find(|w| w.binding == study.weather)
        .context("Study weather is missing")?;
    regular_child(root, &format!("{}/asset.json", weather.path))?;
    app.start_energy_for_project(
        design.model.clone(),
        study,
        root.join(&weather.path),
        alternative.into(),
        engine_dir,
        project.id.clone(),
    )
}

pub fn selected_run(root: &Path, project: &Project, id: &str) -> Result<EnergyRunRecord> {
    let summary = project
        .energy
        .as_ref()
        .and_then(|e| e.runs.iter().find(|r| r.id == id))
        .context("Selected energy snapshot is not retained in this project")?;
    read_run(root, summary)
}

pub fn rows(
    root: &Path,
    project: &Project,
    id: &str,
    start: usize,
    count: usize,
) -> Result<serde_json::Value> {
    let summary = project
        .energy
        .as_ref()
        .and_then(|e| e.runs.iter().find(|r| r.id == id))
        .context("Selected energy snapshot is not retained")?;
    let run = read_result_metadata(root, summary)?;
    let reader = EnergySeriesReader::open(&root.join("runs").join(&run.id).join("series"))?;
    ensure!(
        reader.manifest() == &run.series,
        "Energy series manifest differs from pinned record"
    );
    let rows = reader.rows(start, count)?;
    let decoder = greenlens_energyplus::Decoder::new(&run.model, &run.study)?;
    for (offset, values) in rows.iter().enumerate() {
        decoder.decode(start + offset, values)?;
    }
    // Only complete common ten-minute demand intervals inside this request.
    // Five-minute reference values are summed before conversion, never peaked independently.
    let factor = usize::from(run.study.zone_timesteps_per_hour / 6);
    let first = start.div_ceil(factor) * factor;
    let end = (start + rows.len()) / factor * factor;
    let mut demand = Vec::new();
    for index in (first..end).step_by(factor) {
        let mut facility = 0.;
        let mut end_uses = BTreeMap::new();
        for offset in 0..factor {
            let row = decoder.decode(index + offset, &rows[index + offset - start])?;
            facility += row.facility_j;
            for (key, value) in row.end_uses_j {
                *end_uses.entry(key).or_insert(0.) += value;
            }
        }
        let components = end_uses
            .into_iter()
            .map(|(key, value)| Ok((key, interval_demand_kw(value, 600)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        demand.push(serde_json::json!({"timestamp":interval_timestamp(index/factor)?,"facility_kw":interval_demand_kw(facility,600)?,"end_uses_kw":components}));
    }
    Ok(
        serde_json::json!({"run_id":run.id,"series":run.series,"start":start,"rows":rows,"demand_10min":demand,"integrity_scope":"Pinned record, compiled recipe and requested chunk checks; full evidence reprocessing occurs on open/export"}),
    )
}

/// An assessment of the exact retained method, independently of integrity and
/// current-input status. No energy method is accepted by passing daylight tests.
pub fn assessment(project: &Project, run: &EnergyRunRecord) -> Result<EnergyAssessment> {
    let relation = if let Some(design) = project
        .energy
        .as_ref()
        .and_then(|e| e.designs.get(&run.alternative_id))
    {
        if let Some(study) = &design.study {
            if study.physical_input_hash(&design.model)? == run.provenance.physical_input_hash {
                EnergyInputRelation::Current
            } else {
                EnergyInputRelation::Changed
            }
        } else {
            EnergyInputRelation::Changed
        }
    } else {
        EnergyInputRelation::NotCompared
    };
    Ok(EnergyAssessment {
        execution: EnergyExecutionStatus::Completed,
        evidence_integrity: EnergyIntegrityScope::CompleteRetainedEvidenceChecked,
        current_physical_inputs: relation,
        current_processed_results: relation,
        current_derived_results: EnergyInputRelation::NotCompared,
        scientific: crate::energy_qualification::assess(
            &run.method,
            &run.provenance.physical_input_hash,
        )?,
    })
}

pub fn report_data(
    root: &Path,
    project: &Project,
    a_id: &str,
    b_id: &str,
) -> Result<EnergyReportData> {
    crate::validate_project(project)?;
    let a = selected_run(root, project, a_id)?;
    let b = selected_run(root, project, b_id)?;
    ensure!(
        a.alternative_id == "A" && b.alternative_id == "B",
        "Select baseline A and proposed B snapshots explicitly"
    );
    ensure!(
        a.method == b.method && a.ledger == b.ledger,
        "Comparison requires matching complete engine/method/numerical and meter identities"
    );
    let input_relation = compare_energy_inputs(&a.model, &a.study, &b.model, &b.study)?;
    ensure!(
        input_relation.comparable_geometry_weather_calendar,
        "Comparison requires matching weather, calendar, zone geometry, area and timestep"
    );
    let extension = project.energy.as_ref().context("Energy project missing")?;
    let comparison = compare_energy_totals(
        &a.totals,
        &b.totals,
        input_relation,
        extension
            .lighting_service_evidence
            .clone()
            .unwrap_or_else(|| {
                "Missing: no independent lighting-service equivalence evidence was supplied.".into()
            }),
    )?;
    let data = EnergyReportData { schema_version: ENERGY_REPORT_SCHEMA.into(), id: format!("report_{}", &canonical_hash(&(a_id,b_id,&extension.carbon_factor,&extension.lighting_service_evidence))?[..16]), title: format!("{} — building-energy assessment", project.name), revision: "1".into(), scope: "Fixed rectangular thermal zones; annual explicit air-cooled single-speed DX cooling with supply fans and separately modeled balanced ventilation/exhaust; local standard time.".into(), baseline: case_from_run(root,project,&a,"Baseline",extension)?, proposed: case_from_run(root,project,&b,"Proposed",extension)?, comparison, assets: vec![], evidence: crate::energy_attachments::report_evidence(extension,&a.validated.model_input_sha256,&b.validated.model_input_sha256), limitations: vec!["No heating, DHW, lifts, gas, PV or central plant is modeled; unsupported systems are not zero-valued measured contributions.".into(), "Gross floor area is the EUI denominator. Peak contributions are simultaneous at each case's own facility peak, which may occur at different times.".into(), "Simulation evidence is not criterion evaluation or certification; daylight qualification is independent and remains unchanged.".into()] };
    data.validate()?;
    Ok(data)
}

pub fn assessments(
    project: &Project,
    runs: &[EnergyRunRecord],
) -> Result<BTreeMap<String, EnergyAssessment>> {
    runs.iter()
        .map(|r| Ok((r.id.clone(), assessment(project, r)?)))
        .collect()
}

pub fn report_case(root: &Path, project: &Project, run_id: &str) -> Result<EnergyReportCase> {
    let run = selected_run(root, project, run_id)?;
    let extension = project.energy.as_ref().context("Energy project missing")?;
    let label = if run.alternative_id == "A" {
        "Baseline"
    } else {
        "Proposed"
    };
    case_from_run(root, project, &run, label, extension)
}
fn case_from_run(
    root: &Path,
    project: &Project,
    run: &EnergyRunRecord,
    label: &str,
    extension: &EnergyProject,
) -> Result<EnergyReportCase> {
    let (_, weather) = energy_weather::open(&root.join("runs").join(&run.id).join("weather"))?;
    Ok(EnergyReportCase {
            id: run.alternative_id.clone(), label: label.into(), model_id: format!("model_{}", run.alternative_id), model: run.model.clone(), study: run.study.clone(),
            weather_summary: EnergyWeatherSummary { binding: run.study.weather.clone(), location: weather.data.location, source: InputSource { citation: run.weather_asset.source.citation.clone(), synthetic: run.weather_asset.source.synthetic_non_climatic }, year_hours: 8760 },
            model_input_sha256: run.validated.model_input_sha256.clone(), run_id: run.id.clone(), record_sha256: canonical_hash(run)?, physical_input_sha256: run.provenance.physical_input_hash.clone(), processing_sha256: run.provenance.processing_identity.clone(), method: run.method.clone(), assessment: assessment(project, run)?, gross_floor_area_m2: run.validated.gross_floor_area_m2, conditioned_floor_area_m2: run.validated.conditioned_floor_area_m2, totals: run.totals.clone(), meter_ledger:run.ledger.clone(), diagnostics:EnergyReportDiagnostics{source_record_sha256:canonical_hash(run)?,messages:run.provenance.diagnostics.clone(),warning_count:Some(run.provenance.warning_count),severe_count:Some(run.provenance.severe_count),convergence_disclosure:"Complete finite native interval output and reconciled accounting; timestep/physics acceptance is separately reported by the exact-method assessment.".into()}, carbon_factor: extension.carbon_factor.clone(), limitations: run.provenance.diagnostics.clone(),
        })
}

/// Produce an atomic, engine-free report directory including only explicitly
/// selected inert attachments. No private attachment caption is exported by default.
pub fn render_reports(
    root: &Path,
    project: &Project,
    a: &str,
    b: &str,
    destination: &Path,
    typst: Option<&crate::energy_report::TypstConfig>,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<crate::energy_report::EnergyReportArtifacts> {
    use std::sync::atomic::Ordering;
    ensure!(
        !destination.exists(),
        "Report destination must be a new directory"
    );
    let _lock = WriterLock::acquire(root)?;
    let mut data = report_data(root, project, a, b)?;
    data.evidence.retain(|e| e.included);
    let stage = crate::local_root()?
        .join("energy-report-deliveries")
        .join(Uuid::new_v4().to_string());
    let mut products = crate::energy_report::render(&data, &stage, typst, cancel)?;
    for item in &project
        .energy
        .as_ref()
        .context("Energy project missing")?
        .attachments
    {
        if !data.evidence.iter().any(|e| e.relative_path == item.path) {
            continue;
        }
        crate::energy_attachments::copy(root, &stage, item)?;
        let directory = format!("attachments/{}", item.id);
        let seal = verify_seal(&stage.join(&directory))?;
        for leaf in seal
            .sha256
            .keys()
            .map(String::as_str)
            .chain(std::iter::once("checksums.json"))
        {
            let name = format!("{directory}/{leaf}");
            let path = regular_child(&stage, &name)?;
            products
                .artifacts
                .push(crate::energy_report::ReportArtifact {
                    path: name,
                    bytes: fs::metadata(&path)?.len(),
                    sha256: greenlens_process::file_hash(&path)?,
                });
        }
    }
    write_json(&stage.join("report-artifacts.json"), &products)?;
    let mut files: Vec<String> = products.artifacts.iter().map(|a| a.path.clone()).collect();
    files.push("report-artifacts.json".into());
    let mut budget = PackageBudget::default();
    for file in &files {
        budget.add(file, fs::metadata(regular_child(&stage, file)?)?.len())?;
    }
    ensure!(!cancel.load(Ordering::SeqCst), "Report export cancelled");
    let parent = destination.parent().context("Report parent missing")?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".greenlens-report-{}", Uuid::new_v4()));
    fs::create_dir(&temporary)?;
    for file in &files {
        ensure!(!cancel.load(Ordering::SeqCst), "Report export cancelled");
        let source = regular_child(&stage, file)?;
        let target = temporary.join(relative_path(file)?);
        fs::create_dir_all(target.parent().context("Report file parent missing")?)?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        copy_bounded(
            &mut fs::File::open(&source)?,
            &mut output,
            MAX_EVIDENCE_FILE_BYTES,
        )?;
        output.sync_all()?;
        ensure!(
            greenlens_process::file_hash(&target)? == greenlens_process::file_hash(&source)?,
            "Report changed during copy"
        );
    }
    ensure!(!cancel.load(Ordering::SeqCst), "Report export cancelled");
    fs::rename(temporary, destination)?;
    Ok(products)
}

/// Carbon and service evidence affect derived reporting, not thermal execution.
pub fn update_report_settings(
    project: &Project,
    carbon_factor: Option<CarbonFactor>,
    lighting_service_evidence: Option<String>,
) -> Result<Project> {
    let mut next = project.clone();
    let energy = next.energy.as_mut().context("Energy project missing")?;
    energy.carbon_factor = carbon_factor;
    energy.lighting_service_evidence = lighting_service_evidence;
    crate::validate_project(&next)?;
    Ok(next)
}

/// Default selection is scientific identity plus UTC completion time, never
/// collection order. Explicit historical selection remains a separate UI state.
pub fn default_run_ids(root: &Path, project: &Project) -> Result<BTreeMap<String, String>> {
    let extension = project.energy.as_ref().context("Energy project missing")?;
    let records = extension
        .runs
        .iter()
        .map(|s| read_result_metadata(root, s))
        .collect::<Result<Vec<_>>>()?;
    let mut selected = BTreeMap::new();
    for (id, design) in &extension.designs {
        let run = current_result(design, &records, id)?.or_else(|| {
            records
                .iter()
                .filter(|r| &r.alternative_id == id)
                .max_by_key(|r| {
                    chrono::DateTime::parse_from_rfc3339(&r.provenance.finished_utc).ok()
                })
        });
        if let Some(run) = run {
            selected.insert(id.clone(), run.id.clone());
        }
    }
    Ok(selected)
}
