//! Immutable analytical-only design reports. This additive codec does not call
//! or change historical issue renderers, invent a reference scope, or run an
//! engine. Publication must bind genuine retained records through verify_project.
use crate::{
    energy_report::{
        self as report, architectural::SensitivityReportEvidence, Document, EnergyReportArtifacts,
        NativeDocuments, ReportArtifact, Section, TypstConfig,
    },
    energy_report_workbench::issuance::validate_sensitivity,
    persistence::{self, PackageBudget},
    Project,
};
use anyhow::{ensure, Context, Result};
use greenlens_core::{canonical_hash, energy::*};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub const CONTENT_SCHEMA: &str = "greenlens.design-report-content.v1";
pub const CONTENT_SCHEMA_V2: &str = "greenlens.design-report-content.v2";
pub const INDEX_SCHEMA: &str = "greenlens.design-report-index.v1";
pub const PRODUCT_SCHEMA: &str = "greenlens.design-report-products.v1";
pub const RENDERER_METHOD: &str = "greenlens.analytical-design-report-renderer/1";
pub const RENDERER_METHOD_V2: &str = "greenlens.bound-design-report-renderer/2";
pub const MAX_REPORTS: usize = 32;
pub const MAX_PRODUCTS: usize = 252;
const MAX_CONTENT_BYTES: u64 = 48 * 1024 * 1024;
pub(crate) const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const TEMPLATE: &str = include_str!("../../../reports/templates/design-comparison-v1.typ");
const PINS: &str = include_str!("../../../dependencies/typst.json");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DesignRendererIdentity {
    pub method: String,
    pub figure_method: String,
    pub template_sha256: String,
    pub typst_version: String,
    pub typst_executable_sha256: String,
    pub font_sha256: BTreeMap<String, String>,
}
pub fn renderer_identity() -> Result<DesignRendererIdentity> {
    let pins: Value = serde_json::from_str(PINS)?;
    let text = |v: &Value| {
        v.as_str()
            .map(str::to_owned)
            .context("Invalid owned renderer pin")
    };
    Ok(DesignRendererIdentity {
        method: RENDERER_METHOD.into(),
        figure_method: DESIGN_FIGURE_METHOD.into(),
        template_sha256: report::sha(TEMPLATE.as_bytes()),
        typst_version: text(&pins["version"])?,
        typst_executable_sha256: text(&pins["members"]["typst.exe"]["sha256"])?,
        font_sha256: pins["fonts"]["files"]
            .as_array()
            .context("Owned font pins")?
            .iter()
            .map(|f| Ok((text(&f["name"])?, text(&f["sha256"])?)))
            .collect::<Result<_>>()?,
    })
}
pub fn bound_renderer_identity() -> Result<DesignRendererIdentity> {
    let mut identity = renderer_identity()?;
    identity.method = RENDERER_METHOD_V2.into();
    identity.figure_method = crate::design_report_drawing::FIGURE_METHOD.into();
    Ok(identity)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesignReportContent {
    pub schema_version: String,
    pub content_sha256: String,
    pub workbench: EnergyReportWorkbenchData,
    pub sensitivity: Option<SensitivityReportEvidence>,
    pub proposal_declarations: Vec<crate::design_proposals::ProposalReceipt>,
    pub frames: Vec<DesignFigureFrame>,
    pub analytical_scope: String,
    pub north_declaration: Option<String>,
    pub renderer: DesignRendererIdentity,
    pub include_pdf: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawing: Option<crate::design_report_drawing::DesignReportDrawing>,
}
impl DesignReportContent {
    pub fn identity(&self) -> Result<String> {
        let mut value = self.clone();
        value.content_sha256.clear();
        Ok(canonical_hash(&value)?)
    }
    pub fn figures(&self) -> Result<PairedDesignFigures> {
        let revision = self
            .drawing
            .as_ref()
            .map(|d| d.scope_revision.revision_sha256.clone());
        let mut a =
            DesignFigureCase::from_report_case(&self.workbench.base.baseline, revision.clone())?;
        let mut b = DesignFigureCase::from_report_case(&self.workbench.base.proposed, revision)?;
        // A/B are presentation-side labels. The canonical report retains the
        // actual design IDs; neither record nor physical identity is modified.
        a.design_id = "A".into();
        b.design_id = "B".into();
        let request = PairedDesignFigureRequest {
            schema_version: DESIGN_FIGURE_SCHEMA.into(),
            baseline: a,
            proposed: b,
            frames: self.frames.clone(),
            scope: self.analytical_scope.clone(),
            north_declaration: self.north_declaration.clone(),
        };
        match &self.drawing {
            Some(binding) => binding.figures(&request),
            None => Ok(paired_design_figures(&request)?),
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (self.schema_version == CONTENT_SCHEMA && self.drawing.is_none())
                || (self.schema_version == CONTENT_SCHEMA_V2 && self.drawing.is_some()),
            "Unsupported design-report content codec"
        );
        self.workbench.validate()?;
        if let Some(binding) = &self.drawing {
            binding.validate(&self.workbench, &self.frames)?;
        }
        ensure!(self.workbench.attachments.included_ids.is_empty(),
            "This design report includes only its bound drawing/scope supplement and declared products. Other checked attachments require the full architectural issue workflow; checked evidence cannot disappear silently");
        if let Some(e) = &self.sensitivity {
            validate_sensitivity(e, &self.workbench)?;
        }
        crate::design_proposals::validate_receipts(&self.proposal_declarations)?;
        for receipt in &self.proposal_declarations {
            ensure!(receipt.after_model_sha256 == self.workbench.base.proposed.model_input_sha256,
                "Proposal declaration belongs to another model; select its matching retained run or omit it explicitly");
        }
        self.figures()?;
        ensure!(
            self.renderer.method
                == if self.drawing.is_some() {
                    RENDERER_METHOD_V2
                } else {
                    RENDERER_METHOD
                }
                && self.renderer.figure_method
                    == if self.drawing.is_some() {
                        crate::design_report_drawing::FIGURE_METHOD
                    } else {
                        DESIGN_FIGURE_METHOD
                    }
                && digest(&self.renderer.template_sha256)
                && digest(&self.renderer.typst_executable_sha256)
                && self.renderer.font_sha256.len() == 2
                && self.renderer.font_sha256.values().all(|h| digest(h)),
            "Invalid design renderer identity"
        );
        ensure!(serde_json::to_vec(self)?.len() as u64 <= MAX_CONTENT_BYTES, "Design report content exceeds its 48 MiB bound; preserve the existing study and start a separately scoped document");
        ensure!(
            self.content_sha256 == self.identity()?,
            "Design report content identity differs"
        );
        Ok(())
    }
    pub fn ensure_supported_regeneration(&self) -> Result<()> {
        self.validate()?;
        let expected = if self.drawing.is_some() {
            bound_renderer_identity()?
        } else {
            renderer_identity()?
        };
        ensure!(self.renderer == expected, "Exact design-report regeneration unavailable: retained renderer/template/fonts differ. Preserved products remain inspectable; never replace them");
        Ok(())
    }
}
pub fn prepare(
    workbench: EnergyReportWorkbenchData,
    sensitivity: Option<SensitivityReportEvidence>,
    frames: Vec<DesignFigureFrame>,
    analytical_scope: String,
    north_declaration: Option<String>,
    include_pdf: bool,
) -> Result<DesignReportContent> {
    let mut value = DesignReportContent {
        schema_version: CONTENT_SCHEMA.into(),
        content_sha256: String::new(),
        workbench,
        sensitivity,
        proposal_declarations: vec![],
        frames,
        analytical_scope,
        north_declaration,
        renderer: renderer_identity()?,
        include_pdf,
        drawing: None,
    };
    value.content_sha256 = value.identity()?;
    value.validate()?;
    Ok(value)
}
pub fn with_proposals(
    mut content: DesignReportContent,
    declarations: Vec<crate::design_proposals::ProposalReceipt>,
) -> Result<DesignReportContent> {
    content.proposal_declarations = declarations;
    content.content_sha256 = content.identity()?;
    content.validate()?;
    Ok(content)
}

/// Explicit opt-in to the next derived codec. The v1 path and serialized None
/// stay byte-for-byte unchanged; no retained content is migrated in place.
pub fn with_drawing(
    mut content: DesignReportContent,
    binding: crate::design_report_drawing::DesignReportDrawing,
) -> Result<DesignReportContent> {
    content.schema_version = CONTENT_SCHEMA_V2.into();
    content.frames = binding.frames.clone();
    content.drawing = Some(binding);
    content.renderer = bound_renderer_identity()?;
    content.content_sha256 = content.identity()?;
    content.validate()?;
    Ok(content)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DesignRunBinding {
    pub run_id: String,
    pub record_sha256: String,
    pub design_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DesignReportSummary {
    pub id: String,
    pub path: String,
    pub content_sha256: String,
    pub manifest_file_sha256: String,
    pub products_sha256: String,
    pub baseline: DesignRunBinding,
    pub proposed: DesignRunBinding,
    pub sensitivity_run_ids: Vec<String>,
    pub title: String,
    pub report_revision: String,
    pub include_pdf: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DesignReportIndex {
    pub schema_version: String,
    pub reports: Vec<DesignReportSummary>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesignProductManifest {
    pub schema_version: String,
    pub content_sha256: String,
    pub products_sha256: String,
    pub products: Vec<ReportArtifact>,
}
impl DesignProductManifest {
    pub(crate) fn identity(&self) -> Result<String> {
        Ok(canonical_hash(&(
            PRODUCT_SCHEMA,
            &self.content_sha256,
            &self.products,
        ))?)
    }
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == PRODUCT_SCHEMA && digest(&self.content_sha256),
            "Invalid design product manifest"
        );
        ensure!(
            !self.products.is_empty() && self.products.len() <= MAX_PRODUCTS,
            "Design product count is outside its bound"
        );
        let mut budget = PackageBudget::default();
        let mut previous: Option<&str> = None;
        for product in &self.products {
            budget.add(&product.path, product.bytes)?;
            ensure!(
                !product.path.contains('/')
                    && product.path != "design-products.json"
                    && product.path != "design-render-failure.json"
                    && product.bytes > 0
                    && digest(&product.sha256),
                "Invalid deterministic design product"
            );
            ensure!(
                previous.is_none_or(|p| p < product.path.as_str()),
                "Design products must be in unique deterministic path order"
            );
            previous = Some(&product.path);
        }
        ensure!(
            self.products
                .iter()
                .any(|p| p.path == "design-content.json")
                && self.products_sha256 == self.identity()?,
            "Design manifest content/identity differs"
        );
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignReportInspection {
    pub content: DesignReportContent,
    pub products: DesignProductManifest,
    pub manifest_file_sha256: String,
    pub total_bytes: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignReportRender {
    pub inspection: DesignReportInspection,
    pub runtime: EnergyReportArtifacts,
}

fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn checkpoint(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::SeqCst),
        "Design report operation cancelled"
    );
    crate::project_open::checkpoint(0)
}
fn json_bytes(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}
fn admit_content_bytes(bytes: u64, limit: u64) -> Result<()> {
    ensure!(
        bytes <= limit,
        "Serialized design report content exceeds its {limit}-byte bound; preserve the existing study and start a separately scoped document"
    );
    Ok(())
}
fn content_json_bytes(value: &impl Serialize, limit: u64) -> Result<Vec<u8>> {
    let bytes = json_bytes(value)?;
    admit_content_bytes(bytes.len() as u64, limit)?;
    Ok(bytes)
}
fn read(root: &Path, name: &str, cap: u64, cancel: &AtomicBool) -> Result<Vec<u8>> {
    persistence::read_evidence_bytes_checked(
        &persistence::regular_child(root, name)?,
        cap,
        &mut |n| {
            checkpoint(cancel)?;
            crate::project_open::checkpoint(n)
        },
    )
}
fn png_admission(bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() >= 33 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") && &bytes[12..16] == b"IHDR",
        "Invalid retained PNG product"
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
    Ok(())
}
fn ser(value: &impl Serialize) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}
fn input_value(value: &ThermalInputValue) -> String {
    match value {
        ThermalInputValue::Missing => "Absent".into(),
        ThermalInputValue::Null => "Not supplied".into(),
        ThermalInputValue::Boolean(value) => value.to_string(),
        ThermalInputValue::Number(value) => value.to_string(),
        ThermalInputValue::Text(value) => value.clone(),
        ThermalInputValue::ObjectPresent => "Present".into(),
    }
}
fn csv_field(value: &str) -> String {
    let prefix = if value.trim_start().starts_with(['=', '+', '-', '@']) {
        "'"
    } else {
        ""
    };
    format!("\"{prefix}{}\"", value.replace('"', "\"\""))
}
fn change_csv(figures: &PairedDesignFigures) -> Result<String> {
    let mut result = "object_id,change_kind,before_geometry_sha256,after_geometry_sha256,before_json,after_json\r\n".to_string();
    for row in &figures.changes {
        let values = [
            row.object_id.clone(),
            ser(&row.kind)?,
            row.before_geometry_sha256.clone().unwrap_or_default(),
            row.after_geometry_sha256.clone().unwrap_or_default(),
            ser(&row.before)?,
            ser(&row.after)?,
        ];
        result.push_str(
            &values
                .iter()
                .map(|s| csv_field(s))
                .collect::<Vec<_>>()
                .join(","),
        );
        result.push_str("\r\n");
    }
    Ok(result)
}
fn sensitivity_sections(content: &DesignReportContent, detailed: bool) -> Result<Vec<Section>> {
    if let Some(e) = &content.sensitivity {
        let mut result = vec![Section::prose("Exact-pair numerical sensitivity", vec![
            format!("Retained assessment {} is bound to the selected A/B records and exact 10/5-minute declaration {}. This is numerical sensitivity, not physical validation or automatic qualification.", e.session_id, e.result.plan_sha256),
            "Frozen criteria: at most 1% annual electricity and 3% common ten-minute facility peak difference, plus the retained comfort criteria. Failures and interruptions remain in sensitivity.json.".into()])];
        let assessment = &e.result.assessment;
        result.push(Section::prose("Sensitivity outcome",vec![format!("Numerical criteria passed: {}. All declared runs integrity-checked: {}. Missing slots: {}. Failed/cancelled receipts: {}. Qualification promoted: {}. Complete evidence: sensitivity.json.",assessment.numerical_sensitivity_passed,assessment.all_declared_runs_integrity_checked,if assessment.missing_slots.is_empty(){"none".into()}else{assessment.missing_slots.join(", ")},assessment.interruptions.len(),assessment.qualification_promoted)]));
        if detailed {
            result.push(Section::table(
                "Sensitivity criteria — complete register",
                &[
                    "Criterion / scope",
                    "10 min / 5 min reference",
                    "Difference / limit / outcome",
                ],
                assessment
                    .criteria
                    .iter()
                    .map(|criterion| {
                        vec![
                            format!(
                                "{} / {} / {}\n{}",
                                criterion.id,
                                criterion.alternative_id,
                                criterion.zone_id.as_deref().unwrap_or("building"),
                                criterion.quantity
                            ),
                            format!(
                                "{} / {}",
                                criterion
                                    .ten_minute
                                    .map(|v| v.to_string())
                                    .unwrap_or_else(|| "Missing".into()),
                                criterion
                                    .five_minute_reference
                                    .map(|v| v.to_string())
                                    .unwrap_or_else(|| "Missing".into())
                            ),
                            format!(
                                "{}% / {}% / {}\n{}",
                                criterion
                                    .measured_percent
                                    .map(|v| v.to_string())
                                    .unwrap_or_else(|| "Undefined".into()),
                                criterion.limit_percent,
                                if criterion.passed { "pass" } else { "fail" },
                                criterion.explanation
                            ),
                        ]
                    })
                    .collect(),
            ));
            result.push(Section::table(
                "Sensitivity retained run identities",
                &["Declared slot", "Exact retained evidence"],
                assessment
                    .runs
                    .iter()
                    .map(|run| {
                        vec![
                            run.slot_id.clone(),
                            format!(
                                "Run {}\nRecord {}\nPhysical {}\nExecution {}",
                                run.run_id,
                                run.record_sha256,
                                run.physical_input_sha256,
                                run.execution_input_sha256
                            ),
                        ]
                    })
                    .collect(),
            ));
            result.push(Section::table(
                "Sensitivity interruptions — complete register",
                &["Slot / outcome", "Reason / receipt"],
                assessment
                    .interruptions
                    .iter()
                    .map(|interruption| {
                        Ok(vec![
                            format!("{} / {}", interruption.slot_id, ser(&interruption.kind)?),
                            format!("{}\n{}", interruption.reason, interruption.receipt_sha256),
                        ])
                    })
                    .collect::<Result<_>>()?,
            ));
            result.push(Section::prose(
                "Sensitivity limitations",
                assessment.limitations.clone(),
            ));
        }
        Ok(result)
    } else {
        Ok(vec![Section::prose("Exact-pair numerical sensitivity — missing", vec![
        "No retained sensitivity assessment for this exact selected pair is attached. Another pair's assessment is not transferred. Missing sensitivity limits interpretation; successful execution or rendering does not establish convergence or physical validity.".into()])])
    }
}
fn metadata_section(data: &EnergyReportWorkbenchData) -> Section {
    let metadata = &data.metadata;
    Section::table(
        "Issue metadata",
        &["Field", "Declared value"],
        vec![
            vec!["Revision".into(), metadata.revision.clone()],
            vec![
                "Issue date".into(),
                metadata
                    .issue_date
                    .clone()
                    .unwrap_or_else(|| "Not supplied".into()),
            ],
            vec![
                "Author".into(),
                metadata
                    .author
                    .clone()
                    .unwrap_or_else(|| "Not supplied".into()),
            ],
            vec![
                "Preparer".into(),
                metadata
                    .preparer
                    .clone()
                    .unwrap_or_else(|| "Not supplied".into()),
            ],
            vec![
                "Approval".into(),
                "Not asserted. Report rendering is not professional review or sign-off.".into(),
            ],
        ],
    )
}
fn executive_excerpt(value: &str, count: usize) -> String {
    let mut chars = value.chars();
    let mut text: String = chars.by_ref().take(count).collect();
    if chars.next().is_some() {
        text.push_str("… [complete declaration in the technical report and design-proposals.json]");
    }
    text
}
fn documents(
    content: &DesignReportContent,
    figures: &PairedDesignFigures,
) -> Result<Vec<Document>> {
    let data = &content.workbench;
    let p = data.base.presentation()?;
    let mut old = report::documents(&data.base, &p, &content.content_sha256)?;
    let mut disclosures = data.service_disclosures();
    disclosures.extend(data.base.limitations.clone());
    disclosures.extend(figures.limitations.clone());
    if let Some(binding) = &content.drawing {
        disclosures.extend(binding.disclosures());
    } else {
        disclosures.push("Analytical-only scope: no architectural reference, mapping register, accepted scope revision or source-documentation review is attached. Geometry drawings identify the actual retained models and do not prove whole-building coverage. An empty provenance register must not be read as complete provenance.".into());
    }
    disclosures.push("This report bundle retains canonical input/result snapshots, figures and product identities. It is not a complete engine replay archive. Retain/export the complete GreenLens project for original weather, run evidence and replay.".into());
    disclosures.push(format!("Comparison eligibility: geometry/weather/calendar compatible = {}; unchanged service inputs = {}. Signed differences are retained even when the proposal performs worse; no guaranteed savings or causal attribution is asserted.", figures.comparable_geometry_weather_calendar, figures.unchanged_service_inputs.map(|v|v.to_string()).unwrap_or_else(||"not assessed".into())));
    let comparison = Section::table(
        "Measured outcomes from selected evidence",
        &["Quantity", "Baseline A", "Proposal B"],
        vec![
            vec![
                "Annual facility electricity".into(),
                report::cell(&p.baseline.annual.facility),
                report::cell(&p.proposed.annual.facility),
            ],
            vec![
                "Own coincident facility peak".into(),
                report::cell(&p.baseline.peak_kw),
                report::cell(&p.proposed.peak_kw),
            ],
            vec![
                "Own peak local-standard interval".into(),
                report::timestamp(&p.baseline.peak_timestamp),
                report::timestamp(&p.proposed.peak_timestamp),
            ],
            vec![
                "EUI / gross floor area".into(),
                report::cell(&p.baseline.gross_eui),
                report::cell(&p.proposed.gross_eui),
            ],
            vec![
                "Gross floor area denominator".into(),
                report::cell(&p.baseline.gross_floor_area),
                report::cell(&p.proposed.gross_floor_area),
            ],
            vec![
                "Occupied cooling-unmet interval union".into(),
                report::cell(&p.baseline.building_comfort_union.unmet_percent),
                report::cell(&p.proposed.building_comfort_union.unmet_percent),
            ],
            vec![
                "Operational emissions".into(),
                report::cell(&p.baseline.emissions),
                report::cell(&p.proposed.emissions),
            ],
        ],
    );
    let status = Section::table(
        "Exact run bindings and separate assessment axes",
        &["Side", "Retained evidence and assessment"],
        [(&data.base.baseline, "A"), (&data.base.proposed, "B")]
            .into_iter()
            .map(|(c, side)| {
                Ok(vec![
                    side.into(),
                    format!(
                        "{}\nRun {}\nRecord {}\nModel {}\nPhysical {}\nProcessing {}\n{}",
                        c.label,
                        c.run_id,
                        c.record_sha256,
                        c.model_input_sha256,
                        c.physical_input_sha256,
                        c.processing_sha256,
                        ser(&c.assessment)?
                    ),
                ])
            })
            .collect::<Result<_>>()?,
    );
    let findings=Section::prose("Measured comparison findings",vec![format!("Baseline minus proposal annual facility electricity: {} ({} of baseline). Difference between the cases' own coincident facility peaks: {} ({} of baseline). Negative values are increases, not savings; the peaks may occur at different times. Undefined zero-baseline percentages remain N/A.",report::cell(&p.facility_energy_savings.absolute),report::cell(&p.facility_energy_savings.percent),report::cell(&p.facility_peak_savings.absolute),report::cell(&p.facility_peak_savings.percent)),format!("Comparison eligibility is explicitly {} for geometry/weather/calendar and {} for unchanged service inputs. Reduced demand with changed service must not be described as an efficiency improvement. These are observed results for the bound inputs; drawings and design hypotheses do not establish causation or universal benefit.",if figures.comparable_geometry_weather_calendar{"compatible"}else{"incompatible"},if figures.unchanged_service_inputs==Some(true){"unchanged"}else{"changed or unassessed"})]);
    let mut executive=vec![
        Section::prose("Issue declaration",vec![format!("Revision {} · issue date {} · author {} · preparer {}. No independent approval or professional sign-off is asserted.",data.metadata.revision,data.metadata.issue_date.as_deref().unwrap_or("not supplied"),data.metadata.author.as_deref().unwrap_or("not supplied"),data.metadata.preparer.as_deref().unwrap_or("not supplied"))]),
        findings.clone(),comparison.clone(),
        Section::table("Selected evidence and scientific status", &["Side / exact run", "Status and qualification limits"],
            [(&data.base.baseline,"A"),(&data.base.proposed,"B")].into_iter().map(|(c,side)|Ok(vec![format!("{side} · {}\n{}",c.label,c.run_id),format!("Execution {}; integrity {}; physical inputs {}; processed {}; derived {}. Scientific status {}: {}\nWarnings {}; severe {}. Full method, registry, diagnostic and input identities: technical report and energy-report.json.",ser(&c.assessment.execution)?,ser(&c.assessment.evidence_integrity)?,ser(&c.assessment.current_physical_inputs)?,ser(&c.assessment.current_processed_results)?,ser(&c.assessment.current_derived_results)?,ser(&c.assessment.scientific.status)?,c.assessment.scientific.reason,c.diagnostics.warning_count.map(|v|v.to_string()).unwrap_or_else(||"not checked".into()),c.diagnostics.severe_count.map(|v|v.to_string()).unwrap_or_else(||"not checked".into()))])).collect::<Result<_>>()?),
        Section::prose("Scope and interpretation",vec![
            if let Some(binding) = &content.drawing { binding.disclosures().join("\n") } else { "Analytical-only report: no architectural reference, accepted mapping/scope revision or source-documentation review is attached. Coverage of a real or whole building is not established; an empty provenance register is not complete provenance.".into() },
            format!("Weather source is synthetic for A: {}; B: {}. Values apply to the retained annual local-standard calendar and the named gross-area denominator. Occupied-unmet union is descriptive; per-zone comfort criteria and all native diagnostics remain in the technical report. Unsupported end uses and absent carbon factors remain explicitly unavailable.",data.base.baseline.weather_summary.source.synthetic,data.base.proposed.weather_summary.source.synthetic),
            format!("Lighting-service evidence: {}. No daylight, embodied-carbon, certification or spatial thermal-field benefit is inferred. Complete limitations, assumptions, meter boundaries and exact provenance are in the technical report and machine-readable evidence.",data.base.comparison.lighting_service_equivalence),
        ]),
    ];
    let mut hypotheses=vec![format!("{} reviewed proposal declarations; {} total typed field changes. The executive shows {} rationale excerpts; complete values, sources, identities and tradeoffs remain in the technical report, design-proposals.json and typed-input-differences.json. No design is guaranteed to improve performance.",content.proposal_declarations.len(),data.typed_comparison.total_changes,content.proposal_declarations.len().min(2))];
    for declaration in content.proposal_declarations.iter().take(2) {
        hypotheses.push(format!(
            "{} Source: {}. Synthetic declaration: {}.",
            executive_excerpt(&declaration.draft.rationale, 240),
            executive_excerpt(&declaration.draft.source.citation, 120),
            declaration.draft.source.synthetic
        ));
    }
    if content.proposal_declarations.is_empty() {
        hypotheses.push("No reviewed proposal declaration is attached. The exact selected inputs and complete typed differences still define the compared cases; no rationale is invented.".into());
    }
    executive.push(Section::prose(
        "Design hypothesis and tradeoffs",
        hypotheses,
    ));
    executive.extend(sensitivity_sections(content, false)?);
    let mut summary = vec![
        metadata_section(data),
        findings,
        Section::prose("Interpretation and scope", disclosures.clone()),
        comparison,
        status,
    ];
    let mut typed = Section::table(
        "Parameter annotations — exact retained inputs",
        &[
            "Object / field",
            "Baseline → proposal / unit",
            "Classification / service",
        ],
        data.typed_comparison
            .differences
            .iter()
            .take(128)
            .map(|row| {
                Ok(vec![
                    format!("{} {}", row.object.id, row.field_path),
                    format!(
                        "{} → {} {}",
                        input_value(&row.before),
                        input_value(&row.after),
                        row.unit
                            .as_deref()
                            .unwrap_or("unit not applicable / not supplied")
                    ),
                    format!(
                        "{} / {}\n{}",
                        ser(&row.category)?,
                        ser(&row.service_effect)?,
                        row.message
                    ),
                ])
            })
            .collect::<Result<_>>()?,
    );
    typed.paragraphs.push(format!("Displayed {} of {} retained typed rows; {} total field changes; typed register truncated: {}. Complete available typed rows: typed-input-differences.json. Complete before/after object values: design-changes.json; lossless full inputs: energy-report.json and inputs.csv. Summary compatibility/service flags account for the complete comparison, including undisplayed rows.",data.typed_comparison.differences.len().min(128),data.typed_comparison.differences.len(),data.typed_comparison.total_changes,data.typed_comparison.truncated));
    summary.push(typed);
    summary.push(Section::table(
        "Explicit design hypotheses",
        &["Rationale / source", "Applied parameter declaration"],
        content
            .proposal_declarations
            .iter()
            .map(|r| {
                Ok(vec![
                    format!(
                        "{}\n{}\nSynthetic source: {}",
                        r.draft.rationale, r.draft.source.citation, r.draft.source.synthetic
                    ),
                    format!(
                        "{}\nUnits: {}\nBefore model: {}\nAfter model: {}\nReceipt: {}",
                        ser(&r.draft.action)?,
                        r.draft.units,
                        r.before_model_sha256,
                        r.after_model_sha256,
                        r.receipt_sha256
                    ),
                ])
            })
            .collect::<Result<_>>()?,
    ));
    summary.extend(sensitivity_sections(content, false)?);
    let mut figure_sections = Vec::new();
    for product in &figures.products {
        let frame = figures
            .request
            .frames
            .iter()
            .find(|f| f.id == product.frame_id)
            .context("Figure frame")?;
        let mut caption = format!("Shared frame {}: {:.6} × {:.6} m; {:?}; section {}. {} edges; {} labels displayed, {} omitted labels. Complete object/change identities: design-figures.json and design-changes.json. No geometry sampling or spatial results are implied.", frame.id, frame.vertical_span_m*frame.aspect_ratio,frame.vertical_span_m,frame.kind,ser(&frame.section)?,product.analytical_edge_count,product.displayed_label_count,product.omitted_label_count);
        if content.drawing.is_some() {
            caption = caption.replace("No geometry sampling or spatial results are implied.", "Analytical outlines are complete within the shared frame. Neutral reference reduction, exclusions and exact accepted view/source identities are disclosed in design-drawing.json; context is not a spatial simulation result or proof of equivalence.");
        }
        figure_sections.push(Section::figure(
            &format!("{} — {}", frame.title, product.side),
            &product.filename,
            caption,
        ));
    }
    let rows = figures
        .changes
        .iter()
        .map(|change| {
            Ok(vec![
                change.object_id.clone(),
                ser(&change.kind)?,
                change
                    .before_geometry_sha256
                    .clone()
                    .unwrap_or_else(|| "Absent".into()),
                change
                    .after_geometry_sha256
                    .clone()
                    .unwrap_or_else(|| "Absent".into()),
            ])
        })
        .collect::<Result<_>>()?;
    let mut changes = Section::table(
        "Complete object-change register",
        &[
            "Object",
            "Change",
            "Baseline geometry identity",
            "Proposal geometry identity",
        ],
        rows,
    );
    changes.paragraphs.push(format!("Displayed {} of {} changes; no change rows are omitted. Complete before/after values are in design-changes.json and design-changes.csv. Properties-only and rename-only rows do not manufacture geometry changes.",figures.changes.len(),figures.change_count));
    let make = |kind: &str, sections| Document {
        schema_version: if content.drawing.is_some() {
            "greenlens.design-report-document.v2"
        } else {
            "greenlens.design-report-document.v1"
        },
        title: data.metadata.title.clone(),
        report_id: data.base.id.clone(),
        revision: data.metadata.revision.clone(),
        kind: kind.into(),
        scope: content.analytical_scope.clone(),
        dataset_sha256: content.content_sha256.clone(),
        sections,
    };
    executive.extend(
        figure_sections
            .iter()
            .zip(&figures.products)
            .filter(|(_, p)| {
                p.side == "overlay"
                    && figures.request.frames.iter().any(|frame| {
                        frame.id == p.frame_id && frame.kind == DesignViewKind::Axonometric
                    })
            })
            .take(1)
            .map(|(s, _)| s.clone()),
    );
    executive.push(Section::prose("Complete paired drawings and evidence",vec!["The executive includes one matched axonometric overlay. The technical report retains baseline, proposal and change overlay for every accepted frame, with scale, section, exact bindings and complete change registers. This report bundle is not an engine replay archive; export the complete project for original weather and replay evidence.".into()]));
    let mut technical = summary;
    technical.extend(figure_sections);
    technical.push(changes);
    technical.extend(sensitivity_sections(content, true)?);
    technical.append(&mut old[0].sections);
    let mut appendix = vec![Section::prose(
        "Documentation and complete data",
        disclosures,
    )];
    if let Some(binding) = &content.drawing {
        let revision = &binding.scope_revision;
        let unresolved: Vec<_> = revision
            .review_decisions
            .iter()
            .filter(|d| {
                d.resolution == greenlens_core::study_revision::ReviewResolution::Unresolved
            })
            .collect();
        let mut register = Section::table(
            "Unresolved scope review — bounded excerpt",
            &["Affected review row", "Retained rationale"],
            unresolved
                .iter()
                .take(80)
                .map(|d| {
                    let label = revision
                        .reviewed_rows
                        .as_ref()
                        .and_then(|rows| rows.iter().find(|r| r.row_id == d.row_id))
                        .map(|r| format!("{:?}: {} ({})", r.kind, r.key, d.row_id))
                        .unwrap_or_else(|| {
                            format!("{} — legacy field binding unavailable", d.row_id)
                        });
                    vec![label, d.rationale.clone()]
                })
                .collect(),
        );
        register.paragraphs.push(format!("Displayed {} of {} unresolved decisions from {} total decisions. All decisions, rationales, input documentation, mappings, assumptions and exact accepted source/model snapshots are retained without truncation in design-scope-revision.json. Unresolved matters limit interpretation and do not imply approval or physical qualification.", unresolved.len().min(80), unresolved.len(), revision.review_decisions.len()));
        technical.push(register.clone());
        appendix.push(register);
    }
    appendix.append(&mut old[1].sections);
    let performance = std::mem::take(&mut old[2].sections);
    Ok(vec![
        make("Design executive", executive),
        make("Design technical", technical),
        make("Input evidence appendix", appendix),
        make("Performance table", performance),
    ])
}
type OwnedProducts = (Vec<Document>, BTreeMap<String, Vec<u8>>);
fn owned_products(content: &DesignReportContent) -> Result<OwnedProducts> {
    content.validate()?;
    // Admission covers the actual retained pretty JSON, including its newline,
    // before preparing any native rendering. The reader uses the same bound.
    let content_bytes = content_json_bytes(content, MAX_CONTENT_BYTES)?;
    let figures = content.figures()?;
    let p = content.workbench.base.presentation()?;
    let docs = documents(content, &figures)?;
    let mut products = BTreeMap::from([
        ("design-content.json".into(), content_bytes),
        (
            "energy-report.json".into(),
            json_bytes(&content.workbench.base)?,
        ),
        (
            "workbench-report.json".into(),
            json_bytes(&content.workbench)?,
        ),
        ("performance.json".into(), json_bytes(&p)?),
        ("performance.csv".into(), report::csv(&p)?.into_bytes()),
        (
            "inputs.csv".into(),
            report::inputs_csv(&content.workbench.base)?.into_bytes(),
        ),
        ("design-figures.json".into(), json_bytes(&figures)?),
        ("design-changes.json".into(), json_bytes(&figures.changes)?),
        (
            "design-changes.csv".into(),
            change_csv(&figures)?.into_bytes(),
        ),
        ("sensitivity.json".into(), json_bytes(&content.sensitivity)?),
        (
            "design-proposals.json".into(),
            json_bytes(&content.proposal_declarations)?,
        ),
        (
            "typed-input-differences.json".into(),
            json_bytes(&content.workbench.typed_comparison)?,
        ),
        (
            "baseline-model.svg".into(),
            report::model_svg(&content.workbench.base.baseline).into_bytes(),
        ),
        (
            "proposed-model.svg".into(),
            report::model_svg(&content.workbench.base.proposed).into_bytes(),
        ),
        ("annual.svg".into(), report::chart(&p, false).into_bytes()),
        ("monthly.svg".into(), report::chart(&p, true).into_bytes()),
    ]);
    if let Some(binding) = &content.drawing {
        products.insert("design-drawing.json".into(), json_bytes(binding)?);
        products.insert(
            "design-scope-revision.json".into(),
            json_bytes(&binding.scope_revision)?,
        );
    }
    for product in &figures.products {
        ensure!(
            products
                .insert(product.filename.clone(), product.svg.as_bytes().to_vec())
                .is_none(),
            "Owned figure path collision"
        );
    }
    let image_names: Vec<_> = products
        .keys()
        .filter(|name| name.ends_with(".svg"))
        .cloned()
        .collect();
    let generated: Vec<_> = image_names
        .iter()
        .map(|name| (name.as_str(), products[name].clone()))
        .collect();
    for (name, doc) in [
        "design-executive",
        "design-technical",
        "input-appendix",
        "performance",
    ]
    .iter()
    .zip(&docs)
    {
        products.insert(
            format!("{name}.html"),
            report::html(doc, &generated)?.into_bytes(),
        );
    }
    let mut budget = PackageBudget::default();
    for (name, bytes) in &products {
        budget.add(name, bytes.len() as u64)?;
    }
    Ok((docs, products))
}

/// Destination is fresh external staging. Only a complete product manifest is
/// publication-ready. Errors retain diagnostic staging but never a success index.
pub fn render(
    content: &DesignReportContent,
    destination: &Path,
    typst: Option<&TypstConfig>,
    cancel: &AtomicBool,
) -> Result<DesignReportRender> {
    checkpoint(cancel)?;
    content.ensure_supported_regeneration()?;
    ensure!(
        content.include_pdf == typst.is_some(),
        "PDF selection and supported renderer availability differ; no silent format omission"
    );
    let (docs, products) = owned_products(content)?;
    checkpoint(cancel)?;
    report::create_fresh(destination)?;
    let mut runtime=EnergyReportArtifacts {schema_version:"greenlens.design-report-render-runtime.v1".into(),dataset_sha256:content.content_sha256.clone(),renderer_status:"not_requested".into(),renderer_message:"Deterministic HTML/SVG/CSV/JSON; no PDF requested. No simulation or qualification was performed.".into(),typst_executable_sha256:None,template_sha256:content.renderer.template_sha256.clone(),renderer_work_id:None,renderer_inputs:BTreeMap::new(),artifacts:vec![],processes:vec![]};
    for (name, bytes) in &products {
        checkpoint(cancel)?;
        report::write_new(&destination.join(name), bytes)?;
        runtime.artifacts.push(report::artifact(destination, name)?);
    }
    if let Some(config) = typst {
        let generated: Vec<_> = products
            .iter()
            .filter(|(name, _)| name.ends_with(".svg"))
            .map(|(name, bytes)| (name.as_str(), bytes.clone()))
            .collect();
        let outcome = report::render_native_documents(
            NativeDocuments {
                docs: &docs,
                generated: &generated,
                template: TEMPLATE,
                stages: &[
                    ("design-executive", 0, false),
                    ("design-technical", 1, false),
                    ("input-appendix", 2, false),
                    ("performance", 3, false),
                    ("performance-pages", 3, true),
                ],
            },
            destination,
            config,
            cancel,
            &mut runtime,
        );
        if let Err(error) = outcome {
            runtime.renderer_status = if cancel.load(Ordering::SeqCst) {
                "cancelled"
            } else {
                "failed"
            }
            .into();
            // Staging diagnostics can contain local runtime context. They are
            // deliberately excluded from all accepted/portable product manifests.
            let _ = report::write_new(
                &destination.join("design-render-failure.json"),
                &json_bytes(&runtime)?,
            );
            return Err(error.context(
                "Design rendering stopped; staging is diagnostic only and cannot be published",
            ));
        }
        runtime.renderer_status = "completed".into();
        runtime.renderer_message="Pinned offline renderer completed selected products. Rendering is not scientific qualification.".into();
    }
    checkpoint(cancel)?;
    report::check_budget(destination)?;
    runtime.artifacts.sort_by(|a, b| a.path.cmp(&b.path));
    let mut manifest = DesignProductManifest {
        schema_version: PRODUCT_SCHEMA.into(),
        content_sha256: content.content_sha256.clone(),
        products_sha256: String::new(),
        products: runtime.artifacts.clone(),
    };
    manifest.products_sha256 = manifest.identity()?;
    manifest.validate()?;
    let bytes = json_bytes(&manifest)?;
    ensure!(
        bytes.len() as u64 <= MAX_MANIFEST_BYTES,
        "Product manifest exceeds its bound"
    );
    let mut budget = PackageBudget::default();
    for p in &manifest.products {
        budget.add(&p.path, p.bytes)?;
    }
    budget.add("design-products.json", bytes.len() as u64)?;
    checkpoint(cancel)?;
    report::write_new(&destination.join("design-products.json"), &bytes)?;
    let inspection = inspect_directory(destination, cancel)?;
    Ok(DesignReportRender {
        inspection,
        runtime,
    })
}

pub fn inspect_directory(directory: &Path, cancel: &AtomicBool) -> Result<DesignReportInspection> {
    checkpoint(cancel)?;
    let _guard = persistence::DirectoryGuard::open(directory)?;
    let manifest_bytes = read(
        directory,
        "design-products.json",
        MAX_MANIFEST_BYTES,
        cancel,
    )?;
    let products: DesignProductManifest = serde_json::from_slice(&manifest_bytes)?;
    products.validate()?;
    let mut expected: BTreeSet<_> = products.products.iter().map(|p| p.path.clone()).collect();
    expected.insert("design-products.json".into());
    let mut actual = BTreeSet::new();
    for entry in fs::read_dir(directory)? {
        ensure!(
            actual.len() <= MAX_PRODUCTS,
            "Unbounded extra design-report members"
        );
        let e = entry?;
        ensure!(
            e.file_type()?.is_file(),
            "Only declared regular products may inhabit a design-report directory"
        );
        actual.insert(
            e.file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Non-UTF8 report path"))?,
        );
    }
    ensure!(
        expected == actual,
        "Design bundle has missing or unlisted members"
    );
    let mut budget = PackageBudget::default();
    budget.add("design-products.json", manifest_bytes.len() as u64)?;
    let mut total_bytes = manifest_bytes.len() as u64;
    let mut content_bytes = None;
    for p in &products.products {
        checkpoint(cancel)?;
        let bytes = read(directory, &p.path, p.bytes, cancel)?;
        ensure!(
            bytes.len() as u64 == p.bytes && report::sha(&bytes) == p.sha256,
            "Design product integrity failed: {}",
            p.path
        );
        if p.path.ends_with(".pdf") {
            ensure!(bytes.starts_with(b"%PDF-"), "Retained PDF header differs");
        }
        if p.path.ends_with(".png") {
            png_admission(&bytes)?;
        }
        budget.add(&p.path, p.bytes)?;
        total_bytes += p.bytes;
        if p.path == "design-content.json" {
            admit_content_bytes(p.bytes, MAX_CONTENT_BYTES)?;
            content_bytes = Some(bytes);
        }
    }
    let content: DesignReportContent =
        serde_json::from_slice(&content_bytes.context("Design content missing")?)?;
    content.validate()?;
    ensure!(
        content.content_sha256 == products.content_sha256,
        "Design content/manifest identity differs"
    );
    // Recompute owned non-native products to reject resealed foreign SVG/HTML,
    // mismatched changes and invented overlays. No Typst or engine is needed.
    // This v1 codec has one owned deterministic content algorithm. A missing
    // exact native template/font installation prevents regeneration, not the
    // complete family/data/owned-image checks or inspection of issued PDFs.
    {
        let (_, owned) = owned_products(&content)?;
        for (name, bytes) in &owned {
            let actual = products
                .products
                .iter()
                .find(|p| p.path == *name)
                .context("Required owned report product missing")?;
            ensure!(
                actual.bytes == bytes.len() as u64 && actual.sha256 == report::sha(bytes),
                "Owned report content differs: {name}"
            );
        }
        let extras: Vec<_> = products
            .products
            .iter()
            .filter(|p| !owned.contains_key(&p.path))
            .collect();
        if content.include_pdf {
            for name in [
                "design-executive.pdf",
                "design-technical.pdf",
                "input-appendix.pdf",
                "performance.pdf",
            ] {
                ensure!(
                    extras.iter().any(|p| p.path == name),
                    "Required PDF missing"
                );
            }
            let pages: Vec<_> = extras
                .iter()
                .filter(|p| p.path.starts_with("performance-page-") && p.path.ends_with(".png"))
                .collect();
            ensure!(
                !pages.is_empty() && pages.len() <= 128,
                "Performance PNG sequence missing or oversized"
            );
            for (index, page) in pages.iter().enumerate() {
                ensure!(
                    page.path == format!("performance-page-{:03}.png", index + 1),
                    "Performance PNG page sequence is incomplete or noncanonical"
                );
            }
            ensure!(extras.len() == 4 + pages.len(), "Unexpected native product");
            for product in extras {
                ensure!(
                    matches!(
                        product.path.as_str(),
                        "design-executive.pdf"
                            | "design-technical.pdf"
                            | "input-appendix.pdf"
                            | "performance.pdf"
                    ) || (product.path.starts_with("performance-page-")
                        && product.path.ends_with(".png")),
                    "Unexpected native product"
                );
            }
        } else {
            ensure!(
                extras.is_empty(),
                "HTML-only report contains unexpected products"
            );
        }
    }
    checkpoint(cancel)?;
    Ok(DesignReportInspection {
        content,
        products,
        manifest_file_sha256: report::sha(&manifest_bytes),
        total_bytes,
    })
}
pub fn summary(value: &DesignReportInspection) -> DesignReportSummary {
    let data = &value.content.workbench;
    let bind = |c: &EnergyReportCase| DesignRunBinding {
        run_id: c.run_id.clone(),
        record_sha256: c.record_sha256.clone(),
        design_id: c.id.clone(),
    };
    let mut sensitivity_run_ids: Vec<String> = value
        .content
        .sensitivity
        .as_ref()
        .map(|e| {
            e.result
                .assessment
                .runs
                .iter()
                .map(|r| r.run_id.clone())
                .collect()
        })
        .unwrap_or_default();
    sensitivity_run_ids.sort();
    sensitivity_run_ids.dedup();
    DesignReportSummary {
        id: value.content.content_sha256.clone(),
        path: format!("design-reports/{}", value.content.content_sha256),
        content_sha256: value.content.content_sha256.clone(),
        manifest_file_sha256: value.manifest_file_sha256.clone(),
        products_sha256: value.products.products_sha256.clone(),
        baseline: bind(&data.base.baseline),
        proposed: bind(&data.base.proposed),
        sensitivity_run_ids,
        title: data.metadata.title.clone(),
        report_revision: data.metadata.revision.clone(),
        include_pdf: value.content.include_pdf,
    }
}
fn reports(project: &Project) -> &[DesignReportSummary] {
    project
        .design_reports
        .as_ref()
        .map(|index| index.reports.as_slice())
        .unwrap_or(&[])
}
pub fn validate_index(project: &Project) -> Result<()> {
    if let Some(index) = &project.design_reports {
        ensure!(
            index.schema_version == INDEX_SCHEMA && index.reports.len() <= MAX_REPORTS,
            "Invalid bounded design-report index"
        );
        ensure!(
            serde_json::to_vec(index)?.len() as u64 <= MAX_MANIFEST_BYTES,
            "Design-report index too large"
        );
        let mut ids = BTreeSet::new();
        for s in &index.reports {
            ensure!(
                digest(&s.id)
                    && s.id == s.content_sha256
                    && s.path == format!("design-reports/{}", s.id)
                    && digest(&s.manifest_file_sha256)
                    && digest(&s.products_sha256)
                    && ids.insert(&s.id),
                "Invalid or duplicate design-report identity/path"
            );
            ensure!(
                !s.title.is_empty()
                    && s.title.len() <= 256
                    && s.report_revision.len() <= 80
                    && s.sensitivity_run_ids.len() <= 4,
                "Design-report summary exceeds its bound"
            );
            ensure!(
                s.baseline.design_id == "A"
                    && s.proposed.design_id == "B"
                    && s.baseline.run_id != s.proposed.run_id,
                "Design report requires distinct explicitly selected A/B run bindings"
            );
            ensure!(
                s.sensitivity_run_ids
                    .windows(2)
                    .all(|pair| pair[0] < pair[1]),
                "Sensitivity run bindings must be unique and canonically ordered"
            );
            for binding in [&s.baseline, &s.proposed] {
                ensure!(
                    digest(&binding.record_sha256),
                    "Invalid design-report run identity"
                );
                ensure!(
                    uuid::Uuid::parse_str(&binding.run_id)?.to_string() == binding.run_id,
                    "Design report requires canonical retained run UUIDs"
                );
                let runs = project
                    .energy
                    .as_ref()
                    .context("Design report requires retained energy evidence")?;
                ensure!(
                    runs.runs.iter().any(|r| r.id == binding.run_id
                        && r.alternative_id == binding.design_id
                        && r.record_sha256 == binding.record_sha256),
                    "Design report refers to absent or different retained run"
                );
            }
            for id in &s.sensitivity_run_ids {
                ensure!(
                    project
                        .energy
                        .as_ref()
                        .is_some_and(|energy| energy.runs.iter().any(|r| r.id == *id)),
                    "Design report sensitivity refers to absent run"
                );
            }
        }
    }
    Ok(())
}
pub fn validate_preserved(previous: &Project, candidate: &Project) -> Result<()> {
    validate_index(candidate)?;
    for before in reports(previous) {
        ensure!(
            reports(candidate).iter().any(|after| after == before),
            "Accepted design reports cannot be removed, rebound or overwritten"
        );
    }
    Ok(())
}
pub fn append(project: &Project, value: &DesignReportSummary) -> Result<Project> {
    validate_index(project)?;
    let mut candidate = project.clone();
    let index = candidate
        .design_reports
        .get_or_insert_with(|| DesignReportIndex {
            schema_version: INDEX_SCHEMA.into(),
            reports: vec![],
        });
    ensure!(index.reports.len()<MAX_REPORTS&&!index.reports.iter().any(|r|r.id==value.id),"Design report already exists or 32-report capacity reached; preserve the full study and use a separately scoped document");
    index.reports.push(value.clone());
    validate_preserved(project, &candidate)?;
    Ok(candidate)
}
pub fn inspect(
    root: &Path,
    project: &Project,
    id: &str,
    cancel: &AtomicBool,
) -> Result<DesignReportInspection> {
    validate_index(project)?;
    let expected = reports(project)
        .iter()
        .find(|r| r.id == id)
        .context("Unknown immutable design report")?;
    let result = inspect_directory(
        &root.join(persistence::relative_path(&expected.path)?),
        cancel,
    )?;
    ensure!(
        &summary(&result) == expected,
        "Design-report index differs from immutable content/products"
    );
    if let Some(evidence) = &result.content.sensitivity {
        validate_retained_sensitivity_reference(
            project,
            &evidence.session_id,
            &evidence.receipt_reference,
        )?;
    }
    if let Some(binding) = &result.content.drawing {
        binding.verify_project(
            root,
            project,
            &result.content.workbench,
            &result.content.frames,
            cancel,
        )?;
    }
    Ok(result)
}
fn validate_retained_sensitivity_reference(
    project: &Project,
    session_id: &str,
    reference: &crate::energy_sensitivity_workflow::EnergySensitivityReference,
) -> Result<()> {
    ensure!(
        reference.id == session_id,
        "Design report sensitivity session differs from its receipt identity"
    );
    let retained = project
        .energy
        .as_ref()
        .and_then(|energy| energy.workbench.as_ref())
        .and_then(|workbench| workbench.sensitivity.iter().find(|r| r.id == session_id))
        .context("Design report sensitivity receipt is not retained")?;
    ensure!(retained==reference,"Design report sensitivity receipt/event hashes differ from the retained exact-pair session");
    Ok(())
}
/// Return only a selected integrity-checked inert product. SVG is restricted to
/// owned regenerated image markup; PDF display remains an observed UI claim,
/// never inferred from a successful byte read or external handoff.
pub fn product_bytes(
    root: &Path,
    project: &Project,
    id: &str,
    product: &str,
    cancel: &AtomicBool,
) -> Result<(String, Vec<u8>)> {
    let value = inspect(root, project, id, cancel)?;
    let selected = value
        .products
        .products
        .iter()
        .find(|p| p.path == product)
        .context("Product is absent from this exact retained design report")?;
    let media = if product.ends_with(".pdf") {
        "application/pdf"
    } else if product.ends_with(".png") {
        "image/png"
    } else if product.ends_with(".svg") {
        value.content.ensure_supported_regeneration()?;
        "image/svg+xml"
    } else {
        anyhow::bail!("Only verified PDF/PNG/owned SVG products are available for preview or copy")
    };
    let bytes = read(
        &root.join(format!("design-reports/{id}")),
        product,
        selected.bytes,
        cancel,
    )?;
    ensure!(
        report::sha(&bytes) == selected.sha256,
        "Selected product changed after inspection"
    );
    checkpoint(cancel)?;
    Ok((media.into(), bytes))
}
pub fn verify_project_with_provider<'a>(
    root: &Path,
    project: &Project,
    cancel: &AtomicBool,
    mut provider: impl FnMut(&str) -> Result<&'a crate::energy::EnergyRunRecord>,
) -> Result<()> {
    validate_index(project)?;
    let mut locations = BTreeMap::new();
    for s in reports(project) {
        let value = inspect(root, project, &s.id, cancel)?;
        for case in [
            &value.content.workbench.base.baseline,
            &value.content.workbench.base.proposed,
        ] {
            checkpoint(cancel)?;
            crate::report_issues::validate_case_record(case, provider(&case.run_id)?)?;
            crate::report_issues::validate_case_location(root, case, &mut locations, cancel)?;
        }
        if let Some(e) = &value.content.sensitivity {
            for expected in &e.result.assessment.runs {
                checkpoint(cancel)?;
                crate::report_issues::validate_sensitivity_record(
                    expected,
                    provider(&expected.run_id)?,
                )?;
            }
        }
    }
    Ok(())
}
pub fn verify_project(root: &Path, project: &Project, cancel: &AtomicBool) -> Result<()> {
    validate_index(project)?;
    let mut locations = BTreeMap::new();
    for s in reports(project) {
        let value = inspect(root, project, &s.id, cancel)?;
        for case in [
            &value.content.workbench.base.baseline,
            &value.content.workbench.base.proposed,
        ] {
            checkpoint(cancel)?;
            let run = crate::energy_services::selected_run(root, project, &case.run_id)?;
            crate::report_issues::validate_case_record(case, &run)?;
            crate::report_issues::validate_case_location(root, case, &mut locations, cancel)?;
        }
        if let Some(e) = &value.content.sensitivity {
            for expected in &e.result.assessment.runs {
                checkpoint(cancel)?;
                let run = crate::energy_services::selected_run(root, project, &expected.run_id)?;
                crate::report_issues::validate_sensitivity_record(expected, &run)?;
            }
        }
    }
    Ok(())
}
pub fn evidence_members(root: &Path, project: &Project) -> Result<Vec<String>> {
    let cancel = AtomicBool::new(false);
    let mut members = vec![];
    for s in reports(project) {
        let value = inspect(root, project, &s.id, &cancel)?;
        members.extend(
            value
                .products
                .products
                .iter()
                .map(|p| format!("{}/{}", s.path, p.path)),
        );
        members.push(format!("{}/design-products.json", s.path));
    }
    Ok(members)
}
/// Complete copy; destination must be fresh. A failure never receives a final
/// manifest. Caller publishes the project manifest only after exact admission.
pub fn copy_bundle(
    source: &Path,
    destination: &Path,
    cancel: &AtomicBool,
) -> Result<DesignReportInspection> {
    let before = inspect_directory(source, cancel)?;
    checkpoint(cancel)?;
    report::create_fresh(destination)?;
    for p in &before.products.products {
        let bytes = read(source, &p.path, p.bytes, cancel)?;
        ensure!(
            report::sha(&bytes) == p.sha256,
            "Source changed during design-report copy"
        );
        report::write_new(&destination.join(&p.path), &bytes)?;
    }
    let manifest = read(source, "design-products.json", MAX_MANIFEST_BYTES, cancel)?;
    ensure!(
        report::sha(&manifest) == before.manifest_file_sha256,
        "Manifest changed during design-report copy"
    );
    checkpoint(cancel)?;
    report::write_new(&destination.join("design-products.json"), &manifest)?;
    let after = inspect_directory(destination, cancel)?;
    ensure!(
        summary(&before) == summary(&after),
        "Design copy identity differs"
    );
    Ok(after)
}
pub fn copy_evidence(
    source: &Path,
    destination: &Path,
    project: &Project,
    cancel: &AtomicBool,
) -> Result<()> {
    for s in reports(project) {
        let before = inspect(source, project, &s.id, cancel)?;
        let target = destination.join(persistence::relative_path(&s.path)?);
        if target.exists() {
            let existing = inspect_directory(&target, cancel)?;
            ensure!(
                summary(&existing) == summary(&before),
                "Conflicting destination design report"
            );
        } else {
            copy_bundle(&source.join(&s.path), &target, cancel)?;
        }
    }
    Ok(())
}
pub fn regenerate(
    root: &Path,
    project: &Project,
    id: &str,
    destination: &Path,
    typst: Option<&TypstConfig>,
    cancel: &AtomicBool,
) -> Result<DesignReportRender> {
    let before = inspect(root, project, id, cancel)?;
    before.content.ensure_supported_regeneration()?;
    ensure!(!before.content.include_pdf || typst.is_some(),"The issued PDF products require the separately installed pinned Typst renderer; inspection and existing PDF preview remain available without it");
    let after = render(
        &before.content,
        destination,
        if before.content.include_pdf {
            typst
        } else {
            None
        },
        cancel,
    )?;
    ensure!(before.products.products_sha256==after.inspection.products.products_sha256,"Regenerated deterministic products differ; originals remain unchanged and new files are diagnostic only");
    Ok(after)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_content_admission_counts_pretty_bytes_and_trailing_newline() {
        let value = serde_json::json!({"rows": [{"label": "空間", "value": 12.5}]});
        let compact = serde_json::to_vec(&value).unwrap();
        let actual = json_bytes(&value).unwrap();
        assert!(actual.len() > compact.len());
        assert_eq!(actual.last(), Some(&b'\n'));
        // Compact admission alone would accept this smaller limit. Production
        // serializes through the same helper before launching a renderer.
        assert!(content_json_bytes(&value, compact.len() as u64).is_err());
        assert_eq!(
            content_json_bytes(&value, actual.len() as u64).unwrap(),
            actual
        );
        assert!(content_json_bytes(&value, actual.len() as u64 - 1).is_err());
        assert!(admit_content_bytes(MAX_CONTENT_BYTES, MAX_CONTENT_BYTES).is_ok());
        assert!(admit_content_bytes(MAX_CONTENT_BYTES + 1, MAX_CONTENT_BYTES).is_err());
        assert!(admit_content_bytes(u64::MAX, MAX_CONTENT_BYTES).is_err());
    }

    /// Arithmetic fixture stays NotExecuted/NotChecked. Only pure presentation
    /// functions are tested with it; no accepted/native engine record is forged.
    fn fixture() -> (DesignReportContent, PairedDesignFigures) {
        let base = synthetic_energy_report().unwrap();
        let data = EnergyReportWorkbenchData::new(
            base,
            EnergyReportMetadata::default(),
            vec![
                EnergyReportProfile::Executive,
                EnergyReportProfile::Technical,
            ],
            EnergyReportAttachmentSummary {
                included_ids: vec![],
                excluded_count: 0,
                missing_unselected_count: 0,
                inapplicable_unselected_count: 0,
            },
            crate::energy_report_workbench::template_sha256(),
        )
        .unwrap();
        let frames = default_design_frames(
            &data.base.baseline.model,
            &data.base.baseline.study.calendar,
            &data.base.proposed.model,
            &data.base.proposed.study.calendar,
        )
        .unwrap();
        let mut content=DesignReportContent{schema_version:CONTENT_SCHEMA.into(),content_sha256:String::new(),workbench:data,sensitivity:None,proposal_declarations:vec![],frames,analytical_scope:"Original synthetic arithmetic/rendering fixture; not simulated or whole-building evidence.".into(),north_declaration:None,renderer:renderer_identity().unwrap(),include_pdf:false,drawing:None};
        content.content_sha256 = content.identity().unwrap();
        let case = |id: &str, c: &EnergyReportCase| DesignFigureCase {
            design_id: id.into(),
            label: c.label.clone(),
            model: c.model.clone(),
            calendar: c.study.calendar.clone(),
            study: Some(c.study.clone()),
            scope_revision_sha256: None,
            evidence: DesignFigureEvidence::NotSimulated {
                reason: "Isolated presentation fixture; no native execution.".into(),
            },
        };
        let figures = paired_design_figures(&PairedDesignFigureRequest {
            schema_version: DESIGN_FIGURE_SCHEMA.into(),
            baseline: case("A", &content.workbench.base.baseline),
            proposed: case("B", &content.workbench.base.proposed),
            frames: content.frames.clone(),
            scope: content.analytical_scope.clone(),
            north_declaration: None,
        })
        .unwrap();
        (content, figures)
    }
    fn temp() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "greenlens-design-report-test-{}",
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn unexecuted_arithmetic_cannot_publish_or_become_a_retained_figure() {
        let (content, _) = fixture();
        assert!(content
            .validate()
            .unwrap_err()
            .to_string()
            .contains("completed"));
        let destination = temp();
        assert!(render(&content, &destination, None, &AtomicBool::new(false)).is_err());
        assert!(!destination.exists());
        assert_eq!(
            content.workbench.base.baseline.assessment.execution,
            EnergyExecutionStatus::NotExecuted
        );
        assert_eq!(
            content
                .workbench
                .base
                .proposed
                .assessment
                .evidence_integrity,
            EnergyIntegrityScope::NotChecked
        );
    }
    #[test]
    fn cancellation_and_missing_renderer_leave_no_published_directory() {
        let (content, _) = fixture();
        let destination = temp();
        assert!(render(&content, &destination, None, &AtomicBool::new(true))
            .unwrap_err()
            .to_string()
            .contains("cancelled"));
        assert!(!destination.exists());
        assert!(copy_bundle(&destination, &temp(), &AtomicBool::new(true)).is_err());
    }
    #[test]
    fn complete_change_register_and_shared_frame_are_visible_without_false_results() {
        let (content, figures) = fixture();
        let docs = documents(&content, &figures).unwrap();
        assert_eq!(docs.len(), 4);
        let technical = serde_json::to_string(&docs[1]).unwrap();
        for change in &figures.changes {
            assert!(technical.contains(&change.object_id));
        }
        assert!(technical.contains(&format!(
            "Displayed {} of {} changes",
            figures.change_count, figures.change_count
        )));
        assert!(technical.contains("Exact-pair numerical sensitivity — missing"));
        assert!(technical.contains("no architectural reference"));
        assert!(technical.contains("not_executed"));
        assert!(technical.contains("not_checked"));
        assert!(technical.contains("No geometry sampling or spatial results are implied"));
        assert!(technical.contains("Own coincident facility peak"));
        assert!(technical.contains("Gross floor area denominator"));
        let csv = change_csv(&figures).unwrap();
        assert_eq!(csv.lines().count(), figures.change_count + 1);
        let p = content.workbench.base.presentation().unwrap();
        assert!(technical.contains(&report::cell(&p.baseline.annual.facility)));
        assert!(technical.contains(&report::cell(&p.proposed.annual.facility)));
        assert_eq!(
            serde_json::to_vec(&docs).unwrap(),
            serde_json::to_vec(&documents(&content, &figures).unwrap()).unwrap()
        );
    }
    #[test]
    fn bound_presentation_separates_documentation_and_complete_review_resolution() {
        let (mut content, figures) = fixture();
        let (_, mut revision, selection) = crate::design_report_drawing::tests::fixture();
        // Presentation-only exception stress fixture, not a valid accepted
        // revision or executed run. Publication remains forbidden by validate.
        revision.provenance.documentation_complete = true;
        revision.review_decisions = (0..81)
            .map(|i| greenlens_core::study_revision::ReviewDecision {
                row_id: format!("review-{i}"),
                resolution: greenlens_core::study_revision::ReviewResolution::Unresolved,
                rationale: format!("Rationale {i}: {}", "未確認 ".repeat(140)),
            })
            .collect();
        revision.reviewed_rows = Some(
            (0..81)
                .map(|i| greenlens_core::study_revision::ReviewedRowBinding {
                    row_id: format!("review-{i}"),
                    kind: if i == 80 {
                        greenlens_core::study_revision::RevisionRowKind::Reference
                    } else {
                        greenlens_core::study_revision::RevisionRowKind::Input
                    },
                    key: format!("affected-{i}"),
                })
                .collect(),
        );
        content.schema_version = CONTENT_SCHEMA_V2.into();
        content.renderer = bound_renderer_identity().unwrap();
        content.drawing = Some(crate::design_report_drawing::DesignReportDrawing {
            schema_version: crate::design_report_drawing::DRAWING_SCHEMA.into(),
            selection,
            scope_revision: revision,
            frames: content.frames.clone(),
            context: None,
        });
        let docs = documents(&content, &figures).unwrap();
        let executive = serde_json::to_string(&docs[0]).unwrap();
        assert!(executive.contains("Documentation complete: true"));
        assert!(executive.contains("81/81 decisions remain explicitly unresolved"));
        let technical = serde_json::to_string(&docs[1]).unwrap();
        assert!(technical.contains("Displayed 80 of 81 unresolved decisions"));
        assert!(technical.contains("Input: affected-0 (review-0)"));
        assert!(technical.contains(&"未確認 ".repeat(140)));
        let complete =
            serde_json::to_string(&content.drawing.as_ref().unwrap().scope_revision).unwrap();
        assert!(complete.contains("affected-80"));
        assert!(complete.contains("Rationale 80:"));
        assert!(technical.contains("design-scope-revision.json"));
        assert!(content.validate().is_err());
    }

    #[test]
    fn executive_retains_outcomes_and_limits_without_the_full_technical_register() {
        let (content, mut figures) = fixture();
        // The executive must select an axonometric overlay by its contract,
        // not rely on the supplied frame/product order.
        figures.request.frames.reverse();
        figures.products.reverse();
        let docs = documents(&content, &figures).unwrap();
        let executive = &docs[0];
        let text = serde_json::to_string(executive).unwrap();
        let technical = serde_json::to_string(&docs[1]).unwrap();
        for title in [
            "Measured comparison findings",
            "Measured outcomes from selected evidence",
            "Selected evidence and scientific status",
            "Scope and interpretation",
            "Design hypothesis and tradeoffs",
            "Exact-pair numerical sensitivity — missing",
        ] {
            assert!(text.contains(title));
        }
        for technical_only in [
            "Exact run bindings and separate assessment axes",
            "Parameter annotations — exact retained inputs",
            "Complete object-change register",
        ] {
            assert!(!text.contains(technical_only));
            assert!(technical.contains(technical_only));
        }
        assert!(text.contains("not_executed"));
        assert!(text.contains("not_checked"));
        assert!(text.contains("No independent approval or professional sign-off"));
        assert!(text.contains("no architectural reference"));
        assert!(text.contains("no rationale is invented"));
        assert!(text.contains("complete project"));
        for case in [
            &content.workbench.base.baseline,
            &content.workbench.base.proposed,
        ] {
            assert!(text.contains(&case.run_id));
            assert!(executive
                .sections
                .iter()
                .flat_map(|s| &s.rows)
                .flatten()
                .any(|value| value.contains(&case.assessment.scientific.reason)));
        }
        let p = content.workbench.base.presentation().unwrap();
        for value in [
            report::cell(&p.baseline.annual.facility),
            report::cell(&p.proposed.annual.facility),
            report::cell(&p.facility_energy_savings.absolute),
            report::cell(&p.facility_peak_savings.absolute),
        ] {
            assert!(text.contains(&value));
        }
        let images = executive
            .sections
            .iter()
            .filter_map(|s| s.image.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(images.len(), 1);
        let product = figures
            .products
            .iter()
            .find(|p| &p.filename == images[0])
            .unwrap();
        assert_eq!(product.side, "overlay");
        assert_eq!(
            figures
                .request
                .frames
                .iter()
                .find(|f| f.id == product.frame_id)
                .unwrap()
                .kind,
            DesignViewKind::Axonometric
        );
        assert_eq!(
            docs[1]
                .sections
                .iter()
                .filter(|s| {
                    s.image
                        .as_ref()
                        .is_some_and(|image| figures.products.iter().any(|p| &p.filename == image))
                })
                .count(),
            figures.products.len()
        );
        let excerpt = executive_excerpt(&"節能假設以及限制。".repeat(80), 240);
        assert!(excerpt.starts_with(
            &"節能假設以及限制。"
                .repeat(80)
                .chars()
                .take(240)
                .collect::<String>()
        ));
        assert!(excerpt
            .ends_with("[complete declaration in the technical report and design-proposals.json]"));
    }
    #[test]
    fn metadata_frame_and_exact_input_changes_have_separate_content_identities() {
        let (content, _) = fixture();
        let before = content.identity().unwrap();
        let physical = content
            .workbench
            .base
            .baseline
            .physical_input_sha256
            .clone();
        let mut changed = content.clone();
        changed.workbench.metadata.title = "A different report title".into();
        assert_ne!(before, changed.identity().unwrap());
        assert_eq!(
            physical,
            changed.workbench.base.baseline.physical_input_sha256
        );
        let mut changed = content.clone();
        changed.frames[0].vertical_span_m += 1.;
        assert_ne!(before, changed.identity().unwrap());
        assert_eq!(
            physical,
            changed.workbench.base.baseline.physical_input_sha256
        );
        let serialized = serde_json::to_string(&content).unwrap();
        assert!(!serialized.contains("executable_path"));
        assert!(!serialized.contains("started_utc"));
        assert!(!serialized.contains("renderer_work_id"));
        assert_eq!(
            before,
            serde_json::from_str::<DesignReportContent>(&serialized)
                .unwrap()
                .identity()
                .unwrap()
        );
    }
    #[test]
    fn invalid_paths_duplicate_members_and_oversized_content_are_rejected() {
        let product = |name: &str, bytes: u64| ReportArtifact {
            path: name.into(),
            bytes,
            sha256: "1".repeat(64),
        };
        let mut manifest = DesignProductManifest {
            schema_version: PRODUCT_SCHEMA.into(),
            content_sha256: "2".repeat(64),
            products_sha256: String::new(),
            products: vec![product("design-content.json", 1)],
        };
        manifest.products_sha256 = manifest.identity().unwrap();
        assert!(manifest.validate().is_ok());
        for name in [
            "../escape.json",
            "C:/outside.json",
            "design-products.json",
            "design-render-failure.json",
        ] {
            let mut bad = manifest.clone();
            bad.products.push(product(name, 1));
            bad.products_sha256 = bad.identity().unwrap();
            assert!(bad.validate().is_err());
        }
        let mut bad = manifest.clone();
        bad.products.push(product("design-content.json", 1));
        bad.products_sha256 = bad.identity().unwrap();
        assert!(bad.validate().is_err());
        let mut bad = manifest;
        bad.products[0].bytes = persistence::MAX_EVIDENCE_FILE_BYTES + 1;
        bad.products_sha256 = bad.identity().unwrap();
        assert!(bad.validate().is_err());
        assert_eq!(csv_field("=malicious"), "\"'=malicious\"");
    }
    #[test]
    fn html_escapes_declared_metadata_and_never_drops_changed_objects() {
        let (mut content, figures) = fixture();
        content.workbench.metadata.title = "<script>alert('private')</script>".into();
        let docs = documents(&content, &figures).unwrap();
        let mut generated = figures
            .products
            .iter()
            .map(|p| (p.filename.as_str(), p.svg.as_bytes().to_vec()))
            .collect::<Vec<_>>();
        generated.extend([
            (
                "baseline-model.svg",
                report::model_svg(&content.workbench.base.baseline).into_bytes(),
            ),
            (
                "proposed-model.svg",
                report::model_svg(&content.workbench.base.proposed).into_bytes(),
            ),
            (
                "annual.svg",
                report::chart(&content.workbench.base.presentation().unwrap(), false).into_bytes(),
            ),
            (
                "monthly.svg",
                report::chart(&content.workbench.base.presentation().unwrap(), true).into_bytes(),
            ),
        ]);
        let html = report::html(&docs[1], &generated).unwrap();
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("default-src 'none'"));
        for change in &figures.changes {
            assert!(html.contains(&report::escaped(&change.object_id)));
        }
        assert!(html.contains("NOT SIMULATED"));
    }
    #[test]
    fn sensitivity_receipt_binding_rejects_resealed_events_and_missing_session() {
        // Metadata-only relationship fixture; it does not claim executed or
        // admitted sensitivity evidence and cannot publish a report.
        use crate::energy_sensitivity_workflow::{
            EnergySensitivityEventSeal, EnergySensitivityReference,
        };
        let mut project = crate::energy_services::example_document();
        let id = uuid::Uuid::new_v4().to_string();
        let reference = EnergySensitivityReference {
            id: id.clone(),
            path: format!("energy/sensitivity/{id}"),
            plan_sha256: "a".repeat(64),
            plan_file_sha256: "b".repeat(64),
            events: vec![EnergySensitivityEventSeal {
                sequence: 1,
                file_sha256: "c".repeat(64),
            }],
        };
        assert!(validate_retained_sensitivity_reference(&project, &id, &reference).is_err());
        project
            .energy
            .as_mut()
            .unwrap()
            .workbench
            .get_or_insert_with(Default::default)
            .sensitivity
            .push(reference.clone());
        validate_retained_sensitivity_reference(&project, &id, &reference).unwrap();
        let mut changed = reference.clone();
        changed.events[0].file_sha256 = "d".repeat(64);
        assert!(
            validate_retained_sensitivity_reference(&project, &id, &changed)
                .unwrap_err()
                .to_string()
                .contains("receipt/event hashes")
        );
        let mut changed = reference.clone();
        changed.plan_file_sha256 = "e".repeat(64);
        assert!(validate_retained_sensitivity_reference(&project, &id, &changed).is_err());
        assert!(validate_retained_sensitivity_reference(
            &project,
            &uuid::Uuid::new_v4().to_string(),
            &reference
        )
        .is_err());
        assert_eq!(
            project
                .energy
                .as_ref()
                .unwrap()
                .workbench
                .as_ref()
                .unwrap()
                .sensitivity[0],
            reference
        );
    }
    #[test]
    fn summary_admission_requires_distinct_designs_and_unique_sensitivity_runs() {
        // Index-only fixture with absent engine files; validating this shape
        // never establishes retained product or engine evidence integrity.
        let mut project = crate::energy_services::example_document();
        let a = uuid::Uuid::new_v4().to_string();
        let b = uuid::Uuid::new_v4().to_string();
        for (id, side) in [(&a, "A"), (&b, "B")] {
            project
                .energy
                .as_mut()
                .unwrap()
                .runs
                .push(crate::energy::EnergyRunSummary {
                    id: id.clone(),
                    alternative_id: side.into(),
                    path: format!("energy/runs/{id}"),
                    physical_input_hash: "a".repeat(64),
                    record_sha256: "b".repeat(64),
                });
        }
        let bind = |id: &String, side: &str| DesignRunBinding {
            run_id: id.clone(),
            record_sha256: "b".repeat(64),
            design_id: side.into(),
        };
        let summary = DesignReportSummary {
            id: "c".repeat(64),
            path: format!("design-reports/{}", "c".repeat(64)),
            content_sha256: "c".repeat(64),
            manifest_file_sha256: "d".repeat(64),
            products_sha256: "e".repeat(64),
            baseline: bind(&a, "A"),
            proposed: bind(&b, "B"),
            sensitivity_run_ids: vec![],
            title: "Metadata-only fixture".into(),
            report_revision: "Draft".into(),
            include_pdf: false,
        };
        project.design_reports = Some(DesignReportIndex {
            schema_version: INDEX_SCHEMA.into(),
            reports: vec![summary.clone()],
        });
        validate_index(&project).unwrap();
        project.design_reports.as_mut().unwrap().reports[0].sensitivity_run_ids =
            vec![a.clone(), a.clone()];
        assert!(validate_index(&project)
            .unwrap_err()
            .to_string()
            .contains("unique"));
        project.design_reports.as_mut().unwrap().reports[0] = summary.clone();
        project.design_reports.as_mut().unwrap().reports[0].proposed = summary.baseline.clone();
        assert!(validate_index(&project)
            .unwrap_err()
            .to_string()
            .contains("distinct"));
    }
    #[test]
    fn old_content_omits_drawing_and_retains_its_original_identity_codec() {
        // This stays a not-executed arithmetic fixture; no accepted record is
        // fabricated. Check historical serialization, not physical validity.
        let (content, _) = fixture();
        let bytes = serde_json::to_vec(&content).unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert!(value.get("drawing").is_none());
        assert_eq!(content.schema_version, CONTENT_SCHEMA);
        assert_eq!(content.renderer, renderer_identity().unwrap());
        let restored: DesignReportContent = serde_json::from_slice(&bytes).unwrap();
        assert!(restored.drawing.is_none());
        assert_eq!(restored.identity().unwrap(), content.identity().unwrap());
        assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
        let mut unavailable = restored;
        unavailable.schema_version = CONTENT_SCHEMA_V2.into();
        assert!(unavailable
            .validate()
            .unwrap_err()
            .to_string()
            .contains("codec"));
    }
    #[test]
    fn png_dimensions_are_bounded_before_preview_or_copy() {
        // Header admission only; this is not a decoder or a rendered image.
        let header = |width: u32, height: u32| {
            let mut bytes = vec![0u8; 33];
            bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
            bytes[12..16].copy_from_slice(b"IHDR");
            bytes[16..20].copy_from_slice(&width.to_be_bytes());
            bytes[20..24].copy_from_slice(&height.to_be_bytes());
            bytes
        };
        png_admission(&header(4096, 4096)).unwrap();
        for (width, height) in [
            (0, 1),
            (1, 0),
            (8193, 1),
            (8192, 8192),
            (u32::MAX, u32::MAX),
        ] {
            assert!(png_admission(&header(width, height)).is_err());
        }
        assert!(png_admission(b"\x89PNG\r\n\x1a\n").is_err());
        let mut bad = header(100, 100);
        bad[12..16].copy_from_slice(b"TEXT");
        assert!(png_admission(&bad).is_err());
    }
}
