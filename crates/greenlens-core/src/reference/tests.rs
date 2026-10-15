//! Original synthetic fixtures. No private architectural geometry or assets.
//! Expected coordinates use the declared 1e-8 m + 1e-9 relative tolerance.
use super::*;
use serde_json::{json, Value};
use std::time::Duration;

fn fixture() -> (Value, Vec<u8>) {
    let mut bin = Vec::new();
    for p in [[0f32, 0., 0.], [1., 0., 0.], [0., 1., 0.]] {
        for c in p {
            bin.extend(c.to_le_bytes());
        }
    }
    for i in [0u16, 1, 2] {
        bin.extend(i.to_le_bytes());
    }
    let doc = json!({"asset":{"version":"2.0","generator":"GreenLens original synthetic fixture"},
        "buffers":[{"byteLength":42}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36,"target":34962},
            {"buffer":0,"byteOffset":36,"byteLength":6,"target":34963}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]},
            {"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}],
        "materials":[{"name":"Synthetic neutral","pbrMetallicRoughness":{"baseColorFactor":[0.4,0.5,0.6,1]}}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1,"material":0}]}],
        "nodes":[{"name":"Triangle","mesh":0}],"scenes":[{"nodes":[0]}],"scene":0});
    (doc, bin)
}
fn glb(mut doc: Value, mut bin: Vec<u8>) -> Vec<u8> {
    doc["buffers"][0]["byteLength"] = json!(bin.len());
    while !bin.len().is_multiple_of(4) {
        bin.push(0);
    }
    let mut json = serde_json::to_vec(&doc).unwrap();
    while !json.len().is_multiple_of(4) {
        json.push(b' ');
    }
    let mut bytes = Vec::new();
    bytes.extend(0x4654_6c67u32.to_le_bytes());
    bytes.extend(2u32.to_le_bytes());
    bytes.extend(((28 + json.len() + bin.len()) as u32).to_le_bytes());
    bytes.extend((json.len() as u32).to_le_bytes());
    bytes.extend(0x4e4f_534au32.to_le_bytes());
    bytes.extend(json);
    bytes.extend((bin.len() as u32).to_le_bytes());
    bytes.extend(0x004e_4942u32.to_le_bytes());
    bytes.extend(bin);
    bytes
}
fn admit(doc: Value, bin: Vec<u8>) -> AdmittedReference {
    admit_glb(&glb(doc, bin), &ReferenceImportOptions::default(), |_| true).unwrap()
}
fn rejected(doc: Value, bin: Vec<u8>, needle: &str) {
    let e = admit_glb(&glb(doc, bin), &ReferenceImportOptions::default(), |_| true)
        .unwrap_err()
        .to_string();
    assert!(e.contains(needle), "{e} does not contain {needle}");
}
fn assert_point(a: Vec3, b: Vec3) {
    for i in 0..3 {
        assert!(math::near(a[i], b[i]), "{a:?} != {b:?}");
    }
}

