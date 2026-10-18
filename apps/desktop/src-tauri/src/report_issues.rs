//! Thin desktop issuance actions; all evidence bindings remain Rust services.
use crate::{blocking, energy::ReportControl, CommandResult};
use anyhow::{ensure, Context};
use greenlens_runner::{
    energy_report::TypstConfig, report_issues as service, Application, JobState, Project,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;
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

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProductAction {
    Preview,
    PreviewPdf,
    Open,
    SaveCopy,
    Reveal,
}

#[cfg(windows)]
#[link(name = "ole32")]
unsafe extern "system" {
    fn CoInitializeEx(reserved: *mut std::ffi::c_void, flags: u32) -> i32;
    fn CoUninitialize();
}

#[cfg(windows)]
fn check_shell_com_initialization(result: i32) -> anyhow::Result<()> {
    const RPC_E_CHANGED_MODE: i32 = 0x80010106u32 as i32;
    ensure!(result != RPC_E_CHANGED_MODE, "Windows report handoff requires an STA thread, but this worker already has an incompatible COM apartment (HRESULT 0x80010106). No handoff was requested. The verified copy remains available; use the in-app preview or Save Copy");
    ensure!(result >= 0, "Windows report handoff could not initialize COM (HRESULT {:#010x}). No handoff was requested. The verified copy remains available", result as u32);
    Ok(())
}

/// This guard cannot move to another thread: COM initialization and cleanup
/// must balance on the blocking thread that performs the shell operation.
#[cfg(windows)]
struct ShellApartment(std::marker::PhantomData<std::rc::Rc<()>>);

#[cfg(windows)]
impl ShellApartment {
    fn enter() -> anyhow::Result<Self> {
        const COINIT_APARTMENTTHREADED: u32 = 0x2;
        const COINIT_DISABLE_OLE1DDE: u32 = 0x4;
        // SAFETY: the reserved pointer is null and the flags request an STA
        // only on the current blocking thread; no global setting is changed.
        let result = unsafe {
            CoInitializeEx(
                std::ptr::null_mut(),
                COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE,
            )
        };
        check_shell_com_initialization(result)?;
        // Both S_OK and S_FALSE own one matching CoUninitialize call. Failed
        // initialization (including changed mode) must not create this guard.
        Ok(Self(std::marker::PhantomData))
    }
}

#[cfg(windows)]
impl Drop for ShellApartment {
    fn drop(&mut self) {
        // SAFETY: the non-Send guard exists only after successful initialization
        // and is dropped on that same thread, including early error returns.
        unsafe { CoUninitialize() };
    }
}

#[cfg(windows)]
fn shell_path_from_canonical(target: &Path) -> anyhow::Result<std::path::PathBuf> {
    use std::{
        ffi::OsStr,
        os::windows::ffi::OsStrExt,
        path::{Component, PathBuf, Prefix},
    };
    // Only recognized filesystem namespaces may lose an extended-path prefix.
    // Preserve UTF-16 (including non-UTF-8 Windows names); never use lossy text.
    fn component(value: &OsStr) -> anyhow::Result<()> {
        let units = value.encode_wide().collect::<Vec<_>>();
        ensure!(!units.is_empty() && !matches!(units.last(), Some(32 | 46))
            && units.iter().all(|u| *u >= 32 && ![34, 42, 47, 58, 60, 62, 63, 92, 124].contains(u)),
            "The physical report path cannot be represented safely for a Windows viewer; use the in-app preview or Save Copy");
        Ok(())
    }
    let mut parts = target.components();
    let Some(Component::Prefix(prefix)) = parts.next() else {
        anyhow::bail!("Windows report handoff requires an absolute physical filesystem path");
    };
    let mut result = match prefix.kind() {
        Prefix::VerbatimDisk(drive) | Prefix::Disk(drive) if drive.is_ascii_alphabetic() => {
            PathBuf::from(format!("{}:\\", char::from(drive)))
        }
        Prefix::VerbatimUNC(server, share) | Prefix::UNC(server, share) => {
            component(server)?;
            component(share)?;
            let mut value = std::ffi::OsString::from("\\\\");
            value.push(server);
            value.push("\\");
            value.push(share);
            value.push("\\");
            PathBuf::from(value)
        }
        _ => anyhow::bail!("Unsupported physical path namespace for Windows report handoff; the verified copy is preserved"),
    };
    ensure!(
        matches!(parts.next(), Some(Component::RootDir)),
        "Windows report handoff requires a rooted filesystem path"
    );
    for part in parts {
        let Component::Normal(name) = part else {
            anyhow::bail!("Unresolved physical report path component; no viewer was requested");
        };
        component(name)?;
        result.push(name);
    }
    ensure!(
        result.as_os_str().encode_wide().count() < 32767,
        "Physical report path exceeds the Windows UTF-16 bound; it was not truncated"
    );
    Ok(result)
}

#[cfg(windows)]
fn physical_handoff_path(target: &Path) -> anyhow::Result<std::path::PathBuf> {
    // Packaged launch environments may redirect a logical LOCALAPPDATA path.
    // An external viewer does not necessarily inherit that redirection. Resolve
    // the existing object, then give ShellExecute its physical DOS/UNC spelling.
    let canonical = std::fs::canonicalize(target)
        .context("Cannot resolve the existing verified report copy for Windows handoff")?;
    let result = shell_path_from_canonical(&canonical)?;
    ensure!(
        std::fs::canonicalize(&result)? == canonical,
        "Windows report path normalization changed its target; no viewer was requested"
    );
    Ok(result)
}

#[cfg(windows)]
fn request_handoff(target: &Path) -> anyhow::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let target = physical_handoff_path(target)?;
    let _apartment = ShellApartment::enter()?;
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            window: *mut std::ffi::c_void,
            verb: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show: i32,
        ) -> isize;
    }
    let file = target
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let verb = "open".encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    // Windows accepts a filename, not a command string; no shell interpolation.
    let code = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            1,
        )
    };
    ensure!(code>32,"Windows handoff failed (code {code}). The verified copy remains available. Use Save Copy or Open copy folder; a registered PDF viewer may be missing or broken");
    Ok(())
}
#[cfg(not(windows))]
fn request_handoff(_target: &Path) -> anyhow::Result<()> {
    anyhow::bail!("Native report handoff requires Windows")
}

