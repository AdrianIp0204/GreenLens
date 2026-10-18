//! Shared analytical report issuance. Rendering only consumes verified retained
//! evidence; complete products survive publication conflicts in owned staging.
//! Recovery never renders or launches a simulation.
use crate::{
    design_report::{
        self as report, DesignReportContent, DesignReportInspection, DesignReportSummary,
    },
    energy_report::{architectural::SensitivityReportEvidence, TypstConfig},
    persistence::{self, WriterLock},
    Project, ProjectView,
};
use anyhow::{ensure, Context, Result};
use greenlens_core::energy::{default_design_frames, EnergyReportWorkbenchData};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use uuid::Uuid;

const STAGE_SCHEMA: &str = "greenlens.staged-design-report.v1";
const STAGE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignReportResult {
    pub view: ProjectView,
    pub report: DesignReportInspection,
}

/// Only returned after a complete product inventory was verified. The token
/// names owned local staging, never a caller-supplied executable or file path.
#[derive(Debug)]
pub struct PendingDesignReportPublication {
    pub recovery_token: String,
    pub reason: String,
}
impl std::fmt::Display for PendingDesignReportPublication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Complete design report products remain in local recovery staging {}. Reload/reconcile the document and recover this token without rendering again. No accepted history was overwritten. {}", self.recovery_token, self.reason)
    }
}
impl std::error::Error for PendingDesignReportPublication {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StagedReport {
    schema_version: String,
    project_id: String,
    /// The original disk fence is a local diagnostic. Explicit recovery uses
    /// the caller's newly reviewed exact revision, never this stale value.
    expected_revision: String,
    content_sha256: String,
    baseline_run_id: String,
    proposed_run_id: String,
}
fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn checkpoint(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Design report operation cancelled; no partial report is accepted"
    );
    crate::project_open::checkpoint(0)
}
fn stage_root(token: &str) -> Result<PathBuf> {
    ensure!(
        Uuid::parse_str(token)?.to_string() == token,
        "Invalid local design-report recovery token"
    );
    Ok(crate::local_root()?
        .join("design-report-staging")
        .join(token))
}
fn write_owned(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    crate::energy_report::write_new(path, &bytes)
}
fn pending(token: &str, error: anyhow::Error) -> anyhow::Error {
    PendingDesignReportPublication {
        recovery_token: token.into(),
        reason: format!("{error:#}"),
    }
    .into()
}
fn retained<'a>(project: &'a Project, content_sha256: &str) -> Option<&'a DesignReportSummary> {
    project
        .design_reports
        .as_ref()?
        .reports
        .iter()
        .find(|r| r.content_sha256 == content_sha256)
}
fn validate_staging(staged: &StagedReport, project_id: &str) -> Result<()> {
    ensure!(
        staged.schema_version == STAGE_SCHEMA && staged.project_id == project_id,
        "Staged design report belongs to another document or unsupported recovery contract"
    );
    ensure!(
        digest(&staged.expected_revision) && digest(&staged.content_sha256),
        "Invalid staged report identity"
    );
    for id in [&staged.baseline_run_id, &staged.proposed_run_id] {
        ensure!(
            Uuid::parse_str(id)?.to_string() == *id,
            "Invalid staged run identity"
        );
    }
    ensure!(
        staged.baseline_run_id != staged.proposed_run_id,
        "Staged report requires distinct genuine A/B records"
    );
    Ok(())
}

/// Add an explicit selection disclosure to the new derived content. No proposal
/// declaration is rebound to another input just because it was most recent.
fn proposal_declarations(
    project: &Project,
    data: &mut EnergyReportWorkbenchData,
) -> Result<Vec<crate::design_proposals::ProposalReceipt>> {
    let all = project
        .energy
        .as_ref()
        .and_then(|e| e.workbench.as_ref())
        .map(|w| w.design_proposals.as_slice())
        .unwrap_or(&[]);
    crate::design_proposals::validate_receipts(all)?;
    let included: Vec<_> = all
        .iter()
        .filter(|p| p.after_model_sha256 == data.base.proposed.model_input_sha256)
        .cloned()
        .collect();
    data.base.limitations.push(format!(
        "Proposal declaration selection: {} included of {} retained; {} excluded because their accepted after-model identity differs from the selected proposed run. Excluded declarations remain in the complete project. Included declarations describe reviewed input actions, not proof of savings or a causal explanation of every A/B difference.",
        included.len(), all.len(), all.len() - included.len()
    ));
    // This is new derived report content, not a mutation of a retained run.
    // Recompute the canonical presentation identities through the constructor;
    // the old dataset/derived hashes cannot be reused after adding disclosure.
    *data = EnergyReportWorkbenchData::new(
        data.base.clone(),
        data.metadata.clone(),
        data.profiles.clone(),
        data.attachments.clone(),
        data.template_sha256.clone(),
    )?;
    Ok(included)
}

