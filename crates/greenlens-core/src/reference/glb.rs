use super::*;
use serde::{
    de::{SeqAccess, Visitor},
    Deserializer,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    marker::PhantomData,
    ops::Deref,
};

/// Reject oversized arrays while parsing; an 8 MiB JSON chunk must not expand
/// into millions of empty node/accessor structures before its limits are checked.
struct Limited<T, const N: usize>(Vec<T>);
impl<T, const N: usize> Default for Limited<T, N> {
    fn default() -> Self {
        Self(Vec::new())
    }
}
impl<T, const N: usize> Deref for Limited<T, N> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.0
    }
}
impl<'de, T: Deserialize<'de>, const N: usize> Deserialize<'de> for Limited<T, N> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Bound<T, const N: usize>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>, const N: usize> Visitor<'de> for Bound<T, N> {
            type Value = Limited<T, N>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "at most {N} entries")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(v) = a.next_element()? {
                    if values.len() == N {
                        return Err(serde::de::Error::custom(format!(
                            "Array exceeds {N} entry intake bound"
                        )));
                    }
                    values.push(v);
                }
                Ok(Limited(values))
            }
        }
        d.deserialize_seq(Bound::<T, N>(PhantomData))
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Asset {
    version: String,
    min_version: Option<String>,
    generator: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Buffer {
    byte_length: usize,
    uri: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct View {
    buffer: usize,
    #[serde(default)]
    byte_offset: usize,
    byte_length: usize,
    byte_stride: Option<usize>,
    target: Option<u32>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Accessor {
    buffer_view: Option<usize>,
    #[serde(default)]
    byte_offset: usize,
    component_type: u32,
    #[serde(default)]
    normalized: bool,
    count: usize,
    #[serde(rename = "type")]
    kind: String,
    sparse: Option<serde::de::IgnoredAny>,
    min: Option<Vec<f64>>,
    max: Option<Vec<f64>>,
}
#[derive(Deserialize)]
struct Primitive {
    attributes: BTreeMap<String, usize>,
    indices: Option<usize>,
    material: Option<usize>,
    mode: Option<u32>,
    targets: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    extensions: BTreeMap<String, serde::de::IgnoredAny>,
}
#[derive(Deserialize)]
struct Mesh {
    primitives: Limited<Primitive, MAX_ITEMS>,
    weights: Option<serde::de::IgnoredAny>,
}
#[derive(Deserialize)]
struct Node {
    name: Option<String>,
    mesh: Option<usize>,
    #[serde(default)]
    children: Limited<usize, MAX_NODES>,
    matrix: Option<[f64; 16]>,
    translation: Option<Vec3>,
    rotation: Option<[f64; 4]>,
    scale: Option<Vec3>,
    skin: Option<usize>,
    weights: Option<serde::de::IgnoredAny>,
    camera: Option<usize>,
    #[serde(default)]
    extensions: BTreeMap<String, serde::de::IgnoredAny>,
}
impl Node {
    fn transform(&self) -> ReferenceResult<math::Matrix> {
        if let Some(m) = self.matrix {
            check(
                self.translation.is_none() && self.rotation.is_none() && self.scale.is_none(),
                "node.transform",
                "A node cannot contain both matrix and TRS",
            )?;
            math::validate_affine(m, true)?;
            Ok(m)
        } else {
            math::trs(
                self.translation.unwrap_or([0.; 3]),
                self.rotation.unwrap_or([0., 0., 0., 1.]),
                self.scale.unwrap_or([1.; 3]),
            )
        }
    }
}
#[derive(Deserialize)]
struct Scene {
    #[serde(default)]
    nodes: Limited<usize, MAX_NODES>,
}
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Pbr {
    base_color_factor: Option<[f64; 4]>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Material {
    name: Option<String>,
    #[serde(default)]
    pbr_metallic_roughness: Pbr,
    #[serde(default)]
    double_sided: bool,
    alpha_mode: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Image {
    uri: Option<String>,
    buffer_view: Option<usize>,
    mime_type: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Gltf {
    asset: Asset,
    #[serde(default)]
    buffers: Limited<Buffer, 1>,
    #[serde(default)]
    buffer_views: Limited<View, MAX_ITEMS>,
    #[serde(default)]
    accessors: Limited<Accessor, MAX_ITEMS>,
    #[serde(default)]
    meshes: Limited<Mesh, MAX_ITEMS>,
    #[serde(default)]
    nodes: Limited<Node, MAX_NODES>,
    #[serde(default)]
    scenes: Limited<Scene, 256>,
    scene: Option<usize>,
    #[serde(default)]
    materials: Limited<Material, MAX_MATERIALS>,
    #[serde(default)]
    images: Limited<Image, MAX_TEXTURES>,
    #[serde(default)]
    textures: Limited<serde::de::IgnoredAny, MAX_TEXTURES>,
    #[serde(default)]
    animations: Limited<serde::de::IgnoredAny, 1>,
    #[serde(default)]
    skins: Limited<serde::de::IgnoredAny, 1>,
    #[serde(default)]
    extensions_required: Limited<String, 128>,
    #[serde(default)]
    extensions_used: Limited<String, 128>,
}
fn u32_at(bytes: &[u8], offset: usize) -> ReferenceResult<u32> {
    let end = offset
        .checked_add(4)
        .ok_or_else(|| invalid("glb", "Offset overflow"))?;
    let value = bytes
        .get(offset..end)
        .ok_or_else(|| invalid("glb", "Truncated GLB header"))?;
    Ok(u32::from_le_bytes(value.try_into().unwrap()))
}
fn container(bytes: &[u8]) -> ReferenceResult<(&[u8], &[u8], usize)> {
    check(
        bytes.len() <= MAX_SOURCE_BYTES,
        "glb.file",
        "GLB exceeds 128 MiB; export a selected scope or reduce source assets explicitly",
    )?;
    check(
        u32_at(bytes, 0)? == 0x4654_6c67 && u32_at(bytes, 4)? == 2,
        "glb.header",
        "Expected a GLB 2.0 file",
    )?;
    check(
        u32_at(bytes, 8)? as usize == bytes.len(),
        "glb.length",
        "Declared GLB length differs from actual bytes",
    )?;
    let mut offset = 12usize;
    let mut json = None;
    let mut bin = None;
    let mut unknown = 0;
    while offset < bytes.len() {
        let length = u32_at(bytes, offset)? as usize;
        let kind = u32_at(bytes, offset + 4)?;
        check(
            length.is_multiple_of(4),
            "glb.chunk",
            "GLB chunks must have 4-byte alignment",
        )?;
        let start = offset
            .checked_add(8)
            .ok_or_else(|| invalid("glb.chunk", "Offset overflow"))?;
        let end = start
            .checked_add(length)
            .ok_or_else(|| invalid("glb.chunk", "Length overflow"))?;
        let chunk = bytes
            .get(start..end)
            .ok_or_else(|| invalid("glb.chunk", "Truncated GLB chunk"))?;
        if offset == 12 {
            check(kind == 0x4e4f_534a, "glb.chunk", "First chunk must be JSON")?;
        }
        match kind {
            0x4e4f_534a => {
                check(
                    json.is_none() && offset == 12 && length <= MAX_JSON_BYTES,
                    "glb.json",
                    "Exactly one first JSON chunk, no larger than 8 MiB, is supported",
                )?;
                json = Some(chunk);
            }
            0x004e_4942 => {
                check(
                    json.is_some() && bin.is_none() && unknown == 0,
                    "glb.bin",
                    "Only one BIN chunk directly after JSON is supported",
                )?;
                bin = Some(chunk);
            }
            _ => {
                check(json.is_some(), "glb.chunk", "Unknown chunk precedes JSON")?;
                unknown += 1;
                check(
                    unknown <= 128,
                    "glb.chunks",
                    "More than128 unknown container chunks is unsupported",
                )?;
            }
        }
        offset = end;
    }
    Ok((
        json.ok_or_else(|| invalid("glb.json", "Missing JSON"))?,
        bin.unwrap_or(&[]),
        unknown,
    ))
}

struct Data<'a> {
    document: &'a Gltf,
    bin: &'a [u8],
}
struct Span<'a> {
    bytes: &'a [u8],
    stride: usize,
    count: usize,
    component: u32,
}
impl<'a> Data<'a> {
    fn span(&self, index: usize, position: bool) -> ReferenceResult<Span<'a>> {
        let a = self
            .document
            .accessors
            .get(index)
            .ok_or_else(|| invalid("accessor", "Accessor index is outside array"))?;
        check(a.sparse.is_none()&&!a.normalized,"accessor","Sparse/normalized geometry accessors are unsupported; export static float positions and integer indices")?;
        check(
            a.count > 0 && a.count <= MAX_TRIANGLES * 3,
            "accessor.count",
            "Accessor count is empty or exceeds geometry bound",
        )?;
        let (size, expected_kind) = if position {
            check(
                a.component_type == 5126,
                "accessor.position",
                "Positions must use float32 VEC3",
            )?;
            (12, "VEC3")
        } else {
            (
                match a.component_type {
                    5121 => 1,
                    5123 => 2,
                    5125 => 4,
                    _ => {
                        return Err(invalid(
                            "accessor.indices",
                            "Indices must be unsigned byte, short or int",
                        ))
                    }
                },
                "SCALAR",
            )
        };
        check(
            a.kind == expected_kind,
            "accessor.type",
            "Unsupported geometry accessor type",
        )?;
        let v = self
            .document
            .buffer_views
            .get(
                a.buffer_view.ok_or_else(|| {
                    invalid("accessor.bufferView", "Geometry requires a buffer view")
                })?,
            )
            .ok_or_else(|| invalid("accessor.bufferView", "Buffer view index is outside array"))?;
        let stride = v.byte_stride.unwrap_or(size);
        let component_size = if position { 4 } else { size };
        let start = v
            .byte_offset
            .checked_add(a.byte_offset)
            .ok_or_else(|| invalid("accessor", "Offset overflow"))?;
        check(
            stride >= size
                && stride <= 252
                && stride % component_size == 0
                && a.byte_offset % component_size == 0
                && start % component_size == 0
                && (!position || (a.byte_offset.is_multiple_of(4) && stride.is_multiple_of(4)))
                && (position || v.byte_stride.is_none()),
            "accessor.alignment",
            "Invalid accessor alignment/stride (index buffer views cannot be interleaved)",
        )?;
        if let Some(target) = v.target {
            check(
                target == if position { 34962 } else { 34963 },
                "bufferView.target",
                "Geometry buffer target disagrees with use",
            )?;
        }
        let length = (a.count - 1)
            .checked_mul(stride)
            .and_then(|n| n.checked_add(size))
            .ok_or_else(|| invalid("accessor", "Accessor extent overflow"))?;
        let end = a
            .byte_offset
            .checked_add(length)
            .ok_or_else(|| invalid("accessor", "Accessor offset overflow"))?;
        check(
            end <= v.byte_length,
            "accessor.range",
            "Accessor extends beyond its buffer view",
        )?;
        let absolute_end = start
            .checked_add(length)
            .ok_or_else(|| invalid("accessor", "Extent overflow"))?;
        let bytes = self
            .bin
            .get(start..absolute_end)
            .ok_or_else(|| invalid("accessor", "Accessor exceeds BIN data"))?;
        Ok(Span {
            bytes,
            stride,
            count: a.count,
            component: a.component_type,
        })
    }
}
impl Span<'_> {
    fn point(&self, index: usize) -> ReferenceResult<Vec3> {
        check(
            index < self.count,
            "primitive.indices",
            "Triangle index exceeds POSITION count",
        )?;
        let offset = index * self.stride;
        let p = std::array::from_fn(|i| {
            f32::from_le_bytes(
                self.bytes[offset + i * 4..offset + i * 4 + 4]
                    .try_into()
                    .unwrap(),
            ) as f64
        });
        check(
            p.iter().all(|x: &f64| x.is_finite()),
            "accessor.position",
            "Positions contain NaN/infinity",
        )?;
        Ok(p)
    }
    fn index(&self, i: usize) -> usize {
        let start = i * self.stride;
        match self.component {
            5121 => self.bytes[start] as usize,
            5123 => u16::from_le_bytes(self.bytes[start..start + 2].try_into().unwrap()) as usize,
            _ => u32::from_le_bytes(self.bytes[start..start + 4].try_into().unwrap()) as usize,
        }
    }
}

fn validate_document(g: &Gltf, bin: &[u8]) -> ReferenceResult<Vec<Option<usize>>> {
    check(
        g.asset.version == "2.0" && g.asset.min_version.as_deref().is_none_or(|v| v == "2.0"),
        "asset.version",
        "Only glTF 2.0 assets/minVersion are supported",
    )?;
    if let Some(generator) = &g.asset.generator {
        text(generator, "asset.generator", 2048)?;
    }
    check(g.extensions_required.is_empty(),"extensionsRequired",&format!("Unsupported required extensions: {}. Export uncompressed static triangles without required extensions",g.extensions_required.join(", ")))?;
    for extension in g.extensions_used.iter() {
        text(extension, "extensionsUsed", 128)?;
        check(extension.starts_with("KHR_materials_") || extension=="KHR_texture_transform" || extension=="KHR_lights_punctual",
            "extensionsUsed", "An optional geometry/unknown extension is unsupported; export core static triangles without instancing/compression extensions")?;
    }
    check(
        g.animations.is_empty() && g.skins.is_empty(),
        "geometry",
        "Animation and skins are unsupported; export an explicitly frozen static model",
    )?;
    check(
        g.buffers.len() == 1 && !g.nodes.is_empty() && !g.scenes.is_empty(),
        "glb.scene",
        "Expected a scene and one self-contained BIN buffer",
    )?;
    check(
        g.scene.is_none_or(|i| i < g.scenes.len()),
        "scene",
        "Default scene index is outside array",
    )?;
    let buffer = &g.buffers[0];
    check(
        buffer.uri.is_none(),
        "buffer.uri",
        "External and data URI buffers are not fetched; export one self-contained GLB",
    )?;
    check(
        buffer.byte_length <= bin.len() && bin.len() - buffer.byte_length <= 3,
        "buffer.byteLength",
        "BIN length/padding disagrees with buffer",
    )?;
    for v in g.buffer_views.iter() {
        check(
            v.buffer == 0
                && v.byte_length > 0
                && v.byte_offset
                    .checked_add(v.byte_length)
                    .is_some_and(|end| end <= buffer.byte_length),
            "bufferView",
            "Invalid buffer/range",
        )?;
        if let Some(stride) = v.byte_stride {
            check(
                (4..=252).contains(&stride) && stride.is_multiple_of(4),
                "bufferView.byteStride",
                "Expected stride 4..252, multiple of4",
            )?;
        }
    }
    for image in g.images.iter() {
        check(
            image.uri.is_none(),
            "image.uri",
            "External/data URI images are not admitted; embed them or export geometry-only GLB",
        )?;
        check(
            image.buffer_view.is_some_and(|v| v < g.buffer_views.len()),
            "image.bufferView",
            "Embedded image references an invalid buffer view",
        )?;
        check(image.mime_type.as_deref().is_some_and(|m|["image/png","image/jpeg"].contains(&m)),"image.mimeType","Only inert embedded PNG/JPEG image declarations are admitted (not decoded or retained)")?;
    }
    let mut parents = vec![None; g.nodes.len()];
    let mut edges = 0;
    for (i, n) in g.nodes.iter().enumerate() {
        if let Some(name) = &n.name {
            text(name, "node.name", 512)?;
        }
        check(
            n.skin.is_none() && n.weights.is_none(),
            "node",
            "Skin/morph nodes are unsupported",
        )?;
        check(
            n.extensions.keys().all(|k| k == "KHR_lights_punctual"),
            "node.extensions",
            "Unsupported node extension could alter geometry; export core static triangles",
        )?;
        n.transform()?;
        if let Some(mesh) = n.mesh {
            check(
                mesh < g.meshes.len(),
                "node.mesh",
                "Mesh index is outside array",
            )?;
        }
        for &child in n.children.iter() {
            check(
                child < g.nodes.len() && child != i && parents[child].is_none(),
                "node.children",
                "Invalid child, cycle or multiple parent node",
            )?;
            parents[child] = Some(i);
            edges += 1;
        }
    }
    check(
        edges < g.nodes.len(),
        "node.children",
        "Scene graph has no tree root",
    )?;
    for i in 0..g.nodes.len() {
        let mut at = Some(i);
        let mut depth = 0;
        while let Some(n) = at {
            depth += 1;
            check(
                depth <= MAX_DEPTH,
                "node.depth",
                "Scene graph contains a cycle or exceeds depth64",
            )?;
            at = parents[n];
        }
    }
    for scene in g.scenes.iter() {
        let mut roots = BTreeSet::new();
        for &node in scene.nodes.iter() {
            check(
                node < g.nodes.len() && parents[node].is_none() && roots.insert(node),
                "scene.nodes",
                "Scene roots must exist, be unique and have no parent",
            )?;
        }
    }
    for a in g.accessors.iter() {
        check(
            a.count > 0 && a.count <= MAX_TRIANGLES * 3,
            "accessor.count",
            "Accessor exceeds element limit",
        )?;
        check(
            a.buffer_view.is_none_or(|v| v < g.buffer_views.len()),
            "accessor.bufferView",
            "Invalid accessor buffer view",
        )?;
        for values in [&a.min, &a.max].into_iter().flatten() {
            check(
                values.len() <= 16 && values.iter().all(|v| v.is_finite()),
                "accessor.bounds",
                "Invalid accessor bounds",
            )?;
        }
    }
    let mut primitive_count = 0usize;
    for mesh in g.meshes.iter() {
        check(
            mesh.weights.is_none() && !mesh.primitives.is_empty(),
            "mesh",
            "Morph weights/empty meshes are unsupported",
        )?;
        primitive_count = primitive_count
            .checked_add(mesh.primitives.len())
            .ok_or_else(|| invalid("mesh", "Primitive count overflow"))?;
        check(
            primitive_count <= MAX_ITEMS,
            "mesh.primitives",
            "More than40,000 mesh primitives",
        )?;
        for p in mesh.primitives.iter() {
            check(p.extensions.keys().all(|k|k=="KHR_materials_variants"),"primitive.extensions","Unsupported primitive extension could alter geometry; export uncompressed static triangles")?;
            check(
                p.mode.unwrap_or(4) == 4 && p.targets.is_none(),
                "mesh.primitive",
                "Only static TRIANGLES are supported; export triangulated geometry",
            )?;
            check(
                p.attributes.contains_key("POSITION") && p.attributes.len() <= 32,
                "mesh.attributes",
                "Missing POSITION or too many attributes",
            )?;
            for (name, &index) in &p.attributes {
                text(name, "attribute", 128)?;
                check(
                    index < g.accessors.len(),
                    "mesh.attributes",
                    "Attribute index is outside accessor array",
                )?;
            }
            check(
                p.material.is_none_or(|m| m < g.materials.len()),
                "mesh.material",
                "Material index is outside array",
            )?;
        }
    }
    Ok(parents)
}

/// Admit an inert, bounded static GLB. False from checkpoint cancels without an
/// accepted candidate. No paths are opened, URLs fetched, or commands interpreted.
pub fn admit_glb(
    bytes: &[u8],
    options: &ReferenceImportOptions,
    mut checkpoint: impl FnMut(&ReferenceProgress) -> bool,
) -> ReferenceResult<AdmittedReference> {
    options.validate()?;
    let mut monitor = Monitor::new(&mut checkpoint);
    monitor.step("checking GLB container", 0, bytes.len(), 0)?;
    let (json, bin, unknown_chunks) = container(bytes)?;
    monitor.step("reading bounded GLB metadata", 0, json.len(), 12)?;
    let g: Gltf = serde_json::from_slice(json)?;
    validate_document(&g, bin)?;
    let scene = options
        .scene_index
        .or(g.scene)
        .or((g.scenes.len() == 1).then_some(0))
        .ok_or_else(|| {
            invalid(
                "scene",
                "Several scenes and no default: choose the intended scene explicitly",
            )
        })?;
    check(
        scene < g.scenes.len(),
        "scene",
        "Selected/default scene index is outside array",
    )?;
    // Bound instance amplification before allocating normalized geometry. One
    // source mesh may be referenced by many separate scene nodes.
    let mut count_stack: Vec<_> = g.scenes[scene].nodes.to_vec();
    let mut expected_triangles = 0usize;
    while let Some(i) = count_stack.pop() {
        let n = &g.nodes[i];
        if let Some(mesh) = n.mesh {
            for p in g.meshes[mesh].primitives.iter() {
                let accessor_index = p.indices.unwrap_or(p.attributes["POSITION"]);
                let a = g.accessors.get(accessor_index).ok_or_else(|| {
                    invalid("primitive.indices", "Accessor index is outside array")
                })?;
                check(
                    a.count.is_multiple_of(3),
                    "primitive.count",
                    "Triangle index/vertex count must be a multiple of3",
                )?;
                expected_triangles = expected_triangles
                    .checked_add(a.count / 3)
                    .ok_or_else(|| invalid("reference.triangles", "Instance count overflow"))?;
                check(expected_triangles<=MAX_TRIANGLES,"reference.triangles","Instanced geometry exceeds1,000,000 triangles; export a smaller explicit scope")?;
            }
        }
        count_stack.extend(n.children.iter().copied());
    }
    let source_sha256 = sha256(bytes);
    let revision_sha256 = revision(&source_sha256, options)?;
    let mut warnings = Vec::new();
    let mut exclusions=vec![
        "Reference geometry is not a thermal model; colors, names and bounds supply no construction, glazing, occupancy or HVAC properties.".into(),
        "Original architectural source and original GLB are excluded; retained assets are normalized geometry only, not a complete architectural-source archive.".into(),
        "Imported shading normals, UVs, vertex colors, textures, lights and camera settings are not retained; geometric face normals and base material colors are displayed.".into()];
    if g.scenes.len() > 1 {
        warnings.push(format!(
            "Only selected scene {scene} of {} is admitted; other scenes are excluded.",
            g.scenes.len()
        ));
    }
    if unknown_chunks > 0 {
        warnings.push(format!("Ignored {unknown_chunks} unknown GLB chunks, as permitted by the container specification."));
    }
    if !g.extensions_used.is_empty() {
        warnings.push(format!(
            "Optional extensions are not interpreted: {}. Core static geometry only.",
            g.extensions_used.join(", ")
        ));
    }
    if !g.images.is_empty() || !g.textures.is_empty() {
        warnings.push(format!(
            "{} embedded images and {} texture declarations are not decoded or retained.",
            g.images.len(),
            g.textures.len()
        ));
    }
    if options.metres_per_source_unit != 1. {
        warnings.push("An explicit scale correction overrides the glTF metre convention; confirm known dimensions before mapping.".into());
    }
    if options.declared_north_clockwise_deg.is_none() {
        warnings.push("True north/location are unknown. Display axes are model-local until separately verified.".into());
    }
    let mut materials = Vec::with_capacity(g.materials.len());
    for (i, m) in g.materials.iter().enumerate() {
        let label = m.name.clone().unwrap_or_else(|| format!("Material {i}"));
        text(&label, "material.name", 512)?;
        let color = m
            .pbr_metallic_roughness
            .base_color_factor
            .unwrap_or([1.; 4]);
        check(
            color
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.).contains(v)),
            "material.baseColorFactor",
            "Expected finite RGBA factors [0,1]",
        )?;
        let alpha = m.alpha_mode.clone().unwrap_or_else(|| "OPAQUE".into());
        check(
            ["OPAQUE", "MASK", "BLEND"].contains(&alpha.as_str()),
            "material.alphaMode",
            "Invalid alpha mode",
        )?;
        materials.push(ReferenceMaterial {
            source_index: i,
            label,
            base_color_rgba: color,
            double_sided: m.double_sided,
            alpha_mode: alpha,
        });
    }
    let root = math::root_transform(options);
    let mut stack: Vec<_> = g.scenes[scene]
        .nodes
        .iter()
        .rev()
        .map(|&i| (i, root, Vec::<usize>::new(), None::<String>))
        .collect();
    let data = Data { document: &g, bin };
    let mut occurrences = Vec::new();
    let mut all_bounds = None;
    let mut triangle_area = 0.;
    let mut triangle_count = 0usize;
    let mut pending = Vec::with_capacity(TRIANGLES_PER_CHUNK);
    let mut chunks = Vec::new();
    let mut chunk_manifest = Vec::new();
    let flush = |pending: &mut Vec<ReferenceTriangle>,
                 chunks: &mut Vec<Vec<u8>>,
                 manifest: &mut Vec<ReferenceChunk>| {
        if !pending.is_empty() {
            let bytes = encode_chunk(pending);
            manifest.push(ReferenceChunk {
                path: format!("geometry/{:05}.bin", chunks.len()),
                sha256: sha256(&bytes),
                byte_length: bytes.len(),
                triangle_count: pending.len(),
            });
            chunks.push(bytes);
            pending.clear();
        }
    };
    while let Some((node_index, parent, path, parent_id)) = stack.pop() {
        monitor.step(
            "transforming scene occurrences",
            occurrences.len(),
            g.nodes.len(),
            bytes.len(),
        )?;
        let n = &g.nodes[node_index];
        let world = math::multiply(parent, n.transform()?);
        math::validate_affine(world, false)?;
        let mut node_path = path;
        node_path.push(node_index);
        let id = occurrence_id(&revision_sha256, scene, &node_path)?;
        let mirrored = math::determinant(world) < 0.;
        let occurrence_index = occurrences.len() as u32;
        let mut bounds = None;
        let mut occurrence_area = 0.;
        let mut occurrence_triangles = 0usize;
        if let Some(mesh_index) = n.mesh {
            for primitive in g.meshes[mesh_index].primitives.iter() {
                let positions = data.span(primitive.attributes["POSITION"], true)?;
                let indices = primitive.indices.map(|i| data.span(i, false)).transpose()?;
                let count = indices.as_ref().map_or(positions.count, |i| i.count);
                check(
                    count.is_multiple_of(3),
                    "primitive.count",
                    "Triangle index/vertex count must be a multiple of3",
                )?;
                check(triangle_count.checked_add(count/3).is_some_and(|n|n<=MAX_TRIANGLES),"reference.triangles","Instanced geometry exceeds1,000,000 triangles; export a smaller explicit scope")?;
                // Check all referenced POSITION values and required declared extrema;
                // bounds of rendered geometry are recomputed from used triangles below.
                let accessor = &g.accessors[primitive.attributes["POSITION"]];
                let declared_min =
                    accessor
                        .min
                        .as_ref()
                        .filter(|v| v.len() == 3)
                        .ok_or_else(|| {
                            invalid(
                                "POSITION.min",
                                "POSITION requires three declared minimum values",
                            )
                        })?;
                let declared_max =
                    accessor
                        .max
                        .as_ref()
                        .filter(|v| v.len() == 3)
                        .ok_or_else(|| {
                            invalid(
                                "POSITION.max",
                                "POSITION requires three declared maximum values",
                            )
                        })?;
                let mut actual_bounds = None;
                for i in 0..positions.count {
                    let p = positions.point(i)?;
                    actual_bounds
                        .get_or_insert(ReferenceBounds::point(p))
                        .include(p);
                    if i.is_multiple_of(1024) {
                        monitor.step(
                            "checking vertex accessors",
                            i,
                            positions.count,
                            bytes.len(),
                        )?;
                    }
                }
                let actual = actual_bounds.unwrap();
                check(
                    (0..3).all(|i| {
                        math::near(actual.min_m[i], declared_min[i] as f32 as f64)
                            && math::near(actual.max_m[i], declared_max[i] as f32 as f64)
                    }),
                    "POSITION.bounds",
                    "Declared POSITION extrema differ from decoded float32 values",
                )?;
                for triangle in 0..count / 3 {
                    if triangle_count.is_multiple_of(1024) {
                        monitor.step(
                            "normalizing triangles",
                            triangle_count,
                            expected_triangles,
                            bytes.len(),
                        )?;
                    }
                    let mut v = [[0.; 3]; 3];
                    for (corner, dest) in v.iter_mut().enumerate() {
                        let index = indices
                            .as_ref()
                            .map_or(triangle * 3 + corner, |i| i.index(triangle * 3 + corner));
                        *dest = math::transform(world, positions.point(index)?)?;
                    }
                    if mirrored {
                        v.swap(1, 2);
                    }
                    let area = math::area(v);
                    check(area.is_finite()&&area>0.,"reference.triangle","Degenerate/overflowed triangles are unsupported; repair the source explicitly (none were silently dropped)")?;
                    for p in v {
                        bounds.get_or_insert(ReferenceBounds::point(p)).include(p);
                        all_bounds
                            .get_or_insert(ReferenceBounds::point(p))
                            .include(p);
                    }
                    occurrence_area += area;
                    triangle_area += area;
                    occurrence_triangles += 1;
                    triangle_count += 1;
                    pending.push(ReferenceTriangle {
                        occurrence_index,
                        material_index: primitive.material.map(|i| i as u32),
                        vertices_m: v,
                    });
                    if pending.len() == TRIANGLES_PER_CHUNK {
                        flush(&mut pending, &mut chunks, &mut chunk_manifest);
                    }
                }
            }
        }
        occurrences.push(ReferenceOccurrence {
            id: id.clone(),
            node_index,
            parent_id,
            label: n
                .name
                .clone()
                .unwrap_or_else(|| format!("Node {node_index}")),
            node_path: node_path.clone(),
            source_mesh_index: n.mesh,
            world_from_mesh: world,
            mirrored,
            bounds,
            triangle_count: occurrence_triangles,
            triangle_area_m2: occurrence_area,
        });
        for &child in n.children.iter().rev() {
            stack.push((child, world, node_path.clone(), Some(id.clone())));
        }
    }
    flush(&mut pending, &mut chunks, &mut chunk_manifest);
    check(
        triangle_count > 0,
        "scene",
        "Selected scene contains no triangle geometry",
    )?;
    if occurrences.len() < g.nodes.len() {
        exclusions.push(format!(
            "{} nodes outside the selected scene are not retained.",
            g.nodes.len() - occurrences.len()
        ));
    }
    if g.nodes.iter().any(|n| n.camera.is_some()) {
        warnings.push(
            "Source camera objects are not imported; use explicit GreenLens named views.".into(),
        );
    }
    let normalized_bytes = chunks.iter().map(Vec::len).sum::<usize>();
    let estimate = bytes.len()
        + json.len() * 16
        + normalized_bytes
        + occurrences.len() * 1024
        + TRIANGLES_PER_CHUNK * TRIANGLE_BYTES;
    check(
        normalized_bytes <= MAX_NORMALIZED_BYTES && estimate <= MAX_DECODED_ESTIMATE_BYTES,
        "reference.resources",
        "Conservative decoded allocation budget exceeded",
    )?;
    let mut model=ReferenceModel{schema_version:REFERENCE_SCHEMA.into(),importer_id:IMPORTER_ID.into(),source_sha256,revision_sha256,geometry_sha256:String::new(),
        options:options.clone(),source_units:"metres (glTF 2.0)".into(),coordinate_convention:"right-handed +X east/local-right, +Y north/local-back, +Z up; true north only when declared".into(),
        selected_scene:scene,source_node_count:g.nodes.len(),source_mesh_count:g.meshes.len(),source_material_count:g.materials.len(),source_texture_count:g.textures.len(),
        occurrences,materials,chunks:chunk_manifest,bounds:all_bounds.unwrap(),triangle_count,triangle_area_m2:triangle_area,warnings,exclusions,
        storage_policy:"normalized-geometry-only; original architectural source and GLB excluded".into(),
        resources:ReferenceResources{elapsed_ms:monitor.elapsed_ms(),source_bytes:bytes.len(),json_bytes:json.len(),normalized_bytes,decoded_allocation_estimate_bytes:estimate}};
    model.geometry_sha256 = geometry_identity(&model)?;
    validate_manifest(&model)?;
    monitor.step(
        "reference admitted",
        triangle_count,
        triangle_count,
        bytes.len(),
    )?;
    model.resources.elapsed_ms = monitor.elapsed_ms();
    Ok(AdmittedReference {
        manifest: model,
        chunks,
    })
}
