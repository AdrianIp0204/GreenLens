//! Bounded proposal and drawing access through the shared application service.
//! Input JSON is inert data; this command never starts a simulation or renderer.
use anyhow::{ensure, Context, Result};
use clap::Subcommand;
use greenlens_runner::{design_proposals as service, ProjectView};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

#[derive(Subcommand)]
pub enum DesignCommand {
    /// Transparent design hypotheses and tradeoffs; no automatic changes.
    Catalogue,
    /// Preview one explicit B-only proposal. Includes exact review and disk revision fences.
    Preview { project: PathBuf, request: PathBuf },
    /// Apply the exact reviewed proposal as one accepted transaction and save atomically.
    Apply {
        project: PathBuf,
        request: PathBuf,
        #[arg(long)]
        reviewed_context: String,
        #[arg(long)]
        expected_revision: String,
    },
    /// Print deterministic matched SVGs and complete change data. Without --a/--b,
    /// current accepted geometry is explicitly Not simulated. No report is issued.
    Figures {
        project: PathBuf,
        #[arg(long, requires = "b")]
        a: Option<String>,
        #[arg(long, requires = "a")]
        b: Option<String>,
    },
}

fn print(value: &impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
fn draft(path: &Path) -> Result<service::ProposalDraft> {
    let file = File::open(path).context("Cannot open the explicit proposal JSON")?;
    ensure!(
        file.metadata()?.is_file(),
        "Proposal request must be a regular file"
    );
    let mut bytes = Vec::new();
    file.take(128 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 128 * 1024,
        "Proposal request exceeds 128 KiB"
    );
    serde_json::from_slice(&bytes).context("Invalid bounded proposal JSON")
}
fn verified(path: &Path, cancel: Arc<AtomicBool>) -> Result<ProjectView> {
    let admitted = greenlens_runner::project_open::admit_project(path)?;
    greenlens_runner::project_open::verify_admitted_project(path, &admitted, cancel, |progress| {
        if let Ok(json) = serde_json::to_string(&progress) {
            eprintln!("{json}");
        }
    })
}
pub fn execute(command: DesignCommand) -> Result<()> {
    if matches!(command, DesignCommand::Catalogue) {
        return print(&greenlens_core::energy::design_hypotheses());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))?;
    match command {
        DesignCommand::Catalogue => unreachable!(),
        DesignCommand::Preview { project, request } => {
            let request = draft(&request)?;
            let view = verified(&project, cancel)?;
            let review = service::preview(&view.project, &request)?;
            print(
                &serde_json::json!({"schema_version":"greenlens.cli-design-review.v1", "project_id":view.project.id, "expected_revision":view.revision, "review":review}),
            )
        }
        DesignCommand::Apply {
            project,
            request,
            reviewed_context,
            expected_revision,
        } => {
            let request = draft(&request)?;
            let view = verified(&project, cancel.clone())?;
            ensure!(
                view.revision == expected_revision,
                "Saved document changed after preview; refresh and review instead of overwriting"
            );
            ensure!(
                !cancel.load(Ordering::Relaxed),
                "Proposal apply cancelled before acceptance"
            );
            let next = service::commit(&view.project, &request, &reviewed_context)?;
            ensure!(
                !cancel.load(Ordering::Relaxed),
                "Proposal apply cancelled before publication"
            );
            let saved = greenlens_runner::save_project(&project, &next, Some(&expected_revision))?;
            print(
                &serde_json::json!({"schema_version":"greenlens.cli-design-applied.v1", "project_id":saved.project.id, "revision":saved.revision, "target":"B", "simulation_started":false}),
            )
        }
        DesignCommand::Figures { project, a, b } => {
            let view = verified(&project, cancel.clone())?;
            ensure!(
                !cancel.load(Ordering::Relaxed),
                "Drawing inspection cancelled"
            );
            let figures = match (a, b) {
                (Some(a), Some(b)) => service::retained_figures(&project, &view.project, &a, &b)?,
                (None, None) => service::current_figures(&view.project)?,
                _ => anyhow::bail!("Choose both exact A/B run identities or neither"),
            };
            ensure!(
                !cancel.load(Ordering::Relaxed),
                "Drawing inspection cancelled before output"
            );
            print(&figures)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        command: DesignCommand,
    }
    #[test]
    fn apply_requires_both_review_fences_and_figures_require_exact_pair() {
        assert!(TestCli::try_parse_from(["design", "apply", "project", "request.json"]).is_err());
        assert!(TestCli::try_parse_from([
            "design",
            "apply",
            "project",
            "request.json",
            "--reviewed-context",
            "context",
            "--expected-revision",
            "revision"
        ])
        .is_ok());
        assert!(TestCli::try_parse_from(["design", "figures", "project", "--a", "a"]).is_err());
        assert!(
            TestCli::try_parse_from(["design", "figures", "project", "--a", "a", "--b", "b"])
                .is_ok()
        );
        assert!(TestCli::try_parse_from(["design", "figures", "project"]).is_ok());
    }
}
