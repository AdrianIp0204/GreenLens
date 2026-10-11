//! Selected portable evidence and streaming climate replay. Imported command
//! text is never executed; replay invokes only the compiled supported adapter.
use crate::{
    climate::{self, ClimateRunRecord},
    persistence::*,
    timeseries::SeriesReader,
    weather, Project, ProjectView,
};
use anyhow::{ensure, Context, Result};
use greenlens_core::{canonical_hash, rgb_to_lux, statistics, study::StudyMode};
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
    let climate_comparisons = comparison_pairs(&staging, &view.climate_results)?;
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
    write!(html,"<!doctype html><html lang='en'><meta charset='utf-8'><meta name='viewport' content='width=device-width'><title>GreenLens selected evidence</title><style>body{{font:16px system-ui;color:#192d29;background:#f7f8f3;margin:3rem auto;max-width:1100px;padding:0 24px}}p{{line-height:1.6}}h2{{border-top:1px solid #bbc9c1;padding-top:24px}}svg{{display:block;width:100%;max-width:550px;background:#e1e6de}}table{{border-collapse:collapse;width:100%;font-variant-numeric:tabular-nums}}td,th{{padding:7px;border-bottom:1px solid #ccc;text-align:right}}pre{{white-space:pre-wrap;overflow-wrap:anywhere;font-size:12px}}</style><h1>GreenLens · selected evidence</h1><p>{}</p><p>Immutable selected history; current document inputs may differ. Local calculations use fixed geometry, neutral materials and simplified equivalent glass. Metrics are not certification; analytical fixtures establish limited behavior, and adapter replay is integration verification.</p>",escape(&view.project.name))?;
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
        let current_model = view
            .project
            .alternatives
            .iter()
            .find(|a| a.id == run.alternative_id)
            .context("Unknown report alternative")?;
        let current_study = &view
            .project
            .climate
            .as_ref()
            .context("Missing climate project")?
            .studies[&run.alternative_id];
        let physical_current = current_study.physical_input_hash(&current_model.model)?
            == run.study.physical_input_hash(&run.model)?;
        let metric_current =
            canonical_hash(&current_study.metrics)? == canonical_hash(&run.study.metrics)?;
        write!(html,"<h2>{} · run {}</h2><p><strong>{}</strong></p><p>Physical inputs match current document: {}. Metric settings match: {}. {} sensors; {} retained intervals. UTC run timestamps: {} to {}.</p><p>Weather: {}. Fixed UTC offset {} minutes; source-year labels do not define a real calendar or weekdays.</p><details><summary>Weather attribution, assumptions and warnings</summary><pre>{}</pre><pre>{}</pre></details>",escape(&run.alternative_id),escape(&run.id),escape(run.provenance["calculation_label"].as_str().unwrap_or("")),physical_current,metric_current,run.series.sensor_ids.len(),run.series.interval_count,escape(run.provenance["started_utc"].as_str().unwrap_or("")),escape(run.provenance["finished_utc"].as_str().unwrap_or("")),escape(&run.weather_asset.source.citation),weather.data.location.utc_offset_minutes,escape(&run.weather_asset.source.rights_note),escape(&serde_json::to_string_pretty(&weather.data.warnings)?))?;
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
            write!(html,"<p>DA threshold {:.3} lx; UDI inclusive [{:.3}, {:.3}] lx. Occupancy: {}. Nighttime occupancy remains in the denominator. Percentages are occupancy-weighted temporal metrics per sensor, not floor-area percentages.</p>",run.study.metrics.da_threshold_lux,run.study.metrics.udi_lower_lux,run.study.metrics.udi_upper_lux,escape(&serde_json::to_string(&run.study.metrics.occupancy)?))?;
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
    let identity = annual::AnnualEngineIdentity::discover(&engine)?;
    let mut checks = Vec::new();
    for supplied in records {
        ensure!(!cancel.load(Ordering::SeqCst), "Climate replay cancelled");
        let summary = climate::summary(supplied)?;
        let record = climate::read_run(root, &summary)?;
        ensure!(
            canonical_hash(&record)? == canonical_hash(supplied)?,
            "Replay record differs from verified package snapshot"
        );
        ensure!(
            serde_json::to_value(&identity)? == record.provenance["engine"],
            "Replay requires the retained compatible climate engine/library identity"
        );
        let (_, weather) = weather::open(&root.join(&summary.path).join("weather"))?;
        let profile: annual::AnnualProfile =
            serde_json::from_value(record.provenance["profile"].clone())?;
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
        let expected_indices =
            selected.map_or_else(|| (0..8760).collect::<Vec<_>>(), |index| vec![index]);
        ensure!(
            output.interval_indices == expected_indices
                && output.sensor_count == record.series.sensor_ids.len(),
            "Replay interval or sensor order differs"
        );
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
