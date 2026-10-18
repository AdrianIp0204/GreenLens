//! Paired model drawings, not spatial simulation results. Both cases use one
//! exact world-space frame. No old report/figure codec calls this new method.
use super::*;
use crate::{
    architectural_scope::{ReferenceReportSection, ReferenceSectionAxis, ReferenceSectionKeep},
    canonical_hash, Result,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const DESIGN_FIGURE_SCHEMA: &str = "greenlens.paired-design-figures.v1";
pub const DESIGN_FIGURE_METHOD: &str = "greenlens.analytical-paired-drawing/1";
pub const MAX_DESIGN_FIGURE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_DESIGN_CHANGE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_DESIGN_EDGES: usize = 16_384;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DesignFigureEvidence {
    NotSimulated {
        reason: String,
    },
    /// The shared runner must check actual retained bytes before supplying this
    /// binding. A serialized claim never performs an integrity check itself.
    RetainedRun {
        run_id: String,
        record_sha256: String,
        physical_input_sha256: String,
        assessment: Box<EnergyAssessment>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DesignFigureCase {
    pub design_id: String,
    pub label: String,
    pub model: ThermalModel,
    pub calendar: EnergyCalendar,
    pub study: Option<EnergyStudy>,
    pub scope_revision_sha256: Option<String>,
    pub evidence: DesignFigureEvidence,
}
impl DesignFigureCase {
    pub fn from_report_case(
        case: &EnergyReportCase,
        scope_revision_sha256: Option<String>,
    ) -> Result<Self> {
        case.validate()?;
        Ok(Self {
            design_id: case.id.clone(),
            label: case.label.clone(),
            model: case.model.clone(),
            calendar: case.study.calendar.clone(),
            study: Some(case.study.clone()),
            scope_revision_sha256,
            evidence: DesignFigureEvidence::RetainedRun {
                run_id: case.run_id.clone(),
                record_sha256: case.record_sha256.clone(),
                physical_input_sha256: case.physical_input_sha256.clone(),
                assessment: Box::new(case.assessment.clone()),
            },
        })
    }
    fn validate(&self, id: &str) -> Result<String> {
        if self.design_id != id {
            return Err(invalid(
                "figure.design",
                format!("Expected explicit {id} design snapshot"),
            ));
        }
        text("figure.label", &self.label)?;
        if self.label.len() > 256 {
            return Err(invalid("figure.label", "At most 256 UTF-8 bytes"));
        }
        if let Some(scope) = &self.scope_revision_sha256 {
            hash("figure.scope", scope)?;
        }
        let model = validate_thermal_model(&self.model, &self.calendar)?.model_input_sha256;
        if let Some(study) = &self.study {
            study.validate(&self.model)?;
            if study.calendar != self.calendar {
                return Err(invalid(
                    "figure.calendar",
                    "Figure and study calendars must match exactly",
                ));
            }
        }
        match &self.evidence {
            DesignFigureEvidence::NotSimulated { reason } => text("figure.not_simulated", reason)?,
            DesignFigureEvidence::RetainedRun {
                run_id,
                record_sha256,
                physical_input_sha256,
                assessment,
            } => {
                text("figure.run", run_id)?;
                hash("figure.record", record_sha256)?;
                hash("figure.physical", physical_input_sha256)?;
                let study = self.study.as_ref().ok_or_else(|| {
                    invalid(
                        "figure.study",
                        "A retained run needs its exact study snapshot",
                    )
                })?;
                if study.physical_input_hash(&self.model)? != *physical_input_sha256 {
                    return Err(invalid("figure.evidence", "Retained evidence differs from the depicted inputs; show Not simulated or use the actual historical snapshot"));
                }
                assessment.validate(physical_input_sha256, &assessment.scientific.method)?;
                if assessment.execution != EnergyExecutionStatus::Completed
                    || assessment.evidence_integrity
                        != EnergyIntegrityScope::CompleteRetainedEvidenceChecked
                {
                    return Err(invalid("figure.evidence", "Retained figures require completed, completely checked evidence supplied by the shared runner"));
                }
            }
        }
        Ok(model)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesignViewKind {
    Axonometric,
    Plan,
    Front,
    Right,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DesignFigureFrame {
    pub id: String,
    pub title: String,
    pub kind: DesignViewKind,
    pub eye_m: [f64; 3],
    pub target_m: [f64; 3],
    pub up: [f64; 3],
    pub vertical_span_m: f64,
    pub aspect_ratio: f64,
    pub section: Option<ReferenceReportSection>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PairedDesignFigureRequest {
    pub schema_version: String,
    pub baseline: DesignFigureCase,
    pub proposed: DesignFigureCase,
    pub frames: Vec<DesignFigureFrame>,
    /// Describes declared analytical coverage, not an inferred whole building.
    pub scope: String,
    /// Caller-confirmed orientation; None discloses unknown true north.
    pub north_declaration: Option<String>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesignObjectChangeKind {
    Added,
    Removed,
    GeometryModified,
    PropertiesModified,
    LabelOnly,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DesignObjectChange {
    pub object_id: String,
    pub kind: DesignObjectChangeKind,
    pub before_geometry_sha256: Option<String>,
    pub after_geometry_sha256: Option<String>,
    /// Complete bounded authoritative object snapshots, including dependent
    /// assignments. These are input changes, never a causal savings estimate.
    pub before: Option<Value>,
    pub after: Option<Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DesignFigureProduct {
    pub filename: String,
    pub frame_id: String,
    pub side: String,
    pub svg_sha256: String,
    pub svg: String,
    pub analytical_edge_count: usize,
    pub scale_bar_m: f64,
    pub scale_svg_units_per_m: f64,
    pub displayed_label_count: usize,
    pub omitted_label_count: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PairedDesignFigures {
    pub schema_version: String,
    pub method: String,
    pub request: PairedDesignFigureRequest,
    pub baseline_model_sha256: String,
    pub proposed_model_sha256: String,
    pub changes: Vec<DesignObjectChange>,
    pub change_count: usize,
    pub comparable_geometry_weather_calendar: bool,
    pub unchanged_service_inputs: Option<bool>,
    pub products: Vec<DesignFigureProduct>,
    pub limitations: Vec<String>,
    pub content_sha256: String,
}

type Point = [f64; 3];
fn sub(a: Point, b: Point) -> Point {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: Point, b: Point) -> f64 {
    a.into_iter().zip(b).map(|(x, y)| x * y).sum()
}
fn cross(a: Point, b: Point) -> Point {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(a: Point) -> Point {
    let n = dot(a, a).sqrt();
    a.map(|x| x / n)
}
pub(crate) fn axes(frame: &DesignFigureFrame) -> Result<(Point, Point)> {
    identifier("figure.frame.id", &frame.id)?;
    text("figure.frame.title", &frame.title)?;
    let forward = sub(frame.target_m, frame.eye_m);
    if frame
        .eye_m
        .into_iter()
        .chain(frame.target_m)
        .chain(frame.up)
        .any(|v| !v.is_finite())
        || dot(forward, forward) < 1e-18
        || dot(cross(forward, frame.up), cross(forward, frame.up)) < 1e-18
    {
        return Err(invalid(
            "figure.frame",
            "Finite independent camera, target and up vectors required",
        ));
    }
    range("figure.vertical_span_m", frame.vertical_span_m, 1e-6, 1e9)?;
    range("figure.aspect_ratio", frame.aspect_ratio, 0.25, 4.)?;
    if let Some(section) = &frame.section {
        range("figure.section", section.offset_m, -1e9, 1e9)?;
    }
    let forward = unit(forward);
    let right = unit(cross(forward, frame.up));
    let up = unit(cross(right, forward));
    let expected = match frame.kind {
        DesignViewKind::Plan => Some([0., 0., -1.]),
        DesignViewKind::Front => Some([0., 1., 0.]),
        DesignViewKind::Right => Some([-1., 0., 0.]),
        DesignViewKind::Axonometric => None,
    };
    if expected.is_some_and(|v| (0..3).any(|i| (v[i] - forward[i]).abs() > 1e-10)) {
        return Err(invalid(
            "figure.view_kind",
            "Named orthographic direction differs from the stored frame",
        ));
    }
    Ok((right, up))
}
fn scene_geometry(scene: &ThermalScene) -> BTreeMap<String, Vec<[[f64; 3]; 4]>> {
    let mut objects: BTreeMap<_, _> = scene
        .objects
        .iter()
        .map(|o| (o.id.clone(), o.polygons_m.clone()))
        .collect();
    for zone in scene.objects.iter().filter(|o| o.kind == "zone") {
        objects.insert(
            zone.id.clone(),
            scene
                .objects
                .iter()
                .filter(|o| o.kind == "surface" && o.zone_id.as_ref() == Some(&zone.id))
                .flat_map(|o| o.polygons_m.clone())
                .collect(),
        );
    }
    objects
}
fn snapshots(model: &ThermalModel) -> Result<BTreeMap<String, Value>> {
    let mut result = thermal_object_snapshots(model)?;
    // Full model duplication would obscure object-level changes and inflate the
    // register. Every library/zone object is already retained under its stable ID.
    result.insert("model:building".into(), json!({"name":model.name,"site":model.site,"north_rotation_deg":model.north_rotation_deg,"exterior_lighting_w":model.exterior_lighting_w,"exterior_lighting_schedule":model.exterior_lighting_schedule,"schema_version":model.schema_version}));
    Ok(result)
}
fn without_labels(value: &Value) -> Value {
    match value {
        Value::Object(v) => Value::Object(
            v.iter()
                .filter(|(k, _)| k.as_str() != "name")
                .map(|(k, v)| (k.clone(), without_labels(v)))
                .collect(),
        ),
        Value::Array(v) => Value::Array(v.iter().map(without_labels).collect()),
        other => other.clone(),
    }
}
fn object_changes(
    a: &ThermalModel,
    b: &ThermalModel,
    sa: &ThermalScene,
    sb: &ThermalScene,
) -> Result<Vec<DesignObjectChange>> {
    let a = snapshots(a)?;
    let b = snapshots(b)?;
    let ga = scene_geometry(sa);
    let gb = scene_geometry(sb);
    let mut rows = vec![];
    for id in a.keys().chain(b.keys()).collect::<BTreeSet<_>>() {
        if a.get(id) == b.get(id) {
            continue;
        }
        let ah = ga.get(id).map(canonical_hash).transpose()?;
        let bh = gb.get(id).map(canonical_hash).transpose()?;
        let kind = match (a.get(id), b.get(id)) {
            (None, _) => DesignObjectChangeKind::Added,
            (_, None) => DesignObjectChangeKind::Removed,
            (Some(x), Some(y)) if without_labels(x) == without_labels(y) => {
                DesignObjectChangeKind::LabelOnly
            }
            _ if ah != bh => DesignObjectChangeKind::GeometryModified,
            _ => DesignObjectChangeKind::PropertiesModified,
        };
        rows.push(DesignObjectChange {
            object_id: id.clone(),
            kind,
            before_geometry_sha256: ah,
            after_geometry_sha256: bh,
            before: a.get(id).cloned(),
            after: b.get(id).cloned(),
        });
    }
    if serde_json::to_vec(&rows)?.len() > MAX_DESIGN_CHANGE_BYTES {
        return Err(invalid(
            "figure.changes",
            "Complete change register exceeds 16 MiB; no rows were silently omitted",
        ));
    }
    Ok(rows)
}

/// Fit both validated models once. No display visibility or active selection is
/// consulted. Explicit sections can be added before accepting these frames.
pub fn default_design_frames(
    a: &ThermalModel,
    ca: &EnergyCalendar,
    b: &ThermalModel,
    cb: &EnergyCalendar,
) -> Result<Vec<DesignFigureFrame>> {
    let sa = thermal_scene(a, ca)?;
    let sb = thermal_scene(b, cb)?;
    let points: Vec<_> = sa
        .objects
        .iter()
        .chain(&sb.objects)
        .flat_map(|o| o.polygons_m.iter().flatten().copied())
        .collect();
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    for p in &points {
        for i in 0..3 {
            low[i] = low[i].min(p[i]);
            high[i] = high[i].max(p[i]);
        }
    }
    let target = std::array::from_fn(|i| (low[i] + high[i]) / 2.);
    let mut frames = vec![];
    for (id, title, kind, offset, up) in [
        (
            "axonometric",
            "Matched axonometric",
            DesignViewKind::Axonometric,
            [1., -1., 0.8],
            [0., 0., 1.],
        ),
        (
            "plan",
            "Matched plan",
            DesignViewKind::Plan,
            [0., 0., 1.],
            [0., 1., 0.],
        ),
    ] {
        let mut frame = DesignFigureFrame {
            id: id.into(),
            title: title.into(),
            kind,
            eye_m: std::array::from_fn(|i| target[i] + offset[i]),
            target_m: target,
            up,
            vertical_span_m: 1.,
            aspect_ratio: 1.6,
            section: None,
        };
        let (right, up) = axes(&frame)?;
        let half_w = points
            .iter()
            .map(|p| dot(sub(*p, target), right).abs())
            .fold(0., f64::max);
        let half_h = points
            .iter()
            .map(|p| dot(sub(*p, target), up).abs())
            .fold(0., f64::max);
        frame.vertical_span_m = (2. * half_h).max(2. * half_w / frame.aspect_ratio).max(0.1) * 1.18;
        frames.push(frame);
    }
    Ok(frames)
}
pub(crate) fn clip(a: &mut Point, b: &mut Point, n: Point, offset: f64) -> bool {
    let da = dot(*a, n) - offset;
    let db = dot(*b, n) - offset;
    if da < 0. && db < 0. {
        return false;
    }
    if da < 0. || db < 0. {
        let t = da / (da - db);
        let p = std::array::from_fn(|i| a[i] + t * (b[i] - a[i]));
        if da < 0. {
            *a = p;
        } else {
            *b = p;
        }
    }
    true
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn short(s: &str, max: usize) -> String {
    let mut c = s.chars();
    let mut v: String = c.by_ref().take(max).collect();
    if c.next().is_some() {
        v.push('…');
    }
    v
}
/// Conservative, deterministic line budget: every Unicode scalar receives a
/// full em of horizontal space. Text never depends on host font measurement.
/// Exact unshortened declarations remain in the retained drawing request.
fn wrapped_lines(value: &str, columns: usize, max_lines: usize) -> Vec<String> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<_> = normalized.chars().collect();
    let mut lines = Vec::new();
    let mut start = 0;
    while start < chars.len() && lines.len() < max_lines {
        let mut end = (start + columns).min(chars.len());
        if end < chars.len() {
            if let Some(space) = chars[start..end].iter().rposition(|c| *c == ' ') {
                if space > columns / 2 {
                    end = start + space;
                }
            }
        }
        lines.push(chars[start..end].iter().collect::<String>());
        start = end;
        while start < chars.len() && chars[start] == ' ' {
            start += 1;
        }
    }
    if start < chars.len() {
        let suffix = "… [full text in JSON]";
        if let Some(last) = lines.last_mut() {
            *last = format!(
                "{}{}",
                last.chars()
                    .take(columns - suffix.chars().count())
                    .collect::<String>(),
                suffix
            );
        }
    }
    lines
}
fn text_lines(svg: &mut String, lines: &[String], y: usize, step: usize, role: &str, class: &str) {
    for (i, line) in lines.iter().enumerate() {
        svg.push_str(&format!(
            "<text data-role=\"{role}\" x=\"48\" y=\"{}\" class=\"gl-design-{class}\">{}</text>",
            y + i * step,
            escape(line)
        ));
    }
}
fn evidence_label(case: &DesignFigureCase) -> String {
    match &case.evidence {
        DesignFigureEvidence::NotSimulated { .. } => "NOT SIMULATED".into(),
        DesignFigureEvidence::RetainedRun {
            run_id, assessment, ..
        } => format!(
            "Run {} · science {:?} · inputs {:?}",
            short(run_id, 12),
            assessment.scientific.status,
            assessment.current_physical_inputs
        ),
    }
}
fn geometry_style(
    id: &str,
    side: &str,
    changes: &[DesignObjectChange],
) -> (&'static str, &'static str, f64) {
    match changes.iter().find(|c| c.object_id == id).map(|c| c.kind) {
        Some(DesignObjectChangeKind::Added) => ("#187747", "", 1.8),
        Some(DesignObjectChangeKind::Removed) => ("#b33d57", "5 3", 1.8),
        Some(DesignObjectChangeKind::GeometryModified) if side == "A" => ("#a15a00", "5 3", 1.8),
        Some(DesignObjectChangeKind::GeometryModified) => ("#a15a00", "", 1.8),
        Some(DesignObjectChangeKind::PropertiesModified) => ("#285ba3", "", 1.3),
        _ => ("#737d84", "", 0.8),
    }
}
fn svg_product(
    frame: &DesignFigureFrame,
    side: &str,
    a: &ThermalScene,
    b: &ThermalScene,
    request: &PairedDesignFigureRequest,
    changes: &[DesignObjectChange],
    scoped_annotations: bool,
) -> Result<DesignFigureProduct> {
    let (right, up) = axes(frame)?;
    let width = frame.vertical_span_m * frame.aspect_ratio;
    let height = frame.vertical_span_m;
    let scale = (1024. / width).min(440. / height);
    let left = 88. + (1024. - width * scale) / 2.;
    let top = 274. + (440. - height * scale) / 2.;
    // Explicit pinned family matches the native renderer's isolated font set.
    // Root inheritance and namespaced classes avoid leaking a generic `text`
    // selector into other figures when this SVG is embedded in standalone HTML.
    let mut svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1200\" height=\"1140\" viewBox=\"0 0 1200 1140\" font-family=\"Noto Sans CJK TC,sans-serif\" fill=\"#182b37\"><rect width=\"1200\" height=\"1140\" fill=\"white\"/><style>.gl-design-small{font-size:17px}.gl-design-body{font-size:19px}.gl-design-title{font-size:23px}</style>".to_owned();
    text_lines(
        &mut svg,
        &wrapped_lines(&format!("GreenLens · {} · {side}", frame.title), 46, 2),
        38,
        27,
        "title",
        "title",
    );
    let cases: Vec<_> = if side == "A" {
        vec![(&request.baseline, a, "A")]
    } else if side == "B" {
        vec![(&request.proposed, b, "B")]
    } else {
        vec![(&request.baseline, a, "A"), (&request.proposed, b, "B")]
    };
    for (index, (case, _, _)) in cases.iter().enumerate() {
        text_lines(
            &mut svg,
            &wrapped_lines(
                &format!("{} · {}", case.design_id, evidence_label(case)),
                62,
                2,
            ),
            90 + index * 44,
            22,
            "evidence",
            "small",
        );
    }
    let frame_label = if scoped_annotations
        && frame.id.starts_with("accepted-")
        && frame.kind == DesignViewKind::Axonometric
    {
        "accepted orthographic frame".into()
    } else {
        format!("{:?}", frame.kind)
    };
    text_lines(&mut svg, &wrapped_lines(&format!("Shared orthographic frame: {width:.4} × {height:.4} m; {frame_label}; no spatial simulation field"),62,2),184,22,"frame","small");
    if changes
        .iter()
        .all(|c| c.before_geometry_sha256 == c.after_geometry_sha256)
    {
        text_lines(&mut svg, &wrapped_lines("Geometry is identical. Property and label changes, if any, are listed in the complete change register.",62,2),230,22,"geometry-note","small");
    }
    svg.push_str(&format!("<rect x=\"{left:.3}\" y=\"{top:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"#fafcfc\" stroke=\"#c2cbd0\"/>",width*scale,height*scale));
    let mut count = 0;
    let mut labels = Vec::new();
    let mut label_count = 0usize;
    for (_, scene, case_id) in &cases {
        let mut visible_zones = BTreeSet::new();
        for object in scene.objects.iter().filter(|o| o.kind != "zone") {
            if side == "overlay"
                && *case_id == "A"
                && !changes.iter().any(|c| {
                    c.object_id == object.id
                        && matches!(
                            c.kind,
                            DesignObjectChangeKind::Removed
                                | DesignObjectChangeKind::GeometryModified
                        )
                })
            {
                continue;
            }
            let (stroke, dash, weight) = geometry_style(&object.id, case_id, changes);
            let mut label_at = None;
            for polygon in &object.polygons_m {
                for index in 0..4 {
                    let mut p = polygon[index];
                    let mut q = polygon[(index + 1) % 4];
                    if let Some(s) = &frame.section {
                        let i = match s.axis {
                            ReferenceSectionAxis::X => 0,
                            ReferenceSectionAxis::Y => 1,
                            ReferenceSectionAxis::Z => 2,
                        };
                        let sign = if s.keep == ReferenceSectionKeep::Above {
                            1.
                        } else {
                            -1.
                        };
                        let mut n = [0.; 3];
                        n[i] = sign;
                        if !clip(&mut p, &mut q, n, s.offset_m * sign) {
                            continue;
                        }
                    }
                    let planes = [
                        (right, dot(frame.target_m, right) - width / 2.),
                        (right.map(|v| -v), -dot(frame.target_m, right) - width / 2.),
                        (up, dot(frame.target_m, up) - height / 2.),
                        (up.map(|v| -v), -dot(frame.target_m, up) - height / 2.),
                    ];
                    if !planes.into_iter().all(|(n, d)| clip(&mut p, &mut q, n, d)) {
                        continue;
                    }
                    let project = |v: Point| {
                        let d = sub(v, frame.target_m);
                        [
                            left + (dot(d, right) + width / 2.) * scale,
                            top + (height / 2. - dot(d, up)) * scale,
                        ]
                    };
                    let p = project(p);
                    let q = project(q);
                    if (p[0] - q[0]).abs() + (p[1] - q[1]).abs() < 1e-8 {
                        continue;
                    }
                    count += 1;
                    if count > MAX_DESIGN_EDGES {
                        return Err(invalid("figure.edges","Paired drawing exceeds its edge limit; no geometry was silently sampled"));
                    }
                    svg.push_str(&format!("<path data-object=\"{}\" data-side=\"{}\" d=\"M{:.3},{:.3} L{:.3},{:.3}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"{weight}\" stroke-dasharray=\"{dash}\"/>",escape(&object.id),case_id,p[0],p[1],q[0],q[1]));
                    label_at.get_or_insert(p);
                    if let Some(zone) = &object.zone_id {
                        visible_zones.insert(zone);
                    }
                }
            }
            // All paths keep exact IDs. The page has an explicit label bound;
            // omission is a captioned presentation limit, never geometry loss.
            if object.kind == "window" {
                if let Some(p) = label_at {
                    label_count += 1;
                    if labels.len() < 64 {
                        labels.push((p, format!("{} {}", case_id, short(&object.id, 24))));
                    }
                }
            }
        }
        for zone in scene.objects.iter().filter(|o| o.kind == "zone") {
            if !visible_zones.contains(&zone.id) {
                continue;
            }
            let points = scene
                .objects
                .iter()
                .filter(|o| o.kind == "surface" && o.zone_id.as_ref() == Some(&zone.id))
                .flat_map(|o| o.polygons_m.iter().flatten())
                .collect::<Vec<_>>();
            let center = std::array::from_fn(|i| {
                points.iter().map(|p| p[i]).sum::<f64>() / points.len() as f64
            });
            let d = sub(center, frame.target_m);
            let p = [
                left + (dot(d, right) + width / 2.) * scale,
                top + (height / 2. - dot(d, up)) * scale,
            ];
            if p[0] >= left
                && p[0] <= left + width * scale
                && p[1] >= top
                && p[1] <= top + height * scale
            {
                label_count += 1;
                if labels.len() < 64 {
                    labels.push((p, format!("{} zone {}", case_id, short(&zone.id, 24))));
                }
            }
        }
    }
    // Stable collision displacement with leader lines; labels stay inside the
    // drawing. Full names and semantic IDs are retained in the JSON contract.
    let mut occupied = Vec::<([f64; 2], f64)>::new();
    let mut displayed_labels = 0;
    for (anchor, label) in &labels {
        let label_width = label.chars().count() as f64 * 17. + 8.;
        if width * scale < label_width + 12. || height * scale < 28. {
            continue;
        }
        let mut p = [
            anchor[0].clamp(left + 4., left + width * scale - label_width - 4.),
            (anchor[1] - 5.).clamp(top + 20., top + height * scale - 5.),
        ];
        let mut placed = false;
        for _ in 0..64 {
            if !occupied.iter().any(|(q, w)| {
                p[0] < q[0] + w + 6. && p[0] + label_width + 6. > q[0] && (p[1] - q[1]).abs() < 22.
            }) {
                placed = true;
                break;
            }
            p[1] += 23.;
            if p[1] > top + height * scale - 5. {
                p[1] = top + 20.;
                p[0] = (p[0] + label_width + 8.).min(left + width * scale - label_width - 4.);
            }
        }
        if !placed {
            continue;
        }
        displayed_labels += 1;
        occupied.push((p, label_width));
        svg.push_str(&format!("<path d=\"M{:.3},{:.3} L{:.3},{:.3}\" stroke=\"#c2cbd0\" stroke-width=\"0.6\"/><text x=\"{:.3}\" y=\"{:.3}\" class=\"gl-design-small\" stroke=\"white\" stroke-width=\"3\" paint-order=\"stroke\">{}</text>",anchor[0],anchor[1],p[0],p[1],p[0],p[1],escape(label)));
    }
    let desired = width / 5.;
    if count == 0 {
        svg.push_str("<text x=\"120\" y=\"490\" font-size=\"19\">No analytical edges intersect this frame and section.</text>");
    }
    let exponent = 10f64.powf(desired.log10().floor());
    let ratio = desired / exponent;
    let bar = if ratio >= 5. {
        5.
    } else if ratio >= 2. {
        2.
    } else {
        1.
    } * exponent;
    svg.push_str(&format!("<path d=\"M88,746 h{:.3}\" stroke=\"#182b37\" stroke-width=\"3\"/><text x=\"88\" y=\"772\" class=\"gl-design-body\">{bar:.4} m · scale bar in the orthographic projection</text>",bar*scale));
    let section = frame
        .section
        .as_ref()
        .map(|s| {
            format!(
                "Section {:?} = {:.4} m; keep {:?}; no filled cut faces",
                s.axis, s.offset_m, s.keep
            )
        })
        .unwrap_or_else(|| {
            "No section; transparent outlines, not hidden-surface or thermal-field rendering".into()
        });
    // The public v1 path retains its exact text. Scoped drawings distinguish
    // complete A/B outlines from separately bounded reference context before
    // wrapping; a post-SVG string replacement misses split text nodes.
    let completeness_note = if scoped_annotations {
        "A/B only"
    } else {
        "no sampling"
    };
    let footer=[("Legend: green added · red dashed removed · amber geometry modified · blue property-only · grey unchanged/renamed".to_owned(),2), (format!("{} · {}",section,request.north_declaration.as_deref().unwrap_or("True north not documented")),3), (format!("Scope: {}",request.scope),3), (format!("A model {} · B model {}",&a.model_sha256[..16],&b.model_sha256[..16]),1), (format!("Labels {displayed_labels}/{label_count}; complete IDs in JSON. {} change rows; {count} edges; {completeness_note}. Exact run/scope bindings and full text accompany this figure.",changes.len()),3)];
    let mut footer_y = 804;
    for (line, limit) in footer {
        let lines = wrapped_lines(&line, 62, limit);
        text_lines(&mut svg, &lines, footer_y, 22, "footer", "small");
        footer_y += lines.len() * 22 + 5;
    }
    debug_assert!(footer_y <= 1114);
    svg.push_str("</svg>");
    if svg.len() > MAX_DESIGN_FIGURE_BYTES {
        return Err(invalid(
            "figure.bytes",
            "Drawing exceeds 8 MiB; no content was silently omitted",
        ));
    }
    Ok(DesignFigureProduct {
        filename: format!("design-{}-{}.svg", frame.id, side.to_ascii_lowercase()),
        frame_id: frame.id.clone(),
        side: side.into(),
        svg_sha256: crate::sha256(svg.as_bytes()),
        svg,
        analytical_edge_count: count,
        scale_bar_m: bar,
        scale_svg_units_per_m: scale,
        displayed_label_count: displayed_labels,
        omitted_label_count: label_count - displayed_labels,
    })
}

pub fn paired_design_figures(request: &PairedDesignFigureRequest) -> Result<PairedDesignFigures> {
    paired_design_figures_with_annotations(request, false)
}

pub(crate) fn paired_design_figures_with_annotations(
    request: &PairedDesignFigureRequest,
    scoped_annotations: bool,
) -> Result<PairedDesignFigures> {
    if request.schema_version != DESIGN_FIGURE_SCHEMA {
        return Err(invalid(
            "figure.schema",
            "Unsupported paired figure contract",
        ));
    }
    text("figure.scope", &request.scope)?;
    if let Some(n) = &request.north_declaration {
        text("figure.north", n)?;
    }
    let ah = request.baseline.validate("A")?;
    let bh = request.proposed.validate("B")?;
    if request.frames.len() < 2
        || request.frames.len() > 4
        || !request
            .frames
            .iter()
            .any(|f| f.kind == DesignViewKind::Axonometric)
        || !request
            .frames
            .iter()
            .any(|f| f.kind != DesignViewKind::Axonometric)
    {
        return Err(invalid(
            "figure.frames",
            "Use two to four frames including one axonometric and one plan/elevation",
        ));
    }
    let mut ids = BTreeSet::new();
    for f in &request.frames {
        axes(f)?;
        if !ids.insert(&f.id) {
            return Err(invalid("figure.frames", "Duplicate frame ID"));
        }
    }
    let a = thermal_scene(&request.baseline.model, &request.baseline.calendar)?;
    let b = thermal_scene(&request.proposed.model, &request.proposed.calendar)?;
    let mut changes = object_changes(&request.baseline.model, &request.proposed.model, &a, &b)?;
    for (id, before, after) in [
        (
            "study:calendar",
            serde_json::to_value(&request.baseline.calendar)?,
            serde_json::to_value(&request.proposed.calendar)?,
        ),
        (
            "study:configuration",
            serde_json::to_value(&request.baseline.study)?,
            serde_json::to_value(&request.proposed.study)?,
        ),
    ] {
        if before != after {
            changes.push(DesignObjectChange {
                object_id: id.into(),
                kind: DesignObjectChangeKind::PropertiesModified,
                before_geometry_sha256: None,
                after_geometry_sha256: None,
                before: Some(before),
                after: Some(after),
            });
        }
    }
    if serde_json::to_vec(&changes)?.len() > MAX_DESIGN_CHANGE_BYTES {
        return Err(invalid(
            "figure.changes",
            "Complete change register exceeds 16 MiB",
        ));
    }
    let comparison = match (&request.baseline.study, &request.proposed.study) {
        (Some(sa), Some(sb)) => Some(compare_thermal_inputs_v1(
            &request.baseline.model,
            sa,
            &request.proposed.model,
            sb,
        )?),
        _ => None,
    };
    let comparable_geometry_weather_calendar = comparison
        .as_ref()
        .is_some_and(|c| c.comparable_geometry_weather_calendar);
    let unchanged_service_inputs = comparison.as_ref().map(|c| c.unchanged_service_inputs);
    let mut products = vec![];
    for frame in &request.frames {
        for side in ["A", "B", "overlay"] {
            products.push(svg_product(
                frame,
                side,
                &a,
                &b,
                request,
                &changes,
                scoped_annotations,
            )?);
        }
    }
    let mut value=PairedDesignFigures {schema_version:DESIGN_FIGURE_SCHEMA.into(),method:DESIGN_FIGURE_METHOD.into(),request:request.clone(),baseline_model_sha256:ah,proposed_model_sha256:bh,change_count:changes.len(),comparable_geometry_weather_calendar,unchanged_service_inputs,changes,products,limitations:vec!["Analytical-only geometry. Imported reference context is not supplied by this contract; source appearance is not thermal equivalence.".into(),"Complete analytical outlines are projected without hidden-surface removal or filled section cuts. SVG labels are bounded excerpts; exact IDs and all change snapshots remain in the accompanying JSON.".into(),"Colours show input changes, not spatial energy, temperature, daylight or CFD results. Property-only and rename-only edits do not fabricate changed geometry.".into(),"Run integrity is checked by the shared evidence service, not by deserializing this drawing. Numerical sensitivity and physical qualification remain distinct.".into()],content_sha256:String::new()};
    value.content_sha256 = canonical_hash(&(DESIGN_FIGURE_SCHEMA, &value))?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> PairedDesignFigureRequest {
        let m = synthetic_four_zone_model(false);
        let c = synthetic_energy_calendar();
        let case = |id: &str| DesignFigureCase {
            design_id: id.into(),
            label: format!("Synthetic {id}"),
            model: m.clone(),
            calendar: c.clone(),
            study: None,
            scope_revision_sha256: None,
            evidence: DesignFigureEvidence::NotSimulated {
                reason: "Original geometry-only test, never engine evidence".into(),
            },
        };
        PairedDesignFigureRequest {
            schema_version: DESIGN_FIGURE_SCHEMA.into(),
            baseline: case("A"),
            proposed: case("B"),
            frames: default_design_frames(&m, &c, &m, &c).unwrap(),
            scope: "Original four-zone analytical geometry; no architectural equivalence claim"
                .into(),
            north_declaration: None,
        }
    }
    #[test]
    fn unchanged_cop_and_rename_never_fabricate_geometry() {
        let mut r = request();
        r.proposed.model.zones[0]
            .cooling_system
            .as_mut()
            .unwrap()
            .gross_rated_cop += 0.2;
        r.proposed.model.zones[0].name = "Renamed only".into();
        let f = paired_design_figures(&r).unwrap();
        assert!(!f.changes.is_empty());
        assert!(f
            .changes
            .iter()
            .all(|c| c.before_geometry_sha256 == c.after_geometry_sha256));
        assert!(f.changes.iter().all(|c| matches!(
            c.kind,
            DesignObjectChangeKind::PropertiesModified | DesignObjectChangeKind::LabelOnly
        )));
        assert!(f.products.iter().all(|p| p.svg.contains("NOT SIMULATED")));
    }
    #[test]
    fn additions_removals_and_resize_have_stable_complete_rows() {
        let mut r = request();
        let old = r.proposed.model.zones[0].windows.remove(0);
        r.proposed.model.zones[1].windows[0].width_m -= 0.2;
        r.proposed.model.detached_shading.push(ThermalBoxShade {
            id: "new_shade".into(),
            origin_m: [-2., -3., 0.],
            size_m: [1., 1., 3.],
        });
        let f = paired_design_figures(&r).unwrap();
        assert_eq!(f.change_count, f.changes.len());
        assert!(f
            .changes
            .iter()
            .any(|c| c.object_id == old.id && c.kind == DesignObjectChangeKind::Removed));
        assert!(f
            .changes
            .iter()
            .any(|c| c.object_id == "new_shade" && c.kind == DesignObjectChangeKind::Added));
        assert!(f
            .changes
            .iter()
            .any(|c| c.kind == DesignObjectChangeKind::GeometryModified));
    }
    #[test]
    fn exact_shared_frame_scale_section_and_determinism() {
        let mut r = request();
        r.frames[1].section = Some(ReferenceReportSection {
            axis: ReferenceSectionAxis::Z,
            offset_m: 1.5,
            keep: ReferenceSectionKeep::Below,
        });
        let f = paired_design_figures(&r).unwrap();
        assert_eq!(f, paired_design_figures(&r).unwrap());
        for products in f.products.chunks(3) {
            assert_eq!(
                products[0].scale_svg_units_per_m,
                products[1].scale_svg_units_per_m
            );
            assert_eq!(
                products[1].scale_svg_units_per_m,
                products[2].scale_svg_units_per_m
            );
            assert_eq!(products[0].scale_bar_m, products[1].scale_bar_m);
        }
        assert!(f.products[3].svg.contains("keep Below"));
        assert_eq!(r.baseline.model, r.proposed.model);
    }
    #[test]
    fn invalid_frame_and_false_evidence_fail_closed() {
        let mut r = request();
        r.frames[1].eye_m = r.frames[1].target_m;
        assert!(paired_design_figures(&r).is_err());
        let mut r = request();
        let mut report = synthetic_energy_report().unwrap();
        // The arithmetic fixture uses "baseline"/"proposed" case labels; this
        // contract requires explicit A/B design targets. Its execution stays
        // NotExecuted and integrity NotChecked: this is not native evidence.
        report.baseline.id = "A".into();
        r.baseline = DesignFigureCase::from_report_case(&report.baseline, None).unwrap();
        r.baseline.model.zones[0]
            .cooling_system
            .as_mut()
            .unwrap()
            .gross_rated_cop += 0.2;
        assert!(paired_design_figures(&r)
            .unwrap_err()
            .to_string()
            .contains("differs from the depicted"));
    }
    #[test]
    fn labels_are_escaped_and_products_preserve_exact_input_identity() {
        let mut r = request();
        r.frames[0].title = "<unsafe & title>".into();
        let before = canonical_hash(&r).unwrap();
        let f = paired_design_figures(&r).unwrap();
        assert!(f.products[0].svg.contains("&lt;unsafe &amp; title&gt;"));
        assert_eq!(before, canonical_hash(&r).unwrap());
        assert!(f
            .products
            .iter()
            .all(|p| p.svg_sha256 == crate::sha256(p.svg.as_bytes())));
        assert_eq!(f.products.len(), 6);
    }
    #[test]
    fn section_excludes_upper_objects_and_narrow_frame_has_explicit_labels() {
        let mut r = request();
        r.frames[1].section = Some(ReferenceReportSection {
            axis: ReferenceSectionAxis::Z,
            offset_m: 1.5,
            keep: ReferenceSectionKeep::Below,
        });
        r.frames[1].aspect_ratio = 0.25;
        let f = paired_design_figures(&r).unwrap();
        for zone in r.baseline.model.zones.iter().filter(|z| z.storey == 2) {
            for window in &zone.windows {
                assert!(!f.products[3]
                    .svg
                    .contains(&format!("data-object=\"{}\"", window.id)));
            }
        }
        assert_eq!(f.products[3].displayed_label_count, 0);
        assert!(f.products[3].svg.contains("complete IDs in JSON"));
        let scale =
            (1024. / (r.frames[1].vertical_span_m * 0.25)).min(440. / r.frames[1].vertical_span_m);
        assert_eq!(f.products[3].scale_svg_units_per_m, scale);
        r.frames[1].section.as_mut().unwrap().offset_m = -1.;
        let empty = paired_design_figures(&r).unwrap();
        assert_eq!(empty.products[3].analytical_edge_count, 0);
        assert!(empty.products[3]
            .svg
            .contains("No analytical edges intersect"));
    }

    #[test]
    fn readable_type_and_long_declarations_stay_in_the_page_without_rebinding_inputs() {
        let mut r = request();
        r.frames[0].title = "Long title 樓層 and WIDE CHARACTERS ".repeat(8);
        r.scope =
            "Analytical scope 包括明確界定的空間 with a long explanatory declaration. ".repeat(12);
        r.north_declaration = Some("Illustrative model orientation only 北向尚未實測. ".repeat(12));
        let before = canonical_hash(&r).unwrap();
        let f = paired_design_figures(&r).unwrap();
        assert_eq!(canonical_hash(&r).unwrap(), before);
        assert_eq!(f.request.scope, r.scope);
        assert_eq!(f.request.north_declaration, r.north_declaration);
        for p in &f.products {
            assert!(p.svg.contains(".gl-design-small{font-size:17px}"));
            assert!(p.svg.contains(".gl-design-body{font-size:19px}"));
            assert!(p
                .svg
                .contains("font-family=\"Noto Sans CJK TC,sans-serif\""));
            assert!(!p.svg.contains("<style>text{"));
            assert!(!p.svg.contains("class=\"small\""));
            assert!(p.svg.contains("viewBox=\"0 0 1200 1140\""));
            assert!(p.svg.contains("[full text in JSON]"));
            assert!(p.svg.matches("data-role=\"footer\"").count() <= 12);
            for text in p.svg.split("<text ").skip(1) {
                let y: f64 = text
                    .split(" y=\"")
                    .nth(1)
                    .unwrap()
                    .split('"')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap();
                assert!((17.0..=1114.0).contains(&y));
            }
        }
        for value in [&r.scope, r.north_declaration.as_ref().unwrap()] {
            let lines = wrapped_lines(value, 62, 3);
            assert_eq!(lines.len(), 3);
            assert!(lines.iter().all(|line| line.chars().count() <= 62));
            assert!(lines.last().unwrap().ends_with("[full text in JSON]"));
        }
        // Shared world extent and scale still match for A/B/overlay after the
        // page-space typography/layout change. No model/section is recaptured.
        for group in f.products.chunks(3) {
            assert_eq!(
                group[0].scale_svg_units_per_m,
                group[1].scale_svg_units_per_m
            );
            assert_eq!(
                group[0].scale_svg_units_per_m,
                group[2].scale_svg_units_per_m
            );
        }
    }
}
