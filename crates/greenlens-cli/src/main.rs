mod energy;
mod report_issue;
use anyhow::{ensure, Context, Result};
use clap::{Parser, Subcommand};
use greenlens_core::{validate_model, Model};
use greenlens_radiance::Profile;
use greenlens_runner::{persistence::read_json, Application, JobState};
use std::{path::PathBuf, sync::Arc};

#[derive(Parser)]
#[command(
    version,
    about = "Local daylight and building-energy studies with reproducible evidence."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Immutable scope/evidence-bound report issuance and portable regeneration.
    ReportIssue {
        #[command(subcommand)]
        command: report_issue::ReportIssueCommand,
    },
    /// Read-only evidence admission and one cancellable verified snapshot.
    Open {
        project: PathBuf,
        #[arg(long)]
        admission_only: bool,
    },
    /// Bounded local GLB intake; display geometry remains separate from thermal inputs.
    Reference {
        #[command(subcommand)]
        command: ReferenceCommand,
    },
    /// Thermal building, annual EnergyPlus and reproducible assessment reports.
    Energy {
        #[command(subcommand)]
        command: energy::EnergyCommand,
    },
    /// Configure/run a versioned study using the shared application service.
    Study {
        #[command(subcommand)]
        command: StudyCommand,
    },
    /// Inspect one retained hour or one sensor, without loading a matrix into JSON.
    Result {
        project: PathBuf,
        run: String,
        /// Zero-based weather interval (0..8759), also for a selected-hour run.
        #[arg(long, conflicts_with = "sensor")]
        hour: Option<usize>,
        #[arg(long, conflicts_with = "hour")]
        sensor: Option<String>,
    },
    /// Inspect/import local EPW without network access or an installed engine.
    Weather {
        #[command(subcommand)]
        command: WeatherCommand,
    },
    Doctor {
        #[arg(long)]
        engine_dir: Option<PathBuf>,
    },
    Validate {
        input: PathBuf,
    },
    Example {
        project: PathBuf,
    },
    /// Save As a complete integrity-checked portable copy, including immutable run evidence.
    Copy {
        project: PathBuf,
        destination: PathBuf,
    },
    Run {
        input: PathBuf,
        #[arg(long)]
        project: PathBuf,
        #[arg(long, default_value = "A")]
        alternative: String,
        #[arg(long)]
        engine_dir: Option<PathBuf>,
        #[arg(long)]
        reference: bool,
    },
    Compare {
        project: PathBuf,
        #[arg(long)]
        a: Option<String>,
        #[arg(long)]
        b: Option<String>,
    },
    Export {
        project: PathBuf,
        destination: PathBuf,
        /// Repeat for each immutable snapshot; required for climate projects.
        #[arg(long = "run")]
        runs: Vec<String>,
    },
    Replay {
        package: PathBuf,
        #[arg(long)]
        engine_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ReferenceCommand {
    Import {
        project: PathBuf,
        file: PathBuf,
        #[arg(long)]
        options: Option<PathBuf>,
    },
    Inspect {
        project: PathBuf,
    },
    /// Inspect valid analytical targets without inferring thermal properties.
    ScopeContext {
        project: PathBuf,
    },
    /// Validate a declared mapping against exact admitted and accepted inputs.
    ScopeAssess {
        project: PathBuf,
        scope: PathBuf,
    },
    /// Accept one derived scope transaction and save with conflict protection.
    ScopeApply {
        project: PathBuf,
        scope: PathBuf,
    },
    /// List immutable scope revisions, or inspect one complete exact snapshot.
    Revisions {
        project: PathBuf,
        #[arg(long)]
        revision: Option<String>,
    },
    /// Prepare a carry-forward review against current exact reference/design inputs.
    ScopeReview {
        project: PathBuf,
        #[arg(long)]
        parent: String,
    },
    /// Accept a recomputed review and complete scope; no implicit input rebinding.
    ScopeAccept {
        project: PathBuf,
        #[arg(long)]
        parent: String,
        #[arg(long)]
        plan: String,
        /// Complete candidate ArchitecturalScope JSON from the reviewed proposal.
        #[arg(long)]
        scope: PathBuf,
        /// Array of explicit row_id/resolution/rationale review decisions.
        #[arg(long)]
        decisions: PathBuf,
    },
    /// Activate an exact retained revision without deleting or rewriting history.
    ScopeActivate {
        project: PathBuf,
        #[arg(long)]
        revision: String,
    },
}

#[derive(clap::ValueEnum, Clone, Copy)]
enum WeatherProfile {
    EpwHourEnding,
    PvgisTmy,
}
impl From<WeatherProfile> for greenlens_core::weather::WeatherSourceProfile {
    fn from(value: WeatherProfile) -> Self {
        match value {
            WeatherProfile::EpwHourEnding => Self::EpwHourEnding,
            WeatherProfile::PvgisTmy => Self::PvgisTmy,
        }
    }
}
#[derive(Subcommand)]
enum WeatherCommand {
    /// Find an actual retained local-standard interval; EPW hours end at 1–24.
    FindHour {
        project: PathBuf,
        weather_id: String,
        #[arg(long)]
        month: u8,
        #[arg(long)]
        day: u8,
        #[arg(long)]
        hour_ending: u8,
    },
    /// Write original deterministic, explicitly non-climatic test weather.
    Synthetic {
        destination: PathBuf,
    },
    /// Copy an integrity-checked weather asset into a saved portable project.
    Attach {
        project: PathBuf,
        asset: PathBuf,
    },
    Inspect {
        input: PathBuf,
        #[arg(long, value_enum)]
        source_profile: WeatherProfile,
    },
    Import {
        input: PathBuf,
        destination: PathBuf,
        #[arg(long, value_enum)]
        source_profile: WeatherProfile,
        /// JSON citation/license/rights record; local input path is not retained.
        #[arg(long)]
        source_record: PathBuf,
    },
    Verify {
        asset: PathBuf,
    },
}
#[derive(Subcommand)]
enum StudyCommand {
    /// Inspect complete retained climate evidence and its separate qualification.
    Inspect { project: PathBuf, run: String },
    Compare {
        project: PathBuf,
        a: String,
        b: String,
        #[arg(long, value_enum, default_value = "illuminance")]
        field: ClimateField,
        #[arg(long, default_value = "0")]
        interval: usize,
    },
    Configure {
        project: PathBuf,
        study: PathBuf,
        #[arg(long, default_value = "A")]
        alternative: String,
    },
    Run {
        project: PathBuf,
        #[arg(long, default_value = "A")]
        alternative: String,
        #[arg(long)]
        engine_dir: Option<PathBuf>,
    },
    Metrics {
        project: PathBuf,
        run: String,
        settings: PathBuf,
    },
}
#[derive(clap::ValueEnum, Clone, Copy)]
enum ClimateField {
    Illuminance,
    Da,
    Cda,
    Udi,
    Underlit,
    Overlit,
}
impl From<ClimateField> for greenlens_runner::studies::ResultField {
    fn from(value: ClimateField) -> Self {
        match value {
            ClimateField::Illuminance => Self::Illuminance,
            ClimateField::Da => Self::Da,
            ClimateField::Cda => Self::Cda,
            ClimateField::Udi => Self::Udi,
            ClimateField::Underlit => Self::Underlit,
            ClimateField::Overlit => Self::Overlit,
        }
    }
}
fn execute() -> Result<()> {
    match Cli::parse().command {
        Command::ReportIssue { command } => report_issue::execute(command)?,
        Command::Open {
            project,
            admission_only,
        } => {
            if !admission_only {
                // Honest operation start, before admission does bounded schema
                // and model validation. No evidence has been checked yet.
                eprintln!(
                    "{}",
                    serde_json::to_string(
                        &greenlens_runner::project_open::admission_started_progress()
                    )?
                );
            }
            let admitted = greenlens_runner::project_open::admit_project(&project)?;
            if admission_only {
                println!("{}", serde_json::to_string_pretty(&admitted)?);
            } else {
                let cancel = intake_cancellation()?;
                let view = greenlens_runner::project_open::verify_admitted_project(
                    &project,
                    &admitted,
                    cancel,
                    |p| {
                        if let Ok(s) = serde_json::to_string(&p) {
                            eprintln!("{s}");
                        }
                    },
                )?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&view.project_verification)?
                );
            }
        }
        Command::Reference { command } => {
            fn verified_scope_project(
                project: &std::path::Path,
                cancel: Arc<std::sync::atomic::AtomicBool>,
            ) -> Result<greenlens_runner::ProjectView> {
                eprintln!(
                    "{}",
                    serde_json::to_string(
                        &greenlens_runner::project_open::admission_started_progress()
                    )?
                );
                let admitted = greenlens_runner::project_open::admit_project(project)?;
                greenlens_runner::project_open::verify_admitted_project(
                    project,
                    &admitted,
                    cancel,
                    |progress| {
                        if let Ok(value) = serde_json::to_string(&progress) {
                            eprintln!("{value}");
                        }
                    },
                )
            }
            fn check_scope_cancel(cancel: &std::sync::atomic::AtomicBool) -> Result<()> {
                ensure!(
                    !cancel.load(std::sync::atomic::Ordering::Relaxed),
                    "Scope operation cancelled; no document save was started"
                );
                Ok(())
            }
            match command {
                ReferenceCommand::Import {
                    project,
                    file,
                    options,
                } => {
                    let cancel = intake_cancellation()?;
                    let admitted = greenlens_runner::project_open::admit_project(&project)?;
                    let view = greenlens_runner::project_open::verify_admitted_project(
                        &project,
                        &admitted,
                        cancel.clone(),
                        |p| {
                            if let Ok(s) = serde_json::to_string(&p) {
                                eprintln!("{s}");
                            }
                        },
                    )?;
                    let options = options
                        .map(|p| read_json(&p))
                        .transpose()?
                        .unwrap_or_default();
                    let next = greenlens_runner::reference::import_project(
                        &project,
                        &view.project,
                        Some(&view.revision),
                        &file,
                        &options,
                        &cancel,
                        |p| {
                            if let Ok(s) = serde_json::to_string(p) {
                                eprintln!("{s}");
                            }
                            true
                        },
                    )?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&next.project.architecture)?
                    );
                }
                ReferenceCommand::Inspect { project } => {
                    let view = greenlens_runner::open_project(&project)?;
                    let ext = view
                        .project
                        .architecture
                        .context("Import a local GLB first")?;
                    let asset = ext
                        .assets
                        .iter()
                        .find(|a| a.id == ext.active_asset_id)
                        .context("Active reference missing")?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&greenlens_runner::reference::read_manifest(
                            &project, asset
                        )?)?
                    );
                }
                ReferenceCommand::ScopeContext { project } => {
                    let view = greenlens_runner::open_project(&project)?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&greenlens_runner::reference::scope_context(
                            &project,
                            &view.project
                        )?)?
                    );
                }
                ReferenceCommand::ScopeAssess { project, scope } => {
                    let view = greenlens_runner::open_project(&project)?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&greenlens_runner::reference::assess_scope(
                            &project,
                            &view.project,
                            &read_json(&scope)?
                        )?)?
                    );
                }
                ReferenceCommand::ScopeApply { project, scope } => {
                    let cancel = intake_cancellation()?;
                    let view = verified_scope_project(&project, cancel.clone())?;
                    let next = greenlens_runner::study_revisions::initialize_scope(
                        &project,
                        &view.project,
                        read_json(&scope)?,
                        &cancel,
                    )?;
                    check_scope_cancel(&cancel)?;
                    let saved =
                        greenlens_runner::save_project(&project, &next, Some(&view.revision))?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &saved.project.architecture.and_then(|a| a.scope)
                        )?
                    );
                }
                ReferenceCommand::Revisions { project, revision } => {
                    let cancel = intake_cancellation()?;
                    let view = verified_scope_project(&project, cancel.clone())?;
                    check_scope_cancel(&cancel)?;
                    if let Some(id) = revision {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(
                                greenlens_runner::study_revisions::selected(&view.project, &id)?
                            )?
                        );
                    } else {
                        let active = view
                            .project
                            .architecture
                            .as_ref()
                            .and_then(|a| a.scope_history.as_ref())
                            .and_then(|h| h.active_revision_sha256.as_ref());
                        let records: Vec<_> = greenlens_runner::study_revisions::revisions(&view.project).iter().map(|revision| serde_json::json!({
                        "revision_sha256": revision.revision_sha256, "parent_revision_sha256": revision.parent_revision_sha256,
                        "active": active == Some(&revision.revision_sha256), "title": revision.scope.title,
                        "reference_revision_sha256": revision.reference.manifest.revision_sha256,
                        "design_model_sha256": revision.scope.design_model_sha256,
                        "documentation_complete": revision.provenance.documentation_complete,
                        "unreviewed_fields": revision.provenance.unreviewed_count,
                        "missing_fields": revision.provenance.missing_count,
                        "conflicting_fields": revision.provenance.conflicting_count
                    })).collect();
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({
                                "schema_version": greenlens_runner::study_revisions::SCOPE_HISTORY_SCHEMA,
                                "active_revision_sha256": active, "revisions": records,
                                "legacy_scope_without_history": view.project.architecture.as_ref().is_some_and(|a| a.scope.is_some() && a.scope_history.is_none()),
                                "disclosure": "Retained documentation revisions are separate from execution permission and scientific qualification. Legacy scope is adopted only on an explicit accepted action."
                            }))?
                        );
                    }
                }
                ReferenceCommand::ScopeReview { project, parent } => {
                    let cancel = intake_cancellation()?;
                    let view = verified_scope_project(&project, cancel.clone())?;
                    let plan = greenlens_runner::study_revisions::prepare(
                        &project,
                        &view.project,
                        &parent,
                        &cancel,
                    )?;
                    println!("{}", serde_json::to_string_pretty(&plan)?);
                }
                ReferenceCommand::ScopeAccept {
                    project,
                    parent,
                    plan,
                    scope,
                    decisions,
                } => {
                    let cancel = intake_cancellation()?;
                    let view = verified_scope_project(&project, cancel.clone())?;
                    let next = greenlens_runner::study_revisions::accept(
                        &project,
                        &view.project,
                        &parent,
                        &plan,
                        read_json(&scope)?,
                        read_json(&decisions)?,
                        &cancel,
                    )?;
                    check_scope_cancel(&cancel)?;
                    let saved =
                        greenlens_runner::save_project(&project, &next, Some(&view.revision))?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &saved.project.architecture.and_then(|a| a.scope_history)
                        )?
                    );
                }
                ReferenceCommand::ScopeActivate { project, revision } => {
                    let cancel = intake_cancellation()?;
                    let view = verified_scope_project(&project, cancel.clone())?;
                    let next = greenlens_runner::study_revisions::activate(
                        &view.project,
                        Some(&revision),
                    )?;
                    check_scope_cancel(&cancel)?;
                    let saved =
                        greenlens_runner::save_project(&project, &next, Some(&view.revision))?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "active_revision_sha256": revision, "project_revision": saved.revision,
                            "retained_revision_count": greenlens_runner::study_revisions::revisions(&saved.project).len()
                        }))?
                    );
                }
            }
        }
        Command::Energy { command } => energy::execute(command)?,
        Command::Study { command } => match command {
            StudyCommand::Inspect { project, run } => {
                let manifest = read_json(&greenlens_runner::persistence::regular_child(
                    &project,
                    "project.json",
                )?)?;
                let assessment =
                    greenlens_runner::qualification::inspect_climate(&project, &manifest, &run)?;
                println!("{}", serde_json::to_string_pretty(&assessment)?);
            }
            StudyCommand::Compare {
                project,
                a,
                b,
                field,
                interval,
            } => {
                let manifest: greenlens_runner::Project = read_json(&project.join("project.json"))?;
                let runs = &manifest
                    .climate
                    .as_ref()
                    .context("Project has no climate studies")?
                    .runs;
                let a = runs.iter().find(|r| r.id == a).context("Unknown A run")?;
                let b = runs.iter().find(|r| r.id == b).context("Unknown B run")?;
                let mut comparison =
                    greenlens_runner::studies::compare(&project, a, b, field.into(), interval)?;
                comparison.a.assessment =
                    greenlens_runner::qualification::assess_climate(&project, &manifest, &a.id)?;
                comparison.b.assessment =
                    greenlens_runner::qualification::assess_climate(&project, &manifest, &b.id)?;
                println!("{}", serde_json::to_string_pretty(&comparison)?);
            }
            StudyCommand::Configure {
                project,
                study,
                alternative,
            } => {
                let view = greenlens_runner::open_project(&project)?;
                let configured = greenlens_runner::studies::configure(
                    &view.project,
                    &alternative,
                    read_json(&study)?,
                )?;
                let saved =
                    greenlens_runner::save_project(&project, &configured, Some(&view.revision))?;
                println!("{}", serde_json::to_string_pretty(&saved.project.climate)?);
            }
            StudyCommand::Run {
                project,
                alternative,
                engine_dir,
            } => {
                let view = greenlens_runner::open_project(&project)?;
                let model = view
                    .project
                    .alternatives
                    .iter()
                    .find(|value| value.id == alternative)
                    .context("Alternative must be A or B")?
                    .model
                    .clone();
                let study = view
                    .project
                    .climate
                    .as_ref()
                    .and_then(|extension| extension.studies.get(&alternative))
                    .cloned()
                    .unwrap_or_default();
                let weather =
                    greenlens_runner::studies::weather_path(&project, &view.project, &study)?;
                let app = Arc::new(Application::default());
                let cancel = app.clone();
                ctrlc::set_handler(move || {
                    let _ = cancel.cancel();
                })?;
                app.start_study_for_project(
                    model,
                    alternative,
                    engine_dir,
                    study,
                    weather,
                    Some(view.project.id.clone()),
                )?;
                let status = await_job(&app)?;
                ensure!(
                    status.state == JobState::Completed,
                    "Study {:?}: {}",
                    status.state,
                    status.error.as_deref().unwrap_or("no completed evidence")
                );
                app.collect(&project, &view.project, Some(&view.revision))?;
                println!("{}", serde_json::to_string_pretty(&status)?);
            }
            StudyCommand::Metrics {
                project,
                run,
                settings,
            } => {
                let manifest: greenlens_runner::Project = read_json(&project.join("project.json"))?;
                let summary = manifest
                    .climate
                    .as_ref()
                    .context("Project has no climate studies")?
                    .runs
                    .iter()
                    .find(|r| r.id == run)
                    .context("Unknown climate run")?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&greenlens_runner::studies::derive_metrics(
                        &project,
                        summary,
                        read_json(&settings)?
                    )?)?
                );
            }
        },
        Command::Result {
            project,
            run,
            hour,
            sensor,
        } => {
            let manifest: greenlens_runner::Project = read_json(&project.join("project.json"))?;
            let summary = manifest
                .climate
                .as_ref()
                .context("Project has no climate results")?
                .runs
                .iter()
                .find(|r| r.id == run)
                .context("Unknown climate run")?;
            let mut value = if let Some(hour) = hour {
                serde_json::to_value(greenlens_runner::studies::result_values(
                    &project,
                    summary,
                    greenlens_runner::studies::ResultField::Illuminance,
                    hour,
                    None,
                )?)?
            } else if let Some(sensor) = sensor {
                let (record, series) =
                    greenlens_runner::climate::open_series_for_run(&project, summary)?;
                let first_interval = match record.study.calculation {
                    greenlens_core::study::StudyMode::WeatherHour { interval_index, .. } => {
                        interval_index
                    }
                    _ => 0,
                };
                serde_json::json!({"run_id":run,"sensor_id":sensor,"first_interval_index":first_interval,"interval_step":1,"units":"lx","lux":series.sensor(&sensor)?})
            } else {
                serde_json::to_value(greenlens_runner::climate::read_result_metadata(
                    &project, summary,
                )?)?
            };
            value["assessment"] = serde_json::to_value(
                greenlens_runner::qualification::assess_climate(&project, &manifest, &run)?,
            )?;
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        Command::Weather { command } => {
            let result = match command {
                WeatherCommand::FindHour {
                    project,
                    weather_id,
                    month,
                    day,
                    hour_ending,
                } => {
                    let interval = greenlens_runner::studies::weather_find_interval(
                        &project,
                        &weather_id,
                        month,
                        day,
                        hour_ending,
                    )?;
                    println!("{}", serde_json::to_string_pretty(&interval)?);
                    return Ok(());
                }
                WeatherCommand::Synthetic { destination } => {
                    use std::io::Write;
                    std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(destination)?
                        .write_all(&greenlens_core::weather::synthetic_epw())?;
                    println!("Original synthetic non-climatic hourly fixture written; no measured climate claim.");
                    return Ok(());
                }
                WeatherCommand::Attach { project, asset } => {
                    let view = greenlens_runner::open_project(&project)?;
                    let saved = greenlens_runner::studies::import_weather_asset(
                        &project,
                        &view.project,
                        Some(&view.revision),
                        &asset,
                    )?;
                    println!("{}", serde_json::to_string_pretty(&saved.project.climate)?);
                    return Ok(());
                }
                WeatherCommand::Inspect {
                    input,
                    source_profile,
                } => greenlens_runner::weather::inspect(&input, source_profile.into())?,
                WeatherCommand::Import {
                    input,
                    destination,
                    source_profile,
                    source_record,
                } => greenlens_runner::weather::import(
                    &input,
                    &destination,
                    source_profile.into(),
                    read_json(&source_record)?,
                )?,
                WeatherCommand::Verify { asset } => {
                    greenlens_runner::weather::WeatherInspection::from(
                        &greenlens_runner::weather::open(&asset)?.1.data,
                    )
                }
            };
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        Command::Doctor { engine_dir } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&greenlens_runner::doctor(engine_dir.as_deref())?)?
            );
        }
        Command::Validate { input } => {
            let model: Model = read_json(&input)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&validate_model(&model)?)?
            );
        }
        Command::Example { project } => {
            let view =
                greenlens_runner::save_project(&project, &greenlens_runner::example(), None)?;
            println!("{}", serde_json::to_string_pretty(&view.project)?);
        }
        Command::Copy {
            project,
            destination,
        } => {
            let view = greenlens_runner::open_project(&project)?;
            let copied = greenlens_runner::save_project_as(
                Some(&project),
                &destination,
                &view.project,
                Some(&view.revision),
            )?;
            println!("{}", serde_json::to_string_pretty(&copied.project)?);
        }
        Command::Run {
            input,
            project,
            alternative,
            engine_dir,
            reference,
        } => {
            let model: Model = read_json(&input)?;
            validate_model(&model)?;
            let existing = if project.join("project.json").exists() {
                Some(greenlens_runner::open_project(&project)?)
            } else {
                None
            };
            let mut manifest = existing
                .as_ref()
                .map(|v| v.project.clone())
                .unwrap_or_else(greenlens_runner::example);
            manifest
                .alternatives
                .iter_mut()
                .find(|a| a.id == alternative)
                .context("Alternative must be A or B")?
                .model = model.clone();
            let app = Arc::new(Application::default());
            let cancel = app.clone();
            ctrlc::set_handler(move || {
                let _ = cancel.cancel();
            })?;
            app.start(
                model,
                alternative,
                engine_dir,
                if reference {
                    Profile::reference()
                } else {
                    Profile::shipping()
                },
            )?;
            let status = await_job(&app)?;
            ensure!(
                status.state == JobState::Completed,
                "{}",
                status
                    .error
                    .unwrap_or_else(|| format!("Job {:?}", status.state))
            );
            let view = app.collect(
                &project,
                &manifest,
                existing.as_ref().map(|v| v.revision.as_str()),
            )?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    view.results.last().context("Missing completed result")?
                )?
            );
        }
        Command::Compare { project, a, b } => {
            let view = greenlens_runner::open_project(&project)?;
            let find = |id: Option<String>, alt: &str| -> Result<&greenlens_runner::RunRecord> {
                view.results
                    .iter()
                    .rev()
                    .find(|r| {
                        id.as_ref()
                            .map_or(r.alternative_id == alt, |id| &r.id == id)
                    })
                    .context("Run not found")
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&greenlens_runner::compare_runs(
                    find(a, "A")?,
                    find(b, "B")?
                )?)?
            );
        }
        Command::Export {
            project,
            destination,
            runs,
        } => {
            if runs.is_empty() {
                greenlens_runner::export_project(&project, &destination)?;
            } else {
                greenlens_runner::climate_export::export_selected(&project, &destination, &runs)?;
            }
            println!("Exported {}", destination.display());
        }
        Command::Replay {
            package,
            engine_dir,
        } => {
            let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let signal = cancel.clone();
            ctrlc::set_handler(move || signal.store(true, std::sync::atomic::Ordering::SeqCst))?;
            println!(
                "{}",
                serde_json::to_string_pretty(&greenlens_runner::replay_with_cancel(
                    &package,
                    engine_dir.as_deref(),
                    &cancel
                )?)?
            );
        }
    }
    Ok(())
}
fn intake_cancellation() -> Result<Arc<std::sync::atomic::AtomicBool>> {
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let signal = cancel.clone();
    ctrlc::set_handler(move || signal.store(true, std::sync::atomic::Ordering::SeqCst))?;
    Ok(cancel)
}

fn await_job(app: &Application) -> Result<greenlens_runner::JobStatus> {
    let mut displayed = 0;
    loop {
        let status = app.status()?;
        for line in status.log.iter().skip(displayed) {
            eprintln!("{line}");
        }
        displayed = status.log.len();
        if !matches!(
            status.state,
            JobState::Queued | JobState::Preparing | JobState::Running
        ) {
            return app.wait();
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}
fn main() {
    if let Err(error) = execute() {
        eprintln!("GreenLens: {error:#}");
        std::process::exit(1);
    }
}
