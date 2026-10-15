//! Thin desktop calls into the shared Rust energy services. No frontend physics.
use super::*;
use greenlens_core::energy::*;
use greenlens_runner::{energy_report::TypstConfig, energy_services as service};
use serde::Deserialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Condvar, Mutex,
};
#[tauri::command]
pub(crate) async fn energy_copy_design_preview(
    project: Project,
    source: greenlens_runner::energy_design_copy::EnergyDesignSide,
    target: greenlens_runner::energy_design_copy::EnergyDesignSide,
) -> CommandResult<greenlens_runner::energy_design_copy::EnergyDesignCopyPreview> {
    blocking(move || greenlens_runner::energy_design_copy::preview(&project, source, target)).await
}
#[tauri::command]
pub(crate) async fn energy_copy_design_commit(
    project: Project,
    source: greenlens_runner::energy_design_copy::EnergyDesignSide,
    target: greenlens_runner::energy_design_copy::EnergyDesignSide,
    expected_preview_sha256: String,
) -> CommandResult<Project> {
    blocking(move || {
        greenlens_runner::energy_design_copy::commit(
            &project,
            source,
            target,
            &expected_preview_sha256,
        )
    })
    .await
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum SensitivityAction {
    Plan {
        alternatives: Vec<String>,
    },
    Retain {
        plan: Box<greenlens_runner::energy_sensitivity::EnergySensitivityPlan>,
    },
    Next {
        id: String,
    },
    Observe {
        id: String,
    },
    Finalize {
        id: String,
    },
    MarkInterrupted {
        id: String,
    },
    RecoverMetadata {
        id: String,
    },
}
#[tauri::command]
pub(crate) async fn energy_sensitivity_action(
    app: tauri::AppHandle,
    path: String,
    project: Project,
    expected_revision: String,
    action: SensitivityAction,
    engine_dir: Option<String>,
) -> CommandResult<Value> {
    blocking(move || {
        use greenlens_runner::{
            energy_sensitivity as planning, energy_sensitivity_workflow as workflow,
        };
        let root = Path::new(&path);
        let application = app.state::<Application>();
        app.state::<ReportControl>().ensure_idle()?;
        match action {
            SensitivityAction::Plan { alternatives } => {
                Ok(json!({"plan":planning::plan(root,&project,&alternatives,None)?}))
            }
            SensitivityAction::Retain { plan } => {
                let view = workflow::retain_plan(root, &project, &expected_revision, *plan)?;
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
                let session = workflow::inspect(root, &view.project, id)?;
                Ok(json!({"view":view,"session":session}))
            }
            SensitivityAction::Next { id } => Ok(serde_json::to_value(workflow::next(
                root,
                &project,
                &expected_revision,
                &id,
                &application,
                engine_dir.map(PathBuf::from),
            )?)?),
            SensitivityAction::Observe { id } => {
                Ok(serde_json::to_value(workflow::observe_after_collect(
                    root,
                    &project,
                    &expected_revision,
                    &id,
                    &application,
                )?)?)
            }
            SensitivityAction::Finalize { id } => Ok(serde_json::to_value(workflow::finalize(
                root,
                &project,
                &expected_revision,
                &id,
            )?)?),
            SensitivityAction::MarkInterrupted { id } => {
                let view = workflow::mark_interrupted(
                    root,
                    &project,
                    &expected_revision,
                    &id,
                    &application,
                )?;
                Ok(json!({"view":view,"session":workflow::inspect(root,&view.project,&id)?}))
            }
            SensitivityAction::RecoverMetadata { id } => {
                let view = workflow::recover_metadata(root, &project, &expected_revision, &id)?;
                Ok(json!({"view":view,"session":workflow::inspect(root,&view.project,&id)?}))
            }
        }
    })
    .await
}
#[tauri::command]
pub(crate) async fn energy_sensitivity_input_identity(
    project: Project,
    alternatives: Vec<String>,
) -> CommandResult<String> {
    blocking(move || {
        greenlens_runner::energy_sensitivity::current_inputs_identity(&project, &alternatives)
    })
    .await
}
#[tauri::command]
pub(crate) async fn energy_sensitivity_inspect(
    path: String,
    project: Project,
    id: String,
) -> CommandResult<greenlens_runner::energy_sensitivity_workflow::EnergySensitivitySession> {
    blocking(move || {
        greenlens_runner::energy_sensitivity_workflow::inspect(Path::new(&path), &project, &id)
    })
    .await
}
#[tauri::command]
pub(crate) async fn energy_sensitivity_export_selection(
    path: String,
    project: Project,
    a: String,
    b: String,
) -> CommandResult<greenlens_runner::energy_sensitivity_workflow::EnergySensitivityExportSelection>
{
    blocking(move || {
        greenlens_runner::energy_sensitivity_workflow::export_selection(
            Path::new(&path),
            &project,
            &a,
            &b,
        )
    })
    .await
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum MetadataChange {
    ReportIssue {
        metadata: Option<EnergyReportMetadata>,
    },
    Attachment {
        id: String,
        included: bool,
    },
    ReportSettings {
        carbon_factor: Option<CarbonFactor>,
        lighting_service_evidence: Option<String>,
    },
}
#[tauri::command]
pub(crate) async fn energy_metadata(
    project: Project,
    change: MetadataChange,
) -> CommandResult<Project> {
    blocking(move || match change {
        MetadataChange::ReportIssue { metadata } => {
            greenlens_runner::energy_workbench::set_report_metadata(&project, metadata)
        }
        MetadataChange::Attachment { id, included } => {
            greenlens_runner::energy_attachments::set_inclusion(&project, &id, included)
        }
        MetadataChange::ReportSettings {
            carbon_factor,
            lighting_service_evidence,
        } => service::update_report_settings(&project, carbon_factor, lighting_service_evidence),
    })
    .await
}
#[tauri::command]
pub(crate) async fn energy_attach(
    path: String,
    project: Project,
    expected_revision: Option<String>,
    alternative_id: String,
    file: String,
    caption: String,
    rights: String,
) -> CommandResult<ProjectView> {
    blocking(move || {
        greenlens_runner::energy_attachments::add(
            Path::new(&path),
            &project,
            expected_revision.as_deref(),
            &alternative_id,
            Path::new(&file),
            caption,
            rights,
        )
    })
    .await
}
#[tauri::command]
pub(crate) fn dialog_energy_attachment(
    window: tauri::WebviewWindow,
) -> CommandResult<Option<String>> {
    #[cfg(windows)]
    {
        Ok(
            file_dialog(None, "Import inert PDF or image evidence", &window)
                .add_filter("PDF / PNG / JPEG", &["pdf", "png", "jpg", "jpeg"])
                .pick_file()
                .map(|p| p.to_string_lossy().into_owned()),
        )
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        Err("Native attachment dialog is only supported on Windows".into())
    }
}

#[derive(Default)]
pub(crate) struct ReportControl {
    active: Mutex<bool>,
    changed: Condvar,
    cancelled: AtomicBool,
    closing: AtomicBool,
}
struct ReportGuard<'a>(&'a ReportControl);
impl Drop for ReportGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.0.active.lock() {
            *active = false;
            self.0.changed.notify_all();
        }
    }
}
impl ReportControl {
    fn begin(&self) -> anyhow::Result<ReportGuard<'_>> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| anyhow::anyhow!("Report state unavailable"))?;
        ensure!(
            !self.closing.load(Ordering::SeqCst) && !*active,
            "A report operation is active or the application is closing"
        );
        self.cancelled.store(false, Ordering::SeqCst);
        *active = true;
        Ok(ReportGuard(self))
    }
    pub(crate) fn ensure_idle(&self) -> anyhow::Result<()> {
        ensure!(
            !*self
                .active
                .lock()
                .map_err(|_| anyhow::anyhow!("Report state unavailable"))?,
            "Finish or cancel the report operation first"
        );
        Ok(())
    }
    pub(crate) fn shutdown(&self) {
        self.closing.store(true, Ordering::SeqCst);
        self.cancelled.store(true, Ordering::SeqCst);
        if let Ok(mut active) = self.active.lock() {
            while *active {
                match self.changed.wait(active) {
                    Ok(next) => active = next,
                    Err(_) => break,
                }
            }
        }
    }
}

