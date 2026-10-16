//! Resource-boundary fixtures only. The reference is admitted from original
//! geometry; deliberately expanded inventories are never accepted as reference
//! evidence or presented as engine records.
use super::*;
use crate::{reference, study_revisions, ProjectView};
use greenlens_core::{
    architectural_scope::{
        ArchitecturalScope, ScopeCoverage, ScopeKind, ARCHITECTURAL_SCOPE_SCHEMA,
    },
    canonical_hash,
    reference::ReferenceImportOptions,
};
use serde_json::json;
use std::{collections::BTreeMap, fs::OpenOptions};

fn original_triangle() -> Vec<u8> {
    let binary: Vec<u8> = [[0f32, 0., 0.], [1., 0., 0.], [0., 1., 0.]]
        .into_iter()
        .flatten()
        .flat_map(f32::to_le_bytes)
        .collect();
    let mut metadata = serde_json::to_vec(&json!({
        "asset":{"version":"2.0"},"buffers":[{"byteLength":36}],
        "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],
        "nodes":[{"name":"Original capacity fixture","mesh":0}],"scenes":[{"nodes":[0]}],"scene":0
    })).unwrap();
    while !metadata.len().is_multiple_of(4) {
        metadata.push(b' ');
    }
    let mut bytes = Vec::new();
    bytes.extend(0x4654_6c67u32.to_le_bytes());
    bytes.extend(2u32.to_le_bytes());
    bytes.extend(((28 + metadata.len() + binary.len()) as u32).to_le_bytes());
    bytes.extend((metadata.len() as u32).to_le_bytes());
    bytes.extend(0x4e4f_534au32.to_le_bytes());
    bytes.extend(metadata);
    bytes.extend((binary.len() as u32).to_le_bytes());
    bytes.extend(0x004e_4942u32.to_le_bytes());
    bytes.extend(binary);
    bytes
}

fn fixture() -> (tempfile::TempDir, PathBuf, ProjectView) {
    let parent = crate::local_root()
        .unwrap()
        .join("verification/capacity-tests");
    fs::create_dir_all(&parent).unwrap();
    let temp = tempfile::tempdir_in(parent).unwrap();
    let root = temp.path().join("project");
    let source = temp.path().join("original triangle.glb");
    fs::write(&source, original_triangle()).unwrap();
    let saved =
        crate::save_project(&root, &crate::energy_services::example_document(), None).unwrap();
    let imported = reference::import_project(
        &root,
        &saved.project,
        Some(&saved.revision),
        &source,
        &ReferenceImportOptions::default(),
        &AtomicBool::new(false),
        |_| true,
    )
    .unwrap();
    (temp, root, imported)
}

fn inventory(root: &Path, project: &Project) -> CapacityReport {
    inspect(
        Some(root),
        project,
        &Operation::Inspect,
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn limit<'a>(report: &'a CapacityReport, name: &str) -> &'a CapacityLimit {
    report.limits.iter().find(|row| row.name == name).unwrap()
}

