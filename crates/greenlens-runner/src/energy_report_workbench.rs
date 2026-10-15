//! Additive report profiles from validated retained snapshots. No simulation.
use crate::{
    energy_attachments::{self, EnergyAttachment},
    energy_report::{self as old, Document, EnergyReportArtifacts, Section, TypstConfig},
    persistence::regular_child,
};
use anyhow::{ensure, Result};
use greenlens_core::energy::*;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

const TEMPLATE: &str = include_str!("../../../reports/energy/workbench.typ");
/// Resolve one generated inert file after checking its manifest-bound identity.
pub fn verified_artifact(root: &Path, relative: &str) -> Result<std::path::PathBuf> {
    let manifest: EnergyReportArtifacts =
        crate::persistence::read_json(&regular_child(root, "report-artifacts.json")?)?;
    let item = manifest
        .artifacts
        .iter()
        .find(|a| a.path == relative)
        .ok_or_else(|| anyhow::anyhow!("Choose a generated artifact from the report manifest"))?;
    let target = regular_child(root, relative)?;
    ensure!(
        matches!(
            target.extension().and_then(|s| s.to_str()),
            Some("pdf" | "html" | "png" | "csv" | "json" | "svg")
        ),
        "Only inert generated report products may be opened"
    );
    ensure!(
        item.bytes <= crate::persistence::MAX_EVIDENCE_FILE_BYTES
            && fs::metadata(&target)?.len() == item.bytes
            && greenlens_process::file_hash(&target)? == item.sha256,
        "Report changed after generation; inspect integrity before opening"
    );
    Ok(target)
}
pub fn template_sha256() -> String {
    old::sha(TEMPLATE.as_bytes())
}

