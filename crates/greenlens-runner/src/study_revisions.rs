//! Append-only scope documentation in the portable manifest. Accepted actions
//! return complete candidates; the document service retains its ordinary one-
//! writer/conflict/cancellation gate before publishing. No run bytes are edited.
use crate::{reference, Project};
use anyhow::{ensure, Context, Result};
use greenlens_core::{
    architectural_scope::ArchitecturalScope,
    energy::validate_thermal_model,
    reference::ReferenceModel,
    study_revision::{
        self as core, CarryForwardPlan, ReviewDecision, RevisionDesignSnapshot,
        RevisionReferenceSnapshot, ScopeRevision, MAX_SCOPE_REVISIONS,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub const SCOPE_HISTORY_SCHEMA: &str = "greenlens.scope-history.v1";
/// v0.7 documentation is deliberately bounded below the shared 64 MiB entry
/// limit. Older readers retain their unchanged historical file admission limit.
pub const MAX_REVISION_PROJECT_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScopeHistory {
    pub schema_version: String,
    pub revisions: Vec<ScopeRevision>,
    pub active_revision_sha256: Option<String>,
}

fn checkpoint(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Scope revision operation cancelled; the accepted document is unchanged"
    );
    crate::project_open::checkpoint(0)?;
    Ok(())
}

fn bounded_manifest(value: &impl Serialize) -> Result<()> {
    struct Counter(usize);
    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .ok_or_else(|| std::io::Error::other("Manifest size overflow"))?;
            if self.0 > MAX_REVISION_PROJECT_BYTES {
                return Err(std::io::Error::other("Revision document capacity exceeded"));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    // Reserve the ordinary trailing newline written by atomic_json.
    serde_json::to_writer_pretty(Counter(1), value)
        .context("Study revision history exceeds the 16 MiB v0.7 manifest capacity. Preserve this project; create a separately scoped project instead. No history was pruned")
}

pub fn validate_project_capacity(project: &Project) -> Result<()> {
    if project
        .architecture
        .as_ref()
        .is_some_and(|a| a.scope_history.is_some())
    {
        bounded_manifest(project)?;
    }
    Ok(())
}

/// Ordinary saves preserve accepted documentation even when a client submits
/// an earlier undo snapshot. Only the active selection may move backwards.
pub fn validate_preserved(previous: &Project, candidate: &Project) -> Result<()> {
    ensure!(
        previous.id == candidate.id,
        "Scope history belongs to another document"
    );
    for old in revisions(previous) {
        ensure!(revisions(candidate).iter().any(|new| new == old),
            "Accepted scope revision {} cannot be removed or changed by this save; preserve history and change only its active selection", old.revision_sha256);
    }
    if let Some(old_scope) = previous
        .architecture
        .as_ref()
        .filter(|a| a.scope_history.is_none())
        .and_then(|a| a.scope.as_ref())
    {
        ensure!(candidate.architecture.as_ref().and_then(|a| a.scope.as_ref()) == Some(old_scope)
            || revisions(candidate).iter().any(|r| &r.scope == old_scope),
            "Adopt the exact legacy scope before replacing it; earlier documentation cannot be silently discarded");
    }
    Ok(())
}

pub fn validate_history(extension: &reference::ArchitecturalProject) -> Result<()> {
    let Some(history) = &extension.scope_history else {
        return Ok(());
    };
    bounded_manifest(history)?;
    ensure!(
        history.schema_version == SCOPE_HISTORY_SCHEMA,
        "Unsupported scope history schema"
    );
    ensure!(
        !history.revisions.is_empty() && history.revisions.len() <= MAX_SCOPE_REVISIONS,
        "Retain one to 64 immutable scope revisions; history cannot be silently pruned"
    );
    let mut accepted: BTreeMap<String, &ScopeRevision> = BTreeMap::new();
    for revision in &history.revisions {
        // The open operation's actual-byte verification context also owns
        // follow-on history cancellation and its finite elapsed-time budget.
        crate::project_open::checkpoint(0)?;
        ensure!(
            !accepted.contains_key(&revision.revision_sha256),
            "Duplicate scope revision identity"
        );
        core::validate_revision(revision)?;
        let asset = extension.assets.iter().find(|asset| asset.id == revision.reference.asset_id)
            .context("Scope revision requires its retained reference asset; asset removal is not allowed")?;
        ensure!(
            asset.manifest_sha256 == revision.reference.manifest_sha256
                && asset.revision_sha256 == revision.reference.manifest.revision_sha256
                && asset.geometry_sha256 == revision.reference.manifest.geometry_sha256,
            "Scope revision source binding differs from the retained reference asset"
        );
        if let Some(parent) = &revision.parent_revision_sha256 {
            let previous = accepted
                .get(parent)
                .context("Scope parent must be retained earlier in immutable history")?;
            core::validate_revision_parent(previous, revision)?;
        }
        accepted.insert(revision.revision_sha256.clone(), revision);
    }
    crate::project_open::checkpoint(0)?;
    match &history.active_revision_sha256 {
        Some(id) => {
            let revision = accepted
                .get(id)
                .context("Active scope revision is not retained")?;
            ensure!(
                extension.scope.as_ref() == Some(&revision.scope),
                "Current scope projection differs from its immutable revision"
            );
        }
        None => ensure!(
            extension.scope.is_none(),
            "A cleared active scope cannot retain a different current projection"
        ),
    }
    Ok(())
}

/// The caller has already verified every reference's actual chunks and manifest
/// bytes once. This additionally binds inline immutable snapshots to those
/// exact admitted manifests without a second geometry traversal.
pub fn verify_history_manifests(
    extension: &reference::ArchitecturalProject,
    manifests: &BTreeMap<String, ReferenceModel>,
    cancel: &AtomicBool,
) -> Result<()> {
    if let Some(history) = &extension.scope_history {
        for revision in &history.revisions {
            checkpoint(cancel)?;
            ensure!(
                manifests.get(&revision.reference.asset_id) == Some(&revision.reference.manifest),
                "Scope revision snapshot differs from actual retained reference bytes"
            );
        }
    }
    checkpoint(cancel)
}

pub fn revisions(project: &Project) -> &[ScopeRevision] {
    project
        .architecture
        .as_ref()
        .and_then(|a| a.scope_history.as_ref())
        .map(|h| h.revisions.as_slice())
        .unwrap_or(&[])
}

pub fn selected<'a>(project: &'a Project, revision_sha256: &str) -> Result<&'a ScopeRevision> {
    revisions(project)
        .iter()
        .find(|revision| revision.revision_sha256 == revision_sha256)
        .context("Selected scope revision is not retained in this document")
}

