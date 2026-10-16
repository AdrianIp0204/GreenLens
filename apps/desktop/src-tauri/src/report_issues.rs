//! Thin desktop issuance actions; all evidence bindings remain Rust services.
use crate::{blocking, energy::ReportControl, CommandResult};
use anyhow::{ensure, Context};
use greenlens_runner::{
    energy_report::TypstConfig, report_issues as service, Application, JobState, Project,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{io::Write, path::Path};
use tauri::Manager;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ReportIssueAction {
    Create {
        a: String,
        b: String,
        scope_sha256: String,
        html_only: bool,
    },
    Regenerate {
        id: String,
        destination: String,
    },
    Recover {
        token: String,
    },
    Export {
        destination: String,
    },
}

#[tauri::command]
pub(crate) async fn report_issue_action(
    app: tauri::AppHandle,
    path: String,
    project: Project,
    expected_revision: String,
    action: ReportIssueAction,
    typst_dir: Option<String>,
) -> CommandResult<Value> {
    blocking(move|| {
        let control=app.state::<ReportControl>();let _guard=control.begin()?;
        if let Ok(status)=app.state::<Application>().status() {
            ensure!(!matches!(status.state,JobState::Queued|JobState::Preparing|JobState::Running),"Finish or cancel the calculation before issuing, regenerating or exporting reports");
        }
        let root=Path::new(&path);
        match action {
            ReportIssueAction::Create {a,b,scope_sha256,html_only}=>{
                let config=(!html_only).then(||TypstConfig::discover(typst_dir.as_deref().map(Path::new))).transpose()?;
                match service::issue(root,&project,Some(&expected_revision),&a,&b,&scope_sha256,!html_only,config.as_ref(),control.cancelled.clone()) {
                    Ok(result)=>Ok(json!({"status":"issued","view":result.view,"issue":result.issue})),
                    Err(error)=>match error.downcast_ref::<service::PendingIssuePublication>() {
                        Some(pending)=>Ok(json!({"status":"pending","recovery_token":pending.recovery_token,"message":pending.to_string()})),
                        None=>Err(error),
                    },
                }
            }
            ReportIssueAction::Regenerate {id,destination}=>{
                service::validate_index(&project)?;
                let include_pdf=project.report_issues.as_ref().and_then(|index|index.issues.iter().find(|s|s.id==id)).context("Selected report issue is not retained")?.include_pdf;
                let config=include_pdf.then(||TypstConfig::discover(typst_dir.as_deref().map(Path::new))).transpose()?;
                let artifacts=service::regenerate(root,&project,&id,Path::new(&destination),config.as_ref(),control.cancelled.clone())?;
                Ok(json!({"issue_id":id,"destination":destination,"artifacts":artifacts,"verification":"Every regenerated deterministic product matched its original issued hash; no EnergyPlus calculation was performed."}))
            }
            ReportIssueAction::Recover {token}=>{let result=service::publish_staged_issue(root,&project,Some(&expected_revision),&token,control.cancelled.clone())?;Ok(json!({"status":"issued","view":result.view,"issue":result.issue}))},
            ReportIssueAction::Export {destination}=>greenlens_runner::report_issue_export::export(root,&project,Path::new(&destination),control.cancelled.clone()),
        }
    }).await
}

/// Only products enumerated by a verified immutable issue may be opened. HTML
/// and imported SVG markup are not executed. The frontend renders owned SVG as
/// an image data URL and never inserts it through innerHTML/object/iframe.
#[tauri::command]
pub(crate) async fn report_issue_product(
    app: tauri::AppHandle,
    path: String,
    project: Project,
    id: String,
    product: String,
    open: bool,
) -> CommandResult<Value> {
    blocking(move|| {
        let control=app.state::<ReportControl>();let _guard=control.begin()?;
        let (media_type,bytes)=service::product_bytes(Path::new(&path),&project,&id,&product,!open,control.cancelled.clone())?;
        let sha256=greenlens_runner::persistence::bytes_hash(&bytes);
        if !open {
            ensure!(bytes.len()<=8*1024*1024,"Preview exceeds 8 MiB; open the preserved PDF instead");
            return if media_type=="image/svg+xml" {
                Ok(json!({"path":product,"issue_id":id,"sha256":sha256,"media_type":media_type,"svg":String::from_utf8(bytes)?}))
            } else {Ok(json!({"path":product,"issue_id":id,"sha256":sha256,"media_type":media_type,"bytes":bytes}))};
        }
        ensure!(media_type=="application/pdf" || media_type=="image/png","Only retained PDF/PNG products may be opened externally");
        // Open a private verified copy, not a project filename that another
        // writer could replace between verification and the viewer's read.
        let copy=greenlens_runner::local_root()?.join("report-product-open").join(format!("{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos()));
        std::fs::create_dir_all(&copy)?;
        let name=Path::new(&product).file_name().context("Product filename missing")?;
        let target=copy.join(name);let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&target)?;
        file.write_all(&bytes)?;file.sync_all()?;drop(file);
        #[cfg(windows)] {
            use std::os::windows::ffi::OsStrExt;
            #[link(name="shell32")] unsafe extern "system" {fn ShellExecuteW(window:*mut std::ffi::c_void,verb:*const u16,file:*const u16,parameters:*const u16,directory:*const u16,show:i32)->isize;}
            let target=target.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<_>>();let verb="open".encode_utf16().chain(Some(0)).collect::<Vec<_>>();
            let code=unsafe{ShellExecuteW(std::ptr::null_mut(),verb.as_ptr(),target.as_ptr(),std::ptr::null(),std::ptr::null(),1)};
            ensure!(code>32,"Windows could not open the retained product; choose a default PDF/image viewer (code {code})");
            Ok(json!({"path":product,"opened":true,"issue_id":id,"sha256":sha256}))
        }
        #[cfg(not(windows))] {let _=target;anyhow::bail!("Opening retained report products requires native Windows")}
    }).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actions_are_bounded_and_unknown_execution_fields_are_rejected() {
        let mut create =
            json!({"kind":"create","a":"a","b":"b","scope_sha256":"scope","html_only":true});
        assert!(serde_json::from_value::<ReportIssueAction>(create.clone()).is_ok());
        create["shell"] = json!("not supported");
        assert!(serde_json::from_value::<ReportIssueAction>(create).is_err());
        assert!(serde_json::from_value::<ReportIssueAction>(
            json!({"kind":"execute","arguments":[]})
        )
        .is_err());
        assert!(serde_json::from_value::<ReportIssueAction>(
            json!({"kind":"recover","token":"token"})
        )
        .is_ok());
    }
}