/// Local inventory is complete, including unchecked/unavailable entries. Only
/// selected applicable evidence is opened and hash-verified for inclusion.
pub fn attachment_preflight(
    root: &Path,
    items: &[EnergyAttachment],
    models: [&str; 2],
) -> Result<EnergyAttachmentPreflight> {
    ensure!(items.len() <= 1024, "At most1024 attachment selections");
    let mut decisions = vec![];
    for item in items {
        let (disposition, reason) = if item.validate().is_err() {
            (
                EnergyAttachmentDisposition::Invalid,
                "Attachment metadata is invalid; remove or re-import explicitly.",
            )
        } else if !models.contains(&item.model_input_sha256.as_str()) {
            (EnergyAttachmentDisposition::Inapplicable,"The attachment belongs to neither selected retained model; deselect it or select the matching evidence pair. No automatic rebinding.")
        } else if regular_child(root, &item.path).is_err() {
            (
                EnergyAttachmentDisposition::Missing,
                "A safe local attachment file is unavailable; restore or explicitly deselect it.",
            )
        } else if item.include_in_export && energy_attachments::verify(root, item).is_err() {
            (EnergyAttachmentDisposition::Invalid,"Selected attachment bytes/type/seal do not match; restore or explicitly deselect it.")
        } else if item.include_in_export {
            (
                EnergyAttachmentDisposition::Included,
                "Selected evidence matches one retained model and its verified file seal.",
            )
        } else {
            (
                EnergyAttachmentDisposition::Excluded,
                "Explicitly unchecked; file and private metadata will not be exported.",
            )
        };
        decisions.push(EnergyAttachmentDecision {
            id: item.id.clone(),
            selected: item.include_in_export,
            disposition,
            reason: reason.into(),
        });
    }
    let ready = decisions
        .iter()
        .all(|d| !d.selected || d.disposition == EnergyAttachmentDisposition::Included);
    let result = EnergyAttachmentPreflight {
        schema_version: "greenlens.energy-attachment-preflight.v1".into(),
        ready,
        items: decisions,
    };
    // Duplicate IDs are errors even if no entry is selected.
    if ready {
        result.summary()?;
    }
    Ok(result)
}
pub fn prepare_data(
    mut base: EnergyReportData,
    metadata: EnergyReportMetadata,
    profiles: Vec<EnergyReportProfile>,
    preflight: &EnergyAttachmentPreflight,
) -> Result<EnergyReportWorkbenchData> {
    let summary = preflight.summary()?;
    base.evidence.retain(|e| e.included);
    Ok(EnergyReportWorkbenchData::new(
        base,
        metadata,
        profiles,
        summary,
        template_sha256(),
    )?)
}
fn ser(v: &impl Serialize) -> Result<String> {
    let value = serde_json::to_value(v)?;
    Ok(if let Some(label) = value.as_str() {
        label.replace('_', " ")
    } else {
        serde_json::to_string(&value)?
    })
}
fn input_value(v: &ThermalInputValue) -> String {
    match v {
        ThermalInputValue::Missing => "Absent".into(),
        ThermalInputValue::Null => "Not supplied".into(),
        ThermalInputValue::Boolean(v) => if *v { "Yes" } else { "No" }.into(),
        ThermalInputValue::Number(v) => v.to_string(),
        ThermalInputValue::Text(v) => v.clone(),
        ThermalInputValue::ObjectPresent => "Present".into(),
    }
}
fn source_text(s: &InputSource) -> String {
    format!(
        "{}\n{}",
        s.citation,
        if s.synthetic {
            "Synthetic input; not measured product or climate data."
        } else {
            "Declared non-synthetic source; applicability is not inferred."
        }
    )
}
fn infiltration_text(v: &InfiltrationRate) -> String {
    match v {
        InfiltrationRate::AirChangesPerHour { value } => format!("{value} air changes/hour"),
        InfiltrationRate::FlowM3PerSecond { value } => format!("{value} m³/s total flow"),
        InfiltrationRate::FlowM3PerSecondPerFloorM2 { value } => {
            format!("{value} m³/(s·m²) of floor area")
        }
    }
}
fn curve_equation(form: &CurveForm) -> String {
    let polynomial = |coefficients: &[f64], terms: &[&str]| {
        coefficients
            .iter()
            .zip(terms)
            .map(|(c, t)| format!("({c}){t}"))
            .collect::<Vec<_>>()
            .join(" + ")
    };
    match form {
        CurveForm::Quadratic { coefficients, x } => format!(
            "f(x) = {}\nx: {} to {}",
            polynomial(coefficients, &["", "·x", "·x²"]),
            x[0],
            x[1]
        ),
        CurveForm::Cubic { coefficients, x } => format!(
            "f(x) = {}\nx: {} to {}",
            polynomial(coefficients, &["", "·x", "·x²", "·x³"]),
            x[0],
            x[1]
        ),
        CurveForm::Biquadratic { coefficients, x, y } => format!(
            "f(x,y) = {}\nx: {} to {}; y: {} to {}",
            polynomial(coefficients, &["", "·x", "·x²", "·y", "·y²", "·x·y"]),
            x[0],
            x[1],
            y[0],
            y[1]
        ),
    }
}
fn carbon_text(f: &Option<CarbonFactor>) -> String {
    match f {
        None => "Not supplied; no emissions result is inferred.".into(),
        Some(f) => format!("{} {} per kWh of electricity\nRegion: {}; year: {}\nSource: {}\nVersion: {}; ID: {}\nSynthetic: {}", f.kg_per_kwh, match f.unit { EmissionUnit::KgCo2 => "kgCO2", EmissionUnit::KgCo2e => "kgCO2e" }, f.region, f.year, f.source, f.version, f.id, f.synthetic),
    }
}
/// Complete run-length presentation, preserving exact slot values and 24:00.
fn profile_intervals(profile: &DayProfile) -> String {
    let clock = |slot: usize| format!("{:02}:{:02}", slot / 6, slot % 6 * 10);
    let mut rows = vec![];
    let mut start = 0;
    while start < profile.values.len() {
        let mut end = start + 1;
        while end < profile.values.len() && profile.values[end] == profile.values[start] {
            end += 1;
        }
        rows.push(format!(
            "{}–{}: {}",
            clock(start),
            clock(end),
            profile.values[start]
        ));
        start = end;
    }
    rows.join("\n")
}
fn schedule_table(label: &str, s: &Schedule) -> Section {
    let mut rows = vec![
        vec!["Weekday".into(), profile_intervals(&s.weekday)],
        vec!["Saturday".into(), profile_intervals(&s.saturday)],
        vec!["Sunday".into(), profile_intervals(&s.sunday)],
    ];
    rows.extend(s.holidays.iter().map(|h| {
        vec![
            format!("Holiday {:02}-{:02}", h.date.month, h.date.day),
            profile_intervals(&h.profile),
        ]
    }));
    let mut section = Section::table(
        &format!("{label} schedule {} values", s.id),
        &["Day profile", "Local-standard interval and value"],
        rows,
    );
    section.paragraphs.push(format!("Values are {}. Intervals include their start and exclude their end. Dated holidays override the ordinary day profile; five-minute calculations repeat each ten-minute input without interpolation.", schedule_unit(s.kind)));
    section
}
fn provenance(c: &EnergyReportCase) -> Result<Vec<Section>> {
    let w = &c.weather_summary;
    let loc = &w.location;
    let cal = &c.study.calendar;
    let method = &c.method;
    let mut sections = vec![Section::table(&format!("{} weather and calendar", c.label), &["Item", "Declared value / identity"], vec![
        vec!["Station".into(), format!("{}, {}, {}\nStation: {}; provider: {}", loc.city, loc.region, loc.country, loc.station, loc.source)],
        vec!["Location".into(), format!("{}° north; {}° east; {} m elevation", loc.latitude_deg_north, loc.longitude_deg_east, loc.elevation_m)],
        vec!["Weather source".into(), source_text(&w.source)],
        vec!["Admission profile".into(), w.binding.profile_id.clone()],
        vec!["Coverage / zone timestep".into(), format!("{} hours; {} zone timesteps/hour", w.year_hours, c.study.zone_timesteps_per_hour)],
        vec!["Calendar".into(), format!("Non-leap year; January 1 {}\nFixed local-standard UTC offset: {} minutes; no daylight-saving adjustment.", ser(&cal.start_weekday)?, cal.utc_offset_minutes)],
        vec!["Declared holidays".into(), if cal.holidays.is_empty() { "None".into() } else { cal.holidays.iter().map(|v| format!("{:02}-{:02}", v.month, v.day)).collect::<Vec<_>>().join(", ") }],
        vec!["Original weather SHA-256".into(), w.binding.original_sha256.clone()],
        vec!["Normalized weather SHA-256".into(), w.binding.normalized_sha256.clone()],
        vec!["Time-axis SHA-256".into(), w.binding.time_axis_sha256.clone()],
    ]), Section::prose(&format!("{} carbon factor", c.label), vec![carbon_text(&c.carbon_factor)]),
    Section::table(&format!("{} method and retained evidence", c.label), &["Identity", "Retained value"], vec![
        vec!["Method / adapter / EnergyPlus".into(), format!("{} / {} / {}", method.method_id, method.adapter_version, method.engine_version)],
        vec!["Run / immutable record SHA-256".into(), format!("{}\n{}", c.run_id, c.record_sha256)],
        vec!["Typed model SHA-256".into(), c.model_input_sha256.clone()],
        vec!["Physical inputs SHA-256".into(), c.physical_input_sha256.clone()],
        vec!["Processing SHA-256".into(), c.processing_sha256.clone()],
        vec!["Recipe SHA-256".into(), method.recipe_sha256.clone()],
        vec!["Numerical settings SHA-256".into(), method.numerical_settings_sha256.clone()],
        vec!["Engine package SHA-256".into(), method.engine_package_sha256.clone()],
        vec!["Engine executable SHA-256".into(), method.engine_executable_sha256.clone()],
        vec!["Engine schema SHA-256".into(), method.engine_schema_sha256.clone()],
    ])];
    let count = |v: Option<u32>| {
        v.map(|x| x.to_string())
            .unwrap_or("Unavailable; not assumed zero".into())
    };
    let mut diagnostics = Section::prose(&format!("{} native diagnostics", c.label), vec![format!("Warnings: {}; severe errors: {}. Counts are retained native summaries, not qualification or accuracy measures.", count(c.diagnostics.warning_count), count(c.diagnostics.severe_count)), c.diagnostics.convergence_disclosure.clone(), format!("Diagnostic source record SHA-256: {}", c.diagnostics.source_record_sha256)]);
    diagnostics.paragraphs.extend(
        c.diagnostics
            .messages
            .iter()
            .enumerate()
            .map(|(i, m)| format!("Message {}: {m}", i + 1)),
    );
    sections.push(diagnostics);
    let mut ledger = Section::table(
        &format!("{} electrical accounting boundary", c.label),
        &[
            "End use / classification",
            "Native channels",
            "Boundary / classification source",
        ],
        c.meter_ledger
            .entries
            .iter()
            .map(|e| {
                Ok(vec![
                    format!(
                        "{} / {}\n{}",
                        ser(&e.end_use)?,
                        ser(&e.classification)?,
                        ser(&e.fuel)?
                    ),
                    e.native_channels.join("\n"),
                    format!(
                        "{}\n{}",
                        e.boundary,
                        e.classification_source
                            .as_deref()
                            .unwrap_or("No classification source supplied")
                    ),
                ])
            })
            .collect::<Result<_>>()?,
    );
    ledger.paragraphs.push(format!("Ledger: {}. Facility control total: {}. Listed end uses are disjoint; unsupported systems are not measured zero contributions.", c.meter_ledger.version, c.meter_ledger.facility_native_channel));
    sections.push(ledger);
    Ok(sections)
}
fn issue(d: &EnergyReportWorkbenchData) -> Section {
    let m = &d.metadata;
    Section::table(
        "Issue record",
        &["Field", "Declared value"],
        vec![
            vec!["Title".into(), m.title.clone()],
            vec!["Revision".into(), m.revision.clone()],
            vec![
                "Issue date".into(),
                m.issue_date.clone().unwrap_or("Not supplied".into()),
            ],
            vec![
                "Author".into(),
                m.author.clone().unwrap_or("Not supplied".into()),
            ],
            vec![
                "Preparer".into(),
                m.preparer.clone().unwrap_or("Not supplied".into()),
            ],
        ],
    )
}
fn status(d: &EnergyReportWorkbenchData) -> Result<Section> {
    Ok(Section::table(
        "Evidence and qualification",
        &[
            "Case",
            "Execution / integrity / currentness",
            "Scientific scope",
        ],
        [&d.base.baseline, &d.base.proposed]
            .into_iter()
            .map(|c| {
                Ok(vec![
                    c.label.clone(),
                    format!(
                        "{} / {}\nphysical:{}; processed:{}; derived:{}",
                        ser(&c.assessment.execution)?,
                        ser(&c.assessment.evidence_integrity)?,
                        ser(&c.assessment.current_physical_inputs)?,
                        ser(&c.assessment.current_processed_results)?,
                        ser(&c.assessment.current_derived_results)?
                    ),
                    format!(
                        "{}\n{}\n{}",
                        ser(&c.assessment.scientific.status)?,
                        c.assessment.scientific.reason,
                        c.assessment.scientific.tested_scope
                    ),
                ])
            })
            .collect::<Result<_>>()?,
    ))
}
fn findings(p: &EnergyReportPresentation) -> Section {
    Section::table(
        "Annual results",
        &["Quantity", "Baseline", "Proposed"],
        vec![
            vec![
                "Facility electricity".into(),
                old::cell(&p.baseline.annual.facility),
                old::cell(&p.proposed.annual.facility),
            ],
            vec![
                "Aligned ten-minute facility peak".into(),
                old::cell(&p.baseline.peak_kw),
                old::cell(&p.proposed.peak_kw),
            ],
            vec![
                "Own peak, local standard time".into(),
                old::timestamp(&p.baseline.peak_timestamp),
                old::timestamp(&p.proposed.peak_timestamp),
            ],
            vec![
                "Gross area / EUI".into(),
                format!(
                    "{} / {}",
                    old::cell(&p.baseline.gross_floor_area),
                    old::cell(&p.baseline.gross_eui)
                ),
                format!(
                    "{} / {}",
                    old::cell(&p.proposed.gross_floor_area),
                    old::cell(&p.proposed.gross_eui)
                ),
            ],
            vec![
                "Explicit factor emissions".into(),
                old::cell(&p.baseline.emissions),
                old::cell(&p.proposed.emissions),
            ],
        ],
    )
}
fn comfort(d: &EnergyReportWorkbenchData, p: &EnergyReportPresentation) -> Section {
    let mut rows = vec![];
    for (c, v) in [
        (&d.base.baseline, &p.baseline),
        (&d.base.proposed, &p.proposed),
    ] {
        for r in v
            .comfort
            .iter()
            .chain(std::iter::once(&v.building_comfort_union))
        {
            rows.push(vec![
                c.label.clone(),
                r.zone_id
                    .clone()
                    .unwrap_or("Building interval union".into()),
                old::cell(&r.occupied_hours),
                old::cell(&r.cooling_unmet_hours),
                old::cell(&r.unmet_percent),
            ]);
        }
    }
    Section::table(
        "Occupied cooling comfort",
        &["Case", "Zone / scope", "Occupied", "Unmet", "Unmet %"],
        rows,
    )
}
fn differences(d: &EnergyReportWorkbenchData) -> Result<Section> {
    let t = &d.typed_comparison;
    let mut s = Section::table(
        "Typed input differences",
        &[
            "Object / field",
            "Category / service",
            "Baseline → proposed",
        ],
        t.differences
            .iter()
            .map(|r| {
                Ok(vec![
                    format!(
                        "{} / {}\n{}",
                        r.object.zone_id.as_deref().unwrap_or("building"),
                        r.object.id,
                        r.field_path
                    ),
                    format!("{} / {}", ser(&r.category)?, ser(&r.service_effect)?),
                    format!(
                        "{} → {} {}",
                        input_value(&r.before),
                        input_value(&r.after),
                        r.unit.as_deref().unwrap_or("")
                    ),
                ])
            })
            .collect::<Result<_>>()?,
    );
    s.paragraphs.push(format!("Policy {}. {} changes; {} detailed rows. Truncated: {}. The retained JSON and complete input appendix remain authoritative.",t.policy_id,t.total_changes,t.differences.len(),t.truncated));
    Ok(s)
}
fn disclosures(d: &EnergyReportWorkbenchData) -> Section {
    Section::prose(
        "Comparison and service scope",
        d.service_disclosures()
            .into_iter()
            .chain(d.base.limitations.clone())
            .collect(),
    )
}
fn make_document(d: &EnergyReportWorkbenchData, kind: &str, sections: Vec<Section>) -> Document {
    Document {
        schema_version: "greenlens.energy-document.v0.5",
        title: d.metadata.title.clone(),
        report_id: d.base.id.clone(),
        revision: d.metadata.revision.clone(),
        kind: kind.into(),
        scope: d.base.scope.clone(),
        dataset_sha256: d.derived_sha256.clone(),
        sections,
    }
}
fn executive(d: &EnergyReportWorkbenchData, p: &EnergyReportPresentation) -> Result<Document> {
    let t = &d.typed_comparison;
    let mut groups = BTreeMap::<String, usize>::new();
    for r in &t.differences {
        *groups.entry(ser(&r.category)?).or_default() += 1;
    }
    let mut sections=vec![issue(d),findings(p),Section::prose("Interpretation",vec![format!("Signed baseline minus proposed electricity: {} ({}). Difference between each case's own peak: {} ({}). These are separate own-peak timestamps, not a simultaneous demand reduction.",old::cell(&p.facility_energy_savings.absolute),old::cell(&p.facility_energy_savings.percent),old::cell(&p.facility_peak_savings.absolute),old::cell(&p.facility_peak_savings.percent))]),
        Section::figure("Annual electricity by end use","annual.svg","Annual electricity in kWh. Exact full-precision values are retained in the performance table; unsupported systems are not measured zero contributions.".into()),Section::prose("Design and service",vec![format!("{} typed changes; comparable geometry/weather/calendar: {}; unchanged service inputs: {}. Complete before/after values are in workbench-report.json and the selected technical profile. Detailed-row truncation: {}.",t.total_changes,t.comparable_geometry_weather_calendar,t.unchanged_service_inputs,t.truncated)]),
        Section::table("Change categories",&["Category","Retained detailed rows"],groups.into_iter().map(|(k,v)|vec![k,v.to_string()]).collect()),status(d)?,disclosures(d),
        Section::prose("Comfort and provenance",vec!["The executive profile targets 2–4 pages for the reference example; the technical profile and separate input appendix retain full inputs, sources, diagnostics and revision history. No reviewer, approval or certification is inferred.".into()]),comfort(d,p),
        Section::table("Source snapshot",&["Case","Weather / engine / selected evidence"],[&d.base.baseline,&d.base.proposed].into_iter().map(|c|vec![c.label.clone(),format!("{}; synthetic:{}\nProfile:{}\n{}\n{}",c.weather_summary.source.citation,c.weather_summary.source.synthetic,c.weather_summary.binding.profile_id,c.method.method_id,c.totals.comfort_definition)]).collect()),
        Section::prose("Attachment selection",vec![format!("{} included; {} explicitly excluded ({} unavailable/invalid, {} model-inapplicable). Excluded private metadata is absent from this public report.",d.attachments.included_ids.len(),d.attachments.excluded_count,d.attachments.missing_unselected_count,d.attachments.inapplicable_unselected_count)])];
    // Include complete explicit carbon source/region/year/synthetic status.
    for c in [&d.base.baseline, &d.base.proposed] {
        sections.push(Section::prose(
            &format!("{} carbon factor", c.label),
            vec![carbon_text(&c.carbon_factor)],
        ));
    }
    Ok(make_document(d, "Executive", sections))
}
fn technical(
    d: &EnergyReportWorkbenchData,
    p: &EnergyReportPresentation,
    generated: &mut Vec<(String, Vec<u8>)>,
) -> Result<Document> {
    let mut sections = vec![
        issue(d),
        status(d)?,
        disclosures(d),
        differences(d)?,
        findings(p),
        comfort(d, p),
    ];
    sections.push(Section::table(
        "Revision history",
        &["Revision", "Issue date", "Description"],
        d.metadata
            .revision_history
            .iter()
            .map(|r| {
                vec![
                    r.revision.clone(),
                    r.issue_date.clone(),
                    r.description.clone(),
                ]
            })
            .collect(),
    ));
    for (side, c) in [
        ("baseline", &d.base.baseline),
        ("proposed", &d.base.proposed),
    ] {
        let v = validate_thermal_model(&c.model, &c.study.calendar)?;
        sections.push(Section::figure(&format!("{} geometry",c.label),&format!("{side}-model.svg"),"Retained input illustration in metres; visibility is not simulation inclusion. See the separate complete input appendix for exact face/window/shade dimensions.".into()));
        sections.push(Section::table(
            &format!("{} zones", c.label),
            &[
                "Zone / storey",
                "Origin / dimensions (m)",
                "Area (m²)",
                "Conditioning",
            ],
            c.model
                .zones
                .iter()
                .map(|z| {
                    vec![
                        format!("{} / {}", z.name, z.storey),
                        format!("{:?} / {:?}", z.origin_m, z.size_m),
                        format!("{}", z.size_m[0] * z.size_m[1]),
                        if z.cooling_system.is_some() {
                            "Explicit DX"
                        } else {
                            "Unconditioned"
                        }
                        .into(),
                    ]
                })
                .collect(),
        ));
        sections.push(Section::table(
            &format!("{} envelope assemblies", c.label),
            &[
                "Construction",
                "Outside → inside layers",
                "Display U (W/m²K) / films",
            ],
            c.model
                .constructions
                .iter()
                .map(|a| {
                    vec![
                        format!("{} ({})", a.name, a.id),
                        a.layers
                            .iter()
                            .enumerate()
                            .map(|(i, id)| {
                                let m = c
                                    .model
                                    .materials
                                    .iter()
                                    .find(|m| &m.id == id)
                                    .expect("validated construction material");
                                format!("{}. {} ({}) — {} m", i + 1, m.name, m.id, m.thickness_m)
                            })
                            .collect::<Vec<_>>()
                            .join("\n"),
                        format!(
                            "{} / inside {}, outside {} m²K/W",
                            v.construction_u_values_w_m2k[&a.id],
                            a.derived_inside_film_m2k_w,
                            a.derived_outside_film_m2k_w
                        ),
                    ]
                })
                .collect(),
        ));
        sections.push(Section::table(
            &format!("{} material layers", c.label),
            &[
                "Material",
                "Thickness (m) / conductivity (W/mK)",
                "Density (kg/m³) / heat capacity (J/kgK)",
                "Source",
            ],
            c.model
                .materials
                .iter()
                .map(|m| {
                    vec![
                        format!("{} ({})", m.name, m.id),
                        format!("{} / {}", m.thickness_m, m.conductivity_w_mk),
                        format!("{} / {}", m.density_kg_m3, m.specific_heat_j_kgk),
                        source_text(&m.source),
                    ]
                })
                .collect(),
        ));
        sections.push(Section::table(
            &format!("{} glazing", c.label),
            &[
                "Assembly / basis",
                "U (W/m²K)",
                "SHGC / visible transmittance",
                "Source",
            ],
            c.model
                .glazing
                .iter()
                .map(|g| {
                    Ok(vec![
                        format!("{} / {}", g.name, ser(&g.basis)?),
                        g.u_value_w_m2k.to_string(),
                        format!("{} / {}", g.shgc, g.visible_transmittance),
                        source_text(&g.source),
                    ])
                })
                .collect::<Result<_>>()?,
        ));
        let mut hvac = vec![];
        let mut fans = vec![];
        let mut gains = vec![];
        let mut gain_fractions = vec![];
        let mut assignments = vec![];
        for z in &c.model.zones {
            if let Some(h) = &z.cooling_system {
                hvac.push(vec![
                    z.name.clone(),
                    format!(
                        "{} W / COP {} / SHR {}",
                        h.gross_rated_capacity_w, h.gross_rated_cop, h.rated_shr
                    ),
                    format!(
                        "{} m³/s; {} C WB / {} C DB",
                        h.rated_airflow_m3_s, h.rated_indoor_wet_bulb_c, h.rated_outdoor_dry_bulb_c
                    ),
                    format!(
                        "availability:{}; setpoint:{}; cutout:{} K",
                        h.availability_schedule, h.cooling_setpoint_schedule, h.cooling_deadband_k
                    ),
                    source_text(&h.sizing_source),
                ]);
                assignments.push(vec![
                    z.name.clone(),
                    format!(
                        "Capacity temperature: {}\nEnergy-input-ratio temperature: {}",
                        h.capacity_temperature_curve, h.eir_temperature_curve
                    ),
                    format!(
                        "Capacity flow: {}\nEnergy-input-ratio flow: {}\nPart-load fraction: {}",
                        h.capacity_flow_curve, h.eir_flow_curve, h.part_load_fraction_curve
                    ),
                ]);
                let f = &h.supply_fan;
                let b = &h.balanced_ventilation;
                fans.push(vec![
                    z.name.clone(),
                    format!(
                        "{} Pa / efficiency {}; motor {} / in-air {}",
                        f.pressure_rise_pa,
                        f.total_efficiency,
                        f.motor_efficiency,
                        f.motor_in_air_fraction
                    ),
                    format!(
                        "{} m³/s; {} Pa / efficiency {}; schedule:{}",
                        b.design_flow_m3_s,
                        b.fan_pressure_pa,
                        b.total_efficiency,
                        b.fraction_schedule
                    ),
                ]);
            } else {
                hvac.push(vec![
                    z.name.clone(),
                    "No cooling system".into(),
                    "Not applicable".into(),
                    "Not applicable".into(),
                    "Explicitly unconditioned".into(),
                ]);
            }
            let g = &z.gains;
            gains.push(vec![
                z.name.clone(),
                format!(
                    "{} people/m²; {} / {}",
                    g.people_per_floor_m2, g.occupancy_schedule, g.activity_schedule
                ),
                format!(
                    "{} W/m² / {}",
                    g.lighting_w_per_floor_m2, g.lighting_schedule
                ),
                format!(
                    "{} W/m² / {}",
                    g.equipment_w_per_floor_m2, g.equipment_schedule
                ),
                format!(
                    "{}\nSchedule: {}",
                    infiltration_text(&g.infiltration),
                    g.infiltration_schedule
                ),
            ]);
            gain_fractions.push(vec![
                z.name.clone(),
                format!(
                    "Sensible: {}\nRadiant share of sensible: {}",
                    g.people_sensible_fraction, g.people_radiant_fraction_of_sensible
                ),
                format!(
                    "Radiant: {}\nVisible: {}\nReturn air: {}",
                    g.lighting_radiant_fraction,
                    g.lighting_visible_fraction,
                    g.lighting_return_air_fraction
                ),
                format!(
                    "Radiant: {}\nLatent: {}\nLost: {}",
                    g.equipment_radiant_fraction,
                    g.equipment_latent_fraction,
                    g.equipment_lost_fraction
                ),
                source_text(&g.source),
            ]);
        }
        sections.push(Section::table(
            &format!("{} cooling systems", c.label),
            &[
                "Zone",
                "Gross coil rating",
                "Airflow / rating temperatures",
                "Control schedules",
                "Sizing source",
            ],
            hvac,
        ));
        sections.push(Section::table(
            &format!("{} coil curve assignments", c.label),
            &[
                "Zone",
                "Temperature modifiers",
                "Flow / part-load modifiers",
            ],
            assignments,
        ));
        sections.push(Section::prose(&format!("{} curve interpretation", c.label), vec!["Temperature modifier x is indoor wet-bulb temperature (°C), y is outdoor dry-bulb temperature (°C). Flow modifier x is actual/rated airflow (dimensionless). Part-load-fraction x is part-load ratio (dimensionless). Declared curve bounds and sources do not establish applicability to all operating conditions.".into()]));
        sections.push(Section::table(
            &format!("{} fans and outdoor air", c.label),
            &["Zone", "Supply fan", "Balanced intake/exhaust"],
            fans,
        ));
        sections.push(Section::table(
            &format!("{} gain fractions and sources", c.label),
            &["Zone", "People", "Lighting", "Equipment", "Source"],
            gain_fractions,
        ));
        sections.push(Section::table(
            &format!("{} people, gains and infiltration", c.label),
            &[
                "Zone",
                "People / schedule / activity",
                "Lighting",
                "Equipment",
                "Infiltration",
            ],
            gains,
        ));
        sections.push(Section::table(
            &format!("{} performance curves", c.label),
            &[
                "Curve",
                "Polynomial and input domain",
                "Output limits",
                "Source",
            ],
            c.model
                .curves
                .iter()
                .map(|q| {
                    Ok(vec![
                        q.id.clone(),
                        curve_equation(&q.form),
                        format!("{} to {}", q.output_limits[0], q.output_limits[1]),
                        source_text(&q.source),
                    ])
                })
                .collect::<Result<_>>()?,
        ));
        for (index, s) in c.model.schedules.iter().enumerate() {
            let name = format!("schedule-{side}-{index:03}.svg");
            generated.push((name.clone(), schedule_svg(s).into_bytes()));
            sections.push(Section::figure(&format!("{} schedule {}",c.label,s.id),&name,format!("{}; 144 piecewise-constant ten-minute slots per day, no interpolation. Weekday/Saturday/Sunday shown; {} dated holiday overrides are retained losslessly in inputs.csv and the separate input appendix.",schedule_unit(s.kind),s.holidays.len())));
            sections.push(schedule_table(&c.label, s));
        }
        sections.extend(provenance(c)?);
    }
    sections.push(Section::figure("Annual electrical end uses","annual.svg","Full-precision electrical ledger. Missing or unsupported systems are not zero-valued measured contributions.".into()));
    sections.push(Section::figure("Monthly electricity","monthly.svg","Twelve complete local-standard months; kWh. Detailed numeric values remain in performance.csv/JSON.".into()));
    Ok(make_document(d, "Technical", sections))
}
fn schedule_unit(k: ScheduleKind) -> &'static str {
    match k {
        ScheduleKind::Fraction => "fraction (1)",
        ScheduleKind::ActivityWattsPerPerson => "activity (W/person)",
        ScheduleKind::TemperatureCelsius => "temperature (C)",
    }
}
fn schedule_svg(s: &Schedule) -> String {
    let max = s
        .weekday
        .values
        .iter()
        .chain(&s.saturday.values)
        .chain(&s.sunday.values)
        .copied()
        .fold(0f64, f64::max)
        .max(1.);
    let min = s
        .weekday
        .values
        .iter()
        .chain(&s.saturday.values)
        .chain(&s.sunday.values)
        .copied()
        .fold(0f64, f64::min);
    let y = |v: f64| 190. - 150. * (v - min) / (max - min);
    let mut out=format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 760 240\" role=\"img\"><rect width=\"760\" height=\"240\" fill=\"white\"/><g font-family=\"sans-serif\" font-size=\"12\" fill=\"#213834\"><text x=\"55\" y=\"18\">{} — {}</text><text x=\"5\" y=\"44\">{:.2}</text><text x=\"5\" y=\"192\">{:.2}</text></g><path d=\"M55 40V190H725\" fill=\"none\" stroke=\"#657b75\"/>",old::escaped(&s.id),schedule_unit(s.kind),max,min);
    for h in [0, 6, 12, 18, 24] {
        out.push_str(&format!(
            "<text x=\"{}\" y=\"209\" font-size=\"11\">{:02}:00</text>",
            55. + 670. * f64::from(h) / 24.,
            h
        ));
    }
    for (v, color, label, i) in [
        (&s.weekday.values, "#246954", "Weekday", 0),
        (&s.saturday.values, "#2d67a1", "Saturday", 1),
        (&s.sunday.values, "#a35d2d", "Sunday", 2),
    ] {
        let mut points = String::new();
        for (n, value) in v.iter().enumerate() {
            let x = 55. + 670. * n as f64 / 144.;
            let x2 = 55. + 670. * (n + 1) as f64 / 144.;
            points.push_str(&format!(
                "{x:.4},{:.4} {x2:.4},{:.4} ",
                y(*value),
                y(*value)
            ));
        }
        out.push_str(&format!("<polyline points=\"{points}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"2\"/><text x=\"{}\" y=\"232\" font-family=\"sans-serif\" font-size=\"12\" fill=\"{color}\">{label}</text>",90+i*210));
    }
    out.push_str("</svg>");
    out
}

