//! Selected portable evidence and streaming climate replay. Imported command
//! text is never executed; replay invokes only the compiled supported adapter.
use crate::{
    climate::{self, ClimateRunRecord},
    persistence::*,
    timeseries::SeriesReader,
    weather, Project, ProjectView,
};
use anyhow::{ensure, Context, Result};
use greenlens_core::{
    canonical_hash, rgb_to_lux, statistics,
    study::{MetricSettings, Occupancy, StudyMode},
    weather::WeatherData,
};
use greenlens_radiance::{annual, Engine};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashSet},
    fmt::Write as _,
    fs::{self, File},
    io::{BufReader, Read},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;

fn reduced_project(mut project: Project, selected: &[String]) -> Result<Project> {
    crate::validate_project(&project)?;
    ensure!(
        !selected.is_empty(),
        "Select at least one completed run to export"
    );
    let ids: HashSet<_> = selected.iter().map(String::as_str).collect();
    ensure!(
        ids.len() == selected.len(),
        "Duplicate selected run identifiers"
    );
    let known: HashSet<_> = project
        .runs
        .iter()
        .map(|r| r.id.as_str())
        .chain(
            project
                .climate
                .iter()
                .flat_map(|c| c.runs.iter().map(|r| r.id.as_str())),
        )
        .collect();
    ensure!(
        ids.iter().all(|id| known.contains(id)),
        "Selected export includes an unknown run"
    );
    project.runs.retain(|r| ids.contains(r.id.as_str()));
    if let Some(extension) = &mut project.climate {
        extension.runs.retain(|r| ids.contains(r.id.as_str()));
    }
    Ok(project)
}

fn collect_members(
    root: &Path,
    prefix: &str,
    budget: &mut PackageBudget,
    files: &mut Vec<String>,
) -> Result<()> {
    let seal_name = format!("{prefix}/checksums.json");
    let seal_file = regular_child(root, &seal_name)?;
    budget.add(&seal_name, fs::metadata(&seal_file)?.len())?;
    files.push(seal_name);
    let sums: Checksums = read_json(&seal_file)?;
    ensure!(sums.schema_version == "0.1.0", "Unsupported evidence seal");
    for name in sums.sha256.keys() {
        let path = format!("{prefix}/{name}");
        budget.add(&path, fs::metadata(regular_child(root, &path)?)?.len())?;
        files.push(path);
    }
    Ok(())
}

