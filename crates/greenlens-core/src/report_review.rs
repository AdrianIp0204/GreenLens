//! Derived report documentation and review status. Neither dimension establishes
//! execution, evidence integrity, scientific qualification or professional approval.
use crate::study_revision::{
    validate_revision, ProvenanceChecklist, ReviewDecision, ReviewResolution, RevisionRowKind,
    ScopeRevision,
};
use crate::{canonical_hash, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const REPORT_REVIEW_SCHEMA: &str = "greenlens.report-review-readiness.v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    InitialNoCarryForward,
    NoRecordedDecisions,
    AllRecordedReviewed,
    UnresolvedRetained,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReportReviewRow {
    pub row_id: String,
    /// None means the historical revision did not retain this metadata. Never
    /// infer a category or object from an opaque row digest or display name.
    pub kind: Option<RevisionRowKind>,
    pub key: Option<String>,
    pub resolution: ReviewResolution,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReportReviewReadiness {
    pub schema_version: String,
    pub assessment_sha256: String,
    pub scope_revision_sha256: String,
    pub documentation_complete: bool,
    pub supported_input_rows: usize,
    pub unreviewed_input_rows: usize,
    pub missing_input_rows: usize,
    pub conflicting_input_rows: usize,
    pub review_state: ReviewState,
    pub decision_count: usize,
    pub reviewed_count: usize,
    pub unresolved_count: usize,
    pub category_metadata_unavailable_count: usize,
    pub unresolved_row_ids: Vec<String>,
    pub rows: Vec<ReportReviewRow>,
    pub disclosure: String,
}

impl ReportReviewReadiness {
    pub fn identity(&self) -> Result<String> {
        let mut unsigned = self.clone();
        unsigned.assessment_sha256.clear();
        canonical_hash(&(REPORT_REVIEW_SCHEMA, unsigned))
    }
}

/// Recompute against the authoritative immutable revision. Counting covers all
/// accepted decisions, including Input and Reference decisions that do not make
/// an otherwise complete input-provenance checklist incomplete.
pub fn review_readiness(revision: &ScopeRevision) -> Result<ReportReviewReadiness> {
    validate_revision(revision)?;
    let metadata = revision
        .reviewed_rows
        .as_ref()
        .map(|rows| {
            rows.iter()
                .map(|row| (row.row_id.as_str(), (row.kind, row.key.as_str())))
                .collect()
        })
        .unwrap_or_default();
    summarize(
        &revision.revision_sha256,
        revision.parent_revision_sha256.is_some(),
        &revision.provenance,
        &revision.review_decisions,
        &metadata,
    )
}

fn summarize(
    revision_sha256: &str,
    has_parent: bool,
    provenance: &ProvenanceChecklist,
    decisions: &[ReviewDecision],
    metadata: &BTreeMap<&str, (RevisionRowKind, &str)>,
) -> Result<ReportReviewReadiness> {
    let rows = decisions
        .iter()
        .map(|decision| {
            let context = metadata.get(decision.row_id.as_str());
            ReportReviewRow {
                row_id: decision.row_id.clone(),
                kind: context.map(|v| v.0),
                key: context.map(|v| v.1.to_owned()),
                resolution: decision.resolution,
                rationale: decision.rationale.clone(),
            }
        })
        .collect::<Vec<_>>();
    let unresolved_row_ids = rows
        .iter()
        .filter(|row| row.resolution == ReviewResolution::Unresolved)
        .map(|row| row.row_id.clone())
        .collect::<Vec<_>>();
    let unresolved_count = unresolved_row_ids.len();
    let review_state = if unresolved_count > 0 {
        ReviewState::UnresolvedRetained
    } else if !has_parent {
        ReviewState::InitialNoCarryForward
    } else if rows.is_empty() {
        ReviewState::NoRecordedDecisions
    } else {
        ReviewState::AllRecordedReviewed
    };
    let mut result = ReportReviewReadiness {
        schema_version: REPORT_REVIEW_SCHEMA.into(),
        assessment_sha256: String::new(),
        scope_revision_sha256: revision_sha256.into(),
        documentation_complete: provenance.documentation_complete,
        supported_input_rows: provenance.rows.len(),
        unreviewed_input_rows: provenance.unreviewed_count,
        missing_input_rows: provenance.missing_count,
        conflicting_input_rows: provenance.conflicting_count,
        review_state,
        decision_count: rows.len(),
        reviewed_count: rows.len() - unresolved_count,
        unresolved_count,
        category_metadata_unavailable_count: rows.iter().filter(|row| row.kind.is_none()).count(),
        unresolved_row_ids,
        rows,
        disclosure: "Input documentation and carry-forward review resolution are separate. A complete input checklist does not resolve retained Input, Reference or other review decisions. Reviewed records establish only the recorded review, not independent approval, source truth, execution, integrity or scientific qualification. Missing historical category metadata is unavailable, never inferred.".into(),
    };
    result.assessment_sha256 = result.identity()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_provenance() -> ProvenanceChecklist {
        ProvenanceChecklist {
            schema_version: "test-only".into(),
            rows: vec![],
            unreviewed_count: 0,
            missing_count: 0,
            conflicting_count: 0,
            documentation_complete: true,
            disclosure: "Arithmetic-only fixture, not accepted run evidence".into(),
        }
    }

    #[test]
    fn complete_documentation_does_not_resolve_input_or_reference_decisions() {
        let decisions = [
            ReviewDecision {
                row_id: "input".into(),
                resolution: ReviewResolution::Unresolved,
                rationale: "Await measured input".into(),
            },
            ReviewDecision {
                row_id: "reference".into(),
                resolution: ReviewResolution::Unresolved,
                rationale: "Source remains unresolved".into(),
            },
            ReviewDecision {
                row_id: "mapping".into(),
                resolution: ReviewResolution::Reviewed,
                rationale: "Explicit reviewed mapping".into(),
            },
        ];
        let metadata = BTreeMap::from([
            ("input", (RevisionRowKind::Input, "B/COP")),
            ("reference", (RevisionRowKind::Reference, "source")),
            ("mapping", (RevisionRowKind::Mapping, "occurrence")),
        ]);
        let status =
            summarize("scope", true, &complete_provenance(), &decisions, &metadata).unwrap();
        assert!(status.documentation_complete);
        assert_eq!(status.review_state, ReviewState::UnresolvedRetained);
        assert_eq!(
            (
                status.decision_count,
                status.reviewed_count,
                status.unresolved_count
            ),
            (3, 1, 2)
        );
        assert_eq!(status.rows[0].kind, Some(RevisionRowKind::Input));
        assert_eq!(status.rows[1].kind, Some(RevisionRowKind::Reference));
        assert_eq!(status.assessment_sha256, status.identity().unwrap());
    }

    #[test]
    fn all_rows_and_multibyte_rationales_survive_and_change_identity() {
        let mut decisions = (0..101)
            .map(|i| ReviewDecision {
                row_id: format!("row-{i:03}"),
                resolution: if i == 100 {
                    ReviewResolution::Unresolved
                } else {
                    ReviewResolution::Reviewed
                },
                rationale: "測量來源尚待核實；原始說明。".repeat(30),
            })
            .collect::<Vec<_>>();
        let first = summarize(
            "scope",
            true,
            &complete_provenance(),
            &decisions,
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(first.rows.len(), 101);
        assert_eq!(first.unresolved_row_ids, ["row-100"]);
        assert_eq!(first.category_metadata_unavailable_count, 101);
        assert_eq!(first.rows[100].rationale, decisions[100].rationale);
        decisions[100].rationale.push('改');
        let second = summarize(
            "scope",
            true,
            &complete_provenance(),
            &decisions,
            &BTreeMap::new(),
        )
        .unwrap();
        assert_ne!(first.assessment_sha256, second.assessment_sha256);
    }

    #[test]
    fn zero_decisions_does_not_invent_a_review() {
        let initial = summarize(
            "scope",
            false,
            &complete_provenance(),
            &[],
            &BTreeMap::new(),
        )
        .unwrap();
        let child =
            summarize("scope", true, &complete_provenance(), &[], &BTreeMap::new()).unwrap();
        assert_eq!(initial.review_state, ReviewState::InitialNoCarryForward);
        assert_eq!(child.review_state, ReviewState::NoRecordedDecisions);
    }
}
