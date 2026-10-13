//! Supplementary exact-method assessments. Never changes sealed run bytes.
use anyhow::{ensure, Result};
use greenlens_core::energy::*;

const REGISTRY: &str = include_str!("../../../dependencies/energy-qualification.json");
pub fn assess(method: &EnergyMethodIdentity, physical: &str) -> Result<EnergyQualification> {
    method.validate()?;
    let entries: Vec<EnergyQualification> = serde_json::from_str(REGISTRY)?;
    let matches: Vec<_> = entries
        .into_iter()
        .filter(|q| {
            &q.method == method
                && q.applicable_physical_input_sha256
                    .iter()
                    .any(|h| h == physical)
        })
        .collect();
    ensure!(
        matches.len() <= 1,
        "Conflicting energy qualification registry entries"
    );
    Ok(matches.into_iter().next().unwrap_or_else(|| EnergyQualification {
        status: EnergyQualificationStatus::Unassessed,
        method: method.clone(),
        registry_version: "greenlens.energy-qualification.2026-10-13.2".into(),
        reason: "This exact method, numerical profile and physical input scope has no complete passing assessment. The initial synthetic A/B scope failed its proposed-case timestep peak criterion. Only explicitly listed revised inputs have limited qualification; results from another scope do not establish qualification here.".into(),
        tested_scope: "No general building-accuracy, equipment-calibration or certification claim.".into(),
        applicable_physical_input_sha256: vec![], evidence_ids: vec![],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_exact_scope_is_failed_and_unknown_combinations_are_unassessed() {
        let before = REGISTRY.as_bytes().to_vec();
        let entries: Vec<EnergyQualification> = serde_json::from_str(REGISTRY).unwrap();
        let failed: Vec<_> = entries
            .into_iter()
            .filter(|e| e.status == EnergyQualificationStatus::Failed)
            .collect();
        assert_eq!(failed.len(), 4);
        for entry in failed {
            let physical = &entry.applicable_physical_input_sha256[0];
            let found = assess(&entry.method, physical).unwrap();
            assert_eq!(found, entry);
            assert_eq!(found.status, EnergyQualificationStatus::Failed);
            assert!(found.reason.contains("3.378158637783503%"));
            assert!(!found.evidence_ids.is_empty());
            let check = EnergyAssessment {
                execution: EnergyExecutionStatus::Completed,
                evidence_integrity: EnergyIntegrityScope::CompleteRetainedEvidenceChecked,
                current_physical_inputs: EnergyInputRelation::Current,
                current_processed_results: EnergyInputRelation::Current,
                current_derived_results: EnergyInputRelation::NotCompared,
                scientific: found,
            };
            check.validate(physical, &entry.method).unwrap();
            assert_eq!(
                assess(&entry.method, &"f".repeat(64)).unwrap().status,
                EnergyQualificationStatus::Unassessed
            );
            let mut future = entry.method.clone();
            future.recipe_sha256 = "a".repeat(64);
            assert_eq!(
                assess(&future, physical).unwrap().status,
                EnergyQualificationStatus::Unassessed
            );
            future = entry.method.clone();
            future.engine_executable_sha256 = "b".repeat(64);
            assert_eq!(
                assess(&future, physical).unwrap().status,
                EnergyQualificationStatus::Unassessed
            );
        }
        assert_eq!(REGISTRY.as_bytes(), before);
    }
    #[test]
    fn revision_two_qualification_requires_exact_method_and_physical_scope() {
        let entries: Vec<EnergyQualification> = serde_json::from_str(REGISTRY).unwrap();
        let limited: Vec<_> = entries
            .iter()
            .filter(|e| e.status == EnergyQualificationStatus::LimitedQualified)
            .collect();
        assert_eq!(limited.len(), 4);
        for entry in limited {
            assert_eq!(entry.applicable_physical_input_sha256.len(), 1);
            let physical = &entry.applicable_physical_input_sha256[0];
            assert_eq!(&assess(&entry.method, physical).unwrap(), entry);
            assert!(entry
                .tested_scope
                .contains("not installed-release acceptance"));
            assert!(entry.evidence_ids.len() >= 3);
            for other in &entries {
                if other.method == entry.method
                    && other.applicable_physical_input_sha256
                        == entry.applicable_physical_input_sha256
                {
                    continue;
                }
                assert_ne!(
                    other.applicable_physical_input_sha256,
                    entry.applicable_physical_input_sha256
                );
            }
            let mut altered = entry.method.clone();
            altered.numerical_settings_sha256 = "f".repeat(64);
            assert_eq!(
                assess(&altered, physical).unwrap().status,
                EnergyQualificationStatus::Unassessed
            );
            altered = entry.method.clone();
            altered.engine_executable_sha256 = "f".repeat(64);
            assert_eq!(
                assess(&altered, physical).unwrap().status,
                EnergyQualificationStatus::Unassessed
            );
            assert_eq!(
                assess(&entry.method, &"f".repeat(64)).unwrap().status,
                EnergyQualificationStatus::Unassessed
            );
        }
    }
}