#[allow(clippy::too_many_arguments)]
pub fn prepare(
    root: &Path,
    project: &Project,
    a: &str,
    b: &str,
    scope: String,
    north: Option<String>,
    include_pdf: bool,
    cancel: Arc<AtomicBool>,
) -> Result<DesignReportContent> {
    crate::project_open::inspect_evidence(
        root,
        cancel.clone(),
        |_| {},
        || {
            checkpoint(&cancel)?;
            crate::validate_project(project)?;
            ensure!(
                a != b,
                "Select distinct genuine baseline A and proposed B runs"
            );
            let mut data = crate::energy_services::workbench_report_data(root, project, a, b)?;
            let selection =
                crate::energy_sensitivity_workflow::export_selection(root, project, a, b)?;
            let sensitivity = selection
                .included_session_id
                .map(|id| -> Result<_> {
                    checkpoint(&cancel)?;
                    let session = crate::energy_sensitivity_workflow::inspect(root, project, &id)?;
                    Ok(SensitivityReportEvidence {
                        session_id: id,
                        plan: session.plan,
                        result: session
                            .result
                            .context("Exact-pair sensitivity is not finalized")?,
                        receipt_reference: session.reference,
                    })
                })
                .transpose()?;
            let declarations = proposal_declarations(project, &mut data)?;
            let frames = default_design_frames(
                &data.base.baseline.model,
                &data.base.baseline.study.calendar,
                &data.base.proposed.model,
                &data.base.proposed.study.calendar,
            )?;
            checkpoint(&cancel)?;
            report::with_proposals(
                report::prepare(data, sensitivity, frames, scope, north, include_pdf)?,
                declarations,
            )
        },
    )
}

#[allow(clippy::too_many_arguments)]
pub fn create(
    root: &Path,
    project: &Project,
    expected_revision: Option<&str>,
    a: &str,
    b: &str,
    scope: String,
    north: Option<String>,
    include_pdf: bool,
    typst: Option<&TypstConfig>,
    cancel: Arc<AtomicBool>,
) -> Result<DesignReportResult> {
    checkpoint(&cancel)?;
    let expected =
        expected_revision.context("Save the complete document before issuing a design report")?;
    ensure!(digest(expected), "Invalid expected saved-document revision");
    ensure!(
        !include_pdf || typst.is_some(),
        "PDF issuance requires the pinned offline Typst/font installation"
    );
    persistence::check_revision(root, Some(expected))?;
    let content = prepare(
        root,
        project,
        a,
        b,
        scope,
        north,
        include_pdf,
        cancel.clone(),
    )?;
    if let Some(existing) = retained(project, &content.content_sha256) {
        let inspected = report::inspect(root, project, &existing.id, &cancel)?;
        let _lock = WriterLock::acquire(root)?;
        persistence::check_revision(root, Some(expected))?;
        let mut view = crate::project_open::verify_import_candidate(
            root,
            project,
            Some(expected),
            cancel.clone(),
            |_| {},
        )?;
        checkpoint(&cancel)?;
        persistence::check_revision(root, Some(expected))?;
        view.revision = persistence::atomic_project_cancellable(root, project, &cancel)?;
        return Ok(DesignReportResult {
            view,
            report: inspected,
        });
    }
    crate::capacity::require_start(
        root,
        project,
        &crate::capacity::Operation::DesignReport,
        &cancel,
    )?;
    let token = Uuid::new_v4().to_string();
    let stage = stage_root(&token)?;
    fs::create_dir_all(stage.parent().context("Owned staging parent")?)?;
    fs::create_dir(&stage)?;
    let _guard = persistence::DirectoryGuard::open(&stage)?;
    // Write the exact request before rendering. Its presence alone never makes
    // a partial product tree publishable; recovery independently admits all bytes.
    write_owned(
        &stage.join("staged-design-report.json"),
        &StagedReport {
            schema_version: STAGE_SCHEMA.into(),
            project_id: project.id.clone(),
            expected_revision: expected.into(),
            content_sha256: content.content_sha256.clone(),
            baseline_run_id: a.into(),
            proposed_run_id: b.into(),
        },
    )?;
    let rendered = report::render(&content, &stage.join("products"), if include_pdf { typst } else { None }, &cancel)
        .with_context(|| format!("Design rendering stopped in diagnostic staging {token}; no report was published and no automatic rerender is permitted"))?;
    write_owned(
        &stage.join("local-renderer-receipt.json"),
        &rendered.runtime,
    )
    .map_err(|error| pending(&token, error))?;
    // The runtime receipt stays external. Portable immutable content contains
    // the pinned renderer identity and exact deterministic product hashes.
    recover(root, project, Some(expected), &token, cancel).map_err(|error| {
        if error
            .downcast_ref::<PendingDesignReportPublication>()
            .is_some()
        {
            error
        } else {
            // Rendering and its complete inventory already succeeded. Even a
            // cancellation at the first recovery checkpoint must expose this
            // complete pending result instead of losing the recovery token.
            pending(&token, error)
        }
    })
}

