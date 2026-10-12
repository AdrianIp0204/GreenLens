//! Policy and persistence fixtures; none are simulation/qualification evidence.
use super::*;
use greenlens_radiance::annual::AnnualProfile;
use serde_json::json;

fn identity(entry: &RegistryEntry) -> QualificationIdentity {
    let mut engine = registry().engine_build.clone();
    engine.method_version.clone_from(&entry.method_version);
    engine.adapter_version.clone_from(&entry.adapter_version);
    engine.fingerprint.clear();
    engine.fingerprint = canonical_hash(&engine).unwrap();
    engine.validate().unwrap();
    scientific_qualification(&serde_json::to_value(engine).unwrap(), &entry.profile)
        .unwrap()
        .identity
}

#[test]
fn frozen_failures_bind_complete_historical_profiles_engines_contracts_and_evidence() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert_eq!(registry().engine_build.library_sha256.len(), 147);
    assert_eq!(registry().entries.len(), 6);
    for entry in &registry().entries {
        let profile: AnnualProfile = serde_json::from_value(entry.profile.clone()).unwrap();
        profile.validate_for_method(&entry.method_version).unwrap();
        assert_eq!(
            canonical_hash(&profile).unwrap(),
            canonical_hash(&entry.profile).unwrap()
        );
        let assessed = resolve(identity(entry), registry()).unwrap();
        assert_eq!(assessed.status, QualificationStatus::Failed);
        assert_eq!(
            entry.recipe_contract_sha256,
            assessed.identity.recipe_contract_sha256
        );
        for evidence in &entry.evidence {
            assert_eq!(
                greenlens_core::sha256(&std::fs::read(repo.join(&evidence.id)).unwrap()),
                evidence.sha256
            );
        }
    }
    // Current constructors are checked against frozen data, never used to author it.
    for (i, profile) in [
        (4, AnnualProfile::shipping()),
        (5, AnnualProfile::reference()),
    ] {
        assert_eq!(
            serde_json::to_value(profile).unwrap(),
            registry().entries[i].profile
        );
    }
}

#[test]
fn any_changed_profile_engine_method_or_recipe_contract_is_unassessed() {
    let original = identity(&registry().entries[4]);
    for kind in 0..7 {
        let mut changed = original.clone();
        match kind {
            0 => changed.profile["ambient_divisions"] = json!(65),
            1 => changed.profile["solar_ambient_divisions"] = Value::Null,
            2 => changed.engine["library_sha256"]["lib/rayinit.cal"] = json!("f".repeat(64)),
            3 => changed.engine["version"] = json!("unknown engine"),
            4 => changed.method_version = "future-method/99".into(),
            5 => changed.recipe_contract_sha256 = "f".repeat(64),
            _ => changed.scope = "some_other_scope".into(),
        }
        changed.profile_sha256 = canonical_hash(&changed.profile).unwrap();
        changed.engine_sha256 = canonical_hash(&changed.engine).unwrap();
        assert_eq!(
            resolve(changed, registry()).unwrap().status,
            QualificationStatus::Unassessed,
            "mutation {kind}"
        );
    }
}

#[test]
fn limited_fixture_success_requires_exact_input_scope_and_never_erases_failure() {
    let mut policy = registry().clone();
    policy.entries.clear();
    let mut entry = registry().entries[4].clone();
    entry.scope = "synthetic_uniform_fixture_only".into();
    entry.status = QualificationStatus::LimitedQualified;
    entry.applicable_physical_inputs = vec!["a".repeat(64)];
    entry.evidence = vec![QualificationEvidence {
        id: "synthetic-test-only".into(),
        sha256: "0".repeat(64),
        tested_scope: "Synthetic policy fixture only; not native evidence".into(),
        outcome: QualificationStatus::LimitedQualified,
        reason: "Test resolver scope".into(),
    }];
    policy.entries.push(entry.clone());
    let mut request = identity(&entry);
    request.scope.clone_from(&entry.scope);
    assert_eq!(
        resolve(request.clone(), &policy).unwrap().status,
        QualificationStatus::Unassessed
    );
    request.physical_input_hash = Some("b".repeat(64));
    assert_eq!(
        resolve(request.clone(), &policy).unwrap().status,
        QualificationStatus::Unassessed
    );
    request.physical_input_hash = Some("a".repeat(64));
    assert_eq!(
        resolve(request.clone(), &policy).unwrap().status,
        QualificationStatus::LimitedQualified
    );
    request.scope = PROFILE_SCOPE.into();
    assert_eq!(
        resolve(request.clone(), &policy).unwrap().status,
        QualificationStatus::Unassessed
    );
    // A future successful method/profile and a later exact-key success cannot
    // overwrite the original profile's failed gate.
    policy.entries.push(registry().entries[4].clone());
    let mut later = registry().entries[4].clone();
    later.status = QualificationStatus::LimitedQualified;
    later.applicable_physical_inputs = vec!["a".repeat(64)];
    policy.entries.push(later.clone());
    later.method_version = "future-method/99".into();
    policy.entries.push(later);
    assert_eq!(
        resolve(request, &policy).unwrap().status,
        QualificationStatus::Failed
    );
}

#[test]
fn metadata_assessment_is_bounded_full_inspection_rejects_corrupt_chunks_without_mutation() {
    use crate::persistence::*;
    let temp = tempfile::tempdir().unwrap();
    let scratch = temp.path().join("scratch");
    let root = temp.path().join("project");
    std::fs::create_dir_all(&root).unwrap();
    let record = climate::tests::fixture(&scratch, false);
    let mut project = crate::example();
    project.schema_version = "0.3.0".into();
    project.alternatives[0].model = record.model.clone();
    let mut extension = climate::ClimateProject::default();
    extension.weather.push(
        climate::import_project_weather(
            &scratch.join("weather"),
            &root,
            &uuid::Uuid::new_v4().to_string(),
        )
        .unwrap(),
    );
    extension.studies.insert("A".into(), record.study.clone());
    climate::append_run(&mut extension, &record).unwrap();
    let summary = extension.runs[0].clone();
    project.climate = Some(extension);
    promote_run(&scratch, &root, &record.id).unwrap();
    atomic_json(&root, &project).unwrap();
    let dir = root.join(&summary.path);
    let before = std::fs::read(dir.join("results.json")).unwrap();
    let assessment = inspect_climate(&root, &project, &record.id).unwrap();
    assert_eq!(assessment.record_sha256, summary.record_sha256);
    assert_eq!(assessment.recipe_sha256, record.provenance["recipe_sha256"]);
    assert_eq!(
        assessment.current_input_relation.physical,
        InputRelation::Matches
    );
    assert_eq!(
        assessment.scientific_qualification.status,
        QualificationStatus::Unassessed
    ); // synthetic incomplete engine inventory
    project.alternatives[0].model.window.width_m += 0.1;
    assert_eq!(
        assess_climate(&root, &project, &record.id)
            .unwrap()
            .current_input_relation
            .physical,
        InputRelation::Differs
    );
    let chunk = dir.join("series").join(&record.series.chunks[0].file);
    let mut bytes = std::fs::read(&chunk).unwrap();
    bytes[0] ^= 1;
    std::fs::write(&chunk, bytes).unwrap();
    assert_eq!(
        assess_climate(&root, &project, &record.id)
            .unwrap()
            .evidence_integrity,
        EvidenceIntegrity::SealedMetadataChecked
    );
    assert!(inspect_climate(&root, &project, &record.id).is_err());
    assert_eq!(std::fs::read(dir.join("results.json")).unwrap(), before);
}
