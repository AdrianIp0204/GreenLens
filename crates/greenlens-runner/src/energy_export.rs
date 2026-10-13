//! Selected energy evidence; imported commands and assessments are never trusted.
use crate::{
    energy,
    energy_report::{self, TypstConfig},
    energy_series::EnergySeriesReader,
    energy_services,
    persistence::*,
};
use anyhow::{ensure, Context, Result};
use greenlens_core::canonical_hash;
use serde_json::{json, Value};
use std::{
    fs,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;

fn members(
    root: &Path,
    prefix: &str,
    files: &mut Vec<String>,
    budget: &mut PackageBudget,
) -> Result<()> {
    let sealed = verify_seal(&root.join(prefix))?;
    for name in sealed
        .sha256
        .keys()
        .map(String::as_str)
        .chain(std::iter::once("checksums.json"))
    {
        let name = format!("{prefix}/{name}");
        budget.add(&name, fs::metadata(regular_child(root, &name)?)?.len())?;
        files.push(name);
    }
    Ok(())
}

pub fn export_pair(
    root: &Path,
    a: &str,
    b: &str,
    destination: &Path,
    typst: &TypstConfig,
    cancel: &AtomicBool,
) -> Result<Value> {
    ensure!(
        !destination.exists(),
        "Export destination exists; choose a new file"
    );
    let _lock = WriterLock::acquire(root)?;
    let view = crate::open_project(root)?;
    let mut dataset = energy_services::report_data(root, &view.project, a, b)?;
    // Excluded evidence metadata can itself be private; omit it from public products.
    dataset.evidence.retain(|e| e.included);
    let mut project = view.project;
    // Selection deliberately bounds comparison to one explicit pair. The saved
    // document retains its current inputs; reports use the immutable snapshots.
    project.runs.clear();
    project.climate = None;
    let extension = project.energy.as_mut().context("Energy project missing")?;
    extension.runs.retain(|r| r.id == a || r.id == b);
    extension.attachments.retain(|item| {
        item.include_in_export
            && dataset
                .evidence
                .iter()
                .any(|e| e.relative_path == item.path)
    });
    ensure!(
        extension.runs.len() == 2,
        "Select two distinct retained baseline/proposed runs"
    );
    let mut files = vec!["project.json".into()];
    let mut budget = PackageBudget::default();
    budget.add(
        "project.json",
        serde_json::to_vec_pretty(&project)?.len() as u64 + 1,
    )?;
    let extension = project.energy.as_ref().unwrap();
    for weather in &extension.weather {
        members(root, &weather.path, &mut files, &mut budget)?;
    }
    for run in &extension.runs {
        members(root, &run.path, &mut files, &mut budget)?;
    }
    for item in &extension.attachments {
        members(
            root,
            &format!("attachments/{}", item.id),
            &mut files,
            &mut budget,
        )?;
    }
    ensure!(!cancel.load(Ordering::SeqCst), "Export cancelled");
    let stage = crate::local_root()?
        .join("energy-exports")
        .join(Uuid::new_v4().to_string());
    fs::create_dir_all(&stage)?;
    energy::copy_project_evidence(root, &stage, extension)?;
    write_json(&stage.join("project.json"), &project)?;
    let reports = energy_report::render(&dataset, &stage.join("reports"), Some(typst), cancel)?;
    ensure!(
        reports.renderer_status == "completed",
        "Required PDF/PNG report generation is incomplete"
    );
    for artifact in &reports.artifacts {
        let name = format!("reports/{}", artifact.path);
        let path = regular_child(&stage, &name)?;
        ensure!(
            greenlens_process::file_hash(&path)? == artifact.sha256,
            "Report changed before export"
        );
        budget.add(&name, fs::metadata(path)?.len())?;
        files.push(name);
    }
    let report_receipt = "reports/report-artifacts.json".to_string();
    if !files.contains(&report_receipt) {
        files.push(report_receipt);
    }
    write_json(
        &stage.join("energy-export.json"),
        &json!({"schema_version":"greenlens.energy-export.v0.4","baseline_run_id":a,"proposed_run_id":b,"report_dataset_sha256":dataset.dataset_sha256()?,"assessment_authority":"Supplementary report only; the compiled application independently recomputes integrity and qualification without altering run evidence."}),
    )?;
    fs::write(stage.join("REPLAY.txt"),"GreenLens energy evidence\nInspect/regenerate reports without EnergyPlus or the original weather/cache. Original/normalized weather, immutable inputs and all retained native channels are included.\nPDF regeneration requires the pinned separately installed Typst/font package.\nInstall pinned EnergyPlus 26.2.0 separately for replay: greenlens energy replay evidence.zip --engine-dir <compatible-engine-directory>\nReplay regenerates trusted epJSON from authoritative inputs using the compiled recognized adapter; no imported executable recipe or report is executed.\nEvery channel is checked within max(1e-6 in its unit, 0.1% of reference), with exact shape, channel labels and interval-end local-standard timestamps.\nAgreement is integration verification, not general physical validation or certification.\n")?;
    files.extend(["energy-export.json".into(), "REPLAY.txt".into()]);
    seal(
        &stage,
        &files.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    files.push("checksums.json".into());
    // The final shared writer preflight includes generated reports and seals;
    // nothing is published until every required product fits its own reader.
    ensure!(!cancel.load(Ordering::SeqCst), "Export cancelled");
    zip_directory(&stage, &files, destination)?;
    Ok(
        json!({"schema_version":"greenlens.energy-export-result.v0.4","entries":files.len(),"package_bytes":fs::metadata(destination)?.len(),"package_sha256":greenlens_process::file_hash(destination)?,"report_dataset_sha256":dataset.dataset_sha256()?,"baseline_run_id":a,"proposed_run_id":b}),
    )
}

/// Compare retained native channels in bounded windows, including temperature,
/// setpoint and occupancy channels. Shapes/labels/time axes must match exactly.
pub fn compare_series(
    reference: &EnergySeriesReader,
    candidate: &EnergySeriesReader,
    cancel: &AtomicBool,
) -> Result<Value> {
    let a = reference.manifest();
    let b = candidate.manifest();
    ensure!(
        a.interval_count == b.interval_count
            && a.interval_minutes == b.interval_minutes
            && a.channels == b.channels
            && a.ordering == b.ordering
            && a.time_policy == b.time_policy,
        "Replay dimensions, units, ordering or time axis differ"
    );
    let mut checked = 0usize;
    let mut maximum = 0.0_f64;
    let mut max_ratio = 0.0_f64;
    for start in (0..a.interval_count).step_by(1008) {
        ensure!(
            !cancel.load(Ordering::SeqCst),
            "Replay comparison cancelled"
        );
        let count = 1008.min(a.interval_count - start);
        let x = reference.rows(start, count)?;
        let y = candidate.rows(start, count)?;
        for (offset, (x, y)) in x.iter().zip(&y).enumerate() {
            for (channel, (&r, &v)) in x.iter().zip(y).enumerate() {
                let error = (r - v).abs();
                let allowance = 1e-6_f64.max(r.abs() * 0.001);
                ensure!(error<=allowance,"Replay differs at interval {}, channel {}: reference {}, actual {}, allowance {} {}",start+offset,a.channels[channel].id,r,v,allowance,a.channels[channel].unit);
                maximum = maximum.max(error);
                max_ratio = max_ratio.max(error / allowance);
                checked += 1;
            }
        }
    }
    Ok(
        json!({"intervals":a.interval_count,"channels":a.channels.len(),"checked_values":checked,"maximum_absolute_difference_across_channel_units":maximum,"maximum_error_allowance_ratio":max_ratio,"tolerance":"max(1e-6 in each channel unit, 0.1% of reference)","scope":"Identical-recipe integration agreement; not physical validation"}),
    )
}

pub fn replay(package: &Path, engine_dir: Option<&Path>, cancel: &AtomicBool) -> Result<Value> {
    let stage = crate::local_root()?
        .join("energy-replay")
        .join(Uuid::new_v4().to_string());
    let imported = stage.join("imported");
    extract_package(package, &imported)?;
    let view = crate::open_project(&imported)?;
    let selection: Value = read_json(&regular_child(&imported, "energy-export.json")?)?;
    ensure!(
        selection["schema_version"] == "greenlens.energy-export.v0.4",
        "Unsupported energy package"
    );
    let a = selection["baseline_run_id"]
        .as_str()
        .context("Missing baseline run")?;
    let b = selection["proposed_run_id"]
        .as_str()
        .context("Missing proposed run")?;
    let data = energy_services::report_data(&imported, &view.project, a, b)?;
    let retained_data: greenlens_core::energy::EnergyReportData =
        read_json(&regular_child(&imported, "reports/energy-report.json")?)?;
    validate_report_binding(
        &retained_data,
        &data,
        selection["report_dataset_sha256"]
            .as_str()
            .context("Missing report hash")?,
    )?;
    let mut results = vec![];
    for id in [a, b] {
        ensure!(!cancel.load(Ordering::SeqCst), "Replay cancelled");
        let run = energy_services::selected_run(&imported, &view.project, id)?;
        let output = stage.join(format!("replay-{id}"));
        fs::create_dir(&output)?;
        let repeated = crate::energy_job::run_expected(
            &Uuid::new_v4().to_string(),
            run.model.clone(),
            run.alternative_id.clone(),
            engine_dir,
            run.study.clone(),
            &imported.join("runs").join(id).join("weather"),
            &output,
            cancel,
            &|_| {},
            Some(&run.method),
        )?;
        ensure!(
            run.method == repeated.method,
            "Recognized replay method differs"
        );
        let reference = EnergySeriesReader::open(&imported.join("runs").join(id).join("series"))?;
        let candidate = EnergySeriesReader::open(&output.join("series"))?;
        results.push(json!({"run_id":id,"check":compare_series(&reference,&candidate,cancel)?,"source_record_sha256":canonical_hash(&run)?,"replay_record_sha256":canonical_hash(&repeated)?}));
    }
    let result = json!({"schema_version":"greenlens.energy-replay.v0.4","package_sha256":greenlens_process::file_hash(package)?,"runs":results,"state":"completed"});
    write_json(&stage.join("replay.json"), &result)?;
    Ok(result)
}

// Qualification and currentness are supplementary interpretations, not part of
// immutable run bytes. A later registry message must not invalidate old replay.
fn validate_report_binding(
    retained: &greenlens_core::energy::EnergyReportData,
    current: &greenlens_core::energy::EnergyReportData,
    expected_hash: &str,
) -> Result<()> {
    ensure!(
        retained.dataset_sha256()? == expected_hash,
        "Retained report dataset checksum mismatch"
    );
    current.validate()?;
    for (a, b) in [
        (&retained.baseline, &current.baseline),
        (&retained.proposed, &current.proposed),
    ] {
        let physical = |c: &greenlens_core::energy::EnergyReportCase| {
            json!({
                "id":c.id,"model_id":c.model_id,"model":c.model,"study":c.study,"weather":c.weather_summary,
                "model_hash":c.model_input_sha256,"run_id":c.run_id,"record_hash":c.record_sha256,
                "physical_hash":c.physical_input_sha256,"processing_hash":c.processing_sha256,"method":c.method,
                "gross_area":c.gross_floor_area_m2,"conditioned_area":c.conditioned_floor_area_m2,
                "totals":c.totals,"meters":c.meter_ledger,"diagnostic_record":c.diagnostics.source_record_sha256,
                "diagnostic_messages":c.diagnostics.messages,"warnings":c.diagnostics.warning_count,
                "severe":c.diagnostics.severe_count,"carbon_factor":c.carbon_factor
            })
        };
        ensure!(
            canonical_hash(&physical(a))? == canonical_hash(&physical(b))?,
            "Exported report inputs/results differ from immutable run evidence"
        );
    }
    ensure!(
        retained.comparison.rows == current.comparison.rows
            && retained.comparison.facility_energy_savings
                == current.comparison.facility_energy_savings
            && retained.comparison.facility_peak_savings
                == current.comparison.facility_peak_savings
            && retained.comparison.lighting_service_equivalence
                == current.comparison.lighting_service_equivalence,
        "Exported comparison differs from retained totals/service declaration"
    );
    Ok(())
}

/// Integrity inspection and current compiled assessments without either engine.
/// The retained report is historical; its assessment is never imported as authority.
pub fn inspect_package(package: &Path) -> Result<Value> {
    let imported = crate::local_root()?
        .join("energy-inspection")
        .join(Uuid::new_v4().to_string());
    extract_package(package, &imported)?;
    let view = crate::open_project(&imported)?;
    let selection: Value = read_json(&regular_child(&imported, "energy-export.json")?)?;
    ensure!(
        selection["schema_version"] == "greenlens.energy-export.v0.4",
        "Unsupported energy package"
    );
    let data = energy_services::report_data(
        &imported,
        &view.project,
        selection["baseline_run_id"]
            .as_str()
            .context("Missing baseline run")?,
        selection["proposed_run_id"]
            .as_str()
            .context("Missing proposed run")?,
    )?;
    let retained: greenlens_core::energy::EnergyReportData =
        read_json(&regular_child(&imported, "reports/energy-report.json")?)?;
    validate_report_binding(
        &retained,
        &data,
        selection["report_dataset_sha256"]
            .as_str()
            .context("Missing report hash")?,
    )?;
    Ok(
        json!({"schema_version":"greenlens.energy-package-inspection.v0.4","package_sha256":greenlens_process::file_hash(package)?,"retained_report_dataset_sha256":retained.dataset_sha256()?,"current_report":data,"scope":"Complete retained evidence and report bindings checked; no simulator or renderer was executed."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supplementary_assessment_changes_preserve_replay_but_changed_evidence_rejects() {
        let retained = greenlens_core::energy::synthetic_energy_report().unwrap();
        let old = serde_json::to_vec(&retained).unwrap();
        let hash = retained.dataset_sha256().unwrap();
        let mut current = retained.clone();
        current.baseline.assessment.scientific.reason =
            "A later registry has new evidence; historical bytes remain untouched.".into();
        current.baseline.assessment.scientific.registry_version =
            "greenlens.energy-qualification.v2".into();
        current.baseline.assessment.current_physical_inputs =
            greenlens_core::energy::EnergyInputRelation::Changed;
        current.title = "Regenerated presentation".into();
        validate_report_binding(&retained, &current, &hash).unwrap();
        assert_eq!(old, serde_json::to_vec(&retained).unwrap());
        assert!(validate_report_binding(&retained, &current, &"0".repeat(64)).is_err());
        current.baseline.record_sha256 = "a".repeat(64);
        current.baseline.diagnostics.source_record_sha256 = current.baseline.record_sha256.clone();
        assert!(validate_report_binding(&retained, &current, &hash).is_err());
    }
}
