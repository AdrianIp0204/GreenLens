//! Review-resolution presentation. This version wraps the frozen v2 register
//! assembly without changing its historical products or renderer dispatch.
use super::*;
use greenlens_core::{
    report_review::{review_readiness, ReportReviewReadiness, ReportReviewRow, ReviewState},
    study_revision::{InputProvenanceRow, ReviewResolution, RevisionRowKind},
};
use serde_json::json;

pub(super) const METHOD: &str = "greenlens.energy-report-issue-renderer/3";
pub(super) const DOCUMENT_SCHEMA: &str = "greenlens.energy-document.v0.8-review";
pub(super) const HTML_METHOD: &str = "greenlens.owned-energy-html/3";
pub(super) const TEMPLATE: &str = include_str!("../../../reports/energy/issue-v3.typ");
const ROW_LIMIT: usize = 80;
const COVERAGE_SCHEMA: &str = "greenlens.issue-presentation-coverage.v2";

#[derive(Debug, Serialize)]
struct ReviewCoverage {
    total_rows: usize,
    displayed_rows: usize,
    omitted_rows: usize,
    reviewed_total: usize,
    unresolved_total: usize,
    unresolved_displayed: usize,
    unresolved_omitted: usize,
    category_metadata_unavailable: usize,
    displayed_ids: Vec<String>,
    unresolved_ids: Vec<String>,
    complete_artifacts: [&'static str; 3],
}

fn selected(readiness: &ReportReviewReadiness) -> Vec<&ReportReviewRow> {
    let mut rows = readiness.rows.iter().enumerate().collect::<Vec<_>>();
    rows.sort_by_key(|(index, row)| (row.resolution != ReviewResolution::Unresolved, *index));
    rows.into_iter()
        .take(ROW_LIMIT)
        .map(|(_, row)| row)
        .collect()
}
fn coverage(readiness: &ReportReviewReadiness) -> ReviewCoverage {
    let shown = selected(readiness);
    let unresolved_displayed = shown
        .iter()
        .filter(|row| row.resolution == ReviewResolution::Unresolved)
        .count();
    ReviewCoverage {
        total_rows: readiness.decision_count,
        displayed_rows: shown.len(),
        omitted_rows: readiness.decision_count - shown.len(),
        reviewed_total: readiness.reviewed_count,
        unresolved_total: readiness.unresolved_count,
        unresolved_displayed,
        unresolved_omitted: readiness.unresolved_count - unresolved_displayed,
        category_metadata_unavailable: readiness.category_metadata_unavailable_count,
        displayed_ids: shown.iter().map(|row| row.row_id.clone()).collect(),
        unresolved_ids: readiness.unresolved_row_ids.clone(),
        complete_artifacts: [
            "review-readiness.json",
            "review-decisions.json",
            "review-decisions.csv",
        ],
    }
}
fn enum_label<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_value(value)?
        .as_str()
        .context("Expected named review status")?
        .replace('_', " "))
}
fn csv_cell(value: &str) -> String {
    let guarded = if value.trim_start().starts_with(['=', '+', '-', '@']) {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    format!("\"{}\"", guarded.replace('"', "\"\""))
}
fn decision_csv(readiness: &ReportReviewReadiness) -> Result<Vec<u8>> {
    let mut rows = vec![vec![
        "row_id".into(),
        "kind".into(),
        "key".into(),
        "resolution".into(),
        "rationale".into(),
    ]];
    for row in &readiness.rows {
        rows.push(vec![
            row.row_id.clone(),
            row.kind
                .as_ref()
                .map(enum_label)
                .transpose()?
                .unwrap_or_else(|| "unavailable in historical revision".into()),
            row.key.clone().unwrap_or_default(),
            enum_label(&row.resolution)?,
            row.rationale.clone(),
        ]);
    }
    Ok((rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|cell| csv_cell(cell))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n")
        .into_bytes())
}
pub(super) fn products(content: &IssueContent) -> Result<Vec<(String, Vec<u8>)>> {
    let readiness = review_readiness(&content.scope_revision)?;
    let mut products = v2::products(content)?;
    let (_, bytes) = products
        .iter_mut()
        .find(|(name, _)| name == "presentation-coverage.json")
        .context("Complete register coverage missing")?;
    let mut complete: serde_json::Value = serde_json::from_slice(bytes)?;
    complete["schema_version"] = json!(COVERAGE_SCHEMA);
    complete["review_decisions"] = serde_json::to_value(coverage(&readiness))?;
    complete["review_readiness_sha256"] = json!(readiness.assessment_sha256);
    complete["input_documentation_complete"] = json!(readiness.documentation_complete);
    complete["review_state"] = serde_json::to_value(readiness.review_state)?;
    *bytes = serde_json::to_vec_pretty(&complete)?;
    products.push((
        "review-readiness.json".into(),
        serde_json::to_vec_pretty(&readiness)?,
    ));
    products.push(("review-decisions.json".into(), serde_json::to_vec_pretty(&json!({
        "schema_version":"greenlens.issue-review-decisions.v1",
        "content_sha256":content.content_sha256,
        "scope_revision_sha256":content.scope_revision.revision_sha256,
        "readiness_sha256":readiness.assessment_sha256,
        "rows":readiness.rows,
        "disclosure":"All recorded decisions and full rationales. Null category/key means unavailable in the historical revision; no category is inferred. CSV guards spreadsheet formula prefixes; JSON retains literal strings."
    }))?));
    products.push(("review-decisions.csv".into(), decision_csv(&readiness)?));
    Ok(products)
}
fn review_summary(readiness: &ReportReviewReadiness) -> String {
    let state = match readiness.review_state {
        ReviewState::UnresolvedRetained => "UNRESOLVED decisions retained",
        ReviewState::AllRecordedReviewed => "all recorded decisions reviewed",
        ReviewState::InitialNoCarryForward => "initial scope; no carry-forward review recorded",
        ReviewState::NoRecordedDecisions => "no carry-forward decisions recorded",
    };
    format!("Carry-forward review: {state}. All {} decisions: {} reviewed; {} unresolved, including any Input or Reference decisions. Input documentation is {} independently. Recorded review does not establish independent approval or scientific qualification. Full decisions/rationales: review-decisions.json and review-decisions.csv.", readiness.decision_count, readiness.reviewed_count, readiness.unresolved_count, if readiness.documentation_complete { "complete according to its checklist" } else { "INCOMPLETE" })
}
fn review_object_label(
    row: &ReportReviewRow,
    frozen_inputs: &[InputProvenanceRow],
) -> Result<String> {
    if row.kind == Some(RevisionRowKind::Input) {
        // Input review keys are provenance-row identities, not display names.
        // Resolve only against this issue's selected immutable checklist. A
        // deleted target is never substituted by a same-named current field.
        if let Some(input) = row
            .key
            .as_ref()
            .and_then(|key| frozen_inputs.iter().find(|input| &input.id == key))
        {
            let zone = input
                .object
                .zone_id
                .as_ref()
                .map(|id| format!("\nZone: {id}"))
                .unwrap_or_default();
            let field = if input.field_path == "/gross_rated_cop" {
                format!("Gross rated COP ({})", input.field_path)
            } else {
                input.field_path.clone()
            };
            return Ok(format!(
                "input\nDesign {} · {} {}{}\nField: {}\nUnits: {}\nProvenance row: {}",
                input.design_id,
                enum_label(&input.object.kind)?,
                input.object.id,
                zone,
                field,
                input.units.as_deref().unwrap_or("not declared"),
                input.id
            ));
        }
        return Ok(format!(
            "input\nObject/field unavailable in this selected scope snapshot (deleted or not retained). No current-model substitute is used.\nRetained provenance key: {}",
            row.key.as_deref().unwrap_or("unavailable")
        ));
    }
    Ok(format!(
        "{}\n{}",
        row.kind
            .as_ref()
            .map(enum_label)
            .transpose()?
            .unwrap_or_else(|| "Category unavailable in historical revision".into()),
        row.key
            .as_deref()
            .unwrap_or("Object/field unavailable; opaque ID is not interpreted")
    ))
}
fn review_table(
    readiness: &ReportReviewReadiness,
    frozen_inputs: &[InputProvenanceRow],
) -> Result<Section> {
    let c = coverage(readiness);
    let mut section = Section::table(
        "Carry-forward review decisions and unresolved items",
        &[
            "Row category / object or field",
            "Resolution / row identity",
            "Recorded rationale",
        ],
        selected(readiness)
            .iter()
            .map(|row| {
                Ok(vec![
                    review_object_label(row, frozen_inputs)?,
                    format!("{}\nID: {}", enum_label(&row.resolution)?, row.row_id),
                    row.rationale.clone(),
                ])
            })
            .collect::<Result<_>>()?,
    );
    section.paragraphs = vec![
        review_summary(readiness),
        format!("Displayed {} of {} decision rows; {} omitted from this table. All-row unresolved: {}; {} displayed and {} omitted. Unresolved rows are selected first, followed by retained order. Complete rationales and all omitted IDs remain in review-decisions.json, review-decisions.csv and presentation-coverage.json.",c.displayed_rows,c.total_rows,c.omitted_rows,c.unresolved_total,c.unresolved_displayed,c.unresolved_omitted),
        format!("{} decisions lack retained category/object metadata in the historical revision. Resolution and rationale remain available; missing context does not imply resolution.",c.category_metadata_unavailable),
    ];
    Ok(section)
}
pub(super) fn augment(
    issue: &IssueReport,
    data: &EnergyReportWorkbenchData,
    names: &[&str],
    docs: &mut [Document],
    generated: &mut Vec<(String, Vec<u8>)>,
    cancel: &AtomicBool,
) -> Result<()> {
    v2::augment(issue, data, names, docs, generated, cancel)?;
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Review resolution preparation cancelled"
    );
    let readiness = review_readiness(&issue.content.scope_revision)?;
    // Replace only supplements generated in this new version. Frozen v2 output
    // generation remains separately callable and never receives this step.
    for (name, bytes) in products(&issue.content)? {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "Review resolution products cancelled"
        );
        if let Some((_, old)) = generated.iter_mut().find(|(existing, _)| existing == &name) {
            *old = bytes;
        } else {
            generated.push((name, bytes));
        }
    }
    for (name, document) in names.iter().zip(docs) {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "Review resolution document cancelled"
        );
        document.schema_version = DOCUMENT_SCHEMA;
        if let Some(summary) = document
            .sections
            .iter_mut()
            .find(|s| s.title == "Issued study scope, evidence and readiness")
        {
            summary.paragraphs.insert(0, review_summary(&readiness));
        } else {
            document.sections.insert(
                0,
                Section::prose(
                    "Carry-forward review status",
                    vec![review_summary(&readiness)],
                ),
            );
        }
        if *name == "input-appendix" {
            let insertion = document
                .sections
                .iter()
                .position(|s| s.title == "Source-to-analysis mapping snapshot")
                .unwrap_or(0);
            document.sections.insert(
                insertion,
                review_table(&readiness, &issue.content.scope_revision.provenance.rows)?,
            );
        }
    }
    Ok(())
}
pub(super) fn html(document: &Document, generated: &[(&str, Vec<u8>)]) -> Result<String> {
    Ok(v2::html(document, generated)?.replace("</style>", "section p{overflow-wrap:anywhere}table td{white-space:pre-line}@media print{thead{display:table-header-group}}</style>"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use greenlens_core::{
        architectural_scope::ARCHITECTURAL_SCOPE_SCHEMA_V3,
        report_review::REPORT_REVIEW_SCHEMA,
        study_revision::{
            accept_carry_forward_v3, initial_revision_v3, prepare_carry_forward_v3, ReviewDecision,
            RevisionRowKind,
        },
    };

    fn synthetic_readiness() -> ReportReviewReadiness {
        // Original presentation arithmetic fixture, not a validated run/revision.
        let rows = (0..101)
            .map(|i| ReportReviewRow {
                row_id: format!("row-{i:03}"),
                kind: Some(if i == 100 {
                    RevisionRowKind::Reference
                } else {
                    RevisionRowKind::Input
                }),
                key: Some(format!("Original synthetic field {i:03}")),
                resolution: if i >= 99 {
                    ReviewResolution::Unresolved
                } else {
                    ReviewResolution::Reviewed
                },
                rationale: format!(
                    "原創測試說明 {i:03} — {}",
                    "Long retained source explanation; ".repeat(30)
                ),
            })
            .collect();
        ReportReviewReadiness {
            schema_version: REPORT_REVIEW_SCHEMA.into(),
            assessment_sha256: String::new(),
            scope_revision_sha256: "test".into(),
            documentation_complete: true,
            supported_input_rows: 0,
            unreviewed_input_rows: 0,
            missing_input_rows: 0,
            conflicting_input_rows: 0,
            review_state: ReviewState::UnresolvedRetained,
            decision_count: 101,
            reviewed_count: 99,
            unresolved_count: 2,
            category_metadata_unavailable_count: 0,
            unresolved_row_ids: vec!["row-099".into(), "row-100".into()],
            rows,
            disclosure: "Synthetic review presentation only".into(),
        }
    }

    #[test]
    fn input_review_label_resolves_exact_frozen_cop_identity_and_discloses_deleted_targets() {
        use greenlens_core::{
            energy::{ThermalObjectKind, ThermalObjectRef},
            study_revision::ProvenanceStatus,
        };
        // Original presentation fixture; no accepted run or engine evidence.
        let input = InputProvenanceRow {
            id: "frozen-cop-provenance-row".into(),
            design_id: "B".into(),
            object: ThermalObjectRef::new(
                ThermalObjectKind::CoolingSystem,
                "cooling-east",
                Some("zone-east"),
            ),
            field_path: "/gross_rated_cop".into(),
            applied_value: json!(3.6),
            units: Some("1".into()),
            declared_source: None,
            status: ProvenanceStatus::Unreviewed,
            source_declarations: vec![],
        };
        let mut readiness = synthetic_readiness();
        readiness.rows[99].key = Some(input.id.clone());
        let before = serde_json::to_vec(&input).unwrap();
        let section = review_table(&readiness, std::slice::from_ref(&input)).unwrap();
        let label = &section.rows[0][0];
        for expected in [
            "Design B",
            "cooling system cooling-east",
            "Zone: zone-east",
            "Gross rated COP (/gross_rated_cop)",
            "Units: 1",
            "Provenance row: frozen-cop-provenance-row",
        ] {
            assert!(
                label.contains(expected),
                "Missing exact selected input label: {expected}"
            );
        }
        assert_eq!(serde_json::to_vec(&input).unwrap(), before);

        let mut unrelated = input.clone();
        unrelated.id = "same-field-different-retained-identity".into();
        unrelated.design_id = "A".into();
        let missing = review_object_label(&readiness.rows[99], &[unrelated]).unwrap();
        assert!(missing.contains("unavailable in this selected scope snapshot"));
        assert!(missing.contains("deleted or not retained"));
        assert!(missing.contains("frozen-cop-provenance-row"));
        assert!(!missing.contains("Design A") && !missing.contains("cooling-east"));
        readiness.rows[99].key = None;
        let no_key = review_object_label(&readiness.rows[99], &[input]).unwrap();
        assert!(no_key.contains("Retained provenance key: unavailable"));
    }

    #[test]
    fn late_input_and_reference_unresolved_rows_are_visible_despite_complete_documentation() {
        let readiness = synthetic_readiness();
        let c = coverage(&readiness);
        assert_eq!(
            (c.total_rows, c.displayed_rows, c.omitted_rows),
            (101, 80, 21)
        );
        assert_eq!(
            (
                c.unresolved_total,
                c.unresolved_displayed,
                c.unresolved_omitted
            ),
            (2, 2, 0)
        );
        assert_eq!(&c.displayed_ids[..2], ["row-099", "row-100"]);
        let section = review_table(&readiness, &[]).unwrap();
        assert!(section.paragraphs[0].contains("UNRESOLVED"));
        assert!(section.paragraphs[0].contains("documentation is complete"));
        assert!(section.rows[0][0].contains("input"));
        assert!(section.rows[1][0].contains("reference"));
        assert_eq!(section.rows[1][2], readiness.rows[100].rationale);
        let csv = String::from_utf8(decision_csv(&readiness).unwrap()).unwrap();
        assert!(csv.contains(&readiness.rows[100].rationale));
        assert_eq!(csv.lines().count(), 102);
    }

    #[test]
    fn all_unresolved_omissions_and_formula_rationales_are_explicit_and_lossless_in_json() {
        let mut readiness = synthetic_readiness();
        for row in &mut readiness.rows {
            row.resolution = ReviewResolution::Unresolved;
        }
        readiness.reviewed_count = 0;
        readiness.unresolved_count = 101;
        readiness.unresolved_row_ids = readiness
            .rows
            .iter()
            .map(|row| row.row_id.clone())
            .collect();
        readiness.rows[100].rationale = "=HYPERLINK(\"inert\")\n私人字串測試".into();
        let c = coverage(&readiness);
        assert_eq!(
            (
                c.unresolved_total,
                c.unresolved_displayed,
                c.unresolved_omitted
            ),
            (101, 80, 21)
        );
        assert_eq!(c.unresolved_ids.last().unwrap(), "row-100");
        assert!(review_table(&readiness, &[]).unwrap().paragraphs[1].contains("21 omitted"));
        assert!(String::from_utf8(decision_csv(&readiness).unwrap())
            .unwrap()
            .contains("'=HYPERLINK"));
        let json = serde_json::to_value(&readiness).unwrap();
        assert_eq!(
            json["rows"][100]["rationale"],
            readiness.rows[100].rationale
        );
    }

    #[test]
    fn renderer_three_is_distinct_and_legacy_identities_remain_exact() {
        let one = current_renderer_identity().unwrap();
        let two = renderer_v2_identity().unwrap();
        let three = latest_renderer_identity().unwrap();
        assert_eq!(one.method, ISSUE_RENDERER_METHOD);
        assert_eq!(two.method, v2::METHOD);
        assert_eq!(three.method, METHOD);
        assert_eq!(
            one.template_sha256,
            "1b077568e99d6f45c9428084260e8984071c474d7b8a51de472b8b0090442697"
        );
        assert_eq!(
            two.template_sha256,
            "e447d9f8398b3706c00312bea091b9c97256fc287e852687a175e91d99e23cb5"
        );
        assert_ne!(two.template_sha256, three.template_sha256);
        assert!(TEMPLATE.contains("review resolution"));
        let (legacy, _) = super::super::tests::fixture();
        assert!(required_supplement_products(&legacy.content)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn standalone_new_issue_html_and_complete_products_bind_frozen_review_status() {
        let (mut issue, _) = super::super::tests::fixture();
        // Keep a valid initial legacy revision: its lack of carry-forward is
        // disclosed, never called a completed independent review.
        issue.content.renderer = latest_renderer_identity().unwrap();
        issue.content.content_sha256 = issue.content.identity().unwrap();
        let before = serde_json::to_vec(&issue.content.scope_revision).unwrap();
        let root = crate::local_root()
            .unwrap()
            .join("verification")
            .join(format!("issue-v3-review-{}", uuid::Uuid::new_v4()));
        let a = render_issue(&issue, &root.join("one"), None, &AtomicBool::new(false)).unwrap();
        let b = render_issue(&issue, &root.join("two"), None, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            a.schema_version,
            "greenlens.energy-report-artifacts.v0.8-review"
        );
        assert_eq!(a.artifacts.len(), b.artifacts.len());
        for (x, y) in a.artifacts.iter().zip(&b.artifacts) {
            assert_eq!((&x.path, &x.sha256, x.bytes), (&y.path, &y.sha256, y.bytes));
        }
        for name in ["executive", "technical", "input-appendix"] {
            let html = std::fs::read_to_string(root.join(format!("one/{name}.html"))).unwrap();
            assert!(html.contains("initial scope; no carry-forward review recorded"));
            assert!(html.contains("Input documentation is INCOMPLETE independently"));
        }
        let coverage: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join("one/presentation-coverage.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(coverage["schema_version"], COVERAGE_SCHEMA);
        assert_eq!(coverage["review_decisions"]["total_rows"], 0);
        assert!(root.join("one/review-decisions.json").exists());
        assert!(root.join("one/review-decisions.csv").exists());
        assert_eq!(
            serde_json::to_vec(&issue.content.scope_revision).unwrap(),
            before
        );
    }

    #[test]
    fn validated_carried_input_decision_is_bound_and_visible_in_standalone_report() {
        let started = std::time::Instant::now();
        let stage = |name: &str| {
            eprintln!(
                "[v3 carried review] {name}: {:.3}s",
                started.elapsed().as_secs_f64()
            );
        };
        stage("fixture start");
        let (original, _) = super::super::tests::fixture();
        stage("fixture complete");
        let mut scope = original.content.scope_revision.scope.clone();
        scope.report_views.clear();
        let reference = original.content.scope_revision.reference.clone();
        let designs = original.content.scope_revision.designs.clone();
        let mut previous_designs = designs.clone();
        let previous_b = previous_designs.get_mut("B").unwrap();
        previous_b.model.zones[0]
            .cooling_system
            .as_mut()
            .unwrap()
            .gross_rated_cop += 0.2;
        let previous_b_sha = validate_thermal_model(&previous_b.model, &previous_b.calendar)
            .unwrap()
            .model_input_sha256;
        let mut previous_scope = scope.clone();
        previous_scope
            .design_model_sha256
            .insert("B".into(), previous_b_sha);
        stage("initial revision start");
        let previous =
            initial_revision_v3(previous_scope, reference.clone(), previous_designs).unwrap();
        stage("initial revision complete");
        scope.schema_version = ARCHITECTURAL_SCOPE_SCHEMA_V3.into();
        stage("plan start");
        let plan = prepare_carry_forward_v3(&previous, &reference, &designs, Some(&scope)).unwrap();
        stage("plan complete");
        let decisions = plan
            .rows
            .iter()
            .filter(|row| row.requires_review)
            .map(|row| ReviewDecision {
                row_id: row.id.clone(),
                resolution: if row.kind == RevisionRowKind::Input {
                    ReviewResolution::Unresolved
                } else {
                    ReviewResolution::Reviewed
                },
                rationale: if row.kind == RevisionRowKind::Input {
                    "Retain unresolved source verification — 來源尚待核實".into()
                } else {
                    "Explicit synthetic context review".into()
                },
            })
            .collect::<Vec<_>>();
        assert!(decisions
            .iter()
            .any(|d| d.resolution == ReviewResolution::Unresolved));
        stage("accept start");
        let revision = accept_carry_forward_v3(
            &previous,
            reference,
            designs,
            &plan.plan_sha256,
            scope,
            decisions,
        )
        .unwrap();
        stage("accept complete; prepare start");
        let issue = prepare(original.content.workbench, revision, None, vec![], false).unwrap();
        stage("prepare complete; readiness start");
        let readiness = review_readiness(&issue.content.scope_revision).unwrap();
        stage("readiness complete");
        assert!(readiness.unresolved_count > 0);
        assert!(readiness
            .rows
            .iter()
            .any(|row| row.kind == Some(RevisionRowKind::Input)
                && row.resolution == ReviewResolution::Unresolved));
        assert_eq!(readiness.category_metadata_unavailable_count, 0);
        let root = crate::local_root()
            .unwrap()
            .join("verification")
            .join(format!("issue-v3-carried-review-{}", uuid::Uuid::new_v4()));
        stage("render start");
        render_issue(&issue, &root, None, &AtomicBool::new(false)).unwrap();
        stage("render complete; product assertions start");
        for name in ["executive", "technical", "input-appendix"] {
            let html = std::fs::read_to_string(root.join(format!("{name}.html"))).unwrap();
            assert!(html.contains("UNRESOLVED decisions retained"));
        }
        let appendix = std::fs::read_to_string(root.join("input-appendix.html")).unwrap();
        assert!(appendix.contains("來源尚待核實"));
        assert!(appendix.contains("Gross rated COP (/gross_rated_cop)"));
        assert!(appendix.contains("Design B"));
        let frozen: ReportReviewReadiness =
            serde_json::from_slice(&std::fs::read(root.join("review-readiness.json")).unwrap())
                .unwrap();
        assert_eq!(frozen, readiness);
        stage("product assertions complete; legacy fixture start");
        let (legacy, _) = super::super::tests::fixture();
        stage("legacy fixture complete");
        assert_ne!(
            legacy.content.renderer.method,
            issue.content.renderer.method
        );
        stage("complete");
    }
}