/// IDs explicitly select any mixture of static and climate history. The current
/// models/studies and retained project weather stay in the portable document.
pub fn export_selected(path: &Path, destination: &Path, ids: &[String]) -> Result<()> {
    let _lock = WriterLock::acquire(path)?;
    let project = reduced_project(read_json(&regular_child(path, "project.json")?)?, ids)?;
    let mut budget = PackageBudget::default();
    budget.add(
        "project.json",
        (serde_json::to_vec_pretty(&project)?.len() + 1) as u64,
    )?;
    let mut files = vec!["project.json".into()];
    for summary in &project.runs {
        collect_members(path, &summary.path, &mut budget, &mut files)?;
    }
    if let Some(extension) = &project.climate {
        for reference in &extension.weather {
            collect_members(path, &reference.path, &mut budget, &mut files)?;
        }
        for summary in &extension.runs {
            collect_members(path, &summary.path, &mut budget, &mut files)?;
        }
    }
    for generated in [
        "comparisons.json",
        "qualification.json",
        "report.html",
        "REPLAY.txt",
        "checksums.json",
    ] {
        budget.add(generated, 0)?;
    }
    if !project.runs.is_empty() {
        budget.add("static-report.html", 0)?;
    }
    // The source budget fails before copying outputs or building report arrays.
    let staging = crate::local_root()?
        .join("exports")
        .join(Uuid::new_v4().to_string());
    fs::create_dir_all(&staging)?;
    for summary in &project.runs {
        crate::read_run(path, summary)?;
        promote_run(&path.join(&summary.path), &staging, &summary.id)?;
    }
    if let Some(extension) = &project.climate {
        climate::copy_project_evidence(path, &staging, extension)?;
    }
    write_json(&staging.join("project.json"), &project)?;
    let view = crate::open_project(&staging)?;
    let static_comparisons = crate::latest_compatible_comparisons(&view.results)?;
    let mut climate_comparisons = comparison_pairs(&staging, &view.climate_results)?;
    for pair in &mut climate_comparisons {
        for (side, id_key) in [("a_assessment", "a_id"), ("b_assessment", "b_id")] {
            pair[side] = serde_json::to_value(
                &view.climate_assessments[pair[id_key].as_str().context("Missing pair ID")?],
            )?;
        }
    }
    write_json(
        &staging.join("qualification.json"),
        &json!({
            "schema_version":"greenlens.qualification-supplement.v1",
            "authority":"Compiled application registry; imported supplements are not trusted. Reopening or replaying recalculates assessments without changing immutable runs.",
            "assessments":view.climate_assessments
        }),
    )?;
    files.push("qualification.json".into());
    write_json(
        &staging.join("comparisons.json"),
        &json!({
            "schema_version":"greenlens.comparisons.v0.3",
            "selection_policy":"Most recently saved selected A/B in each compatible condition group; no Cartesian expansion",
            "static":static_comparisons,"climate":climate_comparisons
        }),
    )?;
    files.push("comparisons.json".into());
    if !view.results.is_empty() {
        fs::write(
            staging.join("static-report.html"),
            crate::report::render(&view, &static_comparisons)?,
        )?;
        files.push("static-report.html".into());
    }
    fs::write(
        staging.join("report.html"),
        render_report(&staging, &view, &climate_comparisons)?,
    )?;
    fs::write(staging.join("REPLAY.txt"), "GreenLens selected evidence\nInstall compatible pinned Radiance 6.0.2 separately. Engine binaries are excluded.\nRun: greenlens replay evidence.zip --engine-dir <engine-directory>\nStatic replay retains its original max(0.1 lx, 0.1% reference) tolerance.\nClimate replay regenerates disposable matrices/octrees from validated retained weather/model through the compiled adapter and checks every retained interval/sensor within max(0.5 lx, 0.5% reference).\nNo imported commands are executed. Weather attribution, original bytes and normalization remain inside each climate run and the project weather assets.\nOpen report.html offline. Annual report hourly previews explicitly identify their one retained interval; full time series remain in bounded binary evidence and can be inspected through GreenLens.\nReplay agreement is integration verification, not independent physical validation or certification.\n")?;
    files.extend(["report.html".into(), "REPLAY.txt".into()]);
    seal(
        &staging,
        &files.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    files.push("checksums.json".into());
    // Includes exact generated file sizes; successful exports retain the same
    // 64 MiB/256 MiB/5000-member constraints as extract_package.
    zip_directory(&staging, &files, destination)
}

fn preview_position(run: &ClimateRunRecord) -> usize {
    if run.series.interval_count == 1 {
        0
    } else {
        12
    }
}
fn preview_interval(run: &ClimateRunRecord) -> usize {
    match run.study.calculation {
        StudyMode::WeatherHour { interval_index, .. } => interval_index,
        _ => preview_position(run),
    }
}
fn metric_values(run: &ClimateRunRecord, field: usize) -> Vec<f64> {
    run.metrics
        .iter()
        .flatten()
        .map(|m| {
            [
                m.da_percent,
                m.cda_percent,
                m.udi_percent,
                m.underlit_percent,
                m.overlit_percent,
            ][field]
        })
        .collect()
}

/// Linear compatible groups, preserving explicit retained IDs in the report.
fn comparison_pairs(root: &Path, results: &[ClimateRunRecord]) -> Result<Vec<Value>> {
    type Pair<'a> = (Option<&'a ClimateRunRecord>, Option<&'a ClimateRunRecord>);
    let mut groups: BTreeMap<String, Pair<'_>> = BTreeMap::new();
    for run in results {
        let group = groups
            .entry(climate::comparison_fingerprint(run)?)
            .or_default();
        if run.alternative_id == "A" {
            group.0 = Some(run);
        } else {
            group.1 = Some(run);
        }
    }
    let mut comparisons = Vec::new();
    for (fingerprint, (a, b)) in groups {
        let Some((a, b)) = a.zip(b) else { continue };
        let ar = SeriesReader::open(&root.join("runs").join(&a.id).join("series"))?;
        let br = SeriesReader::open(&root.join("runs").join(&b.id).join("series"))?;
        let av = ar.hour(preview_position(a))?;
        let bv = br.hour(preview_position(b))?;
        ensure!(av.len() == bv.len(), "Comparison sensor shape differs");
        let deltas: Vec<_> = bv.iter().zip(&av).map(|(b, a)| b - a).collect();
        let mut metric_deltas = serde_json::Map::new();
        if a.metrics.is_some() {
            for (field, name) in ["DA", "cDA", "UDI", "underlit", "overlit"]
                .into_iter()
                .enumerate()
            {
                let values: Vec<_> = metric_values(b, field)
                    .iter()
                    .zip(metric_values(a, field))
                    .map(|(b, a)| b - a)
                    .collect();
                metric_deltas.insert(name.into(), json!({"per_sensor_percentage_points":values,"unweighted_sensor_statistics_percentage_points":statistics(&values)?}));
            }
        }
        comparisons.push(json!({"a_id":a.id,"b_id":b.id,"comparison_fingerprint":fingerprint,
            "delta_convention":"B minus A; percentage deltas omitted", "sensor_ids":a.series.sensor_ids,
            "report_preview_interval_index":preview_interval(a),"preview_deltas_lux":deltas,
            "preview_delta_statistics_lux":statistics(&deltas)?,"preview_shared_max_lux":av.iter().chain(&bv).copied().fold(0.0,f64::max),
            "annual_metric_deltas":metric_deltas,
            "design_differences":{"A":{"window":a.model.window,"visible_transmittance":a.model.materials.visible_transmittance,"overhang":a.model.overhang},
                "B":{"window":b.model.window,"visible_transmittance":b.model.materials.visible_transmittance,"overhang":b.model.overhang}}}));
    }
    Ok(comparisons)
}

struct Html(String);
impl std::fmt::Write for Html {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        if self.0.len().saturating_add(text.len()) as u64 > MAX_EVIDENCE_FILE_BYTES {
            return Err(std::fmt::Error);
        }
        self.0.push_str(text);
        Ok(())
    }
}
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn weather_policy(html: &mut Html, weather: &WeatherData) -> Result<()> {
    let policy = &weather.time_policy;
    let yes_no = |value| if value { "yes" } else { "no" };
    let declared_offset = policy.declared_irradiance_offset_hours.map_or_else(
        || "not declared by the source".into(),
        |hours| format!("{hours} h relative to the interval end"),
    );
    write!(html,
        "<h3>Retained weather interpretation</h3><p>Validated interpretation: {}. Source minute {} → canonical hour-ending minute {}; interval duration {} minutes. Representative instant: {} minutes relative to the interval end. Declared irradiation offset: {}.</p><p>Fixed local standard time: {}. Daylight-saving adjustment applied: {}. Source-year labels treated as a calendar: {}. These values come from the reparsed retained weather asset; no operating-system timezone conversion is applied.</p>",
        escape(&policy.interpretation), policy.source_minute, policy.canonical_minute,
        policy.interval_duration_minutes, policy.representative_offset_from_interval_end_minutes,
        escape(&declared_offset), yes_no(policy.fixed_local_standard_time),
        yes_no(policy.applies_daylight_saving), yes_no(policy.source_year_is_calendar),
    )?;
    write!(html,
        "<p>Radiation policy: retained DNI, DHI and GHI are interval accumulations in Wh/m². The mean irradiance in W/m² is the accumulation divided by the {} h interval duration; it is not a direct conversion to lux. Visible sky generation uses DNI, DHI and retained dew point; GHI is retained for checks. Missing required values are rejected, never substituted with night. The retained asset has {} intervals with both DNI and DHI equal to zero; occupied zero-light intervals remain in annual denominators.</p><details><summary>Complete validated time-normalization policy</summary><pre>{}</pre><p>Original weather SHA-256: {}<br>Normalized weather SHA-256: {}<br>Time-axis SHA-256: {}</p></details>",
        f64::from(policy.interval_duration_minutes) / 60.0,
        weather.summary.zero_solar_intervals,
        escape(&serde_json::to_string_pretty(policy)?),
        escape(&weather.original_sha256), escape(&weather.normalized_sha256),
        escape(&weather.time_axis_sha256),
    )?;
    Ok(())
}

