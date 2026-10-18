//! Shared v0.5 authoring actions. Presentation locks are serialized separately
//! from the immutable scientific model and excluded from physical identities.
use crate::{energy::EnergyDesign, energy_services, Project};
use anyhow::{ensure, Context, Result};
use greenlens_core::{canonical_hash, energy::*};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const WORKBENCH_SCHEMA: &str = "greenlens.energy-workbench.v1";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnergyWorkbench {
    pub schema_version: String,
    pub locks: BTreeMap<String, Vec<String>>,
    /// Explicit calendars retained before weather is assigned. A configured
    /// study's calendar is authoritative when present.
    pub calendars: BTreeMap<String, EnergyCalendar>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_metadata: Option<EnergyReportMetadata>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sensitivity: Vec<crate::energy_sensitivity_workflow::EnergySensitivityReference>,
    /// Accepted proposal declarations; separate from physical input identities.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub design_proposals: Vec<crate::design_proposals::ProposalReceipt>,
}
impl Default for EnergyWorkbench {
    fn default() -> Self {
        Self {
            schema_version: WORKBENCH_SCHEMA.into(),
            locks: BTreeMap::new(),
            calendars: BTreeMap::new(),
            report_metadata: None,
            sensitivity: vec![],
            design_proposals: vec![],
        }
    }
}
pub fn validate_state(project: &crate::energy::EnergyProject) -> Result<()> {
    if let Some(state) = &project.workbench {
        ensure!(
            state.schema_version == WORKBENCH_SCHEMA,
            "Unsupported energy workbench schema"
        );
        for (id, locks) in &state.locks {
            let design = project
                .designs
                .get(id)
                .context("Locks reference an unknown design")?;
            enforce_thermal_locks(&design.model, &design.model, locks)?;
        }
        for (id, calendar) in &state.calendars {
            ensure!(
                project.designs.contains_key(id),
                "Calendar references an unknown design"
            );
            calendar.validate()?;
        }
        if let Some(metadata) = &state.report_metadata {
            metadata.validate()?;
        }
        crate::energy_sensitivity_workflow::validate_references(&state.sensitivity)?;
        crate::design_proposals::validate_receipts(&state.design_proposals)?;
    }
    Ok(())
}
pub fn set_report_metadata(
    project: &Project,
    metadata: Option<EnergyReportMetadata>,
) -> Result<Project> {
    if let Some(value) = &metadata {
        value.validate()?;
    }
    let mut next = project.clone();
    next.energy
        .as_mut()
        .context("Energy document missing")?
        .workbench
        .get_or_insert_with(Default::default)
        .report_metadata = metadata;
    crate::validate_project(&next)?;
    Ok(next)
}
pub fn locks<'a>(project: &'a Project, alternative: &str) -> &'a [String] {
    project
        .energy
        .as_ref()
        .and_then(|e| e.workbench.as_ref())
        .and_then(|w| w.locks.get(alternative))
        .map(Vec::as_slice)
        .unwrap_or(&[])
}
pub fn calendar_for(project: &Project, alternative: &str) -> Result<EnergyCalendar> {
    let energy = project.energy.as_ref().context("Energy document missing")?;
    let design = energy.designs.get(alternative).context("Choose A or B")?;
    if let Some(study) = &design.study {
        return Ok(study.calendar.clone());
    }
    if let Some(calendar) = energy
        .workbench
        .as_ref()
        .and_then(|w| w.calendars.get(alternative))
    {
        return Ok(calendar.clone());
    }
    Ok(EnergyCalendar {
        start_weekday: Weekday::Monday,
        holidays: design
            .model
            .schedules
            .iter()
            .flat_map(|s| s.holidays.iter().map(|h| h.date))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect(),
        utc_offset_minutes: 0,
    })
}
pub fn new_document(zone: ZoneBoxInput, calendar: EnergyCalendar) -> Result<Project> {
    let model = new_thermal_model(&zone, &calendar)?;
    let mut project = energy_services::example_document();
    project.name = "New energy project".into();
    let energy = project.energy.as_mut().context("Energy document missing")?;
    for design in energy.designs.values_mut() {
        *design = EnergyDesign {
            model: model.clone(),
            study: None,
        };
    }
    energy.carbon_factor = None;
    energy.lighting_service_evidence = None;
    energy.workbench = Some(EnergyWorkbench {
        calendars: [("A".into(), calendar.clone()), ("B".into(), calendar)].into(),
        ..Default::default()
    });
    crate::validate_project(&project)?;
    Ok(project)
}
pub fn set_locks(project: &Project, alternative: &str, ids: Vec<String>) -> Result<Project> {
    let mut next = project.clone();
    let energy = next.energy.as_mut().context("Energy document missing")?;
    let design = energy.designs.get(alternative).context("Choose A or B")?;
    enforce_thermal_locks(&design.model, &design.model, &ids)?;
    energy
        .workbench
        .get_or_insert_with(Default::default)
        .locks
        .insert(alternative.into(), ids);
    crate::validate_project(&next)?;
    Ok(next)
}
/// Undo/redo replaces only the explicit scientific designs. Current retained
/// runs, weather, report settings and display state survive the restoration.
pub fn restore_designs(
    project: &Project,
    designs: BTreeMap<String, EnergyDesign>,
) -> Result<Project> {
    ensure!(
        !designs.is_empty() && designs.len() <= 2,
        "Restore requires explicit A/B design targets"
    );
    let mut next = project.clone();
    for (id, design) in designs {
        let original = project
            .energy
            .as_ref()
            .and_then(|e| e.designs.get(&id))
            .context("Unknown design target")?;
        enforce_thermal_locks(&original.model, &design.model, locks(project, &id))?;
        *next
            .energy
            .as_mut()
            .context("Energy document missing")?
            .designs
            .get_mut(&id)
            .context("Unknown design target")? = design;
    }
    crate::validate_project(&next)?;
    Ok(next)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionSetPreview {
    pub schema_version: String,
    pub preview_sha256: String,
    pub designs: BTreeMap<String, ThermalTransactionPreview>,
    pub untouched_designs: Vec<String>,
    pub valid: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationPlan {
    pub requests: BTreeMap<String, ThermalTransaction>,
    pub preview: TransactionSetPreview,
}
pub fn plan_operation(
    project: &Project,
    operations: BTreeMap<String, ThermalOperation>,
    topology: TopologyAssignments,
) -> Result<OperationPlan> {
    let energy = project.energy.as_ref().context("Energy document missing")?;
    let mut requests = BTreeMap::new();
    for (id, operation) in operations {
        let design = energy.designs.get(&id).context("Choose A or B")?;
        requests.insert(
            id.clone(),
            ThermalTransaction {
                schema_version: THERMAL_TRANSACTION_SCHEMA.into(),
                expected_model_sha256: validate_thermal_model(
                    &design.model,
                    &calendar_for(project, &id)?,
                )?
                .model_input_sha256,
                operation,
                topology: topology.clone(),
            },
        );
    }
    Ok(OperationPlan {
        preview: preview(project, &requests)?,
        requests,
    })
}
/// The exact target map is user-visible. Missing B means B is unchanged even
/// when the resulting A/B geometry no longer permits a comparison.
pub fn preview(
    project: &Project,
    requests: &BTreeMap<String, ThermalTransaction>,
) -> Result<TransactionSetPreview> {
    crate::validate_project(project)?;
    ensure!(
        !requests.is_empty() && requests.len() <= 2,
        "Explicitly choose A, B or both designs"
    );
    let energy = project.energy.as_ref().context("Energy document missing")?;
    let mut designs = BTreeMap::new();
    for (id, request) in requests {
        let design = energy.designs.get(id).context("Choose A or B")?;
        designs.insert(
            id.clone(),
            preview_thermal_transaction(
                &design.model,
                &calendar_for(project, id)?,
                request,
                locks(project, id),
            )?,
        );
    }
    Ok(TransactionSetPreview {
        schema_version: "greenlens.energy-transaction-preview.v1".into(),
        preview_sha256: canonical_hash(&(
            project.id.as_str(),
            &energy.designs,
            &energy.workbench,
            requests,
        ))?,
        untouched_designs: energy
            .designs
            .keys()
            .filter(|id| !requests.contains_key(*id))
            .cloned()
            .collect(),
        valid: designs.values().all(|d| d.valid),
        designs,
    })
}
pub fn commit(
    project: &Project,
    requests: &BTreeMap<String, ThermalTransaction>,
    expected_preview_sha256: &str,
) -> Result<Project> {
    let preview = preview(project, requests)?;
    ensure!(
        preview.preview_sha256 == expected_preview_sha256,
        "Inputs, targets or locks changed. Review a new preview before applying."
    );
    ensure!(
        preview.valid,
        "Invalid preview: {}",
        preview
            .designs
            .iter()
            .flat_map(|(id, p)| p.problems.iter().map(move |issue| format!("{id}: {issue}")))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let mut next = project.clone();
    let energy = next.energy.as_mut().context("Energy document missing")?;
    for (id, candidate) in preview.designs {
        energy.designs.get_mut(&id).context("Design removed")?.model = candidate.candidate;
    }
    crate::validate_project(&next)?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_targets_and_locks_do_not_silently_change_other_design() {
        let zone = ZoneBoxInput {
            id: "first".into(),
            name: "First".into(),
            storey: 1,
            origin_m: [0., 0., 0.],
            size_m: [8., 6., 3.],
        };
        let p = new_document(zone, synthetic_energy_calendar()).unwrap();
        let a = &p.energy.as_ref().unwrap().designs["A"];
        let request = ThermalTransaction {
            schema_version: THERMAL_TRANSACTION_SCHEMA.into(),
            expected_model_sha256: validate_thermal_model(
                &a.model,
                &calendar_for(&p, "A").unwrap(),
            )
            .unwrap()
            .model_input_sha256,
            operation: ThermalOperation::AddAdjacent {
                source_zone_id: "first".into(),
                id: "second".into(),
                name: "Second".into(),
                face: ThermalFace::East,
                depth_m: 8.,
            },
            topology: synthetic_topology_assignments(),
        };
        let requests = [("A".into(), request)].into();
        let preview = preview(&p, &requests).unwrap();
        assert_eq!(preview.untouched_designs, vec!["B"]);
        let next = commit(&p, &requests, &preview.preview_sha256).unwrap();
        assert_eq!(
            next.energy.as_ref().unwrap().designs["A"].model.zones.len(),
            2
        );
        assert_eq!(
            serde_json::to_vec(&next.energy.as_ref().unwrap().designs["B"]).unwrap(),
            serde_json::to_vec(&p.energy.as_ref().unwrap().designs["B"]).unwrap()
        );
        let locked = set_locks(&p, "A", vec!["first_East".into()]).unwrap();
        assert!(commit(&locked, &requests, &preview.preview_sha256).is_err());
        assert!(!super::preview(&locked, &requests).unwrap().valid);
        assert_eq!(
            validate_thermal_model(&a.model, &calendar_for(&p, "A").unwrap())
                .unwrap()
                .model_input_sha256,
            validate_thermal_model(
                &locked.energy.as_ref().unwrap().designs["A"].model,
                &calendar_for(&locked, "A").unwrap()
            )
            .unwrap()
            .model_input_sha256
        );
    }
    #[test]
    fn old_project_has_no_new_required_fields() {
        let p = energy_services::example_document();
        assert!(!serde_json::to_string(&p).unwrap().contains("workbench"));
        let bytes = serde_json::to_vec(&p).unwrap();
        let decoded: Project = serde_json::from_slice(&bytes).unwrap();
        crate::validate_project(&decoded).unwrap();
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), bytes);
    }
}
