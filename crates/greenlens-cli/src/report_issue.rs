//! CLI access to the same immutable issue services used by the desktop.
use anyhow::{ensure, Result};
use clap::Subcommand;
use greenlens_runner::{
    energy_report::TypstConfig, report_issues as service, Project, ProjectView,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Subcommand)]
pub enum ReportIssueCommand {
    /// Issue selected retained evidence and an accepted scope; never runs EnergyPlus.
    Create {
        project: PathBuf,
        #[arg(long)]
        a: String,
        #[arg(long)]
        b: String,
        #[arg(long)]
        scope: String,
        #[arg(long)]
        html_only: bool,
        #[arg(long)]
        typst_dir: Option<PathBuf>,
        /// Optional caller revision fence; by default use the freshly verified manifest.
        #[arg(long)]
        expected_revision: Option<String>,
    },
    /// List admitted index metadata; does not claim product verification.
    List { project: PathBuf },
    /// Verify actual issued products and retained case bindings, then inspect frozen content.
    Inspect { project: PathBuf, id: String },
    /// Regenerate exact supported products to a new directory; preserve original bytes.
    Regenerate {
        project: PathBuf,
        id: String,
        destination: PathBuf,
        #[arg(long)]
        typst_dir: Option<PathBuf>,
    },
    /// Explicitly reconcile a completed local staging token after an external-save conflict.
    Recover {
        project: PathBuf,
        token: String,
        #[arg(long)]
        expected_revision: Option<String>,
    },
    /// Export the complete retained project/issue dependency closure within archive limits.
    Export {
        project: PathBuf,
        destination: PathBuf,
    },
    /// Validate paths, bounds, actual bytes and immutable bindings in a portable issue ZIP.
    InspectPackage { package: PathBuf },
    /// Replay the selected issue's A/B cases through the recognized native engine adapter.
    Replay {
        package: PathBuf,
        id: String,
        #[arg(long)]
        engine_dir: Option<PathBuf>,
    },
}

fn cancellation() -> Result<Arc<AtomicBool>> {
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))?;
    Ok(cancel)
}
fn verified(path: &Path, cancel: Arc<AtomicBool>) -> Result<ProjectView> {
    let admitted = greenlens_runner::project_open::admit_project(path)?;
    greenlens_runner::project_open::verify_admitted_project(path, &admitted, cancel, |p| {
        if let Ok(value) = serde_json::to_string(&p) {
            eprintln!("{value}");
        }
    })
}
fn print(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
pub fn execute(command: ReportIssueCommand) -> Result<()> {
    let cancel = cancellation()?;
    match command {
        ReportIssueCommand::Create {
            project,
            a,
            b,
            scope,
            html_only,
            typst_dir,
            expected_revision,
        } => {
            let view = verified(&project, cancel.clone())?;
            ensure!(
                expected_revision
                    .as_ref()
                    .is_none_or(|r| r == &view.revision),
                "The project differs from the caller's expected revision; no issue was generated"
            );
            let config = (!html_only)
                .then(|| TypstConfig::discover(typst_dir.as_deref()))
                .transpose()?;
            let result = service::issue(
                &project,
                &view.project,
                Some(&view.revision),
                &a,
                &b,
                &scope,
                !html_only,
                config.as_ref(),
                cancel,
            )?;
            print(
                &serde_json::json!({"project_revision":result.view.revision,"issue":result.issue}),
            )
        }
        ReportIssueCommand::List { project } => {
            let value: Project = greenlens_runner::persistence::read_json(
                &greenlens_runner::persistence::regular_child(&project, "project.json")?,
            )?;
            greenlens_runner::validate_project(&value)?;
            print(
                &serde_json::json!({"issues":value.report_issues,"integrity_scope":"Index/schema/reference admission only. Use inspect to verify actual issued product bytes and retained case evidence."}),
            )
        }
        ReportIssueCommand::Inspect { project, id } => {
            let view = verified(&project, cancel.clone())?;
            print(&service::inspect(&project, &view.project, &id, cancel)?)
        }
        ReportIssueCommand::Regenerate {
            project,
            id,
            destination,
            typst_dir,
        } => {
            let view = verified(&project, cancel.clone())?;
            let inspected = service::inspect(&project, &view.project, &id, cancel.clone())?;
            let config = inspected
                .content
                .include_pdf
                .then(|| TypstConfig::discover(typst_dir.as_deref()))
                .transpose()?;
            let result = service::regenerate(
                &project,
                &view.project,
                &id,
                &destination,
                config.as_ref(),
                cancel,
            )?;
            print(
                &serde_json::json!({"issue_id":id,"destination":destination,"products":result.artifacts,"identity":result.dataset_sha256,"verification":"Every regenerated deterministic product matched its issued hash; no EnergyPlus execution."}),
            )
        }
        ReportIssueCommand::Recover {
            project,
            token,
            expected_revision,
        } => {
            let view = verified(&project, cancel.clone())?;
            ensure!(
                expected_revision
                    .as_ref()
                    .is_none_or(|r| r == &view.revision),
                "The project differs from the expected reconciled revision"
            );
            let result = service::publish_staged_issue(
                &project,
                &view.project,
                Some(&view.revision),
                &token,
                cancel,
            )?;
            print(
                &serde_json::json!({"project_revision":result.view.revision,"issue":result.issue}),
            )
        }
        ReportIssueCommand::Export {
            project,
            destination,
        } => {
            let view = verified(&project, cancel.clone())?;
            print(&greenlens_runner::report_issue_export::export(
                &project,
                &view.project,
                &destination,
                cancel,
            )?)
        }
        ReportIssueCommand::InspectPackage { package } => print(
            &greenlens_runner::report_issue_export::inspect(&package, cancel)?,
        ),
        ReportIssueCommand::Replay {
            package,
            id,
            engine_dir,
        } => print(&greenlens_runner::report_issue_export::replay(
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
        command: ReportIssueCommand,
    }
    #[test]
    fn bounded_issue_commands_require_explicit_pair_scope_and_replay_target() {
        assert!(Arguments::try_parse_from([
            "test",
            "create",
            "project",
            "--a",
            "a",
            "--b",
            "b",
            "--scope",
            "scope",
            "--html-only"
        ])
        .is_ok());
        assert!(
            Arguments::try_parse_from(["test", "create", "project", "--a", "a", "--b", "b"])
                .is_err()
        );
        assert!(Arguments::try_parse_from(["test", "recover", "project", "token"]).is_ok());
        assert!(Arguments::try_parse_from(["test", "replay", "package.zip"]).is_err());
        assert!(Arguments::try_parse_from([
            "test",
            "replay",
            "package.zip",
            "issue",
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
}