fn metric_definitions(html: &mut Html, settings: &MetricSettings) -> Result<()> {
    let weights = settings.weights()?;
    let occupied = weights.iter().filter(|&&weight| weight > 0.0).count();
    let partial = weights
        .iter()
        .filter(|&&weight| weight > 0.0 && weight < 1.0)
        .count();
    // Compensated positive sum for presentation only; metric evaluation remains
    // in the authoritative core accumulator. Do not round tiny masks to zero.
    let (mut total, mut correction) = (0.0_f64, 0.0_f64);
    for weight in &weights {
        let corrected = weight - correction;
        let next = total + corrected;
        correction = (next - total) - corrected;
        total = next;
    }
    let denominator = if total < 0.000001 {
        format!("{total:e}")
    } else {
        total.to_string()
    };
    let schedule = match &settings.occupancy {
        Occupancy::Daily { start_minute, end_minute } => format!(
            "Daily {:02}:{:02}–{:02}:{:02} local standard time, every day; start inclusive and end exclusive{}. Partial hourly overlap supplies fractional weights.",
            start_minute / 60, start_minute % 60, end_minute / 60, end_minute % 60,
            if start_minute > end_minute { ", crossing midnight" } else { "" },
        ),
        Occupancy::Mask { .. } => "Explicit 8,760-interval occupancy-weight mask in the retained weather order; no weekday or calendar inference.".into(),
    };
    write!(html,
        "<h3>Annual metric definitions</h3><p>Occupancy: {} Occupied intervals (positive weight): {} of 8760; fractional-weight intervals: {}. Denominator W = Σ wₜ = {} weighted hourly intervals. Nighttime occupancy remains in the denominator.</p><p>For each sensor, Eₜ is illuminance in lux and wₜ is the retained occupancy weight. T = {} lx; L = {} lx; U = {} lx. The indicator 1[condition] equals one when its condition is true and zero otherwise.</p><ul><li>DA = 100 × Σ(wₜ × 1[Eₜ ≥ T]) / W.</li><li>cDA = 100 × Σ(wₜ × min(Eₜ / T, 1)) / W.</li><li>UDI = 100 × Σ(wₜ × 1[L ≤ Eₜ ≤ U]) / W; both endpoints are included.</li><li>Underlit = 100 × Σ(wₜ × 1[Eₜ &lt; L]) / W; the lower boundary is strict.</li><li>Overlit = 100 × Σ(wₜ × 1[Eₜ &gt; U]) / W; the upper boundary is strict.</li></ul><p>Percentages are occupancy-weighted temporal metrics per sensor, not floor-area percentages or certification criteria. UDI, underlit and overlit partition the occupied denominator.</p>",
        escape(&schedule), occupied, partial, denominator,
        settings.da_threshold_lux, settings.udi_lower_lux, settings.udi_upper_lux,
    )?;
    Ok(())
}
fn heatmap(
    html: &mut Html,
    run: &ClimateRunRecord,
    values: &[f64],
    maximum: f64,
    units: &str,
) -> Result<()> {
    let grid = &run.validated.sensor_grid;
    ensure!(
        values.len() == grid.sensors.len(),
        "Report sensor shape mismatch"
    );
    let x = grid.actual_spacing_x_m;
    let y = grid.actual_spacing_y_m;
    write!(
        html,
        "<svg role='img' aria-label='Sensor heatmap, {}' viewBox='0 0 {} {}'>",
        escape(units),
        grid.columns as f64 * x,
        grid.rows as f64 * y
    )?;
    for (i, value) in values.iter().enumerate() {
        let t = if maximum > 0.0 {
            (value / maximum).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let r = (20.0 + 225.0 * t) as u8;
        let g = (65.0 + 170.0 * t) as u8;
        let b = (100.0 - 45.0 * t) as u8;
        write!(html,"<rect x='{}' y='{}' width='{x}' height='{y}' fill='rgb({r},{g},{b})' stroke='#ffffff55'><title>{}: {:.5} {}</title></rect>",(i%grid.columns) as f64*x,(grid.rows-1-i/grid.columns) as f64*y,escape(&grid.sensors[i].id),value,escape(units))?;
    }
    write!(html,"</svg><p>Scale 0–{maximum:.3} {}. Model +Y ↑; window facade ↓. Unweighted sensor min/mean/max: {}</p>",escape(units),escape(&serde_json::to_string(&statistics(values)?)?))?;
    Ok(())
}
fn render_report(root: &Path, view: &ProjectView, comparisons: &[Value]) -> Result<String> {
    let mut html = Html(String::new());
    write!(html,"<!doctype html><html lang='en'><meta charset='utf-8'><meta name='viewport' content='width=device-width'><title>GreenLens selected evidence</title><style>body{{font:16px system-ui;color:#192d29;background:#f7f8f3;margin:3rem auto;max-width:1100px;padding:0 24px}}p{{line-height:1.6}}h2{{border-top:1px solid #bbc9c1;padding-top:24px}}.qualification{{border:2px solid #937126;background:#fff8df;padding:12px 18px;margin:18px 0}}.qualification.failed{{border-color:#a3372b;background:#fff0ed}}.qualification.limited{{border-color:#506d86;background:#eef4fa}}.qualification h3{{margin-top:0}}svg{{display:block;width:100%;max-width:550px;background:#e1e6de}}table{{border-collapse:collapse;width:100%;font-variant-numeric:tabular-nums}}td,th{{padding:7px;border-bottom:1px solid #ccc;text-align:right}}pre{{white-space:pre-wrap;overflow-wrap:anywhere;font-size:12px}}</style><h1>GreenLens · selected evidence</h1><p>{}</p><p>Immutable selected history; current document inputs may differ. Local calculations use fixed geometry, neutral materials and simplified equivalent glass. Metrics are not certification; analytical fixtures establish limited behavior, and adapter replay is integration verification.</p>",escape(&view.project.name))?;
    if !view.results.is_empty() {
        html.write_str("<p><a href='static-report.html'>Static overcast evidence, heatmaps and comparisons</a></p>")?;
    }
    let mut previews = BTreeMap::new();
    let mut scales: BTreeMap<String, f64> = BTreeMap::new();
    for run in &view.climate_results {
        let reader = SeriesReader::open(&root.join("runs").join(&run.id).join("series"))?;
        let values = reader.hour(preview_position(run))?;
        let scale = scales
            .entry(climate::comparison_fingerprint(run)?)
            .or_default();
        *scale = values.iter().copied().fold(*scale, f64::max);
        previews.insert(run.id.clone(), values);
    }
    for run in &view.climate_results {
        let dir = root.join("runs").join(&run.id);
        let (_, weather) = weather::open(&dir.join("weather"))?;
        let interval = preview_interval(run);
        let hour = &weather.data.hours[interval];
        write!(html,"<h2>{} · run {}</h2><p><strong>{}</strong></p><p>{} sensors; {} retained intervals. UTC run timestamps: {} to {}.</p><p>Weather: {}. Fixed UTC offset {} minutes; source-year labels do not define a real calendar or weekdays.</p><details><summary>Weather attribution, assumptions and warnings</summary><pre>{}</pre><pre>{}</pre></details>",escape(&run.alternative_id),escape(&run.id),escape(run.provenance["calculation_label"].as_str().unwrap_or("")),run.series.sensor_ids.len(),run.series.interval_count,escape(run.provenance["started_utc"].as_str().unwrap_or("")),escape(run.provenance["finished_utc"].as_str().unwrap_or("")),escape(&run.weather_asset.source.citation),weather.data.location.utc_offset_minutes,escape(&run.weather_asset.source.rights_note),escape(&serde_json::to_string_pretty(&weather.data.warnings)?))?;
        render_assessment(&mut html, &view.climate_assessments[&run.id])?;
        weather_policy(&mut html, &weather.data)?;
        write!(html,"<h3>Hourly illuminance preview · interval {interval}</h3><p>Month/day {}/{}, hour-ending {} in local standard time; midpoint {:02}:30. Source-year label {} is provenance. {} This is one hourly preview, not an annual illuminance aggregate.</p>",hour.month,hour.day,hour.hour_ending,hour.hour_ending-1,hour.source_year,if run.series.interval_count==1 {"The selected calculation interval is shown."} else {"The offline report explicitly previews January 1 interval 12; all 8,760 intervals remain in the retained series."})?;
        let preview = &previews[&run.id];
        heatmap(
            &mut html,
            run,
            preview,
            scales[&climate::comparison_fingerprint(run)?],
            "lx",
        )?;
        if run.metrics.is_some() {
            metric_definitions(&mut html, &run.study.metrics)?;
            for (i, name) in ["DA", "cDA", "UDI", "Underlit", "Overlit"]
                .into_iter()
                .enumerate()
            {
                write!(html, "<h3>{name}</h3>")?;
                heatmap(&mut html, run, &metric_values(run, i), 100.0, "%")?;
            }
        }
        html.write_str("<details><summary>Sensor values and provenance</summary><table><tr><th>Sensor</th><th>Preview lx</th><th>DA %</th><th>cDA %</th><th>UDI %</th><th>Underlit %</th><th>Overlit %</th></tr>")?;
        for (i, id) in run.series.sensor_ids.iter().enumerate() {
            write!(
                html,
                "<tr><td>{}</td><td>{:.6}</td>",
                escape(id),
                preview[i]
            )?;
            if let Some(metrics) = &run.metrics {
                let m = &metrics[i];
                for value in [
                    m.da_percent,
                    m.cda_percent,
                    m.udi_percent,
                    m.underlit_percent,
                    m.overlit_percent,
                ] {
                    write!(html, "<td>{value:.6}</td>")?;
                }
            } else {
                for _ in 0..5 {
                    html.write_str("<td>Not annual</td>")?;
                }
            }
            html.write_str("</tr>")?;
        }
        write!(
            html,
            "</table><pre>{}</pre></details>",
            escape(&serde_json::to_string_pretty(&run.provenance)?)
        )?;
    }
    html.write_str("<h2>Compatible historical comparisons</h2><p>Each pair uses the latest selected A and B within matching physical/metric conditions. Hourly previews share a lux scale; annual metric deltas are percentage points. No resampling or Cartesian history expansion is performed.</p>")?;
    for comparison in comparisons {
        for key in ["a_id", "b_id"] {
            let id = comparison[key]
                .as_str()
                .context("Missing comparison side")?;
            write!(html, "<h3>Comparison side · {}</h3>", escape(id))?;
            render_assessment(&mut html, &view.climate_assessments[id])?;
        }
        write!(
            html,
            "<details><summary>{} → {}</summary><pre>{}</pre></details>",
            escape(comparison["a_id"].as_str().unwrap_or("")),
            escape(comparison["b_id"].as_str().unwrap_or("")),
            escape(&serde_json::to_string_pretty(comparison)?)
        )?;
    }
    html.write_str("<h2>Replay</h2><p>See REPLAY.txt. Open this report offline; it contains no executable scripts or network dependencies. Original and normalized weather, per-run licensing, text engine inputs, bounded raw RGB/lux chunks, logs, resource observations and provenance remain in the package. Regenerate disposable engine matrices and octrees through GreenLens only.</p></html>")?;
    Ok(html.0)
}

fn render_assessment(
    html: &mut Html,
    assessment: &crate::qualification::RunAssessment,
) -> Result<()> {
    use crate::qualification::{EvidenceIntegrity, InputRelation, QualificationStatus};
    let scientific = &assessment.scientific_qualification;
    let status = match scientific.status {
        QualificationStatus::Failed => "Failed scientific qualification",
        QualificationStatus::Unassessed => "Scientific qualification unassessed",
        QualificationStatus::LimitedQualified => "Limited scientific qualification",
    };
    let status_class = match scientific.status {
        QualificationStatus::Failed => "failed",
        QualificationStatus::Unassessed => "unassessed",
        QualificationStatus::LimitedQualified => "limited",
    };
    let integrity = match assessment.evidence_integrity {
        EvidenceIntegrity::ValidatedRecordOnly => "Record metadata validated; files not checked",
        EvidenceIntegrity::SealedMetadataChecked => {
            "Sealed result metadata checked; full retained evidence not scanned"
        }
        EvidenceIntegrity::FullRetainedEvidenceChecked => "Full retained evidence checks passed",
    };
    let relation = |value| match value {
        InputRelation::Matches => "matches",
        InputRelation::Differs => "differs",
        InputRelation::NotApplicable => "not applicable",
        InputRelation::NotCompared => "not compared",
    };
    write!(html, "<section class='qualification {status_class}'><h3>{status} · {}</h3><p>{}</p><p>Process completion: retained successful exit records. Evidence integrity: {integrity}. Current physical inputs: {}; current metric settings: {}.</p><p>Method: {}. Registry: {}.</p>", escape(&scientific.profile_display_name), escape(&scientific.reason), relation(assessment.current_input_relation.physical), relation(assessment.current_input_relation.metrics), escape(&scientific.identity.method_version), escape(&scientific.registry_version))?;
    for evidence in &scientific.evidence {
        write!(
            html,
            "<p>Evidence ID: {} (SHA-256 {}). Tested scope: {}. {}</p>",
            escape(&evidence.id),
            escape(&evidence.sha256),
            escape(&evidence.tested_scope),
            escape(&evidence.reason)
        )?;
    }
    html.write_str("<p>Successful execution, integrity checks, current inputs and replay agreement do not establish scientific qualification. Assessment is supplemental; original run bytes and historical identities are unchanged. Full machine-readable assessments, profiles, engine inventories and recipe pins are in qualification.json.</p></section>")?;
    Ok(())
}

fn compare_stream(reference: &SeriesReader, combined: &Path, cancel: &AtomicBool) -> Result<Value> {
    let sensors = reference.manifest().sensor_ids.len();
    let intervals = reference.manifest().interval_count;
    let file = File::open(combined)?;
    ensure!(
        file.metadata()?.len() == (sensors * intervals * 24) as u64,
        "Replay RGB result shape differs"
    );
    let mut actual = BufReader::new(file);
    let mut count = 0usize;
    let mut max_error = 0.0_f64;
    let mut max_ratio = 0.0_f64;
    reference.visit_hours(|_, hour| {
        ensure!(!cancel.load(Ordering::SeqCst), "Climate replay cancelled");
        for expected in hour {
            let mut rgb = [0.0; 3];
            for channel in &mut rgb {
                let mut bytes = [0; 8];
                actual.read_exact(&mut bytes)?;
                *channel = f64::from_le_bytes(bytes);
            }
            let observed = rgb_to_lux(rgb)?;
            let error = (observed - expected[3]).abs();
            let tolerance = 0.5_f64.max(expected[3] * 0.005);
            max_error = max_error.max(error);
            max_ratio = max_ratio.max(error / tolerance);
            count += 1;
        }
        Ok(())
    })?;
    let mut extra = [0];
    ensure!(
        actual.read(&mut extra)? == 0,
        "Replay output grew beyond expected shape"
    );
    Ok(
        json!({"stage":"complete interval/sensor integration comparison","intervals":intervals,"sensors":sensors,"values_checked":count,"max_absolute_error_lux":max_error,"max_tolerance_ratio":max_ratio,"tolerance":"max(0.5 lx, 0.5% of reference)","passed":max_ratio<=1.0}),
    )
}

fn verify_primary_replay(
    original: Option<greenlens_radiance::annual_primary::PrimaryStreams>,
    regenerated: Option<greenlens_radiance::annual_primary::PrimaryStreams>,
) -> Result<()> {
    match (original, regenerated) {
        (None, None) => Ok(()),
        (Some(original), Some(regenerated)) => {
            ensure!(
                original.contract == regenerated.contract
                    && original.sensor_sha256 == regenerated.sensor_sha256
                    && original.sensor_count == regenerated.sensor_count
                    && original.sky_rows == regenerated.sky_rows
                    && original.records.len() == regenerated.records.len()
                    && original
                        .records
                        .iter()
                        .zip(&regenerated.records)
                        .all(|(a, b)| a.purpose == b.purpose
                            && a.process_index == b.process_index
                            && a.input == b.input),
                "Replay primary stream differs from the retained generator receipt"
            );
            // Output hashes identify each execution. Secondary sampling may
            // differ; the existing per-value numerical tolerance decides replay.
            Ok(())
        }
        _ => anyhow::bail!("Replay changed paired-primary method semantics"),
    }
}

pub fn replay_climate(
    root: &Path,
    records: &[ClimateRunRecord],
    engine_dir: Option<&Path>,
    cancel: &AtomicBool,
) -> Result<Vec<Value>> {
    if records.is_empty() {
        return Ok(vec![]);
    }
    ensure!(!cancel.load(Ordering::SeqCst), "Climate replay cancelled");
    let engine = Engine::discover(engine_dir)?;
    let mut checks = Vec::new();
    for supplied in records {
        ensure!(!cancel.load(Ordering::SeqCst), "Climate replay cancelled");
        let summary = climate::summary(supplied)?;
        let record = climate::read_run(root, &summary)?;
        ensure!(
            canonical_hash(&record)? == canonical_hash(supplied)?,
            "Replay record differs from verified package snapshot"
        );
        let retained: annual::AnnualEngineIdentity =
            serde_json::from_value(record.provenance["engine"].clone())?;
        // A package can contain multiple supported historical methods. Rehash
        // the installed engine under each record's compiled method contract;
        // never relabel old provenance with the current adapter identity.
        let identity =
            annual::AnnualEngineIdentity::discover_for_method(&engine, &retained.method_version)?;
        ensure!(
            serde_json::to_value(&identity)? == record.provenance["engine"],
            "Replay requires the retained compatible climate engine/library identity"
        );
        let (_, weather) = weather::open(&root.join(&summary.path).join("weather"))?;
        let profile: annual::AnnualProfile =
            serde_json::from_value(record.provenance["profile"].clone())?;
        profile.validate_for_method(&retained.method_version)?;
        let selected = match record.study.calculation {
            StudyMode::WeatherHour { interval_index, .. } => Some(interval_index),
            _ => None,
        };
        let scratch = crate::local_root()?
            .join("replay-climate")
            .join(Uuid::new_v4().to_string());
        fs::create_dir_all(&scratch)?;
        let output = annual::simulate(
            &engine,
            &record.validated,
            &weather.normalized_epw,
            selected,
            &profile,
            &scratch,
            cancel,
            |_: &str| {},
        )?;
        ensure!(
            output.identity == identity
                && output.recipe.engine == identity
                && output.profile == profile
                && output.recipe.profile == profile
                && output.recipe.method_version == retained.method_version,
            "Replay adapter selected a different method or complete profile"
        );
        let expected_indices =
            selected.map_or_else(|| (0..8760).collect::<Vec<_>>(), |index| vec![index]);
        ensure!(
            output.interval_indices == expected_indices
                && output.sensor_count == record.series.sensor_ids.len(),
            "Replay interval or sensor order differs"
        );
        let retained_recipe_bytes = fs::read(regular_child(
            &root.join(&summary.path),
            "annual-recipe.json",
        )?)?;
        ensure!(
            record.provenance["recipe_sha256"] == bytes_hash(&retained_recipe_bytes),
            "Replay recipe changed after verification"
        );
        let retained_recipe: annual::AnnualRecipe = serde_json::from_slice(&retained_recipe_bytes)?;
        let original_primaries =
            climate::read_primary_streams(&root.join(&summary.path), &retained_recipe)?;
        let regenerated_primaries = climate::read_primary_streams(&scratch, &output.recipe)?;
        verify_primary_replay(original_primaries, regenerated_primaries)?;
        let reader = SeriesReader::open(&root.join(&summary.path).join("series"))?;
        let mut check = compare_stream(&reader, &output.combined_f64, cancel)?;
        check["run_id"] = json!(record.id);
        check["resources"] = serde_json::to_value(&output.resources)?;
        check["verification"] =
            json!("same-engine integration, not independent physical validation");
        checks.push(check);
        ensure!(
            checks.last().is_some_and(|v| v["passed"] == true),
            "Climate replay tolerance failed: {}",
            serde_json::to_string(&checks)?
        );
    }
    Ok(checks)
}

#[cfg(test)]
#[path = "climate_export_tests.rs"]
mod tests;
