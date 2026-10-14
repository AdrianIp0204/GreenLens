//! Bounded retained-run discovery. No engine, series chunks or result matrices.
use crate::{
    energy::{read_result_metadata, EnergyRunSummary},
    energy_services, Project,
};
use anyhow::{ensure, Context, Result};
use greenlens_core::{canonical_hash, energy::*};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const CATALOG_SCHEMA: &str = "greenlens.energy-run-catalog.v1";
pub const MAX_CATALOG_PAGE: usize = 100;
const INTEGRITY: &str = "Requested run metadata and compiled recipe checked against retained document bindings. Series chunks and unrelated evidence are not read; complete integrity is checked when opening/exporting evidence.";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnergyRunCatalogRequest {
    pub offset: usize,
    pub limit: usize,
    pub alternative: Option<String>,
}
impl EnergyRunCatalogRequest {
    fn validate(&self) -> Result<()> {
        ensure!(
            (1..=MAX_CATALOG_PAGE).contains(&self.limit),
            "Run catalog page size must be 1–100"
        );
        ensure!(
            self.alternative
                .as_deref()
                .is_none_or(|a| matches!(a, "A" | "B")),
            "Choose retained alternative A or B"
        );
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergyRunCatalogRow {
    pub id: String,
    pub alternative_id: String,
    pub name: String,
    pub finished_utc: String,
    /// Exact retained model + study revision, not report-issue metadata.
    pub input_revision_sha256: String,
    pub model_input_sha256: String,
    pub physical_input_sha256: String,
    pub record_sha256: String,
    pub method: EnergyMethodIdentity,
    pub interval_minutes: u8,
    pub current_method: bool,
    pub assessment: EnergyAssessment,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergyRunCatalogPage {
    pub schema_version: String,
    pub project_sha256: String,
    pub offset: usize,
    pub limit: usize,
    pub alternative: Option<String>,
    pub total: usize,
    pub next_offset: Option<usize>,
    pub ordering: String,
    pub integrity_scope: String,
    pub rows: Vec<EnergyRunCatalogRow>,
}

struct Metadata<'a> {
    summary: &'a EnergyRunSummary,
    model: &'a ThermalModel,
    study: &'a EnergyStudy,
    model_input_sha256: &'a str,
    method: &'a EnergyMethodIdentity,
    finished_utc: &'a str,
    assessment: EnergyAssessment,
}
fn map_metadata(project: &Project, input: Metadata<'_>) -> Result<EnergyRunCatalogRow> {
    let finished = chrono::DateTime::parse_from_rfc3339(input.finished_utc)?;
    ensure!(
        finished.offset().local_minus_utc() == 0,
        "Completion time must be UTC"
    );
    ensure!(
        matches!(input.study.zone_timesteps_per_hour, 6 | 12),
        "Unsupported retained timestep"
    );
    let current_method = project
        .energy
        .as_ref()
        .and_then(|e| e.designs.get(&input.summary.alternative_id))
        .and_then(|d| d.study.as_ref())
        .map(greenlens_energyplus::method_identity)
        .transpose()?
        .is_some_and(|m| &m == input.method);
    let mut assessment = input.assessment;
    // The shared assessment supplies currentness and exact-scope qualification;
    // this metadata-only request must never inherit its full-reader claim.
    assessment.evidence_integrity = EnergyIntegrityScope::MetadataChecked;
    Ok(EnergyRunCatalogRow {
        id: input.summary.id.clone(),
        alternative_id: input.summary.alternative_id.clone(),
        name: input.model.name.clone(),
        finished_utc: input.finished_utc.into(),
        input_revision_sha256: canonical_hash(&(input.model, input.study))?,
        model_input_sha256: input.model_input_sha256.into(),
        physical_input_sha256: input.summary.physical_input_hash.clone(),
        record_sha256: input.summary.record_sha256.clone(),
        method: input.method.clone(),
        interval_minutes: 60 / input.study.zone_timesteps_per_hour,
        current_method,
        assessment,
    })
}
fn page_summaries<'a>(
    runs: &'a [EnergyRunSummary],
    request: &EnergyRunCatalogRequest,
) -> Result<(usize, Vec<&'a EnergyRunSummary>)> {
    request.validate()?;
    let eligible = |r: &&EnergyRunSummary| {
        request
            .alternative
            .as_deref()
            .is_none_or(|a| r.alternative_id == a)
    };
    let total = runs.iter().filter(eligible).count();
    ensure!(
        request.offset <= total,
        "Catalog offset exceeds retained run count; refresh the catalog"
    );
    Ok((
        total,
        runs.iter()
            .rev()
            .filter(eligible)
            .skip(request.offset)
            .take(request.limit)
            .collect(),
    ))
}
/// Reverse retention order is deliberate: only one page is read, rather than
/// scanning every record to sort by completion time. Exact UTC is shown per row.
pub fn catalog(
    root: &Path,
    project: &Project,
    request: &EnergyRunCatalogRequest,
) -> Result<EnergyRunCatalogPage> {
    crate::validate_project(project)?;
    let energy = project
        .energy
        .as_ref()
        .context("Energy document is unavailable")?;
    let (total, summaries) = page_summaries(&energy.runs, request)?;
    let mut rows = Vec::with_capacity(summaries.len());
    for summary in summaries {
        let run = read_result_metadata(root, summary)?;
        let assessment = energy_services::assessment(project, &run)?;
        rows.push(map_metadata(
            project,
            Metadata {
                summary,
                model: &run.model,
                study: &run.study,
                model_input_sha256: &run.validated.model_input_sha256,
                method: &run.method,
                finished_utc: &run.provenance.finished_utc,
                assessment,
            },
        )?);
    }
    let end = request
        .offset
        .checked_add(rows.len())
        .context("Catalog offset overflow")?;
    Ok(EnergyRunCatalogPage {
        schema_version: CATALOG_SCHEMA.into(),
        project_sha256: canonical_hash(project)?,
        offset: request.offset,
        limit: request.limit,
        alternative: request.alternative.clone(),
        total,
        next_offset: (end < total).then_some(end),
        ordering: "reverse_retention_order".into(),
        integrity_scope: INTEGRITY.into(),
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn page_is_bounded_filtered_and_never_reads_off_page_metadata() {
        let runs = (0..207)
            .map(|i| EnergyRunSummary {
                id: i.to_string(),
                alternative_id: if i % 2 == 0 { "A" } else { "B" }.into(),
                path: "not accessed by pure pagination".into(),
                physical_input_hash: "a".repeat(64),
                record_sha256: "b".repeat(64),
            })
            .collect::<Vec<_>>();
        let mut q = EnergyRunCatalogRequest {
            offset: 0,
            limit: 100,
            alternative: Some("A".into()),
        };
        let (total, first) = page_summaries(&runs, &q).unwrap();
        assert_eq!(total, 104);
        assert_eq!(first.len(), 100);
        assert_eq!(first[0].id, "206");
        assert_eq!(first[99].id, "8");
        q.offset = 100;
        let (_, last) = page_summaries(&runs, &q).unwrap();
        assert_eq!(
            last.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["6", "4", "2", "0"]
        );
        q.offset = 104;
        assert!(page_summaries(&runs, &q).unwrap().1.is_empty());
        q.offset = usize::MAX;
        assert!(page_summaries(&runs, &q).is_err());
        q.offset = 0;
        q.limit = 0;
        assert!(page_summaries(&runs, &q).is_err());
        q.limit = 101;
        assert!(page_summaries(&runs, &q).is_err());
        q.limit = 1;
        q.alternative = Some("C".into());
        assert!(page_summaries(&runs, &q).is_err());
    }
    #[test]
    fn mapped_metadata_is_private_path_free_and_does_not_promote_integrity() {
        // Pure original synthetic report metadata; no native/reader claim.
        let case = synthetic_energy_report().unwrap().baseline;
        let mut project = energy_services::example_document();
        let design = project
            .energy
            .as_mut()
            .unwrap()
            .designs
            .get_mut("A")
            .unwrap();
        design.model = case.model.clone();
        design.study = Some(case.study.clone());
        let method = greenlens_energyplus::method_identity(&case.study).unwrap();
        let summary = EnergyRunSummary {
            id: "00000000-0000-0000-0000-000000000001".into(),
            alternative_id: "A".into(),
            path: "PRIVATE-PATH-DO-NOT-EXPORT".into(),
            physical_input_hash: case.physical_input_sha256.clone(),
            record_sha256: case.record_sha256.clone(),
        };
        let row = map_metadata(
            &project,
            Metadata {
                summary: &summary,
                model: &case.model,
                study: &case.study,
                model_input_sha256: &case.model_input_sha256,
                method: &method,
                finished_utc: "2026-10-14T01:02:03.1234567Z",
                assessment: case.assessment.clone(),
            },
        )
        .unwrap();
        assert_eq!(row.finished_utc, "2026-10-14T01:02:03.1234567Z");
        assert_eq!(row.interval_minutes, 10);
        assert!(row.current_method);
        assert_eq!(
            row.assessment.evidence_integrity,
            EnergyIntegrityScope::MetadataChecked
        );
        assert_eq!(
            row.assessment.current_physical_inputs,
            case.assessment.current_physical_inputs
        );
        assert_eq!(row.assessment.scientific, case.assessment.scientific);
        assert_eq!(
            row.input_revision_sha256,
            canonical_hash(&(&case.model, &case.study)).unwrap()
        );
        let json = serde_json::to_string(&row).unwrap();
        assert!(!json.contains("PRIVATE-PATH"));
        assert!(!json.contains("series"));
        assert!(!json.contains("processes"));
        let mut changed = method.clone();
        changed.recipe_sha256 = "f".repeat(64);
        let mapped = map_metadata(
            &project,
            Metadata {
                summary: &summary,
                model: &case.model,
                study: &case.study,
                model_input_sha256: &case.model_input_sha256,
                method: &changed,
                finished_utc: "2026-10-14T01:02:03Z",
                assessment: case.assessment.clone(),
            },
        )
        .unwrap();
        assert!(!mapped.current_method);
        assert!(map_metadata(
            &project,
            Metadata {
                summary: &summary,
                model: &case.model,
                study: &case.study,
                model_input_sha256: &case.model_input_sha256,
                method: &method,
                finished_utc: "2026-10-14T09:02:03+08:00",
                assessment: case.assessment
            }
        )
        .is_err());
    }
}