#[test]
fn metres_y_up_convert_to_right_handed_z_up_and_verify_cold_chunks() {
    let (doc, bin) = fixture();
    let a = admit(doc, bin);
    assert_eq!(a.manifest.triangle_count, 1);
    assert_point(a.manifest.bounds.min_m, [0., 0., 0.]);
    assert_point(a.manifest.bounds.max_m, [1., 0., 1.]);
    assert_eq!(a.manifest.triangle_area_m2, 0.5);
    verify_geometry(&a.manifest, a.chunks.iter().map(Vec::as_slice), |_| true).unwrap();
    let t = decode_chunk(&a.manifest, 0, &a.chunks[0]).unwrap();
    assert_point(t[0].vertices_m[2], [0., 0., 1.]);
    assert!(a.manifest.options.declared_north_clockwise_deg.is_none());
    assert!(a
        .manifest
        .exclusions
        .iter()
        .any(|s| s.contains("not a thermal model")));
}
#[test]
fn nested_repeated_mirrored_nonuniform_instances_have_distinct_ids_and_correct_winding() {
    let (mut doc, bin) = fixture();
    doc["nodes"] = json!([{"name":"Repeated name","translation":[10,20,30],"children":[1,2]},
        {"name":"Repeated name","mesh":0,"scale":[-2,3,1]},
        {"name":"Repeated name","mesh":0,"translation":[1,0,0]}]);
    let a = admit(doc, bin);
    let t = decode_chunk(&a.manifest, 0, &a.chunks[0]).unwrap();
    assert_eq!(a.manifest.occurrences.len(), 3);
    assert_eq!(t.len(), 2);
    assert!(a.manifest.occurrences[0].bounds.is_none());
    assert_ne!(a.manifest.occurrences[1].id, a.manifest.occurrences[2].id);
    assert!(a.manifest.occurrences[1].mirrored);
    assert!(!a.manifest.occurrences[2].mirrored);
    assert_point(t[0].vertices_m[0], [10., -30., 20.]);
    assert_point(t[0].vertices_m[1], [10., -30., 23.]);
    assert_point(t[0].vertices_m[2], [8., -30., 20.]);
    assert_point(t[1].vertices_m[0], [11., -30., 20.]);
    assert!(math::near(a.manifest.occurrences[1].triangle_area_m2, 3.));
    verify_geometry(&a.manifest, a.chunks.iter().map(Vec::as_slice), |_| true).unwrap();
}
#[test]
fn column_major_matrix_and_nested_trs_composition_are_equivalent() {
    let (mut doc, bin) = fixture();
    doc["nodes"][0]["matrix"] = json!([2, 0, 0, 0, 0, 3, 0, 0, 0, 0, 4, 0, 5, 6, 7, 1]);
    let matrix = admit(doc.clone(), bin.clone());
    doc["nodes"][0].as_object_mut().unwrap().remove("matrix");
    doc["nodes"][0]["scale"] = json!([2, 3, 4]);
    doc["nodes"][0]["translation"] = json!([5, 6, 7]);
    let trs = admit(doc, bin);
    assert_eq!(matrix.chunks, trs.chunks);
    assert_ne!(
        matrix.manifest.revision_sha256,
        trs.manifest.revision_sha256
    );
}
#[test]
fn explicit_scale_rotation_origin_change_revision_without_north_inference() {
    let (doc, bin) = fixture();
    let bytes = glb(doc, bin);
    let default = admit_glb(&bytes, &ReferenceImportOptions::default(), |_| true).unwrap();
    let options = ReferenceImportOptions {
        metres_per_source_unit: 0.001,
        clockwise_rotation_deg: 90.,
        translation_m: [2., 3., 4.],
        ..Default::default()
    };
    let a = admit_glb(&bytes, &options, |_| true).unwrap();
    let t = decode_chunk(&a.manifest, 0, &a.chunks[0]).unwrap();
    assert_point(t[0].vertices_m[0], [2., 3., 4.]);
    assert_point(t[0].vertices_m[1], [2., 2.999, 4.]);
    assert_point(t[0].vertices_m[2], [2., 3., 4.001]);
    assert_ne!(a.manifest.revision_sha256, default.manifest.revision_sha256);
    assert_ne!(
        a.manifest.occurrences[0].id,
        default.manifest.occurrences[0].id
    );
}
#[test]
fn unindexed_float_positions_and_byte_indices_are_supported() {
    let (mut doc, bin) = fixture();
    doc["meshes"][0]["primitives"][0]
        .as_object_mut()
        .unwrap()
        .remove("indices");
    let a = admit(doc, bin);
    assert_eq!(a.manifest.triangle_count, 1);
    let (mut doc, mut bin) = fixture();
    bin.truncate(36);
    bin.extend([0, 1, 2]);
    doc["bufferViews"][1]["byteLength"] = json!(3);
    doc["accessors"][1]["componentType"] = json!(5121);
    assert_eq!(admit(doc, bin).manifest.triangle_count, 1);
}
#[test]
fn uint_indices_and_interleaved_positions_are_supported() {
    let (mut doc, _) = fixture();
    let mut bin = Vec::new();
    for p in [[0f32, 0., 0., 7.], [1., 0., 0., 8.], [0., 1., 0., 9.]] {
        for c in p {
            bin.extend(c.to_le_bytes());
        }
    }
    for i in [0u32, 1, 2] {
        bin.extend(i.to_le_bytes());
    }
    doc["bufferViews"][0]["byteLength"] = json!(48);
    doc["bufferViews"][0]["byteStride"] = json!(16);
    doc["bufferViews"][1]["byteOffset"] = json!(48);
    doc["bufferViews"][1]["byteLength"] = json!(12);
    doc["accessors"][1]["componentType"] = json!(5125);
    assert_eq!(admit(doc, bin).manifest.triangle_area_m2, 0.5);
}
#[test]
fn malformed_index_finite_and_accessor_extent_fail_without_partial_admission() {
    let (doc, mut bin) = fixture();
    bin[40..42].copy_from_slice(&99u16.to_le_bytes());
    rejected(doc, bin, "index exceeds");
    let (doc, mut bin) = fixture();
    bin[..4].copy_from_slice(&f32::NAN.to_le_bytes());
    rejected(doc, bin, "NaN");
    let (mut doc, bin) = fixture();
    // Keep float32 alignment valid so this specifically exercises checked extent
    // addition rather than being rejected earlier by the alignment guard.
    doc["accessors"][0]["byteOffset"] = json!(usize::MAX - 3);
    rejected(doc, bin, "overflow");
    let (mut doc, bin) = fixture();
    doc["accessors"][1]["count"] = json!(6);
    rejected(doc, bin, "beyond");
    let (mut doc, bin) = fixture();
    doc["accessors"][0]["max"] = json!([9, 1, 0]);
    rejected(doc, bin, "extrema differ");
}
#[test]
fn unknown_required_geometry_extensions_and_external_resources_fail_closed() {
    let (mut doc, bin) = fixture();
    doc["extensionsRequired"] = json!(["KHR_draco_mesh_compression"]);
    rejected(doc, bin, "Unsupported required");
    let (mut doc, bin) = fixture();
    doc["extensionsUsed"] = json!(["EXT_mesh_gpu_instancing"]);
    rejected(doc, bin, "geometry/unknown");
    let (mut doc, bin) = fixture();
    doc["buffers"][0]["uri"] = json!("https://invalid.example/private.bin");
    rejected(doc, bin, "not fetched");
    let (mut doc, bin) = fixture();
    doc["images"] = json!([{"uri":"file:///private.png"}]);
    rejected(doc, bin, "not admitted");
}
#[test]
fn optional_display_extensions_and_embedded_textures_are_explicitly_excluded() {
    let (mut doc, bin) = fixture();
    doc["extensionsUsed"] = json!(["KHR_materials_unlit"]);
    doc["images"] = json!([{"bufferView":0,"mimeType":"image/png"}]);
    doc["textures"] = json!([{"source":0}]);
    let a = admit(doc, bin);
    assert!(a
        .manifest
        .warnings
        .iter()
        .any(|w| w.contains("not decoded or retained")));
    assert!(a
        .manifest
        .exclusions
        .iter()
        .any(|w| w.contains("shading normals")));
}
#[test]
fn multiple_scenes_require_explicit_default_and_disclose_omissions() {
    let (mut doc, bin) = fixture();
    doc["scenes"] = json!([{"nodes":[0]},{"nodes":[0]}]);
    doc.as_object_mut().unwrap().remove("scene");
    rejected(doc.clone(), bin.clone(), "choose");
    let a = admit_glb(
        &glb(doc, bin),
        &ReferenceImportOptions {
            scene_index: Some(1),
            ..Default::default()
        },
        |_| true,
    )
    .unwrap();
    assert_eq!(a.manifest.selected_scene, 1);
    assert!(a
        .manifest
        .warnings
        .iter()
        .any(|w| w.contains("other scenes are excluded")));
}
#[test]
fn cycles_multiple_parents_duplicate_roots_and_depth_limit_reject() {
    let (mut doc, bin) = fixture();
    doc["nodes"] = json!([{"children":[1]},{"children":[0],"mesh":0}]);
    rejected(doc, bin, "tree root");
    let (mut doc, bin) = fixture();
    doc["nodes"] = json!([{"children":[2]},{"children":[2]},{"mesh":0}]);
    rejected(doc, bin, "multiple parent");
    let (mut doc, bin) = fixture();
    doc["scenes"][0]["nodes"] = json!([0, 0]);
    rejected(doc, bin, "unique");
    let (mut doc, bin) = fixture();
    let nodes: Vec<_> = (0..65)
        .map(|i| {
            if i == 64 {
                json!({"mesh":0})
            } else {
                json!({"children":[i+1]})
            }
        })
        .collect();
    doc["nodes"] = json!(nodes);
    rejected(doc, bin, "depth64");
}
#[test]
fn singular_sheared_ambiguous_transforms_and_degenerate_triangles_reject() {
    let (mut doc, bin) = fixture();
    doc["nodes"][0]["scale"] = json!([0, 1, 1]);
    rejected(doc, bin, "Singular");
    let (mut doc, bin) = fixture();
    doc["nodes"][0]["matrix"] = json!([1, 0, 0, 0, 1, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
    rejected(doc, bin, "without shear");
    let (mut doc, bin) = fixture();
    doc["nodes"][0]["matrix"] = json!(math::identity());
    doc["nodes"][0]["translation"] = json!([0, 0, 0]);
    rejected(doc, bin, "both matrix and TRS");
    let (doc, mut bin) = fixture();
    bin[40..42].copy_from_slice(&1u16.to_le_bytes());
    rejected(doc, bin, "Degenerate");
}
#[test]
fn sparse_animated_morphed_and_nontriangle_geometry_reject() {
    let (mut doc, bin) = fixture();
    doc["accessors"][0]["sparse"] = json!({"count":1});
    rejected(doc, bin, "Sparse");
    let (mut doc, bin) = fixture();
    doc["animations"] = json!([{}]);
    rejected(doc, bin, "Animation");
    let (mut doc, bin) = fixture();
    doc["meshes"][0]["primitives"][0]["targets"] = json!([]);
    rejected(doc, bin, "TRIANGLES");
    let (mut doc, bin) = fixture();
    doc["meshes"][0]["primitives"][0]["mode"] = json!(1);
    rejected(doc, bin, "TRIANGLES");
}
#[test]
fn resource_limits_are_checked_before_expansion() {
    let (mut doc, bin) = fixture();
    doc["accessors"][0]["count"] = json!(MAX_TRIANGLES * 3 + 1);
    rejected(doc, bin, "element limit");
    let (mut doc, bin) = fixture();
    doc["nodes"] = json!(vec![json!({}); MAX_NODES + 1]);
    rejected(doc, bin, "entry intake bound");
    let (mut doc, bin) = fixture();
    doc["nodes"][0]["name"] = json!("x".repeat(513));
    rejected(doc, bin, "Text exceeds");
    let (mut doc, bin) = fixture();
    doc["extras"] = json!({"unused":"x".repeat(MAX_JSON_BYTES)});
    rejected(doc, bin, "8 MiB");
    let (mut doc, bin) = fixture();
    doc["accessors"][1]["count"] = json!(1_800_000);
    doc["nodes"] = json!([{"mesh":0},{"mesh":0}]);
    doc["scenes"][0]["nodes"] = json!([0, 1]);
    rejected(doc, bin, "Instanced geometry exceeds");
}
#[test]
fn cancellation_before_parse_and_mid_geometry_never_returns_candidate() {
    let (doc, bin) = fixture();
    let bytes = glb(doc, bin);
    assert!(matches!(
        admit_glb(&bytes, &ReferenceImportOptions::default(), |_| false),
        Err(ReferenceError::Cancelled)
    ));
    assert!(matches!(
        admit_glb(&bytes, &ReferenceImportOptions::default(), |p| p.stage
            != "normalizing triangles"),
        Err(ReferenceError::Cancelled)
    ));
    let mut callback = |_: &ReferenceProgress| true;
    let mut monitor = Monitor::new(&mut callback);
    monitor.started = Instant::now() - Duration::from_secs(31);
    assert!(matches!(
        monitor.step("test", 0, 1, 0),
        Err(ReferenceError::Deadline)
    ));
}
#[test]
fn chunks_are_bounded_split_and_tampering_or_missing_chunks_fail() {
    let (mut doc, mut bin) = fixture();
    bin.truncate(36);
    for _ in 0..(TRIANGLES_PER_CHUNK + 1) {
        for i in [0u16, 1, 2] {
            bin.extend(i.to_le_bytes());
        }
    }
    doc["bufferViews"][1]["byteLength"] = json!((TRIANGLES_PER_CHUNK + 1) * 6);
    doc["accessors"][1]["count"] = json!((TRIANGLES_PER_CHUNK + 1) * 3);
    let a = admit(doc, bin);
    assert_eq!(a.chunks.len(), 2);
    assert_eq!(a.manifest.chunks[1].triangle_count, 1);
    verify_geometry(&a.manifest, a.chunks.iter().map(Vec::as_slice), |_| true).unwrap();
    assert!(verify_geometry(&a.manifest, [a.chunks[0].as_slice()], |_| true).is_err());
    let mut changed = a.chunks[0].clone();
    changed[20] ^= 1;
    assert!(decode_chunk(&a.manifest, 0, &changed).is_err());
    assert!(matches!(
        verify_geometry(&a.manifest, a.chunks.iter().map(Vec::as_slice), |_| false),
        Err(ReferenceError::Cancelled)
    ));
}
#[test]
fn manifest_geometry_identity_and_independent_measurement_checks_are_distinct() {
    let (doc, bin) = fixture();
    let a = admit(doc, bin);
    let mut m = a.manifest.clone();
    m.occurrences[0].triangle_area_m2 = 7.;
    assert!(validate_manifest(&m).is_err());
    m.geometry_sha256 = geometry_identity(&m).unwrap();
    validate_manifest(&m).unwrap();
    assert!(verify_geometry(&m, a.chunks.iter().map(Vec::as_slice), |_| true).is_err());
    let mut m = a.manifest.clone();
    m.chunks[0].path = "../geometry.bin".into();
    m.geometry_sha256 = geometry_identity(&m).unwrap();
    assert!(validate_manifest(&m).is_err());
}
#[test]
fn source_header_version_truncation_and_duplicate_chunk_reject() {
    let (doc, bin) = fixture();
    let mut bytes = glb(doc, bin);
    bytes[4..8].copy_from_slice(&1u32.to_le_bytes());
    assert!(admit_glb(&bytes, &ReferenceImportOptions::default(), |_| true).is_err());
    let (doc, bin) = fixture();
    let mut bytes = glb(doc, bin);
    bytes.pop();
    assert!(admit_glb(&bytes, &ReferenceImportOptions::default(), |_| true).is_err());
    assert!(admit_glb(&[], &ReferenceImportOptions::default(), |_| true).is_err());
}
#[test]
fn source_bytes_not_names_define_revision_and_reimport_identity() {
    let (doc, bin) = fixture();
    let a = admit(doc.clone(), bin.clone());
    let b = admit(doc.clone(), bin.clone());
    assert_eq!(a.manifest.revision_sha256, b.manifest.revision_sha256);
    assert_eq!(a.manifest.geometry_sha256, b.manifest.geometry_sha256);
    let mut changed = doc;
    changed["nodes"][0]["name"] = json!("Different label");
    let c = admit(changed, bin);
    assert_ne!(a.manifest.revision_sha256, c.manifest.revision_sha256);
    assert_eq!(a.chunks, c.chunks);
}
