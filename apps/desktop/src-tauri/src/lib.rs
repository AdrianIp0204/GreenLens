use anyhow::{ensure, Context};
use greenlens_core::{
    study::{MetricSettings, Study, StudyMode},
    weather::WeatherSourceProfile,
    Model, ValidatedModel,
};
use greenlens_radiance::Profile;
use greenlens_runner::{
    climate::{self, ClimateRunSummary},
    studies, weather, Application, Comparison, JobStatus, Project, ProjectView, RunRecord,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    io::Read,
    path::{Path, PathBuf},
};
use tauri::Manager;

type CommandResult<T> = Result<T, String>;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
// Parsing, checksums, copies and metric derivation can touch a complete year of
// evidence. Keep those operations off the window thread; IPC returns only the
// service's compact metadata or a bounded result slice.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> CommandResult<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(error)?
        .map_err(error)
}
#[tauri::command]
fn example() -> Project {
    greenlens_runner::example()
}
#[tauri::command]
fn validate(model: Model) -> CommandResult<ValidatedModel> {
    greenlens_core::validate_model(&model).map_err(error)
}
#[tauri::command]
async fn doctor(engine_dir: Option<String>) -> CommandResult<Value> {
    blocking(move || greenlens_runner::doctor(engine_dir.as_deref().map(Path::new))).await
}
#[tauri::command]
async fn open_project(path: String) -> CommandResult<ProjectView> {
    blocking(move || greenlens_runner::open_project(Path::new(&path))).await
}
#[tauri::command]
async fn save_project(
    path: String,
    project: Project,
    expected_revision: Option<String>,
) -> CommandResult<ProjectView> {
    blocking(move || {
        greenlens_runner::save_project(Path::new(&path), &project, expected_revision.as_deref())
    })
    .await
}
#[tauri::command]
async fn save_project_as(
    app: tauri::AppHandle,
    source_path: Option<String>,
    destination: String,
    project: Project,
    expected_source_revision: Option<String>,
    include_pending: Option<bool>,
) -> CommandResult<ProjectView> {
    // Save As is nondestructive recovery and deliberately does not demand that
    // the source manifest still has the UI's previous revision.
    let _ = expected_source_revision;
    blocking(move || {
        app.state::<Application>().save_as(
            source_path.as_deref().map(Path::new),
            Path::new(&destination),
            &project,
            include_pending.unwrap_or(true),
        )
    })
    .await
}
#[tauri::command]
async fn reconcile_completed_run(
    app: tauri::AppHandle,
    path: String,
    expected_revision: String,
) -> CommandResult<ProjectView> {
    blocking(move || {
        app.state::<Application>()
            .reconcile_completed_run(Path::new(&path), &expected_revision)
    })
    .await
}
#[tauri::command]
async fn discard_job(app: tauri::AppHandle) -> CommandResult<()> {
    blocking(move || app.state::<Application>().discard_job()).await
}
#[tauri::command]
async fn cancel_and_wait(app: tauri::AppHandle) -> CommandResult<Option<JobStatus>> {
    blocking(move || {
        let service = app.state::<Application>();
        if service.status().is_err() {
            return Ok(None);
        }
        service.cancel()?;
        service.wait().map(Some)
    })
    .await
}
#[tauri::command]
async fn prepare_close(app: tauri::AppHandle, discard_pending: bool) -> CommandResult<()> {
    blocking(move || app.state::<Application>().prepare_close(discard_pending)).await
}
#[cfg(windows)]
fn file_dialog(
    initial_path: Option<String>,
    title: &str,
    window: &tauri::WebviewWindow,
) -> rfd::FileDialog {
    let dialog = rfd::FileDialog::new().set_title(title).set_parent(window);
    if let Some(path) = initial_path.filter(|path| Path::new(path).is_dir()) {
        dialog.set_directory(path)
    } else {
        dialog
    }
}
#[tauri::command]
fn dialog_open_project(
    window: tauri::WebviewWindow,
    initial_path: Option<String>,
) -> CommandResult<Option<String>> {
    #[cfg(windows)]
    {
        Ok(
            file_dialog(initial_path, "Open GreenLens project directory", &window)
                .pick_folder()
                .map(|p| p.to_string_lossy().into_owned()),
        )
    }
    #[cfg(not(windows))]
    {
        let _ = (window, initial_path);
        Err("Native dialogs are currently supported on Windows".into())
    }
}
#[tauri::command]
fn dialog_save_project(
    window: tauri::WebviewWindow,
    initial_path: Option<String>,
) -> CommandResult<Option<String>> {
    #[cfg(windows)]
    {
        Ok(file_dialog(
            initial_path,
            "Save As: choose or create an empty project directory",
            &window,
        )
        .pick_folder()
        .map(|p| p.to_string_lossy().into_owned()))
    }
    #[cfg(not(windows))]
    {
        let _ = (window, initial_path);
        Err("Native dialogs are currently supported on Windows".into())
    }
}
#[tauri::command]
fn dialog_export_evidence(
    window: tauri::WebviewWindow,
    initial_path: Option<String>,
) -> CommandResult<Option<String>> {
    #[cfg(windows)]
    {
        Ok(file_dialog(
            initial_path,
            "Export GreenLens evidence to a new ZIP",
            &window,
        )
        .add_filter("Evidence ZIP", &["zip"])
        .set_file_name("GreenLens-evidence.zip")
        .save_file()
        .map(|p| p.to_string_lossy().into_owned()))
    }
    #[cfg(not(windows))]
    {
        let _ = (window, initial_path);
        Err("Native dialogs are currently supported on Windows".into())
    }
}
#[tauri::command]
async fn start_run(
    app: tauri::AppHandle,
    model: Model,
    alternative_id: String,
    engine_dir: Option<String>,
    project_id: Option<String>,
) -> CommandResult<JobStatus> {
    blocking(move || {
        app.state::<Application>().start_for_project(
            model,
            alternative_id,
            engine_dir.map(PathBuf::from),
            Profile::shipping(),
            project_id,
        )
    })
    .await
}
#[tauri::command]
async fn job_status(app: tauri::AppHandle) -> CommandResult<JobStatus> {
    blocking(move || app.state::<Application>().status()).await
}
#[tauri::command]
async fn cancel_run(app: tauri::AppHandle) -> CommandResult<JobStatus> {
    blocking(move || app.state::<Application>().cancel()).await
}
#[tauri::command]
async fn collect_run(
    app: tauri::AppHandle,
    path: String,
    project: Project,
    expected_revision: Option<String>,
) -> CommandResult<ProjectView> {
    blocking(move || {
        app.state::<Application>()
            .collect(Path::new(&path), &project, expected_revision.as_deref())
    })
    .await
}
#[tauri::command]
fn compare_runs(a: RunRecord, b: RunRecord) -> CommandResult<Comparison> {
    greenlens_runner::compare_runs(&a, &b).map_err(error)
}
#[tauri::command]
async fn export_project(
    path: String,
    destination: String,
    selected_runs: Option<Vec<String>>,
) -> CommandResult<Value> {
    blocking(move || {
        if let Some(selected) = selected_runs {
            greenlens_runner::climate_export::export_selected(
                Path::new(&path),
                Path::new(&destination),
                &selected,
            )?;
        } else {
            // Old static clients retain their unchanged all-runs export. A
            // climate project must name its immutable snapshots explicitly.
            greenlens_runner::export_project(Path::new(&path), Path::new(&destination))?;
        }
        Ok(json!({"path":destination}))
    })
    .await
}

