//! Versioned disk projection for immutable scope history. The application keeps
//! its bounded hydrated contracts; only v2 history is stored in content-bound
//! sidecars. Revision algorithms and issued product bytes are not changed.
use crate::{persistence, reference::ArchitecturalProject, study_revisions::ScopeHistory, Project};
use anyhow::{ensure, Context, Result};
use greenlens_core::study_revision::{ScopeRevision, MAX_SCOPE_REVISIONS};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::Write,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub const EXTERNAL_HISTORY_SCHEMA: &str = "greenlens.scope-history.v2";
/// Bound the complete serialized history before hydration, independently of
/// the 16 MiB manifest and shared 64/256 MiB entry/package bounds. This is a
/// serialized-data admission bound, not a claim about allocator/process memory.
pub const MAX_HISTORY_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HistoryEntry {
    pub revision_sha256: String,
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct ProjectMetadata {
    pub project_id: String,
    pub project_name: String,
    pub static_runs: usize,
    pub climate_runs: usize,
    pub energy_runs: usize,
}

fn checkpoint(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Scope history operation cancelled; the accepted manifest is unchanged"
    );
    crate::project_open::checkpoint(0)
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn is_external(project: &Project) -> bool {
    project
        .architecture
        .as_ref()
        .and_then(|a| a.scope_history.as_ref())
        .is_some_and(|h| h.schema_version == EXTERNAL_HISTORY_SCHEMA)
}

fn validate_entries(entries: &[HistoryEntry], active: Option<&str>) -> Result<()> {
    ensure!(
        !entries.is_empty() && entries.len() <= MAX_SCOPE_REVISIONS,
        "External history must retain one to 64 revisions"
    );
    let mut identities = BTreeSet::new();
    let mut bytes = 0u64;
    for entry in entries {
        ensure!(
            is_digest(&entry.revision_sha256)
                && is_digest(&entry.sha256)
                && entry.path == format!("scope-history/{}.json", entry.revision_sha256)
                && entry.bytes > 0
                && entry.bytes <= MAX_HISTORY_BYTES,
            "Invalid immutable history descriptor"
        );
        ensure!(
            identities.insert(entry.revision_sha256.as_str()),
            "Duplicate immutable history descriptor"
        );
        bytes = bytes
            .checked_add(entry.bytes)
            .context("History size overflow")?;
        ensure!(bytes <= MAX_HISTORY_BYTES, "Complete scope history exceeds the 32 MiB serialized hydration bound; preserve this document and start a separately scoped study. No history was pruned");
    }
    ensure!(
        active.is_none_or(|id| identities.contains(id)),
        "Active scope revision is absent from the history descriptors"
    );
    Ok(())
}

fn bounded_pretty(value: &impl Serialize, limit: usize) -> Result<Vec<u8>> {
    struct Bounded {
        bytes: Vec<u8>,
        limit: usize,
    }
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            let end = self
                .bytes
                .len()
                .checked_add(bytes.len())
                .ok_or_else(|| std::io::Error::other("Serialization size overflow"))?;
            if end >= self.limit {
                return Err(std::io::Error::other(
                    "Bounded JSON serialization exceeded its capacity",
                ));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut output = Bounded {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer_pretty(&mut output, value)?;
    output.bytes.push(b'\n');
    Ok(output.bytes)
}

fn revision_bytes(revision: &ScopeRevision) -> Result<Vec<u8>> {
    bounded_pretty(revision, MAX_HISTORY_BYTES as usize)
        .context("Immutable revision exceeds the bounded history representation")
}

pub fn entries(project: &Project) -> Result<Vec<HistoryEntry>> {
    if !is_external(project) {
        return Ok(Vec::new());
    }
    let history = project
        .architecture
        .as_ref()
        .and_then(|a| a.scope_history.as_ref())
        .context("External history missing")?;
    entries_for_history(history)
}

pub(crate) fn entries_for_history(history: &ScopeHistory) -> Result<Vec<HistoryEntry>> {
    ensure!(
        history.schema_version == EXTERNAL_HISTORY_SCHEMA,
        "Expected external history storage"
    );
    ensure!(
        !history.revisions.is_empty() && history.revisions.len() <= MAX_SCOPE_REVISIONS,
        "External history must retain one to 64 revisions"
    );
    let mut output = Vec::with_capacity(history.revisions.len());
    let mut total = 0u64;
    for revision in &history.revisions {
        crate::project_open::checkpoint(0)?;
        let bytes = revision_bytes(revision)?;
        total = total
            .checked_add(bytes.len() as u64)
            .context("History size overflow")?;
        ensure!(total <= MAX_HISTORY_BYTES, "Complete scope history exceeds the 32 MiB serialized hydration bound; preserve this document and start a separately scoped study. No history was pruned");
        output.push(HistoryEntry {
            revision_sha256: revision.revision_sha256.clone(),
            path: format!("scope-history/{}.json", revision.revision_sha256),
            bytes: bytes.len() as u64,
            sha256: persistence::bytes_hash(&bytes),
        });
    }
    validate_entries(&output, history.active_revision_sha256.as_deref())?;
    Ok(output)
}

#[derive(Serialize)]
struct HistoryIndexRef<'a> {
    schema_version: &'static str,
    entries: &'a [HistoryEntry],
    active_revision_sha256: &'a Option<String>,
}
#[derive(Serialize)]
struct ArchitectureRef<'a> {
    schema_version: &'a str,
    assets: &'a [crate::reference::ReferenceAsset],
    active_asset_id: &'a str,
    scope: &'a Option<greenlens_core::architectural_scope::ArchitecturalScope>,
    scope_history: HistoryIndexRef<'a>,
}
#[derive(Serialize)]
struct ProjectRef<'a> {
    schema_version: &'a str,
    id: &'a str,
    name: &'a str,
    alternatives: &'a [crate::Alternative],
    runs: &'a [crate::RunSummary],
    #[serde(skip_serializing_if = "Option::is_none")]
    display_state: &'a Option<crate::display::DisplayState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    climate: &'a Option<crate::climate::ClimateProject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    energy: &'a Option<crate::energy::EnergyProject>,
    architecture: ArchitectureRef<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    report_issues: &'a Option<crate::report_issues::ReportIssueIndex>,
    #[serde(skip_serializing_if = "Option::is_none")]
    design_reports: &'a Option<crate::design_report::DesignReportIndex>,
}

