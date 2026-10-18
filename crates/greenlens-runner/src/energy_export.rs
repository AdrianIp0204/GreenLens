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
    export_pair_impl(root, a, b, destination, typst, cancel, false)
}
pub fn export_workbench_pair(
    root: &Path,
    a: &str,
    b: &str,
    destination: &Path,
    typst: &TypstConfig,
    cancel: &AtomicBool,
) -> Result<Value> {
    export_pair_impl(root, a, b, destination, typst, cancel, true)
}
pub fn export_workbench_pair_cancellable(
    root: &Path,
    a: &str,
    b: &str,
    destination: &Path,
    typst: &TypstConfig,
    cancel: std::sync::Arc<AtomicBool>,
) -> Result<Value> {
    crate::project_open::with_cancellation(cancel.clone(), || {
        export_pair_impl(root, a, b, destination, typst, &cancel, true)
    })
}
fn export_pair_impl(
    root: &Path,
    a: &str,
    b: &str,
    destination: &Path,
    typst: &TypstConfig,
    cancel: &AtomicBool,
    workbench: bool,
) -> Result<Value> {
    ensure!(
        !destination.exists(),
        "Export destination exists; choose a new file"
    );
    let _lock = WriterLock::acquire(root)?;
    let view = crate::open_project(root)?;
    ensure!(view.project.design_reports.is_none(), "Use complete design-report project export; a selected energy export cannot omit accepted reports.");
    ensure!(view.project.report_issues.as_ref().is_none_or(|index| index.issues.is_empty()),
        "This project retains immutable report issues. Use complete issued-project export to retain all issue dependencies; selected-pair export would remove required history");
    let supplement = if workbench {
        Some(energy_services::workbench_report_data(
            root,
            &view.project,
            a,
            b,
        )?)
    } else {
        None
    };
    let mut dataset = energy_services::report_data(root, &view.project, a, b)?;
    let sensitivity =
        crate::energy_sensitivity_workflow::export_selection(root, &view.project, a, b)?;
    // Excluded evidence metadata can itself be private; omit it from public products.
    dataset.evidence.retain(|e| e.included);
    let architecture = energy_report::architectural::prepare_project(
        root,
        &view.project,
        supplement.as_ref().map(|d| &d.base).unwrap_or(&dataset),
        supplement.as_ref().map(|d| d.derived_sha256.as_str()),
        cancel,
    )?;
    let mut project = view.project;
    // Selection deliberately bounds comparison to one explicit pair. The saved
    // document retains its current inputs; reports use the immutable snapshots.
    project.runs.clear();
    project.climate = None;
    let extension = project.energy.as_mut().context("Energy project missing")?;
    extension
        .runs
        .retain(|r| sensitivity.included_run_ids.contains(&r.id));
    if let Some(state) = &mut extension.workbench {
        state
            .sensitivity
            .retain(|r| Some(&r.id) == sensitivity.included_session_id.as_ref());
    }
    extension.attachments.retain(|item| {
        item.include_in_export
            && dataset
                .evidence
                .iter()
                .any(|e| e.relative_path == item.path)
    });
    ensure!(
        a != b
            && extension.runs.iter().any(|r| r.id == a)
            && extension.runs.iter().any(|r| r.id == b),
        "Select two distinct retained baseline/proposed runs"
    );
    let mut files = vec!["project.json".into()];
    let mut budget = PackageBudget::default();
    let manifest_bytes = crate::history_storage::manifest_bytes(&project)?;
    budget.add("project.json", manifest_bytes.len() as u64)?;
    for entry in crate::history_storage::entries(&project)? {
        budget.add(&entry.path, entry.bytes)?;
        files.push(entry.path);
    }
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
    for name in crate::energy_sensitivity_workflow::evidence_members(root, &project)? {
        budget.add(&name, fs::metadata(regular_child(root, &name)?)?.len())?;
        files.push(name);
    }
    if let Some(reference) = &project.architecture {
        // Every admitted revision is required by the portable project. Do not
        // silently retain its manifest while omitting geometry assets.
        for name in crate::reference::evidence_members(root, reference)? {
            budget.add(&name, fs::metadata(regular_child(root, &name)?)?.len())?;
            files.push(name);
        }
    }
    ensure!(!cancel.load(Ordering::SeqCst), "Export cancelled");
    let stage = crate::local_root()?
        .join("energy-exports")
        .join(Uuid::new_v4().to_string());
    fs::create_dir_all(&stage)?;
    energy::copy_project_evidence(root, &stage, extension)?;
    crate::energy_sensitivity_workflow::copy_evidence(root, &stage, &project)?;
    if let Some(reference) = &project.architecture {
        crate::reference::copy_evidence(root, &stage, reference)?;
    }
    crate::history_storage::stage(&stage, &project, cancel)?;
    fs::write(stage.join("project.json"), manifest_bytes)?;
    let reports = if let Some(data) = &supplement {
        crate::energy_report_workbench::render_with_architecture(
            data,
            &stage.join("reports"),
            Some(typst),
            cancel,
            architecture.as_ref(),
        )?
    } else {
        energy_report::render_with_architecture(
            &dataset,
            &stage.join("reports"),
            Some(typst),
            cancel,
            architecture.as_ref(),
        )?
    };
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
        &json!({"schema_version":if architecture.is_some(){"greenlens.energy-export.v0.6"}else if workbench {"greenlens.energy-export.v0.5"}else{"greenlens.energy-export.v0.4"},"baseline_run_id":a,"proposed_run_id":b,"report_dataset_sha256":dataset.dataset_sha256()?,"workbench_report_sha256":supplement.as_ref().map(|d|&d.derived_sha256),"architectural_report_sha256":architecture.as_ref().map(|d|&d.data.report_sha256),"architectural_storage":architecture.as_ref().map(|_|energy_report::architectural::STORAGE_DISCLOSURE),"sensitivity":sensitivity,"assessment_authority":"Supplementary report only; the compiled application independently recomputes integrity and qualification without altering run evidence."}),
    )?;
    fs::write(stage.join("REPLAY.txt"),"GreenLens energy evidence\nInspect/regenerate reports without EnergyPlus or the original weather/cache. Original/normalized weather, immutable inputs and all retained native channels are included.\nPDF regeneration requires the pinned separately installed Typst/font package.\nInstall pinned EnergyPlus 26.2.0 separately for replay: greenlens energy replay evidence.zip --engine-dir <compatible-engine-directory>\nReplay regenerates trusted epJSON from authoritative inputs using the compiled recognized adapter; no imported executable recipe or report is executed.\nEvery channel is checked within max(1e-6 in its unit, 0.1% of reference), with exact shape, channel labels and interval-end local-standard timestamps.\nAgreement is integration verification, not general physical validation or certification.\n")?;
    if architecture.is_some() {
        use std::io::Write;
        writeln!(fs::OpenOptions::new().append(true).open(stage.join("REPLAY.txt"))?, "\n{}\nArchitectural scope and figure identities are checked against the selected retained run snapshots. Historical scope remains explicitly labeled; no source-to-model binding is imported as scientific qualification.", energy_report::architectural::STORAGE_DISCLOSURE)?;
    }
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
        json!({"schema_version":if architecture.is_some(){"greenlens.energy-export-result.v0.6"}else{"greenlens.energy-export-result.v0.4"},"entries":files.len(),"package_bytes":fs::metadata(destination)?.len(),"package_sha256":greenlens_process::file_hash(destination)?,"report_dataset_sha256":dataset.dataset_sha256()?,"architectural_report_sha256":architecture.as_ref().map(|d|&d.data.report_sha256),"architectural_storage":architecture.as_ref().map(|_|energy_report::architectural::STORAGE_DISCLOSURE),"baseline_run_id":a,"proposed_run_id":b}),
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
    ensure!(recognized_package(&selection), "Unsupported energy package");
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
    validate_workbench_supplement(&imported, &selection, &retained_data)?;
    validate_architectural_supplement(&imported, &view.project, &selection, &retained_data)?;
    let mut results = vec![];
    for id in view
        .project
        .energy
        .as_ref()
        .context("Energy project missing")?
        .runs
        .iter()
        .map(|r| r.id.as_str())
    {
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

fn recognized_package(selection: &Value) -> bool {
    matches!(
        selection["schema_version"].as_str(),
        Some(
            "greenlens.energy-export.v0.4"
                | "greenlens.energy-export.v0.5"
                | "greenlens.energy-export.v0.6"
        )
    )
}
fn validate_workbench_supplement(
    root: &Path,
    selection: &Value,
    base: &greenlens_core::energy::EnergyReportData,
) -> Result<()> {
    if selection["schema_version"] == "greenlens.energy-export.v0.5"
        || (selection["schema_version"] == "greenlens.energy-export.v0.6"
            && selection["workbench_report_sha256"].is_string())
    {
        let data: greenlens_core::energy::EnergyReportWorkbenchData =
            read_json(&regular_child(root, "reports/workbench-report.json")?)?;
        data.validate()?;
        ensure!(
            data.base == *base
                && Some(data.derived_sha256.as_str())
                    == selection["workbench_report_sha256"].as_str(),
            "Supplementary report identity differs from retained immutable dataset"
        );
    }
    Ok(())
}

fn validate_architectural_supplement(
    root: &Path,
    project: &crate::Project,
    selection: &Value,
    base: &greenlens_core::energy::EnergyReportData,
) -> Result<()> {
    if selection["schema_version"] == "greenlens.energy-export.v0.6" {
        energy_report::architectural::validate_retained_package(
            root,
            project,
            base,
            selection["workbench_report_sha256"].as_str(),
            selection["architectural_report_sha256"]
                .as_str()
                .context("Missing architectural report binding")?,
        )?;
        ensure!(
            selection["architectural_storage"] == energy_report::architectural::STORAGE_DISCLOSURE,
            "Architectural asset inclusion policy is missing or unsupported"
        );
    } else {
        ensure!(project.architecture.is_none(), "An architectural project requires v0.6 export with complete normalized assets and report binding");
    }
    Ok(())
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
    ensure!(recognized_package(&selection), "Unsupported energy package");
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
    validate_workbench_supplement(&imported, &selection, &retained)?;
    validate_architectural_supplement(&imported, &view.project, &selection, &retained)?;
    let architecture = if selection["schema_version"] == "greenlens.energy-export.v0.6" {
        Some(read_json::<
            energy_report::architectural::ArchitecturalReportData,
        >(&regular_child(
            &imported,
            "reports/architecture-report.json",
        )?)?)
    } else {
        None
    };
    Ok(
        json!({"schema_version":"greenlens.energy-package-inspection.v0.4","package_sha256":greenlens_process::file_hash(package)?,"retained_report_dataset_sha256":retained.dataset_sha256()?,"current_report":data,"architectural_report":architecture,"scope":"Complete retained evidence and report bindings checked; no simulator or renderer was executed."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_packages_remain_recognized_and_architecture_cannot_hide_in_old_envelopes() {
        for version in [
            "greenlens.energy-export.v0.4",
            "greenlens.energy-export.v0.5",
            "greenlens.energy-export.v0.6",
        ] {
            assert!(recognized_package(&json!({"schema_version":version})));
        }
        assert!(!recognized_package(
            &json!({"schema_version":"greenlens.energy-export.unknown"})
        ));
        let base = greenlens_core::energy::synthetic_energy_report().unwrap();
        let mut project = crate::example();
        validate_architectural_supplement(
            Path::new("."),
            &project,
            &json!({"schema_version":"greenlens.energy-export.v0.4"}),
            &base,
        )
        .unwrap();
        project.architecture = Some(crate::reference::ArchitecturalProject {
            schema_version: "greenlens.architectural-project.v1".into(),
            assets: vec![],
            active_asset_id: String::new(),
            scope: None,
            scope_history: None,
        });
        assert!(validate_architectural_supplement(
            Path::new("."),
            &project,
            &json!({"schema_version":"greenlens.energy-export.v0.5"}),
            &base
        )
        .is_err());
        assert!(validate_architectural_supplement(
            Path::new("."),
            &project,
            &json!({"schema_version":"greenlens.energy-export.v0.6"}),
            &base
        )
        .is_err());
    }
    #[test]
    fn architectural_assets_use_unchanged_shared_archive_limits() {
        let mut budget = PackageBudget::default();
        assert!(budget
            .add(
                "references/example/geometry/chunk.bin",
                64 * 1024 * 1024 + 1
            )
            .is_err());
        let mut budget = PackageBudget::default();
        for i in 0..4 {
            budget
                .add(
                    &format!("references/example/geometry/{i}.bin"),
                    64 * 1024 * 1024,
                )
                .unwrap();
        }
        assert!(budget.add("reports/architecture-report.json", 1).is_err());
    }
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
        current.baseline.assessment.scientific.status =
            greenlens_core::energy::EnergyQualificationStatus::LimitedQualified;
        current
            .baseline
            .assessment
            .scientific
            .applicable_physical_input_sha256 =
            vec![current.baseline.physical_input_sha256.clone()];
        current.baseline.assessment.scientific.evidence_ids =
            vec!["synthetic:later-exact-scope-evidence".into()];
        current.baseline.assessment.current_physical_inputs =
            greenlens_core::energy::EnergyInputRelation::Changed;
        current.title = "Regenerated presentation".into();
        current.limitations.push(
            "Later presentation explicitly discloses unevaluated lower-temperature service hours."
                .into(),
        );
        validate_report_binding(&retained, &current, &hash).unwrap();
        assert_eq!(old, serde_json::to_vec(&retained).unwrap());
        assert!(validate_report_binding(&retained, &current, &"0".repeat(64)).is_err());
        current.baseline.record_sha256 = "a".repeat(64);
        current.baseline.diagnostics.source_record_sha256 = current.baseline.record_sha256.clone();
        assert!(validate_report_binding(&retained, &current, &hash).is_err());
    }
}
