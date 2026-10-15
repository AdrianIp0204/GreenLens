//! Actual bounded GLB admission and portable storage regressions. The small GLB
//! is original synthetic geometry. These checks do not imply native UI,
//! private architectural intake, or simulation/physical validation acceptance.
use greenlens_core::{canonical_hash, reference::ReferenceImportOptions, sha256};
use greenlens_runner::{
    energy_services, open_project,
    persistence::{
        self, PackageBudget, MAX_EVIDENCE_FILE_BYTES, MAX_PACKAGE_BYTES, MAX_PACKAGE_ENTRIES,
    },
    reference, save_project, save_project_as, ProjectView,
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

fn temporary() -> tempfile::TempDir {
    let parent = greenlens_runner::local_root()
        .unwrap()
        .join("verification/reference-intake-tests");
    fs::create_dir_all(&parent).unwrap();
    tempfile::Builder::new()
        .prefix("synthetic reference 空間 ")
        .tempdir_in(parent)
        .unwrap()
}

/// One original metre-sized triangle. A repeated mesh is instanced through two
/// source nodes, one with an explicit nonuniform mirrored transform.
fn source_glb() -> Vec<u8> {
    let mut binary = Vec::new();
    for point in [[0f32, 0., 0.], [1., 0., 0.], [0., 1., 0.]] {
        for coordinate in point {
            binary.extend(coordinate.to_le_bytes());
        }
    }
    for index in [0u16, 1, 2] {
        binary.extend(index.to_le_bytes());
    }
    let document = json!({
        "asset": {"version":"2.0", "generator":"GreenLens original synthetic reference storage fixture"},
        "buffers":[{"byteLength":42}],
        "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36,"target":34962},
            {"buffer":0,"byteOffset":36,"byteLength":6,"target":34963}],
        "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]},
            {"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}],
        "meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}],
        "nodes":[{"name":"Original synthetic group","children":[1,2]},
            {"name":"Repeated label","mesh":0},
            {"name":"Repeated label","mesh":0,"translation":[2,0,0],"scale":[-1,2,1]}],
        "scenes":[{"nodes":[0]}], "scene":0
    });
    let mut metadata = serde_json::to_vec(&document).unwrap();
    while !metadata.len().is_multiple_of(4) {
        metadata.push(b' ');
    }
    while !binary.len().is_multiple_of(4) {
        binary.push(0);
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

fn setup() -> (tempfile::TempDir, PathBuf, PathBuf, ProjectView) {
    let temp = temporary();
    let root = temp.path().join("project");
    let source = temp.path().join("source model 空間.glb");
    fs::write(&source, source_glb()).unwrap();
    let saved = save_project(&root, &energy_services::example_document(), None).unwrap();
    (temp, root, source, saved)
}

fn import(root: &Path, source: &Path, saved: &ProjectView) -> ProjectView {
    reference::import_project(
        root,
        &saved.project,
        Some(&saved.revision),
        source,
        &ReferenceImportOptions::default(),
        &AtomicBool::new(false),
        |_| true,
    )
    .unwrap()
}

fn files_under(root: &Path) -> BTreeMap<String, String> {
    fn visit(base: &Path, directory: &Path, result: &mut BTreeMap<String, String>) {
        for item in fs::read_dir(directory).unwrap() {
            let path = item.unwrap().path();
            if path.is_dir() {
                visit(base, &path, result);
            } else {
                let name = path
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                result.insert(name, sha256(&fs::read(path).unwrap()));
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

#[test]
fn actual_admission_portable_save_as_and_cold_reopen_need_neither_source_nor_original_project() {
    let (temp, root, source, saved) = setup();
    let original_source = fs::read(&source).unwrap();
    let original_thermal = canonical_hash(&saved.project.energy.as_ref().unwrap().designs).unwrap();
    let admitted = import(&root, &source, &saved);
    assert_eq!(admitted.project.schema_version, "0.6.0");
    assert_eq!(
        canonical_hash(&admitted.project.energy.as_ref().unwrap().designs).unwrap(),
        original_thermal
    );
    assert_eq!(fs::read(&source).unwrap(), original_source);
    let extension = admitted.project.architecture.as_ref().unwrap();
    let asset = &extension.assets[0];
    let manifest = reference::read_manifest(&root, asset).unwrap();
    assert_eq!(manifest.triangle_count, 2);
    assert_eq!(manifest.occurrences.len(), 3);
    assert_ne!(manifest.occurrences[1].id, manifest.occurrences[2].id);
    let immutable = files_under(&root.join(&asset.path));
    let copy = temp.path().join("portable copied project 空間");
    save_project_as(
        Some(&root),
        &copy,
        &admitted.project,
        Some(&admitted.revision),
    )
    .unwrap();
    assert_eq!(files_under(&copy.join(&asset.path)), immutable);
    fs::rename(&root, temp.path().join("unavailable original project")).unwrap();
    fs::rename(&source, temp.path().join("unavailable source.glb")).unwrap();
    let cold = open_project(&copy).unwrap();
    let copied_asset = &cold.project.architecture.as_ref().unwrap().assets[0];
    assert_eq!(
        reference::read_manifest(&copy, copied_asset).unwrap(),
        manifest
    );
    assert_eq!(
        reference::read_chunk(&copy, copied_asset, 0).unwrap().len(),
        2
    );
    assert_eq!(files_under(&copy.join(&asset.path)), immutable);
}

#[test]
fn importing_into_v04_document_keeps_real_legacy_run_bytes_and_thermal_identity() {
    let temp = temporary();
    let root = temp.path().join("legacy project");
    let package =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/legacy-static-v0.1.zip");
    persistence::extract_package(&package, &root).unwrap();
    let legacy = open_project(&root).unwrap();
    assert!(!legacy.project.runs.is_empty());
    let old_runs: Vec<_> = legacy
        .project
        .runs
        .iter()
        .map(|run| (run.path.clone(), files_under(&root.join(&run.path))))
        .collect();
    let mut v04 = legacy.project.clone();
    v04.schema_version = "0.4.0".into();
    v04.energy = energy_services::example_document().energy;
    let saved = save_project(&root, &v04, Some(&legacy.revision)).unwrap();
    let thermal_before = canonical_hash(&saved.project.energy).unwrap();
    let source = temp.path().join("original synthetic.glb");
    fs::write(&source, source_glb()).unwrap();
    let admitted = import(&root, &source, &saved);
    assert_eq!(
        canonical_hash(&admitted.project.energy).unwrap(),
        thermal_before
    );
    assert_eq!(admitted.results.len(), legacy.results.len());
    for (path, hashes) in old_runs {
        assert_eq!(files_under(&root.join(path)), hashes);
    }
}

#[test]
fn same_size_chunk_tampering_is_rejected_by_open_and_save_as() {
    let (temp, root, source, saved) = setup();
    let admitted = import(&root, &source, &saved);
    let asset = &admitted.project.architecture.as_ref().unwrap().assets[0];
    let chunk = root.join(&asset.path).join("geometry/00000.bin");
    let original = fs::read(&chunk).unwrap();
    let mut changed = original.clone();
    changed[24] ^= 1;
    fs::write(&chunk, &changed).unwrap();
    assert_eq!(changed.len(), original.len());
    let project_before = fs::read(root.join("project.json")).unwrap();
    assert!(open_project(&root).is_err());
    let copy = temp.path().join("rejected copy");
    assert!(save_project_as(
        Some(&root),
        &copy,
        &admitted.project,
        Some(&admitted.revision)
    )
    .is_err());
    assert!(!copy.join("project.json").exists());
    assert_eq!(fs::read(root.join("project.json")).unwrap(), project_before);
}

#[test]
fn cancelled_reference_verification_stops_before_any_asset_file_is_opened() {
    let root = temporary();
    let id = uuid::Uuid::new_v4().to_string();
    let asset = reference::ReferenceAsset {
        path: format!("references/{id}"),
        id,
        revision_sha256: "a".repeat(64),
        manifest_sha256: "b".repeat(64),
        geometry_sha256: "c".repeat(64),
    };
    let error = reference::verify_asset(root.path(), &asset, &AtomicBool::new(true)).unwrap_err();
    assert!(error.to_string().contains("cancelled"));
    assert!(!root.path().join("references").exists());
}

#[test]
fn unexpected_seal_members_are_rejected_before_following_their_paths() {
    let (_temp, root, source, saved) = setup();
    let admitted = import(&root, &source, &saved);
    let asset = &admitted.project.architecture.as_ref().unwrap().assets[0];
    let path = root.join(&asset.path).join("checksums.json");
    let mut sealed: persistence::Checksums = persistence::read_json(&path).unwrap();
    sealed
        .sha256
        .insert("unexpected-never-created.bin".into(), "a".repeat(64));
    persistence::write_json(&path, &sealed).unwrap();
    let error = reference::verify_asset(&root, asset, &AtomicBool::new(false)).unwrap_err();
    assert!(
        error.to_string().contains("missing or unexpected members"),
        "{error:#}"
    );
    assert!(!root
        .join(&asset.path)
        .join("unexpected-never-created.bin")
        .exists());
}

#[test]
fn seal_digests_are_bound_before_geometry_reads_and_cannot_reseal_modified_data() {
    let (_temp, root, source, saved) = setup();
    let admitted = import(&root, &source, &saved);
    let asset = &admitted.project.architecture.as_ref().unwrap().assets[0];
    let directory = root.join(&asset.path);
    let path = directory.join("checksums.json");
    let mut sealed: persistence::Checksums = persistence::read_json(&path).unwrap();
    let chunk_path = directory.join("geometry/00000.bin");
    let mut bytes = fs::read(&chunk_path).unwrap();
    bytes[24] ^= 1;
    fs::write(&chunk_path, &bytes).unwrap();
    sealed
        .sha256
        .insert("geometry/00000.bin".into(), sha256(&bytes));
    persistence::write_json(&path, &sealed).unwrap();
    let error = reference::verify_asset(&root, asset, &AtomicBool::new(false)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("digests disagree with the bound manifest"),
        "{error:#}"
    );
    fs::remove_file(&chunk_path).unwrap();
    let missing = reference::verify_asset(&root, asset, &AtomicBool::new(false)).unwrap_err();
    assert!(
        missing
            .to_string()
            .contains("digests disagree with the bound manifest"),
        "{missing:#}"
    );
}

#[test]
fn cancellation_before_and_during_admission_never_advertises_a_reference() {
    let (_temp, root, source, saved) = setup();
    let project_before = fs::read(root.join("project.json")).unwrap();
    let cancelled = AtomicBool::new(true);
    assert!(reference::import_project(
        &root,
        &saved.project,
        Some(&saved.revision),
        &source,
        &ReferenceImportOptions::default(),
        &cancelled,
        |_| true
    )
    .is_err());
    let cancelled = AtomicBool::new(false);
    assert!(reference::import_project(
        &root,
        &saved.project,
        Some(&saved.revision),
        &source,
        &ReferenceImportOptions::default(),
        &cancelled,
        |p| p.stage != "normalizing triangles"
    )
    .is_err());
    assert_eq!(fs::read(root.join("project.json")).unwrap(), project_before);
    assert!(!root.join("references").exists());
    assert!(open_project(&root).unwrap().project.architecture.is_none());
}

#[test]
fn cancellation_after_admission_before_promotion_keeps_existing_document() {
    let (_temp, root, source, saved) = setup();
    let before = fs::read(root.join("project.json")).unwrap();
    let cancelled = AtomicBool::new(false);
    assert!(reference::import_project(
        &root,
        &saved.project,
        Some(&saved.revision),
        &source,
        &ReferenceImportOptions::default(),
        &cancelled,
        |progress| {
            if progress.stage == "reference admitted" {
                cancelled.store(true, Ordering::Relaxed);
            }
            true
        }
    )
    .is_err());
    assert_eq!(fs::read(root.join("project.json")).unwrap(), before);
    assert!(!root.join("references").exists());
}

#[test]
fn external_revision_during_admission_is_preserved_without_reference_promotion() {
    let (_temp, root, source, saved) = setup();
    let mut external = saved.project.clone();
    external.name = "External writer must survive".into();
    let mut external_bytes = None;
    let result = reference::import_project(
        &root,
        &saved.project,
        Some(&saved.revision),
        &source,
        &ReferenceImportOptions::default(),
        &AtomicBool::new(false),
        |progress| {
            if progress.stage == "reference admitted" {
                save_project(&root, &external, Some(&saved.revision)).unwrap();
                external_bytes = Some(fs::read(root.join("project.json")).unwrap());
            }
            true
        },
    );
    assert!(result.is_err());
    assert_eq!(
        fs::read(root.join("project.json")).unwrap(),
        external_bytes.unwrap()
    );
    assert_eq!(open_project(&root).unwrap().project.name, external.name);
    assert!(!root.join("references").exists());
}

#[test]
fn corrupted_retained_run_prevents_reference_manifest_publication() {
    let temp = temporary();
    let root = temp.path().join("retained static project");
    let package =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/legacy-static-v0.1.zip");
    persistence::extract_package(&package, &root).unwrap();
    let saved = open_project(&root).unwrap();
    let run = &saved.project.runs[0];
    let seal: persistence::Checksums =
        persistence::read_json(&root.join(&run.path).join("checksums.json")).unwrap();
    let name = seal.sha256.keys().next().unwrap();
    let member = root.join(&run.path).join(name);
    let mut corrupted = fs::read(&member).unwrap();
    assert!(!corrupted.is_empty());
    corrupted[0] ^= 1;
    fs::write(&member, &corrupted).unwrap();
    let manifest_before = fs::read(root.join("project.json")).unwrap();
    let source = temp.path().join("original synthetic.glb");
    fs::write(&source, source_glb()).unwrap();
    let result = reference::import_project(
        &root,
        &saved.project,
        Some(&saved.revision),
        &source,
        &ReferenceImportOptions::default(),
        &AtomicBool::new(false),
        |_| true,
    );
    assert!(result.is_err());
    assert_eq!(
        fs::read(root.join("project.json")).unwrap(),
        manifest_before
    );
    assert_eq!(fs::read(member).unwrap(), corrupted);
    let retained: greenlens_runner::Project =
        persistence::read_json(&root.join("project.json")).unwrap();
    assert!(retained.architecture.is_none());
}

#[test]
fn cancellation_in_copy_verification_or_at_commit_boundary_never_publishes_candidate() {
    for target in [
        "copying admitted reference",
        "verifying candidate: architectural_reference",
        "ready to publish verified reference",
    ] {
        let (_temp, root, source, saved) = setup();
        let before = fs::read(root.join("project.json")).unwrap();
        let cancelled = AtomicBool::new(false);
        let mut observed = false;
        let result = reference::import_project(
            &root,
            &saved.project,
            Some(&saved.revision),
            &source,
            &ReferenceImportOptions::default(),
            &cancelled,
            |event| {
                if event.stage == target {
                    observed = true;
                    cancelled.store(true, Ordering::Relaxed);
                }
                true
            },
        );
        assert!(
            observed,
            "Required cancellation checkpoint was not reached: {target}"
        );
        assert!(
            result.is_err(),
            "Cancellation must reject before publication: {target}"
        );
        assert_eq!(fs::read(root.join("project.json")).unwrap(), before);
        assert!(open_project(&root).unwrap().project.architecture.is_none());
    }
}

#[test]
fn successful_import_reuses_verified_candidate_and_does_not_relabel_old_open_receipt() {
    let (_temp, root, source, saved) = setup();
    let mut completed = 0;
    let admitted = reference::import_project(
        &root,
        &saved.project,
        Some(&saved.revision),
        &source,
        &ReferenceImportOptions::default(),
        &AtomicBool::new(false),
        |event| {
            if event.stage == "verifying candidate: completed" {
                completed += 1;
            }
            true
        },
    )
    .unwrap();
    assert_eq!(completed, 1);
    assert!(
        admitted.project_verification.is_none(),
        "Old manifest receipt cannot represent a newly published document"
    );
    assert_eq!(
        admitted.revision,
        sha256(&fs::read(root.join("project.json")).unwrap())
    );
    let reopened = open_project(&root).unwrap();
    assert_eq!(reopened.revision, admitted.revision);
    assert_eq!(
        reopened.project_verification.unwrap().manifest_sha256,
        admitted.revision
    );
}

#[test]
fn normalized_asset_package_is_bounded_readable_and_explicitly_excludes_original() {
    let (temp, root, source, saved) = setup();
    let admitted = import(&root, &source, &saved);
    let extension = admitted.project.architecture.as_ref().unwrap();
    let members = reference::evidence_members(&root, extension).unwrap();
    assert_eq!(members.len(), 3); // manifest, one normalized chunk, its seal
    assert!(members
        .iter()
        .all(|p| p.starts_with("references/") && !p.ends_with(".glb") && !p.ends_with(".skp")));
    let manifest = reference::read_manifest(&root, &extension.assets[0]).unwrap();
    assert!(manifest
        .storage_policy
        .contains("original architectural source and GLB excluded"));
    assert!(manifest
        .exclusions
        .iter()
        .any(|s| s.contains("not a complete architectural-source archive")));
    let mut files = vec!["project.json".to_owned()];
    files.extend(members);
    persistence::seal(&root, &files.iter().map(String::as_str).collect::<Vec<_>>()).unwrap();
    files.push("checksums.json".into());
    let archive = temp.path().join("normalized only.zip");
    persistence::zip_directory(&root, &files, &archive).unwrap();
    let restored = temp.path().join("archive reopened");
    persistence::extract_package(&archive, &restored).unwrap();
    let reopened = open_project(&restored).unwrap();
    assert_eq!(
        canonical_hash(&reopened.project.architecture).unwrap(),
        canonical_hash(&admitted.project.architecture).unwrap()
    );
    assert!(!restored.join("source model 空間.glb").exists());
}

#[test]
fn report_and_reference_members_share_the_original_archive_limits() {
    // Metadata-only budgeting exercises exact limits without allocating hundreds
    // of MiB, running engines, or fabricating report/simulation acceptance.
    let mut budget = PackageBudget::default();
    assert!(budget
        .add(
            "references/reference/geometry/00000.bin",
            MAX_EVIDENCE_FILE_BYTES + 1
        )
        .is_err());
    let mut budget = PackageBudget::default();
    for i in 0..4 {
        budget
            .add(&format!("reports/product-{i}.pdf"), MAX_EVIDENCE_FILE_BYTES)
            .unwrap();
    }
    assert_eq!(4 * MAX_EVIDENCE_FILE_BYTES, MAX_PACKAGE_BYTES);
    assert!(budget
        .add("references/reference/reference.json", 1)
        .is_err());
    let mut budget = PackageBudget::default();
    for i in 0..MAX_PACKAGE_ENTRIES {
        budget.add(&format!("reports/entry-{i}.json"), 0).unwrap();
    }
    assert!(budget
        .add("references/reference/checksums.json", 0)
        .is_err());
    assert!(PackageBudget::default()
        .add("../private-source.glb", 1)
        .is_err());
    assert!(PackageBudget::default()
        .add("C:/private-source.glb", 1)
        .is_err());
}

#[test]
#[ignore = "Requires retained real energy evidence; invoke explicitly with GREENLENS_TEST_RETAINED_ENERGY_PROJECT"]
fn retained_energy_evidence_is_immutable_through_reference_import_and_cold_copy() {
    let source = PathBuf::from(
        std::env::var("GREENLENS_TEST_RETAINED_ENERGY_PROJECT")
            .expect("Required retained v0.4/v0.5 energy project is unavailable"),
    );
    let old = open_project(&source)
        .expect("Retained energy evidence must pass full integrity verification");
    let runs = &old
        .project
        .energy
        .as_ref()
        .expect("Expected energy project")
        .runs;
    assert!(
        !runs.is_empty(),
        "Required real retained energy runs are missing"
    );
    let before: Vec<_> = runs
        .iter()
        .map(|run| (run.path.clone(), files_under(&source.join(&run.path))))
        .collect();
    let temp = temporary();
    let working = temp.path().join("retained working copy");
    let copied =
        save_project_as(Some(&source), &working, &old.project, Some(&old.revision)).unwrap();
    let glb = temp.path().join("synthetic reference.glb");
    fs::write(&glb, source_glb()).unwrap();
    let admitted = import(&working, &glb, &copied);
    let cold = temp.path().join("cold copy");
    save_project_as(
        Some(&working),
        &cold,
        &admitted.project,
        Some(&admitted.revision),
    )
    .unwrap();
    fs::rename(&working, temp.path().join("unavailable working copy")).unwrap();
    fs::rename(&glb, temp.path().join("unavailable synthetic.glb")).unwrap();
    let reopened = open_project(&cold).unwrap();
    assert_eq!(reopened.energy_results.len(), old.energy_results.len());
    for (relative, hashes) in before {
        assert_eq!(files_under(&source.join(&relative)), hashes);
        assert_eq!(files_under(&cold.join(relative)), hashes);
    }
}
