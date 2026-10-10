use anyhow::{ensure, Context, Result};
use clap::{Parser, Subcommand};
use greenlens_core::{validate_model, Model};
use greenlens_radiance::Profile;
use greenlens_runner::{persistence::read_json, Application, JobState};
use std::{path::PathBuf, sync::Arc};

#[derive(Parser)]
#[command(
    version,
    about = "Static daylight illuminance under a prescribed CIE standard overcast sky."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
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
    },
    Replay {
        package: PathBuf,
        #[arg(long)]
        engine_dir: Option<PathBuf>,
    },
}
fn execute() -> Result<()> {
    match Cli::parse().command {
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
            let status = app.wait()?;
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
        } => {
            greenlens_runner::export_project(&project, &destination)?;
            println!("Exported {}", destination.display());
        }
        Command::Replay {
            package,
            engine_dir,
        } => println!(
            "{}",
            serde_json::to_string_pretty(&greenlens_runner::replay(
                &package,
                engine_dir.as_deref()
            )?)?
        ),
    }
    Ok(())
}
fn main() {
    if let Err(error) = execute() {
        eprintln!("GreenLens: {error:#}");
        std::process::exit(1);
    }
}
