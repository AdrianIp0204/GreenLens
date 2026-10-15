//! Cancellable, read-only project intake. Reuse is confined to one verified
//! snapshot; a later open always checks actual bytes again, never timestamps.
//! Panel inspection may reuse a private verified record only after rehashing
//! its complete pinned evidence set, without repeating annual arithmetic.
use crate::{energy, energy_sensitivity_workflow, persistence, Project, ProjectView};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

pub const VERIFICATION_DEADLINE_SECONDS: u64 = 120;
pub const MAX_SNAPSHOT_METADATA_BYTES: usize = 512 * 1024 * 1024;

/// Only the full reader and its final byte fence, or an exact byte-checked
/// copy of an existing token, can construct this token.
/// It is process-local, never serialized or accepted from an IPC caller.
pub(crate) struct VerifiedEnergyRun {
    root: PathBuf,
    summary: energy::EnergyRunSummary,
    record: energy::EnergyRunRecord,
    members: BTreeMap<String, String>,
    metadata_bytes: usize,
}
impl VerifiedEnergyRun {
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
    pub(crate) fn summary(&self) -> &energy::EnergyRunSummary {
        &self.summary
    }
    pub(crate) fn record(&self) -> &energy::EnergyRunRecord {
        &self.record
    }
    pub(crate) fn members(&self) -> &BTreeMap<String, String> {
        &self.members
    }
    pub(crate) fn metadata_bytes(&self) -> usize {
        self.metadata_bytes
    }

