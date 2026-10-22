//! Offline reports from one validated immutable Rust dataset. Imported text is
//! never a template, command, SVG or path. Rendering establishes no qualification.
#[path = "energy_report_architectural.rs"]
pub mod architectural;
use anyhow::{ensure, Context, Result};
use greenlens_core::energy::*;
use greenlens_process::{execute, file_hash, scratch_sizes, Limits, NativeRequest, ProcessOutcome};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const TEMPLATE: &str = include_str!("../../../reports/energy/report.typ");
const DEPENDENCY: &str = include_str!("../../../dependencies/typst.json");
const FILE_CAP: u64 = 64 * 1024 * 1024;
const TOTAL_CAP: u64 = 256 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(600);

#[derive(Debug, Clone)]
pub struct TypstConfig {
    /// Separately installed official, pinned Typst distribution.
    pub installation_directory: PathBuf,
    /// Parent for a new UUID work directory; retained on failure for inspection.
    pub scratch_directory: PathBuf,
}
impl TypstConfig {
    pub fn discover(installation: Option<&Path>) -> Result<Self> {
        let local = PathBuf::from(
            std::env::var_os("LOCALAPPDATA")
                .context("LOCALAPPDATA is required for separately installed Typst")?,
        )
        .join("GreenLens");
        let config = Self {
            installation_directory: installation
                .map(Path::to_path_buf)
                .unwrap_or_else(|| local.join("renderers").join("typst-0.15.1")),
            scratch_directory: local.join("report-jobs"),
        };
        renderer_files(&config).context("Pinned Typst/font installation unavailable or modified. Install it with scripts/install-typst.ps1, select its directory, or explicitly request --html-only")?;
        Ok(config)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportArtifact {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnergyReportArtifacts {
    pub schema_version: String,
    pub dataset_sha256: String,
    pub renderer_status: String,
    pub renderer_message: String,
    pub typst_executable_sha256: Option<String>,
    pub template_sha256: String,
    pub renderer_work_id: Option<String>,
    pub renderer_inputs: BTreeMap<String, BTreeMap<String, String>>,
    pub artifacts: Vec<ReportArtifact>,
    pub processes: Vec<ProcessOutcome>,
}
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Section {
    pub(crate) title: String,
    pub(crate) paragraphs: Vec<String>,
    pub(crate) headers: Vec<String>,
    pub(crate) rows: Vec<Vec<String>>,
    pub(crate) image: Option<String>,
    pub(crate) caption: String,
}
impl Section {
    pub(crate) fn table(title: &str, headers: &[&str], rows: Vec<Vec<String>>) -> Self {
        Self {
            title: title.into(),
            paragraphs: vec![],
            headers: headers.iter().map(|s| (*s).into()).collect(),
            rows,
            image: None,
            caption: String::new(),
        }
    }
    pub(crate) fn prose(title: &str, paragraphs: Vec<String>) -> Self {
        Self {
            paragraphs,
            ..Self::table(title, &[], vec![])
        }
    }
    pub(crate) fn figure(title: &str, image: &str, caption: String) -> Self {
        Self {
            image: Some(image.into()),
            caption,
            ..Self::table(title, &[], vec![])
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Document {
    pub(crate) schema_version: &'static str,
    pub(crate) title: String,
    pub(crate) report_id: String,
    pub(crate) revision: String,
    pub(crate) kind: String,
    pub(crate) scope: String,
    pub(crate) dataset_sha256: String,
    pub(crate) sections: Vec<Section>,
}
pub(crate) fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn timestamp(t: &EnergyTimestamp) -> String {
    format!(
        "{:02}/{:02} {:02}:{:02}-{:02}:{:02}\nLocal standard time; interval {}",
        t.month,
        t.day,
        t.start_minute / 60,
        t.start_minute % 60,
        t.end_minute / 60,
        t.end_minute % 60,
        t.interval_index
    )
}
pub(crate) fn escaped(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn serialized(value: &impl Serialize) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}
pub(crate) fn cell(c: &ReportCell) -> String {
    match c {
        ReportCell::Value { display, unit, .. }
        | ReportCell::PreciseValue { display, unit, .. } => format!("{display} {}", unit.symbol()),
        ReportCell::NotApplicable { reason } => format!("N/A — {reason}"),
        ReportCell::Missing { reason } => format!("Missing — {reason}"),
        ReportCell::Unsupported { reason } => format!("Unsupported — {reason}"),
    }
}
pub(crate) fn end_use(e: EnergyEndUse) -> &'static str {
    match e {
        EnergyEndUse::CoolingSystem => "Cooling system",
        EnergyEndUse::SupplyFans => "Supply fans",
        EnergyEndUse::VentilationExhaustFans => "Ventilation / exhaust fans",
        EnergyEndUse::InteriorLighting => "Interior lighting",
        EnergyEndUse::ExteriorLighting => "Exterior lighting",
        EnergyEndUse::PlugEquipment => "Plug / equipment",
    }
}
fn assessment(c: &EnergyReportCase) -> Result<Section> {
    let a = &c.assessment;
    let mut section = Section::table(
        &format!("{} — assessment", c.label),
        &["Axis", "Retained assessment"],
        vec![
            vec!["Execution".into(), serialized(&a.execution)?],
            vec![
                "Evidence integrity".into(),
                serialized(&a.evidence_integrity)?,
            ],
            vec![
                "Physical inputs".into(),
                serialized(&a.current_physical_inputs)?,
            ],
            vec![
                "Processed results".into(),
                serialized(&a.current_processed_results)?,
            ],
            vec![
                "Derived results".into(),
                serialized(&a.current_derived_results)?,
            ],
            vec![
                "Scientific qualification".into(),
                serialized(&a.scientific.status)?,
            ],
            vec!["Reason".into(), a.scientific.reason.clone()],
            vec!["Tested scope".into(), a.scientific.tested_scope.clone()],
            vec!["Registry".into(), a.scientific.registry_version.clone()],
            vec![
                "Scientific evidence IDs".into(),
                if a.scientific.evidence_ids.is_empty() {
                    "None retained".into()
                } else {
                    a.scientific.evidence_ids.join("\n")
                },
            ],
            vec!["Run ID".into(), c.run_id.clone()],
            vec!["Run record SHA-256".into(), c.record_sha256.clone()],
            vec![
                "Physical input SHA-256".into(),
                c.physical_input_sha256.clone(),
            ],
            vec![
                "Processed result SHA-256".into(),
                c.processing_sha256.clone(),
            ],
        ],
    );
    section.paragraphs = c.limitations.clone();
    section.rows.extend([
        vec![
            "Native warnings".into(),
            c.diagnostics
                .warning_count
                .map(|n| n.to_string())
                .unwrap_or_else(|| "Missing — not retained / not executed".into()),
        ],
        vec![
            "Native severe messages".into(),
            c.diagnostics
                .severe_count
                .map(|n| n.to_string())
                .unwrap_or_else(|| "Missing — not retained / not executed".into()),
        ],
        vec![
            "Convergence disclosure".into(),
            c.diagnostics.convergence_disclosure.clone(),
        ],
        vec![
            "Diagnostic source record SHA-256".into(),
            c.diagnostics.source_record_sha256.clone(),
        ],
    ]);
    section.paragraphs.extend(c.diagnostics.messages.clone());
    Ok(section)
}
fn flatten(path: &str, value: &Value, rows: &mut Vec<Vec<String>>) {
    flatten_values(path, value, &mut |p, v| {
        rows.push(vec![p.into(), value_text(v)])
    });
}
fn flatten_values(path: &str, value: &Value, row: &mut impl FnMut(&str, &Value)) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                flatten_values(&format!("{path}.{k}"), v, row);
            }
        }
        // Consecutive equal schedule values retain their original typed value and range.
        Value::Array(a) if a.iter().all(|v| !v.is_array() && !v.is_object()) => {
            if a.is_empty() {
                row(path, value);
            }
            let mut start = 0;
            while start < a.len() {
                let mut end = start + 1;
                while end < a.len() && a[end] == a[start] {
                    end += 1;
                }
                row(&format!("{path}[{start}..{end})"), &a[start]);
                start = end;
            }
        }
        Value::Array(a) => {
            for (i, v) in a.iter().enumerate() {
                flatten_values(&format!("{path}[{i}]"), v, row);
            }
        }
        _ => row(path, value),
    }
}
fn value_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        _ => value.to_string(),
    }
}
pub(crate) fn documents(
    data: &EnergyReportData,
    p: &EnergyReportPresentation,
    hash: &str,
) -> Result<Vec<Document>> {
    let base = |kind: &str, sections| Document {
        schema_version: "greenlens.energy-document.v0.4",
        title: data.title.clone(),
        report_id: data.id.clone(),
        revision: data.revision.clone(),
        kind: kind.into(),
        scope: data.scope.clone(),
        dataset_sha256: hash.into(),
        sections,
    };
    let mut overview = vec![assessment(&data.baseline)?, assessment(&data.proposed)?];
    for c in [&data.baseline, &data.proposed] {
        let mut rows = vec![];
        flatten("method", &serde_json::to_value(&c.method)?, &mut rows);
        overview.push(Section::table(
            &format!("{} — method and engine identity", c.label),
            &["Identity field", "Retained value"],
            rows,
        ));
    }
    overview.push(Section::table(
        "Annual comparison",
        &["Quantity", "Baseline", "Proposed"],
        vec![
            vec![
                "Facility energy".into(),
                cell(&p.baseline.annual.facility),
                cell(&p.proposed.annual.facility),
            ],
            vec![
                "Coincident facility peak".into(),
                cell(&p.baseline.peak_kw),
                cell(&p.proposed.peak_kw),
            ],
            vec![
                "Own peak time (local standard time)".into(),
                timestamp(&p.baseline.peak_timestamp),
                timestamp(&p.proposed.peak_timestamp),
            ],
            vec![
                "Gross floor area".into(),
                cell(&p.baseline.gross_floor_area),
                cell(&p.proposed.gross_floor_area),
            ],
            vec![
                "Conditioned floor area".into(),
                cell(&p.baseline.conditioned_floor_area),
                cell(&p.proposed.conditioned_floor_area),
            ],
            vec![
                "EUI / gross area".into(),
                cell(&p.baseline.gross_eui),
                cell(&p.proposed.gross_eui),
            ],
            vec![
                "EUI / conditioned area".into(),
                cell(&p.baseline.conditioned_eui),
                cell(&p.proposed.conditioned_eui),
            ],
            vec![
                "Emissions".into(),
                cell(&p.baseline.emissions),
                cell(&p.proposed.emissions),
            ],
            vec![
                "Annual reconciliation residual".into(),
                cell(&p.baseline.annual.reconciliation_residual),
                cell(&p.proposed.annual.reconciliation_residual),
            ],
            vec![
                "Peak reconciliation residual".into(),
                cell(&p.baseline.peak_reconciliation_residual),
                cell(&p.proposed.peak_reconciliation_residual),
            ],
            vec![
                "Maximum interval reconciliation residual".into(),
                cell(&p.baseline.maximum_interval_reconciliation_residual),
                cell(&p.proposed.maximum_interval_reconciliation_residual),
            ],
        ],
    ));
    overview.push(Section::table(
        "Signed savings",
        &[
            "Quantity",
            "Baseline minus proposed",
            "Baseline denominator",
        ],
        vec![
            vec![
                "Facility energy".into(),
                cell(&p.facility_energy_savings.absolute),
                cell(&p.facility_energy_savings.percent),
            ],
            vec![
                "Own coincident peaks".into(),
                cell(&p.facility_peak_savings.absolute),
                cell(&p.facility_peak_savings.percent),
            ],
        ],
    ));
    overview.push(Section::prose(
        "Comparison scope and service",
        std::iter::once(data.comparison.lighting_service_equivalence.clone())
            .chain(data.comparison.savings_claim_limitations.clone())
            .chain(data.limitations.clone())
            .collect(),
    ));
    overview.push(Section::prose(
        "Baseline/proposed input differences",
        if data.comparison.input_relation.differences.is_empty() {
            vec!["No input differences are listed for these retained snapshots. Comparison scope and service limitations still apply.".into()]
        } else {
            data.comparison.input_relation.differences.clone()
        },
    ));
    overview.push(Section::prose("Reported findings",vec![format!("The retained facility energy difference (baseline minus proposed) is {}; percentage difference is {}. The two own-peak demand difference is {}. These quantities describe the identified snapshots and declared accounting boundaries.",cell(&p.facility_energy_savings.absolute),cell(&p.facility_energy_savings.percent),cell(&p.facility_peak_savings.absolute)),format!("Baseline scientific status: {}; proposed scientific status: {}. Execution and arithmetic consistency do not promote scientific status or establish equal lighting service.",serialized(&data.baseline.assessment.scientific.status)?,serialized(&data.proposed.assessment.scientific.status)?)]));
    let mut input = vec![Section::prose("Immutable input appendix", vec!["All parameters below come from the retained model and study snapshots, not the current editor. Units are explicit in parameter names. Array ranges use zero-based [start..end) indices; repeated values are represented losslessly. Schedule indices are ten-minute local standard intervals beginning at 00:00; no DST or operating-system timezone is used.".into()])];
    input.extend([assessment(&data.baseline)?, assessment(&data.proposed)?]);
    for (side, c) in [("baseline", &data.baseline), ("proposed", &data.proposed)] {
        input.push(Section::figure(&format!("{} — retained geometry",c.label), &format!("{side}-model.svg"), format!("Building-local plans and exterior elevations; metre geometry, shared north rotation {}°. Retained model {}. Diagrams are input illustrations, not thermal fields.",c.model.north_rotation_deg,c.model_input_sha256)));
        if let Some(table) = overhang_dimensions(c)? {
            input.push(table);
        }
        let value = json!({"model":c.model,"study":c.study,"weather":c.weather_summary,"method":c.method,"carbon_factor":c.carbon_factor});
        for (name, v) in value.as_object().context("Input object")? {
            let mut rows = vec![];
            flatten(name, v, &mut rows);
            input.push(Section::table(
                &format!("{} — {name}", c.label),
                &["Parameter / unit", "Exact retained value"],
                rows,
            ));
        }
    }
    let mut performance = overview.clone();
    performance.push(Section::figure("Annual electrical end uses","annual.svg","Electrical ledger components. Bar lengths use the same full-precision cells as the table; labels use authoritative Rust formatting. No inferred fuel or heating total.".into()));
    performance.push(Section::table(
        "Annual end uses and energy savings",
        &["End use", "Baseline", "Proposed", "Savings", "Savings %"],
        p.end_uses
            .iter()
            .map(|r| {
                vec![
                    end_use(r.end_use).into(),
                    cell(&r.baseline_kwh),
                    cell(&r.proposed_kwh),
                    cell(&r.energy_savings.absolute),
                    cell(&r.energy_savings.percent),
                ]
            })
            .collect(),
    ));
    performance.push(Section::table(
        "Contributions at each design's own facility peak",
        &[
            "End use",
            "Baseline own peak",
            "Proposed own peak",
            "Difference",
            "Difference %",
        ],
        p.end_uses
            .iter()
            .map(|r| {
                vec![
                    end_use(r.end_use).into(),
                    cell(&r.baseline_coincident_kw),
                    cell(&r.proposed_coincident_kw),
                    cell(&r.demand_savings.absolute),
                    cell(&r.demand_savings.percent),
                ]
            })
            .collect(),
    ));
    performance.push(Section::prose("Demand convention",vec!["Facility demand is the maximum aligned ten-minute average. Contributions in each column share that design's own peak timestamp. End-use maxima are never summed. Differences between the two peak contribution columns need not be simultaneous demand changes.".into()]));
    let mut focused_performance = performance.clone();
    for (side, c) in [("baseline", &data.baseline), ("proposed", &data.proposed)] {
        performance.push(Section::figure(&format!("{} — model snapshot",c.label),&format!("{side}-model.svg"),format!("Retained model {}. Original input drawing; full inputs are in the appendix and energy-report.json.",c.model_input_sha256)));
        if let Some(table) = overhang_dimensions(c)? {
            performance.push(table);
        }
        let validated = validate_thermal_model(&c.model, &c.study.calendar)?;
        let mut meter_rows = vec![];
        flatten(
            "meter_ledger",
            &serde_json::to_value(&c.meter_ledger)?,
            &mut meter_rows,
        );
        performance.push(Section::table(
            &format!("{} — electrical accounting boundary", c.label),
            &["Ledger field", "Retained selection / source"],
            meter_rows,
        ));
        let selected = json!({"site":c.model.site,"north_rotation_deg":c.model.north_rotation_deg,"weather":c.weather_summary,"envelope_u_values_w_m2k":validated.construction_u_values_w_m2k,"facade_wwr":validated.facade_wwr,"glazing":c.model.glazing,"carbon_factor":c.carbon_factor});
        let mut rows = vec![];
        flatten("input", &selected, &mut rows);
        performance.push(Section::table(
            &format!("{} — site, weather and envelope", c.label),
            &["Parameter / unit", "Retained input"],
            rows,
        ));
        for z in &c.model.zones {
            let mut rows = vec![];
            flatten(
                &z.id,
                &json!({"origin_m":z.origin_m,"size_m":z.size_m,"gains":z.gains,"cooling_system":z.cooling_system}),
                &mut rows,
            );
            performance.push(Section::table(
                &format!("{} — {} systems and gains", c.label, z.id),
                &["Parameter / unit", "Retained input"],
                rows,
            ));
        }
        performance.push(Section::prose("Schedule and boundary scope",vec!["The input appendix and inputs.csv retain every declared schedule sample, material layer, surface boundary, glazing value, equipment curve and holiday override. Constant schedule runs are represented losslessly by explicit index ranges, never omitted or averaged.".into()]));
    }
    performance.push(Section::figure("Monthly electricity","monthly.svg","Twelve complete months in the explicitly declared non-leap local-standard calendar; kWh, not average power.".into()));
    for (c, cp) in [(&data.baseline, &p.baseline), (&data.proposed, &p.proposed)] {
        let headers: Vec<String> = std::iter::once("Month".into())
            .chain(std::iter::once("Facility (kWh)".into()))
            .chain(
                cp.annual
                    .end_uses
                    .keys()
                    .map(|e| format!("{} (kWh)", end_use(*e))),
            )
            .chain(std::iter::once("Residual (kWh)".into()))
            .collect();
        let rows = cp
            .monthly
            .iter()
            .enumerate()
            .map(|(i, m)| {
                std::iter::once((i + 1).to_string())
                    .chain(std::iter::once(cell(&m.facility)))
                    .chain(m.end_uses.values().map(cell))
                    .chain(std::iter::once(cell(&m.reconciliation_residual)))
                    .collect()
            })
            .collect();
        performance.push(Section {
            headers,
            ..Section::table(&format!("{} — monthly ledger", c.label), &[], rows)
        });
        let mut comfort = cp
            .comfort
            .iter()
            .map(|v| {
                vec![
                    v.zone_id.clone().unwrap_or_default(),
                    cell(&v.occupied_hours),
                    cell(&v.cooling_unmet_hours),
                    cell(&v.unmet_percent),
                ]
            })
            .collect::<Vec<_>>();
        let u = &cp.building_comfort_union;
        comfort.push(vec![
            "Building occupied-interval union".into(),
            cell(&u.occupied_hours),
            cell(&u.cooling_unmet_hours),
            cell(&u.unmet_percent),
        ]);
        performance.push(Section::table(
            &format!("{} — occupied cooling comfort", c.label),
            &["Scope", "Occupied", "Unmet", "Unmet %"],
            comfort,
        ));
        performance.push(Section::prose("Comfort convention",vec![c.totals.comfort_definition.clone(),"The building result is the union of occupied unmet intervals, not the sum of zone hours. No heating unmet-hours claim is made.".into()]));
    }
    let evidence = Section::table(
        "Evidence index",
        &["ID", "Relative source / SHA-256", "Scope / rights"],
        data.evidence
            .iter()
            .map(|e| {
                vec![
                    e.id.clone(),
                    format!("{}\n{}", e.relative_path, e.sha256),
                    format!(
                        "{}\n{}\nIncluded in source evidence package: {}",
                        e.caption, e.rights, e.included
                    ),
                ]
            })
            .collect(),
    );
    focused_performance.push(evidence.clone());
    input.push(evidence.clone());
    performance.push(evidence);
    Ok(vec![
        base("Assessment", performance),
        base("Input appendix", input),
        base("Performance", focused_performance),
    ])
}
pub(crate) fn html(d: &Document, generated: &[(&str, Vec<u8>)]) -> Result<String> {
    let mut out=format!("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src 'self'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'\"><title>{}</title><style>body{{font:15px/1.55 system-ui,sans-serif;color:#213834;max-width:1120px;margin:40px auto;padding:0 24px}}h1,h2{{color:#246954}}table{{border-collapse:collapse;width:100%;margin:20px 0;font-size:13px}}th,td{{border:1px solid #ccd8d1;padding:8px;text-align:left;vertical-align:top;overflow-wrap:anywhere;white-space:pre-wrap}}th{{background:#eef4f0}}figure{{margin:20px 0}}svg{{width:100%;height:auto}}.scope{{padding:16px;background:#eef4f0}}@media print{{thead{{display:table-header-group}}tr{{break-inside:avoid}}}}</style><h1>{}</h1><p>{} · {}</p><p class=\"scope\">{}</p><p>Dataset SHA-256: {}</p>",escaped(&d.title),escaped(&d.title),escaped(&d.kind),escaped(&d.revision),escaped(&d.scope),escaped(&d.dataset_sha256));
    out.push_str("<nav aria-label=\"Contents\"><h2>Contents</h2><ol>");
    for (i, s) in d.sections.iter().enumerate() {
        out.push_str(&format!(
            "<li><a href=\"#section-{i}\">{}</a></li>",
            escaped(&s.title)
        ));
    }
    out.push_str("</ol></nav>");
    for (i, s) in d.sections.iter().enumerate() {
        out.push_str(&format!(
            "<section id=\"section-{i}\"><h2>{}</h2>",
            escaped(&s.title)
        ));
        for p in &s.paragraphs {
            out.push_str(&format!("<p>{}</p>", escaped(p)));
        }
        if let Some(img) = &s.image {
            let bytes = &generated
                .iter()
                .find(|(name, _)| name == img)
                .context("Missing owned inline SVG")?
                .1;
            let svg = std::str::from_utf8(bytes)?;
            out.push_str(&format!(
                "<figure role=\"img\" aria-label=\"{}\">{}<figcaption>{}</figcaption></figure>",
                escaped(&s.caption),
                svg,
                escaped(&s.caption)
            ));
        }
        if !s.headers.is_empty() && s.rows.is_empty() {
            out.push_str("<p>No records were supplied for this section.</p>");
        }
        if !s.headers.is_empty() && !s.rows.is_empty() {
            out.push_str("<table><thead><tr>");
            for h in &s.headers {
                out.push_str(&format!("<th>{}</th>", escaped(h)));
            }
            out.push_str("</tr></thead><tbody>");
            for row in &s.rows {
                out.push_str("<tr>");
                for v in row {
                    out.push_str(&format!("<td>{}</td>", escaped(v)));
                }
                out.push_str("</tr>");
            }
            out.push_str("</tbody></table>");
        }
        out.push_str("</section>");
    }
    out.push_str("<p>Execution, evidence integrity, input currency and scientific qualification are separate. Simulation metrics are not certification.</p></html>");
    Ok(out)
}
pub(crate) fn inputs_csv(data: &EnergyReportData) -> Result<String> {
    let mut rows = vec!["\"input_parameter\",\"exact_value_or_lossless_repeated_range\"".into()];
    for (side, c) in [("baseline", &data.baseline), ("proposed", &data.proposed)] {
        flatten_values(
            side,
            &json!({"model":c.model,"study":c.study,"weather":c.weather_summary,"method":c.method,"carbon_factor":c.carbon_factor}),
            &mut |path, value| rows.push(format!("{},{}", csv_field(path), csv_input_value(value))),
        );
    }
    Ok(rows.join("\r\n") + "\r\n")
}
fn csv_input_value(value: &Value) -> String {
    if value.is_number() {
        format!("\"{}\"", value)
    } else {
        csv_field(&value_text(value))
    }
}
fn csv_field(s: &str) -> String {
    let guarded = if s.trim_start().starts_with(['=', '+', '-', '@']) {
        format!("'{s}")
    } else {
        s.into()
    };
    format!("\"{}\"", guarded.replace('"', "\"\""))
}
pub(crate) fn csv(p: &EnergyReportPresentation) -> Result<String> {
    fn visit(path: &str, v: &Value, rows: &mut Vec<Vec<String>>) {
        if let Some(o) = v.as_object() {
            if let Some(state) = o.get("state").and_then(Value::as_str) {
                rows.push(vec![
                    path.into(),
                    state.into(),
                    o.get("value").map(Value::to_string).unwrap_or_default(),
                    o.get("unit").and_then(Value::as_str).unwrap_or("").into(),
                    o.get("display")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .into(),
                    o.get("reason").and_then(Value::as_str).unwrap_or("").into(),
                ]);
            } else {
                for (k, v) in o {
                    visit(&format!("{path}.{k}"), v, rows);
                }
            }
        } else if let Some(a) = v.as_array() {
            for (i, v) in a.iter().enumerate() {
                visit(&format!("{path}[{i}]"), v, rows);
            }
        }
    }
    let mut rows = vec![vec![
        "field".into(),
        "state".into(),
        "value".into(),
        "unit".into(),
        "display".into(),
        "reason".into(),
    ]];
    visit("presentation", &serde_json::to_value(p)?, &mut rows);
    // Only authored text is formula guarded. Validated numeric value/display
    // columns remain numeric, including legitimate negative savings.
    Ok(rows
        .into_iter()
        .enumerate()
        .map(|(i, r)| {
            r.iter()
                .enumerate()
                .map(|(j, s)| {
                    if i > 0 && [2, 4].contains(&j) {
                        format!("\"{}\"", s.replace('"', "\"\""))
                    } else {
                        csv_field(s)
                    }
                })
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n")
}
fn numeric(c: &ReportCell) -> f64 {
    if let ReportCell::Value { value, .. } | ReportCell::PreciseValue { value, .. } = c {
        *value
    } else {
        0.
    }
}
pub(crate) fn chart(p: &EnergyReportPresentation, monthly: bool) -> String {
    let rows: Vec<(String, &ReportCell, &ReportCell)> = if monthly {
        p.baseline
            .monthly
            .iter()
            .zip(&p.proposed.monthly)
            .enumerate()
            .map(|(i, (a, b))| ((i + 1).to_string(), &a.facility, &b.facility))
            .collect()
    } else {
        p.end_uses
            .iter()
            .map(|r| (end_use(r.end_use).into(), &r.baseline_kwh, &r.proposed_kwh))
            .collect()
    };
    let max = rows
        .iter()
        .flat_map(|(_, a, b)| [numeric(a), numeric(b)])
        .fold(0., f64::max)
        .max(1.);
    let h = 70 + rows.len() * 55;
    let mut out=format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 900 {h}\" role=\"img\"><rect width=\"900\" height=\"{h}\" fill=\"white\"/><g font-family=\"Noto Sans CJK TC,sans-serif\" font-size=\"13\"><text x=\"15\" y=\"22\">Electricity (kWh) — common zero baseline</text><text x=\"420\" y=\"22\" fill=\"#246954\">Baseline</text><text x=\"560\" y=\"22\" fill=\"#729fc7\">Proposed</text>");
    for (i, (label, a, b)) in rows.iter().enumerate() {
        let y = 45 + i * 55;
        out.push_str(&format!(
            "<text x=\"15\" y=\"{}\">{}</text>",
            y + 16,
            escaped(label)
        ));
        for (j, c) in [a, b].iter().enumerate() {
            let w = numeric(c) / max * 480.;
            out.push_str(&format!("<rect x=\"220\" y=\"{}\" width=\"{w:.3}\" height=\"17\" fill=\"{}\"/><text x=\"715\" y=\"{}\">{}</text>",y+j*21,if j==0{"#246954"}else{"#729fc7"},y+j*21+14,escaped(&cell(c))));
        }
    }
    out.push_str("</g></svg>");
    out
}

fn overhang_dimensions(c: &EnergyReportCase) -> Result<Option<Section>> {
    // Facade projections can coincide or become arbitrarily close. Keep all
    // dimension labels in a keyed, paginated table instead of over the geometry.
    let mut rows = Vec::new();
    for z in &c.model.zones {
        for w in &z.windows {
            if let Some(h) = &w.overhang {
                let surface = z
                    .surfaces
                    .iter()
                    .find(|s| s.id == w.surface_id)
                    .context("Overhang window has no retained surface")?;
                rows.push(vec![
                    format!("{}\n{}\n{:?}", z.id, w.id, surface.face),
                    format!("{}\n{}", w.offset_u_m, w.offset_v_m),
                    format!("{}\n{}", w.width_m, w.height_m),
                    format!(
                        "{}\n{}\n{}",
                        h.projection_m, h.gap_above_m, h.side_extension_m
                    ),
                ]);
            }
        }
    }
    Ok((!rows.is_empty()).then(|| {
        Section::table(
            &format!("{} — overhang dimensions", c.label),
            &[
                "Zone / window / local face",
                "Window u / v (m)",
                "Window width / height (m)",
                "Overhang depth / gap above / side extension (m)",
            ],
            rows,
        )
    }))
}

pub(crate) fn model_svg(c: &EnergyReportCase) -> String {
    let model = &c.model;
    let boxes = model
        .zones
        .iter()
        .map(|z| (z.origin_m, z.size_m))
        .chain(
            model
                .detached_shading
                .iter()
                .map(|s| (s.origin_m, s.size_m)),
        )
        .collect::<Vec<_>>();
    let minx = boxes
        .iter()
        .map(|(o, _)| o[0])
        .fold(f64::INFINITY, f64::min);
    let miny = boxes
        .iter()
        .map(|(o, _)| o[1])
        .fold(f64::INFINITY, f64::min);
    let minz = boxes
        .iter()
        .map(|(o, _)| o[2])
        .fold(f64::INFINITY, f64::min);
    let maxx = boxes
        .iter()
        .map(|(o, s)| o[0] + s[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let maxy = boxes
        .iter()
        .map(|(o, s)| o[1] + s[1])
        .fold(f64::NEG_INFINITY, f64::max);
    let maxz = boxes
        .iter()
        .map(|(o, s)| o[2] + s[2])
        .fold(f64::NEG_INFINITY, f64::max);
    let scale = (390. / (maxx - minx)).min(190. / (maxy - miny));
    let mut svg=String::from("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1000 820\" role=\"img\"><rect width=\"1000\" height=\"820\" fill=\"white\"/><g font-family=\"Noto Sans CJK TC,sans-serif\" font-size=\"12\" fill=\"#213834\">");
    svg.push_str(&format!("<text x=\"20\" y=\"24\">{} — retained input geometry (metres)</text><text x=\"20\" y=\"45\">Building-local axes. North rotation clockwise from true north: {}°</text>",escaped(&c.label),model.north_rotation_deg));
    for story in 1..=2 {
        let ox = 30. + (story - 1) as f64 * 500.;
        let oy = 290.;
        svg.push_str(&format!(
            "<text x=\"{ox}\" y=\"72\">Storey {story} plan · local +Y upward</text>"
        ));
        let (sn, cn) = model.north_rotation_deg.to_radians().sin_cos();
        svg.push_str(&format!("<path d=\"M {:.3} 128 L {:.3} {:.3}\" stroke=\"#213834\"/><text x=\"{:.3}\" y=\"91\">True N</text>",ox+435.,ox+435.-24.*sn,128.-24.*cn,ox+410.));
        for shade in &model.detached_shading {
            let x = ox + (shade.origin_m[0] - minx) * scale;
            let y = oy - (shade.origin_m[1] + shade.size_m[1] - miny) * scale;
            svg.push_str(&format!("<rect x=\"{x:.3}\" y=\"{y:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"#e9dac8\" stroke=\"#865d3d\" stroke-dasharray=\"4 2\"/><text x=\"{x:.3}\" y=\"{:.3}\">{}</text>",shade.size_m[0]*scale,shade.size_m[1]*scale,y+13.,escaped(&shade.id)));
        }
        let mut count = 0;
        for z in model.zones.iter().filter(|z| z.storey == story) {
            count += 1;
            let x = ox + (z.origin_m[0] - minx) * scale;
            let y = oy - (z.origin_m[1] + z.size_m[1] - miny) * scale;
            svg.push_str(&format!("<rect x=\"{x:.3}\" y=\"{y:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"#eef4f0\" stroke=\"#246954\"/><text x=\"{:.3}\" y=\"{:.3}\">{}</text><text x=\"{:.3}\" y=\"{:.3}\">{} × {} × {} m</text>",z.size_m[0]*scale,z.size_m[1]*scale,x+4.,y+17.,escaped(&z.id),x+4.,y+33.,z.size_m[0],z.size_m[1],z.size_m[2]));
            for w in &z.windows {
                if let Some(s) = z.surfaces.iter().find(|s| s.id == w.surface_id) {
                    let (a, b) = match s.face {
                        ThermalFace::West => (
                            (x, oy - (z.origin_m[1] + w.offset_u_m - miny) * scale),
                            (
                                x,
                                oy - (z.origin_m[1] + w.offset_u_m + w.width_m - miny) * scale,
                            ),
                        ),
                        ThermalFace::East => (
                            (
                                x + z.size_m[0] * scale,
                                oy - (z.origin_m[1] + w.offset_u_m - miny) * scale,
                            ),
                            (
                                x + z.size_m[0] * scale,
                                oy - (z.origin_m[1] + w.offset_u_m + w.width_m - miny) * scale,
                            ),
                        ),
                        ThermalFace::South => (
                            (x + w.offset_u_m * scale, y + z.size_m[1] * scale),
                            (
                                x + (w.offset_u_m + w.width_m) * scale,
                                y + z.size_m[1] * scale,
                            ),
                        ),
                        ThermalFace::North => (
                            (x + w.offset_u_m * scale, y),
                            (x + (w.offset_u_m + w.width_m) * scale, y),
                        ),
                        _ => continue,
                    };
                    svg.push_str(&format!("<path d=\"M {:.3} {:.3} L {:.3} {:.3}\" stroke=\"#328bb1\" stroke-width=\"4\" fill=\"none\"/>",a.0,a.1,b.0,b.1));
                }
            }
        }
        if count == 0 {
            svg.push_str(&format!(
                "<text x=\"{ox}\" y=\"105\">No zones on this storey</text>"
            ));
        }
    }
    for (i, face) in [
        ThermalFace::South,
        ThermalFace::North,
        ThermalFace::West,
        ThermalFace::East,
    ]
    .iter()
    .enumerate()
    {
        let axis = if matches!(face, ThermalFace::South | ThermalFace::North) {
            0
        } else {
            1
        };
        let lo = if axis == 0 { minx } else { miny };
        let hi = if axis == 0 { maxx } else { maxy };
        let s = (390. / (hi - lo)).min(160. / (maxz - minz));
        let ox = 30. + (i % 2) as f64 * 500.;
        let oy = 510. + (i / 2) as f64 * 240.;
        svg.push_str(&format!(
            "<text x=\"{ox}\" y=\"{}\">Local {face:?} elevation</text>",
            oy - 177.
        ));
        for shade in &model.detached_shading {
            let x = ox + (shade.origin_m[axis] - lo) * s;
            let y = oy - (shade.origin_m[2] + shade.size_m[2] - minz) * s;
            svg.push_str(&format!("<rect x=\"{x:.3}\" y=\"{y:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"#e9dac8\" fill-opacity=\"0.5\" stroke=\"#865d3d\" stroke-dasharray=\"4 2\"/>",shade.size_m[axis]*s,shade.size_m[2]*s));
        }
        for z in &model.zones {
            let exterior = z.surfaces.iter().any(|surface| {
                surface.face == *face && matches!(surface.boundary, ThermalBoundary::Outdoors)
            });
            if !exterior {
                continue;
            }
            let x = ox + (z.origin_m[axis] - lo) * s;
            let y = oy - (z.origin_m[2] + z.size_m[2] - minz) * s;
            svg.push_str(&format!("<rect x=\"{x:.3}\" y=\"{y:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"#f3f5f1\" stroke=\"#246954\"/>",z.size_m[axis]*s,z.size_m[2]*s));
            for w in &z.windows {
                if z.surfaces
                    .iter()
                    .any(|f| f.id == w.surface_id && f.face == *face)
                {
                    let wx = x + w.offset_u_m * s;
                    let wy = oy - (z.origin_m[2] + w.offset_v_m + w.height_m - minz) * s;
                    svg.push_str(&format!("<rect x=\"{wx:.3}\" y=\"{wy:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"#c8e4ef\" stroke=\"#328bb1\"/>",w.width_m*s,w.height_m*s));
                    if let Some(h) = &w.overhang {
                        let hx = wx - h.side_extension_m * s;
                        let hy = wy - h.gap_above_m * s;
                        svg.push_str(&format!("<path d=\"M {hx:.3} {hy:.3} h {:.3}\" stroke=\"#865d3d\" stroke-width=\"3\"><title>{} / {}: overhang depth {} m; gap {} m; side extension {} m</title></path>",(w.width_m+2.*h.side_extension_m)*s,escaped(&z.id),escaped(&w.id),h.projection_m,h.gap_above_m,h.side_extension_m));
                    }
                }
            }
        }
    }
    svg.push_str("<text x=\"20\" y=\"794\">Blue: glazing. Brown: overhang / detached shade. Dimensions are keyed by zone and window in the report.</text><text x=\"20\" y=\"812\">Non-occluding input projections, not thermal results.</text></g></svg>");
    svg
}

fn safe_chain(path: &Path) -> Result<()> {
    for p in path.ancestors() {
        let m = match fs::symlink_metadata(p) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        {
            ensure!(
                !m.file_type().is_symlink(),
                "Report paths cannot contain links"
            );
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    m.file_attributes() & (0x400 | 0x1000 | 0x400000) == 0,
                    "Report paths cannot contain reparse or unavailable cloud entries"
                );
            }
        }
    }
    Ok(())
}
pub(crate) fn create_fresh(path: &Path) -> Result<()> {
    safe_chain(path)?;
    ensure!(
        !path.exists(),
        "Report destination already exists; choose a fresh directory"
    );
    fs::create_dir_all(path)?;
    safe_chain(path)
}
pub(crate) fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    ensure!(bytes.len() as u64 <= FILE_CAP, "Report file exceeds 64 MiB");
    let mut f = OpenOptions::new().write(true).create_new(true).open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}
pub(crate) fn artifact(root: &Path, name: &str) -> Result<ReportArtifact> {
    let path = root.join(name);
    safe_chain(&path)?;
    let m = fs::symlink_metadata(&path)?;
    ensure!(
        m.is_file() && m.len() <= FILE_CAP,
        "Invalid report artifact"
    );
    Ok(ReportArtifact {
        path: name.into(),
        bytes: m.len(),
        sha256: file_hash(&path)?,
    })
}
fn file_bound(path: &Path, expected_bytes: u64, expected_hash: &str) -> Result<()> {
    safe_chain(path)?;
    let m = fs::symlink_metadata(path)?;
    ensure!(
        m.is_file() && m.len() == expected_bytes,
        "Pinned renderer file size differs: {}",
        path.file_name().unwrap_or_default().to_string_lossy()
    );
    ensure!(
        file_hash(path)? == expected_hash,
        "Pinned renderer file hash differs"
    );
    Ok(())
}
type PinnedRendererFile = (String, PathBuf, u64, String);
fn renderer_files(config: &TypstConfig) -> Result<(PathBuf, Vec<PinnedRendererFile>)> {
    let d: Value = serde_json::from_str(DEPENDENCY)?;
    let root = config
        .installation_directory
        .canonicalize()
        .context("Typst is not installed; use scripts/install-typst.ps1 separately")?;
    safe_chain(&config.installation_directory)?;
    let exe = root.join("typst.exe");
    let e = &d["members"]["typst.exe"];
    file_bound(
        &exe,
        e["bytes"].as_u64().context("Typst size pin")?,
        e["sha256"].as_str().context("Typst hash pin")?,
    )?;
    let mut fonts = vec![];
    for f in d["fonts"]["files"]
        .as_array()
        .context("Font pins")?
        .iter()
        .chain(std::iter::once(&d["fonts"]["license_file"]))
    {
        let name = f["name"].as_str().context("Font name")?.to_owned();
        let path = root.join("fonts").join(&name);
        let bytes = f["bytes"].as_u64().context("Font size")?;
        let hash = f["sha256"].as_str().context("Font hash")?.to_owned();
        file_bound(&path, bytes, &hash)?;
        fonts.push((name, path, bytes, hash));
    }
    let actual = fs::read_dir(root.join("fonts"))?
        .map(|e| Ok(e?.file_name().to_string_lossy().into_owned()))
        .collect::<Result<std::collections::BTreeSet<_>>>()?;
    ensure!(
        actual == fonts.iter().map(|f| f.0.clone()).collect(),
        "Renderer font directory must contain only pinned fonts and their license"
    );
    Ok((exe, fonts))
}
pub(crate) fn check_budget(root: &Path) -> Result<()> {
    let (total, max) = scratch_sizes(root)?;
    ensure!(
        total <= TOTAL_CAP && max <= FILE_CAP,
        "Report output exceeds 256 MiB total / 64 MiB file policy"
    );
    Ok(())
}
fn active_stage_budget(work: &Path, stage: &Path, destination: &Path) -> Result<u64> {
    let all = scratch_sizes(work)?.0;
    let current = scratch_sizes(stage)?.0;
    let delivered = scratch_sizes(destination)?.0;
    let outside = all
        .checked_sub(current)
        .and_then(|n| n.checked_add(delivered))
        .context("Report byte accounting overflow")?;
    let allowed = TOTAL_CAP
        .checked_sub(outside)
        .context("Combined report staging and delivery budget exceeded")?
        .min(128 * 1024 * 1024);
    ensure!(
        current <= allowed,
        "Combined report staging and delivery budget exceeded"
    );
    Ok(allowed)
}
fn png_header(path: &Path) -> Result<()> {
    let mut f = File::open(path)?;
    let mut header = [0u8; 24];
    f.read_exact(&mut header)?;
    ensure!(
        header[..8] == [137, 80, 78, 71, 13, 10, 26, 10] && &header[12..16] == b"IHDR",
        "Renderer output is not PNG"
    );
    let w = u32::from_be_bytes(header[16..20].try_into()?);
    let h = u32::from_be_bytes(header[20..24].try_into()?);
    ensure!(
        (1..=10000).contains(&w) && (1..=10000).contains(&h),
        "Invalid PNG dimensions"
    );
    Ok(())
}
fn pdf_header(path: &Path) -> Result<()> {
    let mut f = File::open(path)?;
    let mut header = [0u8; 5];
    f.read_exact(&mut header)?;
    ensure!(&header == b"%PDF-", "Renderer output is not PDF");
    Ok(())
}

/// Exports HTML/CSV/JSON/input SVGs without a renderer. When a renderer is
/// requested, missing/modified dependencies or failed native stages are errors;
/// already written data outputs and bounded diagnostics remain inspectable.
/// The caller must construct this DTO using the shared retained-evidence service.
pub fn render(
    data: &EnergyReportData,
    destination: &Path,
    typst: Option<&TypstConfig>,
    cancel: &AtomicBool,
) -> Result<EnergyReportArtifacts> {
    render_with_architecture(data, destination, typst, cancel, None)
}
pub fn render_with_architecture(
    data: &EnergyReportData,
    destination: &Path,
    typst: Option<&TypstConfig>,
    cancel: &AtomicBool,
    architecture: Option<&architectural::ArchitecturalReport>,
) -> Result<EnergyReportArtifacts> {
    data.validate()?;
    let p = data.presentation()?;
    let hash = data.dataset_sha256()?;
    ensure!(
        !cancel.load(Ordering::SeqCst),
        "Report cancelled before writing"
    );
    let mut docs = documents(data, &p, &hash)?;
    if let Some(scope) = architecture {
        scope.validate_binding(data, None)?;
        for (i, doc) in docs.iter_mut().enumerate() {
            let mut sections = if i == 1 {
                scope.detail_sections()?
            } else {
                vec![scope.summary()?]
            };
            sections.append(&mut doc.sections);
            doc.sections = sections;
            architectural::clarify_sections(&mut doc.sections);
            doc.dataset_sha256 = scope.data.report_sha256.clone();
        }
    }
    let architectural_template = architecture.map(|_| architectural::template(TEMPLATE));
    create_fresh(destination)?;
    let mut result=EnergyReportArtifacts{schema_version:"greenlens.energy-report-artifacts.v0.4".into(),dataset_sha256:hash,renderer_status:"not_requested".into(),renderer_message:"HTML, CSV, JSON and SVG available. PDF/PNG require the separately installed pinned Typst renderer.".into(),typst_executable_sha256:None,template_sha256:sha(TEMPLATE.as_bytes()),renderer_work_id:None,renderer_inputs:BTreeMap::new(),artifacts:vec![],processes:vec![]};
    let mut products = vec![
        (
            "energy-report.json".to_string(),
            serde_json::to_vec_pretty(data)?,
        ),
        ("performance.json".into(), serde_json::to_vec_pretty(&p)?),
        ("performance.csv".into(), csv(&p)?.into_bytes()),
        ("inputs.csv".into(), inputs_csv(data)?.into_bytes()),
        (
            "baseline-model.svg".into(),
            model_svg(&data.baseline).into_bytes(),
        ),
        (
            "proposed-model.svg".into(),
            model_svg(&data.proposed).into_bytes(),
        ),
        ("annual.svg".into(), chart(&p, false).into_bytes()),
        ("monthly.svg".into(), chart(&p, true).into_bytes()),
    ];
    if let Some(scope) = architecture {
        products.extend(scope.products()?);
        result.schema_version = "greenlens.energy-report-artifacts.v0.6".into();
        result.dataset_sha256 = scope.data.report_sha256.clone();
        result.template_sha256 = sha(architectural_template.as_ref().unwrap().as_bytes());
    }
    let names = products.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
    let generated = names
        .iter()
        .map(String::as_str)
        .zip(products.into_iter().map(|(_, b)| b))
        .collect::<Vec<_>>();
    for (name, bytes) in &generated {
        write_new(&destination.join(name), bytes)?;
        result.artifacts.push(artifact(destination, name)?);
    }
    for (name, d) in ["assessment", "input-appendix", "performance"]
        .iter()
        .zip(&docs)
    {
        let file = format!("{name}.html");
        write_new(&destination.join(&file), html(d, &generated)?.as_bytes())?;
        result.artifacts.push(artifact(destination, &file)?);
    }
    check_budget(destination)?;
    if let Some(config) = typst {
        let outcome = if let Some(template) = &architectural_template {
            render_native_documents(
                NativeDocuments {
                    docs: &docs,
                    generated: &generated,
                    template,
                    stages: &[
                        ("assessment", 0, false),
                        ("input-appendix", 1, false),
                        ("performance", 2, false),
                        ("performance-pages", 2, true),
                    ],
                },
                destination,
                config,
                cancel,
                &mut result,
            )
        } else {
            render_native(&docs, &generated, destination, config, cancel, &mut result)
        };
        if let Err(error) = outcome {
            result.renderer_status = if cancel.load(Ordering::SeqCst) {
                "cancelled"
            } else {
                "failed"
            }
            .into();
            result.renderer_message="PDF/PNG rendering did not complete; retained data outputs remain available. No scientific status is changed.".into();
            write_new(
                &destination.join("report-artifacts.json"),
                &serde_json::to_vec_pretty(&result)?,
            )?;
            return Err(error.context("Energy report rendering failed; inspect retained report-artifacts.json and owned renderer stage diagnostics"));
        }
        result.renderer_status = "completed".into();
        result.renderer_message="Pinned offline Typst completed all three PDF documents and performance page PNGs; this is rendering evidence, not scientific qualification.".into();
    }
    write_new(
        &destination.join("report-artifacts.json"),
        &serde_json::to_vec_pretty(&result)?,
    )?;
    check_budget(destination)?;
    Ok(result)
}
fn render_native(
    docs: &[Document],
    generated: &[(&str, Vec<u8>)],
    destination: &Path,
    config: &TypstConfig,
    cancel: &AtomicBool,
    result: &mut EnergyReportArtifacts,
) -> Result<()> {
    render_native_documents(
        NativeDocuments {
            docs,
            generated,
            template: TEMPLATE,
            stages: &[
                ("assessment", 0, false),
                ("input-appendix", 1, false),
                ("performance", 2, false),
                ("performance-pages", 2, true),
            ],
        },
        destination,
        config,
        cancel,
        result,
    )
}
pub(crate) struct NativeDocuments<'a> {
    pub(crate) docs: &'a [Document],
    pub(crate) generated: &'a [(&'a str, Vec<u8>)],
    pub(crate) template: &'a str,
    pub(crate) stages: &'a [(&'a str, usize, bool)],
}
pub(crate) fn render_native_documents(
    bundle: NativeDocuments<'_>,
    destination: &Path,
    config: &TypstConfig,
    cancel: &AtomicBool,
    result: &mut EnergyReportArtifacts,
) -> Result<()> {
    let NativeDocuments {
        docs,
        generated,
        template,
        stages,
    } = bundle;
    ensure!(
        !stages.is_empty() && stages.len() <= 6,
        "Expected bounded owned report stages"
    );
    for (name, index, _) in stages {
        ensure!(
            !name.is_empty()
                && name.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
                && *index < docs.len(),
            "Invalid owned report stage"
        );
    }
    let start = Instant::now();
    let (exe, fonts) = renderer_files(config)?;
    let exe_hash = file_hash(&exe)?;
    result.typst_executable_sha256 = Some(exe_hash.clone());
    safe_chain(&config.scratch_directory)?;
    let work = config
        .scratch_directory
        .join(format!("energy-report-{}", uuid::Uuid::new_v4()));
    create_fresh(&work)?;
    result.renderer_work_id = work.file_name().map(|s| s.to_string_lossy().into_owned());
    for &(name, document_index, png) in stages {
        ensure!(!cancel.load(Ordering::SeqCst), "Report cancelled");
        ensure!(start.elapsed() < DEADLINE, "Whole report deadline exceeded");
        let stage = work.join(name);
        create_fresh(&stage)?;
        for dir in ["fonts", "empty-packages", "empty-package-cache"] {
            fs::create_dir(stage.join(dir))?;
        }
        for (font, source, bytes, hash) in &fonts {
            file_bound(source, *bytes, hash)?;
            fs::copy(source, stage.join("fonts").join(font))?;
            file_bound(&stage.join("fonts").join(font), *bytes, hash)?;
        }
        for (file, bytes) in generated.iter().filter(|(name, _)| name.ends_with(".svg")) {
            write_new(&stage.join(file), bytes)?;
        }
        let d = &docs[document_index];
        write_new(&stage.join("document.json"), &serde_json::to_vec_pretty(d)?)?;
        write_new(&stage.join("report.typ"), template.as_bytes())?;
        let mut input_hashes = fs::read_dir(&stage)?
            .map(|entry| {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    Ok(Some((
                        entry.file_name().to_string_lossy().into_owned(),
                        file_hash(&entry.path())?,
                    )))
                } else {
                    Ok(None)
                }
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<BTreeMap<_, _>>();
        for (font, _, _, hash) in &fonts {
            input_hashes.insert(format!("fonts/{font}"), hash.clone());
        }
        result
            .renderer_inputs
            .insert(name.into(), input_hashes.clone());
        check_budget(&work)?;
        file_bound(&exe, 52_487_680, &exe_hash)?;
        let output = if png {
            "page-{0p}-of-{t}.png"
        } else {
            "report.pdf"
        };
        let mut args = vec![
            "compile",
            "--root",
            ".",
            "--font-path",
            "fonts",
            "--ignore-system-fonts",
            "--ignore-embedded-fonts",
            "--package-path",
            "empty-packages",
            "--package-cache-path",
            "empty-package-cache",
            "--creation-timestamp",
            "0",
            "--jobs",
            "1",
            "--diagnostic-format",
            "short",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        if png {
            args.extend(["--ppi".into(), "96".into()]);
        }
        args.extend(["report.typ".into(), output.into()]);
        let request = NativeRequest {
            executable: exe.clone(),
            arguments: args,
            directory: stage.clone(),
            child_environment: BTreeMap::from([
                ("HTTP_PROXY".into(), "http://127.0.0.1:9".into()),
                ("HTTPS_PROXY".into(), "http://127.0.0.1:9".into()),
                ("ALL_PROXY".into(), "http://127.0.0.1:9".into()),
            ]),
            child_environment_remove: std::env::vars_os()
                .filter_map(|(k, _)| {
                    k.to_str()
                        .filter(|s| s.to_ascii_uppercase().starts_with("TYPST_"))
                        .map(str::to_owned)
                })
                .collect(),
            limits: Limits {
                timeout: DEADLINE
                    .checked_sub(start.elapsed())
                    .context("Whole report deadline exceeded before launch")?
                    .min(Duration::from_secs(120)),
                max_scratch_bytes: active_stage_budget(&work, &stage, destination)?,
                max_file_bytes: FILE_CAP,
                max_observed_working_set_bytes: 2 * 1024 * 1024 * 1024,
            },
        };
        let process = match execute(&request, cancel) {
            Ok(p) => p,
            Err(error) => {
                let path = stage.join("process.json");
                if path.is_file() && path.metadata()?.len() <= 1024 * 1024 {
                    let record = fs::read(&path)?;
                    if let Ok(p) = serde_json::from_slice(&record) {
                        result.processes.push(p);
                    }
                }
                return Err(error);
            }
        };
        result.processes.push(process);
        ensure!(start.elapsed() < DEADLINE, "Whole report deadline exceeded");
        file_bound(&exe, 52_487_680, &exe_hash)?;
        for (file, hash) in &input_hashes {
            ensure!(
                file_hash(&stage.join(file))? == *hash,
                "Owned renderer input changed during execution"
            );
        }
        for (font, _, bytes, hash) in &fonts {
            file_bound(&stage.join("fonts").join(font), *bytes, hash)?;
        }
        for dir in ["empty-packages", "empty-package-cache"] {
            ensure!(
                fs::read_dir(stage.join(dir))?.next().is_none(),
                "Unexpected renderer package activity"
            );
        }
        check_budget(&work)?;
        if !png {
            pdf_header(&stage.join(output))?;
            let file = format!("{name}.pdf");
            copy_artifact(&stage.join(output), destination, &file, result)?;
        } else {
            let mut pages = fs::read_dir(&stage)?
                .collect::<std::io::Result<Vec<_>>>()?
                .into_iter()
                .filter(|e| e.file_name().to_string_lossy().ends_with(".png"))
                .map(|e| e.path())
                .collect::<Vec<_>>();
            pages.sort();
            ensure!(
                !pages.is_empty() && pages.len() <= 128,
                "Expected bounded complete performance PNG page set"
            );
            let total = pages.len();
            for (i, page) in pages.iter().enumerate() {
                let stem = page
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .context("PNG name")?;
                let parts = stem.split('-').collect::<Vec<_>>();
                ensure!(
                    parts.len() == 4
                        && parts[0] == "page"
                        && parts[2] == "of"
                        && parts[1].parse::<usize>()? == i + 1
                        && parts[3].parse::<usize>()? == total,
                    "Incomplete PNG page sequence"
                );
                png_header(page)?;
                let file = format!("performance-page-{:03}.png", i + 1);
                copy_artifact(page, destination, &file, result)?;
            }
        }
        // Only our verified duplicate fonts are removed, after complete child and
        // descendant closure. No source installation or imported file is mutated.
        for (font, _, bytes, hash) in &fonts {
            let file = stage.join("fonts").join(font);
            file_bound(&file, *bytes, hash)?;
            fs::remove_file(file)?;
        }
        check_budget(destination)?;
        ensure!(
            scratch_sizes(&work)?
                .0
                .checked_add(scratch_sizes(destination)?.0)
                .context("Report byte accounting overflow")?
                <= TOTAL_CAP,
            "Combined report staging and delivery budget exceeded"
        );
    }
    ensure!(
        !cancel.load(Ordering::SeqCst) && start.elapsed() < DEADLINE,
        "Report cancelled or whole deadline exceeded before completion"
    );
    Ok(())
}
fn copy_artifact(
    source: &Path,
    destination: &Path,
    name: &str,
    result: &mut EnergyReportArtifacts,
) -> Result<()> {
    safe_chain(source)?;
    let meta = fs::metadata(source)?;
    ensure!(
        meta.is_file() && meta.len() > 0 && meta.len() <= FILE_CAP,
        "Invalid renderer artifact size"
    );
    let mut input = File::open(source)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination.join(name))?;
    let n = std::io::copy(
        &mut Read::by_ref(&mut input).take(FILE_CAP + 1),
        &mut output,
    )?;
    ensure!(
        n == meta.len() && n <= FILE_CAP,
        "Renderer artifact changed or exceeded limit while copying"
    );
    output.sync_all()?;
    let item = artifact(destination, name)?;
    ensure!(
        item.sha256 == file_hash(source)?,
        "Renderer artifact copy differs"
    );
    result.artifacts.push(item);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_eight_zone_overhangs_use_keyed_rows_not_overlapping_svg_text() {
        // Geometry/annotation fixture only; no new simulated result or assessment.
        let data = synthetic_energy_report().unwrap();
        let mut case = data.proposed.clone();
        let mut second_wing = case.model.zones.clone();
        for z in &mut second_wing {
            z.id = format!("copy_{}", z.id);
            z.origin_m[0] += 20.;
            for s in &mut z.surfaces {
                s.id = format!("copy_{}", s.id);
                if let ThermalBoundary::Interzone {
                    zone_id,
                    surface_id,
                } = &mut s.boundary
                {
                    *zone_id = format!("copy_{zone_id}");
                    *surface_id = format!("copy_{surface_id}");
                }
            }
            for w in &mut z.windows {
                w.id = format!("copy_{}", w.id);
                w.surface_id = format!("copy_{}", w.surface_id);
            }
            if let Some(system) = &mut z.cooling_system {
                system.id = format!("copy_{}", system.id);
            }
        }
        case.model.zones.extend(second_wing);
        validate_thermal_model(&case.model, &case.study.calendar).unwrap();
        let before = serde_json::to_vec(&case).unwrap();
        let svg = model_svg(&case);
        let table = overhang_dimensions(&case).unwrap().unwrap();
        assert_eq!(table.rows.len(), 8);
        assert_eq!(svg.matches("stroke-width=\"3\"").count(), 8);
        // The first south overhang is still the same projected line: 36 m
        // total width, window u=2, v=1, w=3, h=1.5, extension/gap=0.1 m.
        assert!(svg.contains("d=\"M 50.583 481.833 h 34.667\""));
        for (z, row) in case.model.zones.iter().zip(&table.rows) {
            assert_eq!(
                row,
                &vec![
                    format!("{}\n{}_window\nSouth", z.id, z.id),
                    "2\n1".into(),
                    "3\n1.5".into(),
                    "0.6\n0.1\n0.1".into(),
                ]
            );
            assert!(svg.contains(&format!(
                "<title>{} / {}_window: overhang depth 0.6 m; gap 0.1 m; side extension 0.1 m</title>",
                z.id, z.id
            )));
        }
        for text in svg.split("<text ").skip(1) {
            let content = text
                .split_once('>')
                .unwrap()
                .1
                .split("</text>")
                .next()
                .unwrap();
            assert!(!content.contains("overhang depth"));
        }
        assert_eq!(svg, model_svg(&case));
        assert_eq!(serde_json::to_vec(&case).unwrap(), before);
        assert!(overhang_dimensions(&data.baseline).unwrap().is_none());

        let docs = documents(&data, &data.presentation().unwrap(), &"a".repeat(64)).unwrap();
        let mut legends = 0;
        for doc in &docs {
            for (i, section) in doc.sections.iter().enumerate() {
                if section.title.ends_with(" — overhang dimensions") {
                    legends += 1;
                    assert_eq!(
                        doc.sections[i - 1].image.as_deref(),
                        Some("proposed-model.svg")
                    );
                }
            }
        }
        // Assessment and appendix contain the model; focused performance does not.
        assert_eq!(legends, 2);
    }
    #[test]
    fn failed_scientific_assessments_are_visible_in_every_document() {
        // Synthetic presentation fixture only: no execution or qualification evidence.
        let mut data = synthetic_energy_report().unwrap();
        for (side, case) in [
            ("baseline", &mut data.baseline),
            ("proposed", &mut data.proposed),
        ] {
            let q = &mut case.assessment.scientific;
            q.status = EnergyQualificationStatus::Failed;
            q.reason = format!("Synthetic {side} failure <criterion> & retained reason");
            q.tested_scope = format!("Synthetic {side} presentation scope only");
            q.registry_version = "synthetic-report-regression-v1".into();
            q.applicable_physical_input_sha256 = vec![case.physical_input_sha256.clone()];
            q.evidence_ids = vec![
                format!("synthetic-{side}-failure-01"),
                format!("synthetic-{side}-failure-02"),
            ];
            assert_eq!(
                case.assessment.execution,
                EnergyExecutionStatus::NotExecuted
            );
        }
        data.validate().unwrap();
        let docs = documents(&data, &data.presentation().unwrap(), &"a".repeat(64)).unwrap();
        assert_eq!(docs.len(), 3);
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("failed report 空間");
        let artifacts = render(&data, &target, None, &AtomicBool::new(false)).unwrap();
        assert_eq!(artifacts.renderer_status, "not_requested");
        assert!(artifacts.processes.is_empty());
        for (doc, name) in docs
            .iter()
            .zip(["assessment", "input-appendix", "performance"])
        {
            let html = fs::read_to_string(target.join(format!("{name}.html"))).unwrap();
            for case in [&data.baseline, &data.proposed] {
                let q = &case.assessment.scientific;
                let section = doc
                    .sections
                    .iter()
                    .find(|s| s.title == format!("{} — assessment", case.label))
                    .unwrap();
                for row in [
                    vec!["Execution".to_owned(), "\"not_executed\"".into()],
                    vec!["Scientific qualification".into(), "\"failed\"".into()],
                    vec!["Reason".into(), q.reason.clone()],
                    vec!["Registry".into(), q.registry_version.clone()],
                    vec!["Scientific evidence IDs".into(), q.evidence_ids.join("\n")],
                ] {
                    assert!(section.rows.contains(&row), "Missing row in {}", doc.kind);
                    for value in row {
                        assert!(
                            html.contains(&escaped(&value)),
                            "Missing escaped value in {name}"
                        );
                    }
                }
                assert!(!html.contains(&q.reason));
            }
        }
    }
    #[test]
    fn input_differences_survive_html_and_pdf_projection_including_empty_case() {
        // Synthetic presentation fixtures only; no engine or PDF process is run.
        let different = synthetic_energy_report().unwrap();
        assert!(!different.comparison.input_relation.differences.is_empty());
        let mut identical = different.clone();
        identical.proposed = identical.baseline.clone();
        identical.proposed.id = "proposed-identical-inputs".into();
        identical.proposed.label = "Synthetic identical-input comparison".into();
        identical.comparison = compare_energy_totals(
            &identical.baseline.totals,
            &identical.proposed.totals,
            compare_energy_inputs(
                &identical.baseline.model,
                &identical.baseline.study,
                &identical.proposed.model,
                &identical.proposed.study,
            )
            .unwrap(),
            identical.comparison.lighting_service_equivalence.clone(),
        )
        .unwrap();
        assert!(identical.comparison.input_relation.differences.is_empty());
        identical.validate().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        for (name, data) in [("different", different), ("identical", identical)] {
            let before = serde_json::to_vec(&data).unwrap();
            let target = tmp.path().join(name);
            let artifacts = render(&data, &target, None, &AtomicBool::new(false)).unwrap();
            assert!(artifacts.processes.is_empty());
            let docs = documents(
                &data,
                &data.presentation().unwrap(),
                &artifacts.dataset_sha256,
            )
            .unwrap();
            let expected = if name == "identical" {
                vec!["No input differences are listed for these retained snapshots. Comparison scope and service limitations still apply.".to_owned()]
            } else {
                data.comparison.input_relation.differences.clone()
            };
            for (doc, file) in [
                (&docs[0], "assessment.html"),
                (&docs[2], "performance.html"),
            ] {
                // This exact serialized Document is written as the PDF template's
                // document.json; compare every ordered entry, not just its heading.
                let pdf_input: Value =
                    serde_json::from_slice(&serde_json::to_vec_pretty(doc).unwrap()).unwrap();
                let section = pdf_input["sections"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|s| s["title"] == "Baseline/proposed input differences")
                    .unwrap();
                assert_eq!(section["paragraphs"], json!(expected));
                let html = fs::read_to_string(target.join(file)).unwrap();
                for difference in &expected {
                    assert!(
                        html.contains(&format!("<p>{}</p>", escaped(difference))),
                        "Missing comparison entry in {name}/{file}: {difference}"
                    );
                }
            }
            let retained: EnergyReportData =
                serde_json::from_slice(&fs::read(target.join("energy-report.json")).unwrap())
                    .unwrap();
            assert_eq!(serde_json::to_vec(&retained).unwrap(), before);
            assert_eq!(serde_json::to_vec(&data).unwrap(), before);
        }
    }
    #[test]
    fn all_formats_use_one_snapshot_and_literal_strings() {
        let mut data = synthetic_energy_report().unwrap();
        data.title = "測試 </title><script>alert(1)</script> #include \"evil.typ\"".into();
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("report 空間");
        let result = render(&data, &target, None, &AtomicBool::new(false)).unwrap();
        assert_eq!(result.renderer_status, "not_requested");
        assert_eq!(result.artifacts.len(), 11);
        let html = fs::read_to_string(target.join("assessment.html")).unwrap();
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("<svg"));
        assert!(!html.contains("<img src="));
        assert!(html.contains("Missing — No source-bound carbon factor"));
        let restored: EnergyReportData =
            serde_json::from_slice(&fs::read(target.join("energy-report.json")).unwrap()).unwrap();
        assert_eq!(restored, data);
        let p = data.presentation().unwrap();
        assert!(html.contains(&escaped(&cell(&p.baseline.annual.facility))));
        assert!(html.contains(&escaped(&cell(&p.proposed.annual.facility))));
        let csv = fs::read_to_string(target.join("performance.csv")).unwrap();
        assert!(csv.contains("presentation.baseline.annual.facility"));
        assert!(csv.contains("\"8760.0\""));
        let svg = fs::read_to_string(target.join("baseline-model.svg")).unwrap();
        for z in &data.baseline.model.zones {
            assert!(svg.contains(&z.id));
        }
        assert!(render(&data, &target, None, &AtomicBool::new(false)).is_err());
    }
    #[test]
    fn invalid_snapshot_cancel_and_missing_renderer_never_claim_pdf_success() {
        let mut data = synthetic_energy_report().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let invalid = tmp.path().join("invalid");
        data.baseline.model.zones[0].size_m[0] += 1.;
        assert!(render(&data, &invalid, None, &AtomicBool::new(false)).is_err());
        assert!(!invalid.exists());
        let data = synthetic_energy_report().unwrap();
        let cancelled = tmp.path().join("cancelled");
        assert!(render(&data, &cancelled, None, &AtomicBool::new(true)).is_err());
        assert!(!cancelled.exists());
        let missing = tmp.path().join("missing");
        let config = TypstConfig {
            installation_directory: tmp.path().join("absent"),
            scratch_directory: tmp.path().join("scratch"),
        };
        assert!(render(&data, &missing, Some(&config), &AtomicBool::new(false)).is_err());
        let receipt: EnergyReportArtifacts =
            serde_json::from_slice(&fs::read(missing.join("report-artifacts.json")).unwrap())
                .unwrap();
        assert_eq!(receipt.renderer_status, "failed");
        assert!(missing.join("assessment.html").exists());
        assert!(!missing.join("assessment.pdf").exists());
    }
    #[test]
    fn csv_text_guards_and_lossless_schedule_ranges() {
        assert_eq!(csv_input_value(&json!(-12.5)), "\"-12.5\"");
        assert_eq!(csv_input_value(&json!("-12.5")), "\"'-12.5\"");
        let mut p = synthetic_energy_report().unwrap().presentation().unwrap();
        p.facility_energy_savings.absolute = ReportCell::value(-12.5, EnergyUnit::Kwh, 2).unwrap();
        let csv = csv(&p).unwrap();
        assert!(csv.contains("\"-12.5\""));
        assert!(!csv.contains("\"'-12.5"));
        assert_eq!(
            timestamp(&interval_timestamp(52559).unwrap()),
            "12/31 23:50-24:00\nLocal standard time; interval 52559"
        );
        assert_eq!(
            csv_field(" =HYPERLINK(\"x\")"),
            "\"' =HYPERLINK(\"\"x\"\")\""
        );
        let mut rows = vec![];
        flatten("schedule", &json!([0, 0, 1, 1, 1, 0]), &mut rows);
        assert_eq!(
            rows,
            vec![
                vec!["schedule[0..2)", "0"],
                vec!["schedule[2..5)", "1"],
                vec!["schedule[5..6)", "0"]
            ]
        );
    }
    #[test]
    #[ignore = "Requires separately installed pinned native Typst; explicit missing-engine failure"]
    fn native_typst_all_three_documents_and_pages() {
        let install = std::env::var_os("GREENLENS_TYPST_DIR")
            .expect("GREENLENS_TYPST_DIR must name pinned Typst installation");
        let base = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
            .join("GreenLens")
            .join("verification")
            .join(format!(
                "energy production report {} 空間",
                uuid::Uuid::new_v4()
            ));
        fs::create_dir_all(&base).unwrap();
        let config = TypstConfig {
            installation_directory: install.into(),
            scratch_directory: base.join("scratch"),
        };
        let mut data = synthetic_energy_report().unwrap();
        data.title = "Original synthetic energy report — 能源報告 #include </script>".into();
        let result = render(
            &data,
            &base.join("report"),
            Some(&config),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(result.renderer_status, "completed");
        assert_eq!(result.processes.len(), 4);
        assert!(
            result
                .artifacts
                .iter()
                .filter(|a| a.path.ends_with(".png"))
                .count()
                > 0
        );
        println!("Retained report: {}", base.display());
    }
}
