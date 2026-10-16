//! Append-only portable report issues. Product hashes and scope/run bindings
//! are immutable; timestamps and local renderer receipts are not content IDs.
//! Inspecting or regenerating an issue never launches a simulation engine.
use crate::{
    energy_report::{
        architectural::{aligned_views, SensitivityReportEvidence},
        EnergyReportArtifacts, ReportArtifact, TypstConfig,
    },
    energy_report_workbench::issuance::{self, IssueContent, IssueReport},
    persistence::{self, *},
    reference, study_revisions, Project, ProjectView,
};
use anyhow::{ensure, Context, Result};
use greenlens_core::{
    architectural_scope::ARCHITECTURAL_SCOPE_SCHEMA, canonical_hash, reference::decode_chunk,
    study_revision::ScopeRevision,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use uuid::Uuid;

pub const INDEX_SCHEMA: &str = "greenlens.report-issue-index.v1";
pub const PRODUCT_SCHEMA: &str = "greenlens.report-issue-products.v1";
pub const MAX_ISSUES: usize = 32;
pub const MAX_PRODUCTS: usize = 252; // Leaves receipt/manifest/seal headroom.
const MAX_INDEX_BYTES: usize = 256 * 1024;
const MAX_PRODUCT_MANIFEST_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReportIssueIndex {
    pub schema_version: String,
    pub issues: Vec<ReportIssueSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IssueRunBinding {
    pub run_id: String,
    pub record_sha256: String,
    pub design_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReportIssueSummary {
    pub id: String,
    pub path: String,
    pub content_sha256: String,
    pub content_file_sha256: String,
    pub products_sha256: String,
    pub products_file_sha256: String,
    pub receipt_file_sha256: String,
    pub scope_revision_sha256: String,
    pub reference_asset_id: String,
    pub reference_manifest_sha256: String,
    pub baseline: IssueRunBinding,
    pub proposed: IssueRunBinding,
    pub sensitivity_session_id: Option<String>,
    pub sensitivity_run_ids: Vec<String>,
    pub title: String,
    pub report_revision: String,
    pub include_pdf: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueProductManifest {
    pub schema_version: String,
    pub content_sha256: String,
    pub products_sha256: String,
    pub products: Vec<ReportArtifact>,
}
impl IssueProductManifest {
    pub fn identity(&self) -> Result<String> {
        Ok(canonical_hash(&(
            PRODUCT_SCHEMA,
            &self.content_sha256,
            &self.products,
        ))?)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == PRODUCT_SCHEMA && hash(&self.content_sha256),
            "Unsupported issue product manifest"
        );
        ensure!(
            !self.products.is_empty() && self.products.len() <= MAX_PRODUCTS,
            "Issue requires one to 252 products"
        );
        let mut budget = PackageBudget::default();
        let mut previous = None;
        for p in &self.products {
            budget.add(&p.path, p.bytes)?;
            ensure!(
                p.bytes > 0 && hash(&p.sha256),
                "Invalid issue product identity"
            );
            ensure!(
                p.path != "checksums.json"
                    && p.path != "issue-products.json"
                    && p.path != "issue-runtime.json"
                    && p.path != "report-artifacts.json",
                "Runtime or self-referential product is not deterministic content"
            );
            ensure!(
                previous.is_none_or(|s: &str| s < p.path.as_str()),
                "Issue products must have unique deterministic path order"
            );
            previous = Some(p.path.as_str());
        }
        ensure!(
            self.products.iter().any(|p| p.path == "issue-content.json"),
            "Issue content product missing"
        );
        ensure!(
            self.products_sha256 == self.identity()?,
            "Issue product manifest identity differs"
        );
        Ok(())
    }
}

/// Portable runtime observations deliberately omit executable paths, arguments,
/// free-form errors and machine configuration. They do not imply qualification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueRuntimeReceipt {
    pub schema_version: String,
    pub content_sha256: String,
    pub issued_utc: String,
    pub renderer_status: String,
    pub process_count: usize,
    pub elapsed_ms: u128,
    pub peak_working_set_bytes: u64,
    pub peak_scratch_bytes: u64,
    pub process_trees_empty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportIssueInspection {
    pub summary: ReportIssueSummary,
    pub content: IssueContent,
    pub products: IssueProductManifest,
    pub receipt: IssueRuntimeReceipt,
    pub regeneration_available: bool,
    pub regeneration_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportIssueResult {
    pub view: ProjectView,
    pub issue: ReportIssueInspection,
}

/// A complete verified render survived a publication conflict/cancellation.
/// The token identifies only owned external staging, never an imported path.
#[derive(Debug)]
pub struct PendingIssuePublication {
    pub recovery_token: String,
    pub reason: String,
}
impl std::fmt::Display for PendingIssuePublication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f,"Completed report products remain in local recovery staging {}. Reload/reconcile the project and recover this token; no issued history was overwritten. {}",self.recovery_token,self.reason)
    }
}
impl std::error::Error for PendingIssuePublication {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StagedIssue {
    schema_version: String,
    project_id: String,
    summary: ReportIssueSummary,
    /// Recovery receipt is local, outside the portable project and products.
    expected_revision: Option<String>,
}

fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn checkpoint(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Report issue operation cancelled; no partial issue was accepted"
    );
    crate::project_open::checkpoint(0)
}
fn read<T: serde::de::DeserializeOwned>(
    root: &Path,
    name: &str,
    limit: u64,
    cancel: &AtomicBool,
) -> Result<T> {
    Ok(serde_json::from_slice(&read_evidence_bytes_checked(
        &regular_child(root, name)?,
        limit,
        &mut |n| {
            checkpoint(cancel)?;
            crate::project_open::checkpoint(n)
        },
    )?)?)
}
fn issues(project: &Project) -> &[ReportIssueSummary] {
    project
        .report_issues
        .as_ref()
        .map(|i| i.issues.as_slice())
        .unwrap_or(&[])
}
fn summary<'a>(project: &'a Project, id: &str) -> Result<&'a ReportIssueSummary> {
    ensure!(hash(id), "Invalid report issue identifier");
    issues(project)
        .iter()
        .find(|s| s.id == id)
        .context("Selected report issue is not retained in this document")
}
fn validate_summary(s: &ReportIssueSummary) -> Result<()> {
    ensure!(
        s.id == issue_identity(&s.content_sha256, &s.products_sha256)?
            && s.path == format!("report-issues/{}", s.id),
        "Invalid report issue identity/path"
    );
    for h in [
        &s.id,
        &s.content_file_sha256,
        &s.products_sha256,
        &s.products_file_sha256,
        &s.receipt_file_sha256,
        &s.scope_revision_sha256,
        &s.reference_manifest_sha256,
        &s.baseline.record_sha256,
        &s.proposed.record_sha256,
    ] {
        ensure!(hash(h), "Invalid report issue digest");
    }
    for id in [
        &s.reference_asset_id,
        &s.baseline.run_id,
        &s.proposed.run_id,
    ]
    .into_iter()
    .chain(s.sensitivity_session_id.iter())
    .chain(s.sensitivity_run_ids.iter())
    {
        ensure!(
            Uuid::parse_str(id)?.to_string() == *id,
            "Report issue requires canonical UUID references"
        );
    }
    ensure!(
        s.baseline.run_id != s.proposed.run_id && s.baseline.design_id != s.proposed.design_id,
        "Issue requires a distinct baseline/proposed pair"
    );
    ensure!(
        !s.title.is_empty() && s.title.len() <= 1024 && s.report_revision.len() <= 256,
        "Invalid bounded report title/revision"
    );
    ensure!(
        s.sensitivity_run_ids.len() <= 4
            && s.sensitivity_run_ids.iter().collect::<BTreeSet<_>>().len()
                == s.sensitivity_run_ids.len(),
        "Invalid sensitivity closure"
    );
    ensure!(
        s.sensitivity_session_id.is_some() || s.sensitivity_run_ids.is_empty(),
        "Unbound sensitivity runs"
    );
    Ok(())
}

fn issue_identity(content: &str, products: &str) -> Result<String> {
    Ok(canonical_hash(&(INDEX_SCHEMA, content, products))?)
}

