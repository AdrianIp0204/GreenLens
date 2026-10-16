//! Version 2 aligned, retained-snapshot drawings. Version 1 remains unchanged.
//! All admitted triangles are examined before bounded view-aware reduction.
//! No reference content is executable and no drawing implies physical fields.
use anyhow::{ensure, Result};
use greenlens_core::{
    architectural_scope::{
        validate_declaration, ArchitecturalScope, MappingStatus, ReferenceReportView,
        ReferenceSectionAxis, ReferenceSectionKeep, ARCHITECTURAL_SCOPE_SCHEMA,
    },
    canonical_hash,
    energy::{thermal_scene, EnergyCalendar, ThermalModel, ThermalScene},
    reference::{validate_manifest, ReferenceModel, ReferenceTriangle, MAX_TRIANGLES},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub const ALIGNED_FIGURE_SCHEMA: &str = "greenlens.aligned-reference-figure.v2";
pub const ALIGNED_FIGURE_METHOD: &str = "greenlens.reference-analytical-orthographic/2";
pub const MAX_VISIBLE_TRIANGLES: usize = 8_000;
pub const MAX_ANALYTICAL_EDGES: usize = 16_384;
pub const MAX_FIGURE_BYTES: usize = 8 * 1024 * 1024;
pub const FIGURE_DEADLINE_SECONDS: u64 = 30;
pub const MAX_FOCUS_OCCURRENCES: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AlignedFigureRequest {
    pub schema_version: String,
    /// Immutable revision identity owned by the study revision service.
    pub scope_revision_sha256: String,
    /// Hash of (greenlens.architectural-scope.v1, the exact scope snapshot).
    pub scope_content_sha256: String,
    pub selected_run_id: String,
    pub selected_record_sha256: String,
    pub view: ReferenceReportView,
    /// Revision-scoped occurrence IDs, never labels. Descendants are included.
    pub focus_occurrence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AlignedFigureDefinition {
    pub schema_version: String,
    pub method: String,
    pub request: AlignedFigureRequest,
    pub source_triangle_count: usize,
    pub visible_triangle_count: usize,
    pub depicted_triangle_count: usize,
    pub omitted_visible_triangle_count: usize,
    pub protected_visible_triangle_count: usize,
    pub represented_occurrence_ids: Vec<String>,
    pub analytical_visible_object_ids: Vec<String>,
    pub analytical_edge_count: usize,
    pub figure_labels: BTreeMap<String, String>,
    pub scale_bar_m: Option<f64>,
    pub caption: String,
    pub limitations: Vec<String>,
    pub unavailable_reason: Option<String>,
    pub svg_sha256: String,
    pub definition_sha256: String,
}
impl AlignedFigureDefinition {
    pub fn identity(&self) -> Result<String> {
        let mut unsigned = self.clone();
        unsigned.definition_sha256.clear();
        Ok(canonical_hash(&(ALIGNED_FIGURE_SCHEMA, unsigned))?)
    }
}

/// SVG bytes are constructed here, never deserialized from an imported recipe.
#[derive(Debug)]
pub struct AlignedFigure {
    pub definition: AlignedFigureDefinition,
    pub svg: Vec<u8>,
}

type Point = [f64; 3];
type Edge = [[f64; 2]; 2];
fn sub(a: Point, b: Point) -> Point {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: Point, b: Point) -> f64 {
    a.into_iter().zip(b).map(|(x, y)| x * y).sum()
}
fn unit(a: Point) -> Point {
    let n = dot(a, a).sqrt();
    a.map(|v| v / n)
}
fn cross(a: Point, b: Point) -> Point {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn clip(a: &mut Point, b: &mut Point, n: Point, offset: f64) -> bool {
    let da = dot(*a, n) - offset;
    let db = dot(*b, n) - offset;
    if da < 0. && db < 0. {
        return false;
    }
    if da < 0. || db < 0. {
        let t = da / (da - db);
        let p = std::array::from_fn(|i| a[i] + t * (b[i] - a[i]));
        if da < 0. {
            *a = p
        } else {
            *b = p
        }
    }
    true
}

struct Frame {
    right: Point,
    up: Point,
    target: Point,
    width: f64,
    height: f64,
    scale: f64,
    left: f64,
    top: f64,
    section: Option<(Point, f64)>,
}
impl Frame {
    fn new(view: &ReferenceReportView) -> Result<Self> {
        let f = sub(view.target_m, view.eye_m);
        ensure!(
            view.eye_m
                .into_iter()
                .chain(view.target_m)
                .chain(view.up)
                .all(f64::is_finite)
                && dot(f, f) > 1e-18
                && dot(cross(f, view.up), cross(f, view.up)) > 1e-18
                && view.vertical_span_m.is_finite()
                && (1e-6..=1e9).contains(&view.vertical_span_m)
                && view.aspect_ratio.is_finite()
                && (0.25..=4.).contains(&view.aspect_ratio),
            "Invalid retained figure camera/frame"
        );
        let forward = unit(f);
        let right = unit(cross(forward, view.up));
        let up = unit(cross(right, forward));
        let height = view.vertical_span_m;
        let width = height * view.aspect_ratio;
        let scale = (800. / width).min(400. / height);
        let section = view.section.as_ref().map(|s| {
            let axis = match s.axis {
                ReferenceSectionAxis::X => 0,
                ReferenceSectionAxis::Y => 1,
                ReferenceSectionAxis::Z => 2,
            };
            let sign = if s.keep == ReferenceSectionKeep::Above {
                1.
            } else {
                -1.
            };
            let mut normal = [0.; 3];
            normal[axis] = sign;
            (normal, sign * s.offset_m)
        });
        ensure!(
            section.is_none_or(|(_, v)| v.is_finite() && v.abs() <= 1e9),
            "Invalid retained section"
        );
        Ok(Self {
            right,
            up,
            target: view.target_m,
            width,
            height,
            scale,
            left: 80. + (800. - width * scale) / 2.,
            top: 104. + (400. - height * scale) / 2.,
            section,
        })
    }
    fn project(&self, p: Point) -> [f64; 2] {
        let d = sub(p, self.target);
        [
            self.left + (dot(d, self.right) + self.width / 2.) * self.scale,
            self.top + (self.height / 2. - dot(d, self.up)) * self.scale,
        ]
    }
    fn edge(&self, mut a: Point, mut b: Point) -> Option<Edge> {
        if self
            .section
            .is_some_and(|(n, d)| !clip(&mut a, &mut b, n, d))
        {
            return None;
        }
        for (n, d) in [
            (self.right, dot(self.target, self.right) - self.width / 2.),
            (
                self.right.map(|v| -v),
                -dot(self.target, self.right) - self.width / 2.,
            ),
            (self.up, dot(self.target, self.up) - self.height / 2.),
            (
                self.up.map(|v| -v),
                -dot(self.target, self.up) - self.height / 2.,
            ),
        ] {
            if !clip(&mut a, &mut b, n, d) {
                return None;
            }
        }
        let edge = [self.project(a), self.project(b)];
        // Three-decimal SVG precision defines visible original edges. This is
        // presentation precision only; admitted scientific coordinates persist.
        if (0..2).all(|i| (edge[0][i] * 1000.).round() == (edge[1][i] * 1000.).round()) {
            None
        } else {
            Some(edge)
        }
    }
}

#[derive(Clone)]
struct VisibleTriangle {
    ordinal: usize,
    occurrence: usize,
    edges: Vec<Edge>,
}
fn hash_text(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}
fn bounded_text(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
/// Integer mixer is only a deterministic illustration selection order, not an
/// integrity digest or a statistical estimator. All mandatory triangles win.
fn rank(ordinal: usize) -> u64 {
    let mut v = (ordinal as u64).wrapping_add(0x9e3779b97f4a7c15);
    v = (v ^ (v >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    v = (v ^ (v >> 27)).wrapping_mul(0x94d049bb133111eb);
    v ^ (v >> 31)
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn short(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.into()
    } else {
        format!(
            "{}…",
            value
                .chars()
                .take(max.saturating_sub(1))
                .collect::<String>()
        )
    }
}
fn text(out: &mut String, x: f64, y: f64, size: usize, value: &str) {
    out.push_str(&format!("<text x=\"{x:.3}\" y=\"{y:.3}\" font-family=\"sans-serif\" font-size=\"{size}\" fill=\"#27363d\">{}</text>", escape(value)));
}
fn path(out: &mut String, edge: Edge, color: &str, width: f64, dash: bool) {
    out.push_str(&format!("<path d=\"M{:.3} {:.3}L{:.3} {:.3}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"{width}\"{}/>",
        edge[0][0], edge[0][1], edge[1][0], edge[1][1], if dash { " stroke-dasharray=\"4 3\"" } else { "" }));
}

/// Supply exactly the complete, integrity-checked admitted chunk sequence.
/// The caller verifies chunk byte hashes; this function additionally validates
/// stream dimensions and occurrence accounting. Caller-selected thermal inputs
/// must be the retained run snapshot, not the currently editable design.
pub fn render<I>(
    reference: &ReferenceModel,
    scope: &ArchitecturalScope,
    model_snapshot: &ThermalModel,
    calendar: &EnergyCalendar,
    request: &AlignedFigureRequest,
    triangles: I,
    cancel: &AtomicBool,
) -> Result<AlignedFigure>
where
    I: IntoIterator<Item = Result<ReferenceTriangle>>,
{
    let started = Instant::now();
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Aligned report figure cancelled"
    );
    validate_manifest(reference)?;
    validate_declaration(scope)?;
    ensure!(
        request.schema_version == ALIGNED_FIGURE_SCHEMA,
        "Unsupported aligned figure contract"
    );
    ensure!(
        hash_text(&request.scope_revision_sha256)
            && hash_text(&request.selected_record_sha256)
            && bounded_text(&request.selected_run_id, 128),
        "Invalid scope revision or selected run binding"
    );
    ensure!(
        request.scope_content_sha256 == canonical_hash(&(ARCHITECTURAL_SCOPE_SCHEMA, scope))?,
        "Figure scope snapshot identity differs"
    );
    let view = &request.view;
    ensure!(
        scope.report_views.iter().any(|v| v == view),
        "Figure must use an exact view accepted in its scope snapshot"
    );
    ensure!(view.reference_revision_sha256 == reference.revision_sha256 && view.reference_geometry_sha256 == reference.geometry_sha256
        && scope.reference_revision_sha256 == reference.revision_sha256 && scope.reference_geometry_sha256 == reference.geometry_sha256,
        "Exact retained reference revision is unavailable; no active source substitution is allowed");
    let scene = thermal_scene(model_snapshot, calendar)?;
    ensure!(
        scene.model_sha256 == view.model_input_sha256
            && scope.design_model_sha256.get(&view.design_id) == Some(&scene.model_sha256),
        "Figure analytical snapshot differs from its selected retained case"
    );
    ensure!(
        request.focus_occurrence_ids.len() <= MAX_FOCUS_OCCURRENCES,
        "Figure focus exceeds 256 occurrence IDs"
    );
    let index: BTreeMap<_, _> = reference
        .occurrences
        .iter()
        .enumerate()
        .map(|(i, o)| (o.id.as_str(), i))
        .collect();
    let focus: BTreeSet<_> = request
        .focus_occurrence_ids
        .iter()
        .map(String::as_str)
        .collect();
    ensure!(
        focus.len() == request.focus_occurrence_ids.len()
            && focus.iter().all(|id| index.contains_key(id)),
        "Unknown or duplicate revision-scoped figure focus"
    );
    let statuses: BTreeMap<_, _> = scope
        .mappings
        .iter()
        .map(|m| (m.occurrence_id.as_str(), m.status))
        .collect();
    ensure!(
        statuses.keys().all(|id| index.contains_key(id)),
        "Scope mapping belongs to a different reference occurrence"
    );
    let protected: Vec<bool> = reference
        .occurrences
        .iter()
        .map(|o| {
            let mut id = Some(o.id.as_str());
            while let Some(current) = id {
                if focus.contains(current) || statuses.get(current) == Some(&MappingStatus::Mapped)
                {
                    return true;
                }
                id = reference.occurrences[index[current]].parent_id.as_deref();
            }
            false
        })
        .collect();
    let frame = Frame::new(view)?;
    let mut mandatory = Vec::new();
    let mut representatives = BTreeMap::new();
    let mut extras = BTreeMap::new();
    let mut occurrence_counts = vec![0usize; reference.occurrences.len()];
    let (mut read, mut visible, mut protected_visible) = (0usize, 0usize, 0usize);
    let mut overflow = false;
    for triangle in triangles {
        if read.is_multiple_of(256) {
            ensure!(
                !cancel.load(Ordering::Relaxed),
                "Aligned report figure cancelled"
            );
            ensure!(
                started.elapsed() <= Duration::from_secs(FIGURE_DEADLINE_SECONDS),
                "Aligned report figure exceeded its 30 second limit"
            );
        }
        let triangle = triangle?;
        ensure!(
            read < reference.triangle_count && read < MAX_TRIANGLES,
            "Reference stream exceeds admitted shape"
        );
        let occurrence = triangle.occurrence_index as usize;
        ensure!(
            occurrence < occurrence_counts.len()
                && triangle.vertices_m.iter().flatten().all(|v| v.is_finite()),
            "Invalid reference triangle"
        );
        occurrence_counts[occurrence] += 1;
        let edges: Vec<_> = [(0, 1), (1, 2), (2, 0)]
            .into_iter()
            .filter_map(|(a, b)| frame.edge(triangle.vertices_m[a], triangle.vertices_m[b]))
            .collect();
        if !edges.is_empty() {
            visible += 1;
            let item = VisibleTriangle {
                ordinal: read,
                occurrence,
                edges,
            };
            if protected[occurrence] {
                protected_visible += 1;
                if mandatory.len() < MAX_VISIBLE_TRIANGLES {
                    mandatory.push(item)
                } else {
                    overflow = true
                }
            } else {
                let room = representatives.len() < MAX_VISIBLE_TRIANGLES;
                match representatives.entry(occurrence) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        // One per admitted occurrence, capped before retaining
                        // drawing buffers for an already unavailable view.
                        if room {
                            entry.insert(item);
                        } else {
                            overflow = true
                        }
                    }
                    std::collections::btree_map::Entry::Occupied(_) => {
                        extras.insert((rank(read), read), item);
                        if extras.len() > MAX_VISIBLE_TRIANGLES {
                            extras.pop_last();
                        }
                    }
                }
            }
        }
        read += 1;
    }
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Aligned report figure cancelled"
    );
    ensure!(
        read == reference.triangle_count
            && occurrence_counts
                .iter()
                .zip(&reference.occurrences)
                .all(|(n, o)| *n == o.triangle_count),
        "Incomplete reference stream or changed occurrence ordering/counts"
    );
    let unavailable = if overflow || mandatory.len() + representatives.len() > MAX_VISIBLE_TRIANGLES
    {
        Some("This frame exceeds the 8,000-triangle drawing limit while preserving all visible mapped/focused triangles and one representative per remaining visible occurrence. Choose a narrower retained frame or split the drawing; no reduced substitute is depicted.".to_owned())
    } else if visible == 0 {
        Some("No original reference edges are visible in this exact frame/section at drawing precision. Choose another retained view; an analytical-only close-up must not imply that reference geometry was shown.".to_owned())
    } else {
        None
    };
    let mut selected = mandatory;
    selected.extend(representatives.into_values());
    let remaining = MAX_VISIBLE_TRIANGLES.saturating_sub(selected.len());
    selected.extend(extras.into_values().take(remaining));
    selected.sort_by_key(|t| t.ordinal);
    if unavailable.is_some() {
        selected.clear()
    }
    let mut result = draw(
        reference,
        scope,
        &scene,
        request,
        &frame,
        &selected,
        unavailable,
    )?;
    result.definition.source_triangle_count = read;
    result.definition.visible_triangle_count = visible;
    result.definition.protected_visible_triangle_count = protected_visible;
    result.definition.omitted_visible_triangle_count = visible - selected.len();
    result.definition.caption.push_str(&format!(" All {read} admitted triangles were examined; {visible} have visible original edges, {} are depicted. {} visible triangles are omitted only from this illustration. Full geometry remains retained.", selected.len(), visible - selected.len()));
    let mut svg = String::from_utf8(result.svg)?;
    svg.truncate(svg.len() - "</svg>".len());
    text(&mut svg, 32., 790., 11, &format!("Reference: {} / {visible} visible triangles depicted (all {read} checked). Analytical: {} visible objects.", selected.len(), result.definition.analytical_visible_object_ids.len()));
    svg.push_str("</svg>");
    result.svg = svg.into_bytes();
    result.definition.svg_sha256 = greenlens_core::sha256(&result.svg);
    result.definition.definition_sha256 = result.definition.identity()?;
    ensure!(
        result.svg.len() <= MAX_FIGURE_BYTES,
        "Aligned SVG exceeds 8 MiB"
    );
    ensure!(
        !cancel.load(Ordering::Relaxed)
            && started.elapsed() <= Duration::from_secs(FIGURE_DEADLINE_SECONDS),
        "Aligned report figure cancelled or exceeded its 30 second limit"
    );
    Ok(result)
}

fn draw(
    reference: &ReferenceModel,
    scope: &ArchitecturalScope,
    scene: &ThermalScene,
    request: &AlignedFigureRequest,
    frame: &Frame,
    triangles: &[VisibleTriangle],
    unavailable: Option<String>,
) -> Result<AlignedFigure> {
    let view = &request.view;
    let caption = format!("Aligned reference and analytical snapshot; source revision {}; analytical design {} / {}; selected run {}; scope revision {}. Exact orthographic frame and section apply to both layers. No geometry equivalence, whole-building coverage or physical validation is implied.", reference.revision_sha256, view.design_id, scene.model_sha256, request.selected_run_id, request.scope_revision_sha256);
    let limitations = vec!["Original edges are clipped; filled cut faces and hidden-line removal are not implemented.".into(),
        "Line drawing only: zone totals are not resolved spatial thermal fields.".into(),
        "Reference materials do not supply thermal properties. Coverage and approximations remain declared scope information.".into(),
        "Short drawing labels key to complete stable object IDs in the figure definition; names are not identity.".into()];
    let mut definition = AlignedFigureDefinition {
        schema_version: ALIGNED_FIGURE_SCHEMA.into(),
        method: ALIGNED_FIGURE_METHOD.into(),
        request: request.clone(),
        source_triangle_count: 0,
        visible_triangle_count: 0,
        depicted_triangle_count: triangles.len(),
        omitted_visible_triangle_count: 0,
        protected_visible_triangle_count: 0,
        represented_occurrence_ids: triangles
            .iter()
            .map(|t| reference.occurrences[t.occurrence].id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        analytical_visible_object_ids: vec![],
        analytical_edge_count: 0,
        figure_labels: BTreeMap::new(),
        scale_bar_m: None,
        caption,
        limitations,
        unavailable_reason: unavailable.clone(),
        svg_sha256: String::new(),
        definition_sha256: String::new(),
    };
    let mut out = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"960\" height=\"820\" viewBox=\"0 0 960 820\"><rect width=\"960\" height=\"820\" fill=\"white\"/>".to_owned();
    text(&mut out, 32., 30., 18, &short(&view.title, 85));
    text(
        &mut out,
        32.,
        54.,
        12,
        "Aligned reference / analytical snapshot · orthographic · world coordinates in metres",
    );
    text(
        &mut out,
        32.,
        76.,
        12,
        &format!(
            "Frame {:.4} × {:.4} m; section {}",
            frame.width,
            frame.height,
            view.section
                .as_ref()
                .map(|s| format!("{:?} {:?} {:.4} m", s.axis, s.keep, s.offset_m))
                .unwrap_or_else(|| "none".into())
        ),
    );
    if let Some(reason) = unavailable {
        text(
            &mut out,
            32.,
            120.,
            17,
            "Retained aligned figure unavailable",
        );
        for (i, part) in reason.chars().collect::<Vec<_>>().chunks(105).enumerate() {
            text(
                &mut out,
                32.,
                155. + i as f64 * 22.,
                13,
                &part.iter().collect::<String>(),
            );
        }
        text(&mut out, 32., 310., 13, "No geometry or scale bar is depicted; the exact requested definition remains retained.");
    } else {
        out.push_str(&format!("<rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"none\" stroke=\"#bac4c8\"/>", frame.left, frame.top, frame.width*frame.scale, frame.height*frame.scale));
        let statuses: BTreeMap<_, _> = scope
            .mappings
            .iter()
            .map(|m| (m.occurrence_id.as_str(), m.status))
            .collect();
        out.push_str("<g id=\"reference-layer\">");
        for triangle in triangles {
            let id = &reference.occurrences[triangle.occurrence].id;
            let color = match statuses.get(id.as_str()) {
                Some(MappingStatus::Mapped) => "#367562",
                Some(MappingStatus::ContextOnly) => "#879194",
                Some(MappingStatus::Excluded) => "#a65c53",
                _ => "#a18340",
            };
            out.push_str(&format!("<g data-occurrence=\"{}\">", escape(id)));
            for edge in &triangle.edges {
                path(&mut out, *edge, color, 0.65, false)
            }
            out.push_str("</g>");
        }
        out.push_str("</g><g id=\"analytical-layer\">");
        let mut labels = Vec::new();
        let mut object_number = BTreeMap::<&str, usize>::new();
        for object in &scene.objects {
            let mut drawn = Vec::new();
            for polygon in &object.polygons_m {
                for (a, b) in [(0, 1), (1, 2), (2, 3), (3, 0)] {
                    if let Some(edge) = frame.edge(polygon[a], polygon[b]) {
                        definition.analytical_edge_count += 1;
                        ensure!(
                            definition.analytical_edge_count <= MAX_ANALYTICAL_EDGES,
                            "Analytical outlines exceed figure limit; narrow the retained view"
                        );
                        drawn.push(edge);
                    }
                }
            }
            if drawn.is_empty() {
                continue;
            }
            definition
                .analytical_visible_object_ids
                .push(object.id.clone());
            let is_opening = object.kind == "window";
            let color = if is_opening { "#753d88" } else { "#244e80" };
            out.push_str(&format!("<g data-object=\"{}\">", escape(&object.id)));
            for edge in &drawn {
                path(
                    &mut out,
                    *edge,
                    color,
                    if is_opening { 1.8 } else { 1.25 },
                    true,
                )
            }
            out.push_str("</g>");
            if is_opening || object.kind == "overhang" || object.kind == "shading" {
                let prefix = match object.kind.as_str() {
                    "window" => "W",
                    "overhang" => "H",
                    _ => "C",
                };
                let n = object_number.entry(prefix).or_default();
                *n += 1;
                let label = format!("{prefix}{n}");
                definition
                    .figure_labels
                    .insert(label.clone(), object.id.clone());
                labels.push((label, drawn[0][0]));
            }
        }
        // Zone labels are anchored on the first visible edge of that zone's
        // envelope rather than a centroid outside the clipped frame.
        for (i, zone) in scene
            .objects
            .iter()
            .filter(|o| o.kind == "zone")
            .enumerate()
        {
            let anchor = scene
                .objects
                .iter()
                .filter(|o| o.kind == "surface" && o.zone_id.as_ref() == Some(&zone.id))
                .flat_map(|o| &o.polygons_m)
                .flat_map(|p| [(p[0], p[1]), (p[1], p[2]), (p[2], p[3]), (p[3], p[0])])
                .find_map(|(a, b)| frame.edge(a, b).map(|e| e[0]));
            if let Some(point) = anchor {
                let label = format!("Z{}", i + 1);
                definition
                    .figure_labels
                    .insert(label.clone(), zone.id.clone());
                labels.push((label, point));
                definition
                    .analytical_visible_object_ids
                    .push(zone.id.clone());
            }
        }
        out.push_str("</g>");
        let mut occupied = Vec::<[f64; 4]>::new();
        for (label, anchor) in &labels {
            let width = 10. + label.len() as f64 * 7.;
            let mut found = None;
            for ring in 0..10 {
                for direction in [1., -1.] {
                    let x = (anchor[0] + 5. + ring as f64 * 11.).clamp(
                        frame.left + 2.,
                        frame.left + frame.width * frame.scale - width - 2.,
                    );
                    let y = (anchor[1] + direction * (12. + ring as f64 * 15.))
                        .clamp(frame.top + 14., frame.top + frame.height * frame.scale - 3.);
                    let b = [x, y - 12., x + width, y + 3.];
                    if occupied.iter().all(|o| {
                        b[2] + 2. < o[0] || b[0] > o[2] + 2. || b[3] + 2. < o[1] || b[1] > o[3] + 2.
                    }) {
                        found = Some((x, y, b));
                        break;
                    }
                }
                if found.is_some() {
                    break;
                }
            }
            let Some((x, y, b)) = found else {
                anyhow::bail!("Analytical labels cannot be placed legibly in this retained frame; split the figure into narrower views")
            };
            occupied.push(b);
            path(&mut out, [*anchor, [x, y - 5.]], "#586c79", 0.65, false);
            out.push_str(&format!("<rect x=\"{x:.3}\" y=\"{:.3}\" width=\"{width:.3}\" height=\"16\" fill=\"white\" fill-opacity=\"0.95\"/>",y-12.));
            text(&mut out, x + 3., y, 12, label);
        }
        let bar = 10f64.powf((frame.width / 5.).log10().floor());
        definition.scale_bar_m = Some(bar);
        path(
            &mut out,
            [[80., 526.], [80. + bar * frame.scale, 526.]],
            "#233e49",
            3.,
            false,
        );
        text(
            &mut out,
            80.,
            548.,
            12,
            &format!("{bar} m in the view plane · original edges clipped; no filled cut faces"),
        );
    }
    text(&mut out,32.,574.,12,"Reference: solid thin lines. Analytical: dashed blue envelope / purple openings; Z zone, W window, H overhang.");
    text(&mut out,32.,595.,12,"Reference mapping: green mapped · grey context · red excluded · amber unresolved. No thermal field is shown.");
    text(&mut out,32.,616.,12,&reference.options.declared_north_clockwise_deg.map(|v|format!("Declared true north {v}° clockwise; projection axes follow the retained camera.")).unwrap_or_else(||"True north unknown. Camera/project axes do not establish geographical orientation.".into()));
    text(&mut out,32.,637.,12,&format!("Scope: {:?}. Coverage: {}. Read the accepted scope for approximation and exclusion details.",scope.scope_kind,match scope.coverage { greenlens_core::architectural_scope::ScopeCoverage::Unknown {..}=>"unknown", _=>"declared source floor-area denominator" }));
    text(
        &mut out,
        32.,
        662.,
        11,
        &format!("Source revision {}", reference.revision_sha256),
    );
    text(
        &mut out,
        32.,
        682.,
        11,
        &format!(
            "Analytical design {} · {}",
            short(&view.design_id, 20),
            scene.model_sha256
        ),
    );
    text(
        &mut out,
        32.,
        702.,
        11,
        &format!("Scope revision {}", request.scope_revision_sha256),
    );
    text(
        &mut out,
        32.,
        722.,
        11,
        &format!("Selected run {}", request.selected_run_id),
    );
    let keys = definition
        .figure_labels
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("; ");
    text(&mut out, 32., 747., 11, &short(&keys, 130));
    text(&mut out,32.,767.,11,"Complete label keys, snapshot bindings, reduction counts and limitations accompany the figure definition.");
    out.push_str("</svg>");
    definition.analytical_visible_object_ids.sort();
    definition.analytical_visible_object_ids.dedup();
    Ok(AlignedFigure {
        definition,
        svg: out.into_bytes(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use greenlens_core::{architectural_scope::*, energy::*, reference::*};
    use serde_json::json;

    // Geometry comparison tolerance fixed before evaluating this original
    // synthetic fixture: 1e-9 m or SVG-pixel coordinates, as appropriate.
    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() <= 1e-9, "{actual} != {expected}");
    }

    /// Target is exactly stream index 1: the legacy ceil(8001/8000)=2 global
    /// stride misses it. Other triangles lie far outside the retained close-up.
    fn admitted(outside_count: usize, all_inside: bool) -> AdmittedReference {
        let target = [[1f32, 0., 1.], [2., 0., 1.], [1., 0., 2.]];
        let outside = if all_inside {
            target
        } else {
            [[100., 0., 1.], [101., 0., 1.], [100., 0., 2.]]
        };
        let mut binary = Vec::new();
        for vertices in [&outside, &target, &outside] {
            for [x, y, z] in vertices {
                for value in [*x, *z, -*y] {
                    binary.extend(value.to_le_bytes());
                }
            }
        }
        let mut indices = Vec::new();
        for _ in 0..outside_count {
            indices.extend([0u32, 1, 2]);
        }
        for index in &indices {
            binary.extend(index.to_le_bytes());
        }
        let outside_min = if all_inside {
            [1., 1., 0.]
        } else {
            [100., 1., 0.]
        };
        let outside_max = if all_inside {
            [2., 2., 0.]
        } else {
            [101., 2., 0.]
        };
        let document = json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":binary.len()}],
            "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":36},{"buffer":0,"byteOffset":72,"byteLength":36},{"buffer":0,"byteOffset":108,"byteLength":indices.len()*4}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":outside_min,"max":outside_max},{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3","min":[1,1,0],"max":[2,2,0]},{"bufferView":2,"componentType":5126,"count":3,"type":"VEC3","min":outside_min,"max":outside_max},{"bufferView":3,"componentType":5125,"count":indices.len(),"type":"SCALAR"}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]},{"primitives":[{"attributes":{"POSITION":1}}]},{"primitives":[{"attributes":{"POSITION":2},"indices":3}]}],
            "nodes":[{"mesh":0,"name":"Original background"},{"mesh":1,"name":"Tiny retained target"},{"mesh":2,"name":"Repeated background"}],"scenes":[{"nodes":[0,1,2]}],"scene":0});
        let mut json = serde_json::to_vec(&document).unwrap();
        while !json.len().is_multiple_of(4) {
            json.push(b' ')
        }
        let mut bytes = Vec::new();
        bytes.extend(0x46546c67u32.to_le_bytes());
        bytes.extend(2u32.to_le_bytes());
        bytes.extend(((28 + json.len() + binary.len()) as u32).to_le_bytes());
        bytes.extend((json.len() as u32).to_le_bytes());
        bytes.extend(0x4e4f534au32.to_le_bytes());
        bytes.extend(json);
        bytes.extend((binary.len() as u32).to_le_bytes());
        bytes.extend(0x004e4942u32.to_le_bytes());
        bytes.extend(binary);
        admit_glb(&bytes, &ReferenceImportOptions::default(), |_| true).unwrap()
    }
    fn fixture(
        reference: &ReferenceModel,
    ) -> (
        ThermalModel,
        EnergyCalendar,
        ArchitecturalScope,
        AlignedFigureRequest,
    ) {
        let model = synthetic_four_zone_model(false);
        let calendar = synthetic_energy_calendar();
        let hash = validate_thermal_model(&model, &calendar)
            .unwrap()
            .model_input_sha256;
        let view = ReferenceReportView {
            schema_version: REFERENCE_REPORT_VIEW_SCHEMA.into(),
            id: "front".into(),
            title: "Synthetic close-up <scope>".into(),
            design_id: "A".into(),
            model_input_sha256: hash.clone(),
            reference_revision_sha256: reference.revision_sha256.clone(),
            reference_geometry_sha256: reference.geometry_sha256.clone(),
            eye_m: [2., -10., 1.5],
            target_m: [2., 0., 1.5],
            up: [0., 0., 1.],
            vertical_span_m: 4.,
            aspect_ratio: 1.,
            section: None,
        };
        let scope = ArchitecturalScope {
            schema_version: ARCHITECTURAL_SCOPE_SCHEMA.into(),
            title: "Original synthetic scope".into(),
            purpose: "Renderer test, not engine acceptance".into(),
            scope_kind: ScopeKind::RepresentativeStudy,
            reference_revision_sha256: reference.revision_sha256.clone(),
            reference_geometry_sha256: reference.geometry_sha256.clone(),
            reference_options_sha256: canonical_hash(&reference.options).unwrap(),
            design_model_sha256: BTreeMap::from([("A".into(), hash.clone())]),
            mappings: vec![ScopeMapping {
                occurrence_id: reference.occurrences[1].id.clone(),
                status: MappingStatus::Mapped,
                targets: vec![AnalyticalTarget {
                    design_id: "A".into(),
                    object_id: model.zones[0].id.clone(),
                    kind: AnalyticalObjectKind::Zone,
                    model_input_sha256: hash,
                }],
                rationale: "Synthetic mapping declaration only".into(),
            }],
            included_scope: vec!["Target retained in close-up".into()],
            excluded_scope: vec!["No building coverage claim".into()],
            boundary_consequences: "Existing thermal inputs are authoritative".into(),
            assumptions: vec![],
            reconciliations: vec![],
            coverage: ScopeCoverage::Unknown {
                reason: "No source-area denominator".into(),
            },
            whole_building_review: None,
            report_views: vec![view.clone()],
        };
        let request = AlignedFigureRequest {
            schema_version: ALIGNED_FIGURE_SCHEMA.into(),
            scope_revision_sha256: "d".repeat(64),
            scope_content_sha256: canonical_hash(&(ARCHITECTURAL_SCOPE_SCHEMA, &scope)).unwrap(),
            selected_run_id: "synthetic-render-fixture-not-engine-evidence".into(),
            selected_record_sha256: "e".repeat(64),
            view,
            focus_occurrence_ids: vec![reference.occurrences[1].id.clone()],
        };
        (model, calendar, scope, request)
    }
    fn chunks(admitted: &AdmittedReference) -> Vec<Result<ReferenceTriangle>> {
        admitted
            .chunks
            .iter()
            .enumerate()
            .flat_map(|(i, b)| {
                decode_chunk(&admitted.manifest, i, b)
                    .unwrap()
                    .into_iter()
                    .map(Ok)
            })
            .collect()
    }
    fn refresh(scope: &mut ArchitecturalScope, request: &mut AlignedFigureRequest) {
        scope.report_views = vec![request.view.clone()];
        request.scope_content_sha256 =
            canonical_hash(&(ARCHITECTURAL_SCOPE_SCHEMA, scope)).unwrap();
    }
    #[test]
    fn close_up_retains_target_between_old_stride_indices_with_exact_snapshot_overlay() {
        let source = admitted(7999, false);
        let (model, calendar, scope, request) = fixture(&source.manifest);
        let before = canonical_hash(&model).unwrap();
        let out = render(
            &source.manifest,
            &scope,
            &model,
            &calendar,
            &request,
            chunks(&source),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(source.manifest.triangle_count, 8001);
        assert_eq!(out.definition.visible_triangle_count, 1);
        assert_eq!(out.definition.depicted_triangle_count, 1);
        assert_eq!(out.definition.protected_visible_triangle_count, 1);
        assert_eq!(
            out.definition.represented_occurrence_ids,
            vec![source.manifest.occurrences[1].id.clone()]
        );
        assert!(out.definition.unavailable_reason.is_none());
        assert!(out.definition.analytical_edge_count > 0);
        assert!(out
            .definition
            .figure_labels
            .keys()
            .any(|k| k.starts_with('Z')));
        assert!(out
            .definition
            .figure_labels
            .keys()
            .any(|k| k.starts_with('W')));
        assert_eq!(before, canonical_hash(&model).unwrap());
        let text = String::from_utf8(out.svg.clone()).unwrap();
        for marker in [
            "reference-layer",
            "analytical-layer",
            "stroke-dasharray=\"4 3\"",
            "True north unknown",
            "&lt;scope&gt;",
            "scope revision",
        ] {
            assert!(
                text.contains(marker) || out.definition.caption.contains(marker),
                "{marker}"
            );
        }
        let again = render(
            &source.manifest,
            &scope,
            &model,
            &calendar,
            &request,
            chunks(&source),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(out.svg, again.svg);
        assert_eq!(out.definition, again.definition);
    }
    #[test]
    fn clipping_and_scale_use_one_identical_frame_for_both_layers() {
        let source = admitted(1, false);
        let (_, _, _, mut request) = fixture(&source.manifest);
        request.view.section = Some(ReferenceReportSection {
            axis: ReferenceSectionAxis::Z,
            offset_m: 1.5,
            keep: ReferenceSectionKeep::Above,
        });
        let frame = Frame::new(&request.view).unwrap();
        let edge = frame.edge([1., 0., 1.], [2., 0., 2.]).unwrap();
        close(edge[0][0], 430.);
        close(edge[0][1], 304.);
        close(edge[1][0], 480.);
        close(edge[1][1], 254.);
        close(frame.scale, 100.);
        assert!(frame.edge([1., 0., 0.], [2., 0., 1.]).is_none());
    }
    #[test]
    fn mapped_overflow_is_explicit_unavailable_instead_of_silent_reduction() {
        let source = admitted(8001, true);
        let (model, calendar, mut scope, mut request) = fixture(&source.manifest);
        request.focus_occurrence_ids = source
            .manifest
            .occurrences
            .iter()
            .map(|o| o.id.clone())
            .collect();
        refresh(&mut scope, &mut request);
        let out = render(
            &source.manifest,
            &scope,
            &model,
            &calendar,
            &request,
            chunks(&source),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(out.definition.visible_triangle_count, 8003);
        assert!(out
            .definition
            .unavailable_reason
            .as_deref()
            .unwrap()
            .contains("8,000"));
        assert_eq!(out.definition.depicted_triangle_count, 0);
        assert_eq!(out.definition.analytical_edge_count, 0);
        assert_eq!(out.definition.scale_bar_m, None);
        assert!(!String::from_utf8(out.svg).unwrap().contains("<path"));
    }
    #[test]
    fn background_reduction_is_bounded_and_never_discards_small_mapped_occurrence() {
        let source = admitted(9000, true);
        let (model, calendar, scope, request) = fixture(&source.manifest);
        let out = render(
            &source.manifest,
            &scope,
            &model,
            &calendar,
            &request,
            chunks(&source),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(out.definition.depicted_triangle_count, 8000);
        assert_eq!(out.definition.omitted_visible_triangle_count, 1002);
        assert_eq!(out.definition.represented_occurrence_ids.len(), 3);
        assert!(out
            .definition
            .represented_occurrence_ids
            .contains(&source.manifest.occurrences[1].id));
    }
    #[test]
    fn rejects_different_live_model_reference_or_incomplete_stream() {
        let source = admitted(1, false);
        let (mut model, calendar, scope, request) = fixture(&source.manifest);
        model.zones[0]
            .cooling_system
            .as_mut()
            .unwrap()
            .gross_rated_cop += 0.1;
        assert!(render(
            &source.manifest,
            &scope,
            &model,
            &calendar,
            &request,
            chunks(&source),
            &AtomicBool::new(false)
        )
        .unwrap_err()
        .to_string()
        .contains("snapshot"));
        let (model, _, _, _) = fixture(&source.manifest);
        assert!(render(
            &source.manifest,
            &scope,
            &model,
            &calendar,
            &request,
            chunks(&source).into_iter().take(2),
            &AtomicBool::new(false)
        )
        .is_err());
        let mut changed = request.clone();
        changed.view.reference_revision_sha256 = "f".repeat(64);
        assert!(render(
            &source.manifest,
            &scope,
            &model,
            &calendar,
            &changed,
            chunks(&source),
            &AtomicBool::new(false)
        )
        .is_err());
    }
    #[test]
    fn cancellation_does_not_publish_a_partial_figure_and_empty_frame_is_explicit() {
        let source = admitted(1, false);
        let (model, calendar, mut scope, mut request) = fixture(&source.manifest);
        assert!(render(
            &source.manifest,
            &scope,
            &model,
            &calendar,
            &request,
            chunks(&source),
            &AtomicBool::new(true)
        )
        .unwrap_err()
        .to_string()
        .contains("cancelled"));
        request.view.eye_m = [200., -10., 1.5];
        request.view.target_m = [200., 0., 1.5];
        refresh(&mut scope, &mut request);
        let out = render(
            &source.manifest,
            &scope,
            &model,
            &calendar,
            &request,
            chunks(&source),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(out
            .definition
            .unavailable_reason
            .as_deref()
            .unwrap()
            .contains("No original reference edges"));
        assert_eq!(out.definition.analytical_edge_count, 0);
    }

    #[test]
    fn cancellation_after_streaming_starts_and_nonfinite_triangles_are_rejected() {
        let source = admitted(600, false);
        let (model, calendar, scope, request) = fixture(&source.manifest);
        let cancel = AtomicBool::new(false);
        let stream = chunks(&source).into_iter().enumerate().map(|(i, t)| {
            if i == 20 {
                cancel.store(true, Ordering::Relaxed);
            }
            t
        });
        let error = render(
            &source.manifest,
            &scope,
            &model,
            &calendar,
            &request,
            stream,
            &cancel,
        )
        .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        let mut stream = chunks(&source);
        stream[0].as_mut().unwrap().vertices_m[0][0] = f64::NAN;
        let error = render(
            &source.manifest,
            &scope,
            &model,
            &calendar,
            &request,
            stream,
            &AtomicBool::new(false),
        )
        .unwrap_err();
        assert!(error.to_string().contains("Invalid reference triangle"));
    }
}
