//! Complete portable issued projects. Every retained run, reference, scope and
//! issue remains included. Imported recipes are inert; replay dispatches only
//! the explicitly selected issue's A/B through the compiled energy adapter.
use crate::{energy_series::EnergySeriesReader, persistence::*, Project, ProjectView};
use anyhow::{ensure, Context, Result};
use greenlens_core::canonical_hash;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use uuid::Uuid;

pub const PACKAGE_SCHEMA: &str = "greenlens.issued-project-export.v0.7";
pub const INSPECTION_SCHEMA: &str = "greenlens.issued-project-inspection.v0.7";
pub const REPLAY_SCHEMA: &str = "greenlens.issued-project-replay.v0.7";
const MANIFEST_NAME: &str = "issued-project-export.json";
const RECIPE_NAME: &str = "REPLAY.txt";
const MAX_PACKAGE_METADATA_BYTES: u64 = 4 * 1024 * 1024;
const IO_BLOCK: usize = 64 * 1024;
const INCLUSION_POLICY: &str = "Complete retained project: every accepted scope revision, report issue and required normalized reference asset; all retained runs, weather, sensitivity records and explicitly included attachments. Original external architectural source files and engines are not included. No retained history was pruned.";
const REPLAY_TEXT: &str = "GreenLens v0.7 complete issued-project evidence\n\nThis package retains the complete accepted project and all report issue bytes. It is not an archive of the original external architectural source. Engine binaries, original external paths and active scratch are not required for inspection. Retained normalized reference assets are included.\n\nInspect and extract with the trusted GreenLens issued-project package reader. It checks archive paths, shared limits, exact member hashes, immutable issue bindings and retained scientific evidence. No command embedded in the package is executed.\n\nRegeneration uses a selected retained issue, its exact recognized owned renderer/template/font identity and the separately installed pinned Typst/fonts. It does not run EnergyPlus. Existing products remain usable if the exact renderer is unavailable.\n\nReplay is an explicit action selecting one issue ID. Only that issue's baseline/proposed energy runs are repeated, using separately installed compatible EnergyPlus and the compiled recognized method. Other historical runs are preserved but not silently rerun. Every native channel must agree within max(1e-6 in its reported unit, 0.1% of reference), with exact dimensions, labels and timestamps. Replay agreement is integration verification, not physical validation or qualification.\n";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackageMember {
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IssuedProjectPackage {
    pub schema_version: String,
    pub content_sha256: String,
    pub project_id: String,
    pub project_file_sha256: String,
    pub project_content_sha256: String,
    pub issue_ids: Vec<String>,
    pub scope_revision_ids: Vec<String>,
    pub inclusion_policy: String,
    /// Exact source-project members only. The outer manifest/readme/seal are
    /// separately covered by the shared package seal, without self-reference.
    pub members: BTreeMap<String, PackageMember>,
}
impl IssuedProjectPackage {
    pub fn identity(&self) -> Result<String> {
        let mut value = self.clone();
        value.content_sha256.clear();
        Ok(canonical_hash(&(PACKAGE_SCHEMA, value))?)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == PACKAGE_SCHEMA && self.inclusion_policy == INCLUSION_POLICY,
            "Unsupported issued-project package or incomplete inclusion policy"
        );
        ensure!(
            Uuid::parse_str(&self.project_id)?.to_string() == self.project_id,
            "Package project identity is invalid"
        );
        for value in [
            &self.content_sha256,
            &self.project_file_sha256,
            &self.project_content_sha256,
        ] {
            ensure!(is_hash(value), "Invalid package content digest");
        }
        ensure!(
            !self.issue_ids.is_empty() && self.issue_ids.len() <= crate::report_issues::MAX_ISSUES,
            "An issued-project package requires one to 32 immutable report issues"
        );
        ensure!(
            !self.scope_revision_ids.is_empty()
                && self.scope_revision_ids.len()
                    <= greenlens_core::study_revision::MAX_SCOPE_REVISIONS,
            "An issued-project package requires retained bounded scope history"
        );
        for ids in [&self.issue_ids, &self.scope_revision_ids] {
            ensure!(
                ids.iter().all(|id| is_hash(id))
                    && ids.iter().collect::<BTreeSet<_>>().len() == ids.len(),
                "Package contains invalid or duplicate historical identities"
            );
        }
        let mut budget = PackageBudget::default();
        ensure!(
            self.members.len() + 3 <= MAX_PACKAGE_ENTRIES,
            "Complete package exceeds shared entry capacity; no history may be silently pruned"
        );
        for (name, member) in &self.members {
            ensure!(
                ![MANIFEST_NAME, RECIPE_NAME, "checksums.json"].contains(&name.as_str()),
                "Package source inventory cannot contain its own envelope"
            );
            ensure!(is_hash(&member.sha256), "Invalid retained member digest");
            budget.add(name, member.bytes)?;
        }
        ensure!(
            self.members
                .get("project.json")
                .is_some_and(|m| m.sha256 == self.project_file_sha256),
            "Package does not bind its exact retained project manifest"
        );
        ensure!(
            self.content_sha256 == self.identity()?,
            "Issued-project package identity differs"
        );
        ensure!(
            serde_json::to_vec(self)?.len() as u64 <= MAX_PACKAGE_METADATA_BYTES,
            "Issued-project inventory exceeds its 4 MiB admission bound"
        );
        Ok(())
    }
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn checkpoint(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Issued-project operation cancelled; no completed package or replay was published"
    );
    crate::project_open::checkpoint(0)
}
fn io_checkpoint(cancel: &AtomicBool, started: Instant) -> Result<()> {
    checkpoint(cancel)?;
    ensure!(
        started.elapsed() < Duration::from_secs(crate::project_open::VERIFICATION_DEADLINE_SECONDS),
        "Issued-project I/O stage exceeded its 120-second budget"
    );
    Ok(())
}
fn bounded_read(root: &Path, name: &str, limit: u64, cancel: &AtomicBool) -> Result<Vec<u8>> {
    read_evidence_bytes_checked(&regular_child(root, name)?, limit, &mut |bytes| {
        checkpoint(cancel)?;
        crate::project_open::checkpoint(bytes)
    })
}
fn json_bytes(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Inventory only: the caller must first complete the shared full reader.
/// It reads known seals, not arbitrary directory contents or machine files.
fn names(root: &Path, project: &Project, cancel: &AtomicBool) -> Result<BTreeSet<String>> {
    crate::validate_project(project)?;
    let mut prefixes = BTreeSet::new();
    for run in &project.runs {
        prefixes.insert(run.path.clone());
    }
    if let Some(climate) = &project.climate {
        for run in &climate.runs {
            prefixes.insert(run.path.clone());
        }
        for weather in &climate.weather {
            prefixes.insert(weather.path.clone());
        }
    }
    if let Some(energy) = &project.energy {
        for run in &energy.runs {
            prefixes.insert(run.path.clone());
        }
        for weather in &energy.weather {
            prefixes.insert(weather.path.clone());
        }
        for attachment in &energy.attachments {
            prefixes.insert(format!("attachments/{}", attachment.id));
        }
    }
    if let Some(reference) = &project.architecture {
        for asset in &reference.assets {
            prefixes.insert(asset.path.clone());
        }
    }
    if let Some(index) = &project.report_issues {
        for issue in &index.issues {
            prefixes.insert(issue.path.clone());
        }
    }
    let mut result = BTreeSet::from(["project.json".into()]);
    // External history remains a required part of the complete project. The
    // full reader has already checked both these bytes and revision semantics;
    // inventory the exact files again rather than regenerating their contents.
    for entry in crate::history_storage::entries(project)? {
        checkpoint(cancel)?;
        ensure!(
            result.insert(entry.path),
            "Scope history overlaps retained evidence"
        );
    }
    for prefix in prefixes {
        checkpoint(cancel)?;
        relative_path(&prefix)?;
        let seal_name = format!("{prefix}/checksums.json");
        let seal: Checksums = serde_json::from_slice(&bounded_read(
            root,
            &seal_name,
            MAX_PACKAGE_METADATA_BYTES,
            cancel,
        )?)?;
        ensure!(
            seal.schema_version == "0.1.0"
                && !seal.sha256.is_empty()
                && seal.sha256.len() < MAX_PACKAGE_ENTRIES,
            "Invalid retained evidence member inventory"
        );
        for (name, digest) in &seal.sha256 {
            relative_path(name)?;
            ensure!(is_hash(digest), "Invalid retained seal digest");
            ensure!(
                result.insert(format!("{prefix}/{name}")),
                "Retained evidence paths overlap"
            );
        }
        ensure!(result.insert(seal_name), "Retained evidence paths overlap");
    }
    if let Some(workbench) = project.energy.as_ref().and_then(|e| e.workbench.as_ref()) {
        for session in &workbench.sensitivity {
            ensure!(
                result.insert(format!("{}/plan.json", session.path)),
                "Sensitivity evidence overlaps retained members"
            );
            for event in &session.events {
                ensure!(
                    result.insert(format!("{}/event-{:03}.json", session.path, event.sequence)),
                    "Sensitivity evidence overlaps retained members"
                );
            }
        }
    }
    let mut budget = PackageBudget::default();
    for name in &result {
        checkpoint(cancel)?;
        budget.add(name, fs::metadata(regular_child(root, name)?)?.len())?;
    }
    Ok(result)
}

fn declared_privacy(project: &Project) -> Result<()> {
    ensure!(
        project
            .report_issues
            .as_ref()
            .is_some_and(|index| !index.issues.is_empty()),
        "Issue a report before exporting a complete issued project"
    );
    ensure!(project.energy.as_ref().is_none_or(|e| e.attachments.iter().all(|a| a.include_in_export)),
        "This complete issued-project package retains all history and attachments. Some attachments are marked excluded: review their inclusion explicitly before export. Nothing was silently omitted or exported");
    Ok(())
}

fn bind_package(
    project: &Project,
    members: BTreeMap<String, PackageMember>,
) -> Result<IssuedProjectPackage> {
    let mut package = IssuedProjectPackage {
        schema_version: PACKAGE_SCHEMA.into(),
        content_sha256: String::new(),
        project_id: project.id.clone(),
        project_file_sha256: members
            .get("project.json")
            .context("Project member missing")?
            .sha256
            .clone(),
        project_content_sha256: canonical_hash(project)?,
        issue_ids: project
            .report_issues
            .as_ref()
            .context("Issued history missing")?
            .issues
            .iter()
            .map(|i| i.id.clone())
            .collect(),
        scope_revision_ids: crate::study_revisions::revisions(project)
            .iter()
            .map(|r| r.revision_sha256.clone())
            .collect(),
        inclusion_policy: INCLUSION_POLICY.into(),
        members,
    };
    package.content_sha256 = package.identity()?;
    package.validate()?;
    Ok(package)
}

struct PackageEnvelope {
    manifest: Vec<u8>,
    seal: Vec<u8>,
    members: BTreeMap<String, PackageMember>,
}

fn envelope(package: &IssuedProjectPackage) -> Result<PackageEnvelope> {
    package.validate()?;
    let manifest = json_bytes(package)?;
    ensure!(
        manifest.len() as u64 <= MAX_PACKAGE_METADATA_BYTES,
        "Pretty-printed package inventory exceeds 4 MiB"
    );
    let mut members = package.members.clone();
    members.insert(
        MANIFEST_NAME.into(),
        PackageMember {
            sha256: bytes_hash(&manifest),
            bytes: manifest.len() as u64,
        },
    );
    members.insert(
        RECIPE_NAME.into(),
        PackageMember {
            sha256: bytes_hash(REPLAY_TEXT.as_bytes()),
            bytes: REPLAY_TEXT.len() as u64,
        },
    );
    let seal = json_bytes(&Checksums {
        schema_version: "0.1.0".into(),
        sha256: members
            .iter()
            .map(|(name, m)| (name.clone(), m.sha256.clone()))
            .collect(),
    })?;
    members.insert(
        "checksums.json".into(),
        PackageMember {
            sha256: bytes_hash(&seal),
            bytes: seal.len() as u64,
        },
    );
    let mut budget = PackageBudget::default();
    for (name, member) in &members {
        budget.add(name, member.bytes)?;
    }
    Ok(PackageEnvelope {
        manifest,
        seal,
        members,
    })
}

/// Every source member is copied with its original byte binding. The complete
/// inventory has already passed shared bounds; no destination is published yet.
fn stage_pinned(
    root: &Path,
    stage: &Path,
    package: &IssuedProjectPackage,
    cancel: &AtomicBool,
) -> Result<BTreeMap<String, PackageMember>> {
    let PackageEnvelope {
        manifest,
        seal,
        members,
    } = envelope(package)?;
    fs::create_dir_all(stage)?;
    let started = Instant::now();
    for (name, expected) in &package.members {
        io_checkpoint(cancel, started)?;
        let bytes = bounded_read(root, name, expected.bytes, cancel)?;
        ensure!(
            bytes.len() as u64 == expected.bytes && bytes_hash(&bytes) == expected.sha256,
            "Retained source changed during export: {name}; no package was published"
        );
        write_new_evidence_bytes(stage, name, &bytes)?;
    }
    write_new_evidence_bytes(stage, MANIFEST_NAME, &manifest)?;
    write_new_evidence_bytes(stage, RECIPE_NAME, REPLAY_TEXT.as_bytes())?;
    write_new_evidence_bytes(stage, "checksums.json", &seal)?;
    io_checkpoint(cancel, started)?;
    Ok(members)
}

/// Cancellable writer using the same path and resource admission as the shared
/// reader. Hashes are checked while compressing, including equal-size changes.
fn write_zip(
    stage: &Path,
    members: &BTreeMap<String, PackageMember>,
    destination: &Path,
    cancel: &AtomicBool,
) -> Result<()> {
    ensure!(
        !destination.exists(),
        "Export destination exists; choose a new filename"
    );
    let mut budget = PackageBudget::default();
    for (name, member) in members {
        budget.add(name, member.bytes)?;
    }
    let temporary = destination.with_extension(format!("{}.tmp", Uuid::new_v4()));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let started = Instant::now();
    let mut buffer = [0; IO_BLOCK];
    for (name, expected) in members {
        io_checkpoint(cancel, started)?;
        archive.start_file(name, options)?;
        let mut file = File::open(regular_child(stage, name)?)?;
        let mut digest = Sha256::new();
        let mut total = 0u64;
        loop {
            io_checkpoint(cancel, started)?;
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            total += count as u64;
            ensure!(total <= expected.bytes, "Staged export member grew: {name}");
            digest.update(&buffer[..count]);
            archive.write_all(&buffer[..count])?;
        }
        ensure!(
            total == expected.bytes && format!("{:x}", digest.finalize()) == expected.sha256,
            "Staged export member changed: {name}"
        );
    }
    archive.finish()?.sync_all()?;
    io_checkpoint(cancel, started)?;
    ensure!(
        !destination.exists(),
        "Export destination appeared while packaging; existing file was preserved"
    );
    // Same-directory publication follows the existing portable writer. A
    // failed/cancelled stage leaves only an unmistakable .tmp file, not a ZIP.
    fs::rename(&temporary, destination)?;
    Ok(())
}

/// Export a saved complete project. Requiring Save first prevents an unsaved
/// client snapshot from removing later historical evidence or hiding conflicts.
pub fn export(
    root: &Path,
    project: &Project,
    destination: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<Value> {
    checkpoint(&cancel)?;
    declared_privacy(project)?;
    crate::validate_project(project)?;
    ensure!(
        !destination.exists(),
        "Export destination exists; choose a new filename"
    );
    let _lock = WriterLock::acquire(root)?;
    let stored = crate::history_storage::read_project(root, &cancel)?;
    ensure!(canonical_hash(&stored)? == canonical_hash(project)?,
        "Save the accepted document before exporting its complete history; the saved project differs from the supplied document");
    let admission = crate::project_open::admit_project(root)?;
    let view =
        crate::project_open::verify_admitted_project(root, &admission, cancel.clone(), |_| {})?;
    let source_snapshot = view
        .project_verification
        .as_ref()
        .context("Complete source verification receipt missing")?
        .content_snapshot_sha256
        .clone();
    let package = crate::project_open::inspect_evidence(
        root,
        cancel.clone(),
        |_| {},
        || {
            let mut members = BTreeMap::new();
            for name in names(root, &view.project, &cancel)? {
                checkpoint(&cancel)?;
                let path = regular_child(root, &name)?;
                members.insert(
                    name,
                    PackageMember {
                        bytes: fs::metadata(&path)?.len(),
                        sha256: evidence_file_hash(&path)?,
                    },
                );
            }
            let actual: BTreeMap<_, _> = members
                .iter()
                .map(|(name, value)| (name.clone(), value.sha256.clone()))
                .collect();
            ensure!(canonical_hash(&actual)? == source_snapshot,
            "Complete project member inventory differs from the verified source snapshot; no unchecked or changed bytes may be exported");
            check_revision(root, Some(&view.revision))?;
            bind_package(&view.project, members)
        },
    )?;
    // Preflight all retained evidence plus generated envelope before copying or
    // opening any final ZIP. No subset/pruning fallback is offered silently.
    let full_inventory = envelope(&package)?.members;
    let uncompressed_bytes: u64 = full_inventory.values().map(|m| m.bytes).sum();
    let stage = crate::local_root()?
        .join("issued-project-exports")
        .join(Uuid::new_v4().to_string());
    let members = stage_pinned(root, &stage, &package, &cancel)?;
    checkpoint(&cancel)?;
    check_revision(root, Some(&view.revision))?;
    write_zip(&stage, &members, destination, &cancel)?;
    Ok(
        json!({"schema_version":PACKAGE_SCHEMA,"state":"completed","package_content_sha256":package.content_sha256,
        "package_sha256":greenlens_process::file_hash(destination)?,"package_bytes":fs::metadata(destination)?.len(),
        "uncompressed_bytes":uncompressed_bytes,"entries":members.len(),"issue_ids":package.issue_ids,
        "scope_revision_ids":package.scope_revision_ids,"inclusion_policy":INCLUSION_POLICY}),
    )
}

/// Shared structural rules with explicit block checkpoints. No directory,
/// symlink, duplicate/case-alias or unsealed member is accepted.
fn extract_checked(
    package: &Path,
    destination: &Path,
    cancel: &AtomicBool,
) -> Result<BTreeMap<String, PackageMember>> {
    checkpoint(cancel)?;
    ensure!(!destination.exists(), "Extraction destination must be new");
    let mut archive = zip::ZipArchive::new(File::open(package)?)?;
    ensure!(
        archive.len() <= MAX_PACKAGE_ENTRIES,
        "Evidence ZIP exceeds shared entry limit"
    );
    let mut budget = PackageBudget::default();
    let mut declared = BTreeMap::new();
    for index in 0..archive.len() {
        checkpoint(cancel)?;
        let item = archive.by_index(index)?;
        budget.add(item.name(), item.size())?;
        ensure!(
            !item.is_dir()
                && item
                    .unix_mode()
                    .is_none_or(|mode| mode & 0o170000 != 0o120000),
            "Directory or symlink entries are forbidden"
        );
        declared.insert(item.name().to_owned(), item.size());
    }
    fs::create_dir_all(destination)?;
    let mut actual = BTreeMap::new();
    let mut actual_budget = PackageBudget::default();
    let started = Instant::now();
    let mut buffer = [0; IO_BLOCK];
    for index in 0..archive.len() {
        io_checkpoint(cancel, started)?;
        let mut item = archive.by_index(index)?;
        let name = item.name().to_owned();
        let path = destination.join(relative_path(&name)?);
        fs::create_dir_all(path.parent().context("Invalid extraction path")?)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        let mut digest = Sha256::new();
        let mut total = 0u64;
        loop {
            io_checkpoint(cancel, started)?;
            let count = item.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            total += count as u64;
            ensure!(
                total <= MAX_EVIDENCE_FILE_BYTES && total <= declared[&name],
                "ZIP member exceeds declared or shared byte bound"
            );
            digest.update(&buffer[..count]);
            file.write_all(&buffer[..count])?;
        }
        file.sync_all()?;
        ensure!(
            total == declared[&name],
            "ZIP member size differs from declaration"
        );
        actual_budget.add(&name, total)?;
        actual.insert(
            name,
            PackageMember {
                sha256: format!("{:x}", digest.finalize()),
                bytes: total,
            },
        );
    }
    let seal: Checksums = serde_json::from_slice(&bounded_read(
        destination,
        "checksums.json",
        MAX_PACKAGE_METADATA_BYTES,
        cancel,
    )?)?;
    ensure!(
        seal.schema_version == "0.1.0" && seal.sha256.contains_key("project.json"),
        "Package seal must bind project.json"
    );
    let expected: BTreeMap<_, _> = actual
        .iter()
        .filter(|(name, _)| name.as_str() != "checksums.json")
        .map(|(name, member)| (name.clone(), member.sha256.clone()))
        .collect();
    ensure!(
        seal.sha256 == expected,
        "Package seal coverage/digests differ from actual extracted bytes"
    );
    io_checkpoint(cancel, started)?;
    Ok(actual)
}

fn validate_extracted(
    root: &Path,
    actual: &BTreeMap<String, PackageMember>,
    cancel: &AtomicBool,
) -> Result<(IssuedProjectPackage, Project)> {
    let package: IssuedProjectPackage = serde_json::from_slice(&bounded_read(
        root,
        MANIFEST_NAME,
        MAX_PACKAGE_METADATA_BYTES,
        cancel,
    )?)?;
    package.validate()?;
    let PackageEnvelope {
        manifest,
        seal,
        members: expected,
    } = envelope(&package)?;
    ensure!(&expected == actual, "Package has missing, changed or undeclared evidence; complete history cannot be silently omitted");
    ensure!(
        bounded_read(root, MANIFEST_NAME, MAX_PACKAGE_METADATA_BYTES, cancel)? == manifest
            && bounded_read(root, "checksums.json", MAX_PACKAGE_METADATA_BYTES, cancel)? == seal,
        "Package envelope serialization differs from the supported deterministic contract"
    );
    let project_bytes = bounded_read(root, "project.json", MAX_EVIDENCE_FILE_BYTES, cancel)?;
    ensure!(
        bytes_hash(&project_bytes) == package.project_file_sha256,
        "Project bytes differ from the package binding"
    );
    let project = crate::history_storage::decode_project(root, &project_bytes, cancel)?;
    crate::validate_project(&project)?;
    declared_privacy(&project)?;
    ensure!(
        project.id == package.project_id
            && canonical_hash(&project)? == package.project_content_sha256,
        "Package project content differs"
    );
    ensure!(
        project
            .report_issues
            .as_ref()
            .unwrap()
            .issues
            .iter()
            .map(|i| i.id.clone())
            .collect::<Vec<_>>()
            == package.issue_ids
            && crate::study_revisions::revisions(&project)
                .iter()
                .map(|r| r.revision_sha256.clone())
                .collect::<Vec<_>>()
                == package.scope_revision_ids,
        "Package history inventory differs from the immutable project bindings"
    );
    ensure!(
        names(root, &project, cancel)? == package.members.keys().cloned().collect(),
        "Package is missing a required retained member or contains an undeclared source file"
    );
    Ok((package, project))
}

/// Publish only after the owned staging directory has passed full verification.
/// Windows MoveFileEx without REPLACE_EXISTING also rejects a destination that
/// appeared after the preflight check, including an externally created empty dir.
fn publish_extracted(stage: &Path, destination: &Path, cancel: &AtomicBool) -> Result<()> {
    checkpoint(cancel)?;
    ensure!(
        !destination.try_exists()?,
        "Extraction destination appeared before publication; existing content was preserved"
    );
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};
        let from: Vec<u16> = stage.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        // SAFETY: null-terminated immutable Windows paths stay alive through
        // this call; no replace flag permits overwriting an existing target.
        let ok = unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) };
        ensure!(
            ok != 0,
            "Cannot publish verified extracted project: {}",
            std::io::Error::last_os_error()
        );
    }
    #[cfg(not(windows))]
    {
        // Native Windows is the release target. This fallback retains ordinary
        // destination preflight for unit-test hosts without Windows APIs.
        fs::rename(stage, destination)?;
    }
    Ok(())
}