    fn from_fenced(
        root: &Path,
        summary: &energy::EnergyRunSummary,
        record: energy::EnergyRunRecord,
        observed: &BTreeMap<String, String>,
    ) -> Result<Self> {
        energy::validate_summary_binding(&record, summary)?;
        let mut names = energy::evidence_files(&record);
        names.push("checksums.json".into());
        let members = names
            .into_iter()
            .map(|name| {
                let name = format!("{}/{name}", summary.path);
                let digest = observed
                    .get(&name)
                    .with_context(|| format!("Validated energy member was not observed: {name}"))?;
                Ok((name, digest.clone()))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let root = std::fs::canonicalize(root)?;
        let metadata_bytes = serde_json::to_vec(&(&root, summary, &record, &members))?.len();
        Ok(Self {
            root,
            summary: summary.clone(),
            record,
            members,
            metadata_bytes,
        })
    }
}

/// Bounded scoped byte observation for independent inspection service calls.
fn inspection_scope<T>(
    root: &Path,
    cancel: Arc<AtomicBool>,
    action: impl FnOnce(&Arc<Control>) -> Result<T>,
) -> Result<T> {
    let control = Arc::new(Control {
        root: root.into(),
        started: Instant::now(),
        cancel,
        progress: Mutex::new(OpenProgress {
            schema_version: "greenlens.project-open-progress.v1".into(),
            stage: "energy_inspection".into(),
            run_id: None,
            runs_completed: 0,
            runs_total: 1,
            bytes_read: 0,
            files_verified: 0,
            numeric_passes: BTreeMap::new(),
            elapsed_ms: 0,
        }),
        observed: Mutex::new(BTreeMap::new()),
        last_emitted: Mutex::new(Instant::now()),
        emit: Box::new(|_| {}),
    });
    // These readers are independent service calls. Refuse accidental nested
    // instrumentation rather than hide a parent operation's cancellation.
    ensure!(
        ACTIVE.with(|active| active.borrow().is_none()),
        "Nested energy inspection must use the verified snapshot provider"
    );
    let _guard = ActiveGuard(ACTIVE.with(|active| active.replace(Some(control.clone()))));
    control.checkpoint(0, false)?;
    let result = action(&control)?;
    control.checkpoint(0, false)?;
    Ok(result)
}

pub(crate) fn verify_energy_for_inspection(
    root: &Path,
    summary: &energy::EnergyRunSummary,
) -> Result<VerifiedEnergyRun> {
    inspection_scope(root, Arc::new(AtomicBool::new(false)), |control| {
        let record = energy::read_run(root, summary)?;
        let observed = control.observed.lock().unwrap().clone();
        verify_member_bytes(root, &observed)?;
        VerifiedEnergyRun::from_fenced(root, summary, record, &observed)
    })
}

fn verify_member_bytes(root: &Path, members: &BTreeMap<String, String>) -> Result<()> {
    for (name, expected) in members {
        ensure!(
            persistence::evidence_file_hash(&persistence::regular_child(root, name)?)? == *expected,
            "Energy evidence changed since complete verification: {name}; reopen the project to validate the changed evidence"
        );
    }
    Ok(())
}

pub(crate) fn check_energy_member_bytes(
    root: &Path,
    members: &BTreeMap<String, String>,
    cancel: Arc<AtomicBool>,
) -> Result<()> {
    inspection_scope(root, cancel, |_| verify_member_bytes(root, members))
}

/// Transfer already validated metadata to a new root only after all copied
/// members, including the outer seal, equal the original pinned bytes. A new
/// self-consistent seal cannot substitute different source evidence mid-copy.
/// This does not change explicit project opening: that still rederives every
/// retained run and performs its own final content fence.
pub(crate) fn verify_copied_energy(
    destination: &Path,
    source: &VerifiedEnergyRun,
) -> Result<VerifiedEnergyRun> {
    inspection_scope(destination, Arc::new(AtomicBool::new(false)), |_| {
        verify_member_bytes(destination, source.members())?;
        VerifiedEnergyRun::from_fenced(
            destination,
            source.summary(),
            source.record().clone(),
            source.members(),
        )
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectAdmission {
    pub schema_version: String,
    pub project_id: String,
    pub project_name: String,
    pub manifest_sha256: String,
    pub static_runs: usize,
    pub climate_runs: usize,
    pub energy_runs: usize,
    pub evidence_checked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenProgress {
    pub schema_version: String,
    pub stage: String,
    pub run_id: Option<String>,
    pub runs_completed: usize,
    pub runs_total: usize,
    /// Instrumented bounded read/hash work, including repeated checked reads.
    /// This is not a hardware disk-I/O or operating-system cache counter.
    pub bytes_read: u64,
    pub files_verified: usize,
    pub numeric_passes: BTreeMap<String, usize>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectVerificationReceipt {
    pub schema_version: String,
    pub started_utc: String,
    pub finished_utc: String,
    pub manifest_sha256: String,
    pub content_snapshot_sha256: String,
    pub progress: OpenProgress,
    pub scope: String,
}

struct Control {
    root: PathBuf,
    started: Instant,
    cancel: Arc<AtomicBool>,
    progress: Mutex<OpenProgress>,
    observed: Mutex<BTreeMap<String, String>>,
    last_emitted: Mutex<Instant>,
    emit: Box<dyn Fn(OpenProgress) + Send + Sync>,
}

thread_local! {
    // Read instrumentation is deliberately scoped to the single blocking intake
    // operation; ordinary readers and scientific outputs are unaffected.
    static ACTIVE: RefCell<Option<Arc<Control>>> = const { RefCell::new(None) };
    #[cfg(test)]
    static TEST_NUMERIC_PASSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[cfg(test)]
pub(crate) fn test_numeric_passes() -> usize {
    TEST_NUMERIC_PASSES.with(|count| count.get())
}
struct ActiveGuard(Option<Arc<Control>>);
impl Drop for ActiveGuard {
    fn drop(&mut self) {
        ACTIVE.with(|active| *active.borrow_mut() = self.0.take());
    }
}
impl Control {
    fn checkpoint(&self, bytes: u64, force: bool) -> Result<()> {
        ensure!(
            !self.cancel.load(Ordering::Relaxed),
            "Project verification cancelled; the previous document is unchanged"
        );
        ensure!(self.started.elapsed() < Duration::from_secs(VERIFICATION_DEADLINE_SECONDS), "Project verification exceeded the 120-second budget; the previous document is unchanged");
        let event = {
            let mut p = self
                .progress
                .lock()
                .map_err(|_| anyhow::anyhow!("Verification progress unavailable"))?;
            p.bytes_read = p
                .bytes_read
                .checked_add(bytes)
                .context("Verification byte counter overflow")?;
            p.elapsed_ms = self
                .started
                .elapsed()
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX);
            p.clone()
        };
        let should_emit = {
            let mut last = self
                .last_emitted
                .lock()
                .map_err(|_| anyhow::anyhow!("Verification progress unavailable"))?;
            if force || last.elapsed() >= Duration::from_millis(100) {
                *last = Instant::now();
                true
            } else {
                false
            }
        };
        if should_emit {
            (self.emit)(event);
        }
        ensure!(
            !self.cancel.load(Ordering::Relaxed),
            "Project verification cancelled; the previous document is unchanged"
        );
        Ok(())
    }
    fn stage(&self, stage: &str, run_id: Option<&str>) -> Result<()> {
        {
            let mut p = self.progress.lock().unwrap();
            p.stage = stage.into();
            p.run_id = run_id.map(str::to_owned);
        }
        self.checkpoint(0, true)
    }
}

pub(crate) fn checkpoint(bytes: u64) -> Result<()> {
    ACTIVE.with(|active| {
        active
            .borrow()
            .as_ref()
            .map_or(Ok(()), |c| c.checkpoint(bytes, false))
    })
}
pub(crate) fn observed_file(path: &Path, digest: &str) -> Result<()> {
    ACTIVE.with(|active| {
        let active = active.borrow();
        let Some(c) = active.as_ref() else {
            return Ok(());
        };
        let relative = path
            .strip_prefix(&c.root)
            .context("Evidence read escaped the admitted project")?;
        let relative = relative.to_string_lossy().replace('\\', "/");
        let mut observed = c.observed.lock().unwrap();
        if let Some(previous) = observed.get(&relative) {
            ensure!(
                previous == digest,
                "Evidence changed during project verification: {relative}"
            );
        } else {
            observed.insert(relative, digest.into());
            c.progress.lock().unwrap().files_verified = observed.len();
        }
        Ok(())
    })
}
pub(crate) fn numeric_pass(run_id: &str) -> Result<()> {
    #[cfg(test)]
    TEST_NUMERIC_PASSES.with(|count| count.set(count.get() + 1));
    ACTIVE.with(|active| {
        let active = active.borrow();
        if let Some(c) = active.as_ref() {
            let mut p = c.progress.lock().unwrap();
            let count = p.numeric_passes.entry(run_id.into()).or_default();
            *count += 1;
            ensure!(
                *count == 1,
                "Repeated numeric traversal during one verified snapshot: {run_id}"
            );
        }
        Ok(())
    })
}

fn manifest(path: &Path) -> Result<(Project, String)> {
    let bytes = persistence::read_json_bytes(&persistence::regular_child(path, "project.json")?)?;
    let project = serde_json::from_slice(&bytes)?;
    crate::validate_project(&project)?;
    Ok((project, persistence::bytes_hash(&bytes)))
}
fn admission(project: &Project, revision: String) -> ProjectAdmission {
    ProjectAdmission {
        schema_version: "greenlens.project-admission.v1".into(),
        project_id: project.id.clone(),
        project_name: project.name.clone(),
        manifest_sha256: revision,
        static_runs: project.runs.len(),
        climate_runs: project.climate.as_ref().map_or(0, |c| c.runs.len()),
        energy_runs: project.energy.as_ref().map_or(0, |e| e.runs.len()),
        evidence_checked: false,
    }
}
/// Cheap admission never opens run chunks, weather, attachments or assessments.
/// It is not permission to display evidence as verified.
pub fn admit_project(path: &Path) -> Result<ProjectAdmission> {
    let (project, revision) = manifest(path)?;
    Ok(admission(&project, revision))
}

pub fn verify_admitted_project(
    path: &Path,
    admitted: &ProjectAdmission,
    cancel: Arc<AtomicBool>,
    progress: impl Fn(OpenProgress) + Send + Sync + 'static,
) -> Result<ProjectView> {
    verify_snapshot(path, admitted, None, cancel, progress)
}

/// Verifies all candidate evidence before a writer publishes its manifest. The
/// existing manifest remains the conflict/final-content fence; it is never
/// replaced merely to make the ordinary reader inspect a proposed document.
/// The caller must keep the writer lock, recheck cancellation/revision before
/// committing, and set the returned revision to atomic_json's successful value.
pub(crate) fn verify_import_candidate(
    path: &Path,
    candidate: &Project,
    expected_revision: Option<&str>,
    cancel: Arc<AtomicBool>,
    progress: impl Fn(OpenProgress) + Send + Sync + 'static,
) -> Result<ProjectView> {
    let admitted = admit_project(path)?;
    ensure!(
        Some(admitted.manifest_sha256.as_str()) == expected_revision,
        "Project changed before reference verification; the previous document is unchanged"
    );
    crate::validate_project(candidate)?;
    verify_snapshot(path, &admitted, Some(candidate.clone()), cancel, progress)
}

fn verify_snapshot(
    path: &Path,
    admitted: &ProjectAdmission,
    candidate: Option<Project>,
    cancel: Arc<AtomicBool>,
    progress: impl Fn(OpenProgress) + Send + Sync + 'static,
) -> Result<ProjectView> {
    let candidate_counts = candidate.as_ref().map(|p| admission(p, String::new()));
    let counts = candidate_counts.as_ref().unwrap_or(admitted);
    let runs_total = counts
        .static_runs
        .checked_add(counts.climate_runs)
        .and_then(|v| v.checked_add(counts.energy_runs))
        .context("Invalid admitted run count")?;
    let started_utc = chrono::Utc::now().to_rfc3339();
    let control = Arc::new(Control {
        root: path.into(),
        started: Instant::now(),
        cancel,
        progress: Mutex::new(OpenProgress {
            schema_version: "greenlens.project-open-progress.v1".into(),
            stage: "admission".into(),
            run_id: None,
            runs_completed: 0,
            runs_total,
            bytes_read: 0,
            files_verified: 0,
            numeric_passes: BTreeMap::new(),
            elapsed_ms: 0,
        }),
        observed: Mutex::new(BTreeMap::new()),
        last_emitted: Mutex::new(Instant::now()),
        emit: Box::new(progress),
    });
    let _guard = ActiveGuard(ACTIVE.with(|active| active.replace(Some(control.clone()))));
    control.checkpoint(0, true)?;
    let (stored_project, revision) = manifest(path)?;
    ensure!(
        &admission(&stored_project, revision.clone()) == admitted,
        "Project changed since admission; choose it again. The previous document is unchanged"
    );
    let is_candidate = candidate.is_some();
    let project = candidate.unwrap_or(stored_project);
    let mut results = Vec::new();
    for summary in &project.runs {
        control.stage("static_evidence", Some(&summary.id))?;
        results.push(crate::read_run(path, summary)?);
        control.progress.lock().unwrap().runs_completed += 1;
    }
    let climate_results = if let Some(extension) = &project.climate {
        control.stage("climate_evidence", None)?;
        crate::climate::verify_project_evidence(path, extension)?;
        let records = extension
            .runs
            .iter()
            .map(|r| crate::climate::read_result_metadata(path, r))
            .collect::<Result<Vec<_>>>()?;
        control.progress.lock().unwrap().runs_completed += records.len();
        records
    } else {
        vec![]
    };
    let mut energy_results = Vec::new();
    let mut energy_metadata_bytes = 0usize;
    if let Some(extension) = &project.energy {
        control.stage("weather_and_attachments", None)?;
        energy::verify_project_assets(path, extension)?;
        for summary in &extension.runs {
            control.stage("energy_evidence", Some(&summary.id))?;
            let run = energy::read_run(path, summary)?;
            energy_metadata_bytes = energy_metadata_bytes
                .checked_add(serde_json::to_vec(&run)?.len())
                .context("Verified metadata size overflow")?;
            ensure!(
                energy_metadata_bytes <= MAX_SNAPSHOT_METADATA_BYTES,
                "Verified project metadata exceeds the 512 MiB intake budget; retain fewer runs"
            );
            energy_results.push(run);
            control.progress.lock().unwrap().runs_completed += 1;
        }
    }
    if let Some(extension) = &project.architecture {
        control.stage("architectural_reference", None)?;
        crate::reference::verify_project(path, extension, &control.cancel)?;
    }
    control.stage("sensitivity_evidence", None)?;
    // The provider is private to this function and cannot be constructed from
    // frontend claims. Each returned record has passed the full reader above.
    let mut provider = |summary: &energy::EnergyRunSummary| -> Result<energy::EnergyRunRecord> {
        let record = energy_results
            .iter()
            .find(|r| r.id == summary.id)
            .context("Assessment references a run outside the verified snapshot")?;
        energy::validate_summary_binding(record, summary)?;
        Ok(record.clone())
    };
    let energy_sensitivity_sessions =
        energy_sensitivity_workflow::inspect_all_with_runs(path, &project, &mut provider)?;
    let energy_assessments = crate::energy_services::assessments(&project, &energy_results)?;
    let climate_assessments =
        crate::qualification::assessments_for_verified_view(&project, &climate_results)?;
    control.stage("finalizing_snapshot", None)?;
    // A previously decoded run may have been altered while another run was
    // inspected. Rehash actual members before publishing the candidate; this
    // fence never decodes values or rederives arithmetic a second time.
    let observed = control.observed.lock().unwrap().clone();
    for (name, expected) in &observed {
        ensure!(
            persistence::evidence_file_hash(&persistence::regular_child(path, name)?)? == *expected,
            "Evidence changed before snapshot acceptance: {name}"
        );
    }
    persistence::check_revision(path, Some(&revision))?;
    control.stage("completed", None)?;
    let receipt = ProjectVerificationReceipt {
        schema_version: "greenlens.project-verification.v1".into(), started_utc,
        finished_utc: chrono::Utc::now().to_rfc3339(), manifest_sha256: revision.clone(),
        content_snapshot_sha256: greenlens_core::canonical_hash(&*control.observed.lock().unwrap())?,
        progress: control.progress.lock().unwrap().clone(),
        scope: "Actual retained bytes checked during this read-only open, with a final content-hash fence; each energy series decoded and arithmetic rederived once. Sensitivity uses only those exact verified records. This is an as-of-open snapshot, not a filesystem monitor, future integrity promise or scientific qualification. Later opens verify bytes again.".into(),
    };
    let view = ProjectView {
        project,
        revision,
        results,
        climate_results,
        energy_results,
        energy_assessments,
        climate_assessments,
        // This receipt binds the old on-disk manifest. Returning it for a newly
        // published candidate would mislabel its content snapshot; the verified
        // records are reused, but a later read-only open produces a new receipt.
        project_verification: if is_candidate { None } else { Some(receipt) },
        energy_sensitivity_sessions,
    };
    ensure!(
        serde_json::to_vec(&view)?.len() <= MAX_SNAPSHOT_METADATA_BYTES,
        "Verified project metadata exceeds the 512 MiB intake budget; retain fewer runs"
    );
    // Cache only fully fenced records after every acceptance/cancellation/size
    // check. Opening always reprocesses each run; this merely prevents a later
    // panel remount from repeating arithmetic on those same unchanged bytes.
    let seeds = view
        .project
        .energy
        .as_ref()
        .map(|extension| {
            extension
                .runs
                .iter()
                .zip(&view.energy_results)
                .rev()
                .take(8)
                .map(|(summary, record)| {
                    control.checkpoint(0, false)?;
                    VerifiedEnergyRun::from_fenced(path, summary, record.clone(), &observed)
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    control.checkpoint(0, false)?;
    for seed in seeds {
        crate::energy_verified_cache::seed(seed);
    }
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scoped_control(root: &Path, cancel: Arc<AtomicBool>) -> (Arc<Control>, ActiveGuard) {
        let control = Arc::new(Control {
            root: root.into(),
            started: Instant::now(),
            cancel,
            progress: Mutex::new(OpenProgress {
                schema_version: "test".into(),
                stage: "test".into(),
                run_id: None,
                runs_completed: 0,
                runs_total: 0,
                bytes_read: 0,
                files_verified: 0,
                numeric_passes: BTreeMap::new(),
                elapsed_ms: 0,
            }),
            observed: Mutex::new(BTreeMap::new()),
            last_emitted: Mutex::new(Instant::now()),
            emit: Box::new(|_| {}),
        });
        let guard = ActiveGuard(ACTIVE.with(|active| active.replace(Some(control.clone()))));
        (control, guard)
    }
    #[test]
    fn admission_does_not_traverse_missing_run_evidence() {
        let root = tempfile::tempdir().unwrap();
        let mut project = crate::example();
        let id = uuid::Uuid::new_v4().to_string();
        project.runs.push(crate::RunSummary {
            path: format!("runs/{id}"),
            id,
            alternative_id: "A".into(),
            scientific_input_hash: "0".repeat(64),
            comparison_fingerprint: "0".repeat(64),
        });
        persistence::atomic_json(root.path(), &project).unwrap();
        let admitted = admit_project(root.path()).unwrap();
        assert_eq!(admitted.static_runs, 1);
        assert!(!admitted.evidence_checked);
        assert!(verify_admitted_project(
            root.path(),
            &admitted,
            Arc::new(AtomicBool::new(false)),
            |_| {}
        )
        .is_err());
        assert_eq!(admit_project(root.path()).unwrap(), admitted);
    }
    #[test]
    fn bounded_stream_checks_cancellation_after_one_block_and_scope_cleans_up() {
        struct CancellingReader(Arc<AtomicBool>);
        impl std::io::Read for CancellingReader {
            fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
                bytes.fill(7);
                self.0.store(true, Ordering::Relaxed);
                Ok(bytes.len())
            }
        }
        let root = tempfile::tempdir().unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let (_, guard) = scoped_control(root.path(), cancelled.clone());
        let error =
            persistence::read_limited(CancellingReader(cancelled), 1024 * 1024).unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        drop(guard);
        assert_eq!(persistence::read_limited(&b"ok"[..], 2).unwrap(), b"ok");
    }
    #[test]
    fn same_size_same_timestamp_mutation_invalidates_observed_content() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("inert-fixture.bin");
        std::fs::write(&path, b"before").unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        let (_, _guard) = scoped_control(root.path(), Arc::new(AtomicBool::new(false)));
        persistence::read_evidence_bytes(&path, 6).unwrap();
        std::fs::write(&path, b"after!").unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            modified
        );
        assert!(persistence::read_evidence_bytes(&path, 6)
            .unwrap_err()
            .to_string()
            .contains("changed during"));
    }
    #[test]
    fn series_cancellation_stops_before_the_next_256_row_block() {
        use crate::energy_series::{EnergyChannel, EnergySeriesReader, EnergySeriesWriter};
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("series");
        let mut writer = EnergySeriesWriter::create(
            &directory,
            vec![EnergyChannel {
                id: "fixture".into(),
                label: "Synthetic storage fixture".into(),
                unit: "J".into(),
            }],
            10,
        )
        .unwrap();
        for i in 0..52560 {
            writer.push(i, &[i as f64]).unwrap();
        }
        writer.finish().unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let (_, _guard) = scoped_control(root.path(), cancel.clone());
        let reader = EnergySeriesReader::open(&directory).unwrap();
        let mut rows = 0;
        let error = reader
            .visit(|index, _| {
                rows += 1;
                if index == 2 {
                    cancel.store(true, Ordering::Relaxed);
                }
                Ok(())
            })
            .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        assert_eq!(rows, 256);
    }
    #[test]
    fn admission_is_unverified_and_changed_manifest_never_returns_a_candidate() {
        let root = tempfile::tempdir().unwrap();
        let mut project = crate::example();
        persistence::atomic_json(root.path(), &project).unwrap();
        let admitted = admit_project(root.path()).unwrap();
        assert!(!admitted.evidence_checked);
        project.name = "External replacement".into();
        persistence::atomic_json(root.path(), &project).unwrap();
        assert!(verify_admitted_project(
            root.path(),
            &admitted,
            Arc::new(AtomicBool::new(false)),
            |_| {}
        )
        .unwrap_err()
        .to_string()
        .contains("changed since admission"));
    }
    #[test]
    fn cancelled_intake_and_final_manifest_conflict_preserve_files() {
        let root = tempfile::tempdir().unwrap();
        let project = crate::example();
        persistence::atomic_json(root.path(), &project).unwrap();
        let admitted = admit_project(root.path()).unwrap();
        let bytes = std::fs::read(root.path().join("project.json")).unwrap();
        assert!(verify_admitted_project(
            root.path(),
            &admitted,
            Arc::new(AtomicBool::new(true)),
            |_| {}
        )
        .unwrap_err()
        .to_string()
        .contains("cancelled"));
        assert_eq!(
            bytes,
            std::fs::read(root.path().join("project.json")).unwrap()
        );
        let directory = root.path().to_owned();
        let error = verify_admitted_project(
            root.path(),
            &admitted,
            Arc::new(AtomicBool::new(false)),
            move |p| {
                if p.stage == "finalizing_snapshot" {
                    let mut replacement = project.clone();
                    replacement.name = "Concurrent writer".into();
                    persistence::atomic_json(&directory, &replacement).unwrap();
                }
            },
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("changed externally")
                || error.to_string().contains("changed during")
        );
    }
    #[test]
    fn successful_reopen_rehashes_content_instead_of_reusing_timestamp_claims() {
        let root = tempfile::tempdir().unwrap();
        persistence::atomic_json(root.path(), &crate::example()).unwrap();
        let admission = admit_project(root.path()).unwrap();
        let first = verify_admitted_project(
            root.path(),
            &admission,
            Arc::new(AtomicBool::new(false)),
            |_| {},
        )
        .unwrap();
        let second = verify_admitted_project(
            root.path(),
            &admission,
            Arc::new(AtomicBool::new(false)),
            |_| {},
        )
        .unwrap();
        assert_eq!(
            first
                .project_verification
                .as_ref()
                .unwrap()
                .content_snapshot_sha256,
            second
                .project_verification
                .as_ref()
                .unwrap()
                .content_snapshot_sha256
        );
        assert!(second.project_verification.unwrap().progress.bytes_read > 0);
    }
}
