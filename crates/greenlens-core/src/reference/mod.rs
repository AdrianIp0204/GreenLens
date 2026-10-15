//! Inert, bounded architectural reference geometry, independent of thermal inputs.
//! This is a deliberately limited GLB intake, not a general glTF conformance claim.
mod glb;
mod math;
#[cfg(test)]
mod tests;

use crate::{canonical_hash, sha256, Vec3};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, time::Instant};
use thiserror::Error;

pub use glb::admit_glb;
pub const REFERENCE_SCHEMA: &str = "greenlens.reference-model.v1";
pub const IMPORTER_ID: &str = "greenlens.glb-static-triangles/1";
pub const MAX_SOURCE_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_JSON_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_MANIFEST_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_NORMALIZED_BYTES: usize = 80 * 1024 * 1024;
pub const MAX_DECODED_ESTIMATE_BYTES: usize = 384 * 1024 * 1024;
pub const MAX_NODES: usize = 20_000;
pub const MAX_DEPTH: usize = 64;
pub const MAX_ITEMS: usize = 40_000;
pub const MAX_MATERIALS: usize = 4096;
pub const MAX_TEXTURES: usize = 1024;
pub const MAX_TRIANGLES: usize = 1_000_000;
pub const TRIANGLES_PER_CHUNK: usize = 8192;
pub const CHUNK_HEADER_BYTES: usize = 16;
pub const TRIANGLE_BYTES: usize = 80;
pub const GEOMETRY_ABSOLUTE_TOLERANCE_M: f64 = 1e-8;
pub const GEOMETRY_RELATIVE_TOLERANCE: f64 = 1e-9;
pub const DEADLINE_SECONDS: u64 = 30;
const CHUNK_MAGIC: &[u8; 8] = b"GLREF001";

