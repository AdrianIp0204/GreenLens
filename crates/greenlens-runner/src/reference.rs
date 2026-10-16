//! Portable admitted reference assets; proprietary originals remain external.
use crate::{persistence::*, Project, ProjectView};
use anyhow::{ensure, Context, Result};
use greenlens_core::{architectural_scope::ArchitecturalScope, reference::*};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
use uuid::Uuid;

// At most 124 members for the admitted geometry limit. This permits the owned
// pretty-printed seal with headroom, without a generic 64 MiB digest-map parse.
const MAX_REFERENCE_SEAL_BYTES: u64 = 64 * 1024;

fn verification_checkpoint(cancel: &AtomicBool, started: Instant) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Reference verification cancelled"
    );
    ensure!(
        started.elapsed() < Duration::from_secs(crate::project_open::VERIFICATION_DEADLINE_SECONDS),
        "Reference verification exceeded the 120-second budget"
    );
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceAsset {
    pub id: String,
    pub path: String,
    pub revision_sha256: String,
    pub manifest_sha256: String,
    pub geometry_sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchitecturalProject {
    pub schema_version: String,
    pub assets: Vec<ReferenceAsset>,
    pub active_asset_id: String,
    pub scope: Option<ArchitecturalScope>,
    /// Additive immutable documentation. Old projects retain their original
    /// representation until an explicit accepted revision action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_history: Option<crate::study_revisions::ScopeHistory>,
}
pub fn validate_extension(value: &ArchitecturalProject) -> Result<()> {
    ensure!(
        value.schema_version == "greenlens.architectural-project.v1",
        "Unsupported architectural project extension"
    );
    ensure!(
        !value.assets.is_empty() && value.assets.len() <= 8,
        "Retain one to eight reference revisions per project"
    );
    let mut ids = BTreeSet::new();
    for asset in &value.assets {
        Uuid::parse_str(&asset.id)?;
        ensure!(ids.insert(&asset.id), "Duplicate reference asset identity");
        ensure!(
            asset.path == format!("references/{}", asset.id),
            "Invalid reference asset path"
        );
        for hash in [
            &asset.revision_sha256,
            &asset.manifest_sha256,
            &asset.geometry_sha256,
        ] {
            ensure!(
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "Invalid reference content identity"
            );
        }
    }
    ensure!(
        ids.contains(&value.active_asset_id),
        "Active reference revision is not retained"
    );
    if let Some(scope) = &value.scope {
        greenlens_core::architectural_scope::validate_declaration(scope)?;
    }
    crate::study_revisions::validate_history(value)?;
    Ok(())
}

pub fn read_manifest(root: &Path, asset: &ReferenceAsset) -> Result<ReferenceModel> {
    let started = Instant::now();
    let cancel = AtomicBool::new(false);
    read_manifest_checked(root, asset, &mut |_| {
        verification_checkpoint(&cancel, started)
    })
}
fn read_manifest_checked(
    root: &Path,
    asset: &ReferenceAsset,
    checkpoint: &mut impl FnMut(u64) -> Result<()>,
) -> Result<ReferenceModel> {
    checkpoint(0)?;
    let path = regular_child(root, &format!("{}/reference.json", asset.path))?;
    let bytes = read_evidence_bytes_checked(&path, MAX_MANIFEST_BYTES as u64, checkpoint)?;
    ensure!(
        bytes_hash(&bytes) == asset.manifest_sha256,
        "Reference manifest was altered"
    );
    let manifest: ReferenceModel = serde_json::from_slice(&bytes)?;
    validate_manifest(&manifest)?;
    ensure!(
        manifest.revision_sha256 == asset.revision_sha256
            && manifest.geometry_sha256 == asset.geometry_sha256,
        "Reference revision binding differs"
    );
    checkpoint(0)?;
    Ok(manifest)
}
pub fn read_chunk(
    root: &Path,
    asset: &ReferenceAsset,
    index: usize,
) -> Result<Vec<ReferenceTriangle>> {
    let started = Instant::now();
    let cancel = AtomicBool::new(false);
    let mut checkpoint = |_| verification_checkpoint(&cancel, started);
    let manifest = read_manifest_checked(root, asset, &mut checkpoint)?;
    let entry = manifest
        .chunks
        .get(index)
        .context("Reference chunk index outside manifest")?;
    let bytes = read_evidence_bytes_checked(
        &regular_child(root, &format!("{}/{}", asset.path, entry.path))?,
        entry.byte_length as u64,
        &mut checkpoint,
    )?;
    let triangles = decode_chunk(&manifest, index, &bytes)?;
    checkpoint(0)?;
    Ok(triangles)
}
pub fn verify_asset(
    root: &Path,
    asset: &ReferenceAsset,
    cancel: &AtomicBool,
) -> Result<ReferenceModel> {
    let started = Instant::now();
    let mut checkpoint = |_| verification_checkpoint(cancel, started);
    checkpoint(0)?;
    let manifest = read_manifest_checked(root, asset, &mut checkpoint)?;
    let sealed: Checksums = serde_json::from_slice(&read_evidence_bytes_checked(
        &regular_child(root, &format!("{}/checksums.json", asset.path))?,
        MAX_REFERENCE_SEAL_BYTES,
        &mut checkpoint,
    )?)?;
    let expected: BTreeSet<String> = std::iter::once("reference.json".to_owned())
        .chain(manifest.chunks.iter().map(|c| c.path.clone()))
        .collect();
    ensure!(
        sealed.schema_version == "0.1.0"
            && sealed.sha256.keys().cloned().collect::<BTreeSet<_>>() == expected,
        "Reference seal contains missing or unexpected members"
    );
    // A supplied seal is not authority. Bind it to the separately checked
    // manifest/asset before opening any named geometry member. Actual bytes
    // are then checked against those hashes by the bounded chunk decoder.
    ensure!(
        sealed.sha256.get("reference.json") == Some(&asset.manifest_sha256)
            && manifest
                .chunks
                .iter()
                .all(|entry| sealed.sha256.get(&entry.path) == Some(&entry.sha256)),
        "Reference seal digests disagree with the bound manifest"
    );
    checkpoint(0)?;
    let chunks = manifest
        .chunks
        .iter()
        .map(|entry| {
            checkpoint(0)?;
            read_evidence_bytes_checked(
                &regular_child(root, &format!("{}/{}", asset.path, entry.path))?,
                entry.byte_length as u64,
                &mut checkpoint,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let verified = verify_geometry(&manifest, chunks.iter().map(Vec::as_slice), |_| {
        checkpoint(0).is_ok()
    });
    checkpoint(0)?;
    verified?;
    Ok(manifest)
}
pub fn verify_project(
    root: &Path,
    extension: &ArchitecturalProject,
    cancel: &AtomicBool,
) -> Result<()> {
    validate_extension(extension)?;
    let mut manifests = BTreeMap::new();
    for asset in &extension.assets {
        manifests.insert(asset.id.clone(), verify_asset(root, asset, cancel)?);
    }
    crate::study_revisions::verify_history_manifests(extension, &manifests, cancel)?;
    Ok(())
}
pub fn evidence_members(root: &Path, extension: &ArchitecturalProject) -> Result<Vec<String>> {
    validate_extension(extension)?;
    let mut members = Vec::new();
    let mut budget = PackageBudget::default();
    for asset in &extension.assets {
        let manifest = read_manifest(root, asset)?;
        for name in std::iter::once("reference.json")
            .chain(manifest.chunks.iter().map(|c| c.path.as_str()))
            .chain(std::iter::once("checksums.json"))
        {
            let path = format!("{}/{name}", asset.path);
            budget.add(&path, fs::metadata(regular_child(root, &path)?)?.len())?;
            members.push(path);
        }
    }
    Ok(members)
}
pub fn copy_evidence(
    source: &Path,
    destination: &Path,
    extension: &ArchitecturalProject,
) -> Result<()> {
    verify_project(source, extension, &AtomicBool::new(false))?;
    evidence_members(source, extension)?;
    for asset in &extension.assets {
        let manifest = read_manifest(source, asset)?;
        let members = bound_members(&manifest, asset);
        let started = Instant::now();
        let cancel = AtomicBool::new(false);
        promote_reference_bound(
            &source.join(&asset.path),
            destination,
            &asset.id,
            &members,
            &mut |_| verification_checkpoint(&cancel, started),
        )?;
        // Generic seal integrity alone cannot establish this asset binding.
        // The caller may publish its project manifest only after this returns.
        verify_asset(destination, asset, &cancel)?;
    }
    Ok(())
}

fn bound_members(manifest: &ReferenceModel, asset: &ReferenceAsset) -> BoundReferenceMembers {
    std::iter::once((
        "reference.json".into(),
        (asset.manifest_sha256.clone(), MAX_MANIFEST_BYTES as u64),
    ))
    .chain(manifest.chunks.iter().map(|entry| {
        (
            entry.path.clone(),
            (entry.sha256.clone(), entry.byte_length as u64),
        )
    }))
    .collect()
}

pub fn asset<'a>(project: &'a Project, id: Option<&str>) -> Result<&'a ReferenceAsset> {
    let extension = project
        .architecture
        .as_ref()
        .context("Import a reference model first")?;
    extension
        .assets
        .iter()
        .find(|a| a.id == id.unwrap_or(&extension.active_asset_id))
        .context("Reference revision is not retained by this project")
}

pub fn scope_models(
    project: &Project,
) -> Result<BTreeMap<String, greenlens_core::energy::ValidatedThermalModel>> {
    let energy = project
        .energy
        .as_ref()
        .context("Create a thermal project before mapping analysis scope")?;
    energy
        .designs
        .iter()
        .map(|(id, design)| {
            Ok((
                id.clone(),
                greenlens_core::energy::validate_thermal_model(
                    &design.model,
                    &crate::energy_workbench::calendar_for(project, id)?,
                )?,
            ))
        })
        .collect()
}

pub fn scope_context(root: &Path, project: &Project) -> Result<serde_json::Value> {
    let reference = read_manifest(root, asset(project, None)?)?;
    let models = scope_models(project)?;
    let mut designs = BTreeMap::new();
    for (id, model) in models {
        let scene = greenlens_core::energy::thermal_scene(
            &model.model,
            &crate::energy_workbench::calendar_for(project, &id)?,
        )?;
        let targets: Vec<_> = scene.objects.into_iter().map(|o| serde_json::json!({
            "object_id":o.id, "kind":if o.kind == "shading" {"detached_shading"} else {&o.kind}, "label":o.label
        })).collect();
        designs.insert(
            id,
            serde_json::json!({"model_input_sha256":model.model_input_sha256,"targets":targets}),
        );
    }
    Ok(
        serde_json::json!({"reference_options_sha256":greenlens_core::canonical_hash(&reference.options)?,"designs":designs}),
    )
}

pub fn assess_scope(
    root: &Path,
    project: &Project,
    scope: &ArchitecturalScope,
) -> Result<greenlens_core::architectural_scope::ScopeAssessment> {
    Ok(greenlens_core::architectural_scope::assess_scope(
        scope,
        &read_manifest(root, asset(project, None)?)?,
        &scope_models(project)?,
    )?)
}

/// Changes only the derived scope. Historical restoration is assessed, never
/// silently rebound to a new reference or thermal revision.
pub fn set_scope(
    root: &Path,
    project: &Project,
    scope: Option<ArchitecturalScope>,
    restoring: bool,
) -> Result<Project> {
    let cancel = AtomicBool::new(false);
    if restoring {
        crate::study_revisions::restore_scope(root, project, scope, &cancel)
    } else if let Some(scope) = scope {
        crate::study_revisions::initialize_scope(root, project, scope, &cancel)
    } else {
        crate::study_revisions::restore_scope(root, project, None, &cancel)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceVertex {
    pub chunk: usize,
    pub triangle: usize,
    pub corner: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceMeasurement {
    pub distance_m: f64,
    pub delta_m: [f64; 3],
    pub points_m: [[f64; 3]; 2],
    pub revision_sha256: String,
}
pub fn measure(
    root: &Path,
    asset: &ReferenceAsset,
    a: ReferenceVertex,
    b: ReferenceVertex,
) -> Result<ReferenceMeasurement> {
    let vertex = |p: ReferenceVertex| -> Result<[f64; 3]> {
        let triangles = read_chunk(root, asset, p.chunk)?;
        Ok(*triangles
            .get(p.triangle)
            .context("Triangle index outside admitted chunk")?
            .vertices_m
            .get(p.corner)
            .context("Corner must be 0, 1 or 2")?)
    };
    let points_m = [vertex(a)?, vertex(b)?];
    let delta_m = std::array::from_fn(|i| points_m[1][i] - points_m[0][i]);
    let distance_m = delta_m[0].hypot(delta_m[1]).hypot(delta_m[2]);
    ensure!(distance_m.is_finite(), "Measurement exceeds finite range");
    Ok(ReferenceMeasurement {
        distance_m,
        delta_m,
        points_m,
        revision_sha256: asset.revision_sha256.clone(),
    })
}

/// Import publishes only after complete admission, candidate evidence verification
/// and revision/conflict checks. A failed import may leave an unadvertised immutable
/// asset, but never replaces the manifest or modifies existing evidence.
pub fn import_project(
    root: &Path,
    project: &Project,
    expected_revision: Option<&str>,
    file: &Path,
    options: &ReferenceImportOptions,
    cancel: &AtomicBool,
    mut progress: impl FnMut(&ReferenceProgress) -> bool,
) -> Result<ProjectView> {
    crate::validate_project(project)?;
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Reference import cancelled"
    );
    ensure!(
        fs::metadata(file)?.len() <= MAX_SOURCE_BYTES as u64,
        "GLB exceeds the128MiB source limit; export a declared supported subset locally"
    );
    let mut input = fs::File::open(file)?;
    let mut bytes = Vec::new();
    let mut block = [0u8; 65536];
    loop {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "Reference import cancelled"
        );
        let count = input.read(&mut block)?;
        if count == 0 {
            break;
        }
        ensure!(
            bytes.len() + count <= MAX_SOURCE_BYTES,
            "GLB exceeds the 128 MiB limit"
        );
        bytes.extend_from_slice(&block[..count]);
    }
    let admitted = admit_glb(&bytes, options, |event| {
        !cancel.load(Ordering::Relaxed) && progress(event)
    })?;
    let id = Uuid::new_v4().to_string();
    let stage = crate::local_root()?.join("reference-intake").join(&id);
    fs::create_dir_all(stage.join("geometry"))?;
    write_json(&stage.join("reference.json"), &admitted.manifest)?;
    for (entry, bytes) in admitted.manifest.chunks.iter().zip(&admitted.chunks) {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "Reference import cancelled"
        );
        let mut out = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(stage.join(relative_path(&entry.path)?))?;
        out.write_all(bytes)?;
        out.sync_all()?;
    }
    let names: Vec<&str> = std::iter::once("reference.json")
        .chain(admitted.manifest.chunks.iter().map(|c| c.path.as_str()))
        .collect();
    seal(&stage, &names)?;
    let asset = ReferenceAsset {
        id: id.clone(),
        path: format!("references/{id}"),
        revision_sha256: admitted.manifest.revision_sha256.clone(),
        geometry_sha256: admitted.manifest.geometry_sha256.clone(),
        manifest_sha256: evidence_file_hash(&stage.join("reference.json"))?,
    };
    let members = bound_members(&admitted.manifest, &asset);
    let mut candidate = project.clone();
    let ext = candidate
        .architecture
        .get_or_insert_with(|| ArchitecturalProject {
            schema_version: "greenlens.architectural-project.v1".into(),
            assets: vec![],
            active_asset_id: id.clone(),
            scope: None,
            scope_history: None,
        });
    ext.assets.push(asset);
    ext.active_asset_id = id.clone();
    // Existing scope stays revision-bound and becomes visibly historical.
    candidate.schema_version = "0.6.0".into();
    crate::validate_project(&candidate)?;
    let _lock = WriterLock::acquire(root)?;
    check_revision(root, expected_revision)?;
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Reference import cancelled"
    );
    let copy_started = Instant::now();
    let mut last_progress = Instant::now();
    let mut copy_bytes = 0usize;
    ensure!(
        progress(&ReferenceProgress {
            stage: "copying admitted reference".into(),
            completed: 0,
            total: 1,
            processed_bytes: 0
        }),
        "Reference import cancelled"
    );
    promote_reference_bound(&stage, root, &id, &members, &mut |bytes| {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "Reference import cancelled; the previous document is unchanged"
        );
        ensure!(
            copy_started.elapsed()
                < Duration::from_secs(crate::project_open::VERIFICATION_DEADLINE_SECONDS),
            "Reference copy exceeded the 120-second budget; the previous document is unchanged"
        );
        copy_bytes = copy_bytes
            .checked_add(bytes as usize)
            .context("Reference copy byte counter overflow")?;
        if last_progress.elapsed() >= Duration::from_millis(100) {
            ensure!(
                progress(&ReferenceProgress {
                    stage: "copying admitted reference".into(),
                    completed: 0,
                    total: 1,
                    processed_bytes: copy_bytes
                }),
                "Reference import cancelled"
            );
            last_progress = Instant::now();
        }
        Ok(())
    })?;
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Reference import cancelled; the previous document is unchanged"
    );
    evidence_members(root, candidate.architecture.as_ref().unwrap())?;
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Reference import cancelled; the previous document is unchanged"
    );
    // The asset can now be read at its final immutable location, but remains
    // unadvertised. A failure leaves the old manifest and all its evidence intact.
    let mut view =
        verify_import_candidate(root, &candidate, expected_revision, cancel, &mut progress)?;
    check_revision(root, expected_revision)?;
    ensure!(
        progress(&ReferenceProgress {
            stage: "ready to publish verified reference".into(),
            completed: 1,
            total: 1,
            processed_bytes: copy_bytes
        }),
        "Reference import cancelled"
    );
    // Final cancellation boundary: once the atomic replacement succeeds this
    // operation is committed. Do not return a later cancellation/read error that
    // would falsely tell the UI its previous document was retained.
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Reference import cancelled; the previous document is unchanged"
    );
    check_revision(root, expected_revision)?;
    view.revision = atomic_project(root, &candidate)?;
    Ok(view)
}

