//! Analytical-only paired reporting through the shared desktop services.
//! Rendering consumes retained evidence; only the explicit replay command runs an engine.
use anyhow::{ensure, Result};
use clap::Subcommand;
use greenlens_runner::{
    design_report as report, design_report_export as archive, design_report_service as service,
    energy_report::TypstConfig, ProjectView,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Subcommand)]
pub enum DesignReportCommand {
    /// Issue matched model-derived figures and canonical results for exact retained A/B runs.
    Create {
        project: PathBuf,
        #[arg(long)]
        a: String,
        #[arg(long)]
        b: String,
        /// Explicit analytical coverage and exclusions; not an invented architectural scope.
        #[arg(long)]
        scope: String,
        /// Optional supported orientation statement; omission discloses unknown true north.
        #[arg(long)]
        north: Option<String>,
        #[arg(long)]
        html_only: bool,
        #[arg(long)]
        typst_dir: Option<PathBuf>,
        #[arg(long)]
        expected_revision: Option<String>,
    },
    /// Inspect the bounded index without claiming product-byte verification.
    List { project: PathBuf },
    /// Verify the complete issued inventory and exact retained simulation bindings.
    Inspect { project: PathBuf, id: String },
    /// Reproduce issued products in a fresh directory with exact deterministic hashes.
    Regenerate {
        project: PathBuf,
        id: String,
        destination: PathBuf,
        #[arg(long)]
        typst_dir: Option<PathBuf>,
    },
    /// Publish complete retained local staging after explicitly reconciling a save conflict.
    Recover {
        project: PathBuf,
        token: String,
        #[arg(long)]
        expected_revision: Option<String>,
    },
    /// Export the complete project, including every retained report and required reference asset.
    Export {
        project: PathBuf,
        destination: PathBuf,
    },
    /// Verify archive paths, bounds, actual bytes and complete report bindings.
    InspectPackage { package: PathBuf },
    /// Explicitly replay this report's exact A/B runs with the separately installed engine.
    Replay {
        package: PathBuf,
        id: String,
        #[arg(long)]
        engine_dir: Option<PathBuf>,
    },
}