#[tauri::command]
fn dialog_import_epw(
    window: tauri::WebviewWindow,
    initial_path: Option<String>,
) -> CommandResult<Option<String>> {
    #[cfg(windows)]
    {
        Ok(
            file_dialog(initial_path, "Import hourly weather EPW", &window)
                .add_filter("EnergyPlus weather", &["epw"])
                .pick_file()
                .map(|path| path.to_string_lossy().into_owned()),
        )
    }
    #[cfg(not(windows))]
    {
        let _ = (window, initial_path);
        Err("Native dialogs are currently supported on Windows".into())
    }
}

#[tauri::command]
fn dialog_weather_source(
    window: tauri::WebviewWindow,
    initial_path: Option<String>,
) -> CommandResult<Option<String>> {
    #[cfg(windows)]
    {
        Ok(file_dialog(
            initial_path,
            "Open weather citation and rights JSON",
            &window,
        )
        .add_filter("Weather source record", &["json"])
        .pick_file()
        .map(|path| path.to_string_lossy().into_owned()))
    }
    #[cfg(not(windows))]
    {
        let _ = (window, initial_path);
        Err("Native dialogs are currently supported on Windows".into())
    }
}

#[tauri::command]
async fn weather_inspect(
    path: String,
    profile: WeatherSourceProfile,
) -> CommandResult<weather::WeatherInspection> {
    blocking(move || weather::inspect(Path::new(&path), profile)).await
}

#[tauri::command]
async fn weather_read_source(path: String) -> CommandResult<weather::WeatherSource> {
    blocking(move || {
        let file = std::fs::File::open(&path).context("Read weather source record")?;
        ensure!(
            file.metadata()?.is_file(),
            "Weather source record must be a regular JSON file"
        );
        let mut bytes = Vec::new();
        file.take(32 * 1024 + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= 32 * 1024,
            "Weather source record exceeds 32 KiB"
        );
        let source: weather::WeatherSource = serde_json::from_slice(&bytes)?;
        source.validate()?;
        Ok(source)
    })
    .await
}

