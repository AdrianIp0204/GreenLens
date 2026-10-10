//! Shared application services for the CLI and native desktop.
pub mod persistence;
mod report;

use anyhow::{bail, ensure, Context, Result};
use chrono::Utc;
use greenlens_core::{canonical_hash, statistics, validate_model, Model, ValidatedModel};
use greenlens_radiance::{Engine, Profile};
use persistence::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};
use uuid::Uuid;

pub const SCHEMA_VERSION: &str = "0.1.0";
pub const CALCULATION_LABEL: &str =
    "Static daylight illuminance under a prescribed CIE standard overcast sky.";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Alternative {
    pub id: String,
    pub model: Model,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub schema_version: String,
    pub id: String,
    pub name: String,
    pub alternatives: Vec<Alternative>,
    pub runs: Vec<RunSummary>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunSummary {
    pub id: String,
    pub alternative_id: String,
    pub path: String,
    pub scientific_input_hash: String,
    pub comparison_fingerprint: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: String,
    pub alternative_id: String,
    pub model: Model,
    pub validated: ValidatedModel,
    pub lux: Vec<f64>,
    pub statistics: Value,
    pub provenance: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectView {
    pub project: Project,
    pub revision: String,
    pub results: Vec<RunRecord>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comparison {
    pub a_id: String,
    pub b_id: String,
    pub deltas_lux: Vec<f64>,
    pub statistics: Value,
    pub scale_max_lux: f64,
    pub conditions: Value,
    pub design_differences: Value,
}

pub fn example() -> Project {
    Project {
        schema_version: SCHEMA_VERSION.into(),
        id: Uuid::new_v4().to_string(),
        name: "Synthetic daylight room".into(),
        alternatives: vec![
            Alternative {
                id: "A".into(),
                model: Model::default(),
            },
            Alternative {
                id: "B".into(),
                model: Model::alternative_b(),
            },
        ],
        runs: vec![],
    }
}
fn validate_project(project: &Project) -> Result<()> {
    ensure!(
        project.schema_version == SCHEMA_VERSION,
        "Unsupported project schema"
    );
    Uuid::parse_str(&project.id)?;
    ensure!(
        !project.name.trim().is_empty() && project.name.len() <= 200,
        "Project name must contain 1–200 characters"
    );
    ensure!(
        project.alternatives.len() == 2,
        "v0.1 requires alternatives A and B"
    );
    for id in ["A", "B"] {
        let a = project
            .alternatives
            .iter()
            .filter(|a| a.id == id)
            .collect::<Vec<_>>();
        ensure!(a.len() == 1, "Expected exactly one alternative {id}");
        validate_model(&a[0].model)?;
    }
    let mut ids = std::collections::HashSet::new();
    for run in &project.runs {
        Uuid::parse_str(&run.id)?;
        ensure!(ids.insert(&run.id), "Duplicate run id");
        ensure!(
            run.path == format!("runs/{}", run.id),
            "Invalid run reference"
        );
        ensure!(
            ["A", "B"].contains(&run.alternative_id.as_str()),
            "Unknown alternative"
        );
    }
    Ok(())
}
pub fn local_root() -> Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_CACHE_HOME"))
        .context("LOCALAPPDATA must be configured")?;
    Ok(PathBuf::from(base).join("GreenLens"))
}
/// Exercise the actual engine and child library lookup without saving a project.
pub fn doctor(engine_dir: Option<&Path>) -> Result<Value> {
    let engine = Engine::discover(engine_dir)?;
    let scratch = local_root()?
        .join("diagnostics")
        .join(Uuid::new_v4().to_string());
    let output = greenlens_radiance::diagnose(&engine, &scratch, &AtomicBool::new(false))?;
    Ok(
        json!({"application_version":env!("CARGO_PKG_VERSION"),"engine":engine.identity(),"status":"Native gensky, oconv and rtrace completed with child library lookup","diagnostic_illuminance_lux":output.lux[0],"processes":output.processes,"calculation":CALCULATION_LABEL,"limitation":"Runtime diagnostic; required scientific acceptance is separately recorded"}),
    )
}
pub fn validate_record(record: &RunRecord) -> Result<()> {
    Uuid::parse_str(&record.id)?;
    ensure!(
        ["A", "B"].contains(&record.alternative_id.as_str()),
        "Unknown alternative"
    );
    let validated = validate_model(&record.model)?;
    ensure!(
        canonical_hash(&validated)? == canonical_hash(&record.validated)?,
        "Stored validated model differs from authoritative inputs"
    );
    greenlens_core::validate_lux(&record.lux, validated.sensor_grid.sensors.len())?;
    ensure!(
        serde_json::to_value(statistics(&record.lux)?)? == record.statistics,
        "Stored statistics mismatch"
    );
    let p = &record.provenance;
    ensure!(
        p["state"] == "completed" && p["units"] == "lx" && p["schema_version"] == SCHEMA_VERSION,
        "Incomplete run provenance"
    );
    ensure!(
        p["input_hash"] == validated.normalized_input_hash
            && p["sensor_hash"] == validated.sensor_hash,
        "Run input/sensor provenance mismatch"
    );
    ensure!(
        p["engine"].is_object()
            && p["profile"].is_object()
            && p["started_utc"].is_string()
            && p["finished_utc"].is_string(),
        "Incomplete engine/profile/time provenance"
    );
    let identity: greenlens_radiance::EngineIdentity = serde_json::from_value(p["engine"].clone())?;
    ensure!(
        identity.version == greenlens_radiance::ENGINE_VERSION
            && identity.executable_sha256.len() == 3
            && !identity.library_sha256.is_empty(),
        "Incomplete engine identity"
    );
    let profile: Profile = serde_json::from_value(p["profile"].clone())?;
    profile.validate()?;
    let start = chrono::DateTime::parse_from_rfc3339(p["started_utc"].as_str().unwrap())?;
    let finish = chrono::DateTime::parse_from_rfc3339(p["finished_utc"].as_str().unwrap())?;
    ensure!(
        start.offset().local_minus_utc() == 0
            && finish.offset().local_minus_utc() == 0
            && finish >= start,
        "Invalid UTC run timestamps"
    );
    let processes: Vec<greenlens_radiance::ProcessRecord> =
        serde_json::from_value(p["processes"].clone())?;
    ensure!(
        processes.len() == 3
            && processes
                .iter()
                .zip(["gensky.exe", "oconv.exe", "rtrace.exe"])
                .all(|(p, exe)| p.executable == exe && p.exit_code == 0),
        "Incomplete or failed process provenance"
    );
    ensure!(
        processes[2].arguments == profile.arguments(),
        "Numerical arguments differ from profile"
    );
    let fingerprint = canonical_hash(
        &json!({"conditions":validated.comparison_conditions_hash,"engine":p["engine"],"profile":p["profile"]}),
    )?;
    ensure!(
        p["comparison_fingerprint"] == fingerprint,
        "Comparison fingerprint mismatch"
    );
    let full = canonical_hash(
        &json!({"input":validated.scientific_input_hash,"engine":p["engine"],"profile":p["profile"]}),
    )?;
    ensure!(p["run_fingerprint"] == full, "Run fingerprint mismatch");
    Ok(())
}
fn read_run(root: &Path, summary: &RunSummary) -> Result<RunRecord> {
    let relative = relative_path(&summary.path)?;
    let dir = root.join(relative);
    // Check each ancestor through the regular child helper before reading.
    regular_child(root, &format!("{}/checksums.json", summary.path))?;
    let sums = verify_seal(&dir)?;
    for required in [
        "input.json",
        "results.json",
        "results.csv",
        "provenance.json",
        "scene.rad",
        "sky.rad",
        "sensors.pts",
        "raw.rgb",
        "logs.json",
        "recipe.json",
    ] {
        ensure!(
            sums.sha256.contains_key(required),
            "Missing required evidence {required}"
        );
    }
    let run: RunRecord = read_json(&dir.join("results.json"))?;
    validate_record(&run)?;
    ensure!(
        run.id == summary.id && run.alternative_id == summary.alternative_id,
        "Run manifest identity mismatch"
    );
    ensure!(
        run.validated.scientific_input_hash == summary.scientific_input_hash
            && run.provenance["comparison_fingerprint"] == summary.comparison_fingerprint,
        "Run manifest fingerprint mismatch"
    );
    ensure!(
        serde_json::to_value(&run.model)? == read_json::<Value>(&dir.join("input.json"))?,
        "Input snapshot mismatch"
    );
    ensure!(
        run.provenance == read_json::<Value>(&dir.join("provenance.json"))?,
        "Provenance snapshot mismatch"
    );
    ensure!(
        run.provenance["processes"] == read_json::<Value>(&dir.join("logs.json"))?,
        "Process log mismatch"
    );
    let raw =
        greenlens_radiance::parse_rgb(&fs::read_to_string(dir.join("raw.rgb"))?, run.lux.len())?;
    for (rgb, value) in raw.iter().zip(&run.lux) {
        ensure!(
            (greenlens_core::rgb_to_lux(*rgb)? - value).abs() <= 1e-8,
            "Raw irradiance and processed lux disagree"
        );
    }
    Ok(run)
}
pub fn open_project(path: &Path) -> Result<ProjectView> {
    let manifest = regular_child(path, "project.json")?;
    ensure!(
        fs::metadata(&manifest)?.len() <= 64 * 1024 * 1024,
        "Project manifest too large"
    );
    let bytes = fs::read(manifest)?;
    let revision = bytes_hash(&bytes);
    let project: Project = serde_json::from_slice(&bytes)?;
    validate_project(&project)?;
    let results = project
        .runs
        .iter()
        .map(|s| read_run(path, s))
        .collect::<Result<Vec<_>>>()?;
    Ok(ProjectView {
        project,
        revision,
        results,
    })
}
pub fn save_project(path: &Path, project: &Project, expected: Option<&str>) -> Result<ProjectView> {
    validate_project(project)?;
    let _lock = WriterLock::acquire(path)?;
    check_revision(path, expected)?;
    for summary in &project.runs {
        read_run(path, summary)?;
    }
    check_revision(path, expected)?;
    atomic_json(path, project)?;
    open_project(path)
}
pub fn is_stale(run: &RunRecord, current: &Model) -> Result<bool> {
    Ok(validate_model(current)?.scientific_input_hash != run.validated.scientific_input_hash)
}
pub fn compare_runs(a: &RunRecord, b: &RunRecord) -> Result<Comparison> {
    validate_record(a)?;
    validate_record(b)?;
    ensure!(a.provenance["comparison_fingerprint"]==b.provenance["comparison_fingerprint"],"Incompatible comparison: room, sensors, environment, materials, engine or settings differ; no resampling is performed");
    ensure!(
        a.validated.sensor_hash == b.validated.sensor_hash,
        "World sensor definitions differ"
    );
    let deltas: Vec<f64> = a.lux.iter().zip(&b.lux).map(|(x, y)| y - x).collect();
    Ok(Comparison {
        a_id: a.id.clone(),
        b_id: b.id.clone(),
        statistics: serde_json::to_value(statistics(&deltas)?)?,
        deltas_lux: deltas,
        scale_max_lux: a.lux.iter().chain(&b.lux).copied().fold(0.0, f64::max),
        conditions: json!({"fingerprint":a.provenance["comparison_fingerprint"],"sensor_hash":a.validated.sensor_hash,"delta":"B minus A (lx)","percentage_delta":"omitted"}),
        design_differences: json!({"a":{"window":a.model.window,"glazing":a.model.materials.visible_transmittance,"overhang":a.model.overhang},"b":{"window":b.model.window,"glazing":b.model.materials.visible_transmittance,"overhang":b.model.overhang}}),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Preparing,
    Running,
    Completed,
    Failed,
    Cancelled,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStatus {
    pub id: String,
    pub state: JobState,
    pub log: Vec<String>,
    pub error: Option<String>,
    pub result: Option<RunRecord>,
}
struct ActiveJob {
    status: Arc<Mutex<JobStatus>>,
    cancel: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    scratch: PathBuf,
}
#[derive(Default)]
pub struct Application {
    active: Mutex<Option<ActiveJob>>,
}
impl Application {
    pub fn start(
        &self,
        model: Model,
        alternative: String,
        engine_dir: Option<PathBuf>,
        profile: Profile,
    ) -> Result<JobStatus> {
        validate_model(&model)?;
        ensure!(
            ["A", "B"].contains(&alternative.as_str()),
            "Unknown alternative"
        );
        let mut active = self
            .active
            .lock()
            .map_err(|_| anyhow::anyhow!("Job mutex poisoned"))?;
        if let Some(previous) = active.as_ref() {
            ensure!(
                !matches!(
                    previous.status.lock().unwrap().state,
                    JobState::Queued | JobState::Preparing | JobState::Running
                ),
                "A simulation job is already active"
            );
        }
        if let Some(mut previous) = active.take() {
            if let Some(h) = previous.handle.take() {
                let _ = h.join();
            }
        }
        let id = Uuid::new_v4().to_string();
        let scratch = local_root()?.join("jobs").join(&id);
        fs::create_dir_all(&scratch)?;
        let initial = JobStatus {
            id: id.clone(),
            state: JobState::Queued,
            log: vec!["Job queued".into()],
            error: None,
            result: None,
        };
        let status = Arc::new(Mutex::new(initial.clone()));
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_status = status.clone();
        let worker_cancel = cancel.clone();
        let worker_scratch = scratch.clone();
        let handle = thread::spawn(move || {
            {
                let mut s = worker_status.lock().unwrap();
                s.state = JobState::Preparing;
                s.log.push("Validating inputs and engine".into());
            }
            let result = run_job(
                &id,
                model,
                alternative,
                engine_dir.as_deref(),
                profile,
                &worker_scratch,
                &worker_cancel,
                &|line| {
                    let mut s = worker_status.lock().unwrap();
                    s.state = JobState::Running;
                    if s.log.len() < 2000 {
                        s.log.push(line.into());
                    }
                },
            );
            let mut s = worker_status.lock().unwrap();
            if worker_cancel.load(Ordering::SeqCst) {
                s.state = JobState::Cancelled;
                s.result = None;
                s.log
                    .push("Cancelled; no completed result published".into());
            } else {
                match result {
                    Ok(run) => {
                        s.state = JobState::Completed;
                        s.result = Some(run);
                        s.log
                            .push("Validated completed result is ready to save".into());
                    }
                    Err(e) => {
                        s.state = JobState::Failed;
                        s.error = Some(format!("{e:#}"));
                        s.log
                            .push("Run failed; no completed result published".into());
                    }
                }
            }
        });
        *active = Some(ActiveJob {
            status,
            cancel,
            handle: Some(handle),
            scratch,
        });
        Ok(initial)
    }
    pub fn status(&self) -> Result<JobStatus> {
        let active = self.active.lock().unwrap();
        let job = active.as_ref().context("No simulation has been started")?;
        let status = job.status.lock().unwrap().clone();
        Ok(status)
    }
    pub fn cancel(&self) -> Result<JobStatus> {
        {
            let active = self.active.lock().unwrap();
            let job = active.as_ref().context("No active simulation")?;
            let s = job.status.lock().unwrap();
            if matches!(
                s.state,
                JobState::Queued | JobState::Preparing | JobState::Running
            ) {
                job.cancel.store(true, Ordering::SeqCst);
            }
        }
        self.status()
    }
    pub fn wait(&self) -> Result<JobStatus> {
        let handle = {
            self.active
                .lock()
                .unwrap()
                .as_mut()
                .context("No job")?
                .handle
                .take()
        };
        if let Some(handle) = handle {
            handle
                .join()
                .map_err(|_| anyhow::anyhow!("Simulation worker panicked"))?;
        }
        self.status()
    }
    pub fn shutdown(&self) {
        let _ = self.cancel();
        let _ = self.wait();
    }
    pub fn collect(
        &self,
        path: &Path,
        project: &Project,
        expected: Option<&str>,
    ) -> Result<ProjectView> {
        let active = self.active.lock().unwrap();
        let job = active.as_ref().context("No completed job")?;
        let status = job.status.lock().unwrap();
        ensure!(
            status.state == JobState::Completed,
            "Job has not completed successfully"
        );
        let run = status.result.as_ref().context("Missing result")?;
        validate_record(run)?;
        validate_project(project)?;
        let _lock = WriterLock::acquire(path)?;
        check_revision(path, expected)?;
        for old in &project.runs {
            read_run(path, old)?;
        }
        promote_run(&job.scratch, path, &run.id)?;
        let mut project = project.clone();
        if !project.runs.iter().any(|s| s.id == run.id) {
            project.runs.push(RunSummary {
                id: run.id.clone(),
                alternative_id: run.alternative_id.clone(),
                path: format!("runs/{}", run.id),
                scientific_input_hash: run.validated.scientific_input_hash.clone(),
                comparison_fingerprint: run.provenance["comparison_fingerprint"]
                    .as_str()
                    .context("Missing fingerprint")?
                    .into(),
            });
        }
        check_revision(path, expected)?;
        atomic_json(path, &project)?;
        open_project(path)
    }
}
impl Drop for Application {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// Keep the immutable run inputs and the explicit cancellation/logging context
// visible together at this single private worker boundary.
#[allow(clippy::too_many_arguments)]
fn run_job(
    id: &str,
    model: Model,
    alternative: String,
    engine_dir: Option<&Path>,
    profile: Profile,
    scratch: &Path,
    cancel: &AtomicBool,
    log: &dyn Fn(&str),
) -> Result<RunRecord> {
    let started = Utc::now().to_rfc3339();
    let validated = validate_model(&model)?;
    ensure!(!cancel.load(Ordering::SeqCst), "Cancelled");
    let engine = Engine::discover(engine_dir)?;
    let output = greenlens_radiance::simulate(&engine, &validated, &profile, scratch, cancel, log)?;
    ensure!(!cancel.load(Ordering::SeqCst), "Cancelled");
    let identity = serde_json::to_value(engine.identity())?;
    let profile = serde_json::to_value(profile)?;
    let provenance = json!({"schema_version":SCHEMA_VERSION,"application_version":env!("CARGO_PKG_VERSION"),"source_commit":option_env!("GREENLENS_SOURCE_COMMIT"),"adapter_version":env!("CARGO_PKG_VERSION"),"calculation_label":CALCULATION_LABEL,"state":"completed","units":"lx","started_utc":started,"finished_utc":Utc::now().to_rfc3339(),"engine":identity,"profile":profile,"input_hash":validated.normalized_input_hash,"sensor_hash":validated.sensor_hash,"run_fingerprint":canonical_hash(&json!({"input":validated.scientific_input_hash,"engine":identity,"profile":profile}))?,"comparison_fingerprint":canonical_hash(&json!({"conditions":validated.comparison_conditions_hash,"engine":identity,"profile":profile}))?,"child_environment":{"RAYPATH":".;<compatible-engine>/lib"},"recipe":"Regenerate scene and octree from input.json with the pinned adapter; never execute package-supplied commands"});
    let mut provenance = provenance;
    provenance["source_worktree_dirty"] =
        json!(option_env!("GREENLENS_SOURCE_DIRTY").map(|value| value == "true"));
    provenance["processes"] = serde_json::to_value(&output.processes)?;
    provenance["adapter_version"] = json!(greenlens_radiance::ADAPTER_VERSION);
    let run = RunRecord {
        id: id.into(),
        alternative_id: alternative,
        model,
        validated,
        statistics: serde_json::to_value(statistics(&output.lux)?)?,
        lux: output.lux,
        provenance,
    };
    validate_record(&run)?;
    write_json(&scratch.join("input.json"), &run.model)?;
    write_json(&scratch.join("results.json"), &run)?;
    write_json(&scratch.join("provenance.json"), &run.provenance)?;
    let mut csv = String::from("sensor_id,x_m,y_m,z_m,nx,ny,nz,illuminance_lux\n");
    for (sensor, lux) in run.validated.sensor_grid.sensors.iter().zip(&run.lux) {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            sensor.id,
            sensor.position[0],
            sensor.position[1],
            sensor.position[2],
            sensor.direction[0],
            sensor.direction[1],
            sensor.direction[2],
            lux
        ));
    }
    fs::write(scratch.join("results.csv"), csv)?;
    let mut files = vec![
        "input.json",
        "results.json",
        "results.csv",
        "provenance.json",
        "scene.rad",
        "sky.rad",
        "sensors.pts",
        "raw.rgb",
    ];
    for optional in ["processes.json", "recipe.json", "logs.json"] {
        if scratch.join(optional).is_file() {
            files.push(optional);
        }
    }
    seal(scratch, &files)?;
    Ok(run)
}

pub fn export_project(path: &Path, destination: &Path) -> Result<()> {
    let _lock = WriterLock::acquire(path)?;
    let view = open_project(path)?;
    ensure!(
        !view.results.is_empty(),
        "Run a calculation before exporting evidence"
    );
    let staging = local_root()?
        .join("exports")
        .join(Uuid::new_v4().to_string());
    fs::create_dir_all(&staging)?;
    write_json(&staging.join("project.json"), &view.project)?;
    let mut files = vec!["project.json".to_string()];
    for run in &view.project.runs {
        let source = path.join(&run.path);
        promote_run(&source, &staging, &run.id)?;
        let sums = verify_seal(&source)?;
        for name in sums.sha256.keys() {
            files.push(format!("{}/{name}", run.path));
        }
        files.push(format!("{}/checksums.json", run.path));
    }
    let comparisons: Vec<Comparison> = view
        .results
        .iter()
        .filter(|a| a.alternative_id == "A")
        .flat_map(|a| {
            view.results
                .iter()
                .filter(|b| b.alternative_id == "B")
                .filter_map(move |b| compare_runs(a, b).ok())
        })
        .collect();
    write_json(&staging.join("comparisons.json"), &comparisons)?;
    files.push("comparisons.json".into());
    fs::write(
        staging.join("report.html"),
        report::render(&view, &comparisons)?,
    )?;
    files.push("report.html".into());
    fs::write(staging.join("REPLAY.txt"),"GreenLens 0.1 evidence\nInstall the compatible pinned Radiance 6.0.2 engine separately.\nVerify and replay: greenlens replay evidence.zip --engine-dir <engine-directory>\nReplay validates path containment, checksums, normalized inputs and engine/profile identity; generates a new disposable octree and reports per-sensor tolerance max(0.1 lx, 0.1% reference).\nNo embedded shell commands are executed. Engine binaries are deliberately excluded.\nOpen report.html locally; no network is required. See project.json and immutable runs for complete inputs.\nIntegration verification is not independent physical validation or certification.\n")?;
    files.push("REPLAY.txt".into());
    seal(
        &staging,
        &files.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    files.push("checksums.json".into());
    zip_directory(&staging, &files, destination)
}

pub fn replay(package: &Path, engine_dir: Option<&Path>) -> Result<Value> {
    let root = local_root()?
        .join("replay")
        .join(Uuid::new_v4().to_string());
    extract_package(package, &root)?;
    let view = open_project(&root)?;
    ensure!(
        !view.results.is_empty(),
        "Evidence package contains no completed runs to replay"
    );
    let engine = Engine::discover(engine_dir)?;
    let mut checks = vec![];
    for run in view.results {
        ensure!(
            serde_json::to_value(engine.identity())? == run.provenance["engine"],
            "Replay requires matching engine/library identity"
        );
        let profile: Profile = serde_json::from_value(run.provenance["profile"].clone())?;
        let scratch = local_root()?.join("jobs").join(Uuid::new_v4().to_string());
        fs::create_dir_all(&scratch)?;
        let output = greenlens_radiance::simulate(
            &engine,
            &validate_model(&run.model)?,
            &profile,
            &scratch,
            &AtomicBool::new(false),
            |_: &str| {},
        )?;
        ensure!(output.lux.len() == run.lux.len(), "Replay count mismatch");
        let max_error = output
            .lux
            .iter()
            .zip(&run.lux)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        let passed = output
            .lux
            .iter()
            .zip(&run.lux)
            .all(|(a, b)| (a - b).abs() <= 0.1_f64.max(b.abs() * 0.001));
        checks.push(json!({"run_id":run.id,"sensors":run.lux.len(),"max_absolute_error_lux":max_error,"passed":passed}));
        if !passed {
            bail!(
                "Replay integration tolerance failed: {}",
                serde_json::to_string(&checks)?
            );
        }
    }
    Ok(
        json!({"verification":"same-engine integration, not independent physical validation","checks":checks,"passed":true}),
    )
}