/// Extract to an owned sibling staging directory; only a completely verified
/// candidate is renamed to the requested new destination. Failures/cancellation
/// leave no project manifest at that destination. No engine is needed.
pub fn extract(package: &Path, destination: &Path, cancel: Arc<AtomicBool>) -> Result<ProjectView> {
    checkpoint(&cancel)?;
    ensure!(
        !destination.try_exists()?,
        "Extraction destination must be new; existing content was preserved"
    );
    let absolute = std::path::absolute(destination)?;
    let parent = absolute
        .parent()
        .context("Extraction destination requires a parent")?;
    ensure!(
        absolute.file_name().is_some(),
        "Invalid extraction destination"
    );
    fs::create_dir_all(parent)?;
    let stage = parent.join(format!(
        ".greenlens-issue-intake-{}.partial",
        Uuid::new_v4()
    ));
    let actual = extract_checked(package, &stage, &cancel)?;
    validate_extracted(&stage, &actual, &cancel)?;
    let admitted = crate::project_open::admit_project(&stage)?;
    let view =
        crate::project_open::verify_admitted_project(&stage, &admitted, cancel.clone(), |_| {})?;
    publish_extracted(&stage, &absolute, &cancel)?;
    // ProjectView contains portable relative references and content receipts,
    // not a local root. No second numeric traversal is needed after the rename.
    Ok(view)
}