/// Only products enumerated by a verified immutable issue may be opened. HTML
/// and imported SVG markup are not executed. The frontend renders owned SVG as
/// an image data URL and never inserts it through innerHTML/object/iframe.
#[tauri::command]
#[allow(clippy::too_many_arguments)] // Tauri transports the query and cancellable progress channel separately.
pub(crate) async fn report_issue_product(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    path: String,
    project: Project,
    id: String,
    product: String,
    action: ProductAction,
    design_report: Option<bool>,
    ticket: u64,
    on_progress: tauri::ipc::Channel<greenlens_runner::project_open::OpenProgress>,
) -> CommandResult<Value> {
    let cancel = app
        .state::<crate::evidence_inspection::EvidenceControl>()
        .start(ticket)?;
    let result=blocking(move|| {
        let (media_type,bytes)=if design_report.unwrap_or(false) {
            greenlens_runner::project_open::inspect_evidence(Path::new(&path),cancel.clone(),move|progress|{let _=on_progress.send(progress);},||greenlens_runner::design_report::product_bytes(Path::new(&path),&project,&id,&product,&cancel))?
        }else{service::product_bytes_with_progress(Path::new(&path),&project,&id,&product,action==ProductAction::Preview,cancel.clone(),move|progress|{let _=on_progress.send(progress);})?};
        ensure!(!cancel.load(std::sync::atomic::Ordering::Relaxed),"Report product inspection cancelled; no product was opened");
        let sha256=greenlens_runner::persistence::bytes_hash(&bytes);
        if matches!(action, ProductAction::Preview | ProductAction::PreviewPdf) {
            ensure!(bytes.len()<=8*1024*1024,"Preview exceeds 8 MiB; open the preserved PDF instead");
            if action==ProductAction::PreviewPdf { ensure!(media_type=="application/pdf", "PDF preview requires an issued PDF product"); }
            return if media_type=="image/svg+xml" {
                Ok(json!({"path":product,"issue_id":id,"sha256":sha256,"media_type":media_type,"svg":String::from_utf8(bytes)?}))
            } else {Ok(json!({"path":product,"issue_id":id,"sha256":sha256,"media_type":media_type,"bytes":bytes}))};
        }
        ensure!(media_type=="application/pdf" || media_type=="image/png","Only retained PDF/PNG products may be opened externally");
        let extension=if media_type=="application/pdf" {"pdf"} else {"png"};
        let copied=if action==ProductAction::SaveCopy {
            #[cfg(windows)] {
                let destination=crate::file_dialog(None,"Save verified report copy to a new filename",&window)
                    .add_filter("Retained report product",&[extension])
                    .set_file_name(Path::new(&product).file_name().context("Product filename missing")?.to_string_lossy())
                    .save_file();
                let Some(destination)=destination else {return Ok(json!({"path":product,"issue_id":id,"sha256":sha256,"status":"cancelled","integrity":"verified","copy":"not_prepared","handoff":"not_requested","viewer_display":"not_observed"}));};
                greenlens_runner::report_product_delivery::save_copy(&destination,&bytes,&sha256,extension,&cancel)?
            }
            #[cfg(not(windows))] {let _=window;anyhow::bail!("Save Copy requires a native Windows dialog");}
        } else {
            greenlens_runner::report_product_delivery::prepare_cached(&greenlens_runner::local_root()?.join("report-product-copies-v1"),&bytes,&sha256,extension,&cancel)?
        };
        let (handoff,message)=if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            ("not_requested",Some("The verified copy was completed before cancellation. No viewer or folder handoff was requested.".to_owned()))
        } else if action==ProductAction::SaveCopy {("not_requested",None)} else {
            let target=if action==ProductAction::Reveal {copied.path.parent().context("Copy folder missing")?} else {&copied.path};
            match request_handoff(target) { Ok(())=>("requested",None),Err(error)=>("failed",Some(error.to_string())) }
        };
        Ok(json!({"schema_version":"greenlens.report-product-delivery.v1","path":product,"issue_id":id,"sha256":sha256,"status":"prepared","integrity":"verified","copy":"verified","prepared":copied,"handoff":handoff,"viewer_display":"not_observed","message":message}))
    }).await;
    app.state::<crate::evidence_inspection::EvidenceControl>()
        .finish(ticket)?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn shell_paths_preserve_dos_unc_and_utf16_without_guessing_namespaces() {
        use std::{
            ffi::OsString,
            os::windows::ffi::{OsStrExt, OsStringExt},
        };
        for (source, expected) in [
            (r"\\?\C:\cache 空間\report.pdf", r"C:\cache 空間\report.pdf"),
            (
                r"\\?\UNC\server\share\cache 空間\report.pdf",
                r"\\server\share\cache 空間\report.pdf",
            ),
            (r"C:\cache\report.pdf", r"C:\cache\report.pdf"),
            (r"\\server\share\report.pdf", r"\\server\share\report.pdf"),
        ] {
            assert_eq!(
                shell_path_from_canonical(Path::new(source)).unwrap(),
                Path::new(expected)
            );
        }
        let mut raw = r"\\?\C:\cache\".encode_utf16().collect::<Vec<_>>();
        raw.push(0xd800); // Windows paths need not be valid Unicode scalar strings.
        raw.extend(".pdf".encode_utf16());
        let source = OsString::from_wide(&raw);
        assert_eq!(
            shell_path_from_canonical(Path::new(&source))
                .unwrap()
                .as_os_str()
                .encode_wide()
                .collect::<Vec<_>>(),
            raw[4..]
        );
        for source in [
            r"report.pdf",
            r"C:report.pdf",
            r"\\.\pipe\report",
            r"\\?\GLOBALROOT\Device\HarddiskVolume1\report.pdf",
            r"\\?\Volume{00000000-0000-0000-0000-000000000000}\report.pdf",
            r"\\?\C:\cache\..\report.pdf",
            r"\\?\C:\cache\report.pdf.",
            r"\\?\C:\cache\report.pdf ",
            r"\\?\C:\cache\report.pdf:stream",
            r"\\?\UNC\server\",
        ] {
            assert!(
                shell_path_from_canonical(Path::new(source)).is_err(),
                "{source}"
            );
        }
        assert!(shell_path_from_canonical(Path::new("\\\\?\\C:\\cache\\bad\0.pdf")).is_err());
        assert!(
            shell_path_from_canonical(Path::new(&format!("\\\\?\\C:\\{}", "a".repeat(32767))))
                .is_err()
        );
    }

    #[cfg(windows)]
    #[test]
    fn handoff_resolution_preserves_existing_copy_bytes_and_directory_identity() {
        let base = greenlens_runner::local_root().unwrap().join("test-scratch");
        std::fs::create_dir_all(&base).unwrap();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = base.join(format!("shell-path-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let copy = root.join("issued 空間 copy.pdf");
        let bytes = b"%PDF original synthetic identity fixture";
        std::fs::write(&copy, bytes).unwrap();
        for target in [&root, &copy] {
            let handed = physical_handoff_path(target).unwrap();
            assert_eq!(
                std::fs::canonicalize(handed).unwrap(),
                std::fs::canonicalize(target).unwrap()
            );
        }
        assert_eq!(std::fs::read(&copy).unwrap(), bytes);
        assert!(physical_handoff_path(&root.join("absent.pdf")).is_err());
        std::fs::remove_file(copy).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn shell_com_accepts_both_success_codes_and_rejects_changed_apartment() {
        assert!(check_shell_com_initialization(0).is_ok()); // S_OK
        assert!(check_shell_com_initialization(1).is_ok()); // S_FALSE; still balanced
        let changed = check_shell_com_initialization(0x80010106u32 as i32)
            .unwrap_err()
            .to_string();
        assert!(changed.contains("incompatible COM apartment"));
        assert!(changed.contains("No handoff was requested"));
        let failed = check_shell_com_initialization(0x8007000eu32 as i32)
            .unwrap_err()
            .to_string();
        assert!(failed.contains("0x8007000e"));
        assert!(failed.contains("verified copy remains available"));
    }

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
