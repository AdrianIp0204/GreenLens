//! Explicit bounded design hypotheses. Preview is not an accepted input or a
//! prediction of savings. The application targets B and records one transaction.
use super::*;
use crate::{canonical_hash, Result};
use serde::{Deserialize, Serialize};

pub const DESIGN_PROPOSAL_SCHEMA: &str = "greenlens.design-proposal.v1";
pub const DESIGN_PROPOSAL_PREVIEW_SCHEMA: &str = "greenlens.design-proposal-preview.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DesignProposalAction {
    PutWindow {
        zone_id: String,
        window: ThermalWindow,
    },
    RemoveWindow {
        zone_id: String,
        window_id: String,
    },
    SetOverhang {
        zone_id: String,
        window_id: String,
        overhang: Option<ThermalOverhang>,
    },
    PutDetachedShade {
        shade: ThermalBoxShade,
    },
    RemoveDetachedShade {
        shade_id: String,
    },
    /// A new explicit glazing ID is assigned only to this window. Existing
    /// library entries may be reused unchanged, but never overwritten silently.
    AssignWindowGlazing {
        zone_id: String,
        window_id: String,
        glazing: ThermalGlazing,
    },
    AssignConstruction {
        zone_id: String,
        surface_id: String,
        construction_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DesignProposalRequest {
    pub schema_version: String,
    pub expected_model_sha256: String,
    pub action: DesignProposalAction,
    pub source: InputSource,
    /// Human description of the actual input units, never a conversion rule.
    pub units: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DesignProposalPreview {
    pub schema_version: String,
    pub request: DesignProposalRequest,
    pub base_model_sha256: String,
    pub candidate_model_sha256: Option<String>,
    pub candidate: ThermalModel,
    pub affected_object_ids: Vec<String>,
    pub tradeoffs: Vec<String>,
    pub problems: Vec<String>,
    pub valid: bool,
    pub preview_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DesignHypothesis {
    pub id: String,
    pub title: String,
    pub applicable_objects: Vec<String>,
    pub rationale: String,
    pub tradeoffs: Vec<String>,
}

pub fn design_hypotheses() -> Vec<DesignHypothesis> {
    vec![
        DesignHypothesis { id: "external_shading".into(), title: "Test external shading".into(), applicable_objects: vec!["window".into(), "shading".into()], rationale: "Compare an explicit fixed shade while retaining service and weather inputs.".into(), tradeoffs: vec!["Shading can reduce solar gains but may change useful daylight; this energy study does not quantify daylight service.".into(), "Cooling and peak demand may improve, stay equal or worsen; only matching simulations establish this case's outcomes.".into()] },
        DesignHypothesis { id: "fenestration".into(), title: "Test a window configuration".into(), applicable_objects: vec!["window".into(), "surface".into()], rationale: "Compare a declared rectangular opening within the supported exterior face.".into(), tradeoffs: vec!["Window area changes conduction and solar gains; equal lighting power does not demonstrate equivalent illumination.".into(), "No natural-ventilation, view-quality or certification benefit is inferred.".into()] },
        DesignHypothesis { id: "envelope".into(), title: "Test explicit envelope properties".into(), applicable_objects: vec!["window".into(), "surface".into()], rationale: "Use declared glazing values or an existing layered construction assignment.".into(), tradeoffs: vec!["Thermal and solar performance require sources or explicitly synthetic assumptions; appearance supplies neither.".into(), "Lower U-value or SHGC does not guarantee lower annual electricity in every climate or operating condition.".into()] },
    ]
}

fn zone_mut<'a>(model: &'a mut ThermalModel, id: &str) -> Result<&'a mut ThermalZone> {
    model
        .zones
        .iter_mut()
        .find(|z| z.id == id)
        .ok_or_else(|| invalid("proposal.zone", "Choose an existing analytical zone"))
}
fn window_mut<'a>(
    model: &'a mut ThermalModel,
    zone: &str,
    id: &str,
) -> Result<&'a mut ThermalWindow> {
    zone_mut(model, zone)?
        .windows
        .iter_mut()
        .find(|w| w.id == id)
        .ok_or_else(|| invalid("proposal.window", "Choose an existing analytical window"))
}
fn changed_model(model: &ThermalModel, action: &DesignProposalAction) -> Result<ThermalModel> {
    let mut next = model.clone();
    match action {
        DesignProposalAction::PutWindow { zone_id, window } => {
            let windows = &mut zone_mut(&mut next, zone_id)?.windows;
            if let Some(existing) = windows.iter_mut().find(|w| w.id == window.id) {
                *existing = window.clone();
            } else {
                windows.push(window.clone());
            }
        }
        DesignProposalAction::RemoveWindow { zone_id, window_id } => {
            window_mut(&mut next, zone_id, window_id)?;
            zone_mut(&mut next, zone_id)?
                .windows
                .retain(|w| &w.id != window_id);
        }
        DesignProposalAction::SetOverhang {
            zone_id,
            window_id,
            overhang,
        } => window_mut(&mut next, zone_id, window_id)?.overhang = overhang.clone(),
        DesignProposalAction::PutDetachedShade { shade } => {
            if let Some(existing) = next.detached_shading.iter_mut().find(|s| s.id == shade.id) {
                *existing = shade.clone();
            } else {
                next.detached_shading.push(shade.clone());
            }
        }
        DesignProposalAction::RemoveDetachedShade { shade_id } => {
            if !next.detached_shading.iter().any(|s| &s.id == shade_id) {
                return Err(invalid(
                    "proposal.shade",
                    "Choose an existing detached shade",
                ));
            }
            next.detached_shading.retain(|s| &s.id != shade_id);
        }
        DesignProposalAction::AssignWindowGlazing {
            zone_id,
            window_id,
            glazing,
        } => {
            window_mut(&mut next, zone_id, window_id)?;
            if let Some(existing) = next.glazing.iter().find(|g| g.id == glazing.id) {
                if existing != glazing {
                    return Err(invalid("proposal.glazing", "This glazing ID already exists with different values. Choose a new ID to avoid silently changing other windows."));
                }
            } else {
                next.glazing.push(glazing.clone());
            }
            window_mut(&mut next, zone_id, window_id)?.glazing_id = glazing.id.clone();
        }
        DesignProposalAction::AssignConstruction {
            zone_id,
            surface_id,
            construction_id,
        } => {
            if !next.constructions.iter().any(|c| &c.id == construction_id) {
                return Err(invalid(
                    "proposal.construction",
                    "Choose an existing explicit layered construction",
                ));
            }
            zone_mut(&mut next, zone_id)?
                .surfaces
                .iter_mut()
                .find(|s| &s.id == surface_id)
                .ok_or_else(|| {
                    invalid("proposal.surface", "Choose an existing analytical surface")
                })?
                .construction_id = construction_id.clone();
        }
    }
    Ok(next)
}