/// Keep the public borrowed cancellation/callback API while the ordinary reader
/// owns its thread-safe cancellation handle. This bounded coordinator forwards
/// actual reader stages and checks caller cancellation at most 50 ms apart.
fn verify_import_candidate(
    root: &Path,
    candidate: &Project,
    expected_revision: Option<&str>,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(&ReferenceProgress) -> bool,
) -> Result<ProjectView> {
    std::thread::scope(|scope| {
        let cancellation = Arc::new(AtomicBool::new(cancel.load(Ordering::Relaxed)));
        let worker_cancel = cancellation.clone();
        let (send, receive) = mpsc::channel();
        let worker = scope.spawn(move || {
            crate::project_open::verify_import_candidate(
                root,
                candidate,
                expected_revision,
                worker_cancel,
                move |event| {
                    let _ = send.send(event);
                },
            )
        });
        loop {
            if cancel.load(Ordering::Relaxed) {
                cancellation.store(true, Ordering::Relaxed);
            }
            match receive.recv_timeout(Duration::from_millis(50)) {
                Ok(event) => {
                    if !cancellation.load(Ordering::Relaxed)
                        && !progress(&ReferenceProgress {
                            stage: format!(
                                "verifying candidate: {}{}",
                                event.stage,
                                event
                                    .run_id
                                    .as_ref()
                                    .map(|id| format!(" ({id})"))
                                    .unwrap_or_default()
                            ),
                            completed: event.runs_completed,
                            total: event.runs_total,
                            processed_bytes: event.bytes_read.try_into().unwrap_or(usize::MAX),
                        })
                    {
                        cancellation.store(true, Ordering::Relaxed);
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => (),
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        let result = worker.join().map_err(|_| {
            anyhow::anyhow!(
                "Reference candidate verification failed; the previous document is unchanged"
            )
        })?;
        ensure!(
            !cancel.load(Ordering::Relaxed) && !cancellation.load(Ordering::Relaxed),
            "Reference import cancelled; the previous document is unchanged"
        );
        result
    })
}

#[cfg(test)]
mod verification_budget_tests {
    use super::*;

    #[test]
    fn standalone_verification_enforces_its_deadline_without_an_open_snapshot() {
        let started = Instant::now()
            - Duration::from_secs(crate::project_open::VERIFICATION_DEADLINE_SECONDS + 1);
        let error = verification_checkpoint(&AtomicBool::new(false), started).unwrap_err();
        assert!(error.to_string().contains("120-second budget"));
    }
}