#[tauri::command]
async fn weather_import(
    path: String,
    destination: String,
    profile: WeatherSourceProfile,
    source: weather::WeatherSource,
) -> CommandResult<weather::WeatherInspection> {
    blocking(move || weather::import(Path::new(&path), Path::new(&destination), profile, source))
        .await
}

#[tauri::command]
async fn weather_import_project(
    path: String,
    project: Project,
    expected_revision: Option<String>,
    file: String,
    profile: WeatherSourceProfile,
    source: weather::WeatherSource,
) -> CommandResult<ProjectView> {
    blocking(move || {
        studies::import_weather_file(
            Path::new(&path),
            &project,
            expected_revision.as_deref(),
            Path::new(&file),
            profile,
            source,
        )
    })
    .await
}

#[tauri::command]
async fn weather_attach(
    path: String,
    project: Project,
    expected_revision: Option<String>,
    asset_path: String,
) -> CommandResult<ProjectView> {
    blocking(move || {
        studies::import_weather_asset(
            Path::new(&path),
            &project,
            expected_revision.as_deref(),
            Path::new(&asset_path),
        )
    })
    .await
}

#[derive(Serialize)]
struct WeatherAssetInspection {
    asset: weather::WeatherAsset,
    inspection: weather::WeatherInspection,
}

#[tauri::command]
async fn weather_asset_inspect(
    path: String,
    weather_id: String,
) -> CommandResult<WeatherAssetInspection> {
    blocking(move || {
        let (asset, parsed) = studies::project_weather(Path::new(&path), &weather_id)?;
        Ok(WeatherAssetInspection {
            asset,
            inspection: weather::WeatherInspection::from(&parsed.data),
        })
    })
    .await
}

#[tauri::command]
async fn weather_interval(
    path: String,
    weather_id: String,
    interval_index: usize,
) -> CommandResult<studies::WeatherIntervalView> {
    blocking(move || studies::weather_interval(Path::new(&path), &weather_id, interval_index)).await
}

#[tauri::command]
async fn weather_find_interval(
    path: String,
    weather_id: String,
    month: u8,
    day: u8,
    hour_ending: u8,
) -> CommandResult<studies::WeatherIntervalView> {
    blocking(move || {
        studies::weather_find_interval(Path::new(&path), &weather_id, month, day, hour_ending)
    })
    .await
}

#[tauri::command]
async fn run_weather_interval(
    path: String,
    summary: ClimateRunSummary,
    interval_index: usize,
) -> CommandResult<climate::RunWeatherIntervalView> {
    blocking(move || climate::run_weather_interval(Path::new(&path), &summary, interval_index))
        .await
}

#[tauri::command]
async fn run_weather_find_interval(
    path: String,
    summary: ClimateRunSummary,
    month: u8,
    day: u8,
    hour_ending: u8,
) -> CommandResult<climate::RunWeatherIntervalView> {
    blocking(move || {
        climate::run_weather_find_interval(Path::new(&path), &summary, month, day, hour_ending)
    })
    .await
}

#[tauri::command]
fn study_defaults() -> Study {
    Study::default()
}

#[derive(Serialize)]
struct StudyValidation {
    study: Study,
    physical_input_hash: String,
    comparison_input_hash: String,
}

#[tauri::command]
fn study_validate(model: Model, study: Study) -> CommandResult<StudyValidation> {
    Ok(StudyValidation {
        physical_input_hash: study.physical_input_hash(&model).map_err(error)?,
        comparison_input_hash: study.comparison_input_hash(&model).map_err(error)?,
        study,
    })
}

#[tauri::command]
fn study_configure(
    project: Project,
    alternative_id: String,
    study: Study,
) -> CommandResult<Project> {
    studies::configure(&project, &alternative_id, study).map_err(error)
}

#[tauri::command]
fn study_restore(
    project: Project,
    alternative_id: String,
    snapshot: studies::StudyHistorySnapshot,
) -> CommandResult<Project> {
    studies::restore_study(&project, &alternative_id, &snapshot).map_err(error)
}

