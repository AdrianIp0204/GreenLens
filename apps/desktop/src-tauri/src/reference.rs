//! Bounded inert reference operations. No imported command or external URL runs.
use crate::{blocking, project_intake::IntakeControl, CommandResult};
use greenlens_core::{
    architectural_scope::{ArchitecturalScope, ScopeAssessment},
    reference::{ReferenceImportOptions, ReferenceModel, ReferenceProgress, ReferenceTriangle},
};
use greenlens_runner::{reference as service, Project, ProjectView};
use std::path::Path;
use tauri::{ipc::Channel, Manager};

#[tauri::command]
pub async fn reference_revision_prepare(
    path: String,
    project: Project,
    parent_revision_sha256: String,
    candidate_scope: Option<ArchitecturalScope>,
) -> CommandResult<greenlens_core::study_revision::CarryForwardPlan> {
    blocking(move || {
        let cancel = std::sync::atomic::AtomicBool::new(false);
        match candidate_scope {
            Some(scope) => greenlens_runner::study_revisions::prepare_candidate(
                Path::new(&path),
                &project,
                &parent_revision_sha256,
                &scope,
                &cancel,
            ),
            None => greenlens_runner::study_revisions::prepare(
                Path::new(&path),
                &project,
                &parent_revision_sha256,
                &cancel,
            ),
        }
    })
    .await
}
#[tauri::command]
pub async fn reference_revision_accept(
    path: String,
    project: Project,
    parent_revision_sha256: String,
    expected_plan_sha256: String,
    scope: ArchitecturalScope,
    decisions: Vec<greenlens_core::study_revision::ReviewDecision>,
) -> CommandResult<Project> {
    blocking(move || {
        greenlens_runner::study_revisions::accept(
            Path::new(&path),
            &project,
            &parent_revision_sha256,
            &expected_plan_sha256,
            scope,
            decisions,
            &std::sync::atomic::AtomicBool::new(false),
        )
    })
    .await
}
#[tauri::command]
pub async fn reference_revision_activate(
    project: Project,
    revision_sha256: Option<String>,
) -> CommandResult<Project> {
    blocking(move || {
        greenlens_runner::study_revisions::activate(&project, revision_sha256.as_deref())
    })
    .await
}

#[tauri::command]
pub fn dialog_open_reference_glb(
    window: tauri::WebviewWindow,
    initial_path: Option<String>,
) -> CommandResult<Option<String>> {
    #[cfg(windows)]
    {
        Ok(crate::file_dialog(
            initial_path,
            "Import local GLB reference (read only)",
            &window,
        )
        .add_filter("GLB 2.0", &["glb"])
        .pick_file()
        .map(|p| p.to_string_lossy().into_owned()))
    }
    #[cfg(not(windows))]
    {
        let _ = (window, initial_path);
        Err("Native dialogs require Windows".into())
    }
}
#[tauri::command]
pub async fn reference_manifest(
    path: String,
    project: Project,
    asset_id: Option<String>,
) -> CommandResult<ReferenceModel> {
    blocking(move || {
        greenlens_runner::validate_project(&project)?;
        service::read_manifest(
            Path::new(&path),
            service::asset(&project, asset_id.as_deref())?,
        )
    })
    .await
}
#[tauri::command]
pub async fn reference_chunk(
    path: String,
    project: Project,
    asset_id: String,
    index: usize,
) -> CommandResult<Vec<ReferenceTriangle>> {
    blocking(move || {
        greenlens_runner::validate_project(&project)?;
        service::read_chunk(
            Path::new(&path),
            service::asset(&project, Some(&asset_id))?,
            index,
        )
    })
    .await
}
#[tauri::command]
pub async fn reference_measure(
    path: String,
    project: Project,
    asset_id: String,
    a: service::ReferenceVertex,
    b: service::ReferenceVertex,
) -> CommandResult<service::ReferenceMeasurement> {
    blocking(move || {
        greenlens_runner::validate_project(&project)?;
        service::measure(
            Path::new(&path),
            service::asset(&project, Some(&asset_id))?,
            a,
            b,
        )
    })
    .await
}
#[tauri::command]
pub async fn reference_scope_context(
    path: String,
    project: Project,
) -> CommandResult<serde_json::Value> {
    blocking(move || service::scope_context(Path::new(&path), &project)).await
}
#[tauri::command]
pub async fn reference_scope_assess(
    path: String,
    project: Project,
    scope: ArchitecturalScope,
) -> CommandResult<ScopeAssessment> {
    blocking(move || service::assess_scope(Path::new(&path), &project, &scope)).await
}
#[tauri::command]
pub async fn reference_scope_apply(
    path: String,
    project: Project,
    scope: Option<ArchitecturalScope>,
    restoring: Option<bool>,
) -> CommandResult<Project> {
    blocking(move || {
        service::set_scope(
            Path::new(&path),
            &project,
            scope,
            restoring.unwrap_or(false),
        )
    })
    .await
}
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Named IPC fields retain explicit revision/cancellation bindings.
pub async fn reference_import(
    app: tauri::AppHandle,
    path: String,
    project: Project,
    expected_revision: Option<String>,
    file: String,
    options: ReferenceImportOptions,
    ticket: u64,
    on_progress: Channel<ReferenceProgress>,
) -> CommandResult<ProjectView> {
    let cancel = app.state::<IntakeControl>().start(ticket)?;
    let result = blocking(move || {
        service::import_project(
            Path::new(&path),
            &project,
            expected_revision.as_deref(),
            Path::new(&file),
            &options,
            &cancel,
            |event| on_progress.send(event.clone()).is_ok(),
        )
    })
    .await;
    app.state::<IntakeControl>().finish(ticket)?;
    result
}
