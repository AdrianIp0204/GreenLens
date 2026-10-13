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
        registry_version: "greenlens.energy-qualification.2026-10-13.3".into(),
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
            .filter(|e| {
                e.status == EnergyQualificationStatus::LimitedQualified
                    && e.registry_version == "greenlens.energy-qualification.2026-10-13.2"
            })
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

    #[test]
    fn revision_three_adds_only_two_exact_proposed_scopes_and_preserves_history() {
        let entries: Vec<EnergyQualification> = serde_json::from_str(REGISTRY).unwrap();
        assert_eq!(entries.len(), 10);
        assert_eq!(
            greenlens_core::canonical_hash(&entries[..8].to_vec()).unwrap(),
            "7baaa42eba90a2aa1d9b25cf4ada7b95bdd0847276c2b581546a1841c044bf81"
        );
        let mut keys = std::collections::BTreeSet::new();
        for entry in &entries {
            assert_eq!(entry.applicable_physical_input_sha256.len(), 1);
            for physical in &entry.applicable_physical_input_sha256 {
                assert!(keys.insert((
                    serde_json::to_string(&entry.method).unwrap(),
                    physical.clone()
                )));
                assert_eq!(assess(&entry.method, physical).unwrap(), *entry);
            }
        }
        let expected = [
            (
                "9a7a6c44d973a333485f5aa33c1ecafc938965e0fdcd5183e87eb9bf1526f45a",
                "58a8dbf3bcf52df9c626b3e3ec608ebdbb5b824f125c0f11dced662c2ba2ec82",
            ),
            (
                "72e816ec0092166fef130842156aeb0e283a16870175f7d3c3ef44f8ed75685b",
                "90227a6bf39574b0f1872a18361bae811780a3804530fd11c8c5f857f138cd63",
            ),
        ];
        for (entry, (physical, numerics)) in entries[8..].iter().zip(expected) {
            assert_eq!(entry.status, EnergyQualificationStatus::LimitedQualified);
            assert_eq!(
                entry.registry_version,
                "greenlens.energy-qualification.2026-10-13.3"
            );
            assert_eq!(
                entry.applicable_physical_input_sha256,
                vec![physical.to_string()]
            );
            assert_eq!(entry.method.numerical_settings_sha256, numerics);
            assert!(entry
                .tested_scope
                .contains("not installed-release acceptance"));
            assert!(entry.evidence_ids.iter().any(
                |e| e.ends_with("7322e79691b19260d2215f421d63fdec2729625393b9fcba61ef480d75f7ccf1")
            ));
            for field in [
                "recipe_sha256",
                "engine_executable_sha256",
                "numerical_settings_sha256",
            ] {
                let mut changed = serde_json::to_value(&entry.method).unwrap();
                changed[field] = "f".repeat(64).into();
                let method: EnergyMethodIdentity = serde_json::from_value(changed).unwrap();
                assert_eq!(
                    assess(&method, physical).unwrap().status,
                    EnergyQualificationStatus::Unassessed
                );
            }
            let unknown = assess(&entry.method, &"f".repeat(64)).unwrap();
            assert_eq!(unknown.status, EnergyQualificationStatus::Unassessed);
            assert_eq!(unknown.registry_version, entry.registry_version);
        }
    }
}