pub fn current_context(
    root: &Path,
    project: &Project,
    cancel: &AtomicBool,
) -> Result<(
    RevisionReferenceSnapshot,
    BTreeMap<String, RevisionDesignSnapshot>,
)> {
    checkpoint(cancel)?;
    let asset = reference::asset(project, None)?;
    let manifest = reference::read_manifest(root, asset)?;
    checkpoint(cancel)?;
    let designs = project
        .energy
        .as_ref()
        .context("Create accepted thermal inputs before accepting a scope")?
        .designs
        .iter()
        .map(|(id, design)| {
            checkpoint(cancel)?;
            Ok((
                id.clone(),
                RevisionDesignSnapshot {
                    model: design.model.clone(),
                    calendar: crate::energy_workbench::calendar_for(project, id)?,
                    study: design.study.clone(),
                },
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let reference = RevisionReferenceSnapshot {
        asset_id: asset.id.clone(),
        manifest_sha256: asset.manifest_sha256.clone(),
        manifest,
    };
    core::validate_revision_context(&reference, &designs)?;
    checkpoint(cancel)?;
    Ok((reference, designs))
}

/// Adopt a v1 scope only during an explicit accepted action. Historical model
/// snapshots come from matching verified evidence when current inputs differ;
/// names, approximate geometry and fresh current models are not substitutes.
fn legacy_revision(
    root: &Path,
    project: &Project,
    scope: ArchitecturalScope,
    cancel: &AtomicBool,
) -> Result<ScopeRevision> {
    let architecture = project
        .architecture
        .as_ref()
        .context("Reference extension missing")?;
    let asset = architecture.assets.iter().find(|a| a.revision_sha256 == scope.reference_revision_sha256
        && a.geometry_sha256 == scope.reference_geometry_sha256).context("Legacy scope reference revision is not retained; restore its exact assets before creating revision history")?;
    checkpoint(cancel)?;
    let reference = RevisionReferenceSnapshot {
        asset_id: asset.id.clone(),
        manifest_sha256: asset.manifest_sha256.clone(),
        manifest: reference::read_manifest(root, asset)?,
    };
    let energy = project
        .energy
        .as_ref()
        .context("Thermal design snapshots unavailable")?;
    let mut designs = energy
        .designs
        .iter()
        .map(|(id, design)| {
            Ok((
                id.clone(),
                RevisionDesignSnapshot {
                    model: design.model.clone(),
                    calendar: crate::energy_workbench::calendar_for(project, id)?,
                    study: design.study.clone(),
                },
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    for (id, required) in &scope.design_model_sha256 {
        checkpoint(cancel)?;
        let current = energy
            .designs
            .get(id)
            .context("Legacy scope design is no longer available")?;
        let calendar = crate::energy_workbench::calendar_for(project, id)?;
        let valid = validate_thermal_model(&current.model, &calendar)?;
        let snapshot = if &valid.model_input_sha256 == required {
            RevisionDesignSnapshot {
                model: current.model.clone(),
                calendar,
                study: current.study.clone(),
            }
        } else {
            let mut retained = None;
            for summary in energy.runs.iter().rev().filter(|r| &r.alternative_id == id) {
                checkpoint(cancel)?;
                let run = crate::energy_services::selected_run(root, project, &summary.id)?;
                if &run.validated.model_input_sha256 == required {
                    retained = Some(RevisionDesignSnapshot {
                        model: run.model,
                        calendar: run.study.calendar.clone(),
                        study: Some(run.study),
                    });
                    break;
                }
            }
            retained.context("Exact inputs for the legacy scope are unavailable. Preserve this historical document and restore its original model or matching retained run; current inputs cannot replace it")?
        };
        designs.insert(id.clone(), snapshot);
    }
    checkpoint(cancel)?;
    Ok(core::initial_revision(scope, reference, designs)?)
}

fn append(project: &Project, revision: ScopeRevision) -> Result<Project> {
    let mut next = project.clone();
    let extension = next
        .architecture
        .as_mut()
        .context("Import a reference first")?;
    let history = extension.scope_history.get_or_insert_with(|| ScopeHistory {
        schema_version: SCOPE_HISTORY_SCHEMA.into(),
        revisions: vec![],
        active_revision_sha256: None,
    });
    if let Some(existing) = history
        .revisions
        .iter()
        .find(|r| r.revision_sha256 == revision.revision_sha256)
    {
        ensure!(
            existing == &revision,
            "Immutable scope content conflicts with a retained identity"
        );
    } else {
        ensure!(
            history.revisions.len() < MAX_SCOPE_REVISIONS,
            "Scope revision capacity is 64; existing history was retained"
        );
        history.revisions.push(revision.clone());
    }
    history.active_revision_sha256 = Some(revision.revision_sha256);
    extension.scope = Some(revision.scope);
    validate_project_capacity(&next)?;
    crate::validate_project(&next)?;
    Ok(next)
}

pub fn initialize_scope(
    root: &Path,
    project: &Project,
    mut scope: ArchitecturalScope,
    cancel: &AtomicBool,
) -> Result<Project> {
    checkpoint(cancel)?;
    crate::validate_project(project)?;
    let mut next = project.clone();
    let extension = next
        .architecture
        .as_ref()
        .context("Import a reference first")?;
    if extension.scope_history.is_none() {
        if let Some(legacy) = extension.scope.clone() {
            let revision = legacy_revision(root, &next, legacy, cancel)?;
            next = append(&next, revision)?;
            if next.architecture.as_ref().and_then(|a| a.scope.as_ref()) == Some(&scope) {
                checkpoint(cancel)?;
                return Ok(next);
            }
        }
    }
    let (reference, designs) = current_context(root, &next, cancel)?;
    let active = next
        .architecture
        .as_ref()
        .and_then(|a| a.scope_history.as_ref())
        .and_then(|h| h.active_revision_sha256.clone());
    let revision = if let Some(id) = active {
        let previous = selected(&next, &id)?;
        // The scope editor remains useful for same-context documentation edits.
        // A scientific/source change requires the explicit carry-forward review.
        if previous.scope == scope && previous.reference == reference && previous.designs == designs
        {
            checkpoint(cancel)?;
            return Ok(next);
        }
        scope.schema_version =
            greenlens_core::architectural_scope::ARCHITECTURAL_SCOPE_SCHEMA_V2.into();
        let plan = core::prepare_carry_forward_v2(previous, &reference, &designs, Some(&scope))?;
        ensure!(!plan.rows.iter().any(|row| row.requires_review), "Scope documentation, source or analytical inputs changed. Use Review changes and carry forward to review the editable candidate before accepting a new scope revision");
        core::accept_carry_forward_v2(
            previous,
            reference,
            designs,
            &plan.plan_sha256,
            scope,
            vec![],
        )?
    } else {
        core::initial_revision_v2(scope, reference, designs)?
    };
    checkpoint(cancel)?;
    let candidate = append(&next, revision)?;
    checkpoint(cancel)?;
    Ok(candidate)
}

pub fn prepare(
    root: &Path,
    project: &Project,
    parent_revision_sha256: &str,
    cancel: &AtomicBool,
) -> Result<CarryForwardPlan> {
    checkpoint(cancel)?;
    crate::validate_project(project)?;
    let previous = selected(project, parent_revision_sha256)?;
    let (reference, designs) = current_context(root, project, cancel)?;
    let plan = core::prepare_carry_forward_v2(previous, &reference, &designs, None)?;
    checkpoint(cancel)?;
    Ok(plan)
}

/// Recompute an edited candidate against the same immutable parent and current
/// accepted context. This only returns a preview; it never mutates a project.
pub fn prepare_candidate(
    root: &Path,
    project: &Project,
    parent_revision_sha256: &str,
    scope: &ArchitecturalScope,
    cancel: &AtomicBool,
) -> Result<CarryForwardPlan> {
    checkpoint(cancel)?;
    crate::validate_project(project)?;
    let previous = selected(project, parent_revision_sha256)?;
    let (reference, designs) = current_context(root, project, cancel)?;
    let plan = core::prepare_carry_forward_v2(previous, &reference, &designs, Some(scope))?;
    checkpoint(cancel)?;
    Ok(plan)
}

#[allow(clippy::too_many_arguments)] // Exact parent, preview and accepted candidate are independently bound.
pub fn accept(
    root: &Path,
    project: &Project,
    parent_revision_sha256: &str,
    expected_plan_sha256: &str,
    scope: ArchitecturalScope,
    decisions: Vec<ReviewDecision>,
    cancel: &AtomicBool,
) -> Result<Project> {
    checkpoint(cancel)?;
    crate::validate_project(project)?;
    let previous = selected(project, parent_revision_sha256)?;
    let (reference, designs) = current_context(root, project, cancel)?;
    let accept = if scope.schema_version
        == greenlens_core::architectural_scope::ARCHITECTURAL_SCOPE_SCHEMA_V2
    {
        core::accept_carry_forward_v2
    } else {
        core::accept_carry_forward
    };
    let revision = accept(
        previous,
        reference,
        designs,
        expected_plan_sha256,
        scope,
        decisions,
    )?;
    checkpoint(cancel)?;
    let candidate = append(project, revision)?;
    checkpoint(cancel)?;
    Ok(candidate)
}

/// Selecting or clearing the current revision never removes immutable history.
pub fn activate(project: &Project, revision_sha256: Option<&str>) -> Result<Project> {
    crate::validate_project(project)?;
    let scope = revision_sha256
        .map(|id| selected(project, id).map(|r| r.scope.clone()))
        .transpose()?;
    let mut next = project.clone();
    let extension = next
        .architecture
        .as_mut()
        .context("Import a reference first")?;
    if let Some(history) = &mut extension.scope_history {
        history.active_revision_sha256 = revision_sha256.map(str::to_owned);
    } else {
        ensure!(
            revision_sha256.is_none(),
            "No scope revision history is retained"
        );
        ensure!(extension.scope.is_none(), "Adopt the legacy scope before clearing it; historical documentation must remain retained");
    }
    extension.scope = scope;
    crate::validate_project(&next)?;
    Ok(next)
}

/// Legacy document-history entries contain the exact scope value. Resolve that
/// value to an already retained revision; never append another record on undo.
pub fn restore_scope(
    root: &Path,
    project: &Project,
    scope: Option<ArchitecturalScope>,
    cancel: &AtomicBool,
) -> Result<Project> {
    checkpoint(cancel)?;
    if let Some(scope) = scope {
        if let Some(revision) = revisions(project).iter().rev().find(|r| r.scope == scope) {
            return activate(project, Some(&revision.revision_sha256));
        }
        ensure!(
            project
                .architecture
                .as_ref()
                .is_none_or(|a| a.scope_history.is_none()),
            "Undo target is not present in immutable scope history; no replacement was accepted"
        );
        // A pre-v0.7 history entry can be adopted explicitly without requiring
        // it to match current geometry, provided its actual snapshots remain.
        let mut candidate = project.clone();
        if let Some(legacy) = candidate
            .architecture
            .as_ref()
            .and_then(|a| a.scope.clone())
        {
            if legacy != scope {
                let retained = legacy_revision(root, &candidate, legacy, cancel)?;
                candidate = append(&candidate, retained)?;
            }
        }
        let revision = legacy_revision(root, &candidate, scope, cancel)?;
        checkpoint(cancel)?;
        append(&candidate, revision)
    } else {
        if let Some(legacy) = project
            .architecture
            .as_ref()
            .filter(|a| a.scope_history.is_none())
            .and_then(|a| a.scope.clone())
        {
            let revision = legacy_revision(root, project, legacy, cancel)?;
            checkpoint(cancel)?;
            return activate(&append(project, revision)?, None);
        }
        activate(project, None)
    }
}

pub fn history_dependency_ids(extension: &reference::ArchitecturalProject) -> BTreeSet<String> {
    extension
        .scope_history
        .as_ref()
        .into_iter()
        .flat_map(|h| &h.revisions)
        .map(|r| r.reference.asset_id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use greenlens_core::{
        architectural_scope::{
            AnalyticalObjectKind, AnalyticalTarget, MappingStatus, ScopeCoverage, ScopeKind,
            ScopeMapping, ARCHITECTURAL_SCOPE_SCHEMA,
        },
        canonical_hash,
        reference::ReferenceImportOptions,
        study_revision::ReviewResolution,
    };
    use serde_json::json;
    use std::{fs, path::PathBuf};

    // Original synthetic geometry; this fixture verifies document operations,
    // not engine execution, architectural fidelity or physical qualification.
    fn source_glb() -> Vec<u8> {
        let mut binary = Vec::new();
        for point in [[0f32, 0., 0.], [1., 0., 0.], [0., 1., 0.]] {
            for coordinate in point {
                binary.extend(coordinate.to_le_bytes());
            }
        }
        let document = json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],
            "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],
            "nodes":[{"name":"Original synthetic occurrence","mesh":0}],"scenes":[{"nodes":[0]}],"scene":0});
        let mut metadata = serde_json::to_vec(&document).unwrap();
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

    fn fixture() -> (
        tempfile::TempDir,
        PathBuf,
        crate::ProjectView,
        ArchitecturalScope,
    ) {
        let parent = crate::local_root()
            .unwrap()
            .join("verification/scope-revision-tests");
        fs::create_dir_all(&parent).unwrap();
        let temp = tempfile::Builder::new()
            .prefix("original synthetic ")
            .tempdir_in(parent)
            .unwrap();
        let root = temp.path().join("project");
        let source = temp.path().join("original.glb");
        fs::write(&source, source_glb()).unwrap();
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
        let (reference, designs) =
            current_context(&root, &imported.project, &AtomicBool::new(false)).unwrap();
        let valid = core::validate_revision_context(&reference, &designs).unwrap();
        let scope = ArchitecturalScope {
            schema_version: ARCHITECTURAL_SCOPE_SCHEMA.into(),
            title: "S1 original synthetic study".into(),
            purpose: "Scope history and portable document regression; no physical validation claim"
                .into(),
            scope_kind: ScopeKind::RepresentativeStudy,
            reference_revision_sha256: reference.manifest.revision_sha256.clone(),
            reference_geometry_sha256: reference.manifest.geometry_sha256.clone(),
            reference_options_sha256: canonical_hash(&reference.manifest.options).unwrap(),
            design_model_sha256: valid
                .iter()
                .map(|(id, model)| (id.clone(), model.model_input_sha256.clone()))
                .collect(),
            mappings: vec![ScopeMapping {
                occurrence_id: reference.manifest.occurrences[0].id.clone(),
                status: MappingStatus::Mapped,
                targets: vec![AnalyticalTarget {
                    design_id: "B".into(),
                    object_id: designs["B"].model.zones[0].id.clone(),
                    kind: AnalyticalObjectKind::Zone,
                    model_input_sha256: valid["B"].model_input_sha256.clone(),
                }],
                rationale:
                    "Explicit original synthetic mapping; not asserted architectural equivalence"
                        .into(),
            }],
            included_scope: vec!["Supported synthetic zone and declared thermal inputs".into()],
            excluded_scope: vec!["Whole-building physical claims".into()],
            boundary_consequences: "Synthetic boundary conditions remain explicit in the model"
                .into(),
            assumptions: vec![],
            reconciliations: vec![],
            coverage: ScopeCoverage::Unknown {
                reason: "No measured architectural denominator".into(),
            },
            whole_building_review: None,
            report_views: vec![],
        };
        (temp, root, imported, scope)
    }

    fn decisions(plan: &CarryForwardPlan) -> Vec<ReviewDecision> {
        plan.rows
            .iter()
            .filter(|row| row.requires_review)
            .map(|row| ReviewDecision {
                row_id: row.id.clone(),
                resolution: ReviewResolution::Reviewed,
                rationale:
                    "Explicit synthetic regression review; values are not physically qualified"
                        .into(),
            })
            .collect()
    }

    #[test]
    fn accepted_cop_revision_keeps_history_and_survives_cold_portable_copy() {
        let (temp, root, imported, scope) = fixture();
        let original_manifest = fs::read(root.join("project.json")).unwrap();
        let initial =
            initialize_scope(&root, &imported.project, scope, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            fs::read(root.join("project.json")).unwrap(),
            original_manifest
        );
        let first = revisions(&initial)[0].clone();
        let first_bytes = serde_json::to_vec(&first).unwrap();
        let mut changed = initial.clone();
        changed
            .energy
            .as_mut()
            .unwrap()
            .designs
            .get_mut("B")
            .unwrap()
            .model
            .zones[0]
            .cooling_system
            .as_mut()
            .unwrap()
            .gross_rated_cop += 0.25;
        let plan = prepare(
            &root,
            &changed,
            &first.revision_sha256,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(
            plan.proposed_scope.mappings[0].status,
            MappingStatus::Mapped
        );
        assert!(initialize_scope(
            &root,
            &changed,
            plan.proposed_scope.clone(),
            &AtomicBool::new(false)
        )
        .is_err());
        assert!(accept(
            &root,
            &changed,
            &first.revision_sha256,
            &plan.plan_sha256,
            plan.proposed_scope.clone(),
            vec![],
            &AtomicBool::new(false)
        )
        .is_err());
        let accepted = accept(
            &root,
            &changed,
            &first.revision_sha256,
            &plan.plan_sha256,
            plan.proposed_scope.clone(),
            decisions(&plan),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(revisions(&accepted).len(), 2);
        assert_eq!(
            serde_json::to_vec(&revisions(&accepted)[0]).unwrap(),
            first_bytes
        );
        let historical = activate(&accepted, Some(&first.revision_sha256)).unwrap();
        assert_eq!(revisions(&historical).len(), 2);
        assert_eq!(
            historical.architecture.as_ref().unwrap().scope.as_ref(),
            Some(&first.scope)
        );
        assert_eq!(
            canonical_hash(&historical.energy).unwrap(),
            canonical_hash(&accepted.energy).unwrap()
        );
        let saved = crate::save_project(&root, &accepted, Some(&imported.revision)).unwrap();
        let copied = temp.path().join("portable copy");
        crate::save_project_as(Some(&root), &copied, &saved.project, Some(&saved.revision))
            .unwrap();
        fs::rename(&root, temp.path().join("unavailable original project")).unwrap();
        fs::remove_file(temp.path().join("original.glb")).unwrap();
        let reopened = crate::open_project(&copied).unwrap();
        assert_eq!(revisions(&reopened.project), revisions(&accepted));
        assert_eq!(
            serde_json::to_vec(selected(&reopened.project, &first.revision_sha256).unwrap())
                .unwrap(),
            first_bytes
        );
        assert_eq!(
            reference::evidence_members(&copied, reopened.project.architecture.as_ref().unwrap())
                .unwrap()
                .len(),
            3
        );
    }

    #[test]
    fn legacy_read_is_unchanged_and_explicit_clear_preserves_old_documentation() {
        let (_temp, root, imported, scope) = fixture();
        let mut legacy = imported.project.clone();
        legacy.architecture.as_mut().unwrap().scope = Some(scope.clone());
        let saved = crate::save_project(&root, &legacy, Some(&imported.revision)).unwrap();
        let original = fs::read(root.join("project.json")).unwrap();
        assert!(!String::from_utf8_lossy(&original).contains("scope_history"));
        let reopened = crate::open_project(&root).unwrap();
        assert!(revisions(&reopened.project).is_empty());
        assert_eq!(fs::read(root.join("project.json")).unwrap(), original);
        assert!(activate(&reopened.project, None).is_err());
        let cleared =
            restore_scope(&root, &reopened.project, None, &AtomicBool::new(false)).unwrap();
        assert_eq!(revisions(&cleared).len(), 1);
        assert_eq!(revisions(&cleared)[0].scope, scope);
        assert!(cleared.architecture.as_ref().unwrap().scope.is_none());
        let restored =
            restore_scope(&root, &cleared, Some(scope), &AtomicBool::new(false)).unwrap();
        assert_eq!(revisions(&restored).len(), 1);
        assert_eq!(fs::read(root.join("project.json")).unwrap(), original);
        assert_eq!(saved.revision, reopened.revision);
    }

    #[test]
    fn rejected_or_cancelled_acceptance_never_changes_document_or_history() {
        let (_temp, root, imported, scope) = fixture();
        let accepted =
            initialize_scope(&root, &imported.project, scope, &AtomicBool::new(false)).unwrap();
        let before = serde_json::to_vec(&accepted).unwrap();
        let disk_before = fs::read(root.join("project.json")).unwrap();
        let first = &revisions(&accepted)[0];
        assert!(prepare(
            &root,
            &accepted,
            &first.revision_sha256,
            &AtomicBool::new(true)
        )
        .is_err());
        let plan = prepare(
            &root,
            &accepted,
            &first.revision_sha256,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(accept(
            &root,
            &accepted,
            &first.revision_sha256,
            &plan.plan_sha256,
            plan.proposed_scope.clone(),
            vec![],
            &AtomicBool::new(true)
        )
        .is_err());
        assert!(accept(
            &root,
            &accepted,
            &first.revision_sha256,
            &"a".repeat(64),
            plan.proposed_scope,
            vec![],
            &AtomicBool::new(false)
        )
        .is_err());
        assert_eq!(serde_json::to_vec(&accepted).unwrap(), before);
        assert_eq!(fs::read(root.join("project.json")).unwrap(), disk_before);
    }

    #[test]
    fn history_rejects_dangling_references_resealed_snapshots_and_capacity() {
        let (_temp, root, imported, scope) = fixture();
        let accepted =
            initialize_scope(&root, &imported.project, scope, &AtomicBool::new(false)).unwrap();
        let mut dangling = accepted.clone();
        dangling
            .architecture
            .as_mut()
            .unwrap()
            .scope_history
            .as_mut()
            .unwrap()
            .revisions[0]
            .reference
            .asset_id = uuid::Uuid::new_v4().to_string();
        assert!(crate::validate_project(&dangling).is_err());
        let extension = accepted.architecture.as_ref().unwrap();
        let mut actual = BTreeMap::new();
        let asset = &extension.assets[0];
        actual.insert(
            asset.id.clone(),
            reference::read_manifest(&root, asset).unwrap(),
        );
        verify_history_manifests(extension, &actual, &AtomicBool::new(false)).unwrap();
        actual.get_mut(&asset.id).unwrap().occurrences[0].label = "Resealed different label".into();
        assert!(verify_history_manifests(extension, &actual, &AtomicBool::new(false)).is_err());
        assert!(bounded_manifest(&"x".repeat(MAX_REVISION_PROJECT_BYTES)).is_err());
        let mut full = accepted.clone();
        let initial = revisions(&full)[0].clone();
        // Structural capacity must reject before trying to append; duplicated
        // placeholders here deliberately do not pretend to be accepted history.
        full.architecture
            .as_mut()
            .unwrap()
            .scope_history
            .as_mut()
            .unwrap()
            .revisions = vec![initial.clone(); MAX_SCOPE_REVISIONS];
        let mut next = initial;
        next.scope.title = "Distinct synthetic scope".into();
        next.revision_sha256 = next.identity().unwrap();
        assert!(append(&full, next)
            .unwrap_err()
            .to_string()
            .contains("capacity is 64"));
        assert_eq!(revisions(&full).len(), MAX_SCOPE_REVISIONS);
    }

    #[test]
    fn ordinary_save_preserves_history_but_can_select_an_earlier_revision() {
        let (_temp, root, imported, scope) = fixture();
        let first = initialize_scope(
            &root,
            &imported.project,
            scope.clone(),
            &AtomicBool::new(false),
        )
        .unwrap();
        let parent = &revisions(&first)[0].revision_sha256;
        let mut changed_scope = revisions(&first)[0].scope.clone();
        changed_scope.title = "S2 reviewed documentation".into();
        assert!(initialize_scope(
            &root,
            &first,
            changed_scope.clone(),
            &AtomicBool::new(false)
        )
        .is_err());
        let plan = prepare_candidate(
            &root,
            &first,
            parent,
            &changed_scope,
            &AtomicBool::new(false),
        )
        .unwrap();
        let second = accept(
            &root,
            &first,
            parent,
            &plan.plan_sha256,
            changed_scope,
            decisions(&plan),
            &AtomicBool::new(false),
        )
        .unwrap();
        let selected_first =
            activate(&second, Some(&revisions(&first)[0].revision_sha256)).unwrap();
        validate_preserved(&second, &selected_first).unwrap();
        assert!(validate_preserved(&second, &first).is_err());
        assert!(validate_preserved(&first, &imported.project).is_err());
        let mut legacy = imported.project.clone();
        legacy.architecture.as_mut().unwrap().scope = Some(scope.clone());
        let adopted_legacy =
            initialize_scope(&root, &legacy, scope, &AtomicBool::new(false)).unwrap();
        validate_preserved(&legacy, &adopted_legacy).unwrap();
        assert!(validate_preserved(&legacy, &imported.project).is_err());
        let mut different_document = first.clone();
        different_document.id = uuid::Uuid::new_v4().to_string();
        assert!(validate_preserved(&first, &different_document).is_err());
    }
}