pub fn manifest_bytes(project: &Project) -> Result<Vec<u8>> {
    let cap = if project.design_reports.is_some()
        || project
            .architecture
            .as_ref()
            .is_some_and(|a| a.scope_history.is_some())
    {
        crate::study_revisions::MAX_REVISION_PROJECT_BYTES
    } else {
        persistence::MAX_EVIDENCE_FILE_BYTES as usize
    };
    if !is_external(project) {
        return bounded_pretty(project, cap);
    }
    let history_entries = entries(project)?;
    external_manifest_bytes(project, &history_entries)
}

fn external_manifest_bytes(project: &Project, history_entries: &[HistoryEntry]) -> Result<Vec<u8>> {
    let architecture = project
        .architecture
        .as_ref()
        .context("External history requires architecture")?;
    let history = architecture
        .scope_history
        .as_ref()
        .context("External history missing")?;
    validate_entries(history_entries, history.active_revision_sha256.as_deref())?;
    let value = ProjectRef {
        schema_version: &project.schema_version,
        id: &project.id,
        name: &project.name,
        alternatives: &project.alternatives,
        runs: &project.runs,
        display_state: &project.display_state,
        climate: &project.climate,
        energy: &project.energy,
        architecture: ArchitectureRef {
            schema_version: &architecture.schema_version,
            assets: &architecture.assets,
            active_asset_id: &architecture.active_asset_id,
            scope: &architecture.scope,
            scope_history: HistoryIndexRef {
                schema_version: EXTERNAL_HISTORY_SCHEMA,
                entries: history_entries,
                active_revision_sha256: &history.active_revision_sha256,
            },
        },
        report_issues: &project.report_issues,
        design_reports: &project.design_reports,
    };
    bounded_pretty(&value, crate::study_revisions::MAX_REVISION_PROJECT_BYTES)
        .context("Project manifest exceeds the unchanged 16 MiB capacity")
}

