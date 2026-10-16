//! Versioned presentation only. v1 document generation, hashes and products are
//! unchanged. All summaries use the full frozen dataset before excerpting rows.
use super::*;
use greenlens_core::{
    architectural_scope::{AssumptionLink, AssumptionStatus, MappingStatus, ScopeUnit},
    study_revision::ProvenanceStatus,
};
use serde_json::json;
use std::collections::BTreeSet;

pub(super) const METHOD: &str = "greenlens.energy-report-issue-renderer/2";
pub(super) const DOCUMENT_SCHEMA: &str = "greenlens.energy-document.v0.8";
pub(super) const HTML_METHOD: &str = "greenlens.owned-energy-html/2";
pub(super) const TEMPLATE: &str = include_str!("../../../reports/energy/issue-v2.typ");
const COVERAGE_SCHEMA: &str = "greenlens.issue-presentation-coverage.v1";
const ROW_LIMIT: usize = 80;

#[derive(Debug, Serialize)]
struct TableCoverage {
    total_rows: usize,
    displayed_rows: usize,
    omitted_rows: usize,
    status_counts: BTreeMap<String, usize>,
    displayed_ids: Vec<String>,
    review_required_total: usize,
    review_required_displayed: usize,
    review_required_omitted: usize,
    review_required_ids: Vec<String>,
    complete_artifacts: Vec<String>,
}
#[derive(Debug, Serialize)]
struct PresentationCoverage {
    schema_version: &'static str,
    content_sha256: String,
    scope_revision_sha256: String,
    selection_policy: &'static str,
    mappings: TableCoverage,
    assumptions: TableCoverage,
    provenance: TableCoverage,
    reconciliations: TableCoverage,
    unmapped_reference_occurrence_ids: Vec<String>,
    typed_change_total: usize,
    typed_change_retained_rows: usize,
    typed_change_rows_truncated: bool,
}
fn label<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_value(value)?
        .as_str()
        .context("Expected named status")?
        .replace('_', " "))
}
fn selected(priorities: &[u8]) -> Vec<usize> {
    let mut indices = (0..priorities.len()).collect::<Vec<_>>();
    indices.sort_by_key(|&i| (priorities[i], i));
    indices.truncate(ROW_LIMIT);
    indices
}
fn table_coverage(rows: Vec<(String, String, u8, bool)>, artifacts: &[&str]) -> TableCoverage {
    let indices = selected(&rows.iter().map(|r| r.2).collect::<Vec<_>>());
    let mut status_counts = BTreeMap::new();
    for (_, status, _, _) in &rows {
        *status_counts.entry(status.clone()).or_default() += 1;
    }
    let review_required_ids = rows
        .iter()
        .filter(|r| r.3)
        .map(|r| r.0.clone())
        .collect::<Vec<_>>();
    let review_required_displayed = indices.iter().filter(|&&i| rows[i].3).count();
    TableCoverage {
        total_rows: rows.len(),
        displayed_rows: indices.len(),
        omitted_rows: rows.len() - indices.len(),
        status_counts,
        displayed_ids: indices.iter().map(|&i| rows[i].0.clone()).collect(),
        review_required_total: review_required_ids.len(),
        review_required_displayed,
        review_required_omitted: review_required_ids.len() - review_required_displayed,
        review_required_ids,
        complete_artifacts: artifacts.iter().map(|s| (*s).into()).collect(),
    }
}
fn coverage(content: &IssueContent) -> Result<PresentationCoverage> {
    let scope = &content.scope_revision.scope;
    let mapped = scope
        .mappings
        .iter()
        .map(|m| m.occurrence_id.as_str())
        .collect::<BTreeSet<_>>();
    let mapping_rows = scope
        .mappings
        .iter()
        .map(|m| {
            Ok((
                m.occurrence_id.clone(),
                label(&m.status)?,
                if m.status == MappingStatus::Unresolved {
                    0
                } else {
                    1
                },
                m.status == MappingStatus::Unresolved,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let assumptions = scope
        .assumptions
        .iter()
        .map(|a| {
            Ok((
                a.id.clone(),
                label(&a.status)?,
                if a.status == AssumptionStatus::Missing {
                    0
                } else {
                    1
                },
                a.status == AssumptionStatus::Missing,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let provenance = content
        .scope_revision
        .provenance
        .rows
        .iter()
        .map(|r| {
            let priority = match r.status {
                ProvenanceStatus::Conflicting => 0,
                ProvenanceStatus::Missing => 1,
                ProvenanceStatus::Unreviewed => 2,
                _ => 3,
            };
            Ok((r.id.clone(), label(&r.status)?, priority, priority < 3))
        })
        .collect::<Result<Vec<_>>>()?;
    let reconciliations = content
        .scope_assessment
        .reconciliations
        .iter()
        .map(|r| {
            let status = if r.source_value.is_none() {
                "source missing"
            } else if r.analytical_minus_source.is_some_and(|d| d != 0.) {
                "different; no acceptance tolerance declared"
            } else {
                "equal declared values"
            };
            (
                format!("{}/{}", r.design_id, r.id),
                status.into(),
                if r.source_value.is_none() { 0 } else { 1 },
                r.source_value.is_none(),
            )
        })
        .collect();
    Ok(PresentationCoverage {
        schema_version: COVERAGE_SCHEMA, content_sha256:content.content_sha256.clone(),
        scope_revision_sha256:content.scope_revision.revision_sha256.clone(),
        selection_policy:"At most 80 human-readable rows per register. Review-critical rows first, then original retained order. Status totals and review IDs always cover all retained rows; unshown rows remain in complete machine artifacts.",
        mappings:table_coverage(mapping_rows,&["scope-mappings.json","scope-mappings.csv"]),
        assumptions:table_coverage(assumptions,&["scope-assumptions.json","scope-assumptions.csv"]),
        provenance:table_coverage(provenance,&["input-provenance.json","input-provenance.csv"]),
        reconciliations:table_coverage(reconciliations,&["scope-reconciliations.json"]),
        unmapped_reference_occurrence_ids:content.scope_revision.reference.manifest.occurrences.iter().filter(|o| !mapped.contains(o.id.as_str())).map(|o|o.id.clone()).collect(),
        typed_change_total:content.workbench.typed_comparison.total_changes,
        typed_change_retained_rows:content.workbench.typed_comparison.differences.len(),
        typed_change_rows_truncated:content.workbench.typed_comparison.truncated,
    })
}
fn csv(rows: Vec<Vec<String>>) -> Vec<u8> {
    // Spreadsheet formula guards affect the CSV presentation only; JSON retains
    // literal source text losslessly, including leading formula characters.
    let cell = |v: &str| {
        let value = if v.trim_start().starts_with(['=', '+', '-', '@']) {
            format!("'{v}")
        } else {
            v.into()
        };
        format!("\"{}\"", value.replace('"', "\"\""))
    };
    (rows
        .iter()
        .map(|r| r.iter().map(|v| cell(v)).collect::<Vec<_>>().join(","))
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n")
        .into_bytes()
}
pub(super) fn products(content: &IssueContent) -> Result<Vec<(String, Vec<u8>)>> {
    let s = &content.scope_revision.scope;
    let mapping = json!({"schema_version":"greenlens.issue-mappings.v1","scope_revision_sha256":content.scope_revision.revision_sha256,"reference_revision_sha256":s.reference_revision_sha256,"rows":s.mappings});
    let assumptions = json!({"schema_version":"greenlens.issue-assumptions.v1","scope_revision_sha256":content.scope_revision.revision_sha256,"rows":s.assumptions});
    let reconciliation = json!({"schema_version":"greenlens.issue-reconciliations.v1","scope_revision_sha256":content.scope_revision.revision_sha256,"rows":content.scope_assessment.reconciliations});
    let mut mapping_csv = vec![vec![
        "occurrence_id".into(),
        "status".into(),
        "targets_json".into(),
        "rationale".into(),
    ]];
    for m in &s.mappings {
        mapping_csv.push(vec![
            m.occurrence_id.clone(),
            label(&m.status)?,
            serde_json::to_string(&m.targets)?,
            m.rationale.clone(),
        ]);
    }
    let mut assumptions_csv = vec![vec![
        "assumption_id".into(),
        "link_json".into(),
        "field".into(),
        "status".into(),
        "source_value_json".into(),
        "units".into(),
        "source".into(),
        "rationale".into(),
        "affected_scope".into(),
    ]];
    for a in &s.assumptions {
        assumptions_csv.push(vec![
            a.id.clone(),
            serde_json::to_string(&a.link)?,
            a.field.clone(),
            label(&a.status)?,
            serde_json::to_string(&a.value)?,
            a.units.clone(),
            a.source.clone(),
            a.rationale.clone(),
            a.affected_scope.clone(),
        ]);
    }
    Ok(vec![
        (
            "scope-mappings.json".into(),
            serde_json::to_vec_pretty(&mapping)?,
        ),
        ("scope-mappings.csv".into(), csv(mapping_csv)),
        (
            "scope-assumptions.json".into(),
            serde_json::to_vec_pretty(&assumptions)?,
        ),
        ("scope-assumptions.csv".into(), csv(assumptions_csv)),
        (
            "scope-reconciliations.json".into(),
            serde_json::to_vec_pretty(&reconciliation)?,
        ),
        (
            "presentation-coverage.json".into(),
            serde_json::to_vec_pretty(&coverage(content)?)?,
        ),
    ])
}
fn counts(table: &TableCoverage) -> String {
    format!("Displayed {} of {} rows; {} omitted from this human-readable table. All-row statuses: {}. Review-required: {} total, {} displayed, {} omitted. Complete retained data: {}. See presentation-coverage.json for the exact selection and all review-required IDs.",table.displayed_rows,table.total_rows,table.omitted_rows,if table.status_counts.is_empty(){"none".into()}else{table.status_counts.iter().map(|(k,v)|format!("{k}: {v}")).collect::<Vec<_>>().join("; ")},table.review_required_total,table.review_required_displayed,table.review_required_omitted,table.complete_artifacts.join("; "))
}
fn occurrence(content: &IssueContent, id: &str) -> String {
    content
        .scope_revision
        .reference
        .manifest
        .occurrences
        .iter()
        .find(|o| o.id == id)
        .map(|o| format!("{}\nID: {id}", o.label))
        .unwrap_or_else(|| format!("Historical occurrence\nID: {id}"))
}
fn object(content: &IssueContent, design: &str, id: &str) -> String {
    let case = [
        &content.workbench.base.baseline,
        &content.workbench.base.proposed,
    ]
    .into_iter()
    .find(|c| c.id == design);
    let name = case
        .and_then(|c| c.model.zones.iter().find(|z| z.id == id))
        .map(|z| z.name.clone());
    format!(
        "{} / {}\nID: {id}",
        case.map(|c| c.label.as_str()).unwrap_or(design),
        name.unwrap_or_else(|| "Analytical object".into())
    )
}
fn unit(u: ScopeUnit) -> &'static str {
    match u {
        ScopeUnit::Metres => "m",
        ScopeUnit::SquareMetres => "m²",
        ScopeUnit::CubicMetres => "m³",
        ScopeUnit::DegreesClockwise => "° clockwise",
    }
}
fn table_sections(content: &IssueContent, c: &PresentationCoverage) -> Result<Vec<Section>> {
    let scope = &content.scope_revision.scope;
    let mapping = scope
        .mappings
        .iter()
        .map(|m| (m.occurrence_id.as_str(), m))
        .collect::<BTreeMap<_, _>>();
    let mut mapped = Section::table(
        "Source-to-analysis mapping snapshot",
        &[
            "Source occurrence / identity",
            "Disposition",
            "Analytical targets",
            "Rationale",
        ],
        c.mappings
            .displayed_ids
            .iter()
            .map(|id| {
                let m = mapping[id.as_str()];
                Ok(vec![
                    occurrence(content, id),
                    label(&m.status)?,
                    m.targets
                        .iter()
                        .map(|t| object(content, &t.design_id, &t.object_id))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    m.rationale.clone(),
                ])
            })
            .collect::<Result<_>>()?,
    );
    mapped.paragraphs.push(counts(&c.mappings));
    mapped.paragraphs.push(format!("{} reference occurrences have no declared mapping row and remain unresolved; no omission means exclusion. Counts describe occurrence records, not floor-area coverage.",c.unmapped_reference_occurrence_ids.len()));
    let assumptions = scope
        .assumptions
        .iter()
        .map(|a| (a.id.as_str(), a))
        .collect::<BTreeMap<_, _>>();
    let mut declared = Section::table(
        "Assumptions and declared source values",
        &[
            "Object / field / declaration ID",
            "Status / source value / units",
            "Source and rationale",
        ],
        c.assumptions
            .displayed_ids
            .iter()
            .map(|id| {
                let a = assumptions[id.as_str()];
                let name = match &a.link {
                    AssumptionLink::Study => "Legacy study declaration".into(),
                    AssumptionLink::StudyField { design_id } => format!("{design_id} study field"),
                    AssumptionLink::CalendarField { design_id } => {
                        format!("{design_id} calendar field")
                    }
                    // Only v3 source contracts can contain this new variant;
                    // all historical v1/v2 inputs retain the exact old output.
                    AssumptionLink::CalendarNoHolidays { design_id } => {
                        format!("{design_id} explicit no-holidays calendar policy")
                    }
                    AssumptionLink::ReferenceOccurrence { occurrence_id } => {
                        occurrence(content, occurrence_id)
                    }
                    AssumptionLink::ThermalObject {
                        design_id,
                        object_id,
                    } => object(content, design_id, object_id),
                };
                Ok(vec![
                    format!("{name}\n{}\nID: {}", a.field, a.id),
                    format!(
                        "{}\n{} {}",
                        label(&a.status)?,
                        a.value.as_deref().unwrap_or("Missing"),
                        a.units
                    ),
                    format!(
                        "{}\n{}\nAffected scope: {}",
                        a.source, a.rationale, a.affected_scope
                    ),
                ])
            })
            .collect::<Result<_>>()?,
    );
    declared.paragraphs=vec![counts(&c.assumptions),"These are source declarations. The actually applied analytical values are separately retained in the supported-input provenance inventory; a source declaration does not overwrite an input.".into()];
    let provenance = content
        .scope_revision
        .provenance
        .rows
        .iter()
        .map(|r| (r.id.as_str(), r))
        .collect::<BTreeMap<_, _>>();
    let mut applied = Section::table(
        "Supported input provenance: source and applied values",
        &[
            "Object / supported field",
            "Applied analytical value / units",
            "Review status / declared source",
        ],
        c.provenance
            .displayed_ids
            .iter()
            .map(|id| {
                let r = provenance[id.as_str()];
                let declarations = r
                    .source_declarations
                    .iter()
                    .map(|a| {
                        format!(
                            "{}: {} {}\n{}",
                            a.id,
                            a.value.as_deref().unwrap_or("Missing"),
                            a.units,
                            a.source
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(vec![
                    format!(
                        "{}\n{}",
                        object(content, &r.design_id, &r.object.id),
                        r.field_path
                    ),
                    format!(
                        "{} {}",
                        serde_json::to_string(&r.applied_value)?,
                        r.units.as_deref().unwrap_or("unit not declared")
                    ),
                    format!(
                        "{}\n{}",
                        label(&r.status)?,
                        if declarations.is_empty() {
                            "No reviewed source declaration".into()
                        } else {
                            declarations
                        }
                    ),
                ])
            })
            .collect::<Result<_>>()?,
    );
    applied.paragraphs=vec![counts(&c.provenance),"Conflicting, missing and unreviewed rows are prioritized across the complete inventory, including rows after the first 80. Empty declarations do not establish complete provenance.".into()];
    let reconciliations = content
        .scope_assessment
        .reconciliations
        .iter()
        .map(|r| (format!("{}/{}", r.design_id, r.id), r))
        .collect::<BTreeMap<_, _>>();
    let mut measured = Section::table(
        "Source / analytical reconciliation snapshot",
        &[
            "Design / measure / units",
            "Source value",
            "Applied analytical value",
            "Difference / relative",
        ],
        c.reconciliations
            .displayed_ids
            .iter()
            .map(|id| {
                let r = reconciliations[id];
                vec![
                    format!("{} / {} / {}", r.design_id, r.id, unit(r.unit)),
                    r.source_value
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "Unknown".into()),
                    r.analytical_value.to_string(),
                    format!(
                        "{} / {}",
                        r.analytical_minus_source
                            .map(|v| format!("{v} {}", unit(r.unit)))
                            .unwrap_or_else(|| "Undefined".into()),
                        r.relative_difference_percent
                            .map(|v| format!("{v}%"))
                            .unwrap_or_else(|| "Undefined denominator".into())
                    ),
                ]
            })
            .collect(),
    );
    measured.paragraphs=vec![counts(&c.reconciliations),"Differences retain full-precision values. No approximation acceptance tolerance or verified area denominator is invented.".into()];
    Ok(vec![mapped, declared, applied, measured])
}
fn summary(content: &IssueContent, c: &PresentationCoverage) -> Section {
    let d = &content.workbench;
    let p = &content.scope_revision.provenance;
    let t = &d.typed_comparison;
    let mut section = Section::prose("Issued study scope, evidence and readiness",vec![
        format!("{} — {}. Scope {:?}; declared floor-area coverage: {}. Exact source, scope and run identities are retained in scope-revision.json and issue-content.json.",content.scope_revision.scope.title,content.scope_revision.scope.purpose,content.scope_revision.scope.scope_kind,content.scope_assessment.coverage_percent.map(|v|format!("{v:.3}% of the declared denominator")).unwrap_or_else(||"unknown; no valid denominator".into())),
        format!("Selected evidence: {} ({}) and {} ({}). Later editor inputs never replace these issued snapshots.",d.base.baseline.label,d.base.baseline.run_id,d.base.proposed.label,d.base.proposed.run_id),
        format!("Documentation: {}. All supported inputs: {} unreviewed, {} missing and {} conflicting. Mappings: {} declared, {} unresolved rows, {} excluded rows; {} source occurrences have no mapping. Human tables are bounded excerpts; complete rows and all-row counts remain in the named machine artifacts.",if p.documentation_complete{"complete according to the declared checklist"}else{"INCOMPLETE"},p.unreviewed_count,p.missing_count,p.conflicting_count,c.mappings.total_rows,c.mappings.status_counts.get("unresolved").copied().unwrap_or(0),c.mappings.status_counts.get("excluded").copied().unwrap_or(0),c.unmapped_reference_occurrence_ids.len()),
        format!("Comparison eligibility: compatible geometry/weather/calendar: {}; unchanged declared service inputs: {}. {} changes; {} retained typed details; truncated: {}. Efficiency improvement alone does not establish equal service or lighting performance. Numerical findings remain conditioned on these disclosures.",t.comparable_geometry_weather_calendar,t.unchanged_service_inputs,t.total_changes,t.differences.len(),t.truncated),
        match &content.sensitivity {Some(e)=>format!("Exact selected-pair sensitivity: {}. Numerical evidence is not physical validation or scientific qualification.",if e.result.assessment.numerical_sensitivity_passed{"passed"}else{"FAILED / incomplete"}),None=>"Exact selected-pair sensitivity: MISSING. Evidence for another pair does not apply.".into()},
        "Issuance is neither independent review, professional approval, whole-building equivalence nor certification. Execution, integrity, relationship to inputs and scientific assessment remain separate. Unassessed models do not inherit qualification from a synthetic example.".into(),
    ]);
    if content.figures.is_empty() {
        section.paragraphs.push("Architectural depiction unavailable: no aligned view was accepted in this scope revision. A later issue requires an explicitly reviewed retained view.".into());
    }
    for figure in &content.figures {
        if let Some(reason) = &figure.unavailable_reason {
            section.paragraphs.push(format!(
                "View {} unavailable: {reason}",
                figure.request.view.title
            ));
        }
    }
    section
}
pub(super) fn augment(
    issue: &IssueReport,
    data: &EnergyReportWorkbenchData,
    names: &[&str],
    docs: &mut [Document],
    generated: &mut Vec<(String, Vec<u8>)>,
    cancel: &AtomicBool,
) -> Result<()> {
    // Run the frozen base assembly, then change only this version's presentation.
    issue.augment_v1(data, names, docs, generated, cancel)?;
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Issue complete-register preparation cancelled"
    );
    let c = coverage(&issue.content)?;
    generated.extend(products(&issue.content)?);
    for (name, doc) in names.iter().zip(docs) {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "Issue document preparation cancelled"
        );
        doc.schema_version = DOCUMENT_SCHEMA;
        if let Some(section) = doc
            .sections
            .iter_mut()
            .find(|s| s.title == "Issued study scope, evidence and readiness")
        {
            *section = summary(&issue.content, &c);
        }
        if *name == "input-appendix" {
            let remove = [
                "Source-to-analysis mapping snapshot",
                "Supported input provenance: source and applied values",
                "Source / analytical reconciliation snapshot",
            ];
            let insertion = doc
                .sections
                .iter()
                .position(|s| remove.contains(&s.title.as_str()))
                .unwrap_or(0);
            doc.sections.retain(|s| !remove.contains(&s.title.as_str()));
            doc.sections
                .splice(insertion..insertion, table_sections(&issue.content, &c)?);
        }
        // Avoid a section containing no content, while preserving meaningful
        // missing-evidence tables and their explicit no-records disclosure.
        doc.sections
            .retain(|s| !s.paragraphs.is_empty() || !s.headers.is_empty() || s.image.is_some());
    }
    Ok(())
}
pub(super) fn html(document: &Document, generated: &[(&str, Vec<u8>)]) -> Result<String> {
    // Owned versioned styling: legacy HTML remains exactly untouched.
    Ok(old::html(document,generated)?.replace("</style>",".scope{border-left:4px solid #246954}nav ol{columns:2;column-gap:32px}h2{font-size:1.35rem;line-height:1.3}section{margin:32px 0}figcaption{font-size:13px;color:#405b50}table{table-layout:fixed}th{font-weight:650}tr:nth-child(even){background:#f8faf9}@media(max-width:700px){nav ol{columns:1}body{padding:0 12px}table{font-size:12px}}@media print{h2{break-after:avoid}figure{break-inside:avoid}section{margin:18px 0}nav{font-size:11px}a{color:inherit}}</style>"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use greenlens_core::{
        architectural_scope::{ScopeAssumption, ScopeMapping},
        reference::{admit_glb, ReferenceImportOptions},
        study_revision::{initial_revision, RevisionReferenceSnapshot},
    };
    use std::fs;

    /// Original instanced triangles with explicitly known metre positions.
    /// This is a presentation/admission fixture, never accepted engine evidence.
    fn many_rows() -> IssueReport {
        let (legacy, _) = super::super::tests::fixture();
        let mut bin = vec![];
        for point in [[0f32, 0., 0.], [1., 0., 0.], [0., 1., 0.]] {
            for v in point {
                bin.extend(v.to_le_bytes());
            }
        }
        let nodes=(0..81).map(|i|json!({"name":format!("Synthetic component {i:03} — 原創"),"mesh":0,"translation":[i,0,0]})).collect::<Vec<_>>();
        let mut source=serde_json::to_vec(&json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteLength":36}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":nodes,"scenes":[{"nodes":(0..81).collect::<Vec<_>>()}],"scene":0})).unwrap();
        while !source.len().is_multiple_of(4) {
            source.push(b' ');
        }
        let mut bytes = vec![];
        for v in [
            0x46546c67u32,
            2,
            (28 + source.len() + bin.len()) as u32,
            source.len() as u32,
            0x4e4f534a,
        ] {
            bytes.extend(v.to_le_bytes());
        }
        bytes.extend(source);
        bytes.extend((bin.len() as u32).to_le_bytes());
        bytes.extend(0x004e4942u32.to_le_bytes());
        bytes.extend(bin);
        let admitted = admit_glb(&bytes, &ReferenceImportOptions::default(), |_| true).unwrap();
        let mut scope = legacy.content.scope_revision.scope.clone();
        scope.reference_revision_sha256 = admitted.manifest.revision_sha256.clone();
        scope.reference_geometry_sha256 = admitted.manifest.geometry_sha256.clone();
        scope.reference_options_sha256 = canonical_hash(&admitted.manifest.options).unwrap();
        scope.report_views.clear();
        scope.mappings = admitted
            .manifest
            .occurrences
            .iter()
            .enumerate()
            .map(|(i, o)| ScopeMapping {
                occurrence_id: o.id.clone(),
                status: if i == 80 {
                    MappingStatus::Unresolved
                } else if i == 79 {
                    MappingStatus::Excluded
                } else {
                    MappingStatus::ContextOnly
                },
                targets: vec![],
                rationale: format!("Original explicit disposition row {i}"),
            })
            .collect();
        scope.assumptions = admitted
            .manifest
            .occurrences
            .iter()
            .enumerate()
            .map(|(i, o)| ScopeAssumption {
                id: format!("source-{i:03}"),
                link: AssumptionLink::ReferenceOccurrence {
                    occurrence_id: o.id.clone(),
                },
                field: "measured dimension".into(),
                status: if i == 80 {
                    AssumptionStatus::Missing
                } else {
                    AssumptionStatus::Synthetic
                },
                value: if i == 80 { None } else { Some("1".into()) },
                units: "m".into(),
                source: "Original synthetic geometry".into(),
                rationale: "Readability fixture; no physical validation".into(),
                affected_scope: "reference example".into(),
            })
            .collect();
        let zone = &legacy.content.workbench.base.proposed.model.zones[0];
        for (id, value) in [("conflict-one", "3.6"), ("conflict-two", "3.7")] {
            scope.assumptions.push(ScopeAssumption {
                id: id.into(),
                link: AssumptionLink::ThermalObject {
                    design_id: "B".into(),
                    object_id: zone.cooling_system.as_ref().unwrap().id.clone(),
                },
                field: "/gross_rated_cop".into(),
                status: AssumptionStatus::Assumed,
                value: Some(value.into()),
                units: "1".into(),
                source: "Contradictory original source fixture".into(),
                rationale: "Must remain visible after the excerpt boundary".into(),
                affected_scope: zone.id.clone(),
            });
        }
        let revision = initial_revision(
            scope,
            RevisionReferenceSnapshot {
                asset_id: legacy.content.scope_revision.reference.asset_id.clone(),
                manifest_sha256: old::sha(&serde_json::to_vec(&admitted.manifest).unwrap()),
                manifest: admitted.manifest,
            },
            legacy.content.scope_revision.designs.clone(),
        )
        .unwrap();
        let mut issue = prepare(legacy.content.workbench, revision, None, vec![], false).unwrap();
        issue.content.renderer = renderer_v2_identity().unwrap();
        issue.content.content_sha256 = issue.content.identity().unwrap();
        issue
    }
    #[test]
    fn late_unresolved_missing_and_conflicting_rows_are_disclosed_from_complete_input() {
        let issue = many_rows();
        issue.content.validate().unwrap();
        let before = serde_json::to_vec(&issue.content).unwrap();
        let c = coverage(&issue.content).unwrap();
        assert_eq!(
            (
                c.mappings.total_rows,
                c.mappings.displayed_rows,
                c.mappings.omitted_rows
            ),
            (81, 80, 1)
        );
        assert_eq!(c.mappings.status_counts["unresolved"], 1);
        assert_eq!(c.mappings.status_counts["excluded"], 1);
        assert_eq!(
            c.mappings.displayed_ids[0],
            issue.content.scope_revision.scope.mappings[80].occurrence_id
        );
        assert_eq!(c.mappings.status_counts.values().sum::<usize>(), 81);
        assert_eq!(c.assumptions.total_rows, 83);
        assert_eq!(c.assumptions.displayed_ids[0], "source-080");
        assert_eq!(c.provenance.status_counts["conflicting"], 1);
        let conflict = issue
            .content
            .scope_revision
            .provenance
            .rows
            .iter()
            .position(|r| r.status == ProvenanceStatus::Conflicting)
            .unwrap();
        assert!(conflict > 80, "fixture must exercise late provenance");
        assert_eq!(
            c.provenance.displayed_ids[0],
            issue.content.scope_revision.provenance.rows[conflict].id
        );
        for table in [
            &c.mappings,
            &c.assumptions,
            &c.provenance,
            &c.reconciliations,
        ] {
            assert_eq!(table.total_rows, table.displayed_rows + table.omitted_rows);
            assert_eq!(
                table.total_rows,
                table.status_counts.values().sum::<usize>()
            );
            assert_eq!(
                table.review_required_total,
                table.review_required_displayed + table.review_required_omitted
            );
        }
        let generated = products(&issue.content).unwrap();
        let mappings: serde_json::Value = serde_json::from_slice(
            &generated
                .iter()
                .find(|(n, _)| n == "scope-mappings.json")
                .unwrap()
                .1,
        )
        .unwrap();
        assert_eq!(mappings["rows"].as_array().unwrap().len(), 81);
        assert_eq!(mappings["rows"][80]["status"], "unresolved");
        let sections = table_sections(&issue.content, &c).unwrap();
        let text = serde_json::to_string(&sections).unwrap();
        for expected in [
            "Displayed 80 of 81",
            "1 omitted",
            "unresolved: 1",
            "excluded: 1",
            "conflicting: 1",
            "Synthetic component 080",
            "scope-mappings.json",
            "Missing",
            "conflict-one",
            "conflict-two",
        ] {
            assert!(text.contains(expected), "Missing {expected}");
        }
        assert_eq!(serde_json::to_vec(&issue.content).unwrap(), before);
    }
    #[test]
    fn omitted_critical_rows_have_explicit_full_counts_and_ids() {
        let rows = (0..101)
            .map(|i| (format!("row-{i}"), "unresolved".into(), 0, true))
            .collect();
        let c = table_coverage(rows, &["complete.json"]);
        assert_eq!(
            (
                c.review_required_total,
                c.review_required_displayed,
                c.review_required_omitted
            ),
            (101, 80, 21)
        );
        assert_eq!(c.review_required_ids.last().unwrap(), "row-100");
        assert!(counts(&c).contains("21 omitted"));
    }
    #[test]
    fn v2_has_new_identity_products_and_repeatable_html_while_legacy_route_stays_v1() {
        let (legacy, _) = super::super::tests::fixture();
        assert_eq!(legacy.content.renderer.method, ISSUE_RENDERER_METHOD);
        assert_eq!(
            legacy.content.renderer.template_sha256,
            "1b077568e99d6f45c9428084260e8984071c474d7b8a51de472b8b0090442697"
        );
        assert!(required_supplement_products(&legacy.content)
            .unwrap()
            .is_empty());
        legacy.content.ensure_supported_regeneration().unwrap();
        let issue = many_rows();
        assert_eq!(issue.content.renderer.method, METHOD);
        assert_ne!(
            issue.content.renderer.template_sha256,
            legacy.content.renderer.template_sha256
        );
        let root = crate::local_root()
            .unwrap()
            .join("verification")
            .join(format!("issue-v2-completeness-{}", uuid::Uuid::new_v4()));
        let a = render_issue(&issue, &root.join("first"), None, &AtomicBool::new(false)).unwrap();
        let b = render_issue(&issue, &root.join("second"), None, &AtomicBool::new(false)).unwrap();
        assert_eq!(a.schema_version, "greenlens.energy-report-artifacts.v0.8");
        assert_eq!(a.artifacts.len(), b.artifacts.len());
        for (x, y) in a.artifacts.iter().zip(&b.artifacts) {
            assert_eq!((&x.path, &x.sha256, x.bytes), (&y.path, &y.sha256, y.bytes));
        }
        let appendix = fs::read_to_string(root.join("first/input-appendix.html")).unwrap();
        assert!(appendix.contains("Displayed 80 of 81"));
        assert!(appendix.contains("Synthetic component 080"));
        let executive = fs::read_to_string(root.join("first/executive.html")).unwrap();
        for required in [
            "Documentation: INCOMPLETE",
            "conflicting",
            "unresolved rows",
            "Exact selected-pair sensitivity: MISSING",
            "Comparison eligibility",
            "Unassessed",
        ] {
            assert!(executive.contains(required), "Missing {required}");
        }
        assert!(executive.contains("&lt;script&gt;"));
        assert!(!executive.contains("<script>"));
        assert!(TEMPLATE.contains("table.header(repeat: true"));
        assert!(!TEMPLATE.contains("pagebreak()"));
    }
    #[test]
    fn csv_formula_guard_is_explicit_and_json_remains_literal() {
        let text = String::from_utf8(csv(vec![vec!["=1+1".into(), "plain,quoted\"value".into()]]))
            .unwrap();
        assert!(text.contains("\"'=1+1\""));
        assert!(text.contains("plain,quoted\"\"value"));
    }
}