/// Explicit publication recovery uses the caller's exact current disk revision.
/// No original receipt, issued artifact or existing project member is rewritten.
pub fn recover(
    root: &Path,
    project: &Project,
    expected_revision: Option<&str>,
    token: &str,
    cancel: Arc<AtomicBool>,
) -> Result<DesignReportResult> {
    let stage = stage_root(token)?;
    let _stage_guard = persistence::DirectoryGuard::open(&stage)?;
    let staged: StagedReport = serde_json::from_slice(&persistence::read_evidence_bytes_checked(
        &persistence::regular_child(&stage, "staged-design-report.json")?,
        STAGE_BYTES,
        &mut |n| {
            checkpoint(&cancel)?;
            crate::project_open::checkpoint(n)
        },
    )?)?;
    validate_staging(&staged, &project.id)?;
    let source = stage.join("products");
    let inspected = report::inspect_directory(&source, &cancel)?;
    let summary = report::summary(&inspected);
    ensure!(summary.content_sha256 == staged.content_sha256
        && summary.baseline.run_id == staged.baseline_run_id
        && summary.proposed.run_id == staged.proposed_run_id,
        "Completed products differ from the exact staged request; no partial or substituted report is accepted");
    publish(
        root,
        project,
        expected_revision,
        &source,
        &summary,
        inspected,
        cancel,
    )
    .map_err(|error| pending(token, error))
}

