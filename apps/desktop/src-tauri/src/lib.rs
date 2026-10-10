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
fn start_run(
    app: tauri::State<'_, Application>,
    model: Model,
    alternative_id: String,
    engine_dir: Option<String>,
) -> CommandResult<JobStatus> {
    app.start(
        model,
        alternative_id,
        engine_dir.map(PathBuf::from),
        Profile::shipping(),
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
