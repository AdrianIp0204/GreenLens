//! Resource inventory is not evidence-integrity verification. This service
//! measures only manifest-bound members, using actual files, without decoding
//! annual results. Publication/export retain their full integrity checks.
use crate::{persistence::*, Project};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

const REMEDY: &str = "Preserve the complete existing study and start a separately scoped document. No history or required assets will be silently pruned. Estimates are not publication guarantees.";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    Inspect,
    ImportReference,
    EnergyRun { alternative: String },
    ReportIssue,
}
#[derive(Debug, Clone, Serialize)]
pub struct CapacityLimit {
    pub name: String,
    pub used: u64,
    pub limit: u64,
}
#[derive(Debug, Clone, Serialize)]
pub struct CapacityReport {
    pub schema_version: String,
    pub project_content_sha256: String,
    pub retained_bytes: u64,
    pub retained_entries: usize,
    pub available_bytes: u64,
    pub available_entries: usize,
    pub limits: Vec<CapacityLimit>,
    pub incremental_lower_bound_bytes: u64,
    pub incremental_lower_bound_entries: usize,
    pub estimated_incremental_bytes: Option<u64>,
    pub estimate_basis: String,
    pub can_start: bool,
    pub warnings: Vec<String>,
    pub inventory_scope: String,
    pub remedy: String,
}
fn check(cancel: &AtomicBool, start: Instant) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Capacity inspection cancelled; no operation was started"
    );
    ensure!(
        start.elapsed() < Duration::from_secs(30),
        "Capacity inventory exceeded its 30-second budget; no operation was started"
    );
    Ok(())
}
fn prefixes(project: &Project) -> BTreeSet<String> {
    let mut result = BTreeSet::new();
    result.extend(project.runs.iter().map(|r| r.path.clone()));
    if let Some(e) = &project.climate {
        result.extend(e.runs.iter().map(|r| r.path.clone()));
        result.extend(e.weather.iter().map(|r| r.path.clone()));
    }
    if let Some(e) = &project.energy {
        result.extend(e.runs.iter().map(|r| r.path.clone()));
        result.extend(e.weather.iter().map(|r| r.path.clone()));
        result.extend(
            e.attachments
                .iter()
                .map(|r| format!("attachments/{}", r.id)),
        );
    }
    if let Some(e) = &project.architecture {
        result.extend(e.assets.iter().map(|r| r.path.clone()));
    }
    if let Some(e) = &project.report_issues {
        result.extend(e.issues.iter().map(|r| r.path.clone()));
    }
    result
}

