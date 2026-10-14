//! Explicit accepted-design copying. This pure transaction never copies evidence
//! or starts a worker; the caller records one ordinary document history entry.
use crate::{energy::EnergyDesign, energy_workbench, Project};
use anyhow::{ensure, Context, Result};
use greenlens_core::{canonical_hash, energy::*};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EnergyDesignSide {
    A,
    B,
}
impl EnergyDesignSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnergyDesignCounts {
    pub zones: usize,
    pub surfaces: usize,
    pub windows: usize,
    pub overhangs: usize,
    pub detached_shades: usize,
    pub cooling_systems: usize,
    pub materials: usize,
    pub constructions: usize,
    pub glazing: usize,
    pub schedules: usize,
    pub curves: usize,
}
fn counts(model: &ThermalModel) -> EnergyDesignCounts {
    EnergyDesignCounts {
        zones: model.zones.len(),
        surfaces: model.zones.iter().map(|z| z.surfaces.len()).sum(),
        windows: model.zones.iter().map(|z| z.windows.len()).sum(),
        overhangs: model
            .zones
            .iter()
            .flat_map(|z| &z.windows)
            .filter(|w| w.overhang.is_some())
            .count(),
        detached_shades: model.detached_shading.len(),
        cooling_systems: model
            .zones
            .iter()
            .filter(|z| z.cooling_system.is_some())
            .count(),
        materials: model.materials.len(),
        constructions: model.constructions.len(),
        glazing: model.glazing.len(),
        schedules: model.schedules.len(),
        curves: model.curves.len(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergyDesignCopyPreview {
    pub schema_version: String,
    pub preview_sha256: String,
    pub source: EnergyDesignSide,
    pub target: EnergyDesignSide,
    pub source_design_sha256: String,
    pub target_design_sha256: String,
    pub candidate: EnergyDesign,
    pub source_calendar: EnergyCalendar,
    pub target_calendar: EnergyCalendar,
    pub candidate_calendar: EnergyCalendar,
    pub calendar_changed: bool,
    pub study_changed: bool,
    pub affected_object_ids: Vec<String>,
    pub before: EnergyDesignCounts,
    pub after: EnergyDesignCounts,
    pub valid: bool,
    pub problems: Vec<String>,
    pub effects: Vec<String>,
}

pub fn preview(
    project: &Project,
    source: EnergyDesignSide,
    target: EnergyDesignSide,
) -> Result<EnergyDesignCopyPreview> {
    ensure!(
        source != target,
        "Choose distinct source and destination designs A and B"
    );
    crate::validate_project(project)?;
    let energy = project.energy.as_ref().context("Energy document missing")?;
    let from = energy
        .designs
        .get(source.as_str())
        .context("Source design missing")?;
    let to = energy
        .designs
        .get(target.as_str())
        .context("Destination design missing")?;
    let source_calendar = energy_workbench::calendar_for(project, source.as_str())?;
    let target_calendar = energy_workbench::calendar_for(project, target.as_str())?;
    let mut candidate_project = project.clone();
    candidate_project
        .energy
        .as_mut()
        .unwrap()
        .designs
        .insert(target.as_str().into(), from.clone());
    let candidate_calendar = energy_workbench::calendar_for(&candidate_project, target.as_str())?;
    let source_locks = energy_workbench::locks(project, source.as_str());
    let target_locks = energy_workbench::locks(project, target.as_str());
    let binding = canonical_hash(&(
        "greenlens.energy-design-copy.v1",
        &project.id,
        source,
        target,
        from,
        to,
        &source_calendar,
        &target_calendar,
        &candidate_calendar,
        source_locks,
        target_locks,
    ))?;
    let mut problems = vec![];
    if let Err(error) = validate_thermal_model(&from.model, &source_calendar) {
        problems.push(error.to_string());
    }
    if let Err(error) = enforce_thermal_locks(&to.model, &from.model, target_locks) {
        problems.push(error.to_string());
    }
    if from.study.is_none() && source_calendar != candidate_calendar {
        problems.push("The unconfigured source and destination authoring calendars differ. Explicitly align their calendars or configure the source study before copying; this action does not copy workbench calendar metadata.".into());
    }
    let calendar_changed = candidate_calendar != target_calendar;
    let study_changed = canonical_hash(&from.study)? != canonical_hash(&to.study)?;
    Ok(EnergyDesignCopyPreview {
        schema_version:"greenlens.energy-design-copy-preview.v1".into(), preview_sha256:binding,
        source, target, source_design_sha256:canonical_hash(from)?, target_design_sha256:canonical_hash(to)?,
        candidate:from.clone(), source_calendar, target_calendar, candidate_calendar, calendar_changed, study_changed,
        affected_object_ids:changed_thermal_objects(&to.model, &from.model)?, before:counts(&to.model), after:counts(&from.model),
        valid:problems.is_empty(), problems,
        effects:vec![
            format!("Replace only accepted {} model and study with the exact accepted {} inputs, including site, resources, assignments and schedules.", target.as_str(), source.as_str()),
            "A copied configured study supplies its authoritative calendar. An absent source study clears the destination study and requires the existing destination authoring calendar to match the source. Workbench calendars stay unchanged, so this is one reversible design/study transaction.".into(),
            "Retain source design, both lock lists, all historical runs/weather/attachments/sensitivity receipts, report metadata and display state. Existing results may become historical; no evidence is copied between designs or relabeled.".into(),
        ],
    })
}

pub fn commit(
    project: &Project,
    source: EnergyDesignSide,
    target: EnergyDesignSide,
    expected_preview_sha256: &str,
) -> Result<Project> {
    let reviewed = preview(project, source, target)?;
    ensure!(
        reviewed.preview_sha256 == expected_preview_sha256,
        "Source, destination, calendar or locks changed; review a fresh design-copy preview"
    );
    ensure!(
        reviewed.valid,
        "Design copy is blocked: {}",
        reviewed.problems.join("; ")
    );
    // Reuse the same lock-aware restoration path as ordinary document undo/redo.
    let next = energy_workbench::restore_designs(
        project,
        [(target.as_str().into(), reviewed.candidate)].into(),
    )?;
    crate::validate_project(&next)?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::energy::{EnergyRunSummary, EnergyWeatherReference};
    fn configured() -> Project {
        let mut project = crate::energy_services::example_document();
        let report = synthetic_energy_report().unwrap();
        let energy = project.energy.as_mut().unwrap();
        energy.designs.insert(
            "A".into(),
            EnergyDesign {
                model: report.baseline.model,
                study: Some(report.baseline.study.clone()),
            },
        );
        energy.designs.insert(
            "B".into(),
            EnergyDesign {
                model: report.proposed.model,
                study: Some(report.proposed.study),
            },
        );
        let weather = "00000000-0000-4000-8000-000000000001".to_string();
        energy.weather.push(EnergyWeatherReference {
            path: format!("weather/{weather}"),
            id: weather,
            binding: report.baseline.study.weather,
        });
        // Metadata-only preservation fixtures, never passed to an evidence reader.
        let id = "00000000-0000-4000-8000-000000000002".to_string();
        energy.runs.push(EnergyRunSummary {
            path: format!("runs/{id}"),
            id,
            alternative_id: "B".into(),
            physical_input_hash: "a".repeat(64),
            record_sha256: "b".repeat(64),
        });
        let id = "00000000-0000-4000-8000-000000000003".to_string();
        energy
            .attachments
            .push(crate::energy_attachments::EnergyAttachment {
                path: format!("attachments/{id}/evidence.pdf"),
                id,
                sha256: "c".repeat(64),
                media_type: "application/pdf".into(),
                model_input_sha256: "d".repeat(64),
                caption: "Synthetic metadata only".into(),
                rights: "Original fixture; no file is represented as native evidence".into(),
                include_in_export: false,
            });
        energy
            .workbench
            .get_or_insert_with(Default::default)
            .report_metadata = Some(EnergyReportMetadata::default());
        crate::validate_project(&project).unwrap();
        project
    }
    #[test]
    fn both_directions_copy_inputs_only_and_preserve_all_history() {
        for (source, target) in [
            (EnergyDesignSide::A, EnergyDesignSide::B),
            (EnergyDesignSide::B, EnergyDesignSide::A),
        ] {
            let project = configured();
            let original = canonical_hash(&project).unwrap();
            let p = preview(&project, source, target).unwrap();
            assert!(p.valid && !p.affected_object_ids.is_empty());
            assert_eq!(p.after.zones, 4);
            assert_eq!(p.after.surfaces, 24);
            let next = commit(&project, source, target, &p.preview_sha256).unwrap();
            assert_eq!(
                canonical_hash(&next.energy.as_ref().unwrap().designs[target.as_str()]).unwrap(),
                canonical_hash(&project.energy.as_ref().unwrap().designs[source.as_str()]).unwrap()
            );
            // Restore only the declared destination and compare everything.
            let mut remainder = next.clone();
            remainder.energy.as_mut().unwrap().designs.insert(
                target.as_str().into(),
                project.energy.as_ref().unwrap().designs[target.as_str()].clone(),
            );
            assert_eq!(canonical_hash(&remainder).unwrap(), original);
            assert_eq!(canonical_hash(&project).unwrap(), original);
        }
    }
    #[test]
    fn stale_source_target_study_calendar_and_locks_reject() {
        let project = configured();
        let p = preview(&project, EnergyDesignSide::A, EnergyDesignSide::B).unwrap();
        for mutation in 0..5 {
            let mut changed = project.clone();
            let energy = changed.energy.as_mut().unwrap();
            match mutation {
                0 => {
                    energy.designs.get_mut("A").unwrap().model.zones[0]
                        .cooling_system
                        .as_mut()
                        .unwrap()
                        .supply_fan
                        .total_efficiency = 0.7
                }
                1 => {
                    energy.designs.get_mut("B").unwrap().model.zones[0]
                        .cooling_system
                        .as_mut()
                        .unwrap()
                        .supply_fan
                        .total_efficiency = 0.7
                }
                2 => {
                    energy
                        .designs
                        .get_mut("A")
                        .unwrap()
                        .study
                        .as_mut()
                        .unwrap()
                        .zone_timesteps_per_hour = 12
                }
                3 => {
                    energy
                        .designs
                        .get_mut("A")
                        .unwrap()
                        .study
                        .as_mut()
                        .unwrap()
                        .calendar
                        .start_weekday = Weekday::Wednesday
                }
                _ => {
                    energy
                        .workbench
                        .as_mut()
                        .unwrap()
                        .locks
                        .insert("B".into(), vec!["zone1".into()]);
                }
            }
            assert!(commit(
                &changed,
                EnergyDesignSide::A,
                EnergyDesignSide::B,
                &p.preview_sha256
            )
            .is_err());
        }
        assert!(preview(&project, EnergyDesignSide::A, EnergyDesignSide::A).is_err());
        assert!(serde_json::from_str::<EnergyDesignSide>("\"C\"").is_err());
        assert!(serde_json::from_str::<EnergyDesignSide>("\"a\"").is_err());
    }
    #[test]
    fn destination_dependency_locks_block_but_source_locks_do_not_copy() {
        let project = configured();
        let locked = energy_workbench::set_locks(&project, "B", vec!["zone1".into()]).unwrap();
        let p = preview(&locked, EnergyDesignSide::A, EnergyDesignSide::B).unwrap();
        assert!(!p.valid && p.problems.iter().any(|m| m.contains("locked")));
        assert!(commit(
            &locked,
            EnergyDesignSide::A,
            EnergyDesignSide::B,
            &p.preview_sha256
        )
        .is_err());
        let locked = energy_workbench::set_locks(&project, "A", vec!["zone1".into()]).unwrap();
        let p = preview(&locked, EnergyDesignSide::A, EnergyDesignSide::B).unwrap();
        let next = commit(
            &locked,
            EnergyDesignSide::A,
            EnergyDesignSide::B,
            &p.preview_sha256,
        )
        .unwrap();
        assert_eq!(energy_workbench::locks(&next, "A"), ["zone1"]);
        assert!(energy_workbench::locks(&next, "B").is_empty());
    }
    #[test]
    fn unconfigured_calendar_mismatch_rejects_then_aligned_copy_clears_study_only() {
        let mut project = configured();
        let energy = project.energy.as_mut().unwrap();
        energy.designs.get_mut("A").unwrap().study = None;
        let source_calendar = EnergyCalendar {
            start_weekday: Weekday::Sunday,
            utc_offset_minutes: 480,
            ..synthetic_energy_calendar()
        };
        energy
            .workbench
            .as_mut()
            .unwrap()
            .calendars
            .insert("A".into(), source_calendar.clone());
        let p = preview(&project, EnergyDesignSide::A, EnergyDesignSide::B).unwrap();
        assert!(!p.valid && p.study_changed);
        assert!(commit(
            &project,
            EnergyDesignSide::A,
            EnergyDesignSide::B,
            &p.preview_sha256
        )
        .is_err());
        project
            .energy
            .as_mut()
            .unwrap()
            .workbench
            .as_mut()
            .unwrap()
            .calendars
            .insert("B".into(), source_calendar.clone());
        let p = preview(&project, EnergyDesignSide::A, EnergyDesignSide::B).unwrap();
        assert!(p.valid && p.study_changed && p.calendar_changed);
        let old_workbench = canonical_hash(&project.energy.as_ref().unwrap().workbench).unwrap();
        let next = commit(
            &project,
            EnergyDesignSide::A,
            EnergyDesignSide::B,
            &p.preview_sha256,
        )
        .unwrap();
        assert!(next.energy.as_ref().unwrap().designs["B"].study.is_none());
        assert_eq!(
            energy_workbench::calendar_for(&next, "B").unwrap(),
            source_calendar
        );
        assert_eq!(
            next.energy.as_ref().unwrap().weather,
            project.energy.as_ref().unwrap().weather
        );
        assert_eq!(
            canonical_hash(&next.energy.as_ref().unwrap().workbench).unwrap(),
            old_workbench
        );
    }
    #[test]
    fn configured_source_copies_authoritative_calendar_without_changing_fallback() {
        let mut project = configured();
        project
            .energy
            .as_mut()
            .unwrap()
            .designs
            .get_mut("B")
            .unwrap()
            .study = None;
        let fallback = EnergyCalendar {
            start_weekday: Weekday::Sunday,
            utc_offset_minutes: 480,
            ..synthetic_energy_calendar()
        };
        project
            .energy
            .as_mut()
            .unwrap()
            .workbench
            .as_mut()
            .unwrap()
            .calendars
            .insert("B".into(), fallback.clone());
        let before = canonical_hash(&project.energy.as_ref().unwrap().workbench).unwrap();
        let p = preview(&project, EnergyDesignSide::A, EnergyDesignSide::B).unwrap();
        assert!(p.valid && p.calendar_changed && p.study_changed);
        let next = commit(
            &project,
            EnergyDesignSide::A,
            EnergyDesignSide::B,
            &p.preview_sha256,
        )
        .unwrap();
        assert_eq!(
            energy_workbench::calendar_for(&next, "B").unwrap(),
            energy_workbench::calendar_for(&project, "A").unwrap()
        );
        assert_eq!(
            next.energy
                .as_ref()
                .unwrap()
                .workbench
                .as_ref()
                .unwrap()
                .calendars["B"],
            fallback
        );
        assert_eq!(
            canonical_hash(&next.energy.as_ref().unwrap().workbench).unwrap(),
            before
        );
    }
}