#[tauri::command]
pub(crate) async fn energy_create_example(path: String) -> CommandResult<ProjectView> {
    blocking(move || service::create_example(Path::new(&path))).await
}
#[tauri::command]
pub(crate) async fn energy_new(
    zone: ZoneBoxInput,
    calendar: EnergyCalendar,
) -> CommandResult<Project> {
    blocking(move || greenlens_runner::energy_workbench::new_document(zone, calendar)).await
}
#[tauri::command]
pub(crate) async fn energy_transaction_preview(
    project: Project,
    requests: std::collections::BTreeMap<String, ThermalTransaction>,
) -> CommandResult<greenlens_runner::energy_workbench::TransactionSetPreview> {
    blocking(move || greenlens_runner::energy_workbench::preview(&project, &requests)).await
}
#[tauri::command]
pub(crate) async fn energy_operation_plan(
    project: Project,
    operations: std::collections::BTreeMap<String, ThermalOperation>,
    topology: TopologyAssignments,
) -> CommandResult<greenlens_runner::energy_workbench::OperationPlan> {
    blocking(move || {
        greenlens_runner::energy_workbench::plan_operation(&project, operations, topology)
    })
    .await
}
#[tauri::command]
pub(crate) async fn energy_transaction_commit(
    project: Project,
    requests: std::collections::BTreeMap<String, ThermalTransaction>,
    expected_preview_sha256: String,
) -> CommandResult<Project> {
    blocking(move || {
        greenlens_runner::energy_workbench::commit(&project, &requests, &expected_preview_sha256)
    })
    .await
}
#[tauri::command]
pub(crate) async fn energy_set_locks(
    project: Project,
    alternative_id: String,
    ids: Vec<String>,
) -> CommandResult<Project> {
    blocking(move || greenlens_runner::energy_workbench::set_locks(&project, &alternative_id, ids))
        .await
}
#[tauri::command]
pub(crate) async fn energy_restore_designs(
    project: Project,
    designs: std::collections::BTreeMap<String, greenlens_runner::energy::EnergyDesign>,
) -> CommandResult<Project> {
    blocking(move || greenlens_runner::energy_workbench::restore_designs(&project, designs)).await
}
#[tauri::command]
pub(crate) async fn energy_profile_blocks(
    blocks: Vec<ScheduleBlock>,
    kind: ScheduleKind,
) -> CommandResult<DayProfile> {
    blocking(move || Ok(profile_from_blocks(&blocks, kind)?)).await
}
#[tauri::command]
pub(crate) async fn energy_calendar_draft(
    model: ThermalModel,
    mut study: EnergyStudy,
    calendar: EnergyCalendar,
) -> CommandResult<greenlens_runner::energy::EnergyDesign> {
    blocking(move || {
        let model = model_with_calendar(&model, &calendar)?;
        study.calendar = calendar;
        study.validate(&model)?;
        Ok(greenlens_runner::energy::EnergyDesign {
            model,
            study: Some(study),
        })
    })
    .await
}
#[tauri::command]
pub(crate) async fn energy_preflight(
    project: Project,
    alternative_id: String,
) -> CommandResult<greenlens_runner::energy_preflight::EnergyPreflight> {
    blocking(move || greenlens_runner::energy_preflight::inputs(&project, &alternative_id)).await
}
#[tauri::command]
pub(crate) async fn energy_dependencies(
    engine_dir: Option<String>,
    typst_dir: Option<String>,
) -> CommandResult<greenlens_runner::energy_preflight::DependencyReadiness> {
    blocking(move || {
        Ok(greenlens_runner::energy_preflight::dependencies(
            engine_dir.as_deref().map(Path::new),
            typst_dir.as_deref().map(Path::new),
        ))
    })
    .await
}
#[tauri::command]
pub(crate) async fn energy_input_differences(
    project: Project,
) -> CommandResult<ThermalInputDifferences> {
    blocking(move || {
        let e = project.energy.as_ref().context("Energy document missing")?;
        let a = &e.designs["A"];
        let b = &e.designs["B"];
        Ok(compare_thermal_inputs_v1(
            &a.model,
            a.study
                .as_ref()
                .context("Assign baseline study weather/calendar")?,
            &b.model,
            b.study
                .as_ref()
                .context("Assign proposed study weather/calendar")?,
        )?)
    })
    .await
}
#[tauri::command]
pub(crate) async fn energy_edit(
    project: Project,
    alternative_id: String,
    model: ThermalModel,
    study: Option<EnergyStudy>,
) -> CommandResult<Project> {
    blocking(move || service::edit(&project, &alternative_id, model, study)).await
}
#[tauri::command]
pub(crate) async fn energy_validate(
    model: ThermalModel,
    calendar: EnergyCalendar,
) -> CommandResult<ValidatedThermalModel> {
    blocking(move || Ok(validate_thermal_model(&model, &calendar)?)).await
}
#[tauri::command]
pub(crate) async fn energy_scene(
    model: ThermalModel,
    calendar: EnergyCalendar,
) -> CommandResult<ThermalScene> {
    blocking(move || Ok(thermal_scene(&model, &calendar)?)).await
}
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Named IPC fields preserve the existing import contract.
pub(crate) async fn energy_import_weather(
    path: String,
    project: Project,
    expected_revision: Option<String>,
    file: String,
    source: greenlens_runner::weather::WeatherSource,
    calendar: EnergyCalendar,
    apply_location: bool,
    profile: Option<greenlens_core::energy::weather::EnergyWeatherProfile>,
) -> CommandResult<ProjectView> {
    blocking(move || {
        service::import_weather_with_profile(
            Path::new(&path),
            &project,
            expected_revision.as_deref(),
            Path::new(&file),
            source,
            calendar,
            apply_location,
            profile
                .unwrap_or(greenlens_core::energy::weather::EnergyWeatherProfile::ExplicitFieldsV1),
        )
    })
    .await
}
#[tauri::command]
pub(crate) async fn energy_start(
    app: tauri::AppHandle,
    path: String,
    project: Project,
    alternative_id: String,
    engine_dir: Option<String>,
) -> CommandResult<JobStatus> {
    blocking(move || {
        app.state::<ReportControl>().ensure_idle()?;
        service::start(
            &app.state::<Application>(),
            Path::new(&path),
            &project,
            &alternative_id,
            engine_dir.map(PathBuf::from),
        )
    })
    .await
}
#[tauri::command]
pub(crate) async fn energy_default_runs(
    path: String,
    project: Project,
) -> CommandResult<std::collections::BTreeMap<String, String>> {
    blocking(move || service::default_run_ids(Path::new(&path), &project)).await
}
#[tauri::command]
pub(crate) async fn energy_run_catalog(
    path: String,
    project: Project,
    request: greenlens_runner::energy_catalog::EnergyRunCatalogRequest,
) -> CommandResult<greenlens_runner::energy_catalog::EnergyRunCatalogPage> {
    blocking(move || {
        greenlens_runner::energy_catalog::catalog(Path::new(&path), &project, &request)
    })
    .await
}
#[tauri::command]
pub(crate) async fn energy_result(
    path: String,
    project: Project,
    run_id: String,
) -> CommandResult<Value> {
    blocking(move || {
        let case = service::report_case(Path::new(&path), &project, &run_id)?;
        let presentation = case.presentation()?;
        Ok(json!({"case":case,"presentation":presentation}))
    })
    .await
}
#[tauri::command]
pub(crate) async fn energy_rows(
    path: String,
    project: Project,
    run_id: String,
    start: usize,
    count: usize,
) -> CommandResult<Value> {
    blocking(move || service::rows(Path::new(&path), &project, &run_id, start, count)).await
}
#[tauri::command]
pub(crate) async fn energy_navigate(
    path: String,
    project: Project,
    run_id: String,
    request: EnergyNavigationRequest,
) -> CommandResult<greenlens_runner::energy_navigation::EnergyNavigationResult> {
    blocking(move || {
        greenlens_runner::energy_navigation::query(Path::new(&path), &project, &run_id, &request)
    })
    .await
}
#[tauri::command]
pub(crate) async fn energy_navigate_pair(
    path: String,
    project: Project,
    a: String,
    b: String,
    request: EnergyNavigationRequest,
    anchor: greenlens_runner::energy_navigation::EnergyNavigationAnchor,
) -> CommandResult<greenlens_runner::energy_navigation::EnergyPairNavigationResult> {
    blocking(move || {
        greenlens_runner::energy_navigation::pair(
            Path::new(&path),
            &project,
            &a,
            &b,
            &request,
            anchor,
        )
    })
    .await
}
#[tauri::command]
pub(crate) async fn energy_compare(
    path: String,
    project: Project,
    a: String,
    b: String,
) -> CommandResult<Value> {
    blocking(move || {
        let data = service::report_data(Path::new(&path), &project, &a, &b)?;
        let presentation = data.presentation()?;
        let typed_comparison = compare_thermal_inputs_v1(
            &data.baseline.model,
            &data.baseline.study,
            &data.proposed.model,
            &data.proposed.study,
        )?;
        Ok(json!({"data":data,"presentation":presentation,"typed_comparison":typed_comparison}))
    })
    .await
}
#[tauri::command]
pub(crate) async fn energy_report_preflight(
    path: String,
    project: Project,
    a: String,
    b: String,
) -> CommandResult<EnergyAttachmentPreflight> {
    blocking(move || service::report_attachment_preflight(Path::new(&path), &project, &a, &b)).await
}
// Tauri exposes these named fields as the bounded report command contract.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub(crate) async fn energy_report(
    app: tauri::AppHandle,
    path: String,
    project: Project,
    a: String,
    b: String,
    destination: String,
    typst_dir: Option<String>,
    html_only: bool,
    portable_zip: bool,
) -> CommandResult<Value> {
    blocking(move || {
        let control = app.state::<ReportControl>();
        let _guard = control.begin()?;
        if let Ok(status) = app.state::<Application>().status() {
            ensure!(
                !matches!(
                    status.state,
                    greenlens_runner::JobState::Queued
                        | greenlens_runner::JobState::Preparing
                        | greenlens_runner::JobState::Running
                ),
                "Finish or cancel the calculation first"
            );
        }

        let config = if html_only {
            None
        } else {
            Some(TypstConfig::discover(typst_dir.as_deref().map(Path::new))?)
        };
        if portable_zip {
            let config = config.context(
                "Portable evidence includes PDF/PNG and requires the pinned Typst renderer",
            )?;
            greenlens_runner::energy_export::export_workbench_pair(
                Path::new(&path),
                &a,
                &b,
                Path::new(&destination),
                &config,
                &control.cancelled,
            )
        } else {
            Ok(serde_json::to_value(service::render_workbench_reports(
                Path::new(&path),
                &project,
                &a,
                &b,
                Path::new(&destination),
                config.as_ref(),
                &control.cancelled,
            )?)?)
        }
    })
    .await
}
#[tauri::command]
pub(crate) fn energy_cancel_report(app: tauri::AppHandle) {
    app.state::<ReportControl>()
        .cancelled
        .store(true, Ordering::SeqCst);
}
#[tauri::command]
pub(crate) async fn energy_open_report(destination: String, path: String) -> CommandResult<()> {
    blocking(move||{
        let target=greenlens_runner::energy_report_workbench::verified_artifact(Path::new(&destination),&path)?;
        #[cfg(windows)] {
            use std::os::windows::ffi::OsStrExt;
            #[link(name="shell32")] unsafe extern "system" {fn ShellExecuteW(window:*mut std::ffi::c_void,verb:*const u16,file:*const u16,parameters:*const u16,directory:*const u16,show:i32)->isize;}
            let target=target.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();let verb="open".encode_utf16().chain(Some(0)).collect::<Vec<_>>();
            let code=unsafe{ShellExecuteW(std::ptr::null_mut(),verb.as_ptr(),target.as_ptr(),std::ptr::null(),std::ptr::null(),1)};
            ensure!(code>32,"Windows could not open this report; select a default application for the file type (code {code})");Ok(())
        }
        #[cfg(not(windows))] {let _=target;anyhow::bail!("Report opening is supported on native Windows")}
    }).await
}
#[tauri::command]
pub(crate) fn dialog_energy_report(
    window: tauri::WebviewWindow,
    portable_zip: bool,
) -> CommandResult<Option<String>> {
    #[cfg(windows)]
    {
        let dialog = file_dialog(
            None,
            if portable_zip {
                "Export energy evidence ZIP"
            } else {
                "Choose a new report directory name"
            },
            &window,
        );
        Ok(if portable_zip {
            dialog
                .add_filter("ZIP evidence", &["zip"])
                .set_file_name("energy-evidence.zip")
                .save_file()
        } else {
            dialog.set_file_name("GreenLens-energy-report").save_file()
        }
        .map(|p| p.to_string_lossy().into_owned()))
    }
    #[cfg(not(windows))]
    {
        let _ = (window, portable_zip);
        Err("Native report dialog is only supported on Windows".into())
    }
}