fn verified(path: &Path, cancel: Arc<AtomicBool>) -> Result<ProjectView> {
    let admitted = greenlens_runner::project_open::admit_project(path)?;
    greenlens_runner::project_open::verify_admitted_project(path, &admitted, cancel, |progress| {
        if let Ok(value) = serde_json::to_string(&progress) {
            eprintln!("{value}");
        }
    })
}
fn print(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
fn publication(result: Result<service::DesignReportResult>) -> Result<()> {
    match result {
        Ok(result) => print(&serde_json::json!({
            "project_revision":result.view.revision,
            "report":result.report,
            "simulation_execution":"None; this operation used verified retained evidence."
        })),
        Err(error) => {
            if let Some(pending) = error.downcast_ref::<service::PendingDesignReportPublication>() {
                eprintln!(
                    "{}",
                    serde_json::to_string(&serde_json::json!({
                        "status":"completed_products_pending_publication",
                        "recovery_token":pending.recovery_token,
                        "reason":pending.reason,
                        "action":"Inspect and reconcile the current document, then recover this token. Do not render again."
                    }))?
                );
            }
            Err(error)
        }
    }
}
fn revision_fence(expected: Option<&str>, actual: &str) -> Result<()> {
    ensure!(
        expected.is_none_or(|value| value == actual),
        "The project differs from the caller's expected revision; no design report was published"
    );
    Ok(())
}

pub fn execute(command: DesignReportCommand) -> Result<()> {
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))?;
    match command {
        DesignReportCommand::Create {
            project,
            a,
            b,
            scope,
            north,
            html_only,
            typst_dir,
            expected_revision,
        } => {
            let view = verified(&project, cancel.clone())?;
            revision_fence(expected_revision.as_deref(), &view.revision)?;
            let config = (!html_only)
                .then(|| TypstConfig::discover(typst_dir.as_deref()))
                .transpose()?;
            publication(service::create(
                &project,
                &view.project,
                Some(&view.revision),
                &a,
                &b,
                scope,
                north,
                !html_only,
                config.as_ref(),
                cancel,
            ))
        }
        DesignReportCommand::List { project } => {
            let value = greenlens_runner::history_storage::read_project(&project, &cancel)?;
            greenlens_runner::validate_project(&value)?;
            print(&serde_json::json!({"reports":value.design_reports,
                "integrity_scope":"Index/schema/reference admission only. Use inspect to verify actual product bytes and retained simulation evidence."}))
        }
        DesignReportCommand::Inspect { project, id } => {
            let view = verified(&project, cancel.clone())?;
            print(&report::inspect(&project, &view.project, &id, &cancel)?)
        }
        DesignReportCommand::Regenerate {
            project,
            id,
            destination,
            typst_dir,
        } => {
            let view = verified(&project, cancel.clone())?;
            let inspected = report::inspect(&project, &view.project, &id, &cancel)?;
            let config = inspected
                .content
                .include_pdf
                .then(|| TypstConfig::discover(typst_dir.as_deref()))
                .transpose()?;
            let result = report::regenerate(
                &project,
                &view.project,
                &id,
                &destination,
                config.as_ref(),
                &cancel,
            )?;
            print(
                &serde_json::json!({"report_id":id,"destination":destination,
                "content_sha256":result.inspection.content.content_sha256,
                "products_sha256":result.inspection.products.products_sha256,
                "products":result.inspection.products.products,
                "verification":"Every deterministic product matched its issued hash. Original products are unchanged; no EnergyPlus execution.",
                "runtime":result.runtime}),
            )
        }
        DesignReportCommand::Recover {
            project,
            token,
            expected_revision,
        } => {
            let view = verified(&project, cancel.clone())?;
            revision_fence(expected_revision.as_deref(), &view.revision)?;
            publication(service::recover(
                &project,
                &view.project,
                Some(&view.revision),
                &token,
                cancel,
            ))
        }
        DesignReportCommand::Export {
            project,
            destination,
        } => {
            let view = verified(&project, cancel.clone())?;
            print(&archive::export(
                &project,
                &view.project,
                &destination,
                cancel,
            )?)
        }
        DesignReportCommand::InspectPackage { package } => {
            print(&archive::inspect(&package, cancel)?)
        }
        DesignReportCommand::Replay {
            package,
            id,
            engine_dir,
        } => print(&archive::replay(
            &package,
            &id,
            engine_dir.as_deref(),
            cancel,
        )?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    #[derive(Parser)]
    struct Arguments {
        #[command(subcommand)]
        command: DesignReportCommand,
    }
    #[test]
    fn exact_pair_scope_and_replay_target_are_explicit() {
        assert!(Arguments::try_parse_from([
            "test",
            "create",
            "project",
            "--a",
            "a",
            "--b",
            "b",
            "--scope",
            "Four original rectangular zones; analytical-only.",
            "--html-only"
        ])
        .is_ok());
        assert!(
            Arguments::try_parse_from(["test", "create", "project", "--a", "a", "--b", "b"])
                .is_err()
        );
        assert!(
            Arguments::try_parse_from(["test", "create", "project", "--scope", "Example"]).is_err()
        );
        assert!(Arguments::try_parse_from(["test", "replay", "package.zip"]).is_err());
        assert!(Arguments::try_parse_from([
            "test",
            "replay",
            "package.zip",
            "exact-report",
            "--engine-dir",
            "engine"
        ])
        .is_ok());
        assert!(Arguments::try_parse_from([
            "test",
            "inspect-package",
            "package.zip",
            "--shell",
            "anything"
        ])
        .is_err());
    }
    #[test]
    fn recovery_and_html_regeneration_need_no_engine_or_renderer_argument() {
        assert!(Arguments::try_parse_from([
            "test",
            "recover",
            "project",
            "token",
            "--expected-revision",
            "revision"
        ])
        .is_ok());
        assert!(Arguments::try_parse_from([
            "test",
            "regenerate",
            "project",
            "report",
            "fresh-output"
        ])
        .is_ok());
        assert!(Arguments::try_parse_from(["test", "export", "project", "archive.zip"]).is_ok());
        assert!(revision_fence(Some("old"), "current").is_err());
        assert!(revision_fence(Some("current"), "current").is_ok());
        assert!(revision_fence(None, "current").is_ok());
    }
}
