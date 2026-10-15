//! Finite, explicit sensitivity steps using the existing Application worker.
//! Every attempt is reserved durably before launch. No loop, retry, alternative
//! run selection, new supervisor or scientific-registry mutation lives here.
use crate::{
    energy, energy_sensitivity as sensitivity, energy_weather,
    persistence::{self, regular_child, WriterLock},
    Application, JobState, JobStatus, Project, ProjectView,
};
use anyhow::{ensure, Context, Result};
use greenlens_core::{canonical_hash, energy::*};
use greenlens_process::ProcessOutcome;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const SCHEMA: &str = "greenlens.energy-sensitivity-session.v1";
const MAX_SESSIONS: usize = 32;
const MAX_EVENTS: usize = 13; // Four reserve/start/outcome triples and one final result.

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnergySensitivityEventSeal {
    pub sequence: u8,
    pub file_sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnergySensitivityReference {
    pub id: String,
    pub path: String,
    pub plan_sha256: String,
    pub plan_file_sha256: String,
    pub events: Vec<EnergySensitivityEventSeal>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedPlan {
    schema_version: String,
    project_id: String,
    plan: sensitivity::EnergySensitivityPlan,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EnergySensitivityEvent {
    Reserved {
        slot_id: String,
    },
    Started {
        slot_id: String,
        job_id: String,
    },
    Collected {
        slot_id: String,
        job_id: String,
        record_sha256: String,
        #[serde(deserialize_with = "deserialize_process_receipts")]
        processes: Vec<ProcessOutcome>,
    },
    Stopped {
        slot_id: String,
        job_id: Option<String>,
        interruption: EnergySensitivityInterruptionKind,
        reason: String,
        status_sha256: Option<String>,
        #[serde(deserialize_with = "deserialize_process_receipts")]
        process: Option<ProcessOutcome>,
    },
    Finalized {
        result: sensitivity::EnergySensitivityResult,
    },
}
// Serde's internally tagged enum buffer does not deserialize u128. Native
// receipts retain that historical elapsed_ms type. An ordinary JSON Value
// restores the JSON number deserializer without changing any retained bytes.
fn deserialize_process_receipts<'de, D, T>(deserializer: D) -> std::result::Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    serde_json::from_value(value).map_err(serde::de::Error::custom)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EventFile {
    schema_version: String,
    sequence: u8,
    plan_sha256: String,
    previous_file_sha256: String,
    recorded_utc: String,
    event: EnergySensitivityEvent,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergySensitivitySession {
    pub reference: EnergySensitivityReference,
    pub plan: sensitivity::EnergySensitivityPlan,
    pub next_slot_id: Option<String>,
    pub active_slot_id: Option<String>,
    pub active_job_id: Option<String>,
    pub completed: BTreeMap<String, String>,
    pub interruptions: Vec<EnergySensitivityInterruption>,
    pub finalized: bool,
    pub result: Option<sensitivity::EnergySensitivityResult>,
    pub execution_policy: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergySensitivityStep {
    pub view: ProjectView,
    pub session: EnergySensitivitySession,
    pub job: Option<JobStatus>,
}

fn hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn uuid(s: &str) -> Result<()> {
    ensure!(
        uuid::Uuid::parse_str(s)?.to_string() == s,
        "Noncanonical sensitivity identifier"
    );
    Ok(())
}
pub fn validate_references(references: &[EnergySensitivityReference]) -> Result<()> {
    ensure!(
        references.len() <= MAX_SESSIONS,
        "At most32 retained sensitivity sessions; retain older sessions in a separate project"
    );
    let mut ids = BTreeSet::new();
    for r in references {
        uuid(&r.id)?;
        ensure!(
            ids.insert(&r.id) && r.path == format!("energy-assessments/{}", r.id),
            "Invalid or duplicate sensitivity path"
        );
        ensure!(
            hash(&r.plan_sha256) && hash(&r.plan_file_sha256) && r.events.len() <= MAX_EVENTS,
            "Invalid sensitivity seals/count"
        );
        for (i, e) in r.events.iter().enumerate() {
            ensure!(
                usize::from(e.sequence) == i + 1 && hash(&e.file_sha256),
                "Invalid sensitivity event order/seal"
            );
        }
    }
    Ok(())
}
fn references(project: &Project) -> &[EnergySensitivityReference] {
    project
        .energy
        .as_ref()
        .and_then(|e| e.workbench.as_ref())
        .map(|w| w.sensitivity.as_slice())
        .unwrap_or(&[])
}
fn reference<'a>(project: &'a Project, id: &str) -> Result<&'a EnergySensitivityReference> {
    references(project)
        .iter()
        .find(|r| r.id == id)
        .context("Retained sensitivity session is missing")
}
fn event_name(sequence: u8) -> String {
    format!("event-{sequence:03}.json")
}
fn write_new(path: &Path, value: &impl Serialize) -> Result<String> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    ensure!(
        bytes.len() as u64 <= persistence::MAX_EVIDENCE_FILE_BYTES,
        "Sensitivity evidence exceeds the existing64MiB entry bound"
    );
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(persistence::bytes_hash(&bytes))
}
fn checked_json<T: serde::de::DeserializeOwned>(
    root: &Path,
    name: &str,
    expected: &str,
) -> Result<T> {
    let bytes = persistence::read_json_bytes(&regular_child(root, name)?)?;
    ensure!(
        persistence::bytes_hash(&bytes) == expected,
        "Sensitivity evidence seal differs: {name}"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
fn utc(s: &str) -> Result<chrono::DateTime<chrono::FixedOffset>> {
    let t = chrono::DateTime::parse_from_rfc3339(s)?;
    ensure!(
        t.offset().local_minus_utc() == 0,
        "Sensitivity receipt time must be UTC"
    );
    Ok(t)
}
fn current_plan(project: &Project, plan: &sensitivity::EnergySensitivityPlan) -> Result<()> {
    plan.validate()?;
    let targets = plan
        .declaration
        .targets
        .iter()
        .map(|t| t.alternative_id.clone())
        .collect::<Vec<_>>();
    ensure!(
        sensitivity::current_inputs_identity(project, &targets)? == plan.current_inputs_sha256,
        "Accepted inputs changed since declaration; restore them or declare a new assessment"
    );
    let energy = project.energy.as_ref().context("Energy document missing")?;
    for target in &plan.declaration.targets {
        let design = &energy.designs[&target.alternative_id];
        let mut study = design
            .study
            .clone()
            .context("Sensitivity study is missing")?;
        study.zone_timesteps_per_hour = 6;
        ensure!(
            design.model == target.model && study == target.study_10min,
            "Declared snapshots differ from accepted input objects"
        );
    }
    for slot in &plan.declaration.slots {
        let (model, study) = plan.declaration.slot_inputs(&slot.id)?;
        ensure!(
            greenlens_energyplus::method_identity(&study)? == slot.method,
            "Declared sensitivity method is not the currently recognized adapter"
        );
        ensure!(
            greenlens_energyplus::preflight_model(model, &study).supported,
            "Declared input is not supported by current adapter preflight"
        );
    }
    Ok(())
}

fn reduce(
    r: &EnergySensitivityReference,
    plan: sensitivity::EnergySensitivityPlan,
    files: &[EventFile],
) -> Result<EnergySensitivitySession> {
    ensure!(
        files.len() == r.events.len(),
        "Sensitivity event coverage differs from the manifest"
    );
    let missing = plan.missing_slots();
    let mut session = EnergySensitivitySession {
        reference: r.clone(), plan, next_slot_id: missing.first().cloned(), active_slot_id: None,
        active_job_id: None, completed: BTreeMap::new(), interruptions: vec![], finalized: false,
        result: None,
        execution_policy: "Explicit next step only; at most one attempt per declared missing slot, in fixed order. Existing Application owns the only worker,1800s/2GiB memory/1GiB scratch/128MiB file per native case. Ordinary collect retains completed evidence. No retry after failure/cancellation/ambiguous recovery; no registry promotion.".into(),
    };
    let mut previous = r.plan_file_sha256.clone();
    let mut previous_utc = utc(&session.plan.created_utc)?;
    let mut job_ids = BTreeSet::new();
    for (file, seal) in files.iter().zip(&r.events) {
        ensure!(
            !session.finalized
                && file.schema_version == SCHEMA
                && file.sequence == seal.sequence
                && file.plan_sha256 == r.plan_sha256
                && file.previous_file_sha256 == previous,
            "Sensitivity event chain/identity differs"
        );
        let at = utc(&file.recorded_utc)?;
        ensure!(
            at >= previous_utc,
            "Sensitivity event chronology moved backwards"
        );
        previous_utc = at;
        previous = seal.file_sha256.clone();
        match &file.event {
            EnergySensitivityEvent::Reserved { slot_id } => {
                ensure!(
                    session.interruptions.is_empty()
                        && session.active_slot_id.is_none()
                        && session.next_slot_id.as_ref() == Some(slot_id),
                    "Only the next unattempted declared slot may be reserved"
                );
                session.active_slot_id = Some(slot_id.clone());
            }
            EnergySensitivityEvent::Started { slot_id, job_id } => {
                uuid(job_id)?;
                ensure!(
                    session.active_slot_id.as_ref() == Some(slot_id)
                        && session.active_job_id.is_none()
                        && job_ids.insert(job_id),
                    "Started job does not match the sole reserved attempt"
                );
                session.active_job_id = Some(job_id.clone());
            }
            EnergySensitivityEvent::Collected {
                slot_id,
                job_id,
                record_sha256,
                processes,
            } => {
                ensure!(
                    session.active_slot_id.as_ref() == Some(slot_id)
                        && session.active_job_id.as_ref() == Some(job_id)
                        && hash(record_sha256),
                    "Collected run does not match the declared job"
                );
                ensure!(
                    processes.len() == 1
                        && processes.iter().all(|p| p.state == "completed"
                            && p.exit_code == Some(0)
                            && p.failure.is_none()
                            && p.process_tree_empty),
                    "Collected event requires one fully successful closed native process"
                );
                session.completed.insert(slot_id.clone(), job_id.clone());
                session.active_slot_id = None;
                session.active_job_id = None;
                session.next_slot_id = missing
                    .iter()
                    .find(|id| !session.completed.contains_key(*id))
                    .cloned();
            }
            EnergySensitivityEvent::Stopped {
                slot_id,
                job_id,
                interruption,
                reason,
                status_sha256,
                ..
            } => {
                ensure!(
                    session.active_slot_id.as_ref() == Some(slot_id)
                        && &session.active_job_id == job_id
                        && session.interruptions.is_empty(),
                    "Stop receipt differs from the active reserved/started slot"
                );
                ensure!(
                    !reason.trim().is_empty()
                        && reason.len() <= 4096
                        && status_sha256.as_ref().is_none_or(|s| hash(s)),
                    "Invalid bounded stop receipt"
                );
                session.interruptions.push(EnergySensitivityInterruption {
                    slot_id: slot_id.clone(),
                    kind: *interruption,
                    receipt_sha256: seal.file_sha256.clone(),
                    reason: reason.clone(),
                });
                session.active_slot_id = None;
                session.active_job_id = None;
                session.next_slot_id = None;
            }
            EnergySensitivityEvent::Finalized { result } => {
                ensure!(
                    session.active_slot_id.is_none()
                        && (session.next_slot_id.is_none() || !session.interruptions.is_empty())
                        && result.plan_sha256 == r.plan_sha256,
                    "Final assessment has active or unattempted non-interrupted slots"
                );
                ensure!(
                    !result.assessment.qualification_promoted,
                    "Sensitivity cannot promote qualification"
                );
                session.result = Some(result.clone());
                session.finalized = true;
            }
        }
    }
    Ok(session)
}

fn read_files(
    root: &Path,
    project: &Project,
    r: &EnergySensitivityReference,
) -> Result<(EnergySensitivitySession, Vec<EventFile>)> {
    validate_references(std::slice::from_ref(r))?;
    let retained: RetainedPlan =
        checked_json(root, &format!("{}/plan.json", r.path), &r.plan_file_sha256)?;
    ensure!(
        retained.schema_version == SCHEMA
            && retained.project_id == project.id
            && retained.plan.identity()? == r.plan_sha256,
        "Sensitivity plan/project binding differs"
    );
    let names: BTreeSet<String> = std::iter::once("plan.json".into())
        .chain(r.events.iter().map(|e| event_name(e.sequence)))
        .collect();
    let mut observed = BTreeSet::new();
    for entry in fs::read_dir(root.join(&r.path))? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("Invalid sensitivity member name"))?;
        if name != ".greenlens.lock" {
            observed.insert(name);
        }
    }
    ensure!(observed == names, "Sensitivity directory contains missing/unreferenced receipts; resolve the interrupted metadata transaction, never silently retry");
    let files = r
        .events
        .iter()
        .map(|e| {
            checked_json(
                root,
                &format!("{}/{}", r.path, event_name(e.sequence)),
                &e.file_sha256,
            )
        })
        .collect::<Result<Vec<EventFile>>>()?;
    let session = reduce(r, retained.plan, &files)?;
    Ok((session, files))
}
/// Fully validates imported/reopened sidecar state and all attached run evidence.
pub fn inspect(root: &Path, project: &Project, id: &str) -> Result<EnergySensitivitySession> {
    let mut checked = BTreeMap::new();
    inspect_with_runs(root, project, id, &mut |summary| {
        if let Some(run) = checked.get(&summary.id) {
            energy::validate_summary_binding(run, summary)?;
            return Ok(run.clone());
        }
        let run = crate::energy_verified_cache::read_run(root, summary)?;
        checked.insert(summary.id.clone(), run.clone());
        Ok(run)
    })
}
fn inspect_with_runs(
    root: &Path,
    project: &Project,
    id: &str,
    runs: &mut impl FnMut(&energy::EnergyRunSummary) -> Result<energy::EnergyRunRecord>,
) -> Result<EnergySensitivitySession> {
    let (mut session, files) = read_files(root, project, reference(project, id)?)?;
    let result = sensitivity::evaluate_with_runs(
        project,
        &session.plan,
        &session.completed,
        &session.interruptions,
        runs,
    )?;
    for file in &files {
        if let EnergySensitivityEvent::Collected {
            slot_id,
            job_id,
            record_sha256,
            processes,
        } = &file.event
        {
            let summary = project
                .energy
                .as_ref()
                .unwrap()
                .runs
                .iter()
                .find(|r| &r.id == job_id)
                .context("Collected sensitivity run missing")?;
            let run = runs(summary)?;
            energy::validate_summary_binding(&run, summary)?;
            ensure!(
                result.assessment.runs.iter().any(|r| &r.slot_id == slot_id
                    && &r.run_id == job_id
                    && &r.record_sha256 == record_sha256)
                    && canonical_hash(&run.provenance.processes)? == canonical_hash(processes)?,
                "Collected receipt differs from the fully validated retained run"
            );
            let reserved = files.iter().find(|f| matches!(&f.event, EnergySensitivityEvent::Reserved {slot_id:id} if id == slot_id)).context("Missing attempt reservation")?;
            let started = utc(&run.provenance.started_utc)?;
            let finished = utc(&run.provenance.finished_utc)?;
            ensure!(utc(&reserved.recorded_utc)? <= started && started <= finished && finished <= utc(&file.recorded_utc)?, "Newly collected run does not belong to the reserved execution interval; older runs must be declared as reuse");
        }
    }
    if let Some(retained) = &session.result {
        // Currentness can change when the accepted model is edited later. It
        // is not a reason to rewrite an immutable historical assessment.
        ensure!(
            canonical_hash(&retained.assessment)? == canonical_hash(&result.assessment)?,
            "Final sensitivity assessment differs from its exact source evidence"
        );
    }
    session.result = session.finalized.then_some(result);
    Ok(session)
}

pub(crate) fn inspect_all_with_runs(
    root: &Path,
    project: &Project,
    runs: &mut impl FnMut(&energy::EnergyRunSummary) -> Result<energy::EnergyRunRecord>,
) -> Result<BTreeMap<String, EnergySensitivitySession>> {
    validate_references(references(project))?;
    let mut sessions = BTreeMap::new();
    for r in references(project) {
        crate::project_open::checkpoint(0)?;
        sessions.insert(r.id.clone(), inspect_with_runs(root, project, &r.id, runs)?);
    }
    Ok(sessions)
}

fn persist_event(
    root: &Path,
    project: &Project,
    id: &str,
    event: EnergySensitivityEvent,
) -> Result<ProjectView> {
    let r = reference(project, id)?;
    let (session, mut files) = read_files(root, project, r)?;
    ensure!(
        r.events.len() < MAX_EVENTS,
        "Sensitivity event bound exceeded"
    );
    let sequence = (r.events.len() + 1) as u8;
    let file = EventFile {
        schema_version: SCHEMA.into(),
        sequence,
        plan_sha256: r.plan_sha256.clone(),
        previous_file_sha256: r
            .events
            .last()
            .map(|e| e.file_sha256.clone())
            .unwrap_or_else(|| r.plan_file_sha256.clone()),
        recorded_utc: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
        event,
    };
    let bytes = {
        let mut b = serde_json::to_vec_pretty(&file)?;
        b.push(b'\n');
        b
    };
    let file_sha256 = persistence::bytes_hash(&bytes);
    let mut next_ref = r.clone();
    next_ref.events.push(EnergySensitivityEventSeal {
        sequence,
        file_sha256: file_sha256.clone(),
    });
    files.push(file.clone());
    reduce(&next_ref, session.plan, &files)?;
    let _directory = WriterLock::acquire(&root.join(&r.path))?;
    ensure!(
        write_new(&root.join(&r.path).join(event_name(sequence)), &file)? == file_sha256,
        "Unexpected sensitivity serialization"
    );
    let mut next = project.clone();
    let refs = &mut next
        .energy
        .as_mut()
        .unwrap()
        .workbench
        .as_mut()
        .unwrap()
        .sensitivity;
    *refs.iter_mut().find(|r| r.id == id).unwrap() = next_ref;
    crate::validate_project(&next)?;
    persistence::atomic_json(root, &next)?;
    crate::open_project(root)
}

/// Returns the guarded saved ProjectView with a new portable plan reference.
pub fn retain_plan(
    root: &Path,
    project: &Project,
    expected: &str,
    plan: sensitivity::EnergySensitivityPlan,
) -> Result<ProjectView> {
    current_plan(project, &plan)?;
    sensitivity::evaluate(root, project, &plan, &BTreeMap::new(), &[])?;
    ensure!(
        references(project).len() < MAX_SESSIONS,
        "Sensitivity session count exceeds32"
    );
    let _lock = WriterLock::acquire(root)?;
    persistence::check_revision(root, Some(expected))?;
    let targets = plan
        .declaration
        .targets
        .iter()
        .map(|target| target.alternative_id.clone())
        .collect::<Vec<_>>();
    let selected = sensitivity::plan(root, project, &targets, Some(&plan.current_inputs_sha256))?;
    ensure!(selected.declaration == plan.declaration && selected.slots == plan.slots,
        "Retain a freshly reviewed plan: exact reusable evidence changed or the supplied selection differs from the service's fixed earliest-run policy");
    let id = uuid::Uuid::new_v4().to_string();
    let path = format!("energy-assessments/{id}");
    let _parent = WriterLock::acquire(&root.join("energy-assessments"))?;
    fs::create_dir(root.join(&path))?;
    let _directory = WriterLock::acquire(&root.join(&path))?;
    let r = EnergySensitivityReference {
        id: id.clone(),
        path: path.clone(),
        plan_sha256: plan.identity()?,
        plan_file_sha256: write_new(
            &root.join(&path).join("plan.json"),
            &RetainedPlan {
                schema_version: SCHEMA.into(),
                project_id: project.id.clone(),
                plan,
            },
        )?,
        events: vec![],
    };
    let mut next = project.clone();
    next.energy
        .as_mut()
        .context("Energy document missing")?
        .workbench
        .get_or_insert_with(Default::default)
        .sensitivity
        .push(r);
    crate::validate_project(&next)?;
    persistence::atomic_json(root, &next)?;
    crate::open_project(root)
}
fn no_active_or_pending(app: &Application) -> Result<()> {
    if let Ok(status) = app.status() {
        ensure!(
            !matches!(
                status.state,
                JobState::Queued | JobState::Preparing | JobState::Running
            ) && !status.pending_evidence,
            "Resolve the existing single-worker job and pending evidence first"
        );
    }
    Ok(())
}
fn reply(
    root: &Path,
    view: ProjectView,
    id: &str,
    job: Option<JobStatus>,
) -> Result<EnergySensitivityStep> {
    let session = inspect(root, &view.project, id)?;
    Ok(EnergySensitivityStep { view, session, job })
}
/// One explicit launch only; the Application still owns cancellation/collection.
pub fn next(
    root: &Path,
    project: &Project,
    expected: &str,
    id: &str,
    app: &Application,
    engine_dir: Option<PathBuf>,
) -> Result<EnergySensitivityStep> {
    no_active_or_pending(app)?;
    let _lock = WriterLock::acquire(root)?;
    persistence::check_revision(root, Some(expected))?;
    let session = inspect(root, project, id)?;
    current_plan(project, &session.plan)?;
    ensure!(
        !session.finalized && session.active_slot_id.is_none() && session.interruptions.is_empty(),
        "This session is active, interrupted or final; no automatic retry is allowed"
    );
    let slot_id = session
        .next_slot_id
        .context("Every declared slot is already present; finalize the assessment")?;
    let slot = session
        .plan
        .declaration
        .slots
        .iter()
        .find(|s| s.id == slot_id)
        .unwrap();
    let (model, study) = session.plan.declaration.slot_inputs(&slot_id)?;
    let weather = project
        .energy
        .as_ref()
        .unwrap()
        .weather
        .iter()
        .find(|w| w.binding == study.weather)
        .context("Declared portable weather is missing")?;
    let (_, parsed) = energy_weather::open(&root.join(&weather.path))?;
    validate_weather_context(model, &study, &parsed.data)?;
    let reserved = persist_event(
        root,
        project,
        id,
        EnergySensitivityEvent::Reserved {
            slot_id: slot_id.clone(),
        },
    )?;
    let started = app.start_energy_for_project(
        model.clone(),
        study,
        root.join(&weather.path),
        slot.alternative_id.clone(),
        engine_dir,
        project.id.clone(),
    );
    match started {
        Ok(job) => {
            let saved = persist_event(
                root,
                &reserved.project,
                id,
                EnergySensitivityEvent::Started {
                    slot_id,
                    job_id: job.id.clone(),
                },
            );
            match saved {
                Ok(view) => reply(root, view, id, Some(job)),
                Err(error) => {
                    app.shutdown();
                    Err(error.context("Started worker was stopped after metadata persistence failed; reserved receipt remains and cannot be retried"))
                }
            }
        }
        Err(error) => {
            let view = persist_event(root, &reserved.project, id, EnergySensitivityEvent::Stopped { slot_id, job_id: None, interruption: EnergySensitivityInterruptionKind::Failed,
                reason: "Application rejected the reserved launch; no retry is permitted in this declaration.".into(), status_sha256: Some(canonical_hash(&format!("{error:#}"))?), process: None })?;
            reply(root, view, id, None)
        }
    }
}

fn failed_process(job_id: &str) -> Result<Option<ProcessOutcome>> {
    uuid(job_id)?;
    let root = crate::local_root()?.join("jobs").join(job_id);
    let path = root.join("engine/process.json");
    match fs::symlink_metadata(&path) {
        Ok(_) => Ok(Some(persistence::read_json(&regular_child(
            &root,
            "engine/process.json",
        )?)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
/// Call after ordinary collect succeeds, or after the normal worker fails or
/// cancels. It neither collects nor replaces Application pending evidence.
pub fn observe_after_collect(
    root: &Path,
    project: &Project,
    expected: &str,
    id: &str,
    app: &Application,
) -> Result<EnergySensitivityStep> {
    let _lock = WriterLock::acquire(root)?;
    persistence::check_revision(root, Some(expected))?;
    let session = inspect(root, project, id)?;
    let slot_id = session
        .active_slot_id
        .clone()
        .context("No attempted sensitivity slot awaits an outcome")?;
    let job_id = session.active_job_id.clone().context(
        "Reserved attempt lacks a persisted job ID; explicit interruption recovery is required",
    )?;
    let job = app.status().ok().filter(|job| job.id == job_id);
    let collected = || -> Result<EnergySensitivityEvent> {
        let summary = project
            .energy
            .as_ref()
            .unwrap()
            .runs
            .iter()
            .find(|r| r.id == job_id)
            .context("Collected sensitivity run is missing from project")?;
        let run = energy::read_run(root, summary)?;
        let mut completed = session.completed.clone();
        completed.insert(slot_id.clone(), job_id.clone());
        sensitivity::evaluate(root, project, &session.plan, &completed, &[])?;
        Ok(EnergySensitivityEvent::Collected {
            slot_id: slot_id.clone(),
            job_id: job_id.clone(),
            record_sha256: canonical_hash(&run)?,
            processes: run.provenance.processes,
        })
    };
    let event = match job.as_ref().map(|job| &job.state) {
        // After a restart, the original worker may no longer be in memory.
        // Only an already-collected exact retained run can close that attempt.
        None => collected()?,
        Some(JobState::Completed) => {
            ensure!(!job.as_ref().unwrap().pending_evidence, "Use ordinary collect before recording sensitivity completion");
            collected()?
        }
        Some(JobState::Failed | JobState::Cancelled) => EnergySensitivityEvent::Stopped { slot_id, job_id: Some(job_id.clone()),
            interruption: if job.as_ref().unwrap().state == JobState::Cancelled { EnergySensitivityInterruptionKind::Cancelled } else { EnergySensitivityInterruptionKind::Failed },
            reason: "Existing Application worker failed or was cancelled; this declaration cannot retry or execute remaining slots. The retained process receipt, when present, describes resource/cleanup status; absence does not attest that an engine ran.".into(),
            status_sha256: Some(canonical_hash(job.as_ref().unwrap())?), process: failed_process(&job_id)? },
        _ => anyhow::bail!("The normal worker is still active"),
    };
    let view = persist_event(root, project, id, event)?;
    reply(root, view, id, job)
}

/// Explicit fail-closed recovery only, after normal pending-job reconciliation
/// has been considered. This operation does not kill or restart any process.
pub fn mark_interrupted(
    root: &Path,
    project: &Project,
    expected: &str,
    id: &str,
    app: &Application,
) -> Result<ProjectView> {
    no_active_or_pending(app)?;
    let _lock = WriterLock::acquire(root)?;
    persistence::check_revision(root, Some(expected))?;
    let session = inspect(root, project, id)?;
    let slot_id = session.active_slot_id.context("No unresolved attempt")?;
    if let Some(job_id) = &session.active_job_id {
        ensure!(!project.energy.as_ref().unwrap().runs.iter().any(|r| &r.id == job_id), "A retained completed run exists; reconcile its outcome instead of calling it interrupted");
    }
    persist_event(root, project, id, EnergySensitivityEvent::Stopped { slot_id, job_id: session.active_job_id.clone(), interruption: EnergySensitivityInterruptionKind::Failed,
        reason: "Explicitly marked interrupted after normal job/pending-evidence review. Execution status is unresolved; no completed evidence or scientific pass is inferred and no retry is allowed.".into(),
        status_sha256: None, process: session.active_job_id.as_deref().map(failed_process).transpose()?.flatten() })
}
/// Repairs only a manifest commit interrupted after an immutable receipt was
/// written. Every old seal stays exact; all additional receipts must form the
/// same finite valid chain and all claimed results are fully reprocessed. This
/// imports no command, starts no process and never removes a failed attempt.
pub fn recover_metadata(
    root: &Path,
    project: &Project,
    expected: &str,
    id: &str,
) -> Result<ProjectView> {
    let _lock = WriterLock::acquire(root)?;
    persistence::check_revision(root, Some(expected))?;
    let r = reference(project, id)?;
    validate_references(std::slice::from_ref(r))?;
    let _: RetainedPlan =
        checked_json(root, &format!("{}/plan.json", r.path), &r.plan_file_sha256)?;
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(root.join(&r.path))? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("Invalid sensitivity member name"))?;
        if name != ".greenlens.lock" && name != "plan.json" {
            names.insert(name);
        }
    }
    ensure!(
        names.len() > r.events.len() && names.len() <= MAX_EVENTS,
        "No bounded interrupted receipt append is available"
    );
    let expected_names: BTreeSet<_> = (1..=names.len()).map(|n| event_name(n as u8)).collect();
    ensure!(
        names == expected_names,
        "Interrupted receipt recovery cannot skip, rename or discard members"
    );
    let mut recovered = r.clone();
    for sequence in (r.events.len() + 1)..=names.len() {
        let bytes = persistence::read_json_bytes(&regular_child(
            root,
            &format!("{}/{}", r.path, event_name(sequence as u8)),
        )?)?;
        recovered.events.push(EnergySensitivityEventSeal {
            sequence: sequence as u8,
            file_sha256: persistence::bytes_hash(&bytes),
        });
    }
    let mut next = project.clone();
    *next
        .energy
        .as_mut()
        .unwrap()
        .workbench
        .as_mut()
        .unwrap()
        .sensitivity
        .iter_mut()
        .find(|r| r.id == id)
        .unwrap() = recovered;
    inspect(root, &next, id)?;
    crate::validate_project(&next)?;
    persistence::atomic_json(root, &next)?;
    crate::open_project(root)
}
pub fn finalize(
    root: &Path,
    project: &Project,
    expected: &str,
    id: &str,
) -> Result<EnergySensitivityStep> {
    let _lock = WriterLock::acquire(root)?;
    persistence::check_revision(root, Some(expected))?;
    let session = inspect(root, project, id)?;
    ensure!(!session.finalized && session.active_slot_id.is_none() && session.next_slot_id.is_none(), "All declared attempts must be collected or the session explicitly stopped before finalizing");
    let result = sensitivity::evaluate(
        root,
        project,
        &session.plan,
        &session.completed,
        &session.interruptions,
    )?;
    let view = persist_event(
        root,
        project,
        id,
        EnergySensitivityEvent::Finalized { result },
    )?;
    reply(root, view, id, None)
}

/// Add these exact relative members to Save As/export, using existing bounded
/// copy/seal APIs and the package's unchanged aggregate limits. Empty projects
/// carrying references require a source directory just like retained runs.
pub fn evidence_members(root: &Path, project: &Project) -> Result<Vec<String>> {
    validate_references(references(project))?;
    let mut members = vec![];
    for r in references(project) {
        inspect(root, project, &r.id)?;
        members.push(format!("{}/plan.json", r.path));
        members.extend(
            r.events
                .iter()
                .map(|e| format!("{}/{}", r.path, event_name(e.sequence))),
        );
    }
    Ok(members)
}

/// Copy only independently verified, manifest-bound receipts. No scratch path
/// or engine is needed to inspect the copied assessment.
pub fn copy_evidence(source: &Path, destination: &Path, project: &Project) -> Result<()> {
    for name in evidence_members(source, project)? {
        let from = regular_child(source, &name)?;
        let bytes = persistence::read_json_bytes(&from)?;
        persistence::write_new_evidence_bytes(destination, &name, &bytes)?;
    }
    evidence_members(destination, project)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergySensitivityExportSelection {
    pub included_session_id: Option<String>,
    pub included_run_ids: Vec<String>,
    pub excluded_session_ids: Vec<String>,
    pub policy: String,
}
/// First retained finalized exact pair, regardless of pass/failure. Never select
/// an assessment by its numerical outcome. At most four runs enter a pair ZIP.
pub fn export_selection(
    root: &Path,
    project: &Project,
    a: &str,
    b: &str,
) -> Result<EnergySensitivityExportSelection> {
    // Validate every reference before selecting one, including excluded sessions.
    // The pure policy below is never an imported-evidence validation boundary.
    let sessions = references(project)
        .iter()
        .map(|r| {
            let session = inspect(root, project, &r.id)?;
            Ok((r.id.clone(), session.result))
        })
        .collect::<Result<Vec<_>>>()?;
    select_export_sessions(a, b, &sessions)
}
pub(crate) fn select_export_sessions(
    a: &str,
    b: &str,
    sessions: &[(String, Option<sensitivity::EnergySensitivityResult>)],
) -> Result<EnergySensitivityExportSelection> {
    let mut selection = EnergySensitivityExportSelection {
        included_session_id: None, included_run_ids: vec![a.into(), b.into()],
        excluded_session_ids: vec![],
        policy: "One explicit A/B pair plus its first retained finalized sensitivity assessment whose A10/B10 run IDs match exactly, including failed outcomes; other sessions remain in the saved project. At most four runs. Shared archive bounds still apply.".into(),
    };
    for (id, result) in sessions {
        let assessment = result.as_ref().map(|r| &r.assessment);
        let matches = assessment.is_some_and(|s| {
            s.runs.iter().any(|r| r.slot_id == "A10" && r.run_id == a)
                && s.runs.iter().any(|r| r.slot_id == "B10" && r.run_id == b)
        });
        if selection.included_session_id.is_none() && matches {
            selection.included_session_id = Some(id.clone());
            selection
                .included_run_ids
                .extend(assessment.unwrap().runs.iter().map(|r| r.run_id.clone()));
        } else {
            selection.excluded_session_ids.push(id.clone());
        }
    }
    selection.included_run_ids.sort();
    selection.included_run_ids.dedup();
    ensure!(
        selection.included_run_ids.len() <= 4,
        "Sensitivity pair exceeds four declared run bound"
    );
    Ok(selection)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (
        EnergySensitivityReference,
        sensitivity::EnergySensitivityPlan,
    ) {
        let report = synthetic_energy_report().unwrap();
        let mut fine = report.baseline.study.clone();
        fine.zone_timesteps_per_hour = 12;
        let declaration = declare_energy_sensitivity(
            vec![EnergySensitivityTarget {
                alternative_id: "A".into(),
                model: report.baseline.model,
                study_10min: report.baseline.study.clone(),
            }],
            greenlens_energyplus::method_identity(&report.baseline.study).unwrap(),
            greenlens_energyplus::method_identity(&fine).unwrap(),
        )
        .unwrap();
        let plan = sensitivity::EnergySensitivityPlan {
            schema_version: "greenlens.energy-sensitivity-plan.v1".into(),
            created_utc: "2026-01-01T00:00:00Z".into(),
            declaration_sha256: declaration.identity().unwrap(),
            current_inputs_sha256: "a".repeat(64),
            slots: declaration
                .slots
                .iter()
                .map(|s| sensitivity::EnergySensitivityPlannedSlot {
                    slot_id: s.id.clone(),
                    reuse: None,
                })
                .collect(),
            declaration,
            execution_authorized: false,
            reuse_policy: sensitivity::REUSE_POLICY.into(),
        };
        let id = "00000000-0000-4000-8000-000000000001".to_owned();
        let r = EnergySensitivityReference {
            path: format!("energy-assessments/{id}"),
            id,
            plan_sha256: plan.identity().unwrap(),
            plan_file_sha256: "b".repeat(64),
            events: vec![],
        };
        (r, plan)
    }
    fn append(
        r: &mut EnergySensitivityReference,
        files: &mut Vec<EventFile>,
        event: EnergySensitivityEvent,
    ) {
        let sequence = (files.len() + 1) as u8;
        files.push(EventFile {
            schema_version: SCHEMA.into(),
            sequence,
            plan_sha256: r.plan_sha256.clone(),
            previous_file_sha256: r
                .events
                .last()
                .map(|e| e.file_sha256.clone())
                .unwrap_or_else(|| r.plan_file_sha256.clone()),
            recorded_utc: "2026-01-01T01:00:00Z".into(),
            event,
        });
        r.events.push(EnergySensitivityEventSeal {
            sequence,
            file_sha256: format!("{:064x}", sequence),
        });
    }
    fn process() -> ProcessOutcome {
        ProcessOutcome {
            schema_version: "synthetic-state-transition-only".into(),
            executable: "not-executed.exe".into(),
            executable_sha256: "a".repeat(64),
            arguments: vec![],
            started_utc: "2026-01-01T01:00:00Z".into(),
            finished_utc: "2026-01-01T01:00:00Z".into(),
            elapsed_ms: 0,
            exit_code: Some(0),
            state: "completed".into(),
            failure: None,
            process_tree_empty: true,
            sampled_peak_runner_plus_job_working_set_bytes: 0,
            peak_observed_scratch_bytes: 0,
            samples: 0,
            stdout_sha256: "b".repeat(64),
            stderr_sha256: "c".repeat(64),
        }
    }
    #[test]
    fn tagged_native_receipts_roundtrip_without_changing_json_or_hashes() {
        let mut receipt = process();
        receipt.elapsed_ms = 21449;
        let events = [
            EnergySensitivityEvent::Collected {
                slot_id: "A5".into(),
                job_id: "00000000-0000-4000-8000-000000000020".into(),
                record_sha256: "e".repeat(64),
                processes: vec![receipt.clone()],
            },
            EnergySensitivityEvent::Stopped {
                slot_id: "B5".into(),
                job_id: None,
                interruption: EnergySensitivityInterruptionKind::Cancelled,
                reason: "Synthetic codec fixture".into(),
                status_sha256: None,
                process: Some(receipt),
            },
            EnergySensitivityEvent::Stopped {
                slot_id: "B5".into(),
                job_id: None,
                interruption: EnergySensitivityInterruptionKind::Failed,
                reason: "No engine receipt exists".into(),
                status_sha256: None,
                process: None,
            },
        ];
        for event in events {
            let bytes = serde_json::to_vec_pretty(&event).unwrap();
            let read: EnergySensitivityEvent = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(serde_json::to_vec_pretty(&read).unwrap(), bytes);
            assert_eq!(
                canonical_hash(&read).unwrap(),
                canonical_hash(&event).unwrap()
            );
            let mut bad = serde_json::to_value(&event).unwrap();
            if bad["kind"] == "collected" {
                bad["processes"][0]["elapsed_ms"] = serde_json::json!(-1);
                assert!(serde_json::from_value::<EnergySensitivityEvent>(bad).is_err());
            }
        }
    }
    #[test]
    fn completion_requires_the_reserved_job_and_successful_closed_process() {
        let (mut r, p) = fixture();
        let mut files = vec![];
        let job = "00000000-0000-4000-8000-000000000020".to_string();
        append(
            &mut r,
            &mut files,
            EnergySensitivityEvent::Reserved {
                slot_id: "A10".into(),
            },
        );
        append(
            &mut r,
            &mut files,
            EnergySensitivityEvent::Started {
                slot_id: "A10".into(),
                job_id: job.clone(),
            },
        );
        let event = EnergySensitivityEvent::Collected {
            slot_id: "A10".into(),
            job_id: job.clone(),
            record_sha256: "e".repeat(64),
            processes: vec![process()],
        };
        append(&mut r, &mut files, event);
        let accepted = reduce(&r, p.clone(), &files).unwrap();
        assert_eq!(accepted.next_slot_id.as_deref(), Some("A5"));
        assert_eq!(accepted.completed.get("A10"), Some(&job));
        // These are only reducer fixtures, never accepted as native evidence:
        // inspect additionally reprocesses the actual retained run and receipt.
        for mutation in 0..6 {
            let mut bad = files.clone();
            let EnergySensitivityEvent::Collected {
                job_id, processes, ..
            } = &mut bad[2].event
            else {
                unreachable!()
            };
            match mutation {
                0 => *job_id = "00000000-0000-4000-8000-000000000021".into(),
                1 => processes.clear(),
                2 => processes[0].exit_code = Some(1),
                3 => processes[0].process_tree_empty = false,
                4 => processes[0].state = "cancelled".into(),
                _ => processes[0].failure = Some("Retained native failure".into()),
            }
            assert!(reduce(&r, p.clone(), &bad).is_err());
        }
        let mut bad = files.clone();
        bad[1].previous_file_sha256 = "f".repeat(64);
        assert!(reduce(&r, p.clone(), &bad).is_err());
        let mut bad = files.clone();
        bad[2].recorded_utc = "2025-12-31T23:59:59Z".into();
        assert!(reduce(&r, p.clone(), &bad).is_err());
        let mut bad = files.clone();
        bad[2].recorded_utc = "2026-01-01T09:00:00+08:00".into();
        assert!(reduce(&r, p, &bad).is_err());
    }
    #[test]
    fn declared_reuse_is_skipped_and_cannot_be_replaced_by_an_attempt() {
        let (mut r, mut p) = fixture();
        p.slots[0].reuse = Some(sensitivity::EnergySensitivityReuse {
            run_id: "00000000-0000-4000-8000-000000000030".into(),
            record_sha256: "d".repeat(64),
            finished_utc: "2025-12-31T12:00:00Z".into(),
        });
        r.plan_sha256 = p.identity().unwrap();
        assert_eq!(
            reduce(&r, p.clone(), &[]).unwrap().next_slot_id.as_deref(),
            Some("A5")
        );
        let mut files = vec![];
        append(
            &mut r,
            &mut files,
            EnergySensitivityEvent::Reserved {
                slot_id: "A10".into(),
            },
        );
        assert!(reduce(&r, p, &files).is_err());
    }
    #[test]
    fn order_duplicate_attempts_and_cancelled_retries_are_rejected_without_launches() {
        let (mut r, p) = fixture();
        let mut files = vec![];
        append(
            &mut r,
            &mut files,
            EnergySensitivityEvent::Reserved {
                slot_id: "A5".into(),
            },
        );
        assert!(reduce(&r, p.clone(), &files).is_err());
        files.clear();
        r.events.clear();
        append(
            &mut r,
            &mut files,
            EnergySensitivityEvent::Reserved {
                slot_id: "A10".into(),
            },
        );
        assert_eq!(
            reduce(&r, p.clone(), &files)
                .unwrap()
                .active_slot_id
                .as_deref(),
            Some("A10")
        );
        append(
            &mut r,
            &mut files,
            EnergySensitivityEvent::Stopped {
                slot_id: "A10".into(),
                job_id: None,
                interruption: EnergySensitivityInterruptionKind::Cancelled,
                reason: "Synthetic cancellation transition only".into(),
                status_sha256: None,
                process: None,
            },
        );
        let stopped = reduce(&r, p.clone(), &files).unwrap();
        assert!(stopped.next_slot_id.is_none());
        assert_eq!(stopped.interruptions.len(), 1);
        append(
            &mut r,
            &mut files,
            EnergySensitivityEvent::Reserved {
                slot_id: "A10".into(),
            },
        );
        assert!(reduce(&r, p, &files).is_err());
    }
    #[test]
    fn imported_plan_cannot_bind_different_model_to_a_current_hash() {
        let mut project = crate::energy_services::example_document();
        let (_, mut plan) = fixture();
        let target = &plan.declaration.targets[0];
        let energy = project.energy.as_mut().unwrap();
        energy.designs.get_mut("A").unwrap().model = target.model.clone();
        energy.designs.get_mut("A").unwrap().study = Some(target.study_10min.clone());
        let weather_id = "00000000-0000-4000-8000-000000000009".to_string();
        energy.weather.push(energy::EnergyWeatherReference {
            path: format!("weather/{weather_id}"),
            id: weather_id,
            binding: target.study_10min.weather.clone(),
        });
        plan.current_inputs_sha256 =
            sensitivity::current_inputs_identity(&project, &["A".into()]).unwrap();
        assert!(current_plan(&project, &plan).is_ok());
        // A forged current hash cannot substitute a different valid snapshot.
        // The source project itself is not changed by declaration inspection.
        let original = canonical_hash(&project).unwrap();
        let mut changed = plan.declaration.targets.clone();
        changed[0].model.zones[0]
            .cooling_system
            .as_mut()
            .unwrap()
            .supply_fan
            .total_efficiency = 0.7;
        let ten = plan.declaration.slots[0].method.clone();
        let five = plan.declaration.slots[1].method.clone();
        plan.declaration = declare_energy_sensitivity(changed, ten, five).unwrap();
        plan.declaration_sha256 = plan.declaration.identity().unwrap();
        assert!(current_plan(&project, &plan).is_err());
        assert_eq!(canonical_hash(&project).unwrap(), original);
        let (mut r, _) = fixture();
        r.path = "../outside".into();
        assert!(validate_references(&[r]).is_err());
    }
    #[test]
    fn exact_receipt_bytes_and_extra_unreferenced_members_fail_closed() {
        let (mut r, p) = fixture();
        let mut project = crate::energy_services::example_document();
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join(&r.path)).unwrap();
        r.plan_file_sha256 = write_new(
            &tmp.path().join(&r.path).join("plan.json"),
            &RetainedPlan {
                schema_version: SCHEMA.into(),
                project_id: project.id.clone(),
                plan: p,
            },
        )
        .unwrap();
        project
            .energy
            .as_mut()
            .unwrap()
            .workbench
            .get_or_insert_with(Default::default)
            .sensitivity
            .push(r.clone());
        assert!(read_files(tmp.path(), &project, &r).is_ok());
        fs::write(tmp.path().join(&r.path).join("event-001.json"), b"{}").unwrap();
        assert!(read_files(tmp.path(), &project, &r).is_err());
        fs::remove_file(tmp.path().join(&r.path).join("event-001.json")).unwrap();
        let path = tmp.path().join(&r.path).join("plan.json");
        let mut bytes = fs::read(&path).unwrap();
        bytes.push(b' ');
        fs::write(path, bytes).unwrap();
        assert!(read_files(tmp.path(), &project, &r).is_err());
    }
}
