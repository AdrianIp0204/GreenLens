use crate::{Comparison, ProjectView, CALCULATION_LABEL};
use anyhow::Result;
use std::fmt::Write;

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
pub fn render(view: &ProjectView, comparisons: &[Comparison]) -> Result<String> {
    let mut html = format!(
        r#"<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>GreenLens evidence</title><style>body{{font:16px system-ui,sans-serif;color:#192d29;background:#f7f8f3;margin:3rem auto;max-width:1100px;padding:0 24px}}h1{{font-size:42px}}h2{{border-top:1px solid #bbc9c1;padding-top:28px}}small,p{{line-height:1.6}}table{{border-collapse:collapse;width:100%;font-variant-numeric:tabular-nums}}td,th{{padding:8px;text-align:right;border-bottom:1px solid #ddd}}th:first-child,td:first-child{{text-align:left}}svg{{max-width:600px;width:100%;background:#e1e6de}}pre{{white-space:pre-wrap;overflow-wrap:anywhere;font-size:12px}}.note{{border-left:3px solid #44745a;padding:8px 16px}}details{{margin:24px 0}}</style><h1>GreenLens · evidence report</h1><p>{}</p><p><strong>{}</strong></p><p class="note">Synthetic geometry, prescribed azimuth-symmetric sky, neutral diffuse surfaces and equivalent zero-thickness glass. No simulated date, weather series, annual daylight or certification evaluation. Analytical fixtures establish limited behavior; adapter replay is integration verification, not measured-building validation.</p>"#,
        escape(&view.project.name),
        CALCULATION_LABEL
    );
    let scale = view
        .results
        .iter()
        .flat_map(|r| r.lux.iter())
        .copied()
        .fold(0.0, f64::max)
        .max(1.0);
    write!(html,"<p>All heatmaps use one shared scale: 0–{scale:.2} lx. Sensor cells have equal area; sensor positions are cell centers. The window facade is at local y=0. Historical run geometry is shown from immutable inputs.</p>")?;
    write!(html,"<div style='max-width:600px;height:18px;background:linear-gradient(to right,rgb(20,65,100),rgb(245,235,55))' aria-label='Shared illuminance legend'></div><p>0 lx <span style='display:inline-block;width:55%'></span>{scale:.2} lx</p>")?;
    for run in &view.results {
        let grid = &run.validated.sensor_grid;
        write!(html,"<h2>Alternative {} · {}</h2><p>Run {} · {} sensors · actual spacing {:.4} × {:.4} m</p><p>Statistics (lx): {}</p>",escape(&run.alternative_id),escape(&run.model.name),escape(&run.id),run.lux.len(),grid.actual_spacing_x_m,grid.actual_spacing_y_m,escape(&run.statistics.to_string()))?;
        if let Some(current) = view
            .project
            .alternatives
            .iter()
            .find(|a| a.id == run.alternative_id)
        {
            if crate::is_stale(run, &current.model)? {
                html.push_str("<p class='note'>This historical run is stale relative to the current alternative. All values below describe its immutable original inputs.</p>");
            }
        }
        write!(html, "<p>Room {:.2} × {:.2} × {:.2} m; workplane {:.2} m; prescribed exterior {:.1} lx; glazing normal transmittance {:.3}.</p><details><summary>Immutable model and material assumptions</summary><pre>{}</pre></details>", run.model.room.width_m,run.model.room.depth_m,run.model.room.height_m,run.model.sensors.workplane_height_m,run.model.sky.exterior_horizontal_illuminance_lux,run.model.materials.visible_transmittance,escape(&serde_json::to_string_pretty(&run.model)?))?;
        let cell_x = grid.actual_spacing_x_m * 80.0;
        let cell_y = grid.actual_spacing_y_m * 80.0;
        write!(html,"<svg role='img' aria-label='Illuminance in lux; model local positive Y at top' viewBox='0 0 {} {}'>",grid.columns as f64*cell_x,grid.rows as f64*cell_y)?;
        for (i, value) in run.lux.iter().enumerate() {
            let x = (i % grid.columns) as f64 * cell_x;
            let y = (grid.rows - 1 - i / grid.columns) as f64 * cell_y;
            let t = (value / scale).clamp(0.0, 1.0);
            let r = (20.0 + 225.0 * t) as u8;
            let g = (65.0 + 170.0 * t) as u8;
            let b = (100.0 - 45.0 * t) as u8;
            write!(html,"<rect x='{x}' y='{y}' width='{cell_x}' height='{cell_y}' fill='rgb({r},{g},{b})' stroke='#ffffff55'><title>{}: {:.3} lx</title></rect>",escape(&grid.sensors[i].id),value)?;
        }
        write!(html,"</svg><p>Model +Y ↑ · window ↓ · facade azimuth {:.1}° clockwise from true north · units lx</p><details><summary>Sensor table and immutable provenance</summary><table><thead><tr><th>ID</th><th>X m</th><th>Y m</th><th>Z m</th><th>Illuminance lx</th></tr></thead><tbody>",(180.0+run.model.room.rotation_deg).rem_euclid(360.0))?;
        for (sensor, value) in grid.sensors.iter().zip(&run.lux) {
            write!(
                html,
                "<tr><td>{}</td><td>{:.4}</td><td>{:.4}</td><td>{:.4}</td><td>{:.4}</td></tr>",
                escape(&sensor.id),
                sensor.position[0],
                sensor.position[1],
                sensor.position[2],
                value
            )?;
        }
        write!(
            html,
            "</tbody></table><pre>{}</pre></details>",
            escape(&serde_json::to_string_pretty(&run.provenance)?)
        )?;
    }
    if comparisons.is_empty() {
        html.push_str("<p>No compatible completed A/B pair is available. Matching sensor, environment, engine and calculation conditions are required; runs are never silently resampled.</p>");
    }
    for comparison in comparisons {
        write!(html,"<h2>Compatible comparison · B minus A</h2><p>{} → {}</p><p>Delta statistics (lx): {}</p><p>Reflective shading can increase individual readings. Percentage deltas are omitted.</p><details><summary>Per-sensor deltas and conditions</summary><pre>{}</pre></details>",escape(&comparison.a_id),escape(&comparison.b_id),escape(&comparison.statistics.to_string()),escape(&serde_json::to_string_pretty(comparison)?))?;
    }
    html.push_str("<h2>Reproduction</h2><p>Install the separately acquired pinned Radiance engine. Run <code>greenlens replay evidence.zip --engine-dir &lt;engine-directory&gt;</code>. See REPLAY.txt, checksums.json and each run's recipe.json. Octrees are disposable and regenerated. This HTML has no network dependencies or executable scripts.</p></html>");
    Ok(html)
}