/// Pure admission only. This does not label product or engine bytes verified.
pub fn validate_index(project: &Project) -> Result<()> {
    let Some(index) = &project.report_issues else {
        return Ok(());
    };
    ensure!(
        index.schema_version == INDEX_SCHEMA
            && !index.issues.is_empty()
            && index.issues.len() <= MAX_ISSUES,
        "Retain one to 32 immutable report issues; no history is silently pruned"
    );
    ensure!(
        serde_json::to_vec(index)?.len() <= MAX_INDEX_BYTES,
        "Issue index exceeds 256 KiB"
    );
    let energy = project
        .energy
        .as_ref()
        .context("Issued reports require retained energy evidence")?;
    let mut seen = BTreeSet::new();
    for s in &index.issues {
        validate_summary(s)?;
        ensure!(seen.insert(&s.id), "Duplicate report issue");
        let revision = study_revisions::selected(project, &s.scope_revision_sha256)?;
        ensure!(
            revision.reference.asset_id == s.reference_asset_id
                && revision.reference.manifest_sha256 == s.reference_manifest_sha256,
            "Issued reference must remain retained"
        );
        for r in [&s.baseline, &s.proposed] {
            ensure!(
                energy.runs.iter().any(|v| v.id == r.run_id
                    && v.record_sha256 == r.record_sha256
                    && v.alternative_id == r.design_id),
                "Issued run evidence must remain retained with unchanged identity"
            );
            ensure!(
                revision.designs.contains_key(&r.design_id),
                "Issued scope lacks selected design"
            );
        }
        if let Some(session) = &s.sensitivity_session_id {
            ensure!(
                energy
                    .workbench
                    .as_ref()
                    .is_some_and(|w| w.sensitivity.iter().any(|r| &r.id == session)),
                "Issued exact-pair sensitivity receipt must remain retained"
            );
            ensure!(
                s.sensitivity_run_ids
                    .iter()
                    .all(|id| energy.runs.iter().any(|r| &r.id == id)),
                "Issued sensitivity run evidence must remain retained"
            );
        }
    }
    Ok(())
}

/// Normal document saves/Save As must not discard an issued record through an
/// older undo snapshot or stale client. Selected portable exports are a separate
/// explicit operation and retain their declared dependency closure.
pub fn validate_preserved(previous: &Project, candidate: &Project) -> Result<()> {
    ensure!(
        previous.id == candidate.id,
        "Issued history belongs to another document"
    );
    for old in issues(previous) {
        ensure!(issues(candidate).iter().any(|new| new == old),
            "Issued report history cannot be removed or changed by this save. Restore the retained issue {} and its complete evidence before saving; no history was pruned", old.id);
    }
    Ok(())
}

fn figures(
    root: &Path,
    project: &Project,
    revision: &ScopeRevision,
    data: &greenlens_core::energy::EnergyReportWorkbenchData,
    requests: Option<&[aligned_views::AlignedFigureDefinition]>,
    cancel: &AtomicBool,
) -> Result<Vec<aligned_views::AlignedFigure>> {
    checkpoint(cancel)?;
    let asset = reference::asset(project, Some(&revision.reference.asset_id))?;
    let manifest = reference::read_manifest(root, asset)?;
    ensure!(
        manifest == revision.reference.manifest
            && asset.manifest_sha256 == revision.reference.manifest_sha256,
        "Retained issue source differs; active reference substitution is prohibited"
    );
    let mut output = Vec::new();
    for (i, view) in revision.scope.report_views.iter().enumerate() {
        checkpoint(cancel)?;
        let case = [&data.base.baseline, &data.base.proposed]
            .into_iter()
            .find(|c| c.id == view.design_id)
            .context("Retained view design absent from selected issue pair")?;
        let request = if let Some(definitions) = requests {
            definitions
                .get(i)
                .context("Frozen figure is missing")?
                .request
                .clone()
        } else {
            aligned_views::AlignedFigureRequest {
                schema_version: aligned_views::ALIGNED_FIGURE_SCHEMA.into(),
                scope_revision_sha256: revision.revision_sha256.clone(),
                scope_content_sha256: canonical_hash(&(
                    ARCHITECTURAL_SCOPE_SCHEMA,
                    &revision.scope,
                ))?,
                selected_run_id: case.run_id.clone(),
                selected_record_sha256: case.record_sha256.clone(),
                view: view.clone(),
                focus_occurrence_ids: vec![],
            }
        };
        // Full stream, one bounded chunk at a time. decode_chunk checks its exact
        // manifest digest/shape. No stride sampling precedes view clipping.
        let stream = manifest
            .chunks
            .iter()
            .enumerate()
            .flat_map(|(index, entry)| {
                let result: Result<Vec<greenlens_core::reference::ReferenceTriangle>> = (|| {
                    let bytes = read_evidence_bytes_checked(
                        &regular_child(root, &format!("{}/{}", asset.path, entry.path))?,
                        entry.byte_length as u64,
                        &mut |n| {
                            checkpoint(cancel)?;
                            crate::project_open::checkpoint(n)
                        },
                    )?;
                    Ok(decode_chunk(&manifest, index, &bytes)?)
                })(
                );
                match result {
                    Ok(triangles) => triangles.into_iter().map(Ok).collect::<Vec<_>>(),
                    Err(error) => vec![Err(error)],
                }
            });
        output.push(aligned_views::render(
            &manifest,
            &revision.scope,
            &case.model,
            &case.study.calendar,
            &request,
            stream,
            cancel,
        )?);
    }
    Ok(output)
}

/// Shared CLI/Tauri preparation. All evidence readers share this cancellable
/// actual-byte inspection scope; there is no implicit active-model substitution.
pub fn prepare_issue(
    root: &Path,
    project: &Project,
    a: &str,
    b: &str,
    scope_sha: &str,
    include_pdf: bool,
    cancel: Arc<AtomicBool>,
) -> Result<IssueReport> {
    crate::project_open::inspect_evidence(
        root,
        cancel.clone(),
        |_| {},
        || {
            checkpoint(&cancel)?;
            crate::validate_project(project)?;
            ensure!(a != b, "Select distinct retained baseline/proposed runs");
            let revision = study_revisions::selected(project, scope_sha)?.clone();
            let data = crate::energy_services::workbench_report_data(root, project, a, b)?;
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
            let rendered = figures(root, project, &revision, &data, None, &cancel)?;
            issuance::prepare(data, revision, sensitivity, rendered, include_pdf)
        },
    )
}

