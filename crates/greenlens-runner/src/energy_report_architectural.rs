//! Versioned architectural presentation bound to selected retained energy cases.
//! Reference assets remain inert. Figures are generated from admitted triangles;
//! imported SVG, commands and executable recipes are never accepted.
use super::{escaped, Section};
use crate::{
    persistence::*,
    reference::{ArchitecturalProject, ReferenceAsset},
    Project,
};
use anyhow::{ensure, Context, Result};
use greenlens_core::{
    architectural_scope::*,
    canonical_hash,
    energy::{validate_thermal_model, EnergyReportData, ValidatedThermalModel},
    reference::{ReferenceModel, ReferenceTriangle},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub const REPORT_SCHEMA: &str = "greenlens.architectural-energy-report.v1";
pub const FIGURE_METHOD: &str = "greenlens.reference-orthographic-projection.v1";
pub const MAX_FIGURE_TRIANGLES: usize = 8000;
const MAX_PRESENTED_ROWS: usize = 80;
#[path = "energy_report_aligned_views.rs"]
pub mod aligned_views;
#[path = "energy_report_reference_views.rs"]
mod views;
pub const STORAGE_DISCLOSURE: &str = "All admitted normalized reference revisions and geometry chunks are included in the portable project package. Original proprietary architectural sources and conversion GLB files remain external and are excluded by the declared storage policy. This is a normalized-reference analysis evidence package, not a complete original architectural-source archive. Standalone reports contain figures and complete scope inventories; use the portable package for retained geometry.";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedCaseBinding {
    pub design_id: String,
    pub run_id: String,
    pub record_sha256: String,
    pub model_input_sha256: String,
    pub physical_input_sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceFigureDefinition {
    pub path: String,
    pub method: String,
    pub projection: String,
    pub axes: Option<[usize; 2]>,
    /// None means the exact bound source is unavailable; never a fabricated zero.
    pub source_triangle_count: Option<usize>,
    /// Triangles submitted before section/frame clipping, not visible triangles.
    /// Absent on older definitions; unknown when the exact source is unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sampled_triangle_count: Option<usize>,
    pub depicted_triangle_count: usize,
    pub deterministic_stride: usize,
    pub svg_sha256: String,
    pub caption: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained_view: Option<ReferenceReportView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SensitivityReportEvidence {
    pub session_id: String,
    pub plan: crate::energy_sensitivity::EnergySensitivityPlan,
    pub result: crate::energy_sensitivity::EnergySensitivityResult,
    pub receipt_reference: crate::energy_sensitivity_workflow::EnergySensitivityReference,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchitecturalReportData {
    pub schema_version: String,
    pub base_dataset_sha256: String,
    pub workbench_report_sha256: Option<String>,
    pub selected_cases: Vec<SelectedCaseBinding>,
    pub active_asset_id: String,
    pub reported_asset_id: String,
    pub reported_reference_is_active: bool,
    pub retained_assets: Vec<ReferenceAsset>,
    pub reference: ReferenceModel,
    pub declared_scope: Option<ArchitecturalScope>,
    pub scope_assessment: Option<ScopeAssessment>,
    pub sensitivity: Option<SensitivityReportEvidence>,
    pub storage_disclosure: String,
    pub figures: Vec<ReferenceFigureDefinition>,
    pub report_sha256: String,
}
impl ArchitecturalReportData {
    pub fn identity(&self) -> Result<String> {
        let mut unsigned = self.clone();
        unsigned.report_sha256.clear();
        Ok(canonical_hash(&(REPORT_SCHEMA, unsigned))?)
    }
}

/// Constructed only by this module from integrity-checked admitted geometry.
/// Private figure buffers cannot be deserialized from an imported report.
#[derive(Debug, Clone)]
pub struct ArchitecturalReport {
    pub data: ArchitecturalReportData,
    figures: Vec<(String, Vec<u8>)>,
}

fn retained_models(base: &EnergyReportData) -> Result<BTreeMap<String, ValidatedThermalModel>> {
    base.validate()?;
    [&base.baseline, &base.proposed]
        .iter()
        .map(|case| {
            let model = validate_thermal_model(&case.model, &case.study.calendar)?;
            ensure!(
                model.model_input_sha256 == case.model_input_sha256,
                "Selected report model identity differs from retained inputs"
            );
            Ok((case.id.clone(), model))
        })
        .collect()
}

pub fn prepare_project(
    root: &Path,
    project: &Project,
    base: &EnergyReportData,
    workbench_report_sha256: Option<&str>,
    cancel: &AtomicBool,
) -> Result<Option<ArchitecturalReport>> {
    let Some(extension) = &project.architecture else {
        return Ok(None);
    };
    crate::reference::validate_extension(extension)?;
    let asset = selected_asset(extension)?;
    let manifest = crate::reference::verify_asset(root, asset, cancel)?;
    let stride = manifest
        .triangle_count
        .div_ceil(MAX_FIGURE_TRIANGLES)
        .max(1);
    let mut selected = Vec::with_capacity(MAX_FIGURE_TRIANGLES);
    let mut index = 0usize;
    for chunk in 0..manifest.chunks.len() {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "Architectural report cancelled"
        );
        for triangle in crate::reference::read_chunk(root, asset, chunk)? {
            if index.is_multiple_of(stride) {
                selected.push(triangle);
            }
            index += 1;
        }
    }
    ensure!(
        index == manifest.triangle_count && selected.len() <= MAX_FIGURE_TRIANGLES,
        "Reference figure dimensions differ from admitted geometry"
    );
    let mut report = prepare(
        base,
        workbench_report_sha256,
        extension,
        manifest,
        &selected,
        stride,
    )?;
    let selection = crate::energy_sensitivity_workflow::export_selection(
        root,
        project,
        &base.baseline.run_id,
        &base.proposed.run_id,
    )?;
    if let Some(id) = selection.included_session_id {
        let session = crate::energy_sensitivity_workflow::inspect(root, project, &id)?;
        let result = session
            .result
            .context("Selected finalized sensitivity evidence is missing")?;
        for (slot, case) in [("A10", &base.baseline), ("B10", &base.proposed)] {
            ensure!(
                result.assessment.runs.iter().any(|r| r.slot_id == slot
                    && r.run_id == case.run_id
                    && r.record_sha256 == case.record_sha256
                    && r.physical_input_sha256 == case.physical_input_sha256),
                "Sensitivity evidence does not bind the exact selected report run and record"
            );
        }
        report.data.sensitivity = Some(SensitivityReportEvidence {
            session_id: id,
            plan: session.plan,
            result,
            receipt_reference: session.reference,
        });
        report.data.report_sha256 = report.data.identity()?;
    }
    Ok(Some(report))
}

fn selected_asset(extension: &ArchitecturalProject) -> Result<&ReferenceAsset> {
    // A historical scope uses its retained revision, with an explicit disclosure
    // when it is no longer active. If absent, assess against active as stale.
    extension
        .scope
        .as_ref()
        .and_then(|s| {
            extension.assets.iter().find(|a| {
                a.revision_sha256 == s.reference_revision_sha256
                    && a.geometry_sha256 == s.reference_geometry_sha256
            })
        })
        .or_else(|| {
            extension
                .assets
                .iter()
                .find(|a| a.id == extension.active_asset_id)
        })
        .context("No retained reference revision is available for architectural reporting")
}

fn prepare(
    base: &EnergyReportData,
    workbench_report_sha256: Option<&str>,
    extension: &ArchitecturalProject,
    reference: ReferenceModel,
    triangles: &[ReferenceTriangle],
    stride: usize,
) -> Result<ArchitecturalReport> {
    let models = retained_models(base)?;
    let asset = selected_asset(extension)?;
    ensure!(
        asset.revision_sha256 == reference.revision_sha256
            && asset.geometry_sha256 == reference.geometry_sha256,
        "Report reference is not the selected retained revision"
    );
    let assessment = extension
        .scope
        .as_ref()
        .map(|scope| assess_scope(scope, &reference, &models))
        .transpose()?;
    let mut figures = Vec::new();
    let mut definitions = Vec::new();
    let bound_scope = extension.scope.as_ref().filter(|scope| {
        scope.reference_revision_sha256 == reference.revision_sha256
            && scope.reference_geometry_sha256 == reference.geometry_sha256
    });
    for (name, projection, axes) in [
        ("reference-plan.svg", "Plan", [0, 1]),
        ("reference-front.svg", "Front elevation", [0, 2]),
        ("reference-right.svg", "Right elevation", [1, 2]),
    ] {
        let bytes =
            projection_svg(&reference, bound_scope, triangles, axes, projection).into_bytes();
        let mut caption = format!("{projection}; world-coordinate axes in metres; {} of {} admitted triangles sampled and projected without section/frame clipping using deterministic global stride {stride}. {} Source materials are display-only; this projection is not a thermal field or a semantic area/volume measurement. Full normalized geometry is retained in the portable package.", triangles.len(), reference.triangle_count,
            if stride == 1 { "No triangle reduction." } else { "Illustrative figure reduction only; absent edges are not analytical exclusions." });
        if extension.scope.is_some() && bound_scope.is_none() {
            caption.push_str(" The scope's exact reference is unavailable. This is the active reference for context only; historical mapping colors and retained report frames are not applied to it.");
        }
        definitions.push(ReferenceFigureDefinition {
            path: name.into(),
            method: FIGURE_METHOD.into(),
            projection: projection.into(),
            axes: Some(axes),
            source_triangle_count: Some(reference.triangle_count),
            sampled_triangle_count: Some(triangles.len()),
            depicted_triangle_count: triangles.len(),
            deterministic_stride: stride,
            svg_sha256: super::sha(&bytes),
            caption,
            retained_view: None,
            unavailable_reason: None,
        });
        figures.push((name.into(), bytes));
    }
    if let Some(scope) = &extension.scope {
        for (index, view) in scope.report_views.iter().enumerate() {
            let name = format!("reference-view-{}.svg", index + 1);
            let source_available = view.reference_revision_sha256 == reference.revision_sha256
                && view.reference_geometry_sha256 == reference.geometry_sha256;
            let unavailable_reason = if !source_available {
                Some("The exact reference revision/geometry bound to this view is unavailable; the active reference is different. Geometry is suppressed rather than drawn in a historical frame.".to_owned())
            } else if models.get(&view.design_id).map(|m| &m.model_input_sha256)
                != Some(&view.model_input_sha256)
            {
                Some("The analytical design bound to this view differs from the selected retained energy case. Geometry is suppressed; choose matching retained evidence to reproduce the figure.".to_owned())
            } else {
                None
            };
            let (svg, depicted_triangle_count) = match &unavailable_reason {
                Some(reason) => (views::unavailable(view, reason), 0),
                None => views::svg(&reference, scope, triangles, view),
            };
            let bytes = svg.into_bytes();
            let sampled_triangle_count =
                source_available.then_some(if unavailable_reason.is_none() {
                    triangles.len()
                } else {
                    0
                });
            let caption = match &unavailable_reason {
                Some(reason) => format!("Retained view unavailable: {}. {reason} The complete named-view definition, title and original bindings remain in architecture-report.json. No reference triangles are depicted; source triangle count is recorded only when the exact bound source is available.", view.title),
                None => format!("Retained view: {}. Exact orthographic camera/frame and optional section are bound to reference and design {} in the scope declaration. {} of {} admitted triangles sampled at deterministic stride {stride}; {depicted_triangle_count} sampled triangles have at least one visible original edge after section/frame clipping at SVG precision. A triangle with several visible edges is counted once. No filled cut faces, occlusion removal or resolved thermal field is implied. Local hidden/isolation state is excluded; analytical inclusion is unchanged. A stale scope remains historical and is not rebound.", view.title, view.design_id, triangles.len(), reference.triangle_count),
            };
            definitions.push(ReferenceFigureDefinition {
                path: name.clone(),
                method: "greenlens.reference-retained-orthographic.v1".into(),
                projection: view.title.clone(),
                axes: None,
                source_triangle_count: source_available.then_some(reference.triangle_count),
                sampled_triangle_count,
                depicted_triangle_count,
                deterministic_stride: stride,
                svg_sha256: super::sha(&bytes),
                caption,
                retained_view: Some(view.clone()),
                unavailable_reason,
            });
            figures.push((name, bytes));
        }
    }
    let mut data = ArchitecturalReportData {
        schema_version: REPORT_SCHEMA.into(),
        base_dataset_sha256: base.dataset_sha256()?,
        workbench_report_sha256: workbench_report_sha256.map(str::to_owned),
        selected_cases: [&base.baseline, &base.proposed]
            .iter()
            .map(|c| SelectedCaseBinding {
                design_id: c.id.clone(),
                run_id: c.run_id.clone(),
                record_sha256: c.record_sha256.clone(),
                model_input_sha256: c.model_input_sha256.clone(),
                physical_input_sha256: c.physical_input_sha256.clone(),
            })
            .collect(),
        active_asset_id: extension.active_asset_id.clone(),
        reported_asset_id: asset.id.clone(),
        reported_reference_is_active: asset.id == extension.active_asset_id,
        retained_assets: extension.assets.clone(),
        reference,
        declared_scope: extension.scope.clone(),
        scope_assessment: assessment,
        sensitivity: None,
        storage_disclosure: STORAGE_DISCLOSURE.into(),
        figures: definitions,
        report_sha256: String::new(),
    };
    data.report_sha256 = data.identity()?;
    Ok(ArchitecturalReport { data, figures })
}

impl ArchitecturalReport {
    pub fn validate_binding(&self, base: &EnergyReportData, workbench: Option<&str>) -> Result<()> {
        ensure!(
            self.data.schema_version == REPORT_SCHEMA
                && self.data.report_sha256 == self.data.identity()?
                && self.data.base_dataset_sha256 == base.dataset_sha256()?
                && self.data.workbench_report_sha256.as_deref() == workbench,
            "Architectural report binding differs from selected immutable energy data"
        );
        ensure!(
            self.data.figures.len() == self.figures.len(),
            "Architectural figure count differs"
        );
        for (definition, (name, bytes)) in self.data.figures.iter().zip(&self.figures) {
            ensure!(
                definition.path == *name && definition.svg_sha256 == super::sha(bytes),
                "Architectural figure identity differs"
            );
        }
        Ok(())
    }
    pub(crate) fn products(&self) -> Result<Vec<(String, Vec<u8>)>> {
        let mut products = self.figures.clone();
        products.push((
            "architecture-report.json".into(),
            serde_json::to_vec_pretty(&self.data)?,
        ));
        products.push((
            "architecture-mappings.csv".into(),
            mappings_csv(&self.data).into_bytes(),
        ));
        products.push((
            "architecture-assumptions.csv".into(),
            assumptions_csv(&self.data).into_bytes(),
        ));
        products.push((
            "architecture-reconciliation.csv".into(),
            reconciliation_csv(&self.data).into_bytes(),
        ));
        Ok(products)
    }
    pub(crate) fn summary(&self) -> Result<Section> {
        let d = &self.data;
        let status = d
            .scope_assessment
            .as_ref()
            .map(|a| {
                format!(
                    "{:?}; {} unresolved mappings; {} missing assumptions",
                    a.relationship, a.unresolved_mapping_count, a.missing_assumption_count
                )
            })
            .unwrap_or_else(|| {
                "Scope not declared; coverage unknown; architectural equivalence is not established"
                    .into()
            });
        let mut s = Section::prose("Architectural scope and coverage", vec![status,
            d.declared_scope.as_ref().map(|s| format!("{:?}: {}. {}", s.scope_kind, s.title, s.purpose)).unwrap_or_else(|| "Draft report: define a defensible supported source-to-analysis scope before making coverage claims.".into()),
            format!("Reported reference is {}. Each assessment uses the selected retained energy run inputs, not the current editor geometry.", if d.reported_reference_is_active {"the active revision"} else {"a retained historical revision, different from the active reference"}),
            d.storage_disclosure.clone(),
            "Reference import, current mapping, engine completion and numerical sensitivity do not establish physical validation, certification or whole-building coverage. Subset energy must not be extrapolated to the whole building.".into()]);
        s.paragraphs.push(match &d.sensitivity {Some(e)=>format!("Exact selected A/B numerical sensitivity: {}. Session {}; declaration {}. Frozen limits and every retained criterion are included in the evidence appendix. No scientific qualification is promoted.",if e.result.assessment.numerical_sensitivity_passed{"passed"}else{"FAILED / incomplete"},e.session_id,e.result.assessment.declaration_sha256),None=>"Exact selected A/B numerical sensitivity: not retained. A result for a different model or run pair does not apply to this report.".into()});
        if let Some(a) = &d.scope_assessment {
            s.paragraphs.push(match a.coverage_percent {Some(v)=>format!("Declared source floor-area coverage: {v:.3}%; denominator and method are retained in the scope inventory."),None=>"Source floor-area coverage: unknown; no valid denominator is declared.".into()});
            for issue in &a.stale_reasons {
                s.paragraphs
                    .push(format!("Stale {}: {}", issue.field, issue.message));
            }
        }
        for figure in &d.figures {
            if let Some(reason) = &figure.unavailable_reason {
                s.paragraphs.push(format!(
                    "Unavailable named view {}: {reason}",
                    figure.projection
                ));
            }
        }
        Ok(s)
    }
    pub(crate) fn detail_sections(&self) -> Result<Vec<Section>> {
        let d = &self.data;
        let mut sections = vec![self.summary()?];
        for figure in &d.figures {
            sections.push(Section::figure(
                &format!("Architectural reference — {}", figure.projection),
                &figure.path,
                figure.caption.clone(),
            ));
        }
        sections.push(Section::figure("Selected retained analytical abstraction — baseline", "baseline-model.svg",
            "Authoritative retained baseline thermal zones, windows and shading. This analytical abstraction is distinct from the architectural reference; no whole-building equivalence is inferred.".into()));
        sections.push(Section::figure("Selected retained analytical abstraction — proposed", "proposed-model.svg",
            "Authoritative retained proposed thermal geometry. Model and run identities below bind this figure to the selected evidence.".into()));
        sections.push(Section::table("Architectural provenance and selected evidence", &["Item", "Value"], vec![
            vec!["Reported / active reference".into(),format!("{} / {}",d.reported_asset_id,d.active_asset_id)],
            vec!["Source / revision / geometry SHA-256".into(),format!("{}\n{}\n{}",d.reference.source_sha256,d.reference.revision_sha256,d.reference.geometry_sha256)],
            vec!["Importer / coordinate convention".into(),format!("{}\n{}",d.reference.importer_id,d.reference.coordinate_convention)],
            vec!["Scale / rotation / translation".into(),format!("{} metres/source unit; {}° clockwise; {:?} m",d.reference.options.metres_per_source_unit,d.reference.options.clockwise_rotation_deg,d.reference.options.translation_m)],
            vec!["True north".into(),d.reference.options.declared_north_clockwise_deg.map(|v|format!("Declared {v}° clockwise; not inferred from model names or geometry")).unwrap_or_else(||"Unknown; model axes do not establish site north".into())],
            vec!["Retained cases".into(),d.selected_cases.iter().map(|c|format!("Design {} / run {}\nModel {}\nRecord {}",c.design_id,c.run_id,c.model_input_sha256,c.record_sha256)).collect::<Vec<_>>().join("\n")],
            vec!["Derived architecture report SHA-256".into(),d.report_sha256.clone()],
        ]));
        if let Some(scope) = &d.declared_scope {
            sections.push(Section::prose(
                "Included scope",
                scope.included_scope.clone(),
            ));
            sections.push(Section::prose("Excluded scope", if scope.excluded_scope.is_empty(){vec!["No exclusions declared; this is a declaration, not independently verified coverage.".into()]}else{scope.excluded_scope.clone()}));
            sections.push(Section::prose(
                "Boundary-condition consequences",
                vec![scope.boundary_consequences.clone()],
            ));
            let mut mappings = Section::table(
                "Source-to-analysis mappings",
                &[
                    "Source occurrence",
                    "Disposition",
                    "Analytical targets",
                    "Rationale",
                ],
                scope
                    .mappings
                    .iter()
                    .take(MAX_PRESENTED_ROWS)
                    .map(|m| {
                        Ok(vec![
                            m.occurrence_id.clone(),
                            label(&m.status)?,
                            m.targets
                                .iter()
                                .map(|t| {
                                    format!("{} / {} ({:?})", t.design_id, t.object_id, t.kind)
                                })
                                .collect::<Vec<_>>()
                                .join("\n"),
                            m.rationale.clone(),
                        ])
                    })
                    .collect::<Result<Vec<_>>>()?,
            );
            mappings.paragraphs.push(format!("Showing up to {MAX_PRESENTED_ROWS} of {} declared mappings; complete lossless data: architecture-mappings.csv and architecture-report.json. Omitted source occurrences are unresolved, not silently excluded.",scope.mappings.len()));
            sections.push(mappings);
            let mut assumptions = Section::table(
                "Assumptions, sources and readiness",
                &[
                    "Object / field",
                    "Status / value / units",
                    "Source",
                    "Rationale / affected scope",
                ],
                scope
                    .assumptions
                    .iter()
                    .take(MAX_PRESENTED_ROWS)
                    .map(|a| {
                        Ok(vec![
                            format!("{}\n{}", serde_json::to_string(&a.link)?, a.field),
                            format!(
                                "{}\n{} {}",
                                label(&a.status)?,
                                a.value.as_deref().unwrap_or("Absent"),
                                a.units
                            ),
                            a.source.clone(),
                            format!("{}\n{}", a.rationale, a.affected_scope),
                        ])
                    })
                    .collect::<Result<Vec<_>>>()?,
            );
            assumptions.paragraphs.push(format!("Showing up to {MAX_PRESENTED_ROWS} of {} assumptions; complete lossless register: architecture-assumptions.csv and architecture-report.json. Values here are explicit source declarations; authoritative thermal inputs remain in inputs.csv and energy-report.json.",scope.assumptions.len()));
            sections.push(assumptions);
            let rows = d
                .scope_assessment
                .as_ref()
                .map(|a| {
                    a.reconciliations
                        .iter()
                        .take(MAX_PRESENTED_ROWS)
                        .map(|r| {
                            vec![
                                format!("{} / {}", r.design_id, r.id),
                                scope_unit_label(r.unit).into(),
                                r.source_value
                                    .map(|v| v.to_string())
                                    .unwrap_or_else(|| "Unknown".into()),
                                r.analytical_value.to_string(),
                                r.analytical_minus_source
                                    .map(|v| v.to_string())
                                    .unwrap_or_else(|| "Undefined".into()),
                                r.relative_difference_percent
                                    .map(|v| format!("{v}%"))
                                    .unwrap_or_else(|| "Undefined".into()),
                            ]
                        })
                        .collect()
                })
                .unwrap_or_default();
            let mut reconciliation = Section::table(
                "Source / analytical reconciliation",
                &[
                    "Design / measure",
                    "Unit",
                    "Source",
                    "Analytical",
                    "Difference",
                    "Relative difference",
                ],
                rows,
            );
            reconciliation.paragraphs.push("Differences are analytical minus declared source. Zero/unknown denominators have no percentage; orientation uses signed shortest angular difference. Stale scope suppresses reconciled values. Complete declared measurement methods and sources remain in architecture-report.json; all derived rows remain in architecture-reconciliation.csv.".into());
            sections.push(reconciliation);
        }
        sections.push(Section::prose(
            "Importer warnings and exclusions",
            d.reference
                .warnings
                .iter()
                .chain(&d.reference.exclusions)
                .cloned()
                .collect(),
        ));
        if let Some(e) = &d.sensitivity {
            let a = &e.result.assessment;
            sections.push(Section::table("Exact-bound numerical sensitivity", &["Item","Retained evidence"],vec![
                vec!["Session / plan SHA-256".into(),format!("{}\n{}",e.session_id,e.result.plan_sha256)],
                vec!["Declaration SHA-256".into(),a.declaration_sha256.clone()],
                vec!["Numerical outcome".into(),format!("Passed: {}; all declared runs integrity checked: {}; qualification promoted: {}",a.numerical_sensitivity_passed,a.all_declared_runs_integrity_checked,a.qualification_promoted)],
                vec!["Missing slots / interruptions".into(),format!("{:?}\n{}",a.missing_slots,serde_json::to_string(&a.interruptions)?)],
                vec!["Exact run/record identities".into(),a.runs.iter().map(|r|format!("{} / {}\n{}",r.slot_id,r.run_id,r.record_sha256)).collect::<Vec<_>>().join("\n")],
                vec!["Finite protocol / evidence scope".into(),format!("{}\n{}",e.plan.reuse_policy,e.result.evidence_scope)],
            ]));
            let mut criteria = Section::table(
                "All declared sensitivity criteria",
                &[
                    "Case / declared slots / quantity / zone",
                    "10-minute",
                    "5-minute reference",
                    "Measured % / limit %",
                    "Outcome",
                ],
                a.criteria.iter().map(sensitivity_criterion_row).collect(),
            );
            criteria.paragraphs.push("Ten-minute versus five-minute demand is compared on common ten-minute intervals. Frozen annual electricity ≤1%, aligned facility peak ≤3%, and original comfort criteria remain unchanged. Comfort predicates each assess one declared run: A10/B10 mean ten-minute and A5/B5 mean five-minute; their pairwise value columns are not applicable, and their actual unmet percentage is in Measured %. Slot/run identities are retained above. Every declared outcome is retained; numerical sensitivity is not independent physical validation.".into());
            sections.push(criteria);
            sections.push(Section::prose(
                "Numerical sensitivity limitations",
                a.limitations.clone(),
            ));
        }
        Ok(sections)
    }
}

fn label(value: &impl Serialize) -> Result<String> {
    Ok(serde_json::to_value(value)?
        .as_str()
        .context("Expected enum label")?
        .replace('_', " "))
}
fn csv_row(values: impl IntoIterator<Item = String>) -> String {
    values
        .into_iter()
        .map(|v| format!("\"{}\"", v.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(",")
        + "\r\n"
}
fn mappings_csv(data: &ArchitecturalReportData) -> String {
    let mut out = csv_row(
        [
            "occurrence_id",
            "status",
            "design_id",
            "object_id",
            "kind",
            "model_input_sha256",
            "rationale",
        ]
        .map(str::to_owned),
    );
    if let Some(scope) = &data.declared_scope {
        for m in &scope.mappings {
            let targets: Vec<Option<&AnalyticalTarget>> = if m.targets.is_empty() {
                vec![None]
            } else {
                m.targets.iter().map(Some).collect()
            };
            for t in targets {
                out += &csv_row([
                    m.occurrence_id.clone(),
                    format!("{:?}", m.status),
                    t.map(|x| x.design_id.clone()).unwrap_or_default(),
                    t.map(|x| x.object_id.clone()).unwrap_or_default(),
                    t.map(|x| format!("{:?}", x.kind)).unwrap_or_default(),
                    t.map(|x| x.model_input_sha256.clone()).unwrap_or_default(),
                    m.rationale.clone(),
                ]);
            }
        }
    }
    out
}
fn assumptions_csv(data: &ArchitecturalReportData) -> String {
    let mut out = csv_row(
        [
            "id",
            "link",
            "field",
            "status",
            "value",
            "units",
            "source",
            "rationale",
            "affected_scope",
        ]
        .map(str::to_owned),
    );
    if let Some(scope) = &data.declared_scope {
        for a in &scope.assumptions {
            out += &csv_row([
                a.id.clone(),
                serde_json::to_string(&a.link).expect("Serializable link"),
                a.field.clone(),
                format!("{:?}", a.status),
                a.value.clone().unwrap_or_default(),
                a.units.clone(),
                a.source.clone(),
                a.rationale.clone(),
                a.affected_scope.clone(),
            ]);
        }
    }
    out
}
fn reconciliation_csv(data: &ArchitecturalReportData) -> String {
    let mut out = csv_row(
        [
            "id",
            "design_id",
            "unit",
            "source_value",
            "analytical_value",
            "analytical_minus_source",
            "relative_difference_percent",
        ]
        .map(str::to_owned),
    );
    if let Some(a) = &data.scope_assessment {
        for r in &a.reconciliations {
            out += &csv_row([
                r.id.clone(),
                r.design_id.clone(),
                format!("{:?}", r.unit),
                r.source_value.map(|v| v.to_string()).unwrap_or_default(),
                r.analytical_value.to_string(),
                r.analytical_minus_source
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                r.relative_difference_percent
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
            ]);
        }
    }
    out
}

fn scope_unit_label(unit: ScopeUnit) -> &'static str {
    match unit {
        ScopeUnit::Metres => "m",
        ScopeUnit::SquareMetres => "m²",
        ScopeUnit::CubicMetres => "m³",
        ScopeUnit::DegreesClockwise => "° clockwise",
    }
}

fn sensitivity_criterion_row(
    c: &greenlens_core::energy::EnergySensitivityCriterion,
) -> Vec<String> {
    let single_run_comfort =
        c.quantity == "occupied_cooling_unmet_percent" && c.slot_ids.len() == 1;
    let value = |v: Option<f64>| {
        v.map(|v| v.to_string()).unwrap_or_else(|| {
            if single_run_comfort {
                "N/A — single-run comfort predicate".into()
            } else {
                "Missing".into()
            }
        })
    };
    vec![
        format!(
            "{} / {} / {} / {}",
            c.alternative_id,
            c.slot_ids.join(", "),
            c.quantity,
            c.zone_id.as_deref().unwrap_or("facility")
        ),
        value(c.ten_minute),
        value(c.five_minute_reference),
        format!(
            "{} / {}",
            c.measured_percent
                .map(|v| v.to_string())
                .unwrap_or_else(|| "Undefined".into()),
            c.limit_percent
        ),
        format!(
            "{}: {}",
            if c.passed { "Pass" } else { "FAIL" },
            c.explanation
        ),
    ]
}

/// v0.6 presentation only. Original datasets and native message inventories are
/// retained verbatim; duplicated disclosure text is not a second native warning.
pub(crate) fn clarify_sections(sections: &mut [Section]) {
    for section in sections {
        if section.title == "Evidence index" {
            section.title = "Optional attachment evidence index".into();
        }
        let mut counts = BTreeMap::<String, usize>::new();
        for p in &section.paragraphs {
            *counts.entry(p.clone()).or_default() += 1;
        }
        let mut emitted = std::collections::BTreeSet::new();
        section.paragraphs.retain(|p| emitted.insert(p.clone()));
        for p in &mut section.paragraphs {
            if counts[p] > 1 {
                *p = format!("{} [Identical disclosure text occurs {} times in the retained source lists; shown once here. Native warning counts and original messages are unchanged.]", p, counts[p]);
            }
        }
    }
}

fn projection_svg(
    reference: &ReferenceModel,
    scope: Option<&ArchitecturalScope>,
    triangles: &[ReferenceTriangle],
    axes: [usize; 2],
    title: &str,
) -> String {
    let [x, y] = axes;
    let min = reference.bounds.min_m;
    let max = reference.bounds.max_m;
    let spanx = (max[x] - min[x]).max(1e-9);
    let spany = (max[y] - min[y]).max(1e-9);
    let scale = (820. / spanx).min(400. / spany);
    let offsetx = 70. + (820. - spanx * scale) / 2.;
    let offsety = 75. + (400. - spany * scale) / 2.;
    let statuses = scope
        .map(|s| {
            s.mappings
                .iter()
                .map(|m| (m.occurrence_id.as_str(), m.status))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let mut svg=format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"960\" height=\"560\" viewBox=\"0 0 960 560\"><rect width=\"960\" height=\"560\" fill=\"white\"/><text x=\"32\" y=\"32\" font-family=\"sans-serif\" font-size=\"20\" fill=\"#213834\">Architectural reference — {}</text><text x=\"32\" y=\"55\" font-family=\"sans-serif\" font-size=\"12\">{} / {} axes; metres; reference geometry only</text>",escaped(title),["X","Y","Z"][x],["X","Y","Z"][y]);
    for t in triangles {
        let occurrence = &reference.occurrences[t.occurrence_index as usize];
        let color = match statuses.get(occurrence.id.as_str()) {
            Some(MappingStatus::Mapped) => "#246954",
            Some(MappingStatus::ContextOnly) => "#7a8588",
            Some(MappingStatus::Excluded) => "#b65d54",
            _ => "#a58027",
        };
        let points = t
            .vertices_m
            .iter()
            .map(|p| {
                format!(
                    "{:.3},{:.3}",
                    offsetx + (p[x] - min[x]) * scale,
                    offsety + spany * scale - (p[y] - min[y]) * scale
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        svg.push_str(&format!(
            "<polygon points=\"{points}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"0.55\"/>"
        ));
    }
    let bar = 10f64.powf((spanx / 5.).log10().floor());
    let bar_px = bar * scale;
    svg.push_str(&format!("<path d=\"M70 493h{bar_px:.3}\" stroke=\"#213834\" stroke-width=\"3\"/><text x=\"70\" y=\"510\" font-family=\"sans-serif\" font-size=\"12\">{bar} m scale bar; reference model axes</text><text x=\"32\" y=\"539\" font-family=\"sans-serif\" font-size=\"12\">Green: mapped · grey: context · red: excluded · amber: unresolved. {}</text></svg>",escaped(&reference.options.declared_north_clockwise_deg.map(|n|format!("Declared north {n}° clockwise")).unwrap_or_else(||"True north unknown".into()))));
    svg
}

/// The v0.4/v0.5 template bytes and identifiers stay unchanged. Only an explicit
/// v0.6 architectural render permits these additional internally generated SVGs.
pub(crate) fn template(base: &str) -> String {
    let named = (1..=MAX_REFERENCE_REPORT_VIEWS)
        .map(|i| format!("\"reference-view-{i}.svg\", "))
        .collect::<String>();
    base.replace("\"baseline-model.svg\",", &format!("\"reference-plan.svg\", \"reference-front.svg\", \"reference-right.svg\", {named}\"baseline-model.svg\","))
        .replace(
            "footer: context [#text(size: 7pt)[#text(d.report_id) · #text(d.revision)] #h(1fr) #counter(page).display(\"1 / 1\", both: true)])",
            "footer: context grid(columns: (1fr, auto), align: (left, right), text(size: 7pt, d.report_id + \" · \" + d.revision), counter(page).display(\"1 / 1\", both: true)))",
        )
}

pub fn validate_retained_package(
    root: &Path,
    project: &Project,
    base: &EnergyReportData,
    workbench: Option<&str>,
    expected_hash: &str,
) -> Result<()> {
    let retained: ArchitecturalReportData =
        read_json(&regular_child(root, "reports/architecture-report.json")?)?;
    ensure!(
        retained.report_sha256 == expected_hash && retained.identity()? == expected_hash,
        "Architectural report checksum mismatch"
    );
    let regenerated = prepare_project(root, project, base, workbench, &AtomicBool::new(false))?
        .context("Architectural package has no retained reference assets")?;
    ensure!(canonical_hash(&retained)?==canonical_hash(&regenerated.data)?,"Architectural report/scope differs from selected retained evidence or admitted reference geometry");
    for figure in &retained.figures {
        ensure!(
            greenlens_process::file_hash(&regular_child(
                root,
                &format!("reports/{}", figure.path)
            )?)? == figure.svg_sha256,
            "Retained architectural figure differs from its canonical definition"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use greenlens_core::{
        energy::synthetic_energy_report,
        reference::{admit_glb, AdmittedReference, ReferenceImportOptions},
    };
    use serde_json::json;
    use std::fs;

    #[test]
    fn sensitivity_rows_identify_single_run_comfort_without_claiming_missing_pair_evidence() {
        use greenlens_core::energy::EnergySensitivityCriterion;
        let mut criterion = EnergySensitivityCriterion {
            id: "A10:comfort:zone1".into(),
            alternative_id: "A".into(),
            slot_ids: vec!["A10".into()],
            quantity: "occupied_cooling_unmet_percent".into(),
            zone_id: Some("zone1".into()),
            ten_minute: None,
            five_minute_reference: None,
            measured_percent: Some(0.),
            limit_percent: 1.,
            passed: true,
            explanation: "Occupied interval predicate for this declared run".into(),
        };
        let retained = serde_json::to_vec(&criterion).unwrap();
        let ten = sensitivity_criterion_row(&criterion);
        assert!(ten[0].contains("A10"));
        assert!(ten[1].starts_with("N/A"));
        assert!(ten[2].starts_with("N/A"));
        assert_eq!(ten[3], "0 / 1");
        assert_eq!(serde_json::to_vec(&criterion).unwrap(), retained);
        criterion.slot_ids = vec!["A5".into()];
        assert!(sensitivity_criterion_row(&criterion)[0].contains("A5"));
        criterion.quantity = "annual_facility_kwh".into();
        criterion.slot_ids = vec!["A10".into(), "A5".into()];
        criterion.passed = false;
        criterion.measured_percent = None;
        let missing = sensitivity_criterion_row(&criterion);
        assert_eq!(missing[1], "Missing");
        assert_eq!(missing[2], "Missing");
        assert!(missing[4].starts_with("FAIL"));
    }

    #[test]
    fn architectural_disclosures_consolidate_exact_repeats_without_changing_native_counts() {
        let mut section = Section::table(
            "Evidence index",
            &["Native warnings"],
            vec![vec!["1".into()]],
        );
        section.paragraphs = vec![
            "Native location warning".into(),
            "Different warning".into(),
            "Native location warning".into(),
        ];
        let original = section.paragraphs.clone();
        clarify_sections(std::slice::from_mut(&mut section));
        assert_eq!(section.title, "Optional attachment evidence index");
        assert_eq!(section.rows, vec![vec!["1"]]);
        assert_eq!(section.paragraphs.len(), 2);
        assert!(section.paragraphs[0].contains("occurs 2 times"));
        assert_eq!(section.paragraphs[1], original[1]);
        assert_eq!(scope_unit_label(ScopeUnit::SquareMetres), "m²");
        assert_eq!(scope_unit_label(ScopeUnit::CubicMetres), "m³");
        for base in [
            super::super::TEMPLATE,
            include_str!("../../../reports/energy/workbench.typ"),
        ] {
            assert!(template(base).contains("footer: context grid("));
            assert!(!base.contains("footer: context grid("));
        }
    }

    fn reference() -> AdmittedReference {
        let mut bin = Vec::new();
        for value in [[0f32, 0., 0.], [4., 0., 0.], [0., 3., 0.]]
            .iter()
            .flatten()
        {
            bin.extend(value.to_le_bytes());
        }
        let doc = json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteLength":36}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[4,3,0]}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"name":"Original <test> geometry","mesh":0}],"scenes":[{"nodes":[0]}],"scene":0});
        let mut json = serde_json::to_vec(&doc).unwrap();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let mut bytes = Vec::new();
        bytes.extend(0x4654_6c67u32.to_le_bytes());
        bytes.extend(2u32.to_le_bytes());
        bytes.extend(((28 + json.len() + bin.len()) as u32).to_le_bytes());
        bytes.extend((json.len() as u32).to_le_bytes());
        bytes.extend(0x4e4f_534au32.to_le_bytes());
        bytes.extend(json);
        bytes.extend((bin.len() as u32).to_le_bytes());
        bytes.extend(0x004e_4942u32.to_le_bytes());
        bytes.extend(bin);
        admit_glb(&bytes, &ReferenceImportOptions::default(), |_| true).unwrap()
    }
    fn fixture() -> (EnergyReportData, ArchitecturalProject, AdmittedReference) {
        let base = synthetic_energy_report().unwrap();
        let admitted = reference();
        let m = &admitted.manifest;
        let id = uuid::Uuid::new_v4().to_string();
        let scope = ArchitecturalScope {
            schema_version: ARCHITECTURAL_SCOPE_SCHEMA.into(),
            title: "Synthetic reference <script>title</script>".into(),
            purpose: "Contract fixture; no equivalence to a measured building".into(),
            scope_kind: ScopeKind::RepresentativeStudy,
            reference_revision_sha256: m.revision_sha256.clone(),
            reference_geometry_sha256: m.geometry_sha256.clone(),
            reference_options_sha256: canonical_hash(&m.options).unwrap(),
            design_model_sha256: BTreeMap::from([
                (
                    base.baseline.id.clone(),
                    base.baseline.model_input_sha256.clone(),
                ),
                (
                    base.proposed.id.clone(),
                    base.proposed.model_input_sha256.clone(),
                ),
            ]),
            mappings: vec![ScopeMapping {
                occurrence_id: m.occurrences[0].id.clone(),
                status: MappingStatus::Mapped,
                targets: vec![AnalyticalTarget {
                    design_id: base.baseline.id.clone(),
                    object_id: base.baseline.model.zones[0].id.clone(),
                    kind: AnalyticalObjectKind::Zone,
                    model_input_sha256: base.baseline.model_input_sha256.clone(),
                }],
                rationale: "Declared test mapping only".into(),
            }],
            included_scope: vec!["Representative declared zone".into()],
            excluded_scope: vec!["Whole-building coverage not established".into()],
            boundary_consequences: "Existing boundary conditions stay authoritative".into(),
            assumptions: vec![ScopeAssumption {
                id: "unprovided".into(),
                link: AssumptionLink::Study,
                field: "source_location".into(),
                status: AssumptionStatus::Missing,
                value: None,
                units: "not supplied".into(),
                source: "No source supplied".into(),
                rationale: "Never infer location from geometry".into(),
                affected_scope: "Illustrative study only".into(),
            }],
            reconciliations: vec![],
            coverage: ScopeCoverage::Unknown {
                reason: "No semantic source area measured".into(),
            },
            whole_building_review: None,
            report_views: vec![],
        };
        let extension = ArchitecturalProject {
            schema_version: "greenlens.architectural-project.v1".into(),
            assets: vec![ReferenceAsset {
                id: id.clone(),
                path: format!("references/{id}"),
                revision_sha256: m.revision_sha256.clone(),
                manifest_sha256: "a".repeat(64),
                geometry_sha256: m.geometry_sha256.clone(),
            }],
            active_asset_id: id,
            scope: Some(scope),
            scope_history: None,
        };
        (base, extension, admitted)
    }
    fn prepared(
        base: &EnergyReportData,
        ext: &ArchitecturalProject,
        admitted: &AdmittedReference,
        workbench: Option<&str>,
    ) -> ArchitecturalReport {
        let triangles =
            greenlens_core::reference::decode_chunk(&admitted.manifest, 0, &admitted.chunks[0])
                .unwrap();
        prepare(
            base,
            workbench,
            ext,
            admitted.manifest.clone(),
            &triangles,
            1,
        )
        .unwrap()
    }
    fn named_view(scope: &ArchitecturalScope, design_id: &str) -> ReferenceReportView {
        ReferenceReportView {
            schema_version: REFERENCE_REPORT_VIEW_SCHEMA.into(),
            id: "view-1".into(),
            title: "Original <section> view".into(),
            design_id: design_id.into(),
            model_input_sha256: scope.design_model_sha256[design_id].clone(),
            reference_revision_sha256: scope.reference_revision_sha256.clone(),
            reference_geometry_sha256: scope.reference_geometry_sha256.clone(),
            eye_m: [2., -20., 1.5],
            target_m: [2., 0., 1.5],
            up: [0., 0., 1.],
            vertical_span_m: 6.,
            aspect_ratio: 4. / 3.,
            section: None,
        }
    }
    fn attach_named_view(ext: &mut ArchitecturalProject, design_id: &str) -> ReferenceReportView {
        let scope = ext.scope.as_mut().unwrap();
        let view = named_view(scope, design_id);
        scope.report_views.push(view.clone());
        view
    }
    #[test]
    fn actual_named_view_figures_bind_report_identity_without_mutating_energy_data() {
        let (base, mut ext, admitted) = fixture();
        let immutable_base = serde_json::to_vec(&base).unwrap();
        let view = attach_named_view(&mut ext, &base.baseline.id);
        let a = prepared(&base, &ext, &admitted, None);
        let b = prepared(&base, &ext, &admitted, None);
        assert_eq!(a.data.report_sha256, b.data.report_sha256);
        assert_eq!(a.figures, b.figures);
        assert_eq!(a.data.figures.len(), 4);
        let definition = &a.data.figures[3];
        assert_eq!(definition.retained_view.as_ref(), Some(&view));
        assert_eq!(definition.source_triangle_count, Some(1));
        assert_eq!(definition.sampled_triangle_count, Some(1));
        assert_eq!(definition.depicted_triangle_count, 1);
        assert!(definition.unavailable_reason.is_none());
        let svg = String::from_utf8_lossy(&a.figures[3].1);
        assert!(svg.contains("M326.667 450.000L633.333 450.000"));
        assert!(svg.contains("M633.333 450.000L326.667 220.000"));
        let products = a.products().unwrap();
        let (_, bytes) = products
            .iter()
            .find(|(name, _)| name == "architecture-report.json")
            .unwrap();
        let retained: ArchitecturalReportData = serde_json::from_slice(bytes).unwrap();
        assert_eq!(retained.figures[3].retained_view, Some(view));
        assert_eq!(retained.identity().unwrap(), a.data.report_sha256);

        ext.scope.as_mut().unwrap().report_views[0].title = "A different issue title".into();
        let renamed = prepared(&base, &ext, &admitted, None);
        assert_ne!(renamed.data.report_sha256, a.data.report_sha256);
        assert_ne!(
            renamed.data.figures[3].svg_sha256,
            a.data.figures[3].svg_sha256
        );
        ext.scope.as_mut().unwrap().report_views[0].vertical_span_m = 8.;
        let reframed = prepared(&base, &ext, &admitted, None);
        assert_ne!(reframed.data.report_sha256, renamed.data.report_sha256);
        assert_ne!(
            reframed.data.figures[3].svg_sha256,
            renamed.data.figures[3].svg_sha256
        );
        assert_eq!(serde_json::to_vec(&base).unwrap(), immutable_base);
        a.validate_binding(&base, None).unwrap();
    }
    #[test]
    fn named_view_counts_only_triangles_with_visible_clipped_edges_once() {
        let (base, mut ext, admitted) = fixture();
        attach_named_view(&mut ext, &base.baseline.id);
        let immutable_base = serde_json::to_vec(&base).unwrap();
        // The admitted triangle spans X=0..4 and Z=0..3. Z>=2 leaves two
        // original edges, Z>=3 only touches its apex, and Z>=4 removes it.
        for (section_offset, expected_triangles, expected_edges) in [
            (None, 1, 3),
            (Some(2.), 1, 2),
            (Some(3.), 0, 0),
            (Some(4.), 0, 0),
        ] {
            ext.scope.as_mut().unwrap().report_views[0].section =
                section_offset.map(|offset_m| ReferenceReportSection {
                    axis: ReferenceSectionAxis::Z,
                    offset_m,
                    keep: ReferenceSectionKeep::Above,
                });
            let report = prepared(&base, &ext, &admitted, None);
            let definition = &report.data.figures[3];
            assert_eq!(definition.source_triangle_count, Some(1));
            assert_eq!(definition.sampled_triangle_count, Some(1));
            assert_eq!(definition.depicted_triangle_count, expected_triangles);
            assert!(definition.unavailable_reason.is_none());
            assert!(definition
                .caption
                .contains("1 of 1 admitted triangles sampled"));
            assert!(definition.caption.contains(&format!(
                "{expected_triangles} sampled triangles have at least one visible original edge"
            )));
            let svg = String::from_utf8_lossy(&report.figures[3].1);
            assert_eq!(svg.matches("stroke-width=\"0.65\"").count(), expected_edges);
            assert!(!svg.contains("<polygon"));
            report.validate_binding(&base, None).unwrap();
        }
        let view = &mut ext.scope.as_mut().unwrap().report_views[0];
        view.section = None;
        view.eye_m[0] = 20.;
        view.target_m[0] = 20.;
        let report = prepared(&base, &ext, &admitted, None);
        assert_eq!(report.data.figures[3].sampled_triangle_count, Some(1));
        assert_eq!(report.data.figures[3].depicted_triangle_count, 0);
        assert!(!String::from_utf8_lossy(&report.figures[3].1).contains("stroke-width=\"0.65\""));
        assert_eq!(serde_json::to_vec(&base).unwrap(), immutable_base);
    }
    #[test]
    fn missing_historical_source_never_draws_active_geometry_in_old_frame() {
        for geometry_only in [false, true] {
            let (base, mut ext, admitted) = fixture();
            let scope = ext.scope.as_mut().unwrap();
            if !geometry_only {
                scope.reference_revision_sha256 = "d".repeat(64);
            }
            scope.reference_geometry_sha256 = "e".repeat(64);
            let view = attach_named_view(&mut ext, &base.baseline.id);
            let original_scope = serde_json::to_vec(&ext.scope).unwrap();
            let report = prepared(&base, &ext, &admitted, None);
            assert_eq!(
                report.data.scope_assessment.as_ref().unwrap().relationship,
                ScopeRelationship::Stale
            );
            let definition = &report.data.figures[3];
            assert_eq!(definition.retained_view.as_ref(), Some(&view));
            assert_eq!(definition.source_triangle_count, None);
            assert_eq!(definition.sampled_triangle_count, None);
            assert_eq!(definition.depicted_triangle_count, 0);
            assert!(definition
                .unavailable_reason
                .as_ref()
                .unwrap()
                .contains("exact reference"));
            let svg = String::from_utf8_lossy(&report.figures[3].1);
            assert!(svg.contains("Retained report view unavailable"));
            assert!(!svg.contains("<path"));
            assert!(!svg.contains("<polygon"));
            assert!(report
                .summary()
                .unwrap()
                .paragraphs
                .iter()
                .any(|p| p.contains("Unavailable named view")));
            for (definition, (_, bytes)) in
                report.data.figures[..3].iter().zip(&report.figures[..3])
            {
                assert!(definition.caption.contains("context only"));
                assert!(!String::from_utf8_lossy(bytes).contains("stroke=\"#246954\""));
            }
            assert_eq!(serde_json::to_vec(&ext.scope).unwrap(), original_scope);
            let triangles =
                greenlens_core::reference::decode_chunk(&admitted.manifest, 0, &admitted.chunks[0])
                    .unwrap();
            let (defensive, count) = views::svg(
                &admitted.manifest,
                ext.scope.as_ref().unwrap(),
                &triangles,
                &view,
            );
            assert_eq!(count, 0);
            assert!(defensive.contains("Retained report view unavailable"));
            assert!(!defensive.contains("<path"));
            if !geometry_only {
                let destination = crate::local_root()
                    .unwrap()
                    .join("verification")
                    .join(format!(
                        "architectural-unavailable-{}",
                        uuid::Uuid::new_v4()
                    ));
                super::super::render_with_architecture(
                    &base,
                    &destination,
                    None,
                    &AtomicBool::new(false),
                    Some(&report),
                )
                .unwrap();
                let assessment = fs::read_to_string(destination.join("assessment.html")).unwrap();
                let appendix = fs::read_to_string(destination.join("input-appendix.html")).unwrap();
                assert!(assessment.contains("Unavailable named view"));
                assert!(appendix.contains("Retained report view unavailable"));
                assert!(appendix.contains("No reference triangles are depicted"));
                let retained: ArchitecturalReportData =
                    read_json(&destination.join("architecture-report.json")).unwrap();
                assert_eq!(retained.figures[3].retained_view.as_ref(), Some(&view));
                assert_eq!(retained.figures[3].source_triangle_count, None);
            }
        }
    }
    #[test]
    fn selected_case_mismatch_retains_definition_and_hides_named_geometry() {
        let (base, mut ext, admitted) = fixture();
        let scope = ext.scope.as_mut().unwrap();
        scope
            .design_model_sha256
            .insert(base.baseline.id.clone(), "b".repeat(64));
        scope.mappings[0].targets[0].model_input_sha256 = "b".repeat(64);
        let view = attach_named_view(&mut ext, &base.baseline.id);
        let report = prepared(&base, &ext, &admitted, None);
        let definition = &report.data.figures[3];
        assert_eq!(definition.retained_view.as_ref(), Some(&view));
        assert_eq!(definition.source_triangle_count, Some(1));
        assert_eq!(definition.sampled_triangle_count, Some(0));
        assert_eq!(definition.depicted_triangle_count, 0);
        assert!(definition
            .unavailable_reason
            .as_ref()
            .unwrap()
            .contains("selected retained energy case"));
        assert!(!String::from_utf8_lossy(&report.figures[3].1).contains("<path"));
        assert_eq!(
            report.data.selected_cases[0].model_input_sha256,
            base.baseline.model_input_sha256
        );
    }
    #[test]
    fn exact_retained_historical_asset_is_used_even_when_another_revision_is_active() {
        let (base, mut ext, admitted) = fixture();
        attach_named_view(&mut ext, &base.baseline.id);
        let historical_id = ext.assets[0].id.clone();
        let active_id = uuid::Uuid::new_v4().to_string();
        ext.assets.push(ReferenceAsset {
            id: active_id.clone(),
            path: format!("references/{active_id}"),
            revision_sha256: "d".repeat(64),
            geometry_sha256: "e".repeat(64),
            manifest_sha256: "f".repeat(64),
        });
        ext.active_asset_id = active_id;
        assert_eq!(selected_asset(&ext).unwrap().id, historical_id);
        let report = prepared(&base, &ext, &admitted, None);
        assert_eq!(report.data.reported_asset_id, historical_id);
        assert!(!report.data.reported_reference_is_active);
        assert!(report.data.figures[3].unavailable_reason.is_none());
        assert!(String::from_utf8_lossy(&report.figures[3].1)
            .contains("M326.667 450.000L633.333 450.000"));
        assert!(report
            .summary()
            .unwrap()
            .paragraphs
            .iter()
            .any(|p| p.contains("retained historical revision")));
    }
    #[test]
    fn historical_asset_selection_requires_both_source_revision_and_geometry() {
        let (_, mut ext, _) = fixture();
        let exact_id = ext.assets[0].id.clone();
        let mut same_source_other_geometry = ext.assets[0].clone();
        same_source_other_geometry.id = uuid::Uuid::new_v4().to_string();
        same_source_other_geometry.path = format!("references/{}", same_source_other_geometry.id);
        same_source_other_geometry.geometry_sha256 = "b".repeat(64);
        ext.active_asset_id = same_source_other_geometry.id.clone();
        ext.assets.insert(0, same_source_other_geometry);
        assert_eq!(selected_asset(&ext).unwrap().id, exact_id);
    }
    #[test]
    fn standalone_products_show_missing_scope_inputs_and_preserve_immutable_dataset() {
        let (base, ext, admitted) = fixture();
        let before = serde_json::to_vec(&base).unwrap();
        let report = prepared(&base, &ext, &admitted, None);
        let destination = crate::local_root()
            .unwrap()
            .join("verification")
            .join(format!("architectural-report-{}", uuid::Uuid::new_v4()));
        let rendered = super::super::render_with_architecture(
            &base,
            &destination,
            None,
            &AtomicBool::new(false),
            Some(&report),
        )
        .unwrap();
        assert_eq!(
            rendered.schema_version,
            "greenlens.energy-report-artifacts.v0.6"
        );
        assert_eq!(rendered.renderer_status, "not_requested");
        let html = fs::read_to_string(destination.join("assessment.html")).unwrap();
        for text in [
            "Source floor-area coverage: unknown",
            "missing assumptions",
            "original architectural-source archive",
            "&lt;script&gt;",
        ] {
            assert!(html.contains(text), "Missing {text}");
        }
        assert!(!html.contains("<script>"));
        let appendix = fs::read_to_string(destination.join("input-appendix.html")).unwrap();
        for text in [
            "Source-to-analysis mappings",
            "Assumptions, sources and readiness",
            "Source / analytical reconciliation",
            "<polygon",
            "True north unknown",
        ] {
            assert!(appendix.contains(text), "Missing {text}");
        }
        assert_eq!(serde_json::to_vec(&base).unwrap(), before);
        let retained: EnergyReportData =
            read_json(&destination.join("energy-report.json")).unwrap();
        assert_eq!(retained, base);
        assert!(
            fs::read_to_string(destination.join("architecture-assumptions.csv"))
                .unwrap()
                .contains("No source supplied")
        );
        assert!(destination.join("architecture-mappings.csv").is_file());
    }
    #[test]
    fn stale_scope_is_visible_and_never_rebound_to_selected_historical_case() {
        let (base, mut ext, admitted) = fixture();
        let scope = ext.scope.as_mut().unwrap();
        scope
            .design_model_sha256
            .insert(base.baseline.id.clone(), "b".repeat(64));
        scope.mappings[0].targets[0].model_input_sha256 = "b".repeat(64);
        let before = serde_json::to_vec(&ext.scope).unwrap();
        let report = prepared(&base, &ext, &admitted, None);
        assert_eq!(
            report.data.scope_assessment.as_ref().unwrap().relationship,
            ScopeRelationship::Stale
        );
        assert!(report
            .summary()
            .unwrap()
            .paragraphs
            .iter()
            .any(|p| p.contains("scientific inputs changed")));
        assert_eq!(
            report.data.selected_cases[0].model_input_sha256,
            base.baseline.model_input_sha256
        );
        assert_eq!(serde_json::to_vec(&ext.scope).unwrap(), before);
        assert!(report
            .data
            .scope_assessment
            .as_ref()
            .unwrap()
            .reconciliations
            .is_empty());
    }
    #[test]
    fn figures_are_deterministic_owned_data_and_bind_declared_geometry_identity() {
        let (base, ext, admitted) = fixture();
        let a = prepared(&base, &ext, &admitted, None);
        let b = prepared(&base, &ext, &admitted, None);
        assert_eq!(a.data.report_sha256, b.data.report_sha256);
        assert_eq!(a.figures, b.figures);
        assert_eq!(a.data.figures.len(), 3);
        assert!(a
            .data
            .figures
            .iter()
            .all(|f| f.depicted_triangle_count == 1 && f.deterministic_stride == 1));
        assert!(a
            .figures
            .iter()
            .all(|(_, bytes)| !String::from_utf8_lossy(bytes).contains("NaN")));
        let mut changed = a.clone();
        changed.data.reference.options.translation_m[0] = 1.;
        assert!(changed.validate_binding(&base, None).is_err());
        let mut changed_base = base.clone();
        changed_base.title = "Different report".into();
        assert!(a.validate_binding(&changed_base, None).is_err());
        assert!(template(super::super::TEMPLATE).contains("reference-plan.svg"));
        assert!(!super::super::TEMPLATE.contains("reference-plan.svg"));
    }
    #[test]
    fn workbench_profiles_keep_old_dataset_identity_and_new_supplement_binding() {
        let (mut base, ext, admitted) = fixture();
        base.evidence.clear();
        let preflight = greenlens_core::energy::EnergyAttachmentPreflight {
            schema_version: "greenlens.energy-attachment-preflight.v1".into(),
            ready: true,
            items: vec![],
        };
        let workbench = crate::energy_report_workbench::prepare_data(
            base,
            Default::default(),
            vec![
                greenlens_core::energy::EnergyReportProfile::Executive,
                greenlens_core::energy::EnergyReportProfile::Technical,
            ],
            &preflight,
        )
        .unwrap();
        let report = prepared(
            &workbench.base,
            &ext,
            &admitted,
            Some(&workbench.derived_sha256),
        );
        let destination = crate::local_root()
            .unwrap()
            .join("verification")
            .join(format!("architectural-workbench-{}", uuid::Uuid::new_v4()));
        let output = crate::energy_report_workbench::render_with_architecture(
            &workbench,
            &destination,
            None,
            &AtomicBool::new(false),
            Some(&report),
        )
        .unwrap();
        assert_eq!(output.dataset_sha256, report.data.report_sha256);
        let retained: greenlens_core::energy::EnergyReportWorkbenchData =
            read_json(&destination.join("workbench-report.json")).unwrap();
        assert_eq!(retained, workbench);
        for file in [
            "executive.html",
            "technical.html",
            "performance.html",
            "input-appendix.html",
        ] {
            assert!(fs::read_to_string(destination.join(file))
                .unwrap()
                .contains("Architectural scope and coverage"));
        }
    }
    #[test]
    fn cold_retained_assets_reconstruct_reports_and_tampering_blocks_package_inspection() {
        let (base, mut ext, admitted) = fixture();
        let root = crate::local_root()
            .unwrap()
            .join("verification")
            .join(format!("architectural-cold-{}", uuid::Uuid::new_v4()));
        let directory = root.join(&ext.assets[0].path);
        fs::create_dir_all(directory.join("geometry")).unwrap();
        write_json(&directory.join("reference.json"), &admitted.manifest).unwrap();
        for (entry, bytes) in admitted.manifest.chunks.iter().zip(&admitted.chunks) {
            fs::write(directory.join(&entry.path), bytes).unwrap();
        }
        let names: Vec<&str> = std::iter::once("reference.json")
            .chain(admitted.manifest.chunks.iter().map(|c| c.path.as_str()))
            .collect();
        seal(&directory, &names).unwrap();
        ext.assets[0].manifest_sha256 =
            greenlens_process::file_hash(&directory.join("reference.json")).unwrap();
        let mut project = crate::example();
        project.architecture = Some(ext);
        let report = prepare_project(&root, &project, &base, None, &AtomicBool::new(false))
            .unwrap()
            .unwrap();
        fs::create_dir_all(root.join("reports")).unwrap();
        for (name, bytes) in report.products().unwrap() {
            fs::write(root.join("reports").join(name), bytes).unwrap();
        }
        validate_retained_package(&root, &project, &base, None, &report.data.report_sha256)
            .unwrap();
        project
            .architecture
            .as_mut()
            .unwrap()
            .scope
            .as_mut()
            .unwrap()
            .purpose = "Externally changed declaration".into();
        assert!(validate_retained_package(
            &root,
            &project,
            &base,
            None,
            &report.data.report_sha256
        )
        .is_err());
        project.architecture.as_mut().unwrap().scope = report.data.declared_scope.clone();
        fs::write(
            directory.join(&admitted.manifest.chunks[0].path),
            b"altered",
        )
        .unwrap();
        assert!(prepare_project(&root, &project, &base, None, &AtomicBool::new(false)).is_err());
    }
}