pub fn render(
    data: &EnergyReportWorkbenchData,
    destination: &Path,
    typst: Option<&TypstConfig>,
    cancel: &AtomicBool,
) -> Result<EnergyReportArtifacts> {
    render_with_architecture(data, destination, typst, cancel, None)
}
pub fn render_with_architecture(
    data: &EnergyReportWorkbenchData,
    destination: &Path,
    typst: Option<&TypstConfig>,
    cancel: &AtomicBool,
    architecture: Option<&old::architectural::ArchitecturalReport>,
) -> Result<EnergyReportArtifacts> {
    data.validate()?;
    ensure!(
        data.template_sha256 == template_sha256(),
        "Workbench template differs from derived identity"
    );
    ensure!(
        !cancel.load(Ordering::SeqCst),
        "Report cancelled before writing"
    );
    let p = data.base.presentation()?;
    let mut generated = vec![
        (
            "workbench-report.json".into(),
            serde_json::to_vec_pretty(data)?,
        ),
        (
            "energy-report.json".into(),
            serde_json::to_vec_pretty(&data.base)?,
        ),
        (
            "typed-differences.json".into(),
            serde_json::to_vec_pretty(&data.typed_comparison)?,
        ),
        ("performance.json".into(), serde_json::to_vec_pretty(&p)?),
        ("performance.csv".into(), old::csv(&p)?.into_bytes()),
        (
            "inputs.csv".into(),
            old::inputs_csv(&data.base)?.into_bytes(),
        ),
        (
            "baseline-model.svg".into(),
            old::model_svg(&data.base.baseline).into_bytes(),
        ),
        (
            "proposed-model.svg".into(),
            old::model_svg(&data.base.proposed).into_bytes(),
        ),
        ("annual.svg".into(), old::chart(&p, false).into_bytes()),
        ("monthly.svg".into(), old::chart(&p, true).into_bytes()),
    ];
    let mut docs = vec![];
    let mut names = vec![];
    for profile in &data.profiles {
        match profile {
            EnergyReportProfile::Executive => {
                docs.push(executive(data, &p)?);
                names.push("executive");
            }
            EnergyReportProfile::Technical => {
                docs.push(technical(data, &p, &mut generated)?);
                names.push("technical");
            }
        }
    }
    let mut legacy = old::documents(&data.base, &p, &data.base_dataset_sha256)?;
    let mut appendix = legacy.remove(1);
    appendix.schema_version = "greenlens.energy-document.v0.5";
    appendix.title = data.metadata.title.clone();
    appendix.revision = data.metadata.revision.clone();
    appendix.dataset_sha256 = data.derived_sha256.clone();
    docs.push(appendix);
    names.push("input-appendix");
    let mut performance = legacy.remove(1);
    performance.sections.retain(|s| {
        !matches!(
            s.title.as_str(),
            "Comparison scope and service"
                | "Baseline/proposed input differences"
                | "Reported findings"
        )
    });
    performance.sections.push(disclosures(data));
    performance.sections.push(differences(data)?);
    // Always retain the complete numeric monthly/comfort tables, even when
    // the executive profile is selected alone. Reuse validated cell text.
    performance
        .sections
        .extend(legacy.remove(0).sections.into_iter().filter(|s| {
            s.title.ends_with("monthly ledger")
                || s.title.ends_with("occupied cooling comfort")
                || s.title == "Comfort convention"
        }));
    performance.schema_version = "greenlens.energy-document.v0.5";
    performance.title = data.metadata.title.clone();
    performance.revision = data.metadata.revision.clone();
    performance.dataset_sha256 = data.derived_sha256.clone();
    docs.push(performance);
    names.push("performance");
    let architectural_template = architecture.map(|_| old::architectural::template(TEMPLATE));
    if let Some(scope) = architecture {
        scope.validate_binding(&data.base, Some(&data.derived_sha256))?;
        generated.extend(scope.products()?);
        for (name, doc) in names.iter().zip(&mut docs) {
            let mut sections = if *name == "input-appendix" {
                scope.detail_sections()?
            } else {
                vec![scope.summary()?]
            };
            if *name == "technical" {
                sections.push(Section::figure(
                    "Architectural reference — plan",
                    "reference-plan.svg",
                    scope.data.figures[0].caption.clone(),
                ));
                sections.push(Section::figure("Selected retained analytical abstraction", "baseline-model.svg", "Baseline retained thermal geometry; scope reconciliation remains separate from the source architecture.".into()));
            }
            sections.append(&mut doc.sections);
            doc.sections = sections;
            old::architectural::clarify_sections(&mut doc.sections);
            doc.dataset_sha256 = scope.data.report_sha256.clone();
        }
    }
    let file_names = generated
        .iter()
        .map(|(n, _)| n.clone())
        .collect::<Vec<String>>();
    let refs = file_names
        .iter()
        .map(String::as_str)
        .zip(generated.into_iter().map(|(_, b)| b))
        .collect::<Vec<_>>();
    old::create_fresh(destination)?;
    let mut result = EnergyReportArtifacts {
        schema_version: if architecture.is_some() {
            "greenlens.energy-report-artifacts.v0.6"
        } else {
            "greenlens.energy-report-artifacts.v0.5"
        }
        .into(),
        dataset_sha256: architecture
            .map(|a| a.data.report_sha256.clone())
            .unwrap_or_else(|| data.derived_sha256.clone()),
        renderer_status: "not_requested".into(),
        renderer_message: "Validated workbench HTML/CSV/JSON/SVG written; PDF/PNG not requested."
            .into(),
        typst_executable_sha256: None,
        template_sha256: architectural_template
            .as_ref()
            .map(|t| old::sha(t.as_bytes()))
            .unwrap_or_else(template_sha256),
        renderer_work_id: None,
        renderer_inputs: BTreeMap::new(),
        artifacts: vec![],
        processes: vec![],
    };
    for (name, bytes) in &refs {
        old::write_new(&destination.join(name), bytes)?;
        result.artifacts.push(old::artifact(destination, name)?);
    }
    for (name, d) in names.iter().zip(&docs) {
        let file = format!("{name}.html");
        old::write_new(&destination.join(&file), old::html(d, &refs)?.as_bytes())?;
        result.artifacts.push(old::artifact(destination, &file)?);
    }
    // Existing assessment filename remains an explicitly identified alias to
    // the selected primary new profile, never a legacy semantic projection.
    old::write_new(
        &destination.join("assessment.html"),
        old::html(&docs[0], &refs)?.as_bytes(),
    )?;
    result
        .artifacts
        .push(old::artifact(destination, "assessment.html")?);
    old::check_budget(destination)?;
    if let Some(config) = typst {
        let mut stages = names
            .iter()
            .enumerate()
            .map(|(i, n)| (*n, i, false))
            .collect::<Vec<_>>();
        stages.push(("performance-pages", docs.len() - 1, true));
        let outcome = old::render_native_documents(
            old::NativeDocuments {
                docs: &docs,
                generated: &refs,
                template: architectural_template.as_deref().unwrap_or(TEMPLATE),
                stages: &stages,
            },
            destination,
            config,
            cancel,
            &mut result,
        );
        if let Err(error) = outcome {
            result.renderer_status = if cancel.load(Ordering::SeqCst) {
                "cancelled"
            } else {
                "failed"
            }
            .into();
            result.renderer_message="Native report rendering incomplete; retained outputs/receipts are diagnostic and do not establish qualification.".into();
            old::write_new(
                &destination.join("report-artifacts.json"),
                &serde_json::to_vec_pretty(&result)?,
            )?;
            return Err(error);
        }
        let primary = format!("{}.pdf", names[0]);
        let bytes = fs::read(destination.join(primary))?;
        old::write_new(&destination.join("assessment.pdf"), &bytes)?;
        result
            .artifacts
            .push(old::artifact(destination, "assessment.pdf")?);
        result.renderer_status = "completed".into();
        result.renderer_message="All requested workbench PDFs and complete performance PNG pages rendered using pinned offline dependencies. Page count and visual acceptance require separate inspection.".into();
    }
    old::write_new(
        &destination.join("report-artifacts.json"),
        &serde_json::to_vec_pretty(&result)?,
    )?;
    old::check_budget(destination)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> EnergyReportWorkbenchData {
        let base = synthetic_energy_report().unwrap();
        let items = base
            .evidence
            .iter()
            .map(|e| EnergyAttachmentDecision {
                id: e.id.clone(),
                selected: e.included,
                disposition: if e.included {
                    EnergyAttachmentDisposition::Included
                } else {
                    EnergyAttachmentDisposition::Excluded
                },
                reason: "Synthetic pure fixture".into(),
            })
            .collect();
        prepare_data(
            base,
            EnergyReportMetadata {
                title: "Original <script> #include — 能源".into(),
                ..Default::default()
            },
            vec![
                EnergyReportProfile::Executive,
                EnergyReportProfile::Technical,
            ],
            &EnergyAttachmentPreflight {
                schema_version: "greenlens.energy-attachment-preflight.v1".into(),
                ready: true,
                items,
            },
        )
        .unwrap()
    }
    #[test]
    fn new_profiles_escape_metadata_keep_exact_data_and_correct_semantics() {
        let data = fixture();
        let root = crate::local_root()
            .unwrap()
            .join("verification")
            .join(format!("report-workbench-{}", uuid::Uuid::new_v4()));
        let r = render(&data, &root, None, &AtomicBool::new(false)).unwrap();
        assert_eq!(r.renderer_status, "not_requested");
        let html = fs::read_to_string(root.join("executive.html")).unwrap();
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
        assert!(html.contains("lower-temperature"));
        let saved: EnergyReportWorkbenchData =
            serde_json::from_slice(&fs::read(root.join("workbench-report.json")).unwrap()).unwrap();
        assert_eq!(saved, data);
        let technical = fs::read_to_string(root.join("technical.html")).unwrap();
        for title in [
            "envelope assemblies",
            "material layers",
            "glazing",
            "cooling systems",
            "fans and outdoor air",
            "people, gains and infiltration",
            "performance curves",
            "schedule",
        ] {
            assert!(technical.contains(title));
        }
        assert!(root.join("inputs.csv").is_file());
        assert!(root.join("performance.csv").is_file());
        assert!(root.join("input-appendix.html").is_file());
        assert!(!root.join("executive.pdf").exists());
        let before = data.base.dataset_sha256().unwrap();
        assert_eq!(before, data.base_dataset_sha256);
    }
    #[test]
    fn missing_selected_attachment_blocks_instead_of_filtering() {
        let item = EnergyAttachment {
            id: uuid::Uuid::new_v4().to_string(),
            path: String::new(),
            sha256: "a".repeat(64),
            media_type: "image/png".into(),
            model_input_sha256: "b".repeat(64),
            caption: "Private caption".into(),
            rights: "Original".into(),
            include_in_export: true,
        };
        let p = attachment_preflight(Path::new("."), &[item], [&"b".repeat(64), &"c".repeat(64)])
            .unwrap();
        assert!(!p.ready);
        assert!(p.summary().is_err());
        let mut d = fixture();
        d.template_sha256 = "f".repeat(64);
        assert!(d.validate().is_err());
    }
    #[test]
    fn technical_rows_reconcile_typed_values_sources_and_exact_machine_outputs() {
        let data = fixture();
        let before = serde_json::to_vec(&data).unwrap();
        let p = data.base.presentation().unwrap();
        let doc = technical(&data, &p, &mut vec![]).unwrap();
        let diff = doc
            .sections
            .iter()
            .find(|s| s.title == "Typed input differences")
            .unwrap();
        assert_eq!(diff.rows.len(), data.typed_comparison.differences.len());
        for (row, value) in diff.rows.iter().zip(&data.typed_comparison.differences) {
            assert_eq!(
                row[2],
                format!(
                    "{} → {} {}",
                    input_value(&value.before),
                    input_value(&value.after),
                    value.unit.as_deref().unwrap_or("")
                )
            );
        }
        for c in [&data.base.baseline, &data.base.proposed] {
            let rows = &doc
                .sections
                .iter()
                .find(|s| s.title == format!("{} performance curves", c.label))
                .unwrap()
                .rows;
            for (row, curve) in rows.iter().zip(&c.model.curves) {
                assert_eq!(row[0], curve.id);
                assert_eq!(row[1], curve_equation(&curve.form));
                assert_eq!(
                    row[2],
                    format!("{} to {}", curve.output_limits[0], curve.output_limits[1])
                );
                assert!(row[3].contains(&curve.source.citation));
            }
            let text = serde_json::to_string(&provenance(c).unwrap()).unwrap();
            for hash in [
                &c.weather_summary.binding.original_sha256,
                &c.weather_summary.binding.normalized_sha256,
                &c.weather_summary.binding.time_axis_sha256,
                &c.record_sha256,
                &c.method.recipe_sha256,
                &c.diagnostics.source_record_sha256,
            ] {
                assert!(text.contains(hash));
            }
            for entry in &c.meter_ledger.entries {
                for channel in &entry.native_channels {
                    assert!(text.contains(channel));
                }
            }
            for schedule in &c.model.schedules {
                let s = schedule_table(&c.label, schedule);
                assert_eq!(s.rows.len(), 3 + schedule.holidays.len());
                assert_eq!(s.rows[0][1], profile_intervals(&schedule.weekday));
                for (row, h) in s.rows[3..].iter().zip(&schedule.holidays) {
                    assert_eq!(
                        row[0],
                        format!("Holiday {:02}-{:02}", h.date.month, h.date.day)
                    );
                    assert_eq!(row[1], profile_intervals(&h.profile));
                }
            }
        }
        assert_eq!(serde_json::to_vec(&data).unwrap(), before);
        let root = crate::local_root()
            .unwrap()
            .join("verification")
            .join(format!("report-readable-{}", uuid::Uuid::new_v4()));
        render(&data, &root, None, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            fs::read(root.join("performance.json")).unwrap(),
            serde_json::to_vec_pretty(&p).unwrap()
        );
        assert_eq!(
            fs::read_to_string(root.join("performance.csv")).unwrap(),
            old::csv(&p).unwrap()
        );
        assert_eq!(
            fs::read_to_string(root.join("inputs.csv")).unwrap(),
            old::inputs_csv(&data.base).unwrap()
        );
        let html = fs::read_to_string(root.join("technical.html")).unwrap();
        assert!(!html.contains("&quot;kind&quot;"));
        assert!(!html.contains("&quot;coefficients&quot;"));
        assert!(!html.contains("&quot;source_record_sha256&quot;"));
    }
    #[test]
    fn readable_equations_intervals_and_units_preserve_signed_values() {
        assert_eq!(input_value(&ThermalInputValue::Number(-0.125)), "-0.125");
        assert_eq!(
            input_value(&ThermalInputValue::Text("<literal>_id".into())),
            "<literal>_id"
        );
        assert_eq!(
            infiltration_text(&InfiltrationRate::FlowM3PerSecondPerFloorM2 { value: 0.002 }),
            "0.002 m³/(s·m²) of floor area"
        );
        assert_eq!(
            curve_equation(&CurveForm::Quadratic {
                coefficients: [1., -2., 3.],
                x: [-1., 4.]
            }),
            "f(x) = (1) + (-2)·x + (3)·x²\nx: -1 to 4"
        );
        assert_eq!(
            curve_equation(&CurveForm::Cubic {
                coefficients: [1., -2., 3., -4.],
                x: [0., 1.]
            }),
            "f(x) = (1) + (-2)·x + (3)·x² + (-4)·x³\nx: 0 to 1"
        );
        assert_eq!(
            curve_equation(&CurveForm::Biquadratic {
                coefficients: [1., 2., 3., 4., 5., 6.],
                x: [10., 30.],
                y: [15., 50.]
            }),
            "f(x,y) = (1) + (2)·x + (3)·x² + (4)·y + (5)·y² + (6)·x·y\nx: 10 to 30; y: 15 to 50"
        );
        let mut profile = DayProfile::constant(-0.5);
        profile.values[1..143].fill(1.25);
        assert_eq!(
            profile_intervals(&profile),
            "00:00–00:10: -0.5\n00:10–23:50: 1.25\n23:50–24:00: -0.5"
        );
        for value in [
            ThermalInputValue::Missing,
            ThermalInputValue::Null,
            ThermalInputValue::Boolean(true),
            ThermalInputValue::Boolean(false),
            ThermalInputValue::ObjectPresent,
        ] {
            assert!(!input_value(&value).contains('{'));
        }
    }
}
