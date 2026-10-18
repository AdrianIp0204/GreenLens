//! Complete portable projects with additive analytical design reports. No
//! artificial architectural scope is required. The old issued-project codec
//! and recipes are unchanged; ZIP/path/limit enforcement is shared with it.
use crate::{
    persistence::*,
    report_issue_export::{self as archive, PackageMember},
    Project, ProjectView,
};
use anyhow::{ensure, Context, Result};
use greenlens_core::canonical_hash;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use uuid::Uuid;

pub const PACKAGE_SCHEMA: &str = "greenlens.design-project-export.v1";
pub const INSPECTION_SCHEMA: &str = "greenlens.design-project-inspection.v1";
pub const REPLAY_SCHEMA: &str = "greenlens.design-project-replay.v1";
const MANIFEST: &str = "design-project-export.json";
const RECIPE: &str = "REPLAY.txt";
const METADATA_CAP: u64 = 4 * 1024 * 1024;
const POLICY: &str = "Complete retained project: all scientific runs, weather, accepted scope/revision history, old report issues, analytical design reports, required normalized reference assets, sensitivity receipts and explicitly included attachments. No retained history is pruned. Original external proprietary source files, engines, local renderer paths and active scratch are not included.";
const REPLAY_TEXT: &str = "GreenLens complete analytical design-project evidence\n\nThis is the complete retained GreenLens project, including all accepted old/new history and normalized reference assets when present. It is not an archive of external proprietary architectural sources. Original input paths, engines and active caches are unnecessary for inspection. All results keep their original execution, integrity, input-relationship and scientific status.\n\nUse the trusted GreenLens design-project reader. It validates paths, entry/file/aggregate bounds, exact member hashes, project history, complete retained scientific evidence and immutable product bindings. Embedded JSON and this recipe are inert data; no imported commands are executed.\n\nInspect selected design reports offline, or regenerate into a new output directory with their exact recognized owned renderer/template/font identity and separately installed pinned Typst/fonts. Regeneration starts no simulation. Existing products stay usable without Typst.\n\nReplay is explicit: select exactly one design-report ID. Only its bound baseline/proposal runs are repeated through the compiled recognized EnergyPlus adapter and separately installed compatible engine. All channels must agree within max(1e-6 in the reported unit, 0.1% of reference), with identical dimensions, labels and timestamps. Other retained history is inspected, not rerun. Replay equality is integration verification, not physical validation or automatic qualification.\n";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DesignProjectPackage {
    pub schema_version: String,
    pub content_sha256: String,
    pub project_id: String,
    pub project_file_sha256: String,
    pub project_content_sha256: String,
    pub design_report_ids: Vec<String>,
    pub report_issue_ids: Vec<String>,
    pub scope_revision_ids: Vec<String>,
    pub inclusion_policy: String,
    pub members: BTreeMap<String, PackageMember>,
}
impl DesignProjectPackage {
    fn identity(&self) -> Result<String> {
        let mut value = self.clone();
        value.content_sha256.clear();
        Ok(canonical_hash(&(PACKAGE_SCHEMA, value))?)
    }
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == PACKAGE_SCHEMA && self.inclusion_policy == POLICY,
            "Unsupported design-project package or inclusion policy"
        );
        ensure!(
            Uuid::parse_str(&self.project_id)?.to_string() == self.project_id,
            "Invalid package project identity"
        );
        for hash in [
            &self.content_sha256,
            &self.project_file_sha256,
            &self.project_content_sha256,
        ] {
            ensure!(digest(hash), "Invalid package content digest");
        }
        ensure!(
            !self.design_report_ids.is_empty()
                && self.design_report_ids.len() <= crate::design_report::MAX_REPORTS,
            "Choose one to 32 retained design reports"
        );
        ensure!(
            self.report_issue_ids.len() <= crate::report_issues::MAX_ISSUES
                && self.scope_revision_ids.len()
                    <= greenlens_core::study_revision::MAX_SCOPE_REVISIONS,
            "History exceeds supported capacity"
        );
        for ids in [
            &self.design_report_ids,
            &self.report_issue_ids,
            &self.scope_revision_ids,
        ] {
            ensure!(
                ids.iter().all(|id| digest(id))
                    && ids.iter().collect::<BTreeSet<_>>().len() == ids.len(),
                "Invalid or duplicate historical identities"
            );
        }
        ensure!(
            self.members.len() + 3 <= MAX_PACKAGE_ENTRIES,
            "Complete package exceeds shared entry limit; no partial export or history pruning"
        );
        let mut budget = PackageBudget::default();
        for (name, member) in &self.members {
            ensure!(
                ![MANIFEST, RECIPE, "checksums.json"].contains(&name.as_str())
                    && digest(&member.sha256),
                "Invalid self-referencing package member"
            );
            budget.add(name, member.bytes)?;
        }
        ensure!(
            self.members
                .get("project.json")
                .is_some_and(|m| m.sha256 == self.project_file_sha256),
            "Package must bind exact saved project bytes"
        );
        ensure!(
            self.content_sha256 == self.identity()?,
            "Design package identity differs"
        );
        ensure!(
            serde_json::to_vec(self)?.len() as u64 <= METADATA_CAP,
            "Package inventory exceeds 4 MiB"
        );
        Ok(())
    }
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn checkpoint(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::SeqCst),
        "Design-project operation cancelled; no completed package published"
    );
    crate::project_open::checkpoint(0)
}
fn io_checkpoint(cancel: &AtomicBool, started: Instant) -> Result<()> {
    checkpoint(cancel)?;
    ensure!(
        started.elapsed() < Duration::from_secs(crate::project_open::VERIFICATION_DEADLINE_SECONDS),
        "Design-package I/O exceeded its 120-second stage budget"
    );
    Ok(())
}
/// The receipt describes these held bytes, not a later read of a mutable path.
/// Windows sharing denies writes and replacement throughout inspection/replay.
/// Other platforms additionally require exact path-byte fences around extraction.
/// Hashing streams a fixed opened length with the existing I/O deadline: no ZIP
/// contents or compressed archive are loaded as one allocation, and no new or
/// relaxed compressed/expanded package allowance is introduced.
struct HeldArchive {
    file: File,
    path: PathBuf,
    sha256: String,
    bytes: u64,
    _parent: DirectoryGuard,
}
impl HeldArchive {
    fn open(path: &Path, cancel: &AtomicBool) -> Result<Self> {
        checkpoint(cancel)?;
        let absolute = std::path::absolute(path)?;
        let parent = absolute
            .parent()
            .context("Archive needs a parent directory")?;
        let guard = DirectoryGuard::open(parent)?;
        let name = absolute
            .file_name()
            .and_then(|s| s.to_str())
            .context("Invalid archive filename")?;
        let path = regular_child(parent, name)?;
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
            };
            options
                .share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        }
        let mut file = options.open(&path)?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "Archive must be a regular held file"
        );
        let bytes = metadata.len();
        let sha256 = hash_archive(&mut file, bytes, cancel)?;
        Ok(Self {
            file,
            path,
            sha256,
            bytes,
            _parent: guard,
        })
    }
    fn verify_path(&mut self, cancel: &AtomicBool) -> Result<()> {
        ensure!(
            hash_archive(&mut self.file, self.bytes, cancel)? == self.sha256,
            "Held archive bytes changed during intake"
        );
        let current = Self::open(&self.path, cancel)?;
        ensure!(
            current.bytes == self.bytes && current.sha256 == self.sha256,
            "Selected archive path changed during intake; no extracted project is accepted"
        );
        Ok(())
    }
}
fn hash_archive(file: &mut File, expected_bytes: u64, cancel: &AtomicBool) -> Result<String> {
    ensure!(
        file.metadata()?.len() == expected_bytes,
        "Archive length changed during intake"
    );
    file.seek(SeekFrom::Start(0))?;
    let started = Instant::now();
    let mut total = 0u64;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        io_checkpoint(cancel, started)?;
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count as u64)
            .context("Archive byte-count overflow")?;
        ensure!(
            total <= expected_bytes,
            "Archive grew while its identity was read"
        );
        hash.update(&buffer[..count]);
    }
    ensure!(
        total == expected_bytes && file.metadata()?.len() == expected_bytes,
        "Archive length changed during intake"
    );
    io_checkpoint(cancel, started)?;
    Ok(format!("{:x}", hash.finalize()))
}
fn read(root: &Path, name: &str, cap: u64, cancel: &AtomicBool) -> Result<Vec<u8>> {
    read_evidence_bytes_checked(&regular_child(root, name)?, cap, &mut |n| {
        checkpoint(cancel)?;
        crate::project_open::checkpoint(n)
    })
}
fn json_bytes(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}
fn privacy(project: &Project) -> Result<()> {
    ensure!(
        project
            .design_reports
            .as_ref()
            .is_some_and(|index| !index.reports.is_empty()),
        "Issue an analytical design report before complete export"
    );
    ensure!(project.energy.as_ref().is_none_or(|energy|energy.attachments.iter().all(|a|a.include_in_export)),"The complete project contains excluded attachments. Review inclusion explicitly before export; no private metadata or files will be silently included, and no history is silently omitted");
    Ok(())
}
fn names(root: &Path, project: &Project, cancel: &AtomicBool) -> Result<BTreeSet<String>> {
    let mut result = archive::names(root, project, cancel)?;
    for name in crate::design_report::evidence_members(root, project)? {
        checkpoint(cancel)?;
        ensure!(
            result.insert(name),
            "Design-report paths overlap retained evidence"
        );
    }
    let mut budget = PackageBudget::default();
    for name in &result {
        checkpoint(cancel)?;
        budget.add(name, fs::metadata(regular_child(root, name)?)?.len())?;
    }
    Ok(result)
}
fn bind(
    project: &Project,
    members: BTreeMap<String, PackageMember>,
) -> Result<DesignProjectPackage> {
    let mut package = DesignProjectPackage {
        schema_version: PACKAGE_SCHEMA.into(),
        content_sha256: String::new(),
        project_id: project.id.clone(),
        project_file_sha256: members
            .get("project.json")
            .context("Project file missing")?
            .sha256
            .clone(),
        project_content_sha256: canonical_hash(project)?,
        design_report_ids: project
            .design_reports
            .as_ref()
            .context("Design-report history missing")?
            .reports
            .iter()
            .map(|r| r.id.clone())
            .collect(),
        report_issue_ids: project
            .report_issues
            .as_ref()
            .map(|i| i.issues.iter().map(|r| r.id.clone()).collect())
            .unwrap_or_default(),
        scope_revision_ids: crate::study_revisions::revisions(project)
            .iter()
            .map(|r| r.revision_sha256.clone())
            .collect(),
        inclusion_policy: POLICY.into(),
        members,
    };
    package.content_sha256 = package.identity()?;
    package.validate()?;
    Ok(package)
}
struct Envelope {
    manifest: Vec<u8>,
    seal: Vec<u8>,
    members: BTreeMap<String, PackageMember>,
}
fn envelope(package: &DesignProjectPackage) -> Result<Envelope> {
    package.validate()?;
    let manifest = json_bytes(package)?;
    ensure!(
        manifest.len() as u64 <= METADATA_CAP,
        "Pretty package manifest exceeds 4 MiB"
    );
    let mut members = package.members.clone();
    members.insert(
        MANIFEST.into(),
        PackageMember {
            sha256: bytes_hash(&manifest),
            bytes: manifest.len() as u64,
        },
    );
    members.insert(
        RECIPE.into(),
        PackageMember {
            sha256: bytes_hash(REPLAY_TEXT.as_bytes()),
            bytes: REPLAY_TEXT.len() as u64,
        },
    );
    let seal = json_bytes(&Checksums {
        schema_version: "0.1.0".into(),
        sha256: members
            .iter()
            .map(|(name, value)| (name.clone(), value.sha256.clone()))
            .collect(),
    })?;
    ensure!(
        seal.len() as u64 <= METADATA_CAP,
        "Seal exceeds metadata bound"
    );
    members.insert(
        "checksums.json".into(),
        PackageMember {
            sha256: bytes_hash(&seal),
            bytes: seal.len() as u64,
        },
    );
    let mut budget = PackageBudget::default();
    for (name, value) in &members {
        budget.add(name, value.bytes)?;
    }
    Ok(Envelope {
        manifest,
        seal,
        members,
    })
}
fn stage(
    root: &Path,
    destination: &Path,
    package: &DesignProjectPackage,
    cancel: &AtomicBool,
) -> Result<BTreeMap<String, PackageMember>> {
    let envelope = envelope(package)?;
    ensure!(!destination.exists(), "Package staging must be fresh");
    fs::create_dir_all(destination)?;
    let started = Instant::now();
    for (name, value) in &package.members {
        io_checkpoint(cancel, started)?;
        let bytes = read(root, name, value.bytes, cancel)?;
        ensure!(
            bytes.len() as u64 == value.bytes && bytes_hash(&bytes) == value.sha256,
            "Retained source changed during export: {name}"
        );
        write_new_evidence_bytes(destination, name, &bytes)?;
    }
    write_new_evidence_bytes(destination, MANIFEST, &envelope.manifest)?;
    write_new_evidence_bytes(destination, RECIPE, REPLAY_TEXT.as_bytes())?;
    write_new_evidence_bytes(destination, "checksums.json", &envelope.seal)?;
    io_checkpoint(cancel, started)?;
    Ok(envelope.members)
}
pub fn export(
    root: &Path,
    project: &Project,
    destination: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<Value> {
    checkpoint(&cancel)?;
    privacy(project)?;
    crate::validate_project(project)?;
    ensure!(!destination.exists(), "Export destination already exists");
    let _lock = WriterLock::acquire(root)?;
    let stored = crate::history_storage::read_project(root, &cancel)?;
    ensure!(
        canonical_hash(&stored)? == canonical_hash(project)?,
        "Save the complete accepted document before exporting; current and saved history differ"
    );
    let admitted = crate::project_open::admit_project(root)?;
    let view =
        crate::project_open::verify_admitted_project(root, &admitted, cancel.clone(), |_| {})?;
    let snapshot = view
        .project_verification
        .as_ref()
        .context("Complete source verification missing")?
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
            ensure!(
                canonical_hash(&actual)? == snapshot,
                "Complete member inventory differs from the verified source snapshot"
            );
            check_revision(root, Some(&view.revision))?;
            bind(&view.project, members)
        },
    )?;
    let full = envelope(&package)?;
    let bytes: u64 = full.members.values().map(|m| m.bytes).sum();
    let staging = crate::local_root()?
        .join("design-project-exports")
        .join(Uuid::new_v4().to_string());
    let members = stage(root, &staging, &package, &cancel)?;
    checkpoint(&cancel)?;
    check_revision(root, Some(&view.revision))?;
    archive::write_zip(&staging, &members, destination, &cancel)?;
    Ok(
        json!({"schema_version":PACKAGE_SCHEMA,"state":"completed","package_content_sha256":package.content_sha256,"package_sha256":greenlens_process::file_hash(destination)?,"package_bytes":fs::metadata(destination)?.len(),"uncompressed_bytes":bytes,"entries":members.len(),"design_report_ids":package.design_report_ids,"report_issue_ids":package.report_issue_ids,"scope_revision_ids":package.scope_revision_ids,"inclusion_policy":POLICY}),
    )
}
fn validate_extracted(
    root: &Path,
    actual: &BTreeMap<String, PackageMember>,
    cancel: &AtomicBool,
) -> Result<(DesignProjectPackage, Project)> {
    let package: DesignProjectPackage =
        serde_json::from_slice(&read(root, MANIFEST, METADATA_CAP, cancel)?)?;
    package.validate()?;
    let expected = envelope(&package)?;
    ensure!(
        expected.members == *actual,
        "Package has missing, changed or undeclared members"
    );
    ensure!(
        read(root, MANIFEST, METADATA_CAP, cancel)? == expected.manifest
            && read(root, "checksums.json", METADATA_CAP, cancel)? == expected.seal,
        "Package envelope serialization differs"
    );
    let bytes = read(root, "project.json", MAX_EVIDENCE_FILE_BYTES, cancel)?;
    ensure!(
        bytes_hash(&bytes) == package.project_file_sha256,
        "Saved manifest bytes differ"
    );
    let project = crate::history_storage::decode_project(root, &bytes, cancel)?;
    crate::validate_project(&project)?;
    privacy(&project)?;
    let rebound = bind(&project, package.members.clone())?;
    ensure!(
        rebound == package,
        "Package project/history identities differ"
    );
    ensure!(
        names(root, &project, cancel)? == package.members.keys().cloned().collect(),
        "Missing required evidence or undeclared source member"
    );
    Ok((package, project))
}
pub fn extract(package: &Path, destination: &Path, cancel: Arc<AtomicBool>) -> Result<ProjectView> {
    let mut package = HeldArchive::open(package, &cancel)?;
    extract_held(&mut package, destination, cancel)
}
fn extract_held(
    package: &mut HeldArchive,
    destination: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<ProjectView> {
    checkpoint(&cancel)?;
    ensure!(
        !destination.try_exists()?,
        "Extraction destination must be new"
    );
    let absolute = std::path::absolute(destination)?;
    let parent = absolute
        .parent()
        .context("Extraction needs a parent directory")?;
    ensure!(absolute.file_name().is_some(), "Invalid destination");
    fs::create_dir_all(parent)?;
    let staging = parent.join(format!(
        ".greenlens-design-intake-{}.partial",
        Uuid::new_v4()
    ));
    package.verify_path(&cancel)?;
    let actual = archive::extract_checked(&package.path, &staging, &cancel)?;
    package.verify_path(&cancel)?;
    validate_extracted(&staging, &actual, &cancel)?;
    let admitted = crate::project_open::admit_project(&staging)?;
    let view =
        crate::project_open::verify_admitted_project(&staging, &admitted, cancel.clone(), |_| {})?;
    archive::publish_extracted(&staging, &absolute, &cancel)?;
    Ok(view)
}
pub fn inspect(package: &Path, cancel: Arc<AtomicBool>) -> Result<Value> {
    let mut package = HeldArchive::open(package, &cancel)?;
    let destination = crate::local_root()?
        .join("design-project-inspection")
        .join(Uuid::new_v4().to_string());
    let view = extract_held(&mut package, &destination, cancel.clone())?;
    checkpoint(&cancel)?;
    Ok(
        json!({"schema_version":INSPECTION_SCHEMA,"state":"completed","package_sha256":package.sha256,"package_bytes":package.bytes,"project_id":view.project.id,"project_revision":view.revision,"verification":view.project_verification,"design_report_ids":view.project.design_reports.as_ref().map(|i|i.reports.iter().map(|r|&r.id).collect::<Vec<_>>()),"energy_runs":view.project.energy.as_ref().map_or(0,|e|e.runs.len()),"simulations_started":0,"inclusion_policy":POLICY,"scientific_disclosure":"Complete retained evidence inspection and arithmetic rederivation, not physical validation or qualification."}),
    )
}
pub fn replay(
    package: &Path,
    report_id: &str,
    engine_dir: Option<&Path>,
    cancel: Arc<AtomicBool>,
) -> Result<Value> {
    ensure!(
        digest(report_id),
        "Select an explicit immutable design-report ID"
    );
    let mut package = HeldArchive::open(package, &cancel)?;
    let staging = crate::local_root()?
        .join("design-project-replay")
        .join(Uuid::new_v4().to_string());
    let imported = staging.join("imported");
    let view = extract_held(&mut package, &imported, cancel.clone())?;
    let report = view
        .project
        .design_reports
        .as_ref()
        .and_then(|i| i.reports.iter().find(|r| r.id == report_id))
        .context("Selected design report is not retained")?;
    let mut results = vec![];
    for binding in [&report.baseline, &report.proposed] {
        checkpoint(&cancel)?;
        let run = view
            .energy_results
            .iter()
            .find(|r| r.id == binding.run_id)
            .context("Verified selected run missing")?;
        ensure!(
            canonical_hash(run)? == binding.record_sha256,
            "Selected report record binding differs"
        );
        let output = staging.join(format!("replay-{}", run.id));
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
            "Replay method differs from exact retained identity"
        );
        let reference = crate::energy_series::EnergySeriesReader::open(
            &imported.join("runs").join(&run.id).join("series"),
        )?;
        let candidate = crate::energy_series::EnergySeriesReader::open(&output.join("series"))?;
        results.push(json!({"run_id":run.id,"source_record_sha256":binding.record_sha256,"replay_record_sha256":canonical_hash(&repeated)?,"check":crate::energy_export::compare_series(&reference,&candidate,&cancel)?}));
    }
    checkpoint(&cancel)?;
    let result = json!({"schema_version":REPLAY_SCHEMA,"state":"completed","package_sha256":package.sha256,"package_bytes":package.bytes,"design_report_id":report_id,"runs":results,"execution_policy":"Only the explicitly selected report's exact A/B were replayed through recognized compiled methods; other retained history was inspected, not rerun.","qualification":"Integration agreement only; no scientific qualification is inferred."});
    write_json(&staging.join("replay.json"), &result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn metadata() -> DesignProjectPackage {
        // Archive metadata fixture only. It cannot pass full project/run intake.
        let mut p = DesignProjectPackage {
            schema_version: PACKAGE_SCHEMA.into(),
            content_sha256: String::new(),
            project_id: Uuid::new_v4().to_string(),
            project_file_sha256: bytes_hash(b"x"),
            project_content_sha256: "b".repeat(64),
            design_report_ids: vec!["c".repeat(64)],
            report_issue_ids: vec![],
            scope_revision_ids: vec![],
            inclusion_policy: POLICY.into(),
            members: BTreeMap::from([(
                "project.json".into(),
                PackageMember {
                    sha256: bytes_hash(b"x"),
                    bytes: 1,
                },
            )]),
        };
        p.content_sha256 = p.identity().unwrap();
        p
    }
    #[test]
    fn archive_identity_is_held_across_extraction_and_later_path_replacement() {
        // A sealed metadata-only ZIP exercises actual archive I/O, not accepted
        // simulation records. No engine or report renderer is involved.
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("project.json"), b"x").unwrap();
        let cancel = AtomicBool::new(false);
        let staged = temp.path().join("staged");
        let members = stage(&source, &staged, &metadata(), &cancel).unwrap();
        let path = temp.path().join("complete evidence 空間.zip");
        archive::write_zip(&staged, &members, &path, &cancel).unwrap();
        let original = fs::read(&path).unwrap();
        let mut held = HeldArchive::open(&path, &cancel).unwrap();
        let extracted = temp.path().join("extracted");
        held.verify_path(&cancel).unwrap();
        assert_eq!(
            archive::extract_checked(&held.path, &extracted, &cancel).unwrap(),
            members
        );
        held.verify_path(&cancel).unwrap();
        assert_eq!(held.sha256, bytes_hash(&original));
        let moved = temp.path().join("earlier.zip");
        let replacement = fs::rename(&path, &moved);
        #[cfg(windows)]
        {
            assert!(
                replacement.is_err(),
                "The held Windows input must deny replacement"
            );
            assert!(
                fs::write(&path, b"different archive bytes").is_err(),
                "The held Windows input must deny writes"
            );
            held.verify_path(&cancel).unwrap();
        }
        #[cfg(not(windows))]
        {
            replacement.unwrap();
            fs::write(&path, b"different archive bytes").unwrap();
            assert!(
                held.verify_path(&cancel).is_err(),
                "A changed path must not feed a new extraction under the old identity"
            );
            assert!(extract_held(
                &mut held,
                &temp.path().join("must-not-publish"),
                Arc::new(AtomicBool::new(false))
            )
            .is_err());
            assert!(!temp.path().join("must-not-publish").exists());
        }
        // Long replay/inspection receipts retain this identity even if a
        // non-Windows source path was replaced after the verified extraction.
        assert_eq!(held.sha256, bytes_hash(&original));
        let expected = held.bytes;
        assert_eq!(
            hash_archive(&mut held.file, expected, &cancel).unwrap(),
            held.sha256
        );
        assert_eq!(fs::read(extracted.join("project.json")).unwrap(), b"x");
        drop(held);
        #[cfg(windows)]
        {
            fs::rename(&path, &moved).unwrap();
            fs::write(&path, b"later replacement").unwrap();
            assert_ne!(bytes_hash(&fs::read(&path).unwrap()), bytes_hash(&original));
        }
    }
    #[test]
    fn held_archive_cancel_and_changed_length_fail_without_publication() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("not-engine-evidence.zip");
        fs::write(&path, b"bounded original bytes").unwrap();
        assert!(HeldArchive::open(&path, &AtomicBool::new(true)).is_err());
        let cancel = AtomicBool::new(false);
        let mut held = HeldArchive::open(&path, &cancel).unwrap();
        let expected = held.bytes;
        assert!(hash_archive(&mut held.file, expected - 1, &cancel).is_err());
        assert!(hash_archive(&mut held.file, expected, &AtomicBool::new(true)).is_err());
        assert!(extract_held(
            &mut held,
            &temp.path().join("cancelled"),
            Arc::new(AtomicBool::new(true))
        )
        .is_err());
        assert!(!temp.path().join("cancelled").exists());
        assert_eq!(fs::read(&path).unwrap(), b"bounded original bytes");
    }
    #[test]
    fn analytical_only_envelope_needs_no_fake_scope_but_preserves_all_limits() {
        let p = metadata();
        p.validate().unwrap();
        assert!(p.scope_revision_ids.is_empty());
        for name in [
            "../outside",
            "C:/outside",
            "bad\\path",
            MANIFEST,
            RECIPE,
            "checksums.json",
        ] {
            let mut bad = p.clone();
            bad.members.insert(
                name.into(),
                PackageMember {
                    sha256: "d".repeat(64),
                    bytes: 1,
                },
            );
            bad.content_sha256 = bad.identity().unwrap();
            assert!(bad.validate().is_err());
        }
        let mut bad = p.clone();
        bad.design_report_ids.push(bad.design_report_ids[0].clone());
        bad.content_sha256 = bad.identity().unwrap();
        assert!(bad.validate().is_err());
        let mut bad = p.clone();
        bad.members.get_mut("project.json").unwrap().bytes = MAX_EVIDENCE_FILE_BYTES + 1;
        bad.content_sha256 = bad.identity().unwrap();
        assert!(bad.validate().is_err());
        let mut bad = p.clone();
        for i in 0..4 {
            bad.members.insert(
                format!("large-{i}"),
                PackageMember {
                    sha256: "d".repeat(64),
                    bytes: MAX_EVIDENCE_FILE_BYTES,
                },
            );
        }
        bad.content_sha256 = bad.identity().unwrap();
        assert!(bad.validate().is_err());
        let mut bad = p;
        for i in 0..MAX_PACKAGE_ENTRIES - 3 {
            bad.members.insert(
                format!("tiny-{i}"),
                PackageMember {
                    sha256: "d".repeat(64),
                    bytes: 0,
                },
            );
        }
        bad.content_sha256 = bad.identity().unwrap();
        assert!(bad.validate().is_err());
    }
    #[test]
    fn envelope_overhead_and_equal_size_tampering_fail_without_publication() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("project.json"), b"x").unwrap();
        let mut p = metadata();
        let staging = temp.path().join("stage");
        let cancel = AtomicBool::new(false);
        let members = stage(&source, &staging, &p, &cancel).unwrap();
        fs::write(staging.join("project.json"), b"y").unwrap();
        let zip = temp.path().join("rejected.zip");
        assert!(archive::write_zip(&staging, &members, &zip, &cancel).is_err());
        assert!(!zip.exists());
        p.members.get_mut("project.json").unwrap().bytes = MAX_EVIDENCE_FILE_BYTES;
        for i in 0..3 {
            p.members.insert(
                format!("large-{i}"),
                PackageMember {
                    sha256: "d".repeat(64),
                    bytes: MAX_EVIDENCE_FILE_BYTES,
                },
            );
        }
        p.content_sha256 = p.identity().unwrap();
        p.validate().unwrap();
        assert!(envelope(&p).is_err());
    }
    #[test]
    fn cancellation_and_consistent_metadata_never_become_accepted_engine_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("project.json"), b"x").unwrap();
        let p = metadata();
        assert!(stage(
            &source,
            &temp.path().join("cancelled"),
            &p,
            &AtomicBool::new(true)
        )
        .is_err());
        let staging = temp.path().join("stage");
        let active = AtomicBool::new(false);
        let members = stage(&source, &staging, &p, &active).unwrap();
        assert!(validate_extracted(&staging, &members, &active).is_err());
        let zip = temp.path().join("metadata-only.zip");
        archive::write_zip(&staging, &members, &zip, &active).unwrap();
        let destination = temp.path().join("not-published");
        assert!(extract(&zip, &destination, Arc::new(AtomicBool::new(false))).is_err());
        assert!(!destination.exists());
    }
    #[test]
    fn excluded_attachments_require_explicit_inclusion_and_complete_export() {
        let mut project = crate::energy_services::example_document();
        assert!(privacy(&project).is_err());
        // Privacy-only metadata, deliberately not an accepted report/run.
        let run = crate::design_report::DesignRunBinding {
            run_id: "not-a-run".into(),
            record_sha256: "a".repeat(64),
            design_id: "A".into(),
        };
        project.design_reports = Some(crate::design_report::DesignReportIndex {
            schema_version: crate::design_report::INDEX_SCHEMA.into(),
            reports: vec![crate::design_report::DesignReportSummary {
                id: "b".repeat(64),
                path: format!("design-reports/{}", "b".repeat(64)),
                content_sha256: "b".repeat(64),
                manifest_file_sha256: "c".repeat(64),
                products_sha256: "d".repeat(64),
                baseline: run.clone(),
                proposed: run,
                sensitivity_run_ids: vec![],
                title: "Metadata test".into(),
                report_revision: "Draft".into(),
                include_pdf: false,
            }],
        });
        privacy(&project).unwrap();
        let id = Uuid::new_v4().to_string();
        project.energy.as_mut().unwrap().attachments.push(
            crate::energy_attachments::EnergyAttachment {
                id: id.clone(),
                path: format!("attachments/{id}/evidence.pdf"),
                sha256: "e".repeat(64),
                media_type: "application/pdf".into(),
                model_input_sha256: "f".repeat(64),
                caption: "Private unselected test declaration".into(),
                rights: "Original synthetic fixture".into(),
                include_in_export: false,
            },
        );
        assert!(privacy(&project)
            .unwrap_err()
            .to_string()
            .contains("excluded attachments"));
        project.energy.as_mut().unwrap().attachments[0].include_in_export = true;
        privacy(&project).unwrap();
        assert!(crate::validate_project(&project).is_err()); // Not accepted evidence.
        let serialized = serde_json::to_string(&metadata()).unwrap();
        assert!(!serialized.contains("LOCALAPPDATA"));
        assert!(!serialized.contains("executable_path"));
        assert!(REPLAY_TEXT.contains("no imported commands"));
        assert!(POLICY.contains("No retained history is pruned"));
    }
}
