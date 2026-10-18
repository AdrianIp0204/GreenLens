//! Additive paired drawing context. The frozen analytical v1 renderer is not
//! modified: this method adds independently admitted neutral reference edges.
use super::*;
use crate::{
    architectural_scope::{
        ArchitecturalScope, MappingStatus, ReferenceSectionAxis, ReferenceSectionKeep,
    },
    canonical_hash,
    reference::{ReferenceModel, ReferenceTriangle, MAX_TRIANGLES},
    Result,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Display,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub const SCOPED_DESIGN_FIGURE_SCHEMA: &str = "greenlens.paired-design-figures.v2";
pub const SCOPED_DESIGN_FIGURE_METHOD: &str = "greenlens.scoped-paired-drawing/2";
pub const DESIGN_DRAWING_SELECTION_SCHEMA: &str = "greenlens.design-report-drawing-selection.v1";
pub const DESIGN_REFERENCE_CONTEXT_SCHEMA: &str = "greenlens.paired-reference-context.v1";
pub const MAX_CONTEXT_TRIANGLES_PER_FRAME: usize = 8_000;
pub const MAX_CONTEXT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DesignDrawingSelection {
    pub schema_version: String,
    pub scope_revision_sha256: String,
    pub accepted_view_ids: Vec<String>,
    pub include_reference_context: bool,
}
impl DesignDrawingSelection {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != DESIGN_DRAWING_SELECTION_SCHEMA
            || !digest(&self.scope_revision_sha256)
            || self.accepted_view_ids.len() > 2
            || self.accepted_view_ids.iter().collect::<BTreeSet<_>>().len()
                != self.accepted_view_ids.len()
        {
            return Err(invalid(
                "drawing.selection",
                "Select one retained scope and at most two distinct accepted views",
            ));
        }
        // Historical accepted view IDs are bounded UTF-8 text, not energy
        // object identifiers. Preserve Unicode and spaces without renaming.
        for id in &self.accepted_view_ids {
            text("drawing.accepted_view_id", id)?;
            if id.len() > 128 {
                return Err(invalid(
                    "drawing.accepted_view_id",
                    "Accepted view IDs are at most 128 UTF-8 bytes",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DesignContextTriangle {
    pub source_ordinal: usize,
    pub occurrence_id: String,
    pub vertices_m: [[f64; 3]; 3],
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DesignReferenceLayer {
    pub frame_sha256: String,
    pub source_triangle_count: usize,
    pub visible_triangle_count: usize,
    pub protected_visible_triangle_count: usize,
    pub omitted_visible_triangle_count: usize,
    pub represented_occurrence_ids: Vec<String>,
    pub triangles: Vec<DesignContextTriangle>,
    pub unavailable_reason: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PairedReferenceContext {
    pub schema_version: String,
    pub scope_revision_sha256: String,
    pub reference_revision_sha256: String,
    pub reference_geometry_sha256: String,
    pub manifest_sha256: String,
    pub layers: Vec<DesignReferenceLayer>,
    pub content_sha256: String,
}
impl PairedReferenceContext {
    pub fn seal(&mut self) -> Result<()> {
        self.content_sha256.clear();
        self.content_sha256 = canonical_hash(self)?;
        Ok(())
    }
    pub fn validate(&self, frames: &[DesignFigureFrame], scope: &str) -> Result<()> {
        if !(2..=4).contains(&frames.len())
            || self.layers.len() != frames.len()
            || self.layers.iter().any(|layer| {
                layer.triangles.len() > MAX_CONTEXT_TRIANGLES_PER_FRAME
                    || layer.represented_occurrence_ids.len() > MAX_CONTEXT_TRIANGLES_PER_FRAME
                    || layer
                        .represented_occurrence_ids
                        .iter()
                        .any(|id| id.len() > 256)
                    || layer
                        .unavailable_reason
                        .as_ref()
                        .is_some_and(|reason| reason.len() > 2048)
            })
        {
            return Err(invalid(
                "drawing.context",
                "Paired reference context exceeds its bounded shape",
            ));
        }
        let mut expected = self.clone();
        expected.seal()?;
        if self.schema_version != DESIGN_REFERENCE_CONTEXT_SCHEMA
            || self.scope_revision_sha256 != scope
            || [
                &self.scope_revision_sha256,
                &self.reference_revision_sha256,
                &self.reference_geometry_sha256,
                &self.manifest_sha256,
            ]
            .iter()
            .any(|h| !digest(h))
            || self.content_sha256 != expected.content_sha256
            || self.layers.len() != frames.len()
            || serde_json::to_vec(self)?.len() > MAX_CONTEXT_BYTES
        {
            return Err(invalid(
                "drawing.context",
                "Invalid, changed or oversized paired reference context",
            ));
        }
        for (layer, frame) in self.layers.iter().zip(frames) {
            if layer.frame_sha256 != canonical_hash(frame)?
                || layer.source_triangle_count > MAX_TRIANGLES
                || layer.visible_triangle_count > layer.source_triangle_count
                || layer.protected_visible_triangle_count > layer.visible_triangle_count
                || layer.triangles.len() > MAX_CONTEXT_TRIANGLES_PER_FRAME
                || layer
                    .omitted_visible_triangle_count
                    .checked_add(layer.triangles.len())
                    != Some(layer.visible_triangle_count)
                || layer.unavailable_reason.is_some() && !layer.triangles.is_empty()
            {
                return Err(invalid(
                    "drawing.context.frame",
                    "Reference context shape or exact frame differs",
                ));
            }
            let mut last = None;
            for t in &layer.triangles {
                if t.source_ordinal >= layer.source_triangle_count
                    || last.is_some_and(|p| p >= t.source_ordinal)
                    || t.occurrence_id.is_empty()
                    || t.occurrence_id.len() > 256
                    || t.vertices_m
                        .iter()
                        .flatten()
                        .any(|x| !x.is_finite() || x.abs() > 1e9)
                {
                    return Err(invalid(
                        "drawing.context.triangle",
                        "Invalid reference triangle or order",
                    ));
                }
                last = Some(t.source_ordinal);
            }
            let ids = layer
                .triangles
                .iter()
                .map(|t| t.occurrence_id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            if ids != layer.represented_occurrence_ids {
                return Err(invalid(
                    "drawing.context.occurrences",
                    "Reference occurrence identities differ",
                ));
            }
        }
        Ok(())
    }
}
fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(x, y)| x * y).sum()
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

struct Frame<'a> {
    value: &'a DesignFigureFrame,
    right: [f64; 3],
    up: [f64; 3],
    width: f64,
    height: f64,
    scale: f64,
    left: f64,
    top: f64,
}
impl<'a> Frame<'a> {
    fn new(value: &'a DesignFigureFrame) -> Result<Self> {
        let (right, up) = super::design_figures::axes(value)?;
        let height = value.vertical_span_m;
        let width = height * value.aspect_ratio;
        let scale = (1024. / width).min(440. / height);
        Ok(Self {
            value,
            right,
            up,
            width,
            height,
            scale,
            left: 88. + (1024. - width * scale) / 2.,
            top: 274. + (440. - height * scale) / 2.,
        })
    }
    fn edge(&self, mut a: [f64; 3], mut b: [f64; 3]) -> Option<[[f64; 2]; 2]> {
        if let Some(section) = &self.value.section {
            let i = match section.axis {
                ReferenceSectionAxis::X => 0,
                ReferenceSectionAxis::Y => 1,
                ReferenceSectionAxis::Z => 2,
            };
            let sign = if section.keep == ReferenceSectionKeep::Above {
                1.
            } else {
                -1.
            };
            let mut n = [0.; 3];
            n[i] = sign;
            if !super::design_figures::clip(&mut a, &mut b, n, section.offset_m * sign) {
                return None;
            }
        }
        for (n, d) in [
            (
                self.right,
                dot(self.value.target_m, self.right) - self.width / 2.,
            ),
            (
                self.right.map(|x| -x),
                -dot(self.value.target_m, self.right) - self.width / 2.,
            ),
            (
                self.up,
                dot(self.value.target_m, self.up) - self.height / 2.,
            ),
            (
                self.up.map(|x| -x),
                -dot(self.value.target_m, self.up) - self.height / 2.,
            ),
        ] {
            if !super::design_figures::clip(&mut a, &mut b, n, d) {
                return None;
            }
        }
        let project = |p| {
            let d = sub(p, self.value.target_m);
            [
                self.left + (dot(d, self.right) + self.width / 2.) * self.scale,
                self.top + (self.height / 2. - dot(d, self.up)) * self.scale,
            ]
        };
        let edge = [project(a), project(b)];
        if (0..2).all(|i| (edge[0][i] * 1000.).round() == (edge[1][i] * 1000.).round()) {
            None
        } else {
            Some(edge)
        }
    }
    fn visible(&self, vertices: [[f64; 3]; 3]) -> bool {
        [(0, 1), (1, 2), (2, 0)]
            .into_iter()
            .any(|(a, b)| self.edge(vertices[a], vertices[b]).is_some())
    }
}

#[derive(Default)]
struct Selection {
    mandatory: Vec<DesignContextTriangle>,
    representatives: BTreeMap<usize, DesignContextTriangle>,
    extras: BTreeMap<(u64, usize), DesignContextTriangle>,
    visible: usize,
    protected: usize,
    overflow: bool,
}
fn rank(i: usize) -> u64 {
    (i as u64).wrapping_mul(0x9e3779b97f4a7c15).rotate_left(17)
}

/// All triangles are inspected before bounded view-aware retention. All visible
/// mapped triangles (including descendant occurrences) are protected. Excess
/// protected content produces an explicit unavailable illustration, not loss.
pub fn prepare_reference_layers<I, E>(
    reference: &ReferenceModel,
    scope: &ArchitecturalScope,
    frames: &[DesignFigureFrame],
    triangles: I,
    cancel: &AtomicBool,
) -> Result<Vec<DesignReferenceLayer>>
where
    I: IntoIterator<Item = std::result::Result<ReferenceTriangle, E>>,
    E: Display,
{
    let started = Instant::now();
    crate::reference::validate_manifest(reference)
        .map_err(|e| invalid("drawing.reference", e.to_string()))?;
    if !(2..=4).contains(&frames.len()) {
        return Err(invalid("drawing.frames", "Use two to four shared frames"));
    }
    if scope.reference_revision_sha256 != reference.revision_sha256
        || scope.reference_geometry_sha256 != reference.geometry_sha256
    {
        return Err(invalid(
            "drawing.reference",
            "Scope and admitted reference differ",
        ));
    }
    let index: BTreeMap<_, _> = reference
        .occurrences
        .iter()
        .enumerate()
        .map(|(i, o)| (o.id.as_str(), i))
        .collect();
    let mapped: BTreeSet<_> = scope
        .mappings
        .iter()
        .filter(|m| m.status == MappingStatus::Mapped)
        .map(|m| m.occurrence_id.as_str())
        .collect();
    if scope
        .mappings
        .iter()
        .any(|m| !index.contains_key(m.occurrence_id.as_str()))
    {
        return Err(invalid(
            "drawing.mapping",
            "Mapping references an absent occurrence",
        ));
    }
    let protected: Vec<_> = reference
        .occurrences
        .iter()
        .map(|o| {
            let mut id = Some(o.id.as_str());
            while let Some(v) = id {
                if mapped.contains(v) {
                    return true;
                }
                id = reference.occurrences[index[v]].parent_id.as_deref();
            }
            false
        })
        .collect();
    let projections = frames.iter().map(Frame::new).collect::<Result<Vec<_>>>()?;
    let mut selections = (0..frames.len())
        .map(|_| Selection::default())
        .collect::<Vec<_>>();
    let mut counts = vec![0usize; reference.occurrences.len()];
    let mut read = 0usize;
    for triangle in triangles {
        if read.is_multiple_of(128)
            && (cancel.load(Ordering::Relaxed) || started.elapsed() > Duration::from_secs(30))
        {
            return Err(invalid(
                "drawing.reference",
                "Reference drawing cancelled or exceeded its 30-second preparation limit",
            ));
        }
        let t = triangle.map_err(|e| invalid("drawing.reference", e.to_string()))?;
        let occurrence = t.occurrence_index as usize;
        if read >= reference.triangle_count
            || read >= MAX_TRIANGLES
            || occurrence >= counts.len()
            || t.vertices_m
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || v.abs() > 1e9)
        {
            return Err(invalid(
                "drawing.reference",
                "Invalid or excessive admitted triangle stream",
            ));
        }
        counts[occurrence] += 1;
        for (selection, frame) in selections.iter_mut().zip(&projections) {
            if !frame.visible(t.vertices_m) {
                continue;
            }
            selection.visible += 1;
            let item = DesignContextTriangle {
                source_ordinal: read,
                occurrence_id: reference.occurrences[occurrence].id.clone(),
                vertices_m: t.vertices_m,
            };
            if protected[occurrence] {
                selection.protected += 1;
                if selection.mandatory.len() < MAX_CONTEXT_TRIANGLES_PER_FRAME {
                    selection.mandatory.push(item)
                } else {
                    selection.overflow = true;
                }
            } else {
                let room = selection.representatives.len() < MAX_CONTEXT_TRIANGLES_PER_FRAME;
                match selection.representatives.entry(occurrence) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        if room {
                            entry.insert(item);
                        } else {
                            selection.overflow = true;
                        }
                    }
                    std::collections::btree_map::Entry::Occupied(_) => {
                        selection.extras.insert((rank(read), read), item);
                        if selection.extras.len() > MAX_CONTEXT_TRIANGLES_PER_FRAME {
                            selection.extras.pop_last();
                        }
                    }
                }
            }
        }
        read += 1;
    }
    if cancel.load(Ordering::Relaxed)
        || read != reference.triangle_count
        || counts
            .iter()
            .zip(&reference.occurrences)
            .any(|(n, o)| *n != o.triangle_count)
    {
        return Err(invalid(
            "drawing.reference",
            "Cancelled, incomplete or reordered reference stream",
        ));
    }
    frames.iter().zip(selections).map(|(frame,mut s)|{
        let unavailable=if s.overflow || s.mandatory.len()+s.representatives.len()>MAX_CONTEXT_TRIANGLES_PER_FRAME {Some("Reference context unavailable: this frame exceeds 8,000 visible triangles while preserving every mapped triangle and one representative per other occurrence. Choose a narrower accepted frame; no substitute is depicted.".into())}else if s.visible==0 {Some("Reference context unavailable: no original reference edges intersect this exact frame and section. Analytical geometry alone does not establish reference coverage.".into())}else{None};
        s.mandatory.extend(s.representatives.into_values());
        let remaining=MAX_CONTEXT_TRIANGLES_PER_FRAME.saturating_sub(s.mandatory.len());s.mandatory.extend(s.extras.into_values().take(remaining));s.mandatory.sort_by_key(|t|t.source_ordinal);
        if unavailable.is_some(){s.mandatory.clear();}
        let represented=s.mandatory.iter().map(|t|t.occurrence_id.clone()).collect::<BTreeSet<_>>().into_iter().collect();
        Ok(DesignReferenceLayer {frame_sha256:canonical_hash(frame)?,source_triangle_count:read,visible_triangle_count:s.visible,protected_visible_triangle_count:s.protected,omitted_visible_triangle_count:s.visible-s.mandatory.len(),represented_occurrence_ids:represented,triangles:s.mandatory,unavailable_reason:unavailable})
    }).collect()
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn note_lines(s: &str) -> Vec<String> {
    let chars = s.chars().collect::<Vec<_>>();
    let mut start = 0;
    let mut lines = Vec::new();
    while start < chars.len() && lines.len() < 2 {
        let mut end = (start + 62).min(chars.len());
        if end < chars.len() {
            if let Some(space) = chars[start..end].iter().rposition(|c| *c == ' ') {
                if space > 31 {
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
        if let Some(last) = lines.last_mut() {
            *last = format!(
                "{}… [complete context in JSON]",
                last.chars().take(33).collect::<String>()
            );
        }
    }
    lines
}
/// New derived codec only. The frozen analytical geometry path uses explicitly
/// scoped annotations before text wrapping, then neutral, identical reference
/// context is composed beneath both cases. Public v1 rendering stays unchanged.
pub fn scoped_paired_design_figures(
    request: &PairedDesignFigureRequest,
    scope_revision_sha256: &str,
    context: Option<&PairedReferenceContext>,
) -> Result<PairedDesignFigures> {
    if !digest(scope_revision_sha256)
        || request.baseline.scope_revision_sha256.as_deref() != Some(scope_revision_sha256)
        || request.proposed.scope_revision_sha256.as_deref() != Some(scope_revision_sha256)
    {
        return Err(invalid(
            "drawing.scope",
            "Both cases must bind the exact selected scope revision",
        ));
    }
    if let Some(context) = context {
        context.validate(&request.frames, scope_revision_sha256)?;
    }
    let mut result = super::design_figures::paired_design_figures_with_annotations(request, true)?;
    for product in &mut result.products {
        let index = request
            .frames
            .iter()
            .position(|f| f.id == product.frame_id)
            .ok_or_else(|| invalid("drawing.frame", "Missing exact frame"))?;
        let frame = Frame::new(&request.frames[index])?;
        let mut paths = String::new();
        let mut reference_note =
            "Neutral reference context not requested; accepted analytical scope is retained."
                .to_owned();
        if let Some(context) = context {
            let layer = &context.layers[index];
            for t in &layer.triangles {
                for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                    if let Some([a, b]) = frame.edge(t.vertices_m[a], t.vertices_m[b]) {
                        paths.push_str(&format!("<path data-reference-occurrence=\"{}\" data-reference-ordinal=\"{}\" d=\"M{:.3},{:.3} L{:.3},{:.3}\" fill=\"none\" stroke=\"#87949e\" stroke-width=\"1.1\" stroke-dasharray=\"3 3\"/>",xml(&t.occurrence_id),t.source_ordinal,a[0],a[1],b[0],b[1]));
                    }
                }
            }
            reference_note = if layer.unavailable_reason.is_some() {
                "REFERENCE CONTEXT UNAVAILABLE in this frame; complete reason and counts are retained in drawing evidence.".into()
            } else {
                format!("Neutral grey dashed reference: {}/{} visible triangles; {} omitted in drawing; all {} admitted triangles checked.",layer.triangles.len(),layer.visible_triangle_count,layer.omitted_visible_triangle_count,layer.source_triangle_count)
            };
        }
        let marker = "fill=\"#fafcfc\" stroke=\"#c2cbd0\"/>";
        let at = product.svg.find(marker).ok_or_else(|| {
            invalid(
                "drawing.codec",
                "Frozen analytical drawing frame marker missing",
            )
        })? + marker.len();
        product.svg.insert_str(at, &paths);
        product.svg = product
            .svg
            .replacen(
                "height=\"1140\" viewBox=\"0 0 1200 1140\"",
                "height=\"1200\" viewBox=\"0 0 1200 1200\"",
                1,
            )
            .replacen(
                "<rect width=\"1200\" height=\"1140\"",
                "<rect width=\"1200\" height=\"1200\"",
                1,
            );
        product.svg.truncate(product.svg.len() - "</svg>".len());
        // Two bounded lines with fixed readable fonts; full declarations and
        // unavailable explanations are lossless in the drawing supplement.
        for (i, line) in note_lines(&reference_note).iter().enumerate() {
            product.svg.push_str(&format!("<text data-role=\"reference-context\" x=\"48\" y=\"{}\" class=\"gl-design-small\">{}</text>",1128+i*22,xml(line)));
        }
        let binding_note = if let Some(c) = context {
            format!(
                "Scope {} · source {} · geometry {}",
                &scope_revision_sha256[..10],
                &c.reference_revision_sha256[..10],
                &c.reference_geometry_sha256[..10]
            )
        } else {
            format!(
                "Scope {} · source geometry not depicted.",
                &scope_revision_sha256[..16]
            )
        };
        product.svg.push_str(&format!("<text data-role=\"scope-revision\" x=\"48\" y=\"1176\" class=\"gl-design-small\">{}</text></svg>",xml(&binding_note)));
        if product.svg.len() > MAX_DESIGN_FIGURE_BYTES {
            return Err(invalid(
                "drawing.bytes",
                "Paired context figure exceeds 8 MiB; no geometry was silently omitted",
            ));
        }
        product.svg_sha256 = crate::sha256(product.svg.as_bytes());
    }
    result.schema_version = SCOPED_DESIGN_FIGURE_SCHEMA.into();
    result.method = SCOPED_DESIGN_FIGURE_METHOD.into();
    result.limitations[0] = if context.is_some() {
        "Reference context is neutral, uses exactly the same world frame and section on A/B, and never supplies thermal properties or proves equivalence. Complete visible/depicted/omitted counts and any unavailable views are retained in the drawing supplement.".into()
    } else {
        "Accepted scope and views are bound, but reference context was explicitly not requested. Scope acceptance does not imply complete provenance, physical validity or approval.".into()
    };
    result.content_sha256.clear();
    result.content_sha256 = canonical_hash(&(SCOPED_DESIGN_FIGURE_SCHEMA, &result, context))?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{architectural_scope::*, reference::*};
    use serde_json::json;

    // Original synthetic GLB: one independent known small target is placed
    // after 8,001 repeated background triangles, between old stride positions.
    // These are presentation fixtures, never accepted engine evidence.
    fn admitted(background_count: usize, inside: bool) -> AdmittedReference {
        let background = if inside {
            [[1f32, 0., 1.], [2., 0., 1.], [1., 0., 2.]]
        } else {
            [[100f32, 0., 1.], [101., 0., 1.], [100., 0., 2.]]
        };
        let target = [[1f32, 0., 1.], [2., 0., 1.], [1., 0., 2.]];
        let mut bin = Vec::new();
        for vertices in [background, target] {
            for [x, y, z] in vertices {
                for value in [x, z, -y] {
                    bin.extend(value.to_le_bytes());
                }
            }
        }
        for _ in 0..background_count {
            for index in [0u32, 1, 2] {
                bin.extend(index.to_le_bytes());
            }
        }
        let (min, max) = if inside {
            ([1, 1, 0], [2, 2, 0])
        } else {
            ([100, 1, 0], [101, 2, 0])
        };
        let doc = json!({"asset":{"version":"2.0","generator":"GreenLens original presentation fixture"},"buffers":[{"byteLength":bin.len()}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":36},{"buffer":0,"byteOffset":72,"byteLength":background_count*12}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":min,"max":max},{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3","min":[1,1,0],"max":[2,2,0]},{"bufferView":2,"componentType":5125,"count":background_count*3,"type":"SCALAR"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":2}]},{"primitives":[{"attributes":{"POSITION":1}}]}],"nodes":[{"mesh":0,"name":"Synthetic background"},{"mesh":1,"name":"Independent target"}],"scenes":[{"nodes":[0,1]}],"scene":0});
        let mut j = serde_json::to_vec(&doc).unwrap();
        while !j.len().is_multiple_of(4) {
            j.push(b' ');
        }
        let mut bytes = Vec::new();
        bytes.extend(0x46546c67u32.to_le_bytes());
        bytes.extend(2u32.to_le_bytes());
        bytes.extend(((28 + j.len() + bin.len()) as u32).to_le_bytes());
        bytes.extend((j.len() as u32).to_le_bytes());
        bytes.extend(0x4e4f534au32.to_le_bytes());
        bytes.extend(j);
        bytes.extend((bin.len() as u32).to_le_bytes());
        bytes.extend(0x004e4942u32.to_le_bytes());
        bytes.extend(bin);
        admit_glb(&bytes, &ReferenceImportOptions::default(), |_| true).unwrap()
    }
    fn fixture(reference: &ReferenceModel) -> (ArchitecturalScope, PairedDesignFigureRequest) {
        let model = synthetic_four_zone_model(false);
        let calendar = synthetic_energy_calendar();
        let model_hash = validate_thermal_model(&model, &calendar)
            .unwrap()
            .model_input_sha256;
        let case = |id: &str| DesignFigureCase {
            design_id: id.into(),
            label: format!("Synthetic {id}"),
            model: model.clone(),
            calendar: calendar.clone(),
            study: None,
            scope_revision_sha256: Some("d".repeat(64)),
            evidence: DesignFigureEvidence::NotSimulated {
                reason: "Original pure presentation fixture, no engine record".into(),
            },
        };
        let mut frames = default_design_frames(&model, &calendar, &model, &calendar).unwrap();
        frames[1] = DesignFigureFrame {
            id: "accepted-front".into(),
            title: "Independent target section".into(),
            kind: DesignViewKind::Front,
            eye_m: [1.5, -10., 1.5],
            target_m: [1.5, 0., 1.5],
            up: [0., 0., 1.],
            vertical_span_m: 3.,
            aspect_ratio: 1.,
            section: Some(ReferenceReportSection {
                axis: ReferenceSectionAxis::Y,
                offset_m: -0.1,
                keep: ReferenceSectionKeep::Above,
            }),
        };
        let scope = ArchitecturalScope {
            schema_version: ARCHITECTURAL_SCOPE_SCHEMA.into(),
            title: "Synthetic scope".into(),
            purpose: "Geometry presentation only".into(),
            scope_kind: ScopeKind::RepresentativeStudy,
            reference_revision_sha256: reference.revision_sha256.clone(),
            reference_geometry_sha256: reference.geometry_sha256.clone(),
            reference_options_sha256: canonical_hash(&reference.options).unwrap(),
            design_model_sha256: BTreeMap::from([
                ("A".into(), model_hash.clone()),
                ("B".into(), model_hash.clone()),
            ]),
            mappings: vec![ScopeMapping {
                occurrence_id: reference.occurrences[1].id.clone(),
                status: MappingStatus::Mapped,
                targets: vec![AnalyticalTarget {
                    design_id: "A".into(),
                    object_id: model.zones[0].id.clone(),
                    kind: AnalyticalObjectKind::Zone,
                    model_input_sha256: model_hash,
                }],
                rationale: "Independent small target protected in drawing".into(),
            }],
            included_scope: vec!["Target".into()],
            excluded_scope: vec!["No whole-building equivalence".into()],
            boundary_consequences: "Presentation fixture".into(),
            assumptions: vec![],
            reconciliations: vec![],
            coverage: ScopeCoverage::Unknown {
                reason: "No denominator claimed".into(),
            },
            whole_building_review: None,
            report_views: vec![],
        };
        (
            scope,
            PairedDesignFigureRequest {
                schema_version: DESIGN_FIGURE_SCHEMA.into(),
                baseline: case("A"),
                proposed: case("B"),
                frames,
                scope: "Original synthetic presentation fixture".into(),
                north_declaration: None,
            },
        )
    }
    fn triangles(
        a: &AdmittedReference,
    ) -> Vec<std::result::Result<ReferenceTriangle, ReferenceError>> {
        a.chunks
            .iter()
            .enumerate()
            .flat_map(|(i, b)| decode_chunk(&a.manifest, i, b).unwrap().into_iter().map(Ok))
            .collect()
    }
    fn context(
        a: &AdmittedReference,
        scope: &ArchitecturalScope,
        request: &PairedDesignFigureRequest,
    ) -> PairedReferenceContext {
        let layers = prepare_reference_layers(
            &a.manifest,
            scope,
            &request.frames,
            triangles(a),
            &AtomicBool::new(false),
        )
        .unwrap();
        let mut c = PairedReferenceContext {
            schema_version: DESIGN_REFERENCE_CONTEXT_SCHEMA.into(),
            scope_revision_sha256: "d".repeat(64),
            reference_revision_sha256: a.manifest.revision_sha256.clone(),
            reference_geometry_sha256: a.manifest.geometry_sha256.clone(),
            manifest_sha256: "e".repeat(64),
            layers,
            content_sha256: String::new(),
        };
        c.seal().unwrap();
        c
    }
    fn visible_text(svg: &str) -> String {
        svg.split("<text ")
            .skip(1)
            .map(|part| {
                part.split_once('>')
                    .unwrap()
                    .1
                    .split_once("</text>")
                    .unwrap()
                    .0
            })
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
    fn optional_visual_fixture_output(
        figures: &PairedDesignFigures,
        context: &PairedReferenceContext,
    ) {
        if let Some(path) = std::env::var_os("GREENLENS_DRAWING_TEST_OUTPUT_DIR") {
            let destination = std::path::PathBuf::from(path);
            let allowed = std::path::PathBuf::from(
                std::env::var_os("LOCALAPPDATA")
                    .expect("LOCALAPPDATA required for external visual fixture output"),
            )
            .join("GreenLens")
            .join("verification")
            .canonicalize()
            .unwrap();
            assert!(
                destination.is_absolute()
                    && destination
                        .parent()
                        .unwrap()
                        .canonicalize()
                        .unwrap()
                        .starts_with(&allowed),
                "Visual test output must be a fresh external verification directory"
            );
            std::fs::create_dir(&destination).unwrap();
            for product in &figures.products {
                std::fs::write(destination.join(&product.filename), &product.svg).unwrap();
            }
            std::fs::write(destination.join("presentation-fixture.json"),serde_json::to_vec_pretty(&json!({"scope":"Original synthetic NotSimulated presentation fixture only; not engine/native/report/PDF acceptance","figures":figures,"reference_context":context})).unwrap()).unwrap();
        }
    }
    #[test]
    fn small_target_after_8000_is_retained_with_common_projection_and_original_v1_bytes() {
        let a = admitted(8001, false);
        let (scope, request) = fixture(&a.manifest);
        let c = context(&a, &scope, &request);
        let layer = &c.layers[1];
        assert_eq!(layer.source_triangle_count, 8002);
        assert_eq!(layer.visible_triangle_count, 1);
        assert_eq!(layer.protected_visible_triangle_count, 1);
        assert_eq!(layer.triangles[0].source_ordinal, 8001);
        assert!(layer.unavailable_reason.is_none());
        let original = paired_design_figures(&request).unwrap();
        let old_bytes = serde_json::to_vec(&original).unwrap();
        let result = scoped_paired_design_figures(&request, &"d".repeat(64), Some(&c)).unwrap();
        assert_eq!(
            serde_json::to_vec(&paired_design_figures(&request).unwrap()).unwrap(),
            old_bytes
        );
        let products = result
            .products
            .iter()
            .filter(|p| p.frame_id == "accepted-front")
            .collect::<Vec<_>>();
        assert_eq!(products.len(), 3);
        let reference_paths = |s: &str| {
            s.split("<path data-reference-occurrence=")
                .skip(1)
                .map(|p| p.split("/>").next().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        for p in &products {
            assert_eq!(reference_paths(&p.svg).len(), 3);
            assert!(p.svg.contains("M526.667,567.333 L673.333,567.333"));
            assert!(p.svg.contains("stroke=\"#87949e\""));
            assert!(p.svg.contains("height=\"1200\""));
            assert!(visible_text(&p.svg).contains("True north not documented"));
            assert!(visible_text(&p.svg).contains("1/1 visible triangles"));
            assert_eq!(p.scale_svg_units_per_m, 440. / 3.);
        }
        assert_eq!(
            reference_paths(&products[0].svg),
            reference_paths(&products[1].svg)
        );
        assert_eq!(
            reference_paths(&products[1].svg),
            reference_paths(&products[2].svg)
        );
        assert_eq!(result.method, SCOPED_DESIGN_FIGURE_METHOD);
        optional_visual_fixture_output(&result, &c);
    }
    #[test]
    fn protected_overflow_is_explicit_unavailable_never_silent_reduction() {
        let a = admitted(8001, true);
        let (mut scope, request) = fixture(&a.manifest);
        scope.mappings.push(ScopeMapping {
            occurrence_id: a.manifest.occurrences[0].id.clone(),
            status: MappingStatus::Mapped,
            targets: scope.mappings[0].targets.clone(),
            rationale: "All background triangles explicitly mapped".into(),
        });
        let c = context(&a, &scope, &request);
        assert_eq!(c.layers[1].protected_visible_triangle_count, 8002);
        assert!(c.layers[1].triangles.is_empty());
        assert!(c.layers[1]
            .unavailable_reason
            .as_deref()
            .unwrap()
            .contains("8,000"));
        let result = scoped_paired_design_figures(&request, &"d".repeat(64), Some(&c)).unwrap();
        assert!(result
            .products
            .iter()
            .find(|p| p.frame_id == "accepted-front")
            .unwrap()
            .svg
            .contains("REFERENCE CONTEXT UNAVAILABLE"));
    }
    #[test]
    fn cancellation_truncation_resealed_frame_and_unbound_case_fail() {
        let a = admitted(1, false);
        let (scope, mut request) = fixture(&a.manifest);
        assert!(prepare_reference_layers(
            &a.manifest,
            &scope,
            &request.frames,
            triangles(&a),
            &AtomicBool::new(true)
        )
        .is_err());
        let mut short = triangles(&a);
        short.pop();
        assert!(prepare_reference_layers(
            &a.manifest,
            &scope,
            &request.frames,
            short,
            &AtomicBool::new(false)
        )
        .is_err());
        let mut c = context(&a, &scope, &request);
        c.layers[1].frame_sha256 = "f".repeat(64);
        c.seal().unwrap();
        assert!(c.validate(&request.frames, &"d".repeat(64)).is_err());
        request.proposed.scope_revision_sha256 = None;
        assert!(scoped_paired_design_figures(&request, &"d".repeat(64), None).is_err());
    }
    #[test]
    fn unprotected_context_reduction_is_deterministic_and_disclosed() {
        let a = admitted(8001, true);
        let (mut scope, request) = fixture(&a.manifest);
        scope.mappings.clear();
        let c = context(&a, &scope, &request);
        let d = context(&a, &scope, &request);
        assert_eq!(c, d);
        assert_eq!(c.layers[1].triangles.len(), 8000);
        assert_eq!(c.layers[1].omitted_visible_triangle_count, 2);
        assert_eq!(c.layers[1].represented_occurrence_ids.len(), 2);
        assert!(c.layers[1].unavailable_reason.is_none());
    }
    #[test]
    fn malformed_counts_and_oversized_layer_shape_fail_without_overflow() {
        let a = admitted(1, false);
        let (scope, request) = fixture(&a.manifest);
        let mut c = context(&a, &scope, &request);
        c.layers[1].omitted_visible_triangle_count = usize::MAX;
        c.seal().unwrap();
        assert!(c.validate(&request.frames, &"d".repeat(64)).is_err());
        let mut c = context(&a, &scope, &request);
        c.layers.extend(c.layers.clone());
        c.layers.push(c.layers[0].clone());
        assert!(c.validate(&request.frames, &"d".repeat(64)).is_err());
        let mut c = context(&a, &scope, &request);
        c.layers[1].unavailable_reason = Some("x".repeat(2049));
        assert!(c.validate(&request.frames, &"d".repeat(64)).is_err());
    }
    #[test]
    fn section_and_frame_changes_affect_reference_and_analytical_together() {
        let a = admitted(1, false);
        let (scope, mut request) = fixture(&a.manifest);
        request.frames[1].section.as_mut().unwrap().offset_m = 0.5;
        let c = context(&a, &scope, &request);
        assert_eq!(c.layers[1].visible_triangle_count, 0);
        assert!(c.layers[1].unavailable_reason.is_some());
        let result = scoped_paired_design_figures(&request, &"d".repeat(64), Some(&c)).unwrap();
        for p in result
            .products
            .iter()
            .filter(|p| p.frame_id == "accepted-front")
        {
            assert!(!p.svg.contains("data-reference-occurrence"));
            assert!(p.svg.contains("Section Y = 0.5000 m; keep Above"));
        }
    }
    #[test]
    fn accepted_reverse_elevation_caption_is_orthographic_without_reclassifying_legacy() {
        let a = admitted(1, false);
        let (_, mut request) = fixture(&a.manifest);
        let mut reverse = request.frames[1].clone();
        reverse.id = "accepted-reverse".into();
        reverse.eye_m = [1.5, 10., 1.5];
        reverse.kind = DesignViewKind::Axonometric;
        request.frames.push(reverse.clone());
        let legacy = paired_design_figures(&request).unwrap();
        let legacy_bytes = serde_json::to_vec(&legacy).unwrap();
        let scoped = scoped_paired_design_figures(&request, &"d".repeat(64), None).unwrap();
        for product in scoped.products.iter().filter(|p| p.frame_id == reverse.id) {
            let rendered_text = visible_text(&product.svg);
            assert!(rendered_text.contains("accepted orthographic frame"));
            assert!(!rendered_text.contains("Axonometric"));
        }
        assert_eq!(request.frames[2], reverse);
        assert_eq!(
            legacy_bytes,
            serde_json::to_vec(&paired_design_figures(&request).unwrap()).unwrap()
        );
    }

    #[test]
    fn scoped_caption_is_chosen_before_front_wrapping_and_v1_is_unchanged() {
        let a = admitted(1, false);
        let (scope, request) = fixture(&a.manifest);
        let c = context(&a, &scope, &request);
        let legacy = paired_design_figures(&request).unwrap();
        let legacy_bytes = serde_json::to_vec(&legacy).unwrap();
        let legacy_front = legacy
            .products
            .iter()
            .filter(|p| p.frame_id == "accepted-front")
            .collect::<Vec<_>>();
        assert_eq!(legacy_front.len(), 3);
        assert!(legacy_front
            .iter()
            .all(|p| visible_text(&p.svg).contains("no sampling.")));
        // Reproduce the actual failure mode: the phrase spans two text nodes,
        // so replacing a contiguous phrase in serialized SVG cannot fix it.
        assert!(legacy_front.iter().any(|p| {
            let lines = p
                .svg
                .split("<text ")
                .skip(1)
                .map(|part| {
                    part.split_once('>')
                        .unwrap()
                        .1
                        .split_once("</text>")
                        .unwrap()
                        .0
                })
                .collect::<Vec<_>>();
            lines
                .windows(2)
                .any(|pair| pair[0].ends_with(" no") && pair[1].starts_with("sampling."))
        }));
        let scoped = scoped_paired_design_figures(&request, &"d".repeat(64), Some(&c)).unwrap();
        for product in scoped
            .products
            .iter()
            .filter(|p| p.frame_id == "accepted-front")
        {
            let rendered_text = visible_text(&product.svg);
            assert!(rendered_text.contains("A/B only."));
            assert!(!rendered_text.contains("no sampling"));
            assert!(rendered_text.contains("Neutral grey dashed reference"));
        }
        assert_eq!(
            legacy_bytes,
            serde_json::to_vec(&paired_design_figures(&request).unwrap()).unwrap()
        );
        assert_eq!(
            legacy_bytes,
            serde_json::to_vec(
                &super::super::design_figures::paired_design_figures_with_annotations(
                    &request, false
                )
                .unwrap()
            )
            .unwrap()
        );
    }
}