pub fn inspect(
    root: Option<&Path>,
    project: &Project,
    operation: &Operation,
    cancel: &AtomicBool,
) -> Result<CapacityReport> {
    inspect_staged(root, project, operation, &[], cancel)
}
pub(crate) fn inspect_staged(
    root: Option<&Path>,
    project: &Project,
    operation: &Operation,
    staged: &[(String, PathBuf)],
    cancel: &AtomicBool,
) -> Result<CapacityReport> {
    crate::validate_project(project)?;
    let start = Instant::now();
    check(cancel, start)?;
    let manifest_bytes = crate::history_storage::manifest_bytes(project)?.len() as u64;
    let (mut retained_bytes, mut entries) = (manifest_bytes, 1usize);
    let mut names = BTreeSet::from(["project.json".to_owned()]);
    let mut largest_entry = manifest_bytes;
    let mut largest_issue = 0u64;
    let mut largest_reference_geometry = 0u64;
    // Pending accepted revisions may not have sidecars yet. Their exact
    // deterministic bytes are accounted before publication; stage verifies any
    // existing member rather than trusting its length or replacing it.
    let history_entries = crate::history_storage::entries(project)?;
    let history_bytes: u64 = history_entries.iter().map(|entry| entry.bytes).sum();
    let mut history_size_mismatch = false;
    for entry in history_entries {
        check(cancel, start)?;
        ensure!(
            names.insert(entry.path.to_ascii_lowercase()),
            "Overlapping history member"
        );
        let actual_bytes = if let Some(root) = root {
            match fs::symlink_metadata(root.join(&entry.path)) {
                Ok(_) => fs::metadata(regular_child(root, &entry.path)?)?.len(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => entry.bytes,
                Err(error) => return Err(error.into()),
            }
        } else {
            entry.bytes
        };
        history_size_mismatch |= actual_bytes != entry.bytes;
        retained_bytes = retained_bytes
            .checked_add(actual_bytes)
            .context("Capacity byte overflow")?;
        entries += 1;
        largest_entry = largest_entry.max(actual_bytes);
    }
    for prefix in prefixes(project) {
        check(cancel, start)?;
        relative_path(&prefix)?;
        let directory = if let Some((_, path)) = staged.iter().find(|(name, _)| *name == prefix) {
            path.clone()
        } else {
            root.context("Save the project before inventorying retained evidence")?
                .join(&prefix)
        };
        let seal: Checksums = serde_json::from_slice(&read_evidence_bytes_checked(
            &regular_child(&directory, "checksums.json")?,
            4 * 1024 * 1024,
            &mut |_| check(cancel, start),
        )?)?;
        ensure!(
            seal.schema_version == "0.1.0"
                && !seal.sha256.is_empty()
                && seal.sha256.len() < MAX_PACKAGE_ENTRIES,
            "Invalid retained inventory for capacity inspection"
        );
        let mut prefix_bytes = 0u64;
        let mut geometry_bytes = 0u64;
        for member in seal
            .sha256
            .keys()
            .map(String::as_str)
            .chain(std::iter::once("checksums.json"))
        {
            check(cancel, start)?;
            let full = format!("{prefix}/{member}");
            relative_path(&full)?;
            ensure!(
                full.len() <= 240 && names.insert(full.to_ascii_lowercase()),
                "Unsafe or overlapping retained capacity member"
            );
            ensure!(names.len()<=MAX_PACKAGE_ENTRIES+1,"Retained member count exceeds supported capacity; preserve this document before adding evidence");
            let size = fs::metadata(regular_child(&directory, member)?)?.len();
            retained_bytes = retained_bytes
                .checked_add(size)
                .context("Capacity byte overflow")?;
            largest_entry = largest_entry.max(size);
            entries += 1;
            prefix_bytes += size;
            if member.starts_with("geometry/") {
                geometry_bytes += size;
            }
        }
        if prefix.starts_with("references/") {
            largest_reference_geometry = largest_reference_geometry.max(geometry_bytes);
        }
        if prefix.starts_with("report-issues/") {
            largest_issue = largest_issue.max(prefix_bytes);
        }
    }
    if let Some(w) = project.energy.as_ref().and_then(|e| e.workbench.as_ref()) {
        for session in &w.sensitivity {
            for name in std::iter::once(format!("{}/plan.json", session.path)).chain(
                session
                    .events
                    .iter()
                    .map(|e| format!("{}/event-{:03}.json", session.path, e.sequence)),
            ) {
                check(cancel, start)?;
                ensure!(
                    names.insert(name.to_ascii_lowercase()),
                    "Overlapping sensitivity capacity member"
                );
                let size = fs::metadata(regular_child(
                    root.context("Retained sensitivity requires a saved project")?,
                    &name,
                )?)?
                .len();
                retained_bytes = retained_bytes
                    .checked_add(size)
                    .context("Capacity byte overflow")?;
                largest_entry = largest_entry.max(size);
                entries += 1;
            }
        }
    }
    let revisions = project
        .architecture
        .as_ref()
        .and_then(|a| a.scope_history.as_ref())
        .map_or(0, |h| h.revisions.len());
    let issues = project.report_issues.as_ref().map_or(0, |i| i.issues.len());
    let references = project.architecture.as_ref().map_or(0, |a| a.assets.len());
    let mut limits = vec![];
    for (name, used, limit) in [
        ("complete retained bytes", retained_bytes, MAX_PACKAGE_BYTES),
        (
            "retained entries",
            entries as u64,
            MAX_PACKAGE_ENTRIES as u64,
        ),
        (
            "largest retained entry",
            largest_entry,
            MAX_EVIDENCE_FILE_BYTES,
        ),
        (
            "candidate manifest bytes",
            manifest_bytes,
            if project
                .architecture
                .as_ref()
                .and_then(|a| a.scope_history.as_ref())
                .is_some()
            {
                crate::study_revisions::MAX_REVISION_PROJECT_BYTES as u64
            } else {
                MAX_EVIDENCE_FILE_BYTES
            },
        ),
        (
            "scope revision count",
            revisions as u64,
            greenlens_core::study_revision::MAX_SCOPE_REVISIONS as u64,
        ),
        (
            "issue count",
            issues as u64,
            crate::report_issues::MAX_ISSUES as u64,
        ),
        ("reference revision count", references as u64, 8),
        (
            "external scope history bytes",
            history_bytes,
            crate::history_storage::MAX_HISTORY_BYTES,
        ),
        (
            "largest retained reference geometry bytes",
            largest_reference_geometry,
            greenlens_core::reference::MAX_NORMALIZED_BYTES as u64,
        ),
    ] {
        limits.push(CapacityLimit {
            name: name.into(),
            used,
            limit,
        });
    }
    let mut report=CapacityReport{schema_version:"greenlens.project-capacity.v1".into(),project_content_sha256:greenlens_core::canonical_hash(project)?,retained_bytes,retained_entries:entries,available_bytes:MAX_PACKAGE_BYTES.saturating_sub(retained_bytes),available_entries:MAX_PACKAGE_ENTRIES.saturating_sub(entries),limits,incremental_lower_bound_bytes:0,incremental_lower_bound_entries:0,estimated_incremental_bytes:None,estimate_basis:"No output-size guarantee. Final report files, logs, receipts and archive envelope are measured at publication/export.".into(),can_start:true,warnings:vec![],inventory_scope:"Actual sizes of manifest-bound regular files plus the serialized candidate manifest. This is a resource inventory, not content-integrity verification. Unreferenced scratch and external proprietary sources are excluded; required history is included. ZIP envelope overhead is not yet included.".into(),remedy:REMEDY.into()};
    match operation {
        Operation::EnergyRun { alternative } => {
            let design = project
                .energy
                .as_ref()
                .and_then(|e| e.designs.get(alternative))
                .context("Capacity forecast needs an energy design")?;
            let study = design
                .study
                .as_ref()
                .context("Configure weather and study before forecasting a run")?;
            let intervals = 8760u64 * u64::from(study.zone_timesteps_per_hour);
            report.incremental_lower_bound_bytes =
                intervals * greenlens_energyplus::channels(&design.model).len() as u64 * 8;
            report.incremental_lower_bound_entries = 53;
            report.estimate_basis="Known lower bound: complete native float64 time/channel series and 53 weekly chunks. Weather copies, input files, logs, manifests and receipts add bytes; final total is unknown.".into();
        }
        Operation::ImportReference => {
            report.can_start = references < 8;
            report.incremental_lower_bound_entries = 2;
            report.estimate_basis="Normalized geometry size is unknown until bounded admission. Existing limits: 1,000,000 expanded triangles, 80 MiB normalized geometry, 16 MiB reference manifest. Source file size is not normalized output size.".into();
        }
        Operation::ReportIssue => {
            report.can_start = issues < crate::report_issues::MAX_ISSUES;
            report.incremental_lower_bound_entries = 4;
            report.estimated_incremental_bytes = (largest_issue > 0).then_some(largest_issue);
            report.estimate_basis="Estimate only: largest previously retained issue, when available. New scope, labels, figures or renderer can change size. Exact products and candidate manifest are admitted after rendering; failed publication preserves completed pending products.".into();
        }
        Operation::Inspect => {}
    }
    if report.limits.iter().any(|l| l.used > l.limit)
        || report.available_bytes < report.incremental_lower_bound_bytes
        || report.available_entries < report.incremental_lower_bound_entries
    {
        report.can_start = false;
        report
            .warnings
            .push("A known retained or incremental lower bound exceeds capacity.".into());
    }
    if history_size_mismatch {
        report.can_start = false;
        report.warnings.push("An existing immutable history file differs in size from its bound record. Restore the exact original member before continuing; it will not be overwritten.".into());
    }
    if report.available_bytes < 16 * 1024 * 1024 {
        report.warnings.push("Less than 16 MiB remains before archive overhead. A new run, reference or issue may not fit.".into());
    }
    if report
        .estimated_incremental_bytes
        .is_some_and(|n| n > report.available_bytes)
    {
        report.warnings.push("The previous-issue size estimate exceeds remaining capacity; rendering is not a publication guarantee.".into());
    }
    check(cancel, start)?;
    Ok(report)
}
pub fn require_start(
    root: &Path,
    project: &Project,
    operation: &Operation,
    cancel: &AtomicBool,
) -> Result<CapacityReport> {
    let report = inspect(Some(root), project, operation, cancel)?;
    ensure!(report.can_start,"Capacity preflight: {} retained bytes / {} entries; known addition at least {} bytes / {} entries. {}",report.retained_bytes,report.retained_entries,report.incremental_lower_bound_bytes,report.incremental_lower_bound_entries,REMEDY);
    Ok(report)
}
pub(crate) fn require_publication(
    root: &Path,
    project: &Project,
    staged: &[(String, PathBuf)],
    cancel: &AtomicBool,
) -> Result<()> {
    let report = inspect_staged(Some(root), project, &Operation::Inspect, staged, cancel)?;
    ensure!(
        report.can_start,
        "Complete project exceeds shared capacity. {}",
        REMEDY
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsaved_inventory_is_not_integrity_and_run_lower_bound_matches_shape() {
        let project = crate::energy_services::example_document();
        let cancel = AtomicBool::new(false);
        let r = inspect(None, &project, &Operation::Inspect, &cancel).unwrap();
        assert_eq!(r.retained_entries, 1);
        assert_eq!(
            r.retained_bytes,
            serde_json::to_vec_pretty(&project).unwrap().len() as u64 + 1
        );
        assert!(r.can_start);
        assert!(r.inventory_scope.contains("not content-integrity"));
        assert!(inspect(None, &project, &Operation::Inspect, &AtomicBool::new(true)).is_err());
    }
    #[test]
    fn boundary_forecasts_do_not_claim_unknown_sizes_fit() {
        let project = crate::example();
        let cancel = AtomicBool::new(false);
        let r = inspect(None, &project, &Operation::ImportReference, &cancel).unwrap();
        assert_eq!(r.incremental_lower_bound_bytes, 0);
        assert_eq!(r.estimated_incremental_bytes, None);
        assert!(r.estimate_basis.contains("unknown"));
        let r = inspect(None, &project, &Operation::ReportIssue, &cancel).unwrap();
        assert_eq!(r.estimated_incremental_bytes, None);
        assert!(r.estimate_basis.contains("Estimate only"));
    }
}

#[cfg(test)]
#[path = "capacity_tests.rs"]
mod capacity_tests;
