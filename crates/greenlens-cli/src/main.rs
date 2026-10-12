use anyhow::{ensure, Context, Result};
use clap::{Parser, Subcommand};
use greenlens_core::{validate_model, Model};
use greenlens_radiance::Profile;
use greenlens_runner::{persistence::read_json, Application, JobState};
use std::{path::PathBuf, sync::Arc};

#[derive(Parser)]
#[command(
    version,
    about = "Local, reproducible static and climate-based daylight studies."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
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
