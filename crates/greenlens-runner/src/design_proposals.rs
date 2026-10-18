//! B-only proposed alternatives. Display selection never determines the target.
use crate::{energy_workbench, Project};
use anyhow::{ensure, Context, Result};
use greenlens_core::{canonical_hash, energy::*};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalDraft {
    pub action: DesignProposalAction,
    pub source: InputSource,
    pub units: String,
    pub rationale: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalReview {
    pub schema_version: String,
    pub target: String,
    pub proposal: DesignProposalPreview,
    pub context_sha256: String,
    pub figures: Vec<DesignFigureProduct>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalReceipt {
    pub schema_version: String,
    pub draft: ProposalDraft,
    pub before_model_sha256: String,
    pub after_model_sha256: String,
    pub reviewed_context_sha256: String,
    pub receipt_sha256: String,
}
fn receipt_hash(r: &ProposalReceipt) -> Result<String> {
    Ok(canonical_hash(&(
        &r.schema_version,
        &r.draft,
        &r.before_model_sha256,
        &r.after_model_sha256,
        &r.reviewed_context_sha256,
    ))?)
}
pub fn validate_receipts(receipts: &[ProposalReceipt]) -> Result<()> {
    ensure!(receipts.len() <= 64, "At most 64 accepted proposal declarations. Preserve this study and begin a separately scoped document; history is not pruned.");
    ensure!(
        serde_json::to_vec(receipts)?.len() <= 512 * 1024,
        "Accepted proposal declarations exceed 512 KiB."
    );
    for r in receipts {
        ensure!(
            r.schema_version == "greenlens.design-proposal-receipt.v1",
            "Unsupported proposal receipt"
        );
        validate_design_proposal_metadata(&r.draft.source, &r.draft.units, &r.draft.rationale)?;
        ensure!(serde_json::to_vec(&r.draft.action)?.len() <= 128 * 1024, "Historical proposal action exceeds its explicit input bound; no geometry revalidation is inferred from this receipt.");
        ensure!(
            r.receipt_sha256 == receipt_hash(r)?,
            "Proposal receipt identity changed"
        );
        for h in [
            &r.before_model_sha256,
            &r.after_model_sha256,
            &r.reviewed_context_sha256,
        ] {
            ensure!(
                h.len() == 64
                    && h.bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
                "Invalid proposal identity"
            );
        }
    }
    Ok(())
}

/// Current accepted geometry has no result attached by this action. It cannot
/// borrow a historical result merely because its design label is A or B.
pub fn current_figures(project: &Project) -> Result<PairedDesignFigures> {
    crate::validate_project(project)?;
    let energy = project.energy.as_ref().context("Energy document missing")?;
    let case = |id: &str| -> Result<DesignFigureCase> {
        let design = energy
            .designs
            .get(id)
            .context("Explicit A and B designs are required")?;
        Ok(DesignFigureCase { design_id: id.into(), label: format!("{id} — accepted geometry, no result attached"), model: design.model.clone(), calendar: energy_workbench::calendar_for(project, id)?, study: design.study.clone(), scope_revision_sha256: None, evidence: DesignFigureEvidence::NotSimulated { reason: "This geometry inspection does not attach simulation evidence. Select exact retained runs for a simulated comparison.".into() } })
    };
    let a = case("A")?;
    let b = case("B")?;
    let frames = default_design_frames(&a.model, &a.calendar, &b.model, &b.calendar)?;
    Ok(paired_design_figures(&PairedDesignFigureRequest { schema_version: DESIGN_FIGURE_SCHEMA.into(), baseline: a, proposed: b, frames, scope: "Supported analytical geometry only; no imported reference equivalence or whole-building coverage is inferred.".into(), north_declaration: None })?)
}

/// Actual selected-run verification belongs to the shared evidence service.
/// This takes no current-model substitute and attaches no arbitrary scope hash.
pub fn retained_figures(
    root: &std::path::Path,
    project: &Project,
    baseline_run_id: &str,
    proposed_run_id: &str,
) -> Result<PairedDesignFigures> {
    let data =
        crate::energy_services::report_data(root, project, baseline_run_id, proposed_run_id)?;
    let a = DesignFigureCase::from_report_case(&data.baseline, None)?;
    let b = DesignFigureCase::from_report_case(&data.proposed, None)?;
    let frames = default_design_frames(&a.model, &a.calendar, &b.model, &b.calendar)?;
    Ok(paired_design_figures(&PairedDesignFigureRequest {
        schema_version: DESIGN_FIGURE_SCHEMA.into(),
        baseline: a,
        proposed: b,
        frames,
        scope: data.scope,
        north_declaration: None,
    })?)
}
pub fn preview(project: &Project, draft: &ProposalDraft) -> Result<ProposalReview> {
    crate::validate_project(project)?;
    let energy = project.energy.as_ref().context("Energy document missing")?;
    let model = &energy
        .designs
        .get("B")
        .context("Create proposed design B first")?
        .model;
    let calendar = energy_workbench::calendar_for(project, "B")?;
    let request = DesignProposalRequest {
        schema_version: DESIGN_PROPOSAL_SCHEMA.into(),
        expected_model_sha256: validate_thermal_model(model, &calendar)?.model_input_sha256,
        action: draft.action.clone(),
        source: draft.source.clone(),
        units: draft.units.clone(),
        rationale: draft.rationale.clone(),
    };
    let proposal = preview_design_proposal(
        model,
        &calendar,
        &request,
        energy_workbench::locks(project, "B"),
    )?;
    let context_sha256 = canonical_hash(&(
        "greenlens.design-proposal-context.v1",
        &project.id,
        &energy.designs,
        &energy.workbench,
        &proposal.preview_sha256,
    ))?;
    let figures = if proposal.valid {
        let a = &energy
            .designs
            .get("A")
            .context("Baseline A is required for a paired preview")?
            .model;
        let ca = energy_workbench::calendar_for(project, "A")?;
        let case = |id: &str, model: &ThermalModel, calendar: &EnergyCalendar| {
            DesignFigureCase { design_id: id.into(), label: format!("{id} — proposal preview, no result attached"), model: model.clone(), calendar: calendar.clone(), study: None, scope_revision_sha256: None, evidence: DesignFigureEvidence::NotSimulated { reason: "No simulation evidence is attached to this provisional preview. Select exact matching retained runs after Apply.".into() } }
        };
        paired_design_figures(&PairedDesignFigureRequest {
            schema_version: DESIGN_FIGURE_SCHEMA.into(),
            baseline: case("A", a, &ca),
            proposed: case("B", &proposal.candidate, &calendar),
            frames: default_design_frames(a, &ca, &proposal.candidate, &calendar)?,
            scope:
                "Supported analytical geometry only; no imported reference equivalence is inferred."
                    .into(),
            north_declaration: None,
        })?
        .products
    } else {
        vec![]
    };
    Ok(ProposalReview {
        schema_version: "greenlens.design-proposal-review.v1".into(),
        target: "B".into(),
        proposal,
        context_sha256,
        figures,
    })
}
pub fn commit(
    project: &Project,
    draft: &ProposalDraft,
    expected_context_sha256: &str,
) -> Result<Project> {
    let review = preview(project, draft)?;
    ensure!(review.context_sha256 == expected_context_sha256, "Designs, calendar, locks or proposal changed after preview. Refresh and review before applying.");
    ensure!(
        review.proposal.valid,
        "Proposal rejected: {}",
        review.proposal.problems.join("; ")
    );
    let mut next = project.clone();
    let mut receipt = ProposalReceipt {
        schema_version: "greenlens.design-proposal-receipt.v1".into(),
        draft: draft.clone(),
        before_model_sha256: review.proposal.base_model_sha256,
        after_model_sha256: review
            .proposal
            .candidate_model_sha256
            .context("Valid candidate hash missing")?,
        reviewed_context_sha256: review.context_sha256,
        receipt_sha256: String::new(),
    };
    receipt.receipt_sha256 = receipt_hash(&receipt)?;
    let energy = next.energy.as_mut().unwrap();
    energy.designs.get_mut("B").unwrap().model = review.proposal.candidate;
    energy
        .workbench
        .get_or_insert_with(Default::default)
        .design_proposals
        .push(receipt);
    crate::validate_project(&next)?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn project() -> Project {
        crate::energy_services::example_document()
    }
    fn draft(p: &Project) -> ProposalDraft {
        let zone = &p.energy.as_ref().unwrap().designs["B"].model.zones[0];
        ProposalDraft {
            action: DesignProposalAction::SetOverhang {
                zone_id: zone.id.clone(),
                window_id: zone.windows[0].id.clone(),
                overhang: Some(ThermalOverhang {
                    projection_m: 0.9,
                    side_extension_m: 0.1,
                    gap_above_m: 0.1,
                }),
            },
            source: InputSource {
                citation: "Original synthetic hypothesis".into(),
                synthetic: true,
            },
            units: "m".into(),
            rationale: "Test a fixed shade; no guaranteed savings".into(),
        }
    }
    #[test]
    fn b_only_commit_preserves_a_evidence_and_rejects_stale_context() {
        let p = project();
        let d = draft(&p);
        let r = preview(&p, &d).unwrap();
        let n = commit(&p, &d, &r.context_sha256).unwrap();
        let a = p.energy.as_ref().unwrap();
        let b = n.energy.as_ref().unwrap();
        assert_eq!(
            canonical_hash(&a.designs["A"]).unwrap(),
            canonical_hash(&b.designs["A"]).unwrap()
        );
        assert_eq!(
            canonical_hash(&a.runs).unwrap(),
            canonical_hash(&b.runs).unwrap()
        );
        assert_ne!(
            canonical_hash(&a.designs["B"]).unwrap(),
            canonical_hash(&b.designs["B"]).unwrap()
        );
        assert!(commit(&n, &d, &r.context_sha256).is_err());
        let mut changed = d.clone();
        changed.rationale.push('!');
        assert!(commit(&p, &changed, &r.context_sha256).is_err());
    }

    #[test]
    fn current_figures_bind_accepted_geometry_without_inferring_simulation() {
        // This is an original geometry-only document, not an accepted engine
        // fixture. Neither a familiar A/B label nor an accepted edit is a run.
        let p = project();
        let before = canonical_hash(&p).unwrap();
        let initial = current_figures(&p).unwrap();
        let d = draft(&p);
        let reviewed = preview(&p, &d).unwrap();
        let changed = commit(&p, &d, &reviewed.context_sha256).unwrap();
        let figures = current_figures(&changed).unwrap();
        assert_eq!(before, canonical_hash(&p).unwrap());
        assert_eq!(initial.baseline_model_sha256, figures.baseline_model_sha256);
        assert_ne!(initial.proposed_model_sha256, figures.proposed_model_sha256);
        assert_eq!(
            Some(figures.proposed_model_sha256.clone()),
            reviewed.proposal.candidate_model_sha256
        );
        for case in [&figures.request.baseline, &figures.request.proposed] {
            assert!(matches!(
                case.evidence,
                DesignFigureEvidence::NotSimulated { .. }
            ));
            assert!(case.scope_revision_sha256.is_none());
        }
        assert_eq!(figures.products.len(), 6);
        assert!(figures.products.iter().all(|product| {
            product.svg.contains("NOT SIMULATED") && !product.svg.contains("Retained run")
        }));
        assert!(changed.energy.as_ref().unwrap().runs.is_empty());
    }

    #[test]
    fn resealed_receipts_still_require_bounded_explicit_metadata() {
        let p = project();
        let d = draft(&p);
        let reviewed = preview(&p, &d).unwrap();
        let accepted = commit(&p, &d, &reviewed.context_sha256).unwrap();
        let original_hash = canonical_hash(&accepted).unwrap();
        let receipt = accepted
            .energy
            .as_ref()
            .unwrap()
            .workbench
            .as_ref()
            .unwrap()
            .design_proposals[0]
            .clone();
        validate_receipts(std::slice::from_ref(&receipt)).unwrap();

        // A valid checksum is an integrity binding, not semantic admission.
        // Test actual UTF-8 bytes at the limit before crossing it.
        let mut bounded = receipt.clone();
        bounded.draft.rationale = format!("{}é", "界".repeat(682));
        assert_eq!(bounded.draft.rationale.len(), 2048);
        bounded.receipt_sha256 = receipt_hash(&bounded).unwrap();
        validate_receipts(std::slice::from_ref(&bounded)).unwrap();
        bounded.draft.rationale.push('界');
        bounded.receipt_sha256 = receipt_hash(&bounded).unwrap();
        assert!(validate_receipts(&[bounded])
            .unwrap_err()
            .to_string()
            .contains("2,048"));

        for field in ["source", "units", "rationale"] {
            let mut invalid = receipt.clone();
            match field {
                "source" => invalid.draft.source.citation = " ".into(),
                "units" => invalid.draft.units = " ".into(),
                _ => invalid.draft.rationale = " ".into(),
            }
            invalid.receipt_sha256 = receipt_hash(&invalid).unwrap();
            assert!(
                validate_receipts(&[invalid]).is_err(),
                "{field} must stay explicit"
            );
        }
        let mut oversized_action = receipt.clone();
        oversized_action.draft.action = DesignProposalAction::RemoveDetachedShade {
            shade_id: "x".repeat(128 * 1024),
        };
        oversized_action.receipt_sha256 = receipt_hash(&oversized_action).unwrap();
        assert!(validate_receipts(&[oversized_action])
            .unwrap_err()
            .to_string()
            .contains("explicit input bound"));
        assert_eq!(original_hash, canonical_hash(&accepted).unwrap());
    }
}
