//! Read-only intake reservations keep cancellation separate from simulation jobs.
use crate::{blocking, error, CommandResult};
use greenlens_runner::{
    project_open::{self, OpenProgress, ProjectAdmission},
    ProjectView,
};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use tauri::{ipc::Channel, Manager};

#[derive(Default)]
pub struct IntakeControl {
    next: AtomicU64,
    active: Mutex<Option<(u64, Arc<AtomicBool>, bool)>>,
}
impl IntakeControl {
    pub(crate) fn start(&self, ticket: u64) -> CommandResult<Arc<AtomicBool>> {
        let mut active = self.active.lock().map_err(error)?;
        let (_, cancel, started) = active
            .as_mut()
            .filter(|(id, _, _)| *id == ticket)
            .ok_or("Intake reservation expired")?;
        if *started {
            return Err("Intake operation already started".into());
        }
        *started = true;
        Ok(cancel.clone())
    }
    pub(crate) fn finish(&self, ticket: u64) -> CommandResult<()> {
        let mut active = self.active.lock().map_err(error)?;
        if active.as_ref().is_some_and(|(id, _, _)| *id == ticket) {
            *active = None;
        }
        Ok(())
    }
}

#[tauri::command]
pub async fn admit_project(path: String) -> CommandResult<ProjectAdmission> {
    blocking(move || project_open::admit_project(Path::new(&path))).await
}

#[tauri::command]
pub fn prepare_project_verification(control: tauri::State<IntakeControl>) -> CommandResult<u64> {
    let mut active = control.active.lock().map_err(error)?;
    if active.is_some() {
        return Err("A project verification is already active; cancel or await it first.".into());
    }
    let ticket = control.next.fetch_add(1, Ordering::Relaxed) + 1;
    *active = Some((ticket, Arc::new(AtomicBool::new(false)), false));
    Ok(ticket)
}

#[tauri::command]
pub fn cancel_project_verification(
    control: tauri::State<IntakeControl>,
    ticket: u64,
) -> CommandResult<()> {
    let mut active = control.active.lock().map_err(error)?;
    if let Some((_, cancel, started)) = active.as_ref().filter(|(id, _, _)| *id == ticket) {
        cancel.store(true, Ordering::Relaxed);
        if !started {
            *active = None;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn verify_project(
    app: tauri::AppHandle,
    path: String,
    admission: ProjectAdmission,
    ticket: u64,
    on_progress: Channel<OpenProgress>,
) -> CommandResult<ProjectView> {
    let cancel = {
        let state = app.state::<IntakeControl>();
        let mut active = state.active.lock().map_err(error)?;
        let (_, cancel, started) = active
            .as_mut()
            .filter(|(id, _, _)| *id == ticket)
            .ok_or_else(|| "Project intake reservation expired".to_string())?;
        if *started {
            return Err("Project verification already started".into());
        }
        *started = true;
        cancel.clone()
    };
    let result = blocking(move || {
        project_open::verify_admitted_project(
            Path::new(&path),
            &admission,
            cancel,
            move |progress| {
                let _ = on_progress.send(progress);
            },
        )
    })
    .await;
    let state = app.state::<IntakeControl>();
    let mut active = state.active.lock().map_err(error)?;
    if active.as_ref().is_some_and(|(id, _, _)| *id == ticket) {
        *active = None;
    }
    result
}
