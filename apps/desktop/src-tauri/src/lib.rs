use greenlens_core::{Model, ValidatedModel};
use greenlens_radiance::Profile;
use greenlens_runner::{Application, Comparison, JobStatus, Project, ProjectView, RunRecord};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tauri::Manager;

type CommandResult<T> = Result<T, String>;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
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
fn doctor(engine_dir: Option<String>) -> CommandResult<Value> {
    greenlens_runner::doctor(engine_dir.as_deref().map(Path::new)).map_err(error)
}
#[tauri::command]
fn open_project(path: String) -> CommandResult<ProjectView> {
    greenlens_runner::open_project(Path::new(&path)).map_err(error)
}
#[tauri::command]
fn save_project(
    path: String,
    project: Project,
    expected_revision: Option<String>,
) -> CommandResult<ProjectView> {
    greenlens_runner::save_project(Path::new(&path), &project, expected_revision.as_deref())
        .map_err(error)
}
#[tauri::command]
fn save_project_as(
    app: tauri::State<'_, Application>,
    source_path: Option<String>,
    destination: String,
    project: Project,
    expected_source_revision: Option<String>,
    include_pending: Option<bool>,
) -> CommandResult<ProjectView> {
    // Save As is nondestructive recovery and deliberately does not demand that
    // the source manifest still has the UI's previous revision.
    let _ = expected_source_revision;
    app.save_as(
        source_path.as_deref().map(Path::new),
        Path::new(&destination),
        &project,
        include_pending.unwrap_or(true),
    )
    .map_err(error)
}
#[tauri::command]
fn reconcile_completed_run(
    app: tauri::State<'_, Application>,
    path: String,
    expected_revision: String,
) -> CommandResult<ProjectView> {
    app.reconcile_completed_run(Path::new(&path), &expected_revision)
        .map_err(error)
}
#[tauri::command]
fn discard_job(app: tauri::State<'_, Application>) -> CommandResult<()> {
    app.discard_job().map_err(error)
}
#[tauri::command]
fn cancel_and_wait(app: tauri::State<'_, Application>) -> CommandResult<Option<JobStatus>> {
    if app.status().is_err() {
        return Ok(None);
    }
    app.cancel().map_err(error)?;
    app.wait().map(Some).map_err(error)
}
#[tauri::command]
fn prepare_close(app: tauri::State<'_, Application>, discard_pending: bool) -> CommandResult<()> {
    app.prepare_close(discard_pending).map_err(error)
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
fn start_run(
    app: tauri::State<'_, Application>,
    model: Model,
    alternative_id: String,
    engine_dir: Option<String>,
    project_id: Option<String>,
) -> CommandResult<JobStatus> {
    app.start_for_project(
        model,
        alternative_id,
        engine_dir.map(PathBuf::from),
        Profile::shipping(),
        project_id,
    )
    .map_err(error)
}
#[tauri::command]
fn job_status(app: tauri::State<'_, Application>) -> CommandResult<JobStatus> {
    app.status().map_err(error)
}
#[tauri::command]
fn cancel_run(app: tauri::State<'_, Application>) -> CommandResult<JobStatus> {
    app.cancel().map_err(error)
}
#[tauri::command]
fn collect_run(
    app: tauri::State<'_, Application>,
    path: String,
    project: Project,
    expected_revision: Option<String>,
) -> CommandResult<ProjectView> {
    app.collect(Path::new(&path), &project, expected_revision.as_deref())
        .map_err(error)
}
#[tauri::command]
fn compare_runs(a: RunRecord, b: RunRecord) -> CommandResult<Comparison> {
    greenlens_runner::compare_runs(&a, &b).map_err(error)
}
#[tauri::command]
fn export_project(path: String, destination: String) -> CommandResult<Value> {
    greenlens_runner::export_project(Path::new(&path), Path::new(&destination)).map_err(error)?;
    Ok(json!({"path":destination}))
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
            export_project
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