pub fn preview_design_proposal(
    model: &ThermalModel,
    calendar: &EnergyCalendar,
    request: &DesignProposalRequest,
    locked_ids: &[String],
) -> Result<DesignProposalPreview> {
    if request.schema_version != DESIGN_PROPOSAL_SCHEMA {
        return Err(invalid(
            "proposal.schema",
            "Unsupported design proposal contract",
        ));
    }
    validate_design_proposal_metadata(&request.source, &request.units, &request.rationale)?;
    let base = validate_thermal_model(model, calendar)?.model_input_sha256;
    if base != request.expected_model_sha256 {
        return Err(invalid(
            "proposal.base",
            "Accepted inputs changed; refresh and review the proposal before applying",
        ));
    }
    let candidate = changed_model(model, &request.action)?;
    let mut problems = vec![];
    let candidate_model_sha256 = match validate_thermal_model(&candidate, calendar) {
        Ok(v) => Some(v.model_input_sha256),
        Err(e) => {
            problems.push(e.to_string());
            None
        }
    };
    if let Err(e) = enforce_thermal_locks(model, &candidate, locked_ids) {
        problems.push(e.to_string());
    }
    let hypothesis = match &request.action {
        DesignProposalAction::SetOverhang { .. }
        | DesignProposalAction::PutDetachedShade { .. }
        | DesignProposalAction::RemoveDetachedShade { .. } => 0,
        DesignProposalAction::PutWindow { .. } | DesignProposalAction::RemoveWindow { .. } => 1,
        _ => 2,
    };
    let mut tradeoffs = design_hypotheses()[hypothesis].tradeoffs.clone();
    tradeoffs.push("Preview is not simulated. The application applies only the explicitly selected proposal to B; A and imported source geometry remain unchanged.".into());
    let mut preview = DesignProposalPreview {
        schema_version: DESIGN_PROPOSAL_PREVIEW_SCHEMA.into(),
        request: request.clone(),
        base_model_sha256: base,
        candidate_model_sha256,
        affected_object_ids: changed_thermal_objects(model, &candidate)?,
        candidate,
        tradeoffs,
        valid: problems.is_empty(),
        problems,
        preview_sha256: String::new(),
    };
    preview.preview_sha256 = canonical_hash(&(
        DESIGN_PROPOSAL_PREVIEW_SCHEMA,
        calendar,
        locked_ids,
        &preview,
    ))?;
    Ok(preview)
}

/// Bounded declaration admission is also used when reading historical proposal
/// receipts. It does not claim that an absent historical model was revalidated.
pub fn validate_design_proposal_metadata(
    source: &InputSource,
    units: &str,
    rationale: &str,
) -> Result<()> {
    for (field, value) in [
        ("proposal.source", source.citation.as_str()),
        ("proposal.units", units),
        ("proposal.rationale", rationale),
    ] {
        text(field, value)?;
        if value.len() > 2048 {
            return Err(invalid(field, "At most 2,048 UTF-8 bytes"));
        }
    }
    Ok(())
}

