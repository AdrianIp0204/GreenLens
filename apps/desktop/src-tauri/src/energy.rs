//! Thin desktop calls into the shared Rust energy services. No frontend physics.
use super::*;
use greenlens_core::energy::*;
use greenlens_runner::{energy_report::TypstConfig, energy_services as service};
use serde::Deserialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Condvar, Mutex,
};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum MetadataChange {
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
pub(crate) async fn energy_import_weather(
    path: String,
    project: Project,
    expected_revision: Option<String>,
    file: String,
    source: greenlens_runner::weather::WeatherSource,
    calendar: EnergyCalendar,
    apply_location: bool,
) -> CommandResult<ProjectView> {
    blocking(move || {
        service::import_weather(
            Path::new(&path),
            &project,
            expected_revision.as_deref(),
            Path::new(&file),
            source,
            calendar,
            apply_location,
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
pub(crate) async fn energy_compare(
    path: String,
    project: Project,
    a: String,
    b: String,
) -> CommandResult<Value> {
    blocking(move || {
        let data = service::report_data(Path::new(&path), &project, &a, &b)?;
        let presentation = data.presentation()?;
        Ok(json!({"data":data,"presentation":presentation}))
    })
    .await
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
            greenlens_runner::energy_export::export_pair(
                Path::new(&path),
                &a,
                &b,
                Path::new(&destination),
                &config,
                &control.cancelled,
            )
        } else {
            Ok(serde_json::to_value(service::render_reports(
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
