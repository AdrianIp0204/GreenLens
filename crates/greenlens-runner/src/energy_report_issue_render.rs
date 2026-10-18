//! Immutable issuance content and owned rendering, independent of persistence.
//! Runtime process receipts do not enter deterministic content identity. The
//! application service must verify retained run/reference bytes before preparing
//! content; accepting a serialized report never establishes evidence integrity.
use super::{old, Document, EnergyReportArtifacts, Section, TypstConfig};
use anyhow::{ensure, Context, Result};
use greenlens_core::{
    architectural_scope::{assess_scope, scope_content_sha256, ScopeAssessment},
    canonical_hash,
    energy::{assess_energy_sensitivity, validate_thermal_model, EnergyReportWorkbenchData},
    study_revision::{validate_revision, ScopeRevision},
};
use old::architectural::{
    aligned_views::{
        AlignedFigure, AlignedFigureDefinition, ALIGNED_FIGURE_METHOD, ALIGNED_FIGURE_SCHEMA,
    },
    SensitivityReportEvidence, STORAGE_DISCLOSURE,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub const ISSUE_CONTENT_SCHEMA: &str = "greenlens.energy-report-issue-content.v1";
pub const ISSUE_RENDERER_METHOD: &str = "greenlens.energy-report-issue-renderer/1";
pub const MAX_ISSUE_CONTENT_BYTES: usize = 48 * 1024 * 1024;
pub(super) const TEMPLATE: &str = include_str!("../../../reports/energy/issue.typ");
const DEPENDENCY: &str = include_str!("../../../dependencies/typst.json");
const MAX_PRESENTED_ROWS: usize = 80;
#[path = "energy_report_issue_render_v2.rs"]
mod v2;
#[path = "energy_report_issue_render_v3.rs"]
mod v3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IssueRendererIdentity {
    pub method: String,
    pub document_schema: String,
    pub html_method: String,
    pub figure_method: String,
    pub template_sha256: String,
    pub typst_version: String,
    pub typst_executable_sha256: String,
    pub font_sha256: BTreeMap<String, String>,
    pub pdf_creation_timestamp: u64,
    pub performance_png_ppi: u32,
    pub system_and_embedded_fonts_disabled: bool,
}

/// Pure supported identity. Actual binary/font bytes are checked by the existing
/// supervised renderer when PDF/PNG output is requested; this is not readiness.
pub fn current_renderer_identity() -> Result<IssueRendererIdentity> {
    let pin: serde_json::Value = serde_json::from_str(DEPENDENCY)?;
    let string = |value: &serde_json::Value| {
        value
            .as_str()
            .map(String::from)
            .context("Missing audited renderer identity")
    };
    let mut fonts = BTreeMap::new();
    for font in pin["fonts"]["files"]
        .as_array()
        .context("Missing font pins")?
    {
        fonts.insert(string(&font["name"])?, string(&font["sha256"])?);
    }
    Ok(IssueRendererIdentity {
        method: ISSUE_RENDERER_METHOD.into(),
        document_schema: "greenlens.energy-document.v0.7".into(),
        html_method: "greenlens.owned-energy-html/1".into(),
        figure_method: ALIGNED_FIGURE_METHOD.into(),
        template_sha256: old::sha(TEMPLATE.as_bytes()),
        typst_version: string(&pin["version"])?,
        typst_executable_sha256: string(&pin["members"]["typst.exe"]["sha256"])?,
        font_sha256: fonts,
        pdf_creation_timestamp: 0,
        performance_png_ppi: 96,
        system_and_embedded_fonts_disabled: true,
    })
}

/// New issuance uses the current presentation; the v1 identity above and its
/// rendering algorithm remain callable for exact historical regeneration.
pub fn latest_renderer_identity() -> Result<IssueRendererIdentity> {
    let mut identity = current_renderer_identity()?;
    identity.method = v3::METHOD.into();
    identity.document_schema = v3::DOCUMENT_SCHEMA.into();
    identity.html_method = v3::HTML_METHOD.into();
    identity.template_sha256 = old::sha(v3::TEMPLATE.as_bytes());
    Ok(identity)
}
fn renderer_v2_identity() -> Result<IssueRendererIdentity> {
    let mut identity = current_renderer_identity()?;
    identity.method = v2::METHOD.into();
    identity.document_schema = v2::DOCUMENT_SCHEMA.into();
    identity.html_method = v2::HTML_METHOD.into();
    identity.template_sha256 = old::sha(v2::TEMPLATE.as_bytes());
    Ok(identity)
}
fn supported_renderer_identity(method: &str) -> Result<IssueRendererIdentity> {
    match method {
        ISSUE_RENDERER_METHOD => current_renderer_identity(),
        v2::METHOD => renderer_v2_identity(),
        v3::METHOD => latest_renderer_identity(),
        _ => anyhow::bail!("Unsupported issue renderer recipe"),
    }
}

/// Full, deterministic supplementary products required only by the new recipe.
/// Product admission recomputes these bytes from frozen issue content.
pub fn required_supplement_products(content: &IssueContent) -> Result<Vec<(String, Vec<u8>)>> {
    match content.renderer.method.as_str() {
        v2::METHOD => v2::products(content),
        v3::METHOD => v3::products(content),
        _ => Ok(vec![]),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssueContent {
    pub schema_version: String,
    pub content_sha256: String,
    /// Frozen canonical data and issuance-time assessments, never active inputs.
    pub workbench: EnergyReportWorkbenchData,
    pub scope_revision: ScopeRevision,
    pub scope_assessment: ScopeAssessment,
    /// Absence is a material report disclosure. Other pairs cannot be rebound.
    pub sensitivity: Option<SensitivityReportEvidence>,
    pub figures: Vec<AlignedFigureDefinition>,
    pub renderer: IssueRendererIdentity,
    pub include_pdf: bool,
}
impl IssueContent {
    pub fn identity(&self) -> Result<String> {
        let mut unsigned = self.clone();
        unsigned.content_sha256.clear();
        Ok(canonical_hash(&(ISSUE_CONTENT_SCHEMA, unsigned))?)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == ISSUE_CONTENT_SCHEMA,
            "Unsupported report issue content schema"
        );
        // Each bounded component is validated before another serialized copy.
        self.workbench.validate()?;
        validate_revision(&self.scope_revision)?;
        ensure!(
            self.figures.len() <= 8,
            "At most eight retained aligned issue views"
        );
        let cases = [&self.workbench.base.baseline, &self.workbench.base.proposed];
        let mut models = BTreeMap::new();
        for case in cases {
            let snapshot = self
                .scope_revision
                .designs
                .get(&case.id)
                .context("Scope revision does not include both selected report designs")?;
            ensure!(
                snapshot.model == case.model && snapshot.calendar == case.study.calendar,
                "Scope revision analytical inputs differ from selected immutable report cases"
            );
            if let Some(study) = &snapshot.study {
                ensure!(
                    study == &case.study,
                    "Scope revision study differs from the selected immutable case"
                );
            }
            models.insert(
                case.id.clone(),
                validate_thermal_model(&case.model, &case.study.calendar)?,
            );
        }
        ensure!(
            self.scope_assessment
                == assess_scope(
                    &self.scope_revision.scope,
                    &self.scope_revision.reference.manifest,
                    &models
                )?,
            "Issue scope assessment differs from retained reference and models"
        );
        ensure!(
            self.figures.len() == self.scope_revision.scope.report_views.len(),
            "Every accepted retained view requires an explicit figure or unavailable figure"
        );
        for (figure, view) in self
            .figures
            .iter()
            .zip(&self.scope_revision.scope.report_views)
        {
            let binding = &figure.request;
            let case = cases
                .iter()
                .find(|case| case.id == view.design_id)
                .context("Figure design is absent from selected pair")?;
            ensure!(figure.schema_version == ALIGNED_FIGURE_SCHEMA && figure.method == ALIGNED_FIGURE_METHOD && figure.definition_sha256 == figure.identity()?
                && binding.view == *view && binding.scope_revision_sha256 == self.scope_revision.revision_sha256
                && binding.scope_content_sha256 == scope_content_sha256(&self.scope_revision.scope)?
                && binding.selected_run_id == case.run_id && binding.selected_record_sha256 == case.record_sha256,
                "Figure must bind the exact issue scope revision, retained view and selected run/record");
        }
        if let Some(sensitivity) = &self.sensitivity {
            validate_sensitivity(sensitivity, &self.workbench)?;
        }
        let supported = supported_renderer_identity(&self.renderer.method)?;
        ensure!(self.renderer.document_schema == supported.document_schema
            && self.renderer.html_method == supported.html_method && self.renderer.figure_method == ALIGNED_FIGURE_METHOD
            && self.renderer.pdf_creation_timestamp == 0 && self.renderer.performance_png_ppi == 96 && self.renderer.system_and_embedded_fonts_disabled,
            "Unsupported issue renderer recipe; historical inspection requires its recognized reader");
        for value in std::iter::once(&self.renderer.template_sha256)
            .chain(std::iter::once(&self.renderer.typst_executable_sha256))
            .chain(self.renderer.font_sha256.values())
        {
            ensure!(
                value.len() == 64
                    && value
                        .bytes()
                        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
                "Invalid renderer digest"
            );
        }
        ensure!(
            self.renderer.font_sha256.len() == 2 && self.renderer.typst_version.len() <= 64,
            "Invalid bounded renderer inventory"
        );
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_ISSUE_CONTENT_BYTES,
            "Issue content exceeds 48 MiB; split the study or retain fewer views before issuance"
        );
        ensure!(
            self.content_sha256 == self.identity()?,
            "Immutable report issue content identity differs"
        );
        Ok(())
    }
    pub fn ensure_supported_regeneration(&self) -> Result<()> {
        self.validate()?;
        ensure!(self.renderer == supported_renderer_identity(&self.renderer.method)?, "Exact regeneration unavailable: this issue's template/renderer/fonts differ from the installed supported recipe. Inspect preserved products; do not replace the old issue");
        Ok(())
    }
}

pub(crate) fn validate_sensitivity(
    e: &SensitivityReportEvidence,
    data: &EnergyReportWorkbenchData,
) -> Result<()> {
    e.plan.validate()?;
    crate::energy_sensitivity_workflow::validate_references(std::slice::from_ref(
        &e.receipt_reference,
    ))?;
    ensure!(
        e.result.schema_version == "greenlens.energy-sensitivity-result.v1"
            && e.result.plan_sha256 == e.plan.identity()?
            && e.receipt_reference.id == e.session_id
            && e.receipt_reference.plan_sha256 == e.result.plan_sha256,
        "Sensitivity report receipt/plan identity differs"
    );
    ensure!(
        e.result.assessment
            == assess_energy_sensitivity(
                &e.plan.declaration,
                &e.result.assessment.runs,
                &e.result.assessment.interruptions
            )?,
        "Sensitivity criteria differ from the exact retained declaration/outcomes"
    );
    for (slot, case) in [("A10", &data.base.baseline), ("B10", &data.base.proposed)] {
        ensure!(e.result.assessment.runs.iter().any(|r|r.slot_id==slot && r.run_id==case.run_id && r.record_sha256==case.record_sha256
            && r.physical_input_sha256==case.physical_input_sha256 && r.method==case.method && r.totals==case.totals),
            "Sensitivity evidence belongs to another pair or output; omit it explicitly and disclose missing exact-pair evidence");
    }
    Ok(())
}

/// Only owned figure buffers enter rendering. Serialized SVG content is not a
/// supported import path. Regeneration reconstructs figures from admitted chunks.
#[derive(Debug, Clone)]
pub struct IssueReport {
    pub content: IssueContent,
    figures: Vec<(String, Vec<u8>)>,
}
pub fn prepare(
    workbench: EnergyReportWorkbenchData,
    scope_revision: ScopeRevision,
    sensitivity: Option<SensitivityReportEvidence>,
    figures: Vec<AlignedFigure>,
    include_pdf: bool,
) -> Result<IssueReport> {
    let models = [&workbench.base.baseline, &workbench.base.proposed]
        .into_iter()
        .map(|c| {
            Ok((
                c.id.clone(),
                validate_thermal_model(&c.model, &c.study.calendar)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let scope_assessment = assess_scope(
        &scope_revision.scope,
        &scope_revision.reference.manifest,
        &models,
    )?;
    let mut content = IssueContent {
        schema_version: ISSUE_CONTENT_SCHEMA.into(),
        content_sha256: String::new(),
        workbench,
        scope_revision,
        scope_assessment,
        sensitivity,
        figures: figures.iter().map(|f| f.definition.clone()).collect(),
        renderer: latest_renderer_identity()?,
        include_pdf,
    };
    content.content_sha256 = content.identity()?;
    for_regeneration(content, figures)
}
pub fn for_regeneration(content: IssueContent, figures: Vec<AlignedFigure>) -> Result<IssueReport> {
    content.ensure_supported_regeneration()?;
    ensure!(
        figures.len() == content.figures.len(),
        "Regeneration is missing an issued figure"
    );
    let mut buffers = Vec::new();
    for (i, (figure, expected)) in figures.into_iter().zip(&content.figures).enumerate() {
        ensure!(
            figure.definition == *expected && old::sha(&figure.svg) == expected.svg_sha256,
            "Regenerated figure differs from the immutable issue; the original issue is preserved"
        );
        buffers.push((format!("aligned-view-{:03}.svg", i + 1), figure.svg));
    }
    Ok(IssueReport {
        content,
        figures: buffers,
    })
}
pub fn render_issue(
    issue: &IssueReport,
    destination: &Path,
    typst: Option<&TypstConfig>,
    cancel: &AtomicBool,
) -> Result<EnergyReportArtifacts> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Issue rendering cancelled before verification"
    );
    issue.content.ensure_supported_regeneration()?;
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Issue rendering cancelled during verification"
    );
    ensure!(issue.content.include_pdf==typst.is_some(),"Regeneration must preserve the issued output family; install compatible Typst for PDF/PNG or issue an explicit HTML-only study");
    super::render_internal(
        &issue.content.workbench,
        destination,
        typst,
        cancel,
        None,
        Some(issue),
    )
}

impl IssueReport {
    pub(super) fn template(&self) -> &'static str {
        if self.content.renderer.method == v3::METHOD {
            v3::TEMPLATE
        } else if self.content.renderer.method == v2::METHOD {
            v2::TEMPLATE
        } else {
            TEMPLATE
        }
    }
    pub(super) fn artifact_schema(&self) -> &'static str {
        if self.content.renderer.method == v3::METHOD {
            "greenlens.energy-report-artifacts.v0.8-review"
        } else if self.content.renderer.method == v2::METHOD {
            "greenlens.energy-report-artifacts.v0.8"
        } else {
            "greenlens.energy-report-artifacts.v0.7"
        }
    }
    pub(super) fn html(
        &self,
        document: &Document,
        generated: &[(&str, Vec<u8>)],
    ) -> Result<String> {
        if self.content.renderer.method == v3::METHOD {
            v3::html(document, generated)
        } else if self.content.renderer.method == v2::METHOD {
            v2::html(document, generated)
        } else {
            old::html(document, generated)
        }
    }
    pub(super) fn augment(
        &self,
        data: &EnergyReportWorkbenchData,
        names: &[&str],
        docs: &mut [Document],
        generated: &mut Vec<(String, Vec<u8>)>,
        cancel: &AtomicBool,
    ) -> Result<()> {
        if self.content.renderer.method == v3::METHOD {
            v3::augment(self, data, names, docs, generated, cancel)
        } else if self.content.renderer.method == v2::METHOD {
            v2::augment(self, data, names, docs, generated, cancel)
        } else {
            self.augment_v1(data, names, docs, generated, cancel)
        }
    }
    fn augment_v1(
        &self,
        data: &EnergyReportWorkbenchData,
        names: &[&str],
        docs: &mut [Document],
        generated: &mut Vec<(String, Vec<u8>)>,
        cancel: &AtomicBool,
    ) -> Result<()> {
        self.content.ensure_supported_regeneration()?;
        ensure!(
            data == &self.content.workbench,
            "Active report values cannot replace an issued dataset"
        );
        ensure!(
            self.figures.len() == self.content.figures.len(),
            "Issued figure buffers are incomplete"
        );
        for ((name, bytes), definition) in self.figures.iter().zip(&self.content.figures) {
            ensure!(
                !cancel.load(Ordering::Relaxed),
                "Issue figure preparation cancelled"
            );
            ensure!(
                old::sha(bytes) == definition.svg_sha256,
                "Owned issued figure buffer changed"
            );
            generated.push((name.clone(), bytes.clone()));
        }
        generated.push((
            "issue-content.json".into(),
            serde_json::to_vec_pretty(&self.content)?,
        ));
        generated.push((
            "scope-revision.json".into(),
            serde_json::to_vec_pretty(&self.content.scope_revision)?,
        ));
        generated.push((
            "input-provenance.json".into(),
            serde_json::to_vec_pretty(&self.content.scope_revision.provenance)?,
        ));
        generated.push((
            "input-provenance.csv".into(),
            self.provenance_csv(cancel)?.into_bytes(),
        ));
        generated.push((
            "aligned-figures.json".into(),
            serde_json::to_vec_pretty(&self.content.figures)?,
        ));
        if let Some(e) = &self.content.sensitivity {
            generated.push((
                "issue-sensitivity.json".into(),
                serde_json::to_vec_pretty(e)?,
            ));
        }
        for (name, doc) in names.iter().zip(docs) {
            ensure!(
                !cancel.load(Ordering::Relaxed),
                "Issue document preparation cancelled"
            );
            let mut sections = vec![self.summary()?];
            if matches!(*name, "technical" | "input-appendix") {
                for ((file, _), figure) in self.figures.iter().zip(&self.content.figures) {
                    sections.push(Section::figure(
                        &format!(
                            "Aligned architectural / analytical view — {}",
                            figure.request.view.title
                        ),
                        file,
                        figure.caption.clone(),
                    ));
                    if let Some(reason) = &figure.unavailable_reason {
                        sections.push(Section::prose(
                            "Figure depiction limit",
                            vec![reason.clone()],
                        ));
                    }
                }
            }
            if *name == "input-appendix" {
                sections.extend(self.detail_sections()?);
            }
            sections.append(&mut doc.sections);
            doc.sections = sections;
            old::architectural::clarify_sections(&mut doc.sections);
            doc.schema_version = "greenlens.energy-document.v0.7";
            doc.dataset_sha256 = self.content.content_sha256.clone();
        }
        Ok(())
    }
    fn summary(&self) -> Result<Section> {
        let d = &self.content;
        let p = &d.scope_revision.provenance;
        let mut paragraphs=vec![format!("Immutable scope revision {}. {}: {}",d.scope_revision.revision_sha256,d.scope_revision.scope.title,d.scope_revision.scope.purpose),
            format!("Baseline run {} / record {}; proposed run {} / record {}. These are retained issue snapshots; later editor changes do not replace them.",d.workbench.base.baseline.run_id,d.workbench.base.baseline.record_sha256,d.workbench.base.proposed.run_id,d.workbench.base.proposed.record_sha256),
            format!("Documentation completeness: {}. Unreviewed {} / missing {} / conflicting {} supported-input provenance rows. {}",if p.documentation_complete{"all checklist rows documented or explicitly accounted for"}else{"INCOMPLETE"},p.unreviewed_count,p.missing_count,p.conflicting_count,p.disclosure),
            format!("Scope relationship at issuance: {:?}; {} unresolved mappings. Source floor-area coverage: {}.",d.scope_assessment.relationship,d.scope_assessment.unresolved_mapping_count,d.scope_assessment.coverage_percent.map(|v|format!("{v:.3}% of declared denominator")).unwrap_or_else(||"unknown; no valid denominator".into())),
            match &d.sensitivity {Some(e)=>format!("Exact selected-pair sensitivity: {}. Declaration {}. This is numerical evidence, not physical validation or scientific qualification.",if e.result.assessment.numerical_sensitivity_passed{"passed"}else{"FAILED / incomplete"},e.plan.declaration_sha256),None=>"Exact selected-pair sensitivity: MISSING. Another pair's assessment does not apply; no assessment is silently rebound.".into()},
            "Issuance preserves these products and bindings; it does not invent review, professional approval, whole-building equivalence or certification. Execution, integrity, input relationship and scientific qualification are disclosed independently in each case's assessment.".into()];
        if d.figures.is_empty() {
            paragraphs.push("No aligned report view was accepted in this scope revision; architectural depiction is unavailable. Define and review a retained view for a later issue.".into());
        }
        for f in &d.figures {
            if let Some(reason) = &f.unavailable_reason {
                paragraphs.push(format!(
                    "View {} unavailable: {reason}",
                    f.request.view.title
                ));
            }
        }
        Ok(Section::prose(
            "Issued study scope, evidence and readiness",
            paragraphs,
        ))
    }
    fn detail_sections(&self) -> Result<Vec<Section>> {
        let d = &self.content;
        let s = &d.scope_revision.scope;
        let mut sections = vec![
            Section::prose("Included analytical scope", s.included_scope.clone()),
            Section::prose(
                "Excluded analytical scope",
                if s.excluded_scope.is_empty() {
                    vec![
                        "No exclusions declared; this is not verified whole-building coverage."
                            .into(),
                    ]
                } else {
                    s.excluded_scope.clone()
                },
            ),
            Section::prose(
                "Boundary consequences and simplification",
                vec![s.boundary_consequences.clone(), STORAGE_DISCLOSURE.into()],
            ),
        ];
        sections.push(Section::table("Immutable issue identities",&["Item","Identity"],vec![
            vec!["Scope revision / parent".into(),format!("{}\n{}",d.scope_revision.revision_sha256,d.scope_revision.parent_revision_sha256.as_deref().unwrap_or("Initial accepted revision"))],
            vec!["Reference revision / geometry".into(),format!("{}\n{}",s.reference_revision_sha256,s.reference_geometry_sha256)],
            vec!["Renderer / template".into(),format!("{}\n{}",d.renderer.method,d.renderer.template_sha256)],
            vec!["Typst executable / fonts".into(),format!("{} / {}\n{}",d.renderer.typst_version,d.renderer.typst_executable_sha256,serde_json::to_string(&d.renderer.font_sha256)?)],
            vec!["Regeneration".into(),"Use retained immutable issue data, exact admitted reference chunks and supported renderer/font identities. No EnergyPlus execution is needed. Runtime receipts are separate; product hashes must match the preserved issue.".into()],
        ]));
        sections.push(Section::table(
            "Source-to-analysis mapping snapshot",
            &[
                "Occurrence",
                "Disposition",
                "Analytical targets",
                "Rationale",
            ],
            s.mappings
                .iter()
                .take(MAX_PRESENTED_ROWS)
                .map(|m| {
                    vec![
                        m.occurrence_id.clone(),
                        format!("{:?}", m.status),
                        m.targets
                            .iter()
                            .map(|t| format!("{} / {}", t.design_id, t.object_id))
                            .collect::<Vec<_>>()
                            .join("\n"),
                        m.rationale.clone(),
                    ]
                })
                .collect(),
        ));
        let p = &d.scope_revision.provenance;
        let mut provenance = Section::table(
            "Supported input provenance: source and applied values",
            &[
                "Design / object / field",
                "Applied analytical value / units",
                "Review status / source declaration",
            ],
            p.rows
                .iter()
                .take(MAX_PRESENTED_ROWS)
                .map(|r| {
                    Ok(vec![
                        format!("{} / {} / {}", r.design_id, r.object.id, r.field_path),
                        format!(
                            "{} {}",
                            serde_json::to_string(&r.applied_value)?,
                            r.units.as_deref().unwrap_or("unit not declared")
                        ),
                        format!(
                            "{:?}\n{}",
                            r.status,
                            if r.source_declarations.is_empty() {
                                "No reviewed source declaration".into()
                            } else {
                                serde_json::to_string(&r.source_declarations)?
                            }
                        ),
                    ])
                })
                .collect::<Result<_>>()?,
        );
        provenance.paragraphs.push(format!("Showing up to {MAX_PRESENTED_ROWS} of {} supported-input rows. Complete lossless values and separate source declarations are in input-provenance.json/CSV and scope-revision.json. Empty source declarations do not imply completeness.",p.rows.len()));
        sections.push(provenance);
        sections.push(Section::table(
            "Source / analytical reconciliation snapshot",
            &[
                "Design / measure",
                "Source",
                "Analytical",
                "Difference / relative",
            ],
            d.scope_assessment
                .reconciliations
                .iter()
                .map(|r| {
                    vec![
                        format!("{} / {} ({:?})", r.design_id, r.id, r.unit),
                        r.source_value
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "Unknown".into()),
                        r.analytical_value.to_string(),
                        format!(
                            "{} / {}",
                            r.analytical_minus_source
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "Undefined".into()),
                            r.relative_difference_percent
                                .map(|v| format!("{v}%"))
                                .unwrap_or_else(|| "Undefined".into())
                        ),
                    ]
                })
                .collect(),
        ));
        if let Some(e) = &d.sensitivity {
            sections.push(Section::table(
                "Exact-pair sensitivity: every declared criterion",
                &["Criterion / design", "Measured / limit", "Outcome"],
                e.result
                    .assessment
                    .criteria
                    .iter()
                    .map(|c| {
                        vec![
                            format!("{} / {}", c.id, c.alternative_id),
                            format!(
                                "{} / {}%",
                                c.measured_percent
                                    .map(|v| format!("{v}%"))
                                    .unwrap_or_else(|| "Missing".into()),
                                c.limit_percent
                            ),
                            format!(
                                "{}: {}",
                                if c.passed {
                                    "passed"
                                } else {
                                    "FAILED / incomplete"
                                },
                                c.explanation
                            ),
                        ]
                    })
                    .collect(),
            ));
        }
        Ok(sections)
    }
    fn provenance_csv(&self, cancel: &AtomicBool) -> Result<String> {
        let cell = |v: &str| format!("\"{}\"", v.replace('"', "\"\""));
        let mut rows=vec!["\"row_id\",\"design\",\"object\",\"field\",\"applied_value\",\"units\",\"status\",\"declared_source\",\"reviewed_source_declarations\"".to_owned()];
        for (i, r) in self
            .content
            .scope_revision
            .provenance
            .rows
            .iter()
            .enumerate()
        {
            if i.is_multiple_of(256) {
                ensure!(
                    !cancel.load(Ordering::Relaxed),
                    "Issue provenance preparation cancelled"
                );
            }
            rows.push(
                [
                    r.id.clone(),
                    r.design_id.clone(),
                    serde_json::to_string(&r.object)?,
                    r.field_path.clone(),
                    serde_json::to_string(&r.applied_value)?,
                    r.units.clone().unwrap_or_default(),
                    serde_json::to_string(&r.status)?,
                    serde_json::to_string(&r.declared_source)?,
                    serde_json::to_string(&r.source_declarations)?,
                ]
                .iter()
                .map(|v| cell(v))
                .collect::<Vec<_>>()
                .join(","),
            );
        }
        Ok(rows.join("\r\n") + "\r\n")
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use greenlens_core::{architectural_scope::*, reference::*, study_revision::*};
    use serde_json::json;
    use std::fs;

    fn admitted() -> AdmittedReference {
        let mut bin = Vec::new();
        for value in [[0f32, 0., 0.], [4., 0., 0.], [0., 3., 0.]]
            .iter()
            .flatten()
        {
            bin.extend(value.to_le_bytes());
        }
        let doc = json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteLength":36}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[4,3,0]}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0});
        let mut json = serde_json::to_vec(&doc).unwrap();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let mut bytes = Vec::new();
        bytes.extend(0x46546c67u32.to_le_bytes());
        bytes.extend(2u32.to_le_bytes());
        bytes.extend(((28 + json.len() + bin.len()) as u32).to_le_bytes());
        bytes.extend((json.len() as u32).to_le_bytes());
        bytes.extend(0x4e4f534au32.to_le_bytes());
        bytes.extend(json);
        bytes.extend((bin.len() as u32).to_le_bytes());
        bytes.extend(0x004e4942u32.to_le_bytes());
        bytes.extend(bin);
        admit_glb(&bytes, &ReferenceImportOptions::default(), |_| true).unwrap()
    }
    pub(crate) fn fixture() -> (IssueReport, AdmittedReference) {
        let mut original = super::super::tests::fixture();
        original.base.baseline.id = "A".into();
        original.base.proposed.id = "B".into();
        original.base.baseline.run_id = "00000000-0000-4000-8000-00000000000a".into();
        original.base.proposed.run_id = "00000000-0000-4000-8000-00000000000b".into();
        let data = EnergyReportWorkbenchData::new(
            original.base,
            original.metadata,
            original.profiles,
            original.attachments,
            original.template_sha256,
        )
        .unwrap();
        let reference = admitted();
        let r = &reference.manifest;
        let a = &data.base.baseline;
        let b = &data.base.proposed;
        let view = ReferenceReportView {
            schema_version: REFERENCE_REPORT_VIEW_SCHEMA.into(),
            id: "front".into(),
            title: "Original issued <front>".into(),
            design_id: a.id.clone(),
            model_input_sha256: a.model_input_sha256.clone(),
            reference_revision_sha256: r.revision_sha256.clone(),
            reference_geometry_sha256: r.geometry_sha256.clone(),
            eye_m: [2., -10., 1.5],
            target_m: [2., 0., 1.5],
            up: [0., 0., 1.],
            vertical_span_m: 4.,
            aspect_ratio: 1.,
            section: None,
        };
        let scope = ArchitecturalScope {
            schema_version: ARCHITECTURAL_SCOPE_SCHEMA.into(),
            title: "Synthetic retained scope".into(),
            purpose: "Original issue contract fixture, not engine evidence".into(),
            scope_kind: ScopeKind::RepresentativeStudy,
            reference_revision_sha256: r.revision_sha256.clone(),
            reference_geometry_sha256: r.geometry_sha256.clone(),
            reference_options_sha256: canonical_hash(&r.options).unwrap(),
            design_model_sha256: BTreeMap::from([
                (a.id.clone(), a.model_input_sha256.clone()),
                (b.id.clone(), b.model_input_sha256.clone()),
            ]),
            mappings: vec![ScopeMapping {
                occurrence_id: r.occurrences[0].id.clone(),
                status: MappingStatus::Mapped,
                targets: vec![AnalyticalTarget {
                    design_id: a.id.clone(),
                    object_id: a.model.zones[0].id.clone(),
                    kind: AnalyticalObjectKind::Zone,
                    model_input_sha256: a.model_input_sha256.clone(),
                }],
                rationale: "Explicit synthetic mapping".into(),
            }],
            included_scope: vec!["Bound model inputs".into()],
            excluded_scope: vec!["No measured whole-building coverage".into()],
            boundary_consequences: "Retained boundary inputs apply".into(),
            assumptions: vec![],
            reconciliations: vec![],
            coverage: ScopeCoverage::Unknown {
                reason: "No denominator".into(),
            },
            whole_building_review: None,
            report_views: vec![view],
        };
        let designs = [a, b]
            .into_iter()
            .map(|c| {
                (
                    c.id.clone(),
                    RevisionDesignSnapshot {
                        model: c.model.clone(),
                        calendar: c.study.calendar.clone(),
                        study: Some(c.study.clone()),
                    },
                )
            })
            .collect();
        let revision = initial_revision(
            scope,
            RevisionReferenceSnapshot {
                asset_id: "00000000-0000-4000-8000-00000000000c".into(),
                manifest_sha256: old::sha(&serde_json::to_vec(r).unwrap()),
                manifest: r.clone(),
            },
            designs,
        )
        .unwrap();
        let figures = make_figures(&data, &revision, &reference);
        // Keep this original fixture on the frozen v1 renderer. New issuance
        // defaults to v2; the old byte-algorithm regression remains independent.
        let mut issue = prepare(data, revision, None, figures, false).unwrap();
        issue.content.renderer = current_renderer_identity().unwrap();
        issue.content.content_sha256 = issue.content.identity().unwrap();
        (issue, reference)
    }
    fn make_figures(
        data: &EnergyReportWorkbenchData,
        revision: &ScopeRevision,
        reference: &AdmittedReference,
    ) -> Vec<AlignedFigure> {
        revision
            .scope
            .report_views
            .iter()
            .map(|view| {
                let case = [&data.base.baseline, &data.base.proposed]
                    .into_iter()
                    .find(|c| c.id == view.design_id)
                    .unwrap();
                let request = old::architectural::aligned_views::AlignedFigureRequest {
                    schema_version: ALIGNED_FIGURE_SCHEMA.into(),
                    scope_revision_sha256: revision.revision_sha256.clone(),
                    scope_content_sha256: canonical_hash(&(
                        ARCHITECTURAL_SCOPE_SCHEMA,
                        &revision.scope,
                    ))
                    .unwrap(),
                    selected_run_id: case.run_id.clone(),
                    selected_record_sha256: case.record_sha256.clone(),
                    view: view.clone(),
                    focus_occurrence_ids: vec![],
                };
                let triangles = reference
                    .chunks
                    .iter()
                    .enumerate()
                    .flat_map(|(i, c)| decode_chunk(&reference.manifest, i, c).unwrap())
                    .map(Ok);
                old::architectural::aligned_views::render(
                    &reference.manifest,
                    &revision.scope,
                    &case.model,
                    &case.study.calendar,
                    &request,
                    triangles,
                    &AtomicBool::new(false),
                )
                .unwrap()
            })
            .collect()
    }
    #[test]
    fn issue_content_regenerates_identical_products_and_shows_missing_provenance_sensitivity() {
        let (issue, reference) = fixture();
        let original = serde_json::to_vec(&issue.content).unwrap();
        let local = crate::local_root()
            .unwrap()
            .join("verification")
            .join(format!("issue-render-test-{}", uuid::Uuid::new_v4()));
        let first =
            render_issue(&issue, &local.join("first"), None, &AtomicBool::new(false)).unwrap();
        let copy: IssueContent = serde_json::from_slice(&original).unwrap();
        let regenerated = for_regeneration(
            copy,
            make_figures(
                &issue.content.workbench,
                &issue.content.scope_revision,
                &reference,
            ),
        )
        .unwrap();
        let second = render_issue(
            &regenerated,
            &local.join("second"),
            None,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(
            first.schema_version,
            "greenlens.energy-report-artifacts.v0.7"
        );
        assert_eq!(first.artifacts.len(), second.artifacts.len());
        for (a, b) in first.artifacts.iter().zip(&second.artifacts) {
            assert_eq!(a.path, b.path);
            assert_eq!(a.sha256, b.sha256);
            assert_eq!(a.bytes, b.bytes);
        }
        assert_eq!(serde_json::to_vec(&issue.content).unwrap(), original);
        let executive = fs::read_to_string(local.join("first/executive.html")).unwrap();
        for marker in [
            "Documentation completeness: INCOMPLETE",
            "Exact selected-pair sensitivity: MISSING",
            "Immutable scope revision",
            "Issuance is not",
        ] {
            assert!(
                executive.contains(marker) || TEMPLATE.contains(marker),
                "{marker}"
            );
        }
        let technical = fs::read_to_string(local.join("first/technical.html")).unwrap();
        for marker in [
            "reference-layer",
            "analytical-layer",
            "True north unknown",
            "Original issued &lt;front&gt;",
        ] {
            assert!(technical.contains(marker), "{marker}");
        }
        assert!(local.join("first/input-provenance.csv").is_file());
        assert!(!local.join("first/executive.pdf").exists());
        assert!(first.processes.is_empty());
    }
    #[test]
    fn historical_renderer_mismatch_allows_inspection_but_not_silent_regeneration() {
        let (issue, reference) = fixture();
        let mut content = issue.content.clone();
        content.renderer.template_sha256 = "f".repeat(64);
        content.content_sha256 = content.identity().unwrap();
        content.validate().unwrap();
        assert!(content
            .ensure_supported_regeneration()
            .unwrap_err()
            .to_string()
            .contains("Exact regeneration unavailable"));
        let figures = make_figures(
            &issue.content.workbench,
            &issue.content.scope_revision,
            &reference,
        );
        assert!(for_regeneration(content, figures).is_err());
    }
    #[test]
    fn changed_binding_or_figure_cannot_be_resealed_into_an_older_issue() {
        let (issue, reference) = fixture();
        let mut figures = make_figures(
            &issue.content.workbench,
            &issue.content.scope_revision,
            &reference,
        );
        figures[0].svg.push(b' ');
        assert!(for_regeneration(issue.content.clone(), figures)
            .unwrap_err()
            .to_string()
            .contains("figure differs"));
        let mut changed = issue.content.clone();
        changed.figures[0].request.selected_record_sha256 = "f".repeat(64);
        changed.figures[0].definition_sha256 = changed.figures[0].identity().unwrap();
        changed.content_sha256 = changed.identity().unwrap();
        assert!(changed
            .validate()
            .unwrap_err()
            .to_string()
            .contains("selected run/record"));
        let mut changed = issue.content.clone();
        changed
            .scope_revision
            .designs
            .get_mut("B")
            .unwrap()
            .model
            .zones[0]
            .cooling_system
            .as_mut()
            .unwrap()
            .gross_rated_cop += 0.1;
        assert!(changed.validate().is_err());
    }
    #[test]
    fn old_render_route_omits_new_issue_contract_and_keeps_old_template() {
        let data = super::super::tests::fixture();
        let local = crate::local_root()
            .unwrap()
            .join("verification")
            .join(format!("legacy-issue-route-test-{}", uuid::Uuid::new_v4()));
        let result = super::super::render(&data, &local, None, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            result.schema_version,
            "greenlens.energy-report-artifacts.v0.5"
        );
        assert_eq!(result.template_sha256, super::super::template_sha256());
        assert_eq!(result.dataset_sha256, data.derived_sha256);
        assert!(!local.join("issue-content.json").exists());
        assert!(!fs::read_to_string(local.join("technical.html"))
            .unwrap()
            .contains("Immutable scope revision"));
    }
}