#[derive(Serialize)]
#[serde(tag = "schema_version", deny_unknown_fields)]
enum DiskHistory {
    #[serde(rename = "greenlens.scope-history.v1")]
    Inline {
        revisions: Vec<ScopeRevision>,
        active_revision_sha256: Option<String>,
    },
    #[serde(rename = "greenlens.scope-history.v2")]
    External {
        entries: Vec<HistoryEntry>,
        active_revision_sha256: Option<String>,
    },
}

// Internally tagged enum deserialization first buffers every member in a
// generic Content tree. Historical inline histories can be large; parse their
// typed records directly instead, retaining integer/f64 token distinctions.
// Presence is explicit so another codec's field (including null) cannot be
// silently accepted as an absent optional member.
#[derive(Default)]
enum Present<T> {
    #[default]
    Missing,
    Value(T),
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Present<T> {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self::Value)
    }
}
impl<'de> Deserialize<'de> for DiskHistory {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        use serde::de::Error as _;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            schema_version: String,
            #[serde(default)]
            revisions: Present<Vec<ScopeRevision>>,
            #[serde(default)]
            entries: Present<Vec<HistoryEntry>>,
            active_revision_sha256: Option<String>,
        }
        let wire = Wire::deserialize(deserializer)?;
        match (wire.schema_version.as_str(), wire.revisions, wire.entries) {
            (
                crate::study_revisions::SCOPE_HISTORY_SCHEMA,
                Present::Value(revisions),
                Present::Missing,
            ) => Ok(Self::Inline {
                revisions,
                active_revision_sha256: wire.active_revision_sha256,
            }),
            (EXTERNAL_HISTORY_SCHEMA, Present::Missing, Present::Value(entries)) => {
                Ok(Self::External {
                    entries,
                    active_revision_sha256: wire.active_revision_sha256,
                })
            }
            _ => Err(D::Error::custom(
                "History codec must contain exactly its versioned revisions or entries field",
            )),
        }
    }
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DiskArchitecture {
    schema_version: String,
    assets: Vec<crate::reference::ReferenceAsset>,
    active_asset_id: String,
    scope: Option<greenlens_core::architectural_scope::ArchitecturalScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scope_history: Option<DiskHistory>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DiskProject {
    schema_version: String,
    id: String,
    name: String,
    alternatives: Vec<crate::Alternative>,
    runs: Vec<crate::RunSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    display_state: Option<crate::display::DisplayState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    climate: Option<crate::climate::ClimateProject>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    energy: Option<crate::energy::EnergyProject>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    architecture: Option<DiskArchitecture>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    report_issues: Option<crate::report_issues::ReportIssueIndex>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    design_reports: Option<crate::design_report::DesignReportIndex>,
}

fn decode_disk(bytes: &[u8], cancel: &AtomicBool) -> Result<DiskProject> {
    checkpoint(cancel)?;
    ensure!(
        bytes.len() as u64 <= persistence::MAX_EVIDENCE_FILE_BYTES,
        "Manifest exceeds the shared file bound"
    );
    let disk: DiskProject =
        serde_json::from_slice(bytes).context("Invalid versioned project manifest")?;
    checkpoint(cancel)?;
    if let Some(history) = disk
        .architecture
        .as_ref()
        .and_then(|a| a.scope_history.as_ref())
    {
        ensure!(
            bytes.len() <= crate::study_revisions::MAX_REVISION_PROJECT_BYTES,
            "History manifest exceeds its 16 MiB capacity"
        );
        // Compact input cannot bypass the canonical pretty-serialization cap.
        bounded_pretty(&disk, crate::study_revisions::MAX_REVISION_PROJECT_BYTES)?;
        if let DiskHistory::External {
            entries,
            active_revision_sha256,
        } = history
        {
            validate_entries(entries, active_revision_sha256.as_deref())?;
        }
    }
    checkpoint(cancel)?;
    Ok(disk)
}

impl DiskProject {
    fn hydrate(self, root: &Path, cancel: &AtomicBool) -> Result<Project> {
        let architecture = self.architecture.map(|value| -> Result<ArchitecturalProject> {
            let scope_history = value.scope_history.map(|history| -> Result<ScopeHistory> {
                match history {
                    DiskHistory::Inline { revisions, active_revision_sha256 } => Ok(ScopeHistory {
                        schema_version: crate::study_revisions::SCOPE_HISTORY_SCHEMA.into(),
                        revisions, active_revision_sha256,
                    }),
                    DiskHistory::External { entries, active_revision_sha256 } => {
                        validate_entries(&entries, active_revision_sha256.as_deref())?;
                        let mut revisions = Vec::with_capacity(entries.len());
                        for entry in &entries {
                            checkpoint(cancel)?;
                            let bytes = persistence::read_evidence_bytes_checked(
                                &persistence::regular_child(root, &entry.path)?, entry.bytes,
                                &mut |_| checkpoint(cancel),
                            )?;
                            ensure!(bytes.len() as u64 == entry.bytes && persistence::bytes_hash(&bytes) == entry.sha256, "Immutable history bytes differ from the manifest: {}", entry.path);
                            let revision: ScopeRevision = serde_json::from_slice(&bytes).context("Invalid immutable history revision")?;
                            checkpoint(cancel)?;
                            ensure!(revision.revision_sha256 == entry.revision_sha256, "Immutable history revision identity differs from its descriptor");
                            greenlens_core::study_revision::validate_revision(&revision)?;
                            // Pin one deterministic representation. Resealing a
                            // different spelling must not change retained bytes.
                            ensure!(revision_bytes(&revision)? == bytes, "Immutable history bytes do not use the supported deterministic representation");
                            revisions.push(revision);
                        }
                        Ok(ScopeHistory { schema_version: EXTERNAL_HISTORY_SCHEMA.into(), revisions, active_revision_sha256 })
                    }
                }
            }).transpose()?;
            Ok(ArchitecturalProject { schema_version: value.schema_version, assets: value.assets,
                active_asset_id: value.active_asset_id, scope: value.scope, scope_history })
        }).transpose()?;
        checkpoint(cancel)?;
        Ok(Project {
            schema_version: self.schema_version,
            id: self.id,
            name: self.name,
            alternatives: self.alternatives,
            runs: self.runs,
            display_state: self.display_state,
            climate: self.climate,
            energy: self.energy,
            architecture,
            report_issues: self.report_issues,
            design_reports: self.design_reports,
        })
    }
}

/// Cheap admission checks only the bounded manifest and descriptor envelope.
/// External revisions and all engine/report evidence remain explicitly unchecked.
pub fn metadata(bytes: &[u8], cancel: &AtomicBool) -> Result<ProjectMetadata> {
    let disk = decode_disk(bytes, cancel)?;
    let external = disk
        .architecture
        .as_ref()
        .and_then(|a| a.scope_history.as_ref())
        .is_some_and(|h| matches!(h, DiskHistory::External { .. }));
    if external {
        ensure!(
            disk.schema_version == "0.6.0",
            "External scope history requires architectural project schema 0.6.0"
        );
        uuid::Uuid::parse_str(&disk.id)?;
        ensure!(
            !disk.name.trim().is_empty() && disk.name.len() <= 200,
            "Invalid project name"
        );
        ensure!(
            disk.architecture
                .as_ref()
                .is_some_and(|a| a.schema_version == "greenlens.architectural-project.v1"),
            "Unsupported architecture envelope"
        );
        return Ok(ProjectMetadata {
            project_id: disk.id,
            project_name: disk.name,
            static_runs: disk.runs.len(),
            climate_runs: disk.climate.as_ref().map_or(0, |e| e.runs.len()),
            energy_runs: disk.energy.as_ref().map_or(0, |e| e.runs.len()),
        });
    }
    let project = disk.hydrate(Path::new(""), cancel)?;
    crate::validate_project(&project)?;
    Ok(ProjectMetadata {
        project_id: project.id,
        project_name: project.name,
        static_runs: project.runs.len(),
        climate_runs: project.climate.as_ref().map_or(0, |e| e.runs.len()),
        energy_runs: project.energy.as_ref().map_or(0, |e| e.runs.len()),
    })
}

/// Hydrate actual-byte-verified revision members. This is not complete project
/// admission or evidence verification: callers must next call validate_project
/// for parent order, active projection, source and issue cross-bindings, then
/// verify the other retained evidence through the ordinary project service.
pub fn decode_project(root: &Path, bytes: &[u8], cancel: &AtomicBool) -> Result<Project> {
    decode_disk(bytes, cancel)?.hydrate(root, cancel)
}

/// Read and hydrate the disk envelope; see decode_project for the additional
/// required project validation and run/report evidence verification stages.
pub fn read_project(root: &Path, cancel: &AtomicBool) -> Result<Project> {
    let bytes = persistence::read_evidence_bytes_checked(
        &persistence::regular_child(root, "project.json")?,
        persistence::MAX_EVIDENCE_FILE_BYTES,
        &mut |_| checkpoint(cancel),
    )?;
    decode_project(root, &bytes, cancel)
}

/// Stage immutable sidecars before atomically publishing their manifest. A
/// cancelled/failed publication may leave unadvertised complete sidecars; it
/// never edits or removes accepted records. Callers retain the writer lock.
pub fn stage(root: &Path, project: &Project, cancel: &AtomicBool) -> Result<()> {
    checkpoint(cancel)?;
    let descriptors = entries(project)?;
    if descriptors.is_empty() {
        return Ok(());
    }
    let revisions = crate::study_revisions::revisions(project);
    for (entry, revision) in descriptors.iter().zip(revisions) {
        checkpoint(cancel)?;
        let bytes = revision_bytes(revision)?;
        match std::fs::symlink_metadata(root.join(&entry.path)) {
            Ok(_) => {
                let existing = persistence::read_evidence_bytes_checked(
                    &persistence::regular_child(root, &entry.path)?,
                    entry.bytes,
                    &mut |_| checkpoint(cancel),
                )?;
                ensure!(
                    existing == bytes && persistence::bytes_hash(&existing) == entry.sha256,
                    "Retained history sidecar was changed; it will not be overwritten: {}",
                    entry.path
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                persistence::write_new_verified_evidence_bytes(
                    root,
                    &entry.path,
                    &bytes,
                    &entry.sha256,
                    &mut |_| checkpoint(cancel),
                )?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    checkpoint(cancel)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::fs;

    fn external_fixture() -> (tempfile::TempDir, std::path::PathBuf, Project) {
        let (directory, root, imported, scope) = crate::study_revisions::tests::fixture();
        let project = crate::study_revisions::initialize_scope(
            &root,
            &imported.project,
            scope,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(is_external(&project));
        (directory, root, project)
    }

    #[test]
    fn legacy_projection_and_revision_algorithms_remain_unchanged() {
        let (_directory, root, mut project) = external_fixture();
        project
            .architecture
            .as_mut()
            .unwrap()
            .scope_history
            .as_mut()
            .unwrap()
            .schema_version = crate::study_revisions::SCOPE_HISTORY_SCHEMA.into();
        let mut expected = serde_json::to_vec_pretty(&project).unwrap();
        expected.push(b'\n');
        let actual = manifest_bytes(&project).unwrap();
        assert_eq!(actual, expected);
        assert!(entries(&project).unwrap().is_empty());
        let decoded = decode_project(&root, &actual, &AtomicBool::new(false)).unwrap();
        crate::validate_project(&decoded).unwrap();
        assert_eq!(
            serde_json::to_value(&decoded).unwrap(),
            serde_json::to_value(&project).unwrap()
        );
        // Preserve historical integer/f64 serialization, not only numeric value.
        let mut decoded_bytes = serde_json::to_vec_pretty(&decoded).unwrap();
        decoded_bytes.push(b'\n');
        assert_eq!(decoded_bytes, expected);
        stage(&root, &project, &AtomicBool::new(false)).unwrap();
        assert!(!root.join("scope-history").exists());
    }

    #[test]
    fn external_roundtrip_is_bounded_verified_and_not_a_legacy_project() {
        let (_directory, root, project) = external_fixture();
        let prior_manifest = fs::read(root.join("project.json")).unwrap();
        let bytes = manifest_bytes(&project).unwrap();
        let cancel = AtomicBool::new(false);
        let descriptors = entries(&project).unwrap();
        assert_eq!(descriptors.len(), 1);
        // Admission can inspect paths/schema before replacement guards, but it
        // cannot hydrate or advertise missing revision bytes as verified.
        assert_eq!(metadata(&bytes, &cancel).unwrap().project_id, project.id);
        assert!(decode_project(&root, &bytes, &cancel).is_err());
        assert!(serde_json::from_slice::<Project>(&bytes).is_err());
        stage(&root, &project, &cancel).unwrap();
        let original_sidecar = fs::read(root.join(&descriptors[0].path)).unwrap();
        stage(&root, &project, &cancel).unwrap();
        assert_eq!(
            fs::read(root.join(&descriptors[0].path)).unwrap(),
            original_sidecar
        );
        assert_eq!(fs::read(root.join("project.json")).unwrap(), prior_manifest);
        let decoded = decode_project(&root, &bytes, &cancel).unwrap();
        crate::validate_project(&decoded).unwrap();
        assert_eq!(
            serde_json::to_value(&decoded).unwrap(),
            serde_json::to_value(&project).unwrap()
        );
        assert_eq!(manifest_bytes(&decoded).unwrap(), bytes);
        assert_eq!(entries(&decoded).unwrap(), descriptors);
        let mut downgrade = decoded.clone();
        downgrade
            .architecture
            .as_mut()
            .unwrap()
            .scope_history
            .as_mut()
            .unwrap()
            .schema_version = crate::study_revisions::SCOPE_HISTORY_SCHEMA.into();
        assert!(crate::study_revisions::validate_preserved(&decoded, &downgrade).is_err());
    }

    #[test]
    fn changed_and_resealed_history_cannot_be_silently_reused() {
        let (_directory, root, project) = external_fixture();
        let cancel = AtomicBool::new(false);
        stage(&root, &project, &cancel).unwrap();
        let bytes = manifest_bytes(&project).unwrap();
        let descriptor = entries(&project).unwrap().remove(0);
        let path = root.join(&descriptor.path);
        let original = fs::read(&path).unwrap();
        let original_capacity = crate::capacity::inspect(
            Some(&root),
            &project,
            &crate::capacity::Operation::Inspect,
            &cancel,
        )
        .unwrap();
        let mut changed = original.clone();
        changed.push(b' ');
        fs::write(&path, &changed).unwrap();
        assert!(decode_project(&root, &bytes, &cancel).is_err());
        assert!(stage(&root, &project, &cancel).is_err());
        assert_eq!(fs::read(&path).unwrap(), changed);
        let capacity = crate::capacity::inspect(
            Some(&root),
            &project,
            &crate::capacity::Operation::Inspect,
            &cancel,
        )
        .unwrap();
        assert!(!capacity.can_start);
        assert_eq!(
            capacity.retained_bytes,
            original_capacity.retained_bytes + 1
        );
        assert!(capacity
            .warnings
            .iter()
            .any(|warning| warning.contains("history") || warning.contains("History")));
        assert_eq!(fs::read(&path).unwrap(), changed);
        let mut resealed: Value = serde_json::from_slice(&bytes).unwrap();
        resealed["architecture"]["scope_history"]["entries"][0]["bytes"] =
            (changed.len() as u64).into();
        resealed["architecture"]["scope_history"]["entries"][0]["sha256"] =
            persistence::bytes_hash(&changed).into();
        assert!(
            decode_project(&root, &serde_json::to_vec(&resealed).unwrap(), &cancel)
                .unwrap_err()
                .to_string()
                .contains("deterministic")
        );
        // Resealing altered semantic content cannot replace historical identity.
        let mut semantic: Value = serde_json::from_slice(&original).unwrap();
        semantic["scope"]["purpose"] = "Different historical assertion".into();
        let changed = serde_json::to_vec_pretty(&semantic).unwrap();
        fs::write(&path, &changed).unwrap();
        resealed["architecture"]["scope_history"]["entries"][0]["bytes"] =
            (changed.len() as u64).into();
        resealed["architecture"]["scope_history"]["entries"][0]["sha256"] =
            persistence::bytes_hash(&changed).into();
        assert!(decode_project(&root, &serde_json::to_vec(&resealed).unwrap(), &cancel).is_err());
    }

    #[test]
    fn descriptor_paths_and_hydration_budgets_fail_before_reading_sidecars() {
        let (_directory, _root, project) = external_fixture();
        let cancel = AtomicBool::new(false);
        let mut disk: Value = serde_json::from_slice(&manifest_bytes(&project).unwrap()).unwrap();
        disk["architecture"]["scope_history"]["entries"][0]["path"] = "../outside.json".into();
        assert!(metadata(&serde_json::to_vec(&disk).unwrap(), &cancel).is_err());
        let entry = HistoryEntry {
            revision_sha256: "a".repeat(64),
            path: format!("scope-history/{}.json", "a".repeat(64)),
            bytes: MAX_HISTORY_BYTES,
            sha256: "b".repeat(64),
        };
        assert!(
            validate_entries(std::slice::from_ref(&entry), Some(&entry.revision_sha256)).is_ok()
        );
        let second = HistoryEntry {
            revision_sha256: "c".repeat(64),
            path: format!("scope-history/{}.json", "c".repeat(64)),
            bytes: 1,
            sha256: "d".repeat(64),
        };
        assert!(validate_entries(&[entry.clone(), second], None).is_err());
        assert!(validate_entries(&[entry.clone(), entry], None).is_err());
        assert!(validate_entries(&[], None).is_err());
        assert!(bounded_pretty(&"abcd", 6).is_err());
        assert_eq!(bounded_pretty(&"abcd", 7).unwrap(), b"\"abcd\"\n");
    }

    #[test]
    fn cancelled_storage_does_not_publish_or_mutate_accepted_data() {
        let (_directory, root, project) = external_fixture();
        let before = fs::read(root.join("project.json")).unwrap();
        let bytes = manifest_bytes(&project).unwrap();
        let cancelled = AtomicBool::new(true);
        assert!(stage(&root, &project, &cancelled).is_err());
        assert!(metadata(&bytes, &cancelled).is_err());
        assert!(decode_project(&root, &bytes, &cancelled).is_err());
        assert!(!root.join("scope-history").exists());
        assert_eq!(fs::read(root.join("project.json")).unwrap(), before);
    }

    #[test]
    fn fourth_revision_envelope_grows_by_a_descriptor_not_an_inline_record() {
        let (_directory, _root, project) = external_fixture();
        // Descriptor-only admission fixture: declared bodies are deliberately
        // absent and cannot pass hydration or masquerade as accepted evidence.
        // Four-MiB declared records test capacity without allocating redundant
        // revision trees or generating unnecessary simulation evidence.
        let mut descriptors = entries(&project).unwrap();
        descriptors[0].bytes = 4 * 1024 * 1024;
        for digit in 1..8 {
            let identity = format!("{digit:064x}");
            descriptors.push(HistoryEntry {
                revision_sha256: identity.clone(),
                path: format!("scope-history/{identity}.json"),
                bytes: 4 * 1024 * 1024,
                sha256: "f".repeat(64),
            });
        }
        let three = external_manifest_bytes(&project, &descriptors[..3]).unwrap();
        let four = external_manifest_bytes(&project, &descriptors[..4]).unwrap();
        assert!(four.len() > three.len() && four.len() - three.len() < 1024);
        assert!(four.len() < crate::study_revisions::MAX_REVISION_PROJECT_BYTES);
        assert_eq!(
            descriptors[..4]
                .iter()
                .map(|entry| entry.bytes)
                .sum::<u64>(),
            crate::study_revisions::MAX_REVISION_PROJECT_BYTES as u64
        );
        assert!(external_manifest_bytes(&project, &descriptors).is_ok());
        let identity = format!("{:064x}", 9);
        descriptors.push(HistoryEntry {
            revision_sha256: identity.clone(),
            path: format!("scope-history/{identity}.json"),
            bytes: 1,
            sha256: "f".repeat(64),
        });
        assert!(external_manifest_bytes(&project, &descriptors).is_err());
    }

    #[test]
    fn direct_history_codec_rejects_cross_version_null_duplicate_and_unknown_fields() {
        for source in [
            r#"{"schema_version":"greenlens.scope-history.v1","revisions":[],"entries":[],"active_revision_sha256":null}"#,
            r#"{"schema_version":"greenlens.scope-history.v1","revisions":[],"entries":null,"active_revision_sha256":null}"#,
            r#"{"schema_version":"greenlens.scope-history.v2","entries":[],"revisions":null,"active_revision_sha256":null}"#,
            r#"{"schema_version":"greenlens.scope-history.v2","entries":[],"entries":[],"active_revision_sha256":null}"#,
            r#"{"schema_version":"greenlens.scope-history.v2","entries":[],"unknown":null,"active_revision_sha256":null}"#,
        ] {
            assert!(
                serde_json::from_str::<DiskHistory>(source).is_err(),
                "Unexpectedly accepted {source}"
            );
        }
    }
}