fn publish(
    root: &Path,
    project: &Project,
    expected_revision: Option<&str>,
    source: &Path,
    summary: &DesignReportSummary,
    inspected: DesignReportInspection,
    cancel: Arc<AtomicBool>,
) -> Result<DesignReportResult> {
    checkpoint(&cancel)?;
    let expected =
        expected_revision.context("Reload the saved document before recovering a report")?;
    ensure!(digest(expected), "Invalid recovery revision");
    let candidate = if let Some(existing) = retained(project, &summary.content_sha256) {
        ensure!(
            existing == summary,
            "Existing design report differs; immutable products will not be overwritten"
        );
        project.clone()
    } else {
        report::append(project, summary)?
    };
    crate::validate_project(&candidate)?;
    let _lock = WriterLock::acquire(root)?;
    persistence::check_revision(root, Some(expected))?;
    crate::capacity::require_publication(
        root,
        &candidate,
        &[(summary.path.clone(), source.to_owned())],
        &cancel,
    )?;
    let mut members: persistence::BoundReferenceMembers = inspected
        .products
        .products
        .iter()
        .map(|p| (p.path.clone(), (p.sha256.clone(), p.bytes)))
        .collect();
    let manifest = persistence::regular_child(source, "design-products.json")?;
    members.insert(
        "design-products.json".into(),
        (
            inspected.manifest_file_sha256.clone(),
            fs::metadata(manifest)?.len(),
        ),
    );
    persistence::promote_design_report_bound(source, root, &summary.id, &members, &mut |n| {
        checkpoint(&cancel)?;
        crate::project_open::checkpoint(n)
    })?;
    // Verify the full candidate, including genuine record/result arithmetic and
    // exact report/run bindings, before the final manifest becomes visible.
    let mut view = crate::project_open::verify_import_candidate(
        root,
        &candidate,
        Some(expected),
        cancel.clone(),
        |_| {},
    )?;
    checkpoint(&cancel)?;
    persistence::check_revision(root, Some(expected))?;
    view.revision = persistence::atomic_project_cancellable(root, &candidate, &cancel)?;
    Ok(DesignReportResult {
        view,
        report: inspected,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use greenlens_core::energy::*;

    #[test]
    fn proposal_selection_disclosure_rebinds_presentation_not_physics() {
        // Arithmetic-only presentation test. The fixture stays NotExecuted and
        // NotChecked; it is never issued, sealed or accepted as engine evidence.
        let mut base = synthetic_energy_report().unwrap();
        base.evidence.clear();
        let mut data = EnergyReportWorkbenchData::new(
            base,
            EnergyReportMetadata::default(),
            vec![
                EnergyReportProfile::Executive,
                EnergyReportProfile::Technical,
            ],
            EnergyReportAttachmentSummary::default(),
            crate::energy_report_workbench::template_sha256(),
        )
        .unwrap();
        let before = data.clone();
        assert!(
            proposal_declarations(&crate::energy_services::example_document(), &mut data)
                .unwrap()
                .is_empty()
        );
        data.validate().unwrap();
        assert_ne!(data.base_dataset_sha256, before.base_dataset_sha256);
        assert_ne!(data.derived_sha256, before.derived_sha256);
        assert_eq!(data.base.baseline, before.base.baseline);
        assert_eq!(data.base.proposed, before.base.proposed);
        assert!(data
            .base
            .limitations
            .last()
            .unwrap()
            .contains("0 included of 0 retained"));
        assert_eq!(
            data.base.proposed.assessment.execution,
            EnergyExecutionStatus::NotExecuted
        );
    }

    #[test]
    fn staging_is_project_and_exact_request_bound() {
        // Local control metadata only: no native or accepted run is fabricated.
        let staged = StagedReport {
            schema_version: STAGE_SCHEMA.into(),
            project_id: "project_fixture".into(),
            expected_revision: "a".repeat(64),
            content_sha256: "b".repeat(64),
            baseline_run_id: "00000000-0000-4000-8000-000000000001".into(),
            proposed_run_id: "00000000-0000-4000-8000-000000000002".into(),
        };
        validate_staging(&staged, "project_fixture").unwrap();
        assert!(validate_staging(&staged, "another_document").is_err());
        let mut changed = staged.clone();
        changed.content_sha256 = "../outside".into();
        assert!(validate_staging(&changed, "project_fixture").is_err());
        changed = staged.clone();
        changed.proposed_run_id = changed.baseline_run_id.clone();
        assert!(validate_staging(&changed, "project_fixture").is_err());
        assert!(stage_root("../outside").is_err());
        assert!(stage_root("00000000-0000-4000-8000-000000000001/child").is_err());
    }
    #[test]
    fn cancellation_and_unsaved_document_fail_before_any_render_or_staging() {
        let project = crate::energy_services::example_document();
        let unused = Path::new("unused-no-render-fixture");
        let error = create(
            unused,
            &project,
            Some(&"a".repeat(64)),
            "a",
            "b",
            "scope".into(),
            None,
            false,
            None,
            Arc::new(AtomicBool::new(true)),
        )
        .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        let error = create(
            unused,
            &project,
            None,
            "a",
            "b",
            "scope".into(),
            None,
            false,
            None,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap_err();
        assert!(error.to_string().contains("Save the complete"));
    }
}