#[derive(Debug, Error)]
pub enum ReferenceError {
    #[error("{field}: {message}")]
    Admission { field: String, message: String },
    #[error("Reference verification cancelled; no candidate was accepted")]
    Cancelled,
    #[error("Reference intake exceeded its 30 second processing deadline")]
    Deadline,
    #[error("Reference JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Reference identity cannot be serialized: {0}")]
    Identity(#[from] crate::CoreError),
}
pub type ReferenceResult<T> = Result<T, ReferenceError>;
pub(crate) fn invalid(field: impl Into<String>, message: impl Into<String>) -> ReferenceError {
    ReferenceError::Admission {
        field: field.into(),
        message: message.into(),
    }
}
pub(crate) fn check(condition: bool, field: &str, message: &str) -> ReferenceResult<()> {
    if condition {
        Ok(())
    } else {
        Err(invalid(field, message))
    }
}
pub(crate) fn text(value: &str, field: &str, max: usize) -> ReferenceResult<()> {
    check(
        value.len() <= max && !value.chars().any(char::is_control),
        field,
        "Text exceeds its bound or contains control characters",
    )
}
fn hash_valid(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConversionIdentity {
    pub application: String,
    pub version: String,
    pub options: String,
    /// User-declared provenance only; never interpreted as commands or license authority.
    pub provenance_note: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReferenceImportOptions {
    pub scene_index: Option<usize>,
    /// glTF distances are metres. A correction requires an explicit user choice.
    pub metres_per_source_unit: f64,
    pub clockwise_rotation_deg: f64,
    pub translation_m: Vec3,
    /// None means true north is unknown; it is never inferred from geometry.
    pub declared_north_clockwise_deg: Option<f64>,
    pub conversion: Option<ConversionIdentity>,
}
impl Default for ReferenceImportOptions {
    fn default() -> Self {
        Self {
            scene_index: None,
            metres_per_source_unit: 1.,
            clockwise_rotation_deg: 0.,
            translation_m: [0.; 3],
            declared_north_clockwise_deg: None,
            conversion: None,
        }
    }
}
impl ReferenceImportOptions {
    pub fn validate(&self) -> ReferenceResult<()> {
        check(
            self.scene_index.is_none_or(|i| i < 256),
            "scene_index",
            "Scene selection exceeds the 256 scene bound",
        )?;
        check(
            self.metres_per_source_unit.is_finite()
                && (1e-6..=1000.).contains(&self.metres_per_source_unit),
            "metres_per_source_unit",
            "Choose a finite explicit scale between 0.000001 and 1000",
        )?;
        check(
            self.clockwise_rotation_deg.is_finite() && self.clockwise_rotation_deg.abs() <= 360.,
            "clockwise_rotation_deg",
            "Expected a finite rotation from -360 to 360 degrees",
        )?;
        check(
            self.translation_m
                .iter()
                .all(|v| v.is_finite() && v.abs() <= 1e7),
            "translation_m",
            "Expected finite offsets within 10,000,000 metres",
        )?;
        if let Some(n) = self.declared_north_clockwise_deg {
            check(
                n.is_finite() && (0.0..360.).contains(&n),
                "declared_north_clockwise_deg",
                "Use [0,360) degrees or unknown",
            )?;
        }
        if let Some(c) = &self.conversion {
            for (field, value) in [
                ("application", &c.application),
                ("version", &c.version),
                ("options", &c.options),
                ("provenance_note", &c.provenance_note),
            ] {
                text(value, field, 2048)?;
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReferenceBounds {
    pub min_m: Vec3,
    pub max_m: Vec3,
}
impl ReferenceBounds {
    pub fn dimensions_m(&self) -> Vec3 {
        std::array::from_fn(|i| self.max_m[i] - self.min_m[i])
    }
    pub(crate) fn include(&mut self, p: Vec3) {
        for (i, v) in p.iter().enumerate() {
            self.min_m[i] = self.min_m[i].min(*v);
            self.max_m[i] = self.max_m[i].max(*v);
        }
    }
    pub(crate) fn point(p: Vec3) -> Self {
        Self { min_m: p, max_m: p }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReferenceOccurrence {
    pub id: String,
    pub node_index: usize,
    pub parent_id: Option<String>,
    pub label: String,
    pub node_path: Vec<usize>,
    pub source_mesh_index: Option<usize>,
    /// Column-major source-to-world transform, including the Y-up/Z-up conversion.
    pub world_from_mesh: [f64; 16],
    pub mirrored: bool,
    /// Direct mesh triangles only, excluding descendants; grouping nodes have none.
    pub bounds: Option<ReferenceBounds>,
    pub triangle_count: usize,
    pub triangle_area_m2: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReferenceMaterial {
    pub source_index: usize,
    pub label: String,
    pub base_color_rgba: [f64; 4],
    pub double_sided: bool,
    pub alpha_mode: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReferenceChunk {
    pub path: String,
    pub sha256: String,
    pub byte_length: usize,
    pub triangle_count: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReferenceResources {
    pub elapsed_ms: u64,
    pub source_bytes: usize,
    pub json_bytes: usize,
    pub normalized_bytes: usize,
    /// Conservative accounting bound, not measured process working-set memory.
    pub decoded_allocation_estimate_bytes: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReferenceModel {
    pub schema_version: String,
    pub importer_id: String,
    pub source_sha256: String,
    pub revision_sha256: String,
    pub geometry_sha256: String,
    pub options: ReferenceImportOptions,
    pub source_units: String,
    pub coordinate_convention: String,
    pub selected_scene: usize,
    pub source_node_count: usize,
    pub source_mesh_count: usize,
    pub source_material_count: usize,
    pub source_texture_count: usize,
    pub occurrences: Vec<ReferenceOccurrence>,
    pub materials: Vec<ReferenceMaterial>,
    pub chunks: Vec<ReferenceChunk>,
    pub bounds: ReferenceBounds,
    pub triangle_count: usize,
    pub triangle_area_m2: f64,
    pub warnings: Vec<String>,
    pub exclusions: Vec<String>,
    pub storage_policy: String,
    pub resources: ReferenceResources,
}
#[derive(Debug, Clone)]
pub struct AdmittedReference {
    pub manifest: ReferenceModel,
    pub chunks: Vec<Vec<u8>>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReferenceTriangle {
    pub occurrence_index: u32,
    pub material_index: Option<u32>,
    pub vertices_m: [Vec3; 3],
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceProgress {
    pub stage: String,
    pub completed: usize,
    pub total: usize,
    pub processed_bytes: usize,
}
pub(crate) struct Monitor<'a> {
    started: Instant,
    callback: &'a mut dyn FnMut(&ReferenceProgress) -> bool,
}
impl<'a> Monitor<'a> {
    fn new(callback: &'a mut dyn FnMut(&ReferenceProgress) -> bool) -> Self {
        Self {
            started: Instant::now(),
            callback,
        }
    }
    pub(crate) fn step(
        &mut self,
        stage: &str,
        completed: usize,
        total: usize,
        bytes: usize,
    ) -> ReferenceResult<()> {
        if !(self.callback)(&ReferenceProgress {
            stage: stage.into(),
            completed,
            total,
            processed_bytes: bytes,
        }) {
            return Err(ReferenceError::Cancelled);
        }
        if self.started.elapsed().as_secs() >= DEADLINE_SECONDS {
            return Err(ReferenceError::Deadline);
        }
        Ok(())
    }
    pub(crate) fn elapsed_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
}

pub(crate) fn revision(source: &str, options: &ReferenceImportOptions) -> ReferenceResult<String> {
    Ok(canonical_hash(&(IMPORTER_ID, source, options))?)
}
pub(crate) fn occurrence_id(
    revision: &str,
    scene: usize,
    path: &[usize],
) -> ReferenceResult<String> {
    Ok(format!("ref-{}", canonical_hash(&(revision, scene, path))?))
}
pub(crate) fn geometry_identity(model: &ReferenceModel) -> ReferenceResult<String> {
    Ok(canonical_hash(&(
        &model.revision_sha256,
        model.selected_scene,
        &model.occurrences,
        &model.materials,
        &model.chunks,
        model.bounds,
        model.triangle_count,
        model.triangle_area_m2,
    ))?)
}
pub(crate) fn encode_chunk(triangles: &[ReferenceTriangle]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(CHUNK_HEADER_BYTES + triangles.len() * TRIANGLE_BYTES);
    bytes.extend(CHUNK_MAGIC);
    bytes.extend(1u32.to_le_bytes());
    bytes.extend((triangles.len() as u32).to_le_bytes());
    for t in triangles {
        bytes.extend(t.occurrence_index.to_le_bytes());
        bytes.extend(t.material_index.unwrap_or(u32::MAX).to_le_bytes());
        for v in t.vertices_m.iter().flatten() {
            bytes.extend(v.to_le_bytes());
        }
    }
    bytes
}
fn bounds_valid(b: ReferenceBounds) -> bool {
    (0..3).all(|i| {
        b.min_m[i].is_finite()
            && b.max_m[i].is_finite()
            && b.min_m[i] <= b.max_m[i]
            && b.min_m[i].abs() <= 1e8
            && b.max_m[i].abs() <= 1e8
    })
}
pub fn validate_manifest(model: &ReferenceModel) -> ReferenceResult<()> {
    check(
        model.schema_version == REFERENCE_SCHEMA && model.importer_id == IMPORTER_ID,
        "reference.schema",
        "Unrecognized reference schema/importer",
    )?;
    model.options.validate()?;
    check(
        hash_valid(&model.source_sha256) && hash_valid(&model.geometry_sha256),
        "reference.hash",
        "Expected lowercase SHA-256 identities",
    )?;
    check(
        model.revision_sha256 == revision(&model.source_sha256, &model.options)?,
        "reference.revision",
        "Reference revision does not match source/options",
    )?;
    check(model.source_units == "metres (glTF 2.0)" && model.coordinate_convention == "right-handed +X east/local-right, +Y north/local-back, +Z up; true north only when declared", "reference.coordinates", "Unrecognized coordinate convention")?;
    check(
        model.storage_policy
            == "normalized-geometry-only; original architectural source and GLB excluded",
        "reference.storage_policy",
        "Unrecognized storage policy",
    )?;
    check(
        model.selected_scene < 256
            && model.source_node_count <= MAX_NODES
            && model.source_mesh_count <= MAX_ITEMS
            && model.source_material_count <= MAX_MATERIALS
            && model.source_texture_count <= MAX_TEXTURES,
        "reference.counts",
        "Source counts exceed bounded intake limits",
    )?;
    check(
        !model.occurrences.is_empty()
            && model.occurrences.len() <= model.source_node_count
            && model.materials.len() == model.source_material_count,
        "reference.occurrences",
        "Invalid occurrence/material count",
    )?;
    check(
        model.triangle_count > 0
            && model.triangle_count <= MAX_TRIANGLES
            && bounds_valid(model.bounds)
            && model.triangle_area_m2.is_finite()
            && model.triangle_area_m2 > 0.,
        "reference.geometry",
        "Invalid triangle count, bounds or area",
    )?;
    let mut ids = BTreeSet::new();
    let mut nodes = BTreeSet::new();
    let mut triangles = 0usize;
    for o in &model.occurrences {
        text(&o.label, "reference.label", 512)?;
        check(
            !o.node_path.is_empty()
                && o.node_path.len() <= MAX_DEPTH
                && o.node_path.last() == Some(&o.node_index)
                && o.node_index < model.source_node_count
                && nodes.insert(o.node_index),
            "reference.node_path",
            "Invalid or duplicate scene occurrence",
        )?;
        check(
            o.id == occurrence_id(&model.revision_sha256, model.selected_scene, &o.node_path)?
                && ids.insert(o.id.clone()),
            "reference.id",
            "Invalid occurrence identity",
        )?;
        let expected_parent = if o.node_path.len() > 1 {
            Some(occurrence_id(
                &model.revision_sha256,
                model.selected_scene,
                &o.node_path[..o.node_path.len() - 1],
            )?)
        } else {
            None
        };
        check(
            o.parent_id == expected_parent
                && o.parent_id.as_ref().is_none_or(|id| ids.contains(id)),
            "reference.parent",
            "Missing or inconsistent parent occurrence",
        )?;
        math::validate_affine(o.world_from_mesh, false)?;
        check(
            o.mirrored == (math::determinant(o.world_from_mesh) < 0.),
            "reference.mirror",
            "Mirrored flag disagrees with transform",
        )?;
        check(
            o.source_mesh_index
                .is_none_or(|m| m < model.source_mesh_count)
                && o.triangle_count <= MAX_TRIANGLES
                && o.triangle_area_m2.is_finite()
                && o.triangle_area_m2 >= 0.
                && o.bounds.is_none_or(bounds_valid)
                && (o.triangle_count == 0) == o.bounds.is_none(),
            "reference.occurrence_geometry",
            "Invalid occurrence geometry declaration",
        )?;
        triangles = triangles
            .checked_add(o.triangle_count)
            .ok_or_else(|| invalid("reference.count", "Count overflow"))?;
    }
    check(
        triangles == model.triangle_count,
        "reference.count",
        "Occurrence triangle total differs",
    )?;
    for (i, m) in model.materials.iter().enumerate() {
        text(&m.label, "reference.material.label", 512)?;
        check(
            m.source_index == i
                && m.base_color_rgba
                    .iter()
                    .all(|v| v.is_finite() && (0.0..=1.).contains(v))
                && ["OPAQUE", "MASK", "BLEND"].contains(&m.alpha_mode.as_str()),
            "reference.material",
            "Invalid display material",
        )?;
    }
    let expected_chunks = model.triangle_count.div_ceil(TRIANGLES_PER_CHUNK);
    check(
        model.chunks.len() == expected_chunks,
        "reference.chunks",
        "Unexpected geometry chunk count",
    )?;
    let mut total_bytes = 0;
    for (i, chunk) in model.chunks.iter().enumerate() {
        let expected_count =
            (model.triangle_count - i * TRIANGLES_PER_CHUNK).min(TRIANGLES_PER_CHUNK);
        check(
            chunk.path == format!("geometry/{i:05}.bin")
                && hash_valid(&chunk.sha256)
                && chunk.triangle_count == expected_count
                && chunk.byte_length == CHUNK_HEADER_BYTES + expected_count * TRIANGLE_BYTES,
            "reference.chunk",
            "Invalid geometry path, shape or seal",
        )?;
        total_bytes += chunk.byte_length;
    }
    let r = &model.resources;
    check(
        r.source_bytes <= MAX_SOURCE_BYTES
            && r.json_bytes <= MAX_JSON_BYTES
            && r.normalized_bytes == total_bytes
            && total_bytes <= MAX_NORMALIZED_BYTES
            && r.decoded_allocation_estimate_bytes <= MAX_DECODED_ESTIMATE_BYTES
            && r.elapsed_ms <= DEADLINE_SECONDS * 1000,
        "reference.resources",
        "Reference resource bound exceeded or accounting differs",
    )?;
    check(
        model.warnings.len() <= 128 && model.exclusions.len() <= 128,
        "reference.warnings",
        "Too many diagnostics",
    )?;
    for t in model.warnings.iter().chain(&model.exclusions) {
        text(t, "reference.warning", 2048)?;
    }
    check(
        geometry_identity(model)? == model.geometry_sha256,
        "reference.geometry_sha256",
        "Geometry identity disagrees with manifest",
    )?;
    check(
        serde_json::to_vec(model)?.len() <= MAX_MANIFEST_BYTES,
        "reference.manifest",
        "Reference metadata exceeds 16 MiB",
    )?;
    Ok(())
}

/// Decode one bounded, sealed chunk. Manifest-only validation is not full geometry verification.
pub fn decode_chunk(
    model: &ReferenceModel,
    index: usize,
    bytes: &[u8],
) -> ReferenceResult<Vec<ReferenceTriangle>> {
    let chunk = model
        .chunks
        .get(index)
        .ok_or_else(|| invalid("reference.chunk", "Chunk index is outside manifest"))?;
    check(
        chunk.triangle_count > 0
            && chunk.triangle_count <= TRIANGLES_PER_CHUNK
            && bytes.len() == CHUNK_HEADER_BYTES + chunk.triangle_count * TRIANGLE_BYTES
            && bytes.len() == chunk.byte_length
            && bytes.len() <= CHUNK_HEADER_BYTES + TRIANGLES_PER_CHUNK * TRIANGLE_BYTES
            && sha256(bytes) == chunk.sha256,
        "reference.chunk",
        "Geometry chunk size/hash differs",
    )?;
    check(
        bytes.len() >= 16
            && &bytes[..8] == CHUNK_MAGIC
            && u32::from_le_bytes(bytes[8..12].try_into().unwrap()) == 1
            && u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize
                == chunk.triangle_count,
        "reference.chunk",
        "Unrecognized geometry chunk header",
    )?;
    let mut triangles = Vec::with_capacity(chunk.triangle_count);
    for record in bytes[16..].as_chunks::<TRIANGLE_BYTES>().0 {
        let occurrence_index = u32::from_le_bytes(record[..4].try_into().unwrap());
        let material = u32::from_le_bytes(record[4..8].try_into().unwrap());
        check(
            (occurrence_index as usize) < model.occurrences.len()
                && (material == u32::MAX || (material as usize) < model.materials.len()),
            "reference.chunk.indices",
            "Geometry references a missing occurrence or material",
        )?;
        let mut vertices_m = [[0.; 3]; 3];
        for (i, v) in vertices_m.iter_mut().flatten().enumerate() {
            *v = f64::from_le_bytes(record[8 + i * 8..16 + i * 8].try_into().unwrap());
        }
        check(
            vertices_m
                .iter()
                .flatten()
                .all(|v| v.is_finite() && v.abs() <= 1e8)
                && math::area(vertices_m) > 0.,
            "reference.chunk.vertices",
            "Nonfinite, out-of-range or degenerate triangle",
        )?;
        triangles.push(ReferenceTriangle {
            occurrence_index,
            material_index: (material != u32::MAX).then_some(material),
            vertices_m,
        });
    }
    check(
        triangles.len() == chunk.triangle_count,
        "reference.chunk",
        "Truncated triangle data",
    )?;
    Ok(triangles)
}

/// Recompute actual normalized measurements. Caller reads canonical bounded files;
/// no source GLB, engine, cached timestamp or imported executable recipe is needed.
pub fn verify_geometry<'a>(
    model: &ReferenceModel,
    chunks: impl IntoIterator<Item = &'a [u8]>,
    mut checkpoint: impl FnMut(&ReferenceProgress) -> bool,
) -> ReferenceResult<()> {
    validate_manifest(model)?;
    let mut monitor = Monitor::new(&mut checkpoint);
    let mut counts = vec![0usize; model.occurrences.len()];
    let mut areas = vec![0.; model.occurrences.len()];
    let mut bounds: Vec<Option<ReferenceBounds>> = vec![None; model.occurrences.len()];
    let mut all_bounds = None;
    let mut total = 0;
    let mut bytes = 0;
    let mut seen_chunks = 0;
    let mut total_area = 0.;
    for (i, chunk) in chunks.into_iter().enumerate() {
        monitor.step(
            "verifying normalized geometry",
            total,
            model.triangle_count,
            bytes,
        )?;
        for t in decode_chunk(model, i, chunk)? {
            let n = t.occurrence_index as usize;
            counts[n] += 1;
            total += 1;
            let area = math::area(t.vertices_m);
            areas[n] += area;
            total_area += area;
            for p in t.vertices_m {
                bounds[n]
                    .get_or_insert(ReferenceBounds::point(p))
                    .include(p);
                all_bounds
                    .get_or_insert(ReferenceBounds::point(p))
                    .include(p);
            }
            if total.is_multiple_of(1024) {
                monitor.step(
                    "verifying normalized geometry",
                    total,
                    model.triangle_count,
                    bytes,
                )?;
            }
        }
        bytes += chunk.len();
        seen_chunks += 1;
    }
    check(
        seen_chunks == model.chunks.len() && total == model.triangle_count,
        "reference.geometry",
        "Missing geometry chunks",
    )?;
    for (i, o) in model.occurrences.iter().enumerate() {
        check(
            counts[i] == o.triangle_count
                && math::near(areas[i], o.triangle_area_m2)
                && math::bounds_near(bounds[i], o.bounds),
            "reference.measurements",
            "Occurrence measurements differ from retained geometry",
        )?;
    }
    check(
        math::near(total_area, model.triangle_area_m2)
            && math::bounds_near(all_bounds, Some(model.bounds)),
        "reference.measurements",
        "Global measurements differ from retained geometry",
    )?;
    monitor.step("reference verified", total, total, bytes)
}