fn sparse(path: &Path, length: u64) {
    let file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .unwrap();
    #[cfg(windows)]
    {
        use std::{ffi::c_void, os::windows::io::AsRawHandle};
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn DeviceIoControl(
                handle: *mut c_void,
                code: u32,
                input: *const c_void,
                input_size: u32,
                output: *mut c_void,
                output_size: u32,
                returned: *mut u32,
                overlapped: *mut c_void,
            ) -> i32;
        }
        let mut returned = 0;
        // Only this owned test file is made sparse; no large allocation or
        // scientific engine output is needed for a file-size admission test.
        let ok = unsafe {
            DeviceIoControl(
                file.as_raw_handle(),
                0x0009_00c4,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        assert_ne!(
            ok, 0,
            "The resource fixture requires a sparse-capable filesystem"
        );
    }
    file.set_len(length).unwrap();
}

/// Deliberate metadata-only inventory extension. This is not valid reference
/// evidence: reference verification must reject its unbound extra members.
fn expanded_inventory(root: &Path, project: &Project, sizes: &[u64]) -> PathBuf {
    let asset = &project.architecture.as_ref().unwrap().assets[0];
    let directory = root.join(&asset.path);
    let mut seal: Checksums = read_json(&directory.join("checksums.json")).unwrap();
    for (index, size) in sizes.iter().enumerate() {
        let name = format!("capacity-only-{index:04}.bin");
        sparse(&directory.join(&name), *size);
        seal.sha256.insert(name, "0".repeat(64));
    }
    write_json(&directory.join("checksums.json"), &seal).unwrap();
    directory
}

#[test]
fn retained_scope_reports_its_effective_16_mib_manifest_bound() {
    let (_temp, root, saved) = fixture();
    let without = inventory(&root, &saved.project);
    assert_eq!(
        limit(&without, "candidate manifest bytes").limit,
        MAX_EVIDENCE_FILE_BYTES
    );
    let (reference, designs) =
        study_revisions::current_context(&root, &saved.project, &AtomicBool::new(false)).unwrap();
    let validated =
        greenlens_core::study_revision::validate_revision_context(&reference, &designs).unwrap();
    let scope = ArchitecturalScope {
        schema_version: ARCHITECTURAL_SCOPE_SCHEMA.into(),
        title: "Capacity documentation fixture".into(),
        purpose: "Inventory testing only; no energy or physical-validation claim".into(),
        scope_kind: ScopeKind::RepresentativeStudy,
        reference_revision_sha256: reference.manifest.revision_sha256.clone(),
        reference_geometry_sha256: reference.manifest.geometry_sha256.clone(),
        reference_options_sha256: canonical_hash(&reference.manifest.options).unwrap(),
        design_model_sha256: validated
            .iter()
            .map(|(id, value)| (id.clone(), value.model_input_sha256.clone()))
            .collect(),
        mappings: vec![],
        included_scope: vec!["Original synthetic fixture".into()],
        excluded_scope: vec!["Physical validation".into()],
        boundary_consequences: "No geometric equivalence is asserted".into(),
        assumptions: vec![],
        reconciliations: vec![],
        coverage: ScopeCoverage::Unknown {
            reason: "No architectural denominator".into(),
        },
        whole_building_review: None,
        report_views: vec![],
    };
    let candidate =
        study_revisions::initialize_scope(&root, &saved.project, scope, &AtomicBool::new(false))
            .unwrap();
    let with_history = inventory(&root, &candidate);
    assert_eq!(
        limit(&with_history, "candidate manifest bytes").limit,
        study_revisions::MAX_REVISION_PROJECT_BYTES as u64
    );
    assert_eq!(limit(&with_history, "scope revision count").used, 1);
    assert_eq!(
        with_history.project_content_sha256,
        canonical_hash(&candidate).unwrap()
    );
    assert!(with_history.can_start);
    assert_eq!(revision(&root).unwrap(), Some(saved.revision));
}

#[test]
fn capacity_is_not_a_same_length_tamper_verification_cache() {
    let (_temp, root, saved) = fixture();
    let before = inventory(&root, &saved.project);
    let manifest_before = fs::read(root.join("project.json")).unwrap();
    let asset = &saved.project.architecture.as_ref().unwrap().assets[0];
    let manifest = reference::read_manifest(&root, asset).unwrap();
    let chunk = root.join(&asset.path).join(&manifest.chunks[0].path);
    let mut bytes = fs::read(&chunk).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    fs::write(&chunk, &bytes).unwrap();
    let after = inventory(&root, &saved.project);
    assert_eq!(after.retained_bytes, before.retained_bytes);
    assert_eq!(after.retained_entries, before.retained_entries);
    assert!(after.can_start);
    assert!(after.inventory_scope.contains("not content-integrity"));
    assert!(crate::save_project(&root, &saved.project, Some(&saved.revision)).is_err());
    assert!(crate::open_project(&root).is_err());
    assert_eq!(
        fs::read(root.join("project.json")).unwrap(),
        manifest_before
    );
}

#[test]
fn exact_and_excess_entry_bytes_preserve_manifest_and_staged_files() {
    let (_temp, root, saved) = fixture();
    let directory = expanded_inventory(&root, &saved.project, &[MAX_EVIDENCE_FILE_BYTES]);
    let exact = inventory(&root, &saved.project);
    assert_eq!(
        limit(&exact, "largest retained entry").used,
        MAX_EVIDENCE_FILE_BYTES
    );
    assert!(exact.can_start);
    let manifest = fs::read(root.join("project.json")).unwrap();
    let oversized = directory.join("capacity-only-0000.bin");
    OpenOptions::new()
        .write(true)
        .open(&oversized)
        .unwrap()
        .set_len(MAX_EVIDENCE_FILE_BYTES + 1)
        .unwrap();
    assert!(!inventory(&root, &saved.project).can_start);
    let error = atomic_project(&root, &saved.project).unwrap_err();
    assert!(error.to_string().contains("capacity"), "{error:#}");
    assert_eq!(fs::read(root.join("project.json")).unwrap(), manifest);
    assert_eq!(
        fs::metadata(&oversized).unwrap().len(),
        MAX_EVIDENCE_FILE_BYTES + 1
    );
    assert!(!fs::read_dir(&root).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".project-")));
}

