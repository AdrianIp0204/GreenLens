//! CLI dispatch uses the same typed application actions as the desktop.
use anyhow::{ensure, Result};
use clap::Subcommand;
use greenlens_runner::{energy_services as service, persistence::read_json, Application, JobState};
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

#[derive(Subcommand)]
pub enum EnergyCommand {
    /// Create the original four-zone, two-storey A/B example and portable synthetic weather.
    Example {
        project: PathBuf,
    },
    /// Validate the pinned native EnergyPlus package and its required runtime files.
    Doctor {
        #[arg(long)]
        engine_dir: Option<PathBuf>,
    },
    Validate {
        model: PathBuf,
        calendar: PathBuf,
    },
    /// Inspect thermal EPW suitability; daylight admission is not sufficient.
    WeatherInspect {
        input: PathBuf,
    },
    WeatherImport {
        input: PathBuf,
        destination: PathBuf,
        #[arg(long)]
        source_record: PathBuf,
    },
    /// Attach retained weather and an explicit non-leap calendar to both designs.
    WeatherAttach {
        project: PathBuf,
        asset: PathBuf,
        calendar: PathBuf,
        #[arg(long)]
        apply_location: bool,
    },
    Configure {
        project: PathBuf,
        model: PathBuf,
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
    /// Inspect immutable run inputs, totals and four separate status dimensions.
    Inspect {
        project: PathBuf,
        run: String,
    },
    Rows {
        project: PathBuf,
        run: String,
        #[arg(long, default_value = "0")]
        start: usize,
        #[arg(long, default_value = "144")]
        count: usize,
    },
    Compare {
        project: PathBuf,
        a: String,
        b: String,
    },
    /// Generate all report products from one authoritative Rust dataset.
    Report {
        project: PathBuf,
        a: String,
        b: String,
        destination: PathBuf,
        #[arg(long)]
        typst_dir: Option<PathBuf>,
        #[arg(long)]
        html_only: bool,
    },
    Export {
        project: PathBuf,
        a: String,
        b: String,
        destination: PathBuf,
        #[arg(long)]
        typst_dir: Option<PathBuf>,
    },
    /// Check package paths, checksums, complete run evidence and report bindings without engines.
    PackageInspect {
        package: PathBuf,
    },
    /// Retain optional PDF/PNG/JPEG evidence privately, bound to current model A or B.
    Attach {
        project: PathBuf,
        file: PathBuf,
        #[arg(long, default_value = "A")]
        alternative: String,
        #[arg(long)]
        caption: String,
        #[arg(long)]
        rights: String,
    },
    AttachmentInclude {
        project: PathBuf,
        id: String,
        #[arg(long, action = clap::ArgAction::Set)]
        included: bool,
    },
    /// Set derived report inputs; omission explicitly clears the corresponding value.
    ReportSettings {
        project: PathBuf,
        #[arg(long)]
        carbon_factor: Option<PathBuf>,
        #[arg(long)]
        lighting_service_evidence: Option<String>,
    },
    Replay {
        package: PathBuf,
        #[arg(long)]
        engine_dir: Option<PathBuf>,
    },
}
fn print(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
pub fn execute(command: EnergyCommand) -> Result<()> {
    match command {
        EnergyCommand::Example { project } => print(&service::create_example(&project)?.project)?,
        EnergyCommand::Doctor { engine_dir } => {
            print(&greenlens_runner::energy::doctor(engine_dir.as_deref())?)?
        }
        EnergyCommand::Validate { model, calendar } => {
            print(&greenlens_core::energy::validate_thermal_model(
                &read_json(&model)?,
                &read_json(&calendar)?,
            )?)?
        }
        EnergyCommand::WeatherInspect { input } => {
            print(&greenlens_runner::energy_weather::inspect(&input)?)?
        }
        EnergyCommand::WeatherImport {
            input,
            destination,
            source_record,
        } => print(&greenlens_runner::energy_weather::import(
            &input,
            &destination,
            read_json(&source_record)?,
        )?)?,
        EnergyCommand::WeatherAttach {
            project,
            asset,
            calendar,
            apply_location,
        } => {
            let view = greenlens_runner::open_project(&project)?;
            print(
                &service::attach_weather(
                    &project,
                    &view.project,
                    Some(&view.revision),
                    &asset,
                    read_json(&calendar)?,
                    apply_location,
                )?
                .project
                .energy,
            )?;
        }
        EnergyCommand::Configure {
            project,
            model,
            study,
            alternative,
        } => {
            let view = greenlens_runner::open_project(&project)?;
            let next = service::edit(
                &view.project,
                &alternative,
                read_json(&model)?,
                Some(read_json(&study)?),
            )?;
            print(
                &greenlens_runner::save_project(&project, &next, Some(&view.revision))?
                    .project
                    .energy,
            )?;
        }
        EnergyCommand::Run {
            project,
            alternative,
            engine_dir,
        } => {
            let view = greenlens_runner::open_project(&project)?;
            let app = Arc::new(Application::default());
            let signal = app.clone();
            ctrlc::set_handler(move || {
                let _ = signal.cancel();
            })?;
            service::start(&app, &project, &view.project, &alternative, engine_dir)?;
            let status = crate::await_job(&app)?;
            ensure!(
                status.state == JobState::Completed,
                "Energy {:?}: {}",
                status.state,
                status.error.as_deref().unwrap_or("No completed evidence")
            );
            app.collect(&project, &view.project, Some(&view.revision))?;
            print(&status)?;
        }
        EnergyCommand::Inspect { project, run } => {
            let view = greenlens_runner::open_project(&project)?;
            let record = service::selected_run(&project, &view.project, &run)?;
            print(
                &serde_json::json!({"run":record,"assessment":service::assessment(&view.project,&record)?}),
            )?;
        }
        EnergyCommand::Rows {
            project,
            run,
            start,
            count,
        } => {
            let view = greenlens_runner::open_project(&project)?;
            print(&service::rows(&project, &view.project, &run, start, count)?)?;
        }
        EnergyCommand::Compare { project, a, b } => {
            let view = greenlens_runner::open_project(&project)?;
            print(&service::report_data(&project, &view.project, &a, &b)?)?;
        }
        EnergyCommand::Report {
            project,
            a,
            b,
            destination,
            typst_dir,
            html_only,
        } => {
            let view = greenlens_runner::open_project(&project)?;
            let config = if html_only {
                None
            } else {
                Some(greenlens_runner::energy_report::TypstConfig::discover(
                    typst_dir.as_deref(),
                )?)
            };
            let cancel = Arc::new(AtomicBool::new(false));
            let signal = cancel.clone();
            ctrlc::set_handler(move || signal.store(true, std::sync::atomic::Ordering::SeqCst))?;
            print(&service::render_reports(
                &project,
                &view.project,
                &a,
                &b,
                &destination,
                config.as_ref(),
                &cancel,
            )?)?;
        }
        EnergyCommand::Export {
            project,
            a,
            b,
            destination,
            typst_dir,
        } => {
            let cancel = Arc::new(AtomicBool::new(false));
            let signal = cancel.clone();
            ctrlc::set_handler(move || signal.store(true, std::sync::atomic::Ordering::SeqCst))?;
            let config =
                greenlens_runner::energy_report::TypstConfig::discover(typst_dir.as_deref())?;
            print(&greenlens_runner::energy_export::export_pair(
                &project,
                &a,
                &b,
                &destination,
                &config,
                &cancel,
            )?)?;
        }
        EnergyCommand::Attach {
            project,
            file,
            alternative,
            caption,
            rights,
        } => {
            let view = greenlens_runner::open_project(&project)?;
            print(
                &greenlens_runner::energy_attachments::add(
                    &project,
                    &view.project,
                    Some(&view.revision),
                    &alternative,
                    &file,
                    caption,
                    rights,
                )?
                .project
                .energy,
            )?;
        }
        EnergyCommand::AttachmentInclude {
            project,
            id,
            included,
        } => {
            let view = greenlens_runner::open_project(&project)?;
            let next =
                greenlens_runner::energy_attachments::set_inclusion(&view.project, &id, included)?;
            print(
                &greenlens_runner::save_project(&project, &next, Some(&view.revision))?
                    .project
                    .energy,
            )?;
        }
        EnergyCommand::ReportSettings {
            project,
            carbon_factor,
            lighting_service_evidence,
        } => {
            let view = greenlens_runner::open_project(&project)?;
            let factor = carbon_factor.map(|p| read_json(&p)).transpose()?;
            let next =
                service::update_report_settings(&view.project, factor, lighting_service_evidence)?;
            print(
                &greenlens_runner::save_project(&project, &next, Some(&view.revision))?
                    .project
                    .energy,
            )?;
        }
        EnergyCommand::PackageInspect { package } => {
            print(&greenlens_runner::energy_export::inspect_package(&package)?)?
        }
        EnergyCommand::Replay {
            package,
            engine_dir,
        } => {
            let cancel = Arc::new(AtomicBool::new(false));
            let signal = cancel.clone();
            ctrlc::set_handler(move || signal.store(true, std::sync::atomic::Ordering::SeqCst))?;
            print(&greenlens_runner::energy_export::replay(
                &package,
                engine_dir.as_deref(),
                &cancel,
            )?)?;
        }
    }
    Ok(())
}
