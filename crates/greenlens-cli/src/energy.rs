//! CLI dispatch uses the same typed application actions as the desktop.
use anyhow::{ensure, Result};
use clap::Subcommand;
use greenlens_runner::{energy_services as service, persistence::read_json, Application, JobState};
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum EnergyEpwProfile {
    ExplicitFields,
    DryEnvelope,
}
impl From<EnergyEpwProfile> for greenlens_core::energy::weather::EnergyWeatherProfile {
    fn from(value: EnergyEpwProfile) -> Self {
        match value {
            EnergyEpwProfile::ExplicitFields => Self::ExplicitFieldsV1,
            EnergyEpwProfile::DryEnvelope => Self::DryEnvelopeV1,
        }
    }
}

#[derive(Subcommand)]
pub enum EnergyCommand {
    /// Declare a finite assessment, execute one missing case, or inspect/finalize its exact receipts.
    Sensitivity {
        #[command(subcommand)]
        action: SensitivityCommand,
    },
    /// Create a one-zone starting document with explicit geometry and labeled synthetic presets.
    New {
        project: PathBuf,
        #[arg(long, default_value = "8")]
        width: f64,
        #[arg(long, default_value = "6")]
        depth: f64,
        #[arg(long, default_value = "3")]
        height: f64,
    },
    /// Shared core/adapter readiness, with object-linked issues before any native process.
    Preflight {
        project: PathBuf,
        #[arg(long, default_value = "A")]
        alternative: String,
    },
    /// Typed differences between accepted A/B inputs (no historical report rewrite).
    InputDifferences {
        project: PathBuf,
    },
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
        #[arg(long, value_enum, default_value = "explicit-fields")]
        profile: EnergyEpwProfile,
    },
    WeatherImport {
        input: PathBuf,
        destination: PathBuf,
        #[arg(long)]
        source_record: PathBuf,
        #[arg(long, value_enum, default_value = "explicit-fields")]
        profile: EnergyEpwProfile,
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
#[derive(Subcommand)]
pub enum SensitivityCommand {
    Plan {
        project: PathBuf,
        #[arg(long, value_delimiter = ',', default_value = "A,B")]
        alternatives: Vec<String>,
        #[arg(long)]
        retain: bool,
    },
    Inspect {
        project: PathBuf,
        id: String,
    },
    /// Executes exactly one reserved missing slot; never loops or retries.
    Step {
        project: PathBuf,
        id: String,
        #[arg(long)]
        engine_dir: Option<PathBuf>,
    },
    Finalize {
        project: PathBuf,
        id: String,
    },
    /// Reconcile one interrupted metadata commit, or explicitly stop an orphaned attempt.
    Recover {
        project: PathBuf,
        id: String,
        #[arg(long)]
        mark_interrupted: bool,
    },
}
fn sensitivity_command(action: SensitivityCommand) -> Result<()> {
    use greenlens_runner::{
        energy_sensitivity as planning, energy_sensitivity_workflow as workflow,
    };
    match action {
        SensitivityCommand::Plan {
            project,
            alternatives,
            retain,
        } => {
            let view = greenlens_runner::open_project(&project)?;
            let plan = planning::plan(&project, &view.project, &alternatives, None)?;
            if retain {
                let view = workflow::retain_plan(&project, &view.project, &view.revision, plan)?;
                let id = &view
                    .project
                    .energy
                    .as_ref()
                    .unwrap()
                    .workbench
                    .as_ref()
                    .unwrap()
                    .sensitivity
                    .last()
                    .unwrap()
                    .id;
                print(&workflow::inspect(&project, &view.project, id)?)?;
            } else {
                print(&plan)?;
            }
        }
        SensitivityCommand::Inspect { project, id } => {
            let view = greenlens_runner::open_project(&project)?;
            print(&workflow::inspect(&project, &view.project, &id)?)?;
        }
        SensitivityCommand::Finalize { project, id } => {
            let view = greenlens_runner::open_project(&project)?;
            print(&workflow::finalize(&project, &view.project, &view.revision, &id)?.session)?;
        }
        SensitivityCommand::Recover {
            project,
            id,
            mark_interrupted,
        } => {
            // Recovery reads only the bounded manifest first because opening the
            // complete document correctly rejects an unreferenced crash receipt.
            let bytes = greenlens_runner::persistence::read_json_bytes(
                &greenlens_runner::persistence::regular_child(&project, "project.json")?,
            )?;
            let model: greenlens_runner::Project = serde_json::from_slice(&bytes)?;
            greenlens_runner::validate_project(&model)?;
            let revision = greenlens_runner::persistence::bytes_hash(&bytes);
            let view = if mark_interrupted {
                workflow::mark_interrupted(
                    &project,
                    &model,
                    &revision,
                    &id,
                    &Application::default(),
                )?
            } else {
                workflow::recover_metadata(&project, &model, &revision, &id)?
            };
            print(&workflow::inspect(&project, &view.project, &id)?)?;
        }
        SensitivityCommand::Step {
            project,
            id,
            engine_dir,
        } => {
            let view = greenlens_runner::open_project(&project)?;
            let app = Arc::new(Application::default());
            let signal = app.clone();
            ctrlc::set_handler(move || {
                let _ = signal.cancel();
            })?;
            let mut step = workflow::next(
                &project,
                &view.project,
                &view.revision,
                &id,
                &app,
                engine_dir,
            )?;
            if step.job.is_some() {
                let status = crate::await_job(&app)?;
                if status.state == JobState::Completed {
                    step.view =
                        app.collect(&project, &step.view.project, Some(&step.view.revision))?;
                }
                step = workflow::observe_after_collect(
                    &project,
                    &step.view.project,
                    &step.view.revision,
                    &id,
                    &app,
                )?;
            }
            print(&step.session)?;
            ensure!(step.session.interruptions.is_empty(),"Sensitivity attempt stopped; failure/cancellation is retained and this declaration cannot retry");
        }
    }
    Ok(())
}
fn print(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
pub fn execute(command: EnergyCommand) -> Result<()> {
    match command {
        EnergyCommand::Sensitivity { action } => sensitivity_command(action)?,
        EnergyCommand::New {
            project,
            width,
            depth,
            height,
        } => {
            ensure!(
                !project.exists() || std::fs::read_dir(&project)?.next().is_none(),
                "New energy project destination must be empty"
            );
            let next = greenlens_runner::energy_workbench::new_document(
                greenlens_core::energy::ZoneBoxInput {
                    id: "zone1".into(),
                    name: "Zone 1".into(),
                    storey: 1,
                    origin_m: [0., 0., 0.],
                    size_m: [width, depth, height],
                },
                greenlens_core::energy::EnergyCalendar {
                    start_weekday: greenlens_core::energy::Weekday::Monday,
                    holidays: vec![],
                    utc_offset_minutes: 0,
                },
            )?;
            print(&greenlens_runner::save_project(&project, &next, None)?.project)?;
        }
        EnergyCommand::Preflight {
            project,
            alternative,
        } => {
            let view = greenlens_runner::open_project(&project)?;
            print(&greenlens_runner::energy_preflight::inputs(
                &view.project,
                &alternative,
            )?)?;
        }
        EnergyCommand::InputDifferences { project } => {
            let view = greenlens_runner::open_project(&project)?;
            let e = view
                .project
                .energy
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("Energy document missing"))?;
            let a = &e.designs["A"];
            let b = &e.designs["B"];
            print(&greenlens_core::energy::compare_thermal_inputs_v1(
                &a.model,
                a.study
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Assign baseline study"))?,
                &b.model,
                b.study
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Assign proposed study"))?,
            )?)?;
        }
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
        EnergyCommand::WeatherInspect { input, profile } => print(
            &greenlens_runner::energy_weather::inspect_with_profile(&input, profile.into())?,
        )?,
        EnergyCommand::WeatherImport {
            input,
            destination,
            source_record,
            profile,
        } => print(&greenlens_runner::energy_weather::import_with_profile(
            &input,
            &destination,
            read_json(&source_record)?,
            profile.into(),
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
            print(&service::render_workbench_reports(
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
            print(&greenlens_runner::energy_export::export_workbench_pair(
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