fn bindings_match(s: &ReportIssueSummary, c: &IssueContent) -> Result<()> {
    ensure!(
        s.content_sha256 == c.content_sha256
            && s.scope_revision_sha256 == c.scope_revision.revision_sha256
            && s.reference_asset_id == c.scope_revision.reference.asset_id
            && s.reference_manifest_sha256 == c.scope_revision.reference.manifest_sha256
            && s.include_pdf == c.include_pdf,
        "Issue index differs from immutable content bindings"
    );
    for (r, case) in [
        (&s.baseline, &c.workbench.base.baseline),
        (&s.proposed, &c.workbench.base.proposed),
    ] {
        ensure!(
            r.run_id == case.run_id
                && r.record_sha256 == case.record_sha256
                && r.design_id == case.id,
            "Issue run binding differs from frozen content"
        );
    }
    ensure!(
        s.title == c.workbench.metadata.title && s.report_revision == c.workbench.metadata.revision,
        "Issue presentation metadata differs"
    );
    let mut sensitivity_runs = c
        .sensitivity
        .as_ref()
        .map(|e| {
            e.result
                .assessment
                .runs
                .iter()
                .map(|r| r.run_id.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    sensitivity_runs.sort();
    sensitivity_runs.dedup();
    ensure!(
        s.sensitivity_session_id == c.sensitivity.as_ref().map(|e| e.session_id.clone())
            && s.sensitivity_run_ids == sensitivity_runs,
        "Issue sensitivity binding differs"
    );
    Ok(())
}

/// Shape and independently reproducible products are bound to frozen content,
/// not just a caller-supplied/resealed inventory. PDF/HTML product digests are
/// additionally bound by the issued identity; hashes are not author signatures.
fn validate_product_bindings(c: &IssueContent, products: &IssueProductManifest) -> Result<()> {
    use greenlens_core::energy::EnergyReportProfile;
    let d = &c.workbench;
    let p = d.base.presentation()?;
    // Serialize original typed values, preserving their declared field order.
    let mut required: BTreeMap<String, Option<String>> = BTreeMap::new();
    macro_rules! json_product {
        ($name:expr, $value:expr) => {
            required.insert(
                $name.into(),
                Some(bytes_hash(&serde_json::to_vec_pretty($value)?)),
            );
        };
    }
    json_product!("issue-content.json", c);
    json_product!("scope-revision.json", &c.scope_revision);
    json_product!("input-provenance.json", &c.scope_revision.provenance);
    json_product!("aligned-figures.json", &c.figures);
    json_product!("workbench-report.json", d);
    json_product!("energy-report.json", &d.base);
    json_product!("typed-differences.json", &d.typed_comparison);
    json_product!("performance.json", &p);
    if let Some(e) = &c.sensitivity {
        json_product!("issue-sensitivity.json", e);
    }
    required.insert("input-provenance.csv".into(), None);
    for (name, bytes) in [
        (
            "performance.csv",
            crate::energy_report::csv(&p)?.into_bytes(),
        ),
        (
            "inputs.csv",
            crate::energy_report::inputs_csv(&d.base)?.into_bytes(),
        ),
        (
            "baseline-model.svg",
            crate::energy_report::model_svg(&d.base.baseline).into_bytes(),
        ),
        (
            "proposed-model.svg",
            crate::energy_report::model_svg(&d.base.proposed).into_bytes(),
        ),
        (
            "annual.svg",
            crate::energy_report::chart(&p, false).into_bytes(),
        ),
        (
            "monthly.svg",
            crate::energy_report::chart(&p, true).into_bytes(),
        ),
    ] {
        required.insert(name.into(), Some(bytes_hash(&bytes)));
    }
    for (i, figure) in c.figures.iter().enumerate() {
        required.insert(
            format!("aligned-view-{:03}.svg", i + 1),
            Some(figure.svg_sha256.clone()),
        );
    }
    let mut names = vec!["input-appendix", "performance", "assessment"];
    for profile in &d.profiles {
        names.push(match profile {
            EnergyReportProfile::Executive => "executive",
            EnergyReportProfile::Technical => "technical",
        });
    }
    if d.profiles.contains(&EnergyReportProfile::Technical) {
        for (side, case) in [
            ("baseline", &d.base.baseline),
            ("proposed", &d.base.proposed),
        ] {
            for (i, schedule) in case.model.schedules.iter().enumerate() {
                required.insert(
                    format!("schedule-{side}-{i:03}.svg"),
                    Some(bytes_hash(
                        crate::energy_report_workbench::schedule_svg(schedule).as_bytes(),
                    )),
                );
            }
        }
    }
    for name in names {
        required.insert(format!("{name}.html"), None);
        if c.include_pdf {
            required.insert(format!("{name}.pdf"), None);
        }
    }
    let pages = products
        .products
        .iter()
        .filter(|p| p.path.starts_with("performance-page-"))
        .collect::<Vec<_>>();
    if c.include_pdf {
        ensure!(
            !pages.is_empty() && pages.len() <= 128,
            "Issued PDF family requires its complete bounded performance PNG page sequence"
        );
        for (i, page) in pages.iter().enumerate() {
            let name = format!("performance-page-{:03}.png", i + 1);
            ensure!(
                page.path == name,
                "Issued performance PNG page sequence is incomplete"
            );
            required.insert(name, None);
        }
    } else {
        ensure!(
            pages.is_empty(),
            "HTML-only issue cannot silently acquire native rendered pages"
        );
    }
    let actual = products
        .products
        .iter()
        .map(|p| (p.path.as_str(), p))
        .collect::<BTreeMap<_, _>>();
    ensure!(
        actual.len() == required.len() && required.keys().all(|n| actual.contains_key(n.as_str())),
        "Issued product family is incomplete or contains an unsupported product"
    );
    for (name, hash) in required {
        if let Some(expected) = hash {
            ensure!(
                actual[name.as_str()].sha256 == expected,
                "Issued product differs from frozen content: {name}"
            );
        }
    }
    Ok(())
}

fn inspect_directory(
    directory: &Path,
    s: &ReportIssueSummary,
    cancel: &AtomicBool,
) -> Result<ReportIssueInspection> {
    validate_summary(s)?;
    checkpoint(cancel)?;
    let content_bytes = read_evidence_bytes_checked(
        &regular_child(directory, "issue-content.json")?,
        MAX_EVIDENCE_FILE_BYTES,
        &mut |n| {
            checkpoint(cancel)?;
            crate::project_open::checkpoint(n)
        },
    )?;
    ensure!(
        bytes_hash(&content_bytes) == s.content_file_sha256,
        "Issued content bytes changed"
    );
    let content: IssueContent = serde_json::from_slice(&content_bytes)?;
    content.validate()?;
    bindings_match(s, &content)?;
    let products_bytes = read_evidence_bytes_checked(
        &regular_child(directory, "issue-products.json")?,
        MAX_PRODUCT_MANIFEST_BYTES,
        &mut |n| {
            checkpoint(cancel)?;
            crate::project_open::checkpoint(n)
        },
    )?;
    ensure!(
        bytes_hash(&products_bytes) == s.products_file_sha256,
        "Issued product manifest bytes changed"
    );
    let products: IssueProductManifest = serde_json::from_slice(&products_bytes)?;
    products.validate()?;
    validate_product_bindings(&content, &products)?;
    ensure!(
        products.content_sha256 == s.content_sha256
            && products.products_sha256 == s.products_sha256,
        "Issued product set binding differs"
    );
    let sealed: Checksums = read(directory, "checksums.json", 64 * 1024, cancel)?;
    let mut expected: BTreeMap<_, _> = products
        .products
        .iter()
        .map(|p| (p.path.clone(), p.sha256.clone()))
        .collect();
    expected.insert("issue-products.json".into(), s.products_file_sha256.clone());
    // Runtime receipt has no deterministic effect, but its actual bytes are
    // retained and sealed. Arbitrary extra sealed members are not admitted.
    let runtime_bytes = read_evidence_bytes_checked(
        &regular_child(directory, "issue-runtime.json")?,
        64 * 1024,
        &mut |n| {
            checkpoint(cancel)?;
            crate::project_open::checkpoint(n)
        },
    )?;
    ensure!(
        bytes_hash(&runtime_bytes) == s.receipt_file_sha256,
        "Issued runtime receipt bytes changed"
    );
    expected.insert("issue-runtime.json".into(), s.receipt_file_sha256.clone());
    ensure!(
        sealed.schema_version == "0.1.0" && sealed.sha256 == expected,
        "Issue seal has changed/missing/unexpected products"
    );
    let receipt: IssueRuntimeReceipt = serde_json::from_slice(&runtime_bytes)?;
    ensure!(
        receipt.schema_version == "greenlens.report-issue-runtime.v1"
            && receipt.content_sha256 == s.content_sha256
            && receipt.process_trees_empty
            && receipt.issued_utc.len() <= 64
            && chrono::DateTime::parse_from_rfc3339(&receipt.issued_utc).is_ok()
            && if content.include_pdf {
                receipt.renderer_status == "completed"
                    && receipt.process_count == content.workbench.profiles.len() + 3
            } else {
                receipt.renderer_status == "not_requested" && receipt.process_count == 0
            },
        "Invalid issue runtime receipt"
    );
    let mut budget = PackageBudget::default();
    for p in &products.products {
        checkpoint(cancel)?;
        budget.add(&p.path, p.bytes)?;
        let path = regular_child(directory, &p.path)?;
        ensure!(
            fs::metadata(&path)?.len() == p.bytes,
            "Issued product size changed: {}",
            p.path
        );
        let actual = read_evidence_bytes_checked(&path, p.bytes, &mut |n| {
            checkpoint(cancel)?;
            crate::project_open::checkpoint(n)
        })?;
        ensure!(
            bytes_hash(&actual) == p.sha256,
            "Issued product bytes changed: {}",
            p.path
        );
    }
    let supported = content.ensure_supported_regeneration();
    let message=match &supported {Ok(())=>"Exact renderer/template/font recipe is supported; actual dependencies are checked before regeneration".into(),Err(e)=>e.to_string()};
    Ok(ReportIssueInspection {
        summary: s.clone(),
        content,
        products,
        receipt,
        regeneration_available: supported.is_ok(),
        regeneration_message: message,
    })
}

fn inspect_impl(
    root: &Path,
    project: &Project,
    id: &str,
    cancel: &AtomicBool,
) -> Result<ReportIssueInspection> {
    validate_index(project)?;
    let s = summary(project, id)?;
    let value = inspect_directory(&root.join(relative_path(&s.path)?), s, cancel)?;
    ensure!(
        study_revisions::selected(project, &s.scope_revision_sha256)?
            == &value.content.scope_revision,
        "Issued scope snapshot differs from retained history"
    );
    if let Some(evidence) = &value.content.sensitivity {
        let reference = project
            .energy
            .as_ref()
            .and_then(|e| e.workbench.as_ref())
            .and_then(|w| w.sensitivity.iter().find(|r| r.id == evidence.session_id))
            .context("Issued sensitivity receipt is not retained")?;
        ensure!(
            reference == &evidence.receipt_reference,
            "Issued sensitivity receipt/event hashes differ from the retained exact-pair session"
        );
    }
    Ok(value)
}
pub fn inspect(
    root: &Path,
    project: &Project,
    id: &str,
    cancel: Arc<AtomicBool>,
) -> Result<ReportIssueInspection> {
    crate::project_open::inspect_evidence(
        root,
        cancel.clone(),
        |_| {},
        || inspect_in_operation(root, project, id, &cancel),
    )
}

/// For a shared cancellable history/report IPC operation which already owns
/// project_open::inspect_evidence. Avoid nesting a second operation/control.
pub fn inspect_in_operation(
    root: &Path,
    project: &Project,
    id: &str,
    cancel: &AtomicBool,
) -> Result<ReportIssueInspection> {
    let value = inspect_impl(root, project, id, cancel)?;
    let mut weather_locations = BTreeMap::new();
    for case in [
        &value.content.workbench.base.baseline,
        &value.content.workbench.base.proposed,
    ] {
        checkpoint(cancel)?;
        let run = crate::energy_services::selected_run(root, project, &case.run_id)?;
        validate_case_record(case, &run)?;
        validate_case_location(root, case, &mut weather_locations, cancel)?;
    }
    if let Some(evidence) = &value.content.sensitivity {
        for expected in &evidence.result.assessment.runs {
            checkpoint(cancel)?;
            let run = crate::energy_services::selected_run(root, project, &expected.run_id)?;
            validate_sensitivity_record(expected, &run)?;
        }
    }
    Ok(value)
}
pub fn verify_project(root: &Path, project: &Project, cancel: &AtomicBool) -> Result<()> {
    validate_index(project)?;
    for issue in issues(project) {
        checkpoint(cancel)?;
        inspect_in_operation(root, project, &issue.id, cancel)?;
    }
    Ok(())
}

fn validate_case_record(
    case: &greenlens_core::energy::EnergyReportCase,
    run: &crate::energy::EnergyRunRecord,
) -> Result<()> {
    ensure!(case.run_id==run.id && case.id==run.alternative_id && case.record_sha256==canonical_hash(run)? && case.model==run.model && case.study==run.study
        && case.model_input_sha256==run.validated.model_input_sha256 && case.physical_input_sha256==run.provenance.physical_input_hash
        && case.processing_sha256==run.provenance.processing_identity && case.method==run.method && case.totals==run.totals && case.meter_ledger==run.ledger
        && case.gross_floor_area_m2==run.validated.gross_floor_area_m2 && case.conditioned_floor_area_m2==run.validated.conditioned_floor_area_m2
        && case.diagnostics.source_record_sha256==case.record_sha256 && case.diagnostics.messages==run.provenance.diagnostics
        && case.diagnostics.warning_count==Some(run.provenance.warning_count) && case.diagnostics.severe_count==Some(run.provenance.severe_count),
        "Issued case differs from its exact retained engine record; historical active-input or registry states are not substituted");
    validate_weather_source(&case.weather_summary, &run.weather_asset)?;
    Ok(())
}

fn validate_weather_source(
    summary: &greenlens_core::energy::EnergyWeatherSummary,
    asset: &crate::energy_weather::EnergyWeatherAsset,
) -> Result<()> {
    ensure!(summary.binding == asset.binding
        && summary.source.citation == asset.source.citation
        && summary.source.synthetic == asset.source.synthetic_non_climatic,
        "Issued weather citation or synthetic status differs from the exact retained weather source");
    Ok(())
}

fn validate_case_location(
    root: &Path,
    case: &greenlens_core::energy::EnergyReportCase,
    locations: &mut BTreeMap<String, greenlens_core::weather::WeatherLocation>,
    cancel: &AtomicBool,
) -> Result<()> {
    checkpoint(cancel)?;
    // The selected run reader already reparsed and validated this file. Read
    // only its metadata shape here, retain its actual-byte observation in the
    // surrounding integrity fence, and deduplicate across issues in one open.
    // Skipped hourly fields are not allocated or numerically processed again.
    #[derive(Deserialize)]
    struct LocationEnvelope {
        location: greenlens_core::weather::WeatherLocation,
    }
    if !locations.contains_key(&case.run_id) {
        let weather: LocationEnvelope = read(
            root,
            &format!("runs/{}/weather/weather.json", case.run_id),
            MAX_EVIDENCE_FILE_BYTES,
            cancel,
        )?;
        locations.insert(case.run_id.clone(), weather.location);
    }
    ensure!(
        locations[&case.run_id] == case.weather_summary.location,
        "Issued weather location/station metadata differs from retained normalized weather"
    );
    Ok(())
}

fn validate_sensitivity_record(
    expected: &greenlens_core::energy::EnergySensitivityRun,
    run: &crate::energy::EnergyRunRecord,
) -> Result<()> {
    ensure!(
        expected.run_id == run.id
            && expected.record_sha256 == canonical_hash(run)?
            && expected.physical_input_sha256 == run.provenance.physical_input_hash
            && expected.execution_input_sha256 == run.provenance.execution_input_hash
            && expected.method == run.method
            && expected.totals == run.totals,
        "Issued sensitivity result differs from its exact retained engine output"
    );
    Ok(())
}

/// The opening transaction has already processed and byte-fenced these exact
/// records. Reuse them without a second annual arithmetic traversal. Product
/// bytes and immutable scope bindings are still checked here.
pub fn verify_project_with_provider<'a>(
    root: &Path,
    project: &Project,
    cancel: &AtomicBool,
    mut provider: impl FnMut(&str) -> Result<&'a crate::energy::EnergyRunRecord>,
) -> Result<()> {
    validate_index(project)?;
    let mut weather_locations = BTreeMap::new();
    for issue in issues(project) {
        checkpoint(cancel)?;
        let inspected = inspect_impl(root, project, &issue.id, cancel)?;
        for case in [
            &inspected.content.workbench.base.baseline,
            &inspected.content.workbench.base.proposed,
        ] {
            checkpoint(cancel)?;
            validate_case_record(case, provider(&case.run_id)?)?;
            validate_case_location(root, case, &mut weather_locations, cancel)?;
        }
        if let Some(evidence) = &inspected.content.sensitivity {
            for expected in &evidence.result.assessment.runs {
                checkpoint(cancel)?;
                validate_sensitivity_record(expected, provider(&expected.run_id)?)?;
            }
        }
    }
    Ok(())
}

fn bound_members(
    directory: &Path,
    inspection: &ReportIssueInspection,
) -> Result<BoundReferenceMembers> {
    let mut result: BoundReferenceMembers = inspection
        .products
        .products
        .iter()
        .map(|p| (p.path.clone(), (p.sha256.clone(), p.bytes)))
        .collect();
    for name in ["issue-products.json", "issue-runtime.json"] {
        let path = regular_child(directory, name)?;
        result.insert(
            name.into(),
            (evidence_file_hash(&path)?, fs::metadata(path)?.len()),
        );
    }
    Ok(result)
}
pub fn evidence_members(root: &Path, project: &Project) -> Result<Vec<String>> {
    let cancel = AtomicBool::new(false);
    let mut result = Vec::new();
    for s in issues(project) {
        let value = inspect_impl(root, project, &s.id, &cancel)?;
        result.extend(
            value
                .products
                .products
                .iter()
                .map(|p| format!("{}/{}", s.path, p.path)),
        );
        result.extend(
            [
                "issue-products.json",
                "issue-runtime.json",
                "checksums.json",
            ]
            .map(|p| format!("{}/{}", s.path, p)),
        );
    }
    Ok(result)
}
pub fn copy_evidence(
    source: &Path,
    destination: &Path,
    project: &Project,
    cancel: &AtomicBool,
) -> Result<()> {
    validate_index(project)?;
    for s in issues(project) {
        let inspection = inspect_impl(source, project, &s.id, cancel)?;
        let directory = source.join(relative_path(&s.path)?);
        let members = bound_members(&directory, &inspection)?;
        persistence::promote_issue_bound(&directory, destination, &s.id, &members, &mut |n| {
            checkpoint(cancel)?;
            crate::project_open::checkpoint(n)
        })?;
    }
    Ok(())
}

/// Enumerated PDF/PNG access or inert SVG preview. SVG previews are freshly
/// generated by owned code and compared with the issued hash; imported SVG
/// strings are never returned as trusted active markup.
pub fn product_bytes(
    root: &Path,
    project: &Project,
    id: &str,
    product: &str,
    preview: bool,
    cancel: Arc<AtomicBool>,
) -> Result<(String, Vec<u8>)> {
    crate::project_open::inspect_evidence(
        root,
        cancel.clone(),
        |_| {},
        || {
            let inspected = inspect_in_operation(root, project, id, &cancel)?;
            let expected = inspected
                .products
                .products
                .iter()
                .find(|p| p.path == product)
                .context("Product is not in this immutable issue")?;
            relative_path(product)?;
            let lower = product.to_ascii_lowercase();
            if lower.ends_with(".svg") {
                ensure!(
                    preview,
                    "SVG is available only as an inert image preview, not an executable document"
                );
                inspected.content.ensure_supported_regeneration()?;
                let data = &inspected.content.workbench;
                let owned = if product == "baseline-model.svg" {
                    crate::energy_report::model_svg(&data.base.baseline).into_bytes()
                } else if product == "proposed-model.svg" {
                    crate::energy_report::model_svg(&data.base.proposed).into_bytes()
                } else if product == "annual.svg" || product == "monthly.svg" {
                    crate::energy_report::chart(
                        &data.base.presentation()?,
                        product == "monthly.svg",
                    )
                    .into_bytes()
                } else if let Some(suffix) = product
                    .strip_prefix("aligned-view-")
                    .and_then(|s| s.strip_suffix(".svg"))
                {
                    let index = suffix
                        .parse::<usize>()?
                        .checked_sub(1)
                        .context("Invalid aligned view number")?;
                    ensure!(
                        format!("aligned-view-{:03}.svg", index + 1) == product,
                        "Noncanonical figure path"
                    );
                    let rendered = figures(
                        root,
                        project,
                        &inspected.content.scope_revision,
                        data,
                        Some(&inspected.content.figures),
                        &cancel,
                    )?;
                    rendered
                        .into_iter()
                        .nth(index)
                        .context("Issued aligned view unavailable")?
                        .svg
                } else {
                    let found = [
                        ("baseline", &data.base.baseline),
                        ("proposed", &data.base.proposed),
                    ]
                    .into_iter()
                    .find_map(|(side, case)| {
                        case.model
                            .schedules
                            .iter()
                            .enumerate()
                            .find(|(i, _)| format!("schedule-{side}-{i:03}.svg") == product)
                            .map(|(_, s)| s)
                    });
                    crate::energy_report_workbench::schedule_svg(
                        found.context("Unsupported owned SVG preview product")?,
                    )
                    .into_bytes()
                };
                ensure!(owned.len()<=8*1024*1024 && owned.len() as u64==expected.bytes && bytes_hash(&owned)==expected.sha256,"Owned preview differs from the preserved issued figure; inspect the original PDF instead");
                return Ok(("image/svg+xml".into(), owned));
            }
            ensure!(lower.ends_with(".png") || (!preview && lower.ends_with(".pdf")),"Open supports retained PDF/PNG products; preview supports owned SVG and bounded PNG images");
            let limit = if preview {
                8 * 1024 * 1024
            } else {
                MAX_EVIDENCE_FILE_BYTES
            };
            ensure!(
                expected.bytes <= limit,
                "Product exceeds the bounded preview/open limit"
            );
            let bytes = read_evidence_bytes_checked(
                &regular_child(root, &format!("{}/{product}", inspected.summary.path))?,
                expected.bytes,
                &mut |n| {
                    checkpoint(&cancel)?;
                    crate::project_open::checkpoint(n)
                },
            )?;
            ensure!(
                bytes_hash(&bytes) == expected.sha256,
                "Product changed during inspection"
            );
            if lower.ends_with(".png") {
                ensure!(
                    bytes.len() >= 33
                        && &bytes[..8] == b"\x89PNG\r\n\x1a\n"
                        && &bytes[12..16] == b"IHDR",
                    "Invalid PNG product"
                );
                let width = u32::from_be_bytes(bytes[16..20].try_into()?);
                let height = u32::from_be_bytes(bytes[20..24].try_into()?);
                ensure!(
                    width > 0
                        && height > 0
                        && width <= 8192
                        && height <= 8192
                        && u64::from(width) * u64::from(height) <= 16_777_216,
                    "PNG preview dimensions exceed the 16-megapixel bound"
                );
                Ok(("image/png".into(), bytes))
            } else {
                ensure!(
                    bytes.starts_with(b"%PDF-"),
                    "Invalid retained PDF signature"
                );
                Ok(("application/pdf".into(), bytes))
            }
        },
    )
}

fn manifest_from_artifacts(
    content: &IssueContent,
    artifacts: &EnergyReportArtifacts,
) -> Result<IssueProductManifest> {
    ensure!(
        artifacts.dataset_sha256 == content.content_sha256,
        "Renderer returned a different issue identity"
    );
    ensure!(
        !content.include_pdf || artifacts.renderer_status == "completed",
        "Required PDF/PNG generation incomplete; no report issue was accepted"
    );
    let mut products = artifacts.artifacts.clone();
    products.sort_by(|a, b| a.path.cmp(&b.path));
    let mut result = IssueProductManifest {
        schema_version: PRODUCT_SCHEMA.into(),
        content_sha256: content.content_sha256.clone(),
        products_sha256: String::new(),
        products,
    };
    result.products_sha256 = result.identity()?;
    result.validate()?;
    validate_product_bindings(content, &result)?;
    Ok(result)
}
fn make_summary(
    content: &IssueContent,
    products: &IssueProductManifest,
    directory: &Path,
) -> Result<ReportIssueSummary> {
    let binding = |c: &greenlens_core::energy::EnergyReportCase| IssueRunBinding {
        run_id: c.run_id.clone(),
        record_sha256: c.record_sha256.clone(),
        design_id: c.id.clone(),
    };
    let mut sensitivity_run_ids = content
        .sensitivity
        .as_ref()
        .map(|s| {
            s.result
                .assessment
                .runs
                .iter()
                .map(|r| r.run_id.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    sensitivity_run_ids.sort();
    sensitivity_run_ids.dedup();
    let id = issue_identity(&content.content_sha256, &products.products_sha256)?;
    let value = ReportIssueSummary {
        path: format!("report-issues/{id}"),
        id,
        content_sha256: content.content_sha256.clone(),
        content_file_sha256: evidence_file_hash(&regular_child(directory, "issue-content.json")?)?,
        products_sha256: products.products_sha256.clone(),
        products_file_sha256: evidence_file_hash(&regular_child(
            directory,
            "issue-products.json",
        )?)?,
        receipt_file_sha256: evidence_file_hash(&regular_child(directory, "issue-runtime.json")?)?,
        scope_revision_sha256: content.scope_revision.revision_sha256.clone(),
        reference_asset_id: content.scope_revision.reference.asset_id.clone(),
        reference_manifest_sha256: content.scope_revision.reference.manifest_sha256.clone(),
        baseline: binding(&content.workbench.base.baseline),
        proposed: binding(&content.workbench.base.proposed),
        sensitivity_session_id: content.sensitivity.as_ref().map(|s| s.session_id.clone()),
        sensitivity_run_ids,
        title: content.workbench.metadata.title.clone(),
        report_revision: content.workbench.metadata.revision.clone(),
        include_pdf: content.include_pdf,
    };
    validate_summary(&value)?;
    Ok(value)
}

fn stage_root(token: &str) -> Result<PathBuf> {
    ensure!(
        Uuid::parse_str(token)?.to_string() == token,
        "Invalid local staged issue recovery identifier"
    );
    Ok(crate::local_root()?
        .join("report-issue-staging")
        .join(token))
}

#[allow(clippy::too_many_arguments)]
pub fn issue(
    root: &Path,
    project: &Project,
    expected_revision: Option<&str>,
    a: &str,
    b: &str,
    scope_sha: &str,
    include_pdf: bool,
    typst: Option<&TypstConfig>,
    cancel: Arc<AtomicBool>,
) -> Result<ReportIssueResult> {
    ensure!(
        !include_pdf || typst.is_some(),
        "PDF issuance requires the pinned offline Typst/font installation"
    );
    check_revision(root, expected_revision)?;
    let prepared = prepare_issue(root, project, a, b, scope_sha, include_pdf, cancel.clone())?;
    if let Some(existing) = issues(project)
        .iter()
        .find(|s| s.content_sha256 == prepared.content.content_sha256)
    {
        let retained = inspect(root, project, &existing.id, cancel.clone())?;
        let _lock = WriterLock::acquire(root)?;
        check_revision(root, expected_revision)?;
        let mut view = crate::project_open::verify_import_candidate(
            root,
            project,
            expected_revision,
            cancel.clone(),
            |_| {},
        )?;
        checkpoint(&cancel)?;
        check_revision(root, expected_revision)?;
        view.revision = atomic_project(root, project)?;
        return Ok(ReportIssueResult {
            view,
            issue: retained,
        });
    }
    ensure!(
        issues(project).len() < MAX_ISSUES,
        "Report issue capacity is 32; preserve this study and start a separately scoped document"
    );
    let token = Uuid::new_v4().to_string();
    let stage = stage_root(&token)?;
    fs::create_dir_all(&stage)?;
    let products_dir = stage.join("products");
    let receipt = issuance::render_issue(
        &prepared,
        &products_dir,
        if include_pdf { typst } else { None },
        &cancel,
    )?;
    let products = manifest_from_artifacts(&prepared.content, &receipt)?;
    write_json(&stage.join("local-renderer-receipt.json"), &receipt)?;
    // The full receipt is intentionally external. Only deterministic products
    // and a structured non-identifying receipt enter the portable issue.
    let portable_receipt = IssueRuntimeReceipt {
        schema_version: "greenlens.report-issue-runtime.v1".into(),
        content_sha256: prepared.content.content_sha256.clone(),
        issued_utc: chrono::Utc::now().to_rfc3339(),
        renderer_status: receipt.renderer_status.clone(),
        process_count: receipt.processes.len(),
        elapsed_ms: receipt.processes.iter().map(|p| p.elapsed_ms).sum(),
        peak_working_set_bytes: receipt
            .processes
            .iter()
            .map(|p| p.sampled_peak_runner_plus_job_working_set_bytes)
            .max()
            .unwrap_or(0),
        peak_scratch_bytes: receipt
            .processes
            .iter()
            .map(|p| p.peak_observed_scratch_bytes)
            .max()
            .unwrap_or(0),
        process_trees_empty: receipt.processes.iter().all(|p| p.process_tree_empty),
    };
    write_json(&products_dir.join("issue-products.json"), &products)?;
    write_json(&products_dir.join("issue-runtime.json"), &portable_receipt)?;
    let s = make_summary(&prepared.content, &products, &products_dir)?;
    let mut names = products
        .products
        .iter()
        .map(|p| p.path.as_str())
        .collect::<Vec<_>>();
    names.extend(["issue-products.json", "issue-runtime.json"]);
    seal(&products_dir, &names)?;
    write_json(
        &stage.join("staged-issue.json"),
        &StagedIssue {
            schema_version: "greenlens.staged-report-issue.v1".into(),
            project_id: project.id.clone(),
            summary: s,
            expected_revision: expected_revision.map(String::from),
        },
    )?;
    publish_staged_issue(root, project, expected_revision, &token, cancel).map_err(|error| {
        PendingIssuePublication {
            recovery_token: token,
            reason: format!("{error:#}"),
        }
        .into()
    })
}

/// Reconcile an external-save conflict explicitly, using the newly accepted
/// project revision. Complete staged products are reused without re-rendering.
pub fn publish_staged_issue(
    root: &Path,
    project: &Project,
    expected_revision: Option<&str>,
    token: &str,
    cancel: Arc<AtomicBool>,
) -> Result<ReportIssueResult> {
    checkpoint(&cancel)?;
    let stage = stage_root(token)?;
    let staged: StagedIssue = read(&stage, "staged-issue.json", MAX_INDEX_BYTES as u64, &cancel)?;
    ensure!(
        staged.schema_version == "greenlens.staged-report-issue.v1"
            && staged.project_id == project.id,
        "Staged issue belongs to another project"
    );
    let source = stage.join("products");
    let inspection = inspect_directory(&source, &staged.summary, &cancel)?;
    let mut candidate = project.clone();
    if let Some(existing) = issues(&candidate)
        .iter()
        .find(|s| s.id == staged.summary.id)
    {
        ensure!(
            existing == &staged.summary,
            "Existing issue identity conflict; preserved products were not overwritten"
        );
    } else {
        let index = candidate
            .report_issues
            .get_or_insert_with(|| ReportIssueIndex {
                schema_version: INDEX_SCHEMA.into(),
                issues: vec![],
            });
        ensure!(
            index.issues.len() < MAX_ISSUES,
            "Issue history capacity exceeded; no history was pruned"
        );
        index.issues.push(staged.summary.clone());
    }
    validate_index(&candidate)?;
    ensure!(
        study_revisions::selected(&candidate, &staged.summary.scope_revision_sha256)?
            == &inspection.content.scope_revision,
        "Staged scope revision is not retained unchanged in the reconciled document"
    );
    let _lock = WriterLock::acquire(root)?;
    check_revision(root, expected_revision)?;
    crate::validate_project(&candidate)?;
    preflight_project(root, &candidate, Some((&staged.summary, &source)), &cancel)?;
    let members = bound_members(&source, &inspection)?;
    persistence::promote_issue_bound(&source, root, &staged.summary.id, &members, &mut |n| {
        checkpoint(&cancel)?;
        crate::project_open::checkpoint(n)
    })?;
    let mut view = crate::project_open::verify_import_candidate(
        root,
        &candidate,
        expected_revision,
        cancel.clone(),
        |_| {},
    )?;
    checkpoint(&cancel)?;
    check_revision(root, expected_revision)?;
    view.revision = atomic_project(root, &candidate)?;
    Ok(ReportIssueResult {
        view,
        issue: inspection,
    })
}

fn add_sealed_budget(
    root: &Path,
    prefix: &str,
    budget: &mut PackageBudget,
    cancel: &AtomicBool,
) -> Result<()> {
    let seal: Checksums = read(
        root,
        &format!("{prefix}/checksums.json"),
        MAX_PRODUCT_MANIFEST_BYTES,
        cancel,
    )?;
    ensure!(
        seal.schema_version == "0.1.0"
            && !seal.sha256.is_empty()
            && seal.sha256.len() < MAX_PACKAGE_ENTRIES,
        "Invalid evidence inventory for package preflight"
    );
    for name in seal
        .sha256
        .keys()
        .map(String::as_str)
        .chain(std::iter::once("checksums.json"))
    {
        checkpoint(cancel)?;
        let name = format!("{prefix}/{name}");
        let path = regular_child(root, &name)?;
        budget.add(&name, fs::metadata(path)?.len())?;
    }
    Ok(())
}
/// Resource admission includes the complete retained project, never just the
/// newest issue. It measures actual files; integrity is checked separately.
fn preflight_project(
    root: &Path,
    project: &Project,
    staged: Option<(&ReportIssueSummary, &Path)>,
    cancel: &AtomicBool,
) -> Result<()> {
    let mut budget = PackageBudget::default();
    budget.add(
        "project.json",
        serde_json::to_vec_pretty(project)?.len() as u64 + 1,
    )?;
    let mut prefixes = BTreeSet::new();
    for r in &project.runs {
        prefixes.insert(r.path.clone());
    }
    if let Some(e) = &project.climate {
        for r in &e.runs {
            prefixes.insert(r.path.clone());
        }
        for w in &e.weather {
            prefixes.insert(w.path.clone());
        }
    }
    if let Some(e) = &project.energy {
        for r in &e.runs {
            prefixes.insert(r.path.clone());
        }
        for w in &e.weather {
            prefixes.insert(w.path.clone());
        }
        for a in &e.attachments {
            prefixes.insert(format!("attachments/{}", a.id));
        }
    }
    if let Some(e) = &project.architecture {
        for a in &e.assets {
            prefixes.insert(a.path.clone());
        }
    }
    for prefix in prefixes {
        add_sealed_budget(root, &prefix, &mut budget, cancel)?;
    }
    for name in crate::energy_sensitivity_workflow::evidence_members(root, project)? {
        checkpoint(cancel)?;
        budget.add(&name, fs::metadata(regular_child(root, &name)?)?.len())?;
    }
    for s in issues(project) {
        let directory = if staged.is_some_and(|(v, _)| v.id == s.id) {
            staged.unwrap().1.to_path_buf()
        } else {
            root.join(relative_path(&s.path)?)
        };
        let inspected = inspect_directory(&directory, s, cancel)?;
        for p in &inspected.products.products {
            budget.add(&format!("{}/{}", s.path, p.path), p.bytes)?;
        }
        for n in [
            "issue-products.json",
            "issue-runtime.json",
            "checksums.json",
        ] {
            budget.add(
                &format!("{}/{}", s.path, n),
                fs::metadata(regular_child(&directory, n)?)?.len(),
            )?;
        }
    }
    Ok(())
}

/// Regeneration writes a new destination only after deterministic product
/// hashes match the original issue. Existing issued products are never edited.
pub fn regenerate(
    root: &Path,
    project: &Project,
    id: &str,
    destination: &Path,
    typst: Option<&TypstConfig>,
    cancel: Arc<AtomicBool>,
) -> Result<EnergyReportArtifacts> {
    ensure!(
        !destination.exists(),
        "Regeneration requires a new empty destination; preserved issues were not overwritten"
    );
    let prepared = crate::project_open::inspect_evidence(
        root,
        cancel.clone(),
        |_| {},
        || {
            let inspected = inspect_impl(root, project, id, &cancel)?;
            inspected.content.ensure_supported_regeneration()?;
            let mut weather_locations = BTreeMap::new();
            for case in [
                &inspected.content.workbench.base.baseline,
                &inspected.content.workbench.base.proposed,
            ] {
                let run = crate::energy_services::selected_run(root, project, &case.run_id)?;
                validate_case_record(case, &run)?;
                validate_case_location(root, case, &mut weather_locations, &cancel)?;
            }
            let drawings = figures(
                root,
                project,
                &inspected.content.scope_revision,
                &inspected.content.workbench,
                Some(&inspected.content.figures),
                &cancel,
            )?;
            Ok((
                issuance::for_regeneration(inspected.content, drawings)?,
                inspected.products,
            ))
        },
    )?;
    ensure!(!prepared.0.content.include_pdf || typst.is_some(),"Exact PDF issue regeneration requires the pinned Typst/fonts; inspect preserved PDFs while unavailable");
    let token = Uuid::new_v4().to_string();
    let stage = stage_root(&token)?.join("regenerated");
    let receipt = issuance::render_issue(
        &prepared.0,
        &stage,
        if prepared.0.content.include_pdf {
            typst
        } else {
            None
        },
        &cancel,
    )?;
    let products = manifest_from_artifacts(&prepared.0.content, &receipt)?;
    ensure!(products.products_sha256==prepared.1.products_sha256,"Regenerated product hashes differ; original issue preserved and candidate retained externally for diagnosis");
    // Destination is explicitly chosen by the caller. Use bounded copies after
    // exact comparison; a cancellation leaves an unadvertised incomplete folder.
    fs::create_dir_all(destination)?;
    for p in &products.products {
        checkpoint(&cancel)?;
        let bytes =
            read_evidence_bytes_checked(&regular_child(&stage, &p.path)?, p.bytes, &mut |n| {
                checkpoint(&cancel)?;
                crate::project_open::checkpoint(n)
            })?;
        ensure!(
            bytes_hash(&bytes) == p.sha256,
            "Regeneration changed during copy"
        );
        let target = destination.join(relative_path(&p.path)?);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        use std::io::Write;
        let mut output = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(target)?;
        output.write_all(&bytes)?;
        output.sync_all()?;
    }
    write_json(&destination.join("issue-products.json"), &products)?;
    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    // Original arithmetic/rendering fixture only. This creates no accepted
    // engine record and cannot establish real simulation/native acceptance.
    fn sealed_fixture(directory: &Path) -> (ReportIssueSummary, IssueProductManifest) {
        let (prepared, _) = issuance::tests::fixture();
        let receipts =
            issuance::render_issue(&prepared, directory, None, &AtomicBool::new(false)).unwrap();
        let products = manifest_from_artifacts(&prepared.content, &receipts).unwrap();
        write_json(&directory.join("issue-products.json"), &products).unwrap();
        write_json(
            &directory.join("issue-runtime.json"),
            &IssueRuntimeReceipt {
                schema_version: "greenlens.report-issue-runtime.v1".into(),
                content_sha256: prepared.content.content_sha256.clone(),
                issued_utc: "2026-01-01T00:00:00Z".into(),
                renderer_status: receipts.renderer_status,
                process_count: 0,
                elapsed_ms: 0,
                peak_working_set_bytes: 0,
                peak_scratch_bytes: 0,
                process_trees_empty: true,
            },
        )
        .unwrap();
        let s = make_summary(&prepared.content, &products, directory).unwrap();
        reseal(directory, &products);
        (s, products)
    }
    fn reseal(directory: &Path, products: &IssueProductManifest) {
        let mut names = products
            .products
            .iter()
            .map(|p| p.path.as_str())
            .collect::<Vec<_>>();
        names.extend(["issue-products.json", "issue-runtime.json"]);
        seal(directory, &names).unwrap();
    }
    #[test]
    fn immutable_products_are_readable_after_copy_and_resealed_tampering_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let (s, products) = sealed_fixture(&source);
        let original = inspect_directory(&source, &s, &AtomicBool::new(false)).unwrap();
        let copy = root.path().join("copy");
        fs::create_dir(&copy).unwrap();
        promote_issue_bound(
            &source,
            &copy,
            &s.id,
            &bound_members(&source, &original).unwrap(),
            &mut |_| Ok(()),
        )
        .unwrap();
        let copied = inspect_directory(&copy.join(&s.path), &s, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            copied.products.products_sha256,
            original.products.products_sha256
        );
        let before = fs::read(source.join("issue-content.json")).unwrap();
        let target = &products
            .products
            .iter()
            .find(|p| p.path.ends_with(".html"))
            .unwrap()
            .path;
        fs::write(copy.join(&s.path).join(target), b"changed product").unwrap();
        reseal(&copy.join(&s.path), &products);
        assert!(inspect_directory(&copy.join(&s.path), &s, &AtomicBool::new(false)).is_err());
        assert_eq!(fs::read(source.join("issue-content.json")).unwrap(), before);
        // A runtime receipt is separate from content identity, but remains an
        // immutable issued member. Resealing it cannot alter the bound bytes.
        fs::write(source.join("issue-runtime.json"), b"{}").unwrap();
        reseal(&source, &products);
        assert!(inspect_directory(&source, &s, &AtomicBool::new(false))
            .unwrap_err()
            .to_string()
            .contains("receipt bytes changed"));
    }
    #[test]
    fn cancelled_product_verification_and_promotion_never_advertise_an_issue() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let (s, _) = sealed_fixture(&source);
        assert!(inspect_directory(&source, &s, &AtomicBool::new(true))
            .unwrap_err()
            .to_string()
            .contains("cancelled"));
        let inspected = inspect_directory(&source, &s, &AtomicBool::new(false)).unwrap();
        let destination = root.path().join("destination");
        fs::create_dir(&destination).unwrap();
        let mut bytes = 0;
        let error = promote_issue_bound(
            &source,
            &destination,
            &s.id,
            &bound_members(&source, &inspected).unwrap(),
            &mut |n| {
                bytes += n;
                ensure!(bytes < 128, "Synthetic cooperative cancellation");
                Ok(())
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("cancellation"));
        assert!(!destination.join(&s.path).exists());
        assert!(!destination.join("project.json").exists());
        inspect_directory(&source, &s, &AtomicBool::new(false)).unwrap();
    }
    #[test]
    fn resealed_inventory_cannot_omit_reports_or_substitute_frozen_figures() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("products");
        let (original, products) = sealed_fixture(&directory);
        let content: IssueContent = read_json(&directory.join("issue-content.json")).unwrap();
        for changed_member in [
            "executive.html",
            "aligned-view-001.svg",
            "scope-revision.json",
        ] {
            let mut changed = products.clone();
            if changed_member == "executive.html" {
                changed.products.retain(|p| p.path != changed_member);
            } else {
                let bytes = b"resealed but inconsistent with frozen content";
                fs::write(directory.join(changed_member), bytes).unwrap();
                let entry = changed
                    .products
                    .iter_mut()
                    .find(|p| p.path == changed_member)
                    .unwrap();
                entry.bytes = bytes.len() as u64;
                entry.sha256 = bytes_hash(bytes);
            }
            changed.products_sha256 = changed.identity().unwrap();
            write_json(&directory.join("issue-products.json"), &changed).unwrap();
            reseal(&directory, &changed);
            // Even a coherently changed outer index cannot make a new issue
            // whose products contradict its frozen content contract.
            let updated = make_summary(&content, &changed, &directory).unwrap();
            assert_ne!(updated.id, original.id);
            let mut false_old_identity = updated.clone();
            false_old_identity.id = original.id.clone();
            false_old_identity.path = original.path.clone();
            assert!(validate_summary(&false_old_identity).is_err());
            let error = inspect_directory(&directory, &updated, &AtomicBool::new(false))
                .unwrap_err()
                .to_string();
            assert!(
                error.contains("product family") || error.contains("frozen content"),
                "{changed_member}: {error}"
            );
        }
        // A requested native output family cannot be represented by the
        // HTML-only fixture. This is structural verification, not a PDF run.
        let mut native_content = content;
        native_content.include_pdf = true;
        assert!(validate_product_bindings(&native_content, &products)
            .unwrap_err()
            .to_string()
            .contains("PNG page"));
    }
    #[test]
    fn weather_source_and_station_labels_cannot_be_rebound_by_report_metadata() {
        // Metadata extraction/comparison only; no fabricated engine record is
        // admitted and this fixture makes no execution or qualification claim.
        let (issue, _) = issuance::tests::fixture();
        let case = issue.content.workbench.base.baseline;
        let asset = crate::energy_weather::EnergyWeatherAsset {
            schema_version: crate::energy_weather::ASSET_SCHEMA.into(),
            binding: case.weather_summary.binding.clone(),
            source: crate::weather::WeatherSource {
                citation: case.weather_summary.source.citation.clone(),
                source_url: None,
                license: "Apache-2.0".into(),
                rights_note: "Original synthetic metadata fixture only".into(),
                synthetic_non_climatic: case.weather_summary.source.synthetic,
            },
        };
        validate_weather_source(&case.weather_summary, &asset).unwrap();
        let mut changed = case.weather_summary.clone();
        changed.source.synthetic = !changed.source.synthetic;
        assert!(validate_weather_source(&changed, &asset).is_err());
        changed = case.weather_summary.clone();
        changed.source.citation = "Different purported source".into();
        assert!(validate_weather_source(&changed, &asset).is_err());
        let root = tempfile::tempdir().unwrap();
        let weather = root.path().join("runs").join(&case.run_id).join("weather");
        fs::create_dir_all(&weather).unwrap();
        write_json(
            &weather.join("weather.json"),
            &serde_json::json!({
                "location": case.weather_summary.location,
                "hours": [{"metadata_extraction_fixture_only": true}]
            }),
        )
        .unwrap();
        let mut locations = BTreeMap::new();
        validate_case_location(root.path(), &case, &mut locations, &AtomicBool::new(false))
            .unwrap();
        let mut changed_case = case.clone();
        changed_case.weather_summary.location.station = "Another station".into();
        assert!(validate_case_location(
            root.path(),
            &changed_case,
            &mut locations,
            &AtomicBool::new(false)
        )
        .is_err());
        changed_case = case.clone();
        changed_case.weather_summary.location.city = "Another location".into();
        assert!(validate_case_location(
            root.path(),
            &changed_case,
            &mut locations,
            &AtomicBool::new(false)
        )
        .is_err());
        validate_case_location(root.path(), &case, &mut locations, &AtomicBool::new(false))
            .unwrap();
        assert_eq!(locations.len(), 1);
        assert!(
            validate_case_location(root.path(), &case, &mut locations, &AtomicBool::new(true))
                .is_err()
        );
    }
    fn product(path: &str, size: u64) -> ReportArtifact {
        ReportArtifact {
            path: path.into(),
            bytes: size,
            sha256: "a".repeat(64),
        }
    }
    fn inventory(products: Vec<ReportArtifact>) -> IssueProductManifest {
        let mut value = IssueProductManifest {
            schema_version: PRODUCT_SCHEMA.into(),
            content_sha256: "b".repeat(64),
            products_sha256: String::new(),
            products,
        };
        value.products_sha256 = value.identity().unwrap();
        value
    }
    #[test]
    fn product_limits_use_shared_archive_bounds_and_reject_unsafe_or_duplicate_paths() {
        let good = inventory(vec![product("issue-content.json", 1)]);
        good.validate().unwrap();
        for path in [
            "../outside",
            "C:/outside",
            "A\\outside",
            "checksums.json",
            "issue-products.json",
            "report-artifacts.json",
        ] {
            assert!(
                inventory(vec![product(path, 1)]).validate().is_err(),
                "{path}"
            );
        }
        assert!(inventory(vec![
            product("issue-content.json", 1),
            product("issue-content.json", 1)
        ])
        .validate()
        .is_err());
        assert!(inventory(vec![product(
            "issue-content.json",
            MAX_EVIDENCE_FILE_BYTES + 1
        )])
        .validate()
        .is_err());
        assert!(inventory(vec![
            product("a.bin", MAX_EVIDENCE_FILE_BYTES),
            product("b.bin", MAX_EVIDENCE_FILE_BYTES),
            product("c.bin", MAX_EVIDENCE_FILE_BYTES),
            product("d.bin", MAX_EVIDENCE_FILE_BYTES),
            product("issue-content.json", 1)
        ])
        .validate()
        .is_err());
        let mut excess = (0..MAX_PRODUCTS)
            .map(|i| product(&format!("a{i:03}.txt"), 1))
            .collect::<Vec<_>>();
        excess.push(product("issue-content.json", 1));
        assert!(inventory(excess).validate().is_err());
    }
    #[test]
    fn summary_identity_and_runtime_privacy_are_explicit() {
        let root = tempfile::tempdir().unwrap();
        let (s, _) = sealed_fixture(&root.path().join("products"));
        let mut changed = s.clone();
        changed.path = "report-issues/../outside".into();
        assert!(validate_summary(&changed).is_err());
        changed = s.clone();
        changed.id = "a".repeat(64);
        assert!(validate_summary(&changed).is_err());
        let receipt: serde_json::Value =
            read_json(&root.path().join("products/issue-runtime.json")).unwrap();
        let text = serde_json::to_string(&receipt).unwrap();
        assert!(
            !text.contains("executable")
                && !text.contains("arguments")
                && !text.contains("LOCALAPPDATA")
                && !text.contains("C:")
        );
        assert_eq!(
            serde_json::from_value::<IssueRuntimeReceipt>(receipt)
                .unwrap()
                .process_count,
            0
        );
    }

    #[test]
    fn older_undo_snapshot_cannot_delete_or_rebind_issued_history() {
        let root = tempfile::tempdir().unwrap();
        let (summary, _) = sealed_fixture(&root.path().join("products"));
        // Pure history policy only: no fabricated run is accepted or processed.
        let mut previous = crate::example();
        previous.report_issues = Some(ReportIssueIndex {
            schema_version: INDEX_SCHEMA.into(),
            issues: vec![summary],
        });
        let mut next = previous.clone();
        next.name = "Later descriptive title".into();
        validate_preserved(&previous, &next).unwrap();
        next.report_issues = None;
        assert!(validate_preserved(&previous, &next)
            .unwrap_err()
            .to_string()
            .contains("cannot be removed"));
        next = previous.clone();
        next.report_issues.as_mut().unwrap().issues[0].products_sha256 = "f".repeat(64);
        assert!(validate_preserved(&previous, &next).is_err());
        next = previous.clone();
        let mut appended = next.report_issues.as_ref().unwrap().issues[0].clone();
        appended.id = "e".repeat(64);
        next.report_issues.as_mut().unwrap().issues.push(appended);
        // Pure preservation permits append; ordinary validate_index separately
        // enforces every new identity, scope/run reference and capacity.
        validate_preserved(&previous, &next).unwrap();
    }
}