#[test]
fn exact_and_excess_total_bytes_use_actual_sizes_without_allocating_payloads() {
    let (_temp, root, saved) = fixture();
    let directory = expanded_inventory(
        &root,
        &saved.project,
        &[
            MAX_EVIDENCE_FILE_BYTES,
            MAX_EVIDENCE_FILE_BYTES,
            MAX_EVIDENCE_FILE_BYTES,
            0,
        ],
    );
    let initial = inventory(&root, &saved.project);
    let tail = directory.join("capacity-only-0003.bin");
    let remaining = MAX_PACKAGE_BYTES - initial.retained_bytes;
    assert!(remaining < MAX_EVIDENCE_FILE_BYTES);
    OpenOptions::new()
        .write(true)
        .open(&tail)
        .unwrap()
        .set_len(remaining)
        .unwrap();
    let exact = inventory(&root, &saved.project);
    assert_eq!(exact.retained_bytes, MAX_PACKAGE_BYTES);
    assert_eq!(exact.available_bytes, 0);
    assert!(exact.can_start);
    assert!(!inspect(
        Some(&root),
        &saved.project,
        &Operation::ImportReference,
        &AtomicBool::new(false)
    )
    .unwrap()
    .warnings
    .is_empty());
    OpenOptions::new()
        .write(true)
        .open(&tail)
        .unwrap()
        .set_len(remaining + 1)
        .unwrap();
    let over = inventory(&root, &saved.project);
    assert_eq!(over.retained_bytes, MAX_PACKAGE_BYTES + 1);
    assert!(!over.can_start);
    assert!(require_publication(&root, &saved.project, &[], &AtomicBool::new(false)).is_err());
    assert_eq!(revision(&root).unwrap(), Some(saved.revision));
}

#[test]
fn exact_and_excess_member_counts_are_bounded_independently_of_bytes() {
    let (_temp, root, saved) = fixture();
    let asset = &saved.project.architecture.as_ref().unwrap().assets[0];
    let directory = root.join(&asset.path);
    let before = inventory(&root, &saved.project);
    let needed = MAX_PACKAGE_ENTRIES - before.retained_entries;
    expanded_inventory(&root, &saved.project, &vec![0; needed]);
    let exact = inventory(&root, &saved.project);
    assert_eq!(exact.retained_entries, MAX_PACKAGE_ENTRIES);
    assert_eq!(exact.available_entries, 0);
    assert!(exact.can_start);
    assert!(require_start(
        &root,
        &saved.project,
        &Operation::ImportReference,
        &AtomicBool::new(false)
    )
    .is_err());
    fs::write(directory.join("one-extra.bin"), []).unwrap();
    let mut seal: Checksums = read_json(&directory.join("checksums.json")).unwrap();
    seal.sha256.insert("one-extra.bin".into(), bytes_hash(&[]));
    write_json(&directory.join("checksums.json"), &seal).unwrap();
    let over = inventory(&root, &saved.project);
    assert_eq!(over.retained_entries, MAX_PACKAGE_ENTRIES + 1);
    assert!(!over.can_start);
    assert!(require_publication(&root, &saved.project, &[], &AtomicBool::new(false)).is_err());
    assert_eq!(revision(&root).unwrap(), Some(saved.revision));
}

#[test]
fn cancelled_publication_preserves_prior_manifest_and_complete_candidate() {
    let (_temp, root, saved) = fixture();
    let before = fs::read(root.join("project.json")).unwrap();
    let mut candidate = saved.project.clone();
    candidate.name = "Cancelled candidate remains inspectable".into();
    let candidate_identity = canonical_hash(&candidate).unwrap();
    let error = atomic_project_cancellable(&root, &candidate, &AtomicBool::new(true)).unwrap_err();
    assert!(error.to_string().contains("cancelled"), "{error:#}");
    assert_eq!(canonical_hash(&candidate).unwrap(), candidate_identity);
    assert_eq!(fs::read(root.join("project.json")).unwrap(), before);
    assert_eq!(revision(&root).unwrap(), Some(saved.revision));
    assert_eq!(
        crate::open_project(&root).unwrap().project.name,
        saved.project.name
    );
    assert!(!fs::read_dir(&root).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".project-")));
}

#[test]
fn staged_capacity_rejection_does_not_promote_or_remove_completed_assets() {
    let (_temp, root, saved) = fixture();
    let asset = &saved.project.architecture.as_ref().unwrap().assets[0];
    let stage = root.parent().unwrap().join("completed external stage");
    fs::create_dir(&stage).unwrap();
    sparse(&stage.join("too-large.bin"), MAX_EVIDENCE_FILE_BYTES + 1);
    write_json(
        &stage.join("checksums.json"),
        &Checksums {
            schema_version: "0.1.0".into(),
            sha256: BTreeMap::from([("too-large.bin".into(), "0".repeat(64))]),
        },
    )
    .unwrap();
    let manifest = fs::read(root.join("project.json")).unwrap();
    let before = fs::read(root.join(&asset.path).join("checksums.json")).unwrap();
    assert!(require_publication(
        &root,
        &saved.project,
        &[(asset.path.clone(), stage.clone())],
        &AtomicBool::new(false)
    )
    .is_err());
    assert_eq!(fs::read(root.join("project.json")).unwrap(), manifest);
    assert_eq!(
        fs::read(root.join(&asset.path).join("checksums.json")).unwrap(),
        before
    );
    assert_eq!(
        fs::metadata(stage.join("too-large.bin")).unwrap().len(),
        MAX_EVIDENCE_FILE_BYTES + 1
    );
}