pub fn inspect(package: &Path, cancel: Arc<AtomicBool>) -> Result<Value> {
    let destination = crate::local_root()?
        .join("issued-project-inspection")
        .join(Uuid::new_v4().to_string());
    let view = extract(package, &destination, cancel.clone())?;
    checkpoint(&cancel)?;
    Ok(
        json!({"schema_version":INSPECTION_SCHEMA,"state":"completed","package_sha256":greenlens_process::file_hash(package)?,
        "project_id":view.project.id,"project_revision":view.revision,"verification":view.project_verification,
        "issue_ids":view.project.report_issues.as_ref().map(|i|i.issues.iter().map(|i|&i.id).collect::<Vec<_>>()),
        "scope_revision_ids":crate::study_revisions::revisions(&view.project).iter().map(|r|&r.revision_sha256).collect::<Vec<_>>(),
        "energy_runs":view.project.energy.as_ref().map_or(0,|e|e.runs.len()),"simulations_started":0,
        "inclusion_policy":INCLUSION_POLICY,"scientific_disclosure":"Package integrity and arithmetic rederivation do not confer scientific qualification; retained method assessments remain separate."}),
    )
}

/// Explicit integration replay of exactly one retained report's A/B. Imported
/// commands and other historical run lists cannot expand this execution set.
pub fn replay(
    package: &Path,
    issue_id: &str,
    engine_dir: Option<&Path>,
    cancel: Arc<AtomicBool>,
) -> Result<Value> {
    ensure!(
        is_hash(issue_id),
        "Select an explicit retained report issue identity"
    );
    let stage = crate::local_root()?
        .join("issued-project-replay")
        .join(Uuid::new_v4().to_string());
    let imported = stage.join("imported");
    let view = extract(package, &imported, cancel.clone())?;
    let issue = view
        .project
        .report_issues
        .as_ref()
        .and_then(|i| i.issues.iter().find(|i| i.id == issue_id))
        .context("Selected report issue is not retained in this package")?;
    let mut results = Vec::new();
    for binding in [&issue.baseline, &issue.proposed] {
        checkpoint(&cancel)?;
        let run = view
            .energy_results
            .iter()
            .find(|r| r.id == binding.run_id)
            .context("Verified issue run is missing")?;
        ensure!(
            canonical_hash(run)? == binding.record_sha256,
            "Selected issue run binding differs"
        );
        let output = stage.join(format!("replay-{}", run.id));
        fs::create_dir(&output)?;
        let repeated = crate::energy_job::run_expected(
            &Uuid::new_v4().to_string(),
            run.model.clone(),
            run.alternative_id.clone(),
            engine_dir,
            run.study.clone(),
            &imported.join("runs").join(&run.id).join("weather"),
            &output,
            &cancel,
            &|_| {},
            Some(&run.method),
        )?;
        ensure!(
            run.method == repeated.method,
            "Replay adapter changed the exact retained method"
        );
        let reference =
            EnergySeriesReader::open(&imported.join("runs").join(&run.id).join("series"))?;
        let candidate = EnergySeriesReader::open(&output.join("series"))?;
        results.push(json!({"run_id":run.id,"source_record_sha256":binding.record_sha256,
            "replay_record_sha256":canonical_hash(&repeated)?,"check":crate::energy_export::compare_series(&reference,&candidate,&cancel)?}));
    }
    checkpoint(&cancel)?;
    let result = json!({"schema_version":REPLAY_SCHEMA,"state":"completed","package_sha256":greenlens_process::file_hash(package)?,
        "issue_id":issue_id,"scope_revision_sha256":issue.scope_revision_sha256,"runs":results,
        "execution_policy":"Only the explicitly selected issue's baseline/proposed were replayed through recognized compiled EnergyPlus methods. Other retained evidence was inspected, not rerun.",
        "qualification":"Identical-recipe integration agreement only; no scientific qualification is inferred."});
    write_json(&stage.join("replay.json"), &result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    fn synthetic_envelope() -> IssuedProjectPackage {
        // Deliberately only archive metadata: this is never passed off as an
        // accepted engine record or complete report issue.
        let mut package = IssuedProjectPackage {
            schema_version: PACKAGE_SCHEMA.into(),
            content_sha256: String::new(),
            project_id: Uuid::new_v4().to_string(),
            project_file_sha256: "a".repeat(64),
            project_content_sha256: "b".repeat(64),
            issue_ids: vec!["c".repeat(64)],
            scope_revision_ids: vec!["d".repeat(64)],
            inclusion_policy: INCLUSION_POLICY.into(),
            members: BTreeMap::from([(
                "project.json".into(),
                PackageMember {
                    sha256: "a".repeat(64),
                    bytes: 1,
                },
            )]),
        };
        package.content_sha256 = package.identity().unwrap();
        package
    }
    #[test]
    fn metadata_rejects_unsafe_paths_duplicate_identities_and_shared_bounds() {
        let mut package = synthetic_envelope();
        package.validate().unwrap();
        for name in [
            "../outside",
            "C:/outside",
            "bad\\path",
            MANIFEST_NAME,
            RECIPE_NAME,
            "checksums.json",
        ] {
            package = synthetic_envelope();
            package.members.insert(
                name.into(),
                PackageMember {
                    sha256: "e".repeat(64),
                    bytes: 1,
                },
            );
            package.content_sha256 = package.identity().unwrap();
            assert!(package.validate().is_err(), "{name}");
        }
        package = synthetic_envelope();
        package.issue_ids.push(package.issue_ids[0].clone());
        package.content_sha256 = package.identity().unwrap();
        assert!(package.validate().is_err());
        package = synthetic_envelope();
        package.members.get_mut("project.json").unwrap().bytes = MAX_EVIDENCE_FILE_BYTES + 1;
        package.content_sha256 = package.identity().unwrap();
        assert!(package.validate().is_err());
        package = synthetic_envelope();
        for index in 0..4 {
            package.members.insert(
                format!("large-{index}.bin"),
                PackageMember {
                    sha256: "e".repeat(64),
                    bytes: MAX_EVIDENCE_FILE_BYTES,
                },
            );
        }
        package.content_sha256 = package.identity().unwrap();
        assert!(package.validate().is_err());
        package = synthetic_envelope();
        for index in 0..MAX_PACKAGE_ENTRIES - 3 {
            package.members.insert(
                format!("tiny-{index}.bin"),
                PackageMember {
                    sha256: "e".repeat(64),
                    bytes: 0,
                },
            );
        }
        package.content_sha256 = package.identity().unwrap();
        assert!(package.validate().is_err());
    }
    #[test]
    fn envelope_overhead_is_preflighted_and_same_size_stage_tampering_fails() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("project.json"), b"x").unwrap();
        let mut package = synthetic_envelope();
        package.project_file_sha256 = bytes_hash(b"x");
        package.members.get_mut("project.json").unwrap().sha256 =
            package.project_file_sha256.clone();
        package.content_sha256 = package.identity().unwrap();
        let stage = temp.path().join("stage");
        let cancel = AtomicBool::new(false);
        let members = stage_pinned(&source, &stage, &package, &cancel).unwrap();
        assert_eq!(members.len(), 4);
        fs::write(stage.join("project.json"), b"y").unwrap();
        let destination = temp.path().join("rejected.zip");
        assert!(write_zip(&stage, &members, &destination, &cancel).is_err());
        assert!(!destination.exists());
        package.members.get_mut("project.json").unwrap().bytes = MAX_EVIDENCE_FILE_BYTES;
        for index in 0..3 {
            package.members.insert(
                format!("large-{index}.bin"),
                PackageMember {
                    sha256: "e".repeat(64),
                    bytes: MAX_EVIDENCE_FILE_BYTES,
                },
            );
        }
        package.content_sha256 = package.identity().unwrap();
        package.validate().unwrap();
        assert!(envelope(&package).is_err());
    }
    #[test]
    fn cancellation_never_publishes_a_package_or_adopts_metadata_as_engine_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("cancelled.zip");
        assert!(write_zip(
            temp.path(),
            &BTreeMap::new(),
            &destination,
            &AtomicBool::new(true)
        )
        .is_err());
        assert!(!destination.exists());
        let package = synthetic_envelope();
        assert!(
            validate_extracted(temp.path(), &BTreeMap::new(), &AtomicBool::new(false)).is_err()
        );
        let mut unknown = package;
        unknown.schema_version = "unrecognized executable recipe".into();
        unknown.content_sha256 = unknown.identity().unwrap();
        assert!(unknown.validate().is_err());
    }
    #[test]
    fn invalid_or_cancelled_extraction_never_publishes_a_project_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        // Intentionally not a project or accepted scientific record. It has a
        // consistent archive seal so failure occurs after member extraction.
        fs::write(source.join("project.json"), b"x").unwrap();
        let mut package = synthetic_envelope();
        package.project_file_sha256 = bytes_hash(b"x");
        package.members.get_mut("project.json").unwrap().sha256 =
            package.project_file_sha256.clone();
        package.content_sha256 = package.identity().unwrap();
        let stage = temp.path().join("archive-stage");
        let active = AtomicBool::new(false);
        let members = stage_pinned(&source, &stage, &package, &active).unwrap();
        let archive = temp.path().join("invalid-project.zip");
        write_zip(&stage, &members, &archive, &active).unwrap();
        let rejected = temp.path().join("requested-project");
        assert!(extract(&archive, &rejected, Arc::new(AtomicBool::new(false))).is_err());
        assert!(!rejected.exists());
        assert!(!rejected.join("project.json").exists());
        let cancelled = temp.path().join("cancelled-project");
        assert!(extract(&archive, &cancelled, Arc::new(AtomicBool::new(true))).is_err());
        assert!(!cancelled.exists());

        let publication_stage = temp.path().join("publication-stage");
        fs::create_dir(&publication_stage).unwrap();
        fs::write(
            publication_stage.join("project.json"),
            b"publication fixture",
        )
        .unwrap();
        let late_cancel = AtomicBool::new(true);
        assert!(publish_extracted(&publication_stage, &cancelled, &late_cancel).is_err());
        assert!(!cancelled.exists());
        assert!(publication_stage.exists());
        fs::create_dir(&rejected).unwrap();
        fs::write(rejected.join("existing.txt"), b"unrelated existing bytes").unwrap();
        assert!(publish_extracted(&publication_stage, &rejected, &active).is_err());
        assert_eq!(
            fs::read(rejected.join("existing.txt")).unwrap(),
            b"unrelated existing bytes"
        );
        assert!(!rejected.join("project.json").exists());
        assert!(publication_stage.exists());
    }
    #[test]
    #[ignore = "Requires a genuine issued project in GREENLENS_TEST_ISSUED_PROJECT; no engine or report rendering is performed"]
    fn genuine_external_history_copy_export_and_cold_extract_preserve_all_evidence() {
        let started = Instant::now();
        let source = PathBuf::from(
            std::env::var("GREENLENS_TEST_ISSUED_PROJECT")
                .expect("Provide the declared genuine retained issued project"),
        );
        let active = AtomicBool::new(false);
        let original = crate::open_project(&source).unwrap();
        let original_manifest = fs::read(source.join("project.json")).unwrap();
        let original_members: BTreeMap<_, _> = names(&source, &original.project, &active)
            .unwrap()
            .into_iter()
            .map(|name| {
                let hash = evidence_file_hash(&regular_child(&source, &name).unwrap()).unwrap();
                (name, hash)
            })
            .collect();
        let original_bytes: u64 = original_members
            .keys()
            .map(|name| {
                fs::metadata(regular_child(&source, name).unwrap())
                    .unwrap()
                    .len()
            })
            .sum();
        let source_verification_seconds = started.elapsed().as_secs_f64();
        let mut candidate = original.project.clone();
        let history = candidate
            .architecture
            .as_mut()
            .and_then(|a| a.scope_history.as_mut())
            .expect("The genuine fixture must contain retained scope history");
        assert!(!history.revisions.is_empty());
        history.schema_version = crate::history_storage::EXTERNAL_HISTORY_SCHEMA.into();
        let expected_revisions: Vec<_> = history
            .revisions
            .iter()
            .map(|revision| revision.revision_sha256.clone())
            .collect();
        let parent = crate::local_root()
            .unwrap()
            .join("verification/external-history-portability");
        fs::create_dir_all(&parent).unwrap();
        let temporary = tempfile::Builder::new()
            .prefix("issued history ")
            .tempdir_in(parent)
            .unwrap();
        let migrated_root = temporary.path().join("complete migrated copy 空間");
        let copy_started = Instant::now();
        let migrated = crate::save_project_as(
            Some(&source),
            &migrated_root,
            &candidate,
            Some(&original.revision),
        )
        .unwrap();
        let copy_seconds = copy_started.elapsed().as_secs_f64();
        assert_eq!(
            canonical_hash(&migrated.project).unwrap(),
            canonical_hash(&candidate).unwrap()
        );
        assert!(
            fs::metadata(migrated_root.join("project.json"))
                .unwrap()
                .len()
                <= crate::study_revisions::MAX_REVISION_PROJECT_BYTES as u64
        );
        let history_entries = crate::history_storage::entries(&migrated.project).unwrap();
        assert_eq!(history_entries.len(), expected_revisions.len());
        for (entry, revision) in history_entries.iter().zip(expected_revisions) {
            assert_eq!(entry.revision_sha256, revision);
            assert_eq!(
                evidence_file_hash(&regular_child(&migrated_root, &entry.path).unwrap()).unwrap(),
                entry.sha256
            );
        }
        for (name, hash) in &original_members {
            assert_eq!(
                evidence_file_hash(&regular_child(&source, name).unwrap()).unwrap(),
                *hash,
                "The source must remain unchanged: {name}"
            );
            if name != "project.json" {
                assert_eq!(
                    evidence_file_hash(&regular_child(&migrated_root, name).unwrap()).unwrap(),
                    *hash,
                    "Migration must retain the exact original evidence bytes: {name}"
                );
            }
        }
        assert_eq!(
            fs::read(source.join("project.json")).unwrap(),
            original_manifest
        );
        let archive = temporary.path().join("complete history.zip");
        let export_started = Instant::now();
        let exported = export(
            &migrated_root,
            &migrated.project,
            &archive,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        let export_seconds = export_started.elapsed().as_secs_f64();
        let cold_root = temporary.path().join("cold retained project 空間");
        let cold_started = Instant::now();
        let cold = extract(&archive, &cold_root, Arc::new(AtomicBool::new(false))).unwrap();
        let cold_extract_seconds = cold_started.elapsed().as_secs_f64();
        assert_eq!(
            canonical_hash(&cold.project).unwrap(),
            canonical_hash(&migrated.project).unwrap()
        );
        let copied_members = names(&migrated_root, &migrated.project, &active).unwrap();
        let mut copied_bytes = 0u64;
        let mut cold_retained_bytes = 0u64;
        for name in &copied_members {
            copied_bytes += fs::metadata(regular_child(&migrated_root, name).unwrap())
                .unwrap()
                .len();
            cold_retained_bytes += fs::metadata(regular_child(&cold_root, name).unwrap())
                .unwrap()
                .len();
            assert_eq!(
                evidence_file_hash(&regular_child(&migrated_root, name).unwrap()).unwrap(),
                evidence_file_hash(&regular_child(&cold_root, name).unwrap()).unwrap(),
                "Complete export/extraction must preserve every exact member: {name}"
            );
        }
        assert_eq!(
            cold.project.report_issues.as_ref().unwrap().issues,
            original.project.report_issues.as_ref().unwrap().issues
        );
        let measurement = json!({
            "schema_version":"greenlens.external-history-portability-check.v1",
            "completed":true,
            "wall_seconds":started.elapsed().as_secs_f64(),
            "source_verification_seconds":source_verification_seconds,
            "copy_seconds":copy_seconds,
            "export_seconds":export_seconds,
            "cold_extract_seconds":cold_extract_seconds,
            "old_manifest_bytes":original_manifest.len(),
            "new_manifest_bytes":fs::metadata(migrated_root.join("project.json")).unwrap().len(),
            "history_sidecar_count":history_entries.len(),
            "history_sidecar_bytes":history_entries.iter().map(|entry|entry.bytes).sum::<u64>(),
            "original_retained_members":original_members.len(),
            "original_retained_bytes":original_bytes,
            "copied_retained_members":copied_members.len(),
            "copied_retained_bytes":copied_bytes,
            "cold_retained_members":copied_members.len(),
            "cold_retained_bytes":cold_retained_bytes,
            "export_members":exported["entries"],
            "export_uncompressed_bytes":exported["uncompressed_bytes"],
            "export_zip_bytes":exported["package_bytes"],
            "all_prior_nonmanifest_members_unchanged":true,
            "source_manifest_unchanged":true,
            "engine_or_renderer_execution_requested":false,
            "scope":"Storage portability and actual-byte preservation only; no native UI or scientific qualification claim. Retained-member totals exclude package envelope files."
        });
        let measurement = serde_json::to_string(&measurement).unwrap();
        assert!(
            measurement.len() <= 4096,
            "Measurement receipt must remain bounded"
        );
        println!("{measurement}");
    }
    #[test]
    #[ignore = "Requires a saved genuine issued project in GREENLENS_TEST_ISSUED_PROJECT; never fabricates or runs energy evidence"]
    fn genuine_complete_issued_project_roundtrip_preserves_every_member_without_engines() {
        let source = PathBuf::from(
            std::env::var("GREENLENS_TEST_ISSUED_PROJECT")
                .expect("Provide the declared genuine retained issued project"),
        );
        let parent = crate::local_root()
            .unwrap()
            .join("verification/issued-project-portability");
        fs::create_dir_all(&parent).unwrap();
        let temp = tempfile::Builder::new()
            .prefix("retained issued evidence ")
            .tempdir_in(parent)
            .unwrap();
        let view = crate::open_project(&source).unwrap();
        // The desktop sends the accepted Project back through JSON.stringify:
        // integral f64 tokens become integer tokens. Exercise the same query
        // decoder and exact history/export comparisons with that wire shape.
        fn javascript_integer_tokens(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Number(number) if number.is_f64() => {
                    let n = number.as_f64().unwrap();
                    if n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_991.0 {
                        *number = serde_json::Number::from(n as i64);
                    }
                }
                serde_json::Value::Array(rows) => {
                    rows.iter_mut().for_each(javascript_integer_tokens)
                }
                serde_json::Value::Object(rows) => {
                    rows.values_mut().for_each(javascript_integer_tokens)
                }
                _ => {}
            }
        }
        let mut wire = serde_json::to_value(&view.project).unwrap();
        javascript_integer_tokens(&mut wire);
        let client: Project = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(
            canonical_hash(&client).unwrap(),
            canonical_hash(&view.project).unwrap()
        );
        for issue in &client.report_issues.as_ref().unwrap().issues {
            let query: crate::energy_inspection::EnergyEvidenceQuery = serde_json::from_value(
                serde_json::json!({"command":"report_issue_inspect","arguments":{
                    "path":source.to_str().unwrap(),"project":wire,"id":issue.id
                }}),
            )
            .unwrap();
            let inspected = query
                .execute(Arc::new(AtomicBool::new(false)), |_| {})
                .unwrap();
            assert_eq!(inspected["summary"]["id"], issue.id);
            assert_eq!(
                inspected["summary"]["scope_revision_sha256"],
                issue.scope_revision_sha256
            );
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let package = temp.path().join("complete.zip");
        export(&source, &client, &package, cancel.clone()).unwrap();
        let copy = temp.path().join("portable");
        let reopened = extract(&package, &copy, cancel).unwrap();
        assert_eq!(
            canonical_hash(&reopened.project).unwrap(),
            canonical_hash(&view.project).unwrap()
        );
        for name in names(&source, &view.project, &AtomicBool::new(false)).unwrap() {
            assert_eq!(
                fs::read(regular_child(&source, &name).unwrap()).unwrap(),
                fs::read(regular_child(&copy, &name).unwrap()).unwrap(),
                "{name}"
            );
        }
        assert!(
            reopened
                .project
                .report_issues
                .as_ref()
                .unwrap()
                .issues
                .len()
                >= 2,
            "Finite S1/R1→S2/R2 fixture required"
        );
    }
}
