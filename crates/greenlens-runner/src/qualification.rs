//! Supplemental assessment of immutable evidence. This registry is compiled
//! application policy, never imported from a project or an evidence package.
use crate::{climate, studies, Project};
use anyhow::{Context, Result};
use greenlens_core::{canonical_hash, study::StudyMode};
use greenlens_radiance::annual::{self, AnnualEngineIdentity};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path, sync::OnceLock};

const REGISTRY_BYTES: &str = include_str!("qualification-registry.json");
pub const PROFILE_SCOPE: &str = "climate_daylight_profile";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualificationStatus {
    Failed,
    Unassessed,
    LimitedQualified,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceIntegrity {
    ValidatedRecordOnly,
    SealedMetadataChecked,
    FullRetainedEvidenceChecked,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessCompletion {
    RetainedSuccessfulExitRecords,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputRelation {
    Matches,
    Differs,
    NotApplicable,
    NotCompared,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentInputRelation {
    pub physical: InputRelation,
    /// Currency of the retained metric results: both physical identity and
    /// metric settings must match. This is not settings equality alone.
    pub metrics: InputRelation,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationIdentity {
    pub method_version: String,
    pub adapter_version: String,
    pub profile: Value,
    pub profile_sha256: String,
    pub engine: Value,
    pub engine_sha256: String,
    pub recipe_contract_sha256: String,
    pub physical_input_hash: Option<String>,
    pub scope: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationEvidence {
    pub id: String,
    pub sha256: String,
    pub tested_scope: String,
    pub outcome: QualificationStatus,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScientificQualification {
    pub status: QualificationStatus,
    pub registry_version: String,
    pub registry_sha256: String,
    pub identity: QualificationIdentity,
    pub profile_display_name: String,
    pub reason: String,
    pub evidence: Vec<QualificationEvidence>,
    pub limitations: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunAssessment {
    pub schema_version: String,
    pub run_id: String,
    pub record_sha256: String,
    pub recipe_sha256: String,
    pub process_completion: ProcessCompletion,
    pub evidence_integrity: EvidenceIntegrity,
    pub current_input_relation: CurrentInputRelation,
    pub scientific_qualification: ScientificQualification,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RegistryEntry {
    method_version: String,
    adapter_version: String,
    profile: Value,
    scope: String,
    recipe_contract_sha256: String,
    #[serde(default)]
    applicable_physical_inputs: Vec<String>,
    status: QualificationStatus,
    evidence: Vec<QualificationEvidence>,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    schema_version: String,
    version: String,
    engine_build: AnnualEngineIdentity,
    entries: Vec<RegistryEntry>,
}
fn registry() -> &'static Registry {
    static VALUE: OnceLock<Registry> = OnceLock::new();
    VALUE.get_or_init(|| {
        let value: Registry =
            serde_json::from_str(REGISTRY_BYTES).expect("compiled qualification registry");
        assert_eq!(value.schema_version, "greenlens.qualification-registry.v1");
        value
    })
}

/// Display-only name; never write this over the serialized numerical role.
pub fn profile_display_name(profile: &Value) -> &'static str {
    match profile["name"].as_str() {
        Some("shipping") => "Candidate profile",
        Some("reference") => "Higher-sampling comparison profile",
        _ => "Unrecognized numerical profile",
    }
}

fn resolve(
    identity: QualificationIdentity,
    registry: &Registry,
) -> Result<ScientificQualification> {
    let mut status = QualificationStatus::Unassessed;
    let mut evidence = Vec::new();
    for entry in &registry.entries {
        // The complete engine inventory is bound, including every library.
        // Method/adapter and their redundant fingerprint are derived for this
        // frozen entry, not from the currently selected adapter constructor.
        let mut engine = registry.engine_build.clone();
        engine.method_version.clone_from(&entry.method_version);
        engine.adapter_version.clone_from(&entry.adapter_version);
        engine.fingerprint.clear();
        engine.fingerprint = canonical_hash(&engine)?;
        if entry.method_version != identity.method_version
            || entry.adapter_version != identity.adapter_version
            || entry.scope != identity.scope
            || entry.recipe_contract_sha256 != identity.recipe_contract_sha256
            || canonical_hash(&entry.profile)? != identity.profile_sha256
            || canonical_hash(&engine)? != identity.engine_sha256
        {
            continue;
        }
        // Positive fixture findings require an exact physical-input scope.
        // No caller context (or a broad profile label) can expand that scope.
        if entry.status == QualificationStatus::LimitedQualified
            && !identity
                .physical_input_hash
                .as_ref()
                .is_some_and(|hash| entry.applicable_physical_inputs.contains(hash))
        {
            continue;
        }
        evidence.extend(entry.evidence.clone());
        // A later limited success cannot erase a known failed exact identity.
        if entry.status == QualificationStatus::Failed {
            status = QualificationStatus::Failed;
        } else if status != QualificationStatus::Failed {
            status = entry.status;
        }
    }
    let reason = match status {
        QualificationStatus::Failed => "This exact numerical profile and engine failed a recorded scientific release gate. This does not assert that every geometry or individual reading is inaccurate.",
        QualificationStatus::Unassessed => "No compiled scientific assessment matches this exact method, complete numerical profile, engine inventory and requested scope. Successful execution, integrity checks and replay do not establish qualification.",
        QualificationStatus::LimitedQualified => "Agreement is qualified only for the explicitly listed tested scope and assumptions. It is not general building validation or certification.",
    };
    Ok(ScientificQualification {
        status,
        registry_version: registry.version.clone(),
        registry_sha256: greenlens_core::sha256(REGISTRY_BYTES.as_bytes()),
        profile_display_name: profile_display_name(&identity.profile).into(),
        identity,
        reason: reason.into(),
        evidence,
        limitations: vec![
            "Assessment supplements the immutable run; it does not rewrite result bytes, fingerprints or historical outcomes.".into(),
            "Input compatibility, successful process records, evidence integrity and same-recipe replay are separate checks, not transport accuracy or certification.".into(),
        ],
    })
}

fn recipe_contract_hash(method: &str) -> Result<String> {
    let (schema, ground, direct) = match method {
        annual::LEGACY_METHOD_VERSION => (
            annual::LEGACY_RECIPE_SCHEMA_VERSION,
            annual::LEGACY_GROUND_POLICY,
            annual::LEGACY_DIRECT_POLICY,
        ),
        annual::EXPLICIT_METHOD_VERSION => (
            annual::EXPLICIT_RECIPE_SCHEMA_VERSION,
            annual::EXPLICIT_GROUND_POLICY,
            annual::EXPLICIT_DIRECT_POLICY,
        ),
        annual::REPEATED_METHOD_VERSION => (
            annual::REPEATED_RECIPE_SCHEMA_VERSION,
            annual::REPEATED_GROUND_POLICY,
            annual::REPEATED_DIRECT_POLICY,
        ),
        annual::METHOD_VERSION => (
            annual::RECIPE_SCHEMA_VERSION,
            annual::GROUND_POLICY,
            annual::DIRECT_POLICY,
        ),
        _ => return Ok("unknown".into()),
    };
    Ok(canonical_hash(
        &serde_json::json!({"schema":schema,"ground":ground,"direct":direct,
        "time":annual::TIME_POLICY,"ordering":annual::RESULT_ORDERING,"units":annual::RESULT_UNITS}),
    )?)
}

/// Pure scientific policy lookup. Unknown identities remain unassessed; this
/// function does not grant permission to open or execute an unknown recipe.
pub fn scientific_qualification(
    engine: &Value,
    profile: &Value,
) -> Result<ScientificQualification> {
    let identity = QualificationIdentity {
        method_version: engine["method_version"]
            .as_str()
            .unwrap_or("unknown")
            .into(),
        adapter_version: engine["adapter_version"]
            .as_str()
            .unwrap_or("unknown")
            .into(),
        profile: profile.clone(),
        profile_sha256: canonical_hash(profile)?,
        engine: engine.clone(),
        engine_sha256: canonical_hash(engine)?,
        recipe_contract_sha256: recipe_contract_hash(
            engine["method_version"].as_str().unwrap_or("unknown"),
        )?,
        physical_input_hash: None,
        scope: PROFILE_SCOPE.into(),
    };
    resolve(identity, registry())
}

/// Only call after the corresponding reader has performed the stated checks.
/// This constructor is crate-private so clients cannot claim a full scan.
pub(crate) fn from_record(
    record: &climate::ClimateRunRecord,
    project: Option<&Project>,
    integrity: EvidenceIntegrity,
) -> Result<RunAssessment> {
    climate::validate_record(record)?;
    let relation = if let Some(project) = project {
        crate::validate_project(project)?;
        let current = project
            .alternatives
            .iter()
            .find(|a| a.id == record.alternative_id)
            .context("Unknown assessment alternative")?;
        let study = project
            .climate
            .as_ref()
            .and_then(|c| c.studies.get(&record.alternative_id))
            .cloned()
            .unwrap_or_default();
        let physical = !studies::is_stale(record, &current.model, &study)?;
        let metrics = if !matches!(record.study.calculation, StudyMode::Annual { .. }) {
            InputRelation::NotApplicable
        } else if physical
            && matches!(study.calculation, StudyMode::Annual { .. })
            && study.metrics == record.study.metrics
        {
            InputRelation::Matches
        } else {
            InputRelation::Differs
        };
        CurrentInputRelation {
            physical: if physical {
                InputRelation::Matches
            } else {
                InputRelation::Differs
            },
            metrics,
        }
    } else {
        CurrentInputRelation {
            physical: InputRelation::NotCompared,
            metrics: InputRelation::NotCompared,
        }
    };
    Ok(RunAssessment {
        schema_version: "greenlens.run-assessment.v1".into(),
        run_id: record.id.clone(),
        record_sha256: canonical_hash(record)?,
        recipe_sha256: record.provenance["recipe_sha256"]
            .as_str()
            .context("Missing retained recipe hash")?
            .into(),
        process_completion: ProcessCompletion::RetainedSuccessfulExitRecords,
        evidence_integrity: integrity,
        current_input_relation: relation,
        scientific_qualification: {
            let mut identity = scientific_qualification(
                &record.provenance["engine"],
                &record.provenance["profile"],
            )?
            .identity;
            identity.physical_input_hash = Some(record.study.physical_input_hash(&record.model)?);
            resolve(identity, registry())?
        },
    })
}

fn assess(root: &Path, project: &Project, run_id: &str, full: bool) -> Result<RunAssessment> {
    crate::validate_project(project)?;
    let summary = project
        .climate
        .as_ref()
        .context("No climate evidence")?
        .runs
        .iter()
        .find(|run| run.id == run_id)
        .context("Unknown climate run")?;
    let record = if full {
        climate::read_run(root, summary)?
    } else {
        climate::read_result_metadata(root, summary)?
    };
    from_record(
        &record,
        Some(project),
        if full {
            EvidenceIntegrity::FullRetainedEvidenceChecked
        } else {
            EvidenceIntegrity::SealedMetadataChecked
        },
    )
}
/// Bounded fresh relation to an in-memory document; no annual chunk scan.
pub fn assess_climate(root: &Path, project: &Project, run_id: &str) -> Result<RunAssessment> {
    assess(root, project, run_id, false)
}
/// Full evidence inspection. Errors never yield a successful integrity claim.
pub fn inspect_climate(root: &Path, project: &Project, run_id: &str) -> Result<RunAssessment> {
    assess(root, project, run_id, true)
}
pub(crate) fn assessments_for_verified_view(
    project: &Project,
    records: &[climate::ClimateRunRecord],
) -> Result<BTreeMap<String, RunAssessment>> {
    records
        .iter()
        .map(|run| {
            Ok((
                run.id.clone(),
                from_record(
                    run,
                    Some(project),
                    EvidenceIntegrity::FullRetainedEvidenceChecked,
                )?,
            ))
        })
        .collect()
}

#[cfg(test)]
#[path = "qualification_tests.rs"]
mod tests;