#[tauri::command]
async fn start_study(
    app: tauri::AppHandle,
    path: Option<String>,
    project: Project,
    alternative_id: String,
    study: Study,
    engine_dir: Option<String>,
) -> CommandResult<JobStatus> {
    blocking(move || {
        // Validate the same explicit document transaction as configuration. The
        // caller retains unsaved edits; this command starts a job, not a save.
        let project = studies::configure(&project, &alternative_id, study.clone())?;
        let model = project
            .alternatives
            .iter()
            .find(|item| item.id == alternative_id)
            .context("Alternative must be A or B")?
            .model
            .clone();
        let weather_path = if matches!(study.calculation, StudyMode::StaticOvercast) {
            None
        } else {
            let root = path
                .as_deref()
                .context("Save the project and import portable weather before calculating")?;
            studies::weather_path(Path::new(root), &project, &study)?
        };
        app.state::<Application>().start_study_for_project(
            model,
            alternative_id,
            engine_dir.map(PathBuf::from),
            study,
            weather_path,
            Some(project.id),
        )
    })
    .await
}

#[tauri::command]
async fn climate_assessment(
    path: String,
    project: Project,
    run_id: String,
) -> CommandResult<greenlens_runner::qualification::RunAssessment> {
    blocking(move || {
        greenlens_runner::qualification::assess_climate(Path::new(&path), &project, &run_id)
    })
    .await
}

#[tauri::command]
async fn current_climate_result(
    path: String,
    project: Project,
    alternative_id: String,
    study: Study,
) -> CommandResult<studies::CurrentClimateResult> {
    blocking(move || studies::current_result(Path::new(&path), &project, &alternative_id, &study))
        .await
}

#[tauri::command]
async fn climate_result_values(
    path: String,
    summary: ClimateRunSummary,
    field: studies::ResultField,
    interval_index: usize,
    settings: Option<MetricSettings>,
) -> CommandResult<studies::ClimateValues> {
    blocking(move || {
        studies::result_values(Path::new(&path), &summary, field, interval_index, settings)
    })
    .await
}

#[derive(Serialize)]
struct ClimateSensorSeries {
    run_id: String,
    sensor_id: String,
    units: &'static str,
    time_axis_sha256: String,
    first_interval_index: usize,
    values: Vec<f64>,
}

#[tauri::command]
async fn climate_sensor_series(
    path: String,
    summary: ClimateRunSummary,
    sensor_id: String,
) -> CommandResult<ClimateSensorSeries> {
    blocking(move || {
        let (record, reader) = climate::open_series_for_run(Path::new(&path), &summary)?;
        let first_interval_index = match record.study.calculation {
            StudyMode::WeatherHour { interval_index, .. } => interval_index,
            StudyMode::Annual { .. } => 0,
            StudyMode::StaticOvercast => anyhow::bail!("Use the legacy static result reader"),
        };
        // One identified sensor only, at most 8,760 scalar illuminances. The
        // complete sensor-by-time transport array never crosses IPC.
        let values = reader.sensor(&sensor_id)?;
        Ok(ClimateSensorSeries {
            run_id: record.id,
            sensor_id,
            units: "lx",
            time_axis_sha256: record.weather_asset.binding.time_axis_sha256,
            first_interval_index,
            values,
        })
    })
    .await
}

#[tauri::command]
async fn climate_derive_metrics(
    path: String,
    summary: ClimateRunSummary,
    settings: MetricSettings,
) -> CommandResult<studies::DerivedMetrics> {
    blocking(move || studies::derive_metrics(Path::new(&path), &summary, settings)).await
}

#[tauri::command]
async fn climate_compare(
    path: String,
    a: ClimateRunSummary,
    b: ClimateRunSummary,
    field: studies::ResultField,
    interval_index: usize,
) -> CommandResult<studies::ClimateComparison> {
    blocking(move || studies::compare(Path::new(&path), &a, &b, field, interval_index)).await
}

pub fn run() {
    tauri::Builder::default()
        .manage(Application::default())
        .invoke_handler(tauri::generate_handler![
            example,
            validate,
            doctor,
            open_project,
            save_project,
            save_project_as,
            reconcile_completed_run,
            discard_job,
            cancel_and_wait,
            prepare_close,
            dialog_open_project,
            dialog_save_project,
            dialog_export_evidence,
            start_run,
            job_status,
            cancel_run,
            collect_run,
            compare_runs,
            export_project,
            dialog_import_epw,
            dialog_weather_source,
            weather_inspect,
            weather_read_source,
            weather_import,
            weather_import_project,
            weather_attach,
            weather_asset_inspect,
            weather_interval,
            weather_find_interval,
            run_weather_interval,
            run_weather_find_interval,
            study_defaults,
            study_validate,
            study_configure,
            study_restore,
            start_study,
            current_climate_result,
            climate_assessment,
            climate_result_values,
            climate_sensor_series,
            climate_derive_metrics,
            climate_compare
        ])
        .build(tauri::generate_context!())
        .expect("failed to build GreenLens desktop")
        .run(|app, event| {
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                app.state::<Application>().shutdown();
            }
        });
}
