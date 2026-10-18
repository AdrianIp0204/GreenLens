//! Typed analytical-only design report actions; scientific work remains shared.
use crate::{blocking, energy::ReportControl, CommandResult};
use anyhow::Context;
use greenlens_runner::{
    design_report, design_report_service as service, energy_report::TypstConfig, Application,
    Project,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;
use tauri::Manager;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Action {
    Create {
        a: String,
        b: String,
        scope: String,
        north: Option<String>,
        html_only: bool,
    },
    Recover {
        token: String,
    },
    Regenerate {
        id: String,
        destination: String,
    },
    Export {
        destination: String,
    },
}
#[tauri::command]
pub(crate) async fn design_report_action(
    app: tauri::AppHandle,
    path: String,
    project: Project,
    expected_revision: String,
    action: Action,
    typst_dir: Option<String>,
) -> CommandResult<Value> {
    blocking(move||{
        let control=app.state::<ReportControl>();let _guard=control.begin()?;
        app.state::<Application>().ensure_report_ready()?;
        let root=Path::new(&path);
        match action {
            Action::Create{a,b,scope,north,html_only}=>{
                let config=(!html_only).then(||TypstConfig::discover(typst_dir.as_deref().map(Path::new))).transpose()?;
                match service::create(root,&project,Some(&expected_revision),&a,&b,scope,north,!html_only,config.as_ref(),control.cancelled.clone()){
                    Ok(result)=>Ok(json!({"status":"issued","view":result.view,"report":result.report})),
                    Err(error)=>if let Some(pending)=error.downcast_ref::<service::PendingDesignReportPublication>(){Ok(json!({"status":"pending","recovery_token":pending.recovery_token,"message":pending.to_string()}))}else{Err(error)},
                }
            },
            Action::Recover{token}=>{let result=service::recover(root,&project,Some(&expected_revision),&token,control.cancelled.clone())?;Ok(json!({"status":"issued","view":result.view,"report":result.report}))},
            Action::Regenerate{id,destination}=>{
                let summary=project.design_reports.as_ref().and_then(|i|i.reports.iter().find(|r|r.id==id)).context("Selected design report is not retained")?;
                let config=summary.include_pdf.then(||TypstConfig::discover(typst_dir.as_deref().map(Path::new))).transpose()?;
                let artifacts=design_report::regenerate(root,&project,&id,Path::new(&destination),config.as_ref(),&control.cancelled)?;
                Ok(json!({"destination":destination,"products_sha256":artifacts.inspection.products.products_sha256,"simulations_started":0}))
            },
            Action::Export{destination}=>{
                let receipt=greenlens_runner::design_report_export::export(root,&project,Path::new(&destination),control.cancelled.clone())?;
                Ok(json!({"destination":destination,"receipt":receipt}))
            },
        }
    }).await
}