pub fn commit_design_proposal(
    model: &ThermalModel,
    calendar: &EnergyCalendar,
    request: &DesignProposalRequest,
    locked_ids: &[String],
    reviewed_preview_sha256: &str,
) -> Result<ThermalModel> {
    let preview = preview_design_proposal(model, calendar, request, locked_ids)?;
    if preview.preview_sha256 != reviewed_preview_sha256 {
        return Err(invalid("proposal.review", "Proposal, accepted inputs, calendar or locks changed after review; refresh before applying"));
    }
    if !preview.valid {
        return Err(invalid("proposal", preview.problems.join("\n")));
    }
    Ok(preview.candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(model: &ThermalModel, action: DesignProposalAction) -> DesignProposalRequest {
        DesignProposalRequest {
            schema_version: DESIGN_PROPOSAL_SCHEMA.into(),
            expected_model_sha256: validate_thermal_model(model, &synthetic_energy_calendar())
                .unwrap()
                .model_input_sha256,
            action,
            source: InputSource {
                citation: "Original synthetic test assumption".into(),
                synthetic: true,
            },
            units: "metres".into(),
            rationale: "Explicit test alternative; no claimed improvement".into(),
        }
    }
    #[test]
    fn preview_commit_keeps_original_and_rejects_changed_review() {
        let a = synthetic_four_zone_model(false);
        let b = a.clone();
        let before = canonical_hash(&a).unwrap();
        let cal = synthetic_energy_calendar();
        let req = request(
            &b,
            DesignProposalAction::SetOverhang {
                zone_id: b.zones[0].id.clone(),
                window_id: b.zones[0].windows[0].id.clone(),
                overhang: Some(ThermalOverhang {
                    projection_m: 0.8,
                    side_extension_m: 0.1,
                    gap_above_m: 0.1,
                }),
            },
        );
        let p = preview_design_proposal(&b, &cal, &req, &[]).unwrap();
        assert!(p.valid);
        assert_ne!(p.candidate_model_sha256.as_ref().unwrap(), &before);
        let changed = commit_design_proposal(&b, &cal, &req, &[], &p.preview_sha256).unwrap();
        assert_eq!(canonical_hash(&a).unwrap(), before);
        assert_eq!(b, a);
        assert_ne!(changed, b);
        let mut edit = req.clone();
        edit.rationale.push('!');
        assert!(commit_design_proposal(&b, &cal, &edit, &[], &p.preview_sha256).is_err());
        assert!(preview_design_proposal(&changed, &cal, &req, &[]).is_err());
    }
    #[test]
    fn invalid_geometry_and_locks_keep_candidate_without_acceptance() {
        let m = synthetic_four_zone_model(false);
        let cal = synthetic_energy_calendar();
        let mut w = m.zones[0].windows[0].clone();
        w.width_m = 100.;
        let req = request(
            &m,
            DesignProposalAction::PutWindow {
                zone_id: m.zones[0].id.clone(),
                window: w,
            },
        );
        let p = preview_design_proposal(&m, &cal, &req, &[]).unwrap();
        assert!(!p.valid);
        assert!(commit_design_proposal(&m, &cal, &req, &[], &p.preview_sha256).is_err());
        let req = request(
            &m,
            DesignProposalAction::RemoveWindow {
                zone_id: m.zones[0].id.clone(),
                window_id: m.zones[0].windows[0].id.clone(),
            },
        );
        let p =
            preview_design_proposal(&m, &cal, &req, &[m.zones[0].windows[0].surface_id.clone()])
                .unwrap();
        assert!(!p.valid);
        assert!(p.problems.iter().any(|p| p.contains("locked")));
    }
    #[test]
    fn new_glazing_assigns_one_window_and_cannot_overwrite_shared_entry() {
        let m = synthetic_four_zone_model(false);
        let cal = synthetic_energy_calendar();
        let mut g = m.glazing[0].clone();
        g.shgc = 0.3;
        let action = |glazing| DesignProposalAction::AssignWindowGlazing {
            zone_id: m.zones[0].id.clone(),
            window_id: m.zones[0].windows[0].id.clone(),
            glazing,
        };
        assert!(preview_design_proposal(&m, &cal, &request(&m, action(g.clone())), &[]).is_err());
        g.id = "explicit_new_glazing".into();
        let p = preview_design_proposal(&m, &cal, &request(&m, action(g)), &[]).unwrap();
        assert!(p.valid);
        assert_eq!(p.candidate.glazing[0], m.glazing[0]);
        assert_eq!(p.candidate.zones[1], m.zones[1]);
        assert_eq!(
            p.candidate.zones[0].windows[0].glazing_id,
            "explicit_new_glazing"
        );
    }
    #[test]
    fn utf8_sources_and_unknown_targets_are_bounded() {
        let m = synthetic_four_zone_model(false);
        let mut req = request(
            &m,
            DesignProposalAction::RemoveDetachedShade {
                shade_id: "missing".into(),
            },
        );
        assert!(preview_design_proposal(&m, &synthetic_energy_calendar(), &req, &[]).is_err());
        req.rationale = "界".repeat(683);
        assert!(
            preview_design_proposal(&m, &synthetic_energy_calendar(), &req, &[])
                .unwrap_err()
                .to_string()
                .contains("2,048")
        );
    }
}
