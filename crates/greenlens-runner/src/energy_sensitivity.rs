//! Declaration, immutable-run reuse and assessment only. This module never
//! launches an engine. A caller must retain the plan before explicit missing-run
//! execution and supply every failed/cancelled attempt to the final assessment.
use crate::{
    energy::{read_result_metadata, read_run, EnergyRunRecord},
    energy_weather, Project,
};
use anyhow::{ensure, Context, Result};
use greenlens_core::{canonical_hash, energy::*};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergySensitivityReuse {
    pub run_id: String,
    pub record_sha256: String,
    pub finished_utc: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergySensitivityPlannedSlot {
    pub slot_id: String,
    pub reuse: Option<EnergySensitivityReuse>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergySensitivityPlan {
    pub schema_version: String,
    pub created_utc: String,
    pub declaration: EnergySensitivityDeclaration,
    pub declaration_sha256: String,
    pub current_inputs_sha256: String,
    pub slots: Vec<EnergySensitivityPlannedSlot>,
    pub execution_authorized: bool,
    pub reuse_policy: String,
}
pub(crate) const REUSE_POLICY: &str = "Reuse the earliest UTC-completed exact input/method run, breaking equal-time ties by canonical run ID. Full evidence reprocessing is required before reuse. Never choose by numerical outcome; missing slots require separate explicit orchestration, with every failure/cancellation retained.";
impl EnergySensitivityPlan {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == "greenlens.energy-sensitivity-plan.v1"
                && !self.execution_authorized
                && self.reuse_policy == REUSE_POLICY,
            "Unsupported or altered sensitivity reuse plan"
        );
        let time = chrono::DateTime::parse_from_rfc3339(&self.created_utc)?;
        ensure!(
            time.offset().local_minus_utc() == 0,
            "Plan time must explicitly be UTC"
        );
        ensure!(
            self.declaration.identity()? == self.declaration_sha256,
            "Sensitivity declaration seal mismatch"
        );
        ensure!(
            is_hash(&self.current_inputs_sha256),
            "Invalid current-input binding"
        );
        ensure!(
            self.slots
                .iter()
                .map(|s| &s.slot_id)
                .eq(self.declaration.slots.iter().map(|s| &s.id)),
            "Plan slot coverage/order differs from declaration"
        );
        for slot in &self.slots {
            if let Some(reuse) = &slot.reuse {
                ensure!(
                    uuid::Uuid::parse_str(&reuse.run_id)?.to_string() == reuse.run_id
                        && is_hash(&reuse.record_sha256),
                    "Invalid reused run binding"
                );
                let finished = chrono::DateTime::parse_from_rfc3339(&reuse.finished_utc)?;
                ensure!(
                    finished.offset().local_minus_utc() == 0 && finished <= time,
                    "Reuse requires an exact completed run that predates this declaration"
                );
            }
        }
        Ok(())
    }
    pub fn identity(&self) -> Result<String> {
        self.validate()?;
        Ok(canonical_hash(self)?)
    }
    pub fn missing_slots(&self) -> Vec<String> {
        self.slots
            .iter()
            .filter(|s| s.reuse.is_none())
            .map(|s| s.slot_id.clone())
            .collect()
    }
}
fn is_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn ordered_targets(targets: &[String]) -> Result<Vec<String>> {
    ensure!(
        !targets.is_empty()
            && targets.len() <= 2
            && targets.iter().all(|s| s == "A" || s == "B")
            && targets.iter().collect::<BTreeSet<_>>().len() == targets.len(),
        "Explicitly select unique A and/or B sensitivity targets"
    );
    Ok(["A", "B"]
        .into_iter()
        .filter(|id| targets.iter().any(|t| t == id))
        .map(String::from)
        .collect())
}
/// Binds the accepted source models/studies, including their current timestep;
/// report settings, view state and newly appended run history do not affect it.
pub fn current_inputs_identity(project: &Project, targets: &[String]) -> Result<String> {
    crate::validate_project(project)?;
    let ordered = ordered_targets(targets)?;
    let energy = project.energy.as_ref().context("Energy document missing")?;
    let inputs = ordered
        .iter()
        .map(|id| {
            Ok((
                id,
                energy.designs.get(id).context("Selected design missing")?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(canonical_hash(&(
        "greenlens.energy-sensitivity-current-inputs.v1",
        inputs,
    ))?)
}
fn matches_slot(run: &EnergyRunRecord, slot: &EnergySensitivitySlot) -> bool {
    run.alternative_id == slot.alternative_id
        && run.provenance.physical_input_hash == slot.physical_input_sha256
        && run.provenance.execution_input_hash == slot.execution_input_sha256
        && run.method == slot.method
        && run.validated.model_input_sha256 == slot.model_input_sha256
}
fn choose_earliest(
    records: &[EnergyRunRecord],
    slot: &EnergySensitivitySlot,
) -> Result<Option<String>> {
    let mut candidates = Vec::new();
    for run in records.iter().filter(|r| matches_slot(r, slot)) {
        candidates.push((
            chrono::DateTime::parse_from_rfc3339(&run.provenance.finished_utc)?,
            run.id.as_str(),
        ));
    }
    candidates.sort();
    Ok(candidates.first().map(|(_, id)| (*id).to_owned()))
}

pub fn plan(
    root: &Path,
    project: &Project,
    targets: &[String],
    expected_current_inputs_sha256: Option<&str>,
) -> Result<EnergySensitivityPlan> {
    let current = current_inputs_identity(project, targets)?;
    if let Some(expected) = expected_current_inputs_sha256 {
        ensure!(
            expected == current,
            "Accepted study inputs changed; review a fresh sensitivity declaration"
        );
    }
    let energy = project.energy.as_ref().context("Energy document missing")?;
    let mut declared = Vec::new();
    for alternative in ordered_targets(targets)? {
        let design = &energy.designs[&alternative];
        let mut study = design
            .study
            .clone()
            .context("Sensitivity requires a configured study and portable original weather")?;
        let weather = energy
            .weather
            .iter()
            .find(|w| w.binding == study.weather)
            .context("Declared weather asset is missing")?;
        let (asset, parsed) = energy_weather::open(&root.join(&weather.path))?;
        ensure!(
            asset.binding == study.weather,
            "Reparsed original/normalized weather differs from study binding"
        );
        validate_weather_context(&design.model, &study, &parsed.data)?;
        study.zone_timesteps_per_hour = 6;
        let adapter = greenlens_energyplus::preflight_model(&design.model, &study);
        ensure!(
            adapter.supported,
            "Sensitivity input is unsupported by the adapter: {}",
            adapter
                .issues
                .iter()
                .map(|i| i.message.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        );
        declared.push(EnergySensitivityTarget {
            alternative_id: alternative,
            model: design.model.clone(),
            study_10min: study,
        });
    }
    let ten = greenlens_energyplus::method_identity(&declared[0].study_10min)?;
    let mut fine = declared[0].study_10min.clone();
    fine.zone_timesteps_per_hour = 12;
    let five = greenlens_energyplus::method_identity(&fine)?;
    let declaration = declare_energy_sensitivity(declared, ten, five)?;
    let mut slots = Vec::new();
    for slot in &declaration.slots {
        let records = energy
            .runs
            .iter()
            .filter(|r| {
                r.alternative_id == slot.alternative_id
                    && r.physical_input_hash == slot.physical_input_sha256
            })
            .map(|summary| read_result_metadata(root, summary))
            .collect::<Result<Vec<_>>>()?;
        let reuse = if let Some(id) = choose_earliest(&records, slot)? {
            let summary = energy
                .runs
                .iter()
                .find(|r| r.id == id)
                .context("Reuse record disappeared")?;
            let run = read_run(root, summary)?;
            ensure!(
                matches_slot(&run, slot),
                "Fully read run no longer matches declared slot"
            );
            Some(EnergySensitivityReuse {
                run_id: run.id.clone(),
                record_sha256: canonical_hash(&run)?,
                finished_utc: run.provenance.finished_utc.clone(),
            })
        } else {
            None
        };
        slots.push(EnergySensitivityPlannedSlot {
            slot_id: slot.id.clone(),
            reuse,
        });
    }
    let result = EnergySensitivityPlan {
        schema_version: "greenlens.energy-sensitivity-plan.v1".into(),
        created_utc: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
        declaration_sha256: declaration.identity()?,
        declaration,
        current_inputs_sha256: current,
        slots,
        execution_authorized: false,
        reuse_policy: REUSE_POLICY.into(),
    };
    result.validate()?;
    Ok(result)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergySensitivityResult {
    pub schema_version: String,
    pub plan_sha256: String,
    pub current_inputs_match: bool,
    pub assessment: EnergySensitivityAssessment,
    pub evidence_scope: String,
}
/// `completed` binds explicit missing slots to their newly retained run IDs.
/// Reuse slots cannot be replaced by a later, more favorable result.
pub fn evaluate(
    root: &Path,
    project: &Project,
    plan: &EnergySensitivityPlan,
    completed: &BTreeMap<String, String>,
    interruptions: &[EnergySensitivityInterruption],
) -> Result<EnergySensitivityResult> {
    evaluate_with_runs(project, plan, completed, interruptions, &mut |summary| {
        read_run(root, summary)
    })
}
/// Internal provider must return records checked by the full immutable reader.
/// This seam never accepts frontend/serialized verification claims.
pub(crate) fn evaluate_with_runs(
    project: &Project,
    plan: &EnergySensitivityPlan,
    completed: &BTreeMap<String, String>,
    interruptions: &[EnergySensitivityInterruption],
    runs: &mut impl FnMut(&crate::energy::EnergyRunSummary) -> Result<EnergyRunRecord>,
) -> Result<EnergySensitivityResult> {
    plan.validate()?;
    let energy = project.energy.as_ref().context("Energy document missing")?;
    ensure!(
        completed
            .keys()
            .all(|id| plan.slots.iter().any(|s| &s.slot_id == id)),
        "Completion map contains undeclared slots"
    );
    let mut assessed_runs = Vec::new();
    for planned in &plan.slots {
        crate::project_open::checkpoint(0)?;
        let id = if let Some(reuse) = &planned.reuse {
            ensure!(
                completed
                    .get(&planned.slot_id)
                    .is_none_or(|id| id == &reuse.run_id),
                "A declared reused run cannot be replaced"
            );
            Some(&reuse.run_id)
        } else {
            completed.get(&planned.slot_id)
        };
        if let Some(id) = id {
            let summary = energy
                .runs
                .iter()
                .find(|r| &r.id == id)
                .context("Completed sensitivity run is not retained by this project")?;
            let run = runs(summary)?;
            crate::energy::validate_summary_binding(&run, summary)?;
            let slot = plan
                .declaration
                .slots
                .iter()
                .find(|s| s.id == planned.slot_id)
                .context("Missing declared slot")?;
            ensure!(
                matches_slot(&run, slot),
                "Selected run has a different complete study/model/method identity"
            );
            let seal = canonical_hash(&run)?;
            if let Some(reuse) = &planned.reuse {
                ensure!(
                    seal == reuse.record_sha256
                        && run.provenance.finished_utc == reuse.finished_utc,
                    "Reused immutable run changed after declaration"
                );
            }
            let (model, study) = plan.declaration.slot_inputs(&slot.id)?;
            ensure!(
                run.model == *model && run.study == study,
                "Retained typed inputs differ from the exact declared snapshots"
            );
            assessed_runs.push(EnergySensitivityRun {
                slot_id: slot.id.clone(),
                run_id: run.id,
                record_sha256: seal,
                physical_input_sha256: run.provenance.physical_input_hash,
                execution_input_sha256: run.provenance.execution_input_hash,
                method: run.method,
                totals: run.totals,
            });
        }
    }
    let targets = plan
        .declaration
        .targets
        .iter()
        .map(|t| t.alternative_id.clone())
        .collect::<Vec<_>>();
    let current = current_inputs_identity(project, &targets)?;
    Ok(EnergySensitivityResult{schema_version:"greenlens.energy-sensitivity-result.v1".into(),plan_sha256:plan.identity()?,current_inputs_match:current==plan.current_inputs_sha256,
        assessment:assess_energy_sensitivity(&plan.declaration,&assessed_runs,interruptions)?,evidence_scope:"Every attached run was fully integrity-checked and reprocessed from immutable series, original weather, recipe, inputs and process receipts. Interruption receipt hashes are orchestration bindings; this function does not independently open those external receipts or authorize execution.".into()})
}

#[cfg(test)]
mod tests {
    use super::*;
    // Synthetic incomplete records below are selection fixtures, never native
    // evidence and never passed to read_run or evaluate.
    fn fixture() -> (EnergySensitivitySlot, EnergyRunRecord) {
        let report = synthetic_energy_report().unwrap();
        let c = report.baseline;
        let method = greenlens_energyplus::method_identity(&c.study).unwrap();
        let physical = c.study.physical_input_hash(&c.model).unwrap();
        let execution = c.study.execution_input_hash(&c.model, &method).unwrap();
        let slot = EnergySensitivitySlot {
            id: "A10".into(),
            alternative_id: "A".into(),
            zone_timesteps_per_hour: 6,
            model_input_sha256: c.model_input_sha256.clone(),
            physical_input_sha256: physical.clone(),
            execution_input_sha256: execution.clone(),
            method: method.clone(),
        };
        let run = EnergyRunRecord {
            schema_version: crate::energy::RUN_SCHEMA.into(),
            id: "00000000-0000-4000-8000-000000000001".into(),
            alternative_id: "A".into(),
            model: c.model.clone(),
            study: c.study.clone(),
            validated: validate_thermal_model(&c.model, &c.study.calendar).unwrap(),
            weather_asset: energy_weather::EnergyWeatherAsset {
                schema_version: energy_weather::ASSET_SCHEMA.into(),
                binding: c.study.weather.clone(),
                source: crate::energy::synthetic_source(),
            },
            method,
            ledger: c.meter_ledger,
            series: crate::energy_series::EnergySeriesManifest {
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
            totals: c.totals,
            provenance: crate::energy::EnergyProvenance {
                application_version: "synthetic-selection-only".into(),
                source_commit: None,
                source_worktree_dirty: None,
                started_utc: "2026-01-01T00:00:00Z".into(),
                finished_utc: "2026-01-01T01:00:00Z".into(),
                physical_input_hash: physical,
                execution_input_hash: execution,
                physical_result_hash: "a".repeat(64),
                processing_identity: "b".repeat(64),
                processes: vec![],
                diagnostics: vec![],
                warning_count: 0,
                severe_count: 0,
                raw_representation: "synthetic selection fixture".into(),
            },
        };
        (slot, run)
    }
    #[test]
    fn reuse_is_exact_and_earliest_never_selected_by_favorable_totals() {
        let (slot, first) = fixture();
        let mut later = first.clone();
        later.id = "00000000-0000-4000-8000-000000000002".into();
        later.provenance.finished_utc = "2026-01-02T01:00:00Z".into();
        later.totals.annual.facility_kwh = 0.;
        assert_eq!(
            choose_earliest(&[later.clone(), first.clone()], &slot).unwrap(),
            Some(first.id.clone())
        );
        let mut wrong = first.clone();
        wrong.method.numerical_settings_sha256 = "f".repeat(64);
        assert_eq!(choose_earliest(&[wrong], &slot).unwrap(), None);
        let mut wrong = first.clone();
        wrong.provenance.physical_input_hash = "f".repeat(64);
        assert_eq!(choose_earliest(&[wrong], &slot).unwrap(), None);
        later.provenance.finished_utc = first.provenance.finished_utc.clone();
        assert_eq!(
            choose_earliest(&[later, first.clone()], &slot).unwrap(),
            Some(first.id)
        );
    }
    #[test]
    fn current_binding_ignores_view_reports_but_rejects_model_study_changes() {
        let p = crate::energy_services::example_document();
        let targets = vec!["A".into(), "B".into()];
        let id = current_inputs_identity(&p, &targets).unwrap();
        let mut report = p.clone();
        report.name = "Changed report title".into();
        report.energy.as_mut().unwrap().lighting_service_evidence =
            Some("Explicit separate service evidence".into());
        assert_eq!(current_inputs_identity(&report, &targets).unwrap(), id);
        report
            .energy
            .as_mut()
            .unwrap()
            .designs
            .get_mut("B")
            .unwrap()
            .model
            .zones[0]
            .cooling_system
            .as_mut()
            .unwrap()
            .supply_fan
            .total_efficiency = 0.7;
        assert_ne!(current_inputs_identity(&report, &targets).unwrap(), id);
        assert!(ordered_targets(&["A".into(), "A".into()]).is_err());
        assert!(ordered_targets(&["C".into()]).is_err());
    }
}
