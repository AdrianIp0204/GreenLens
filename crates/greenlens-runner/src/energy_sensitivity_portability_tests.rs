//! Synthetic storage/policy regressions only. No simulator, fabricated native
//! run, or scientific qualification is used by these tests.
use crate::{
    energy_sensitivity::{
        self as sensitivity, EnergySensitivityPlan, EnergySensitivityPlannedSlot,
    },
    energy_sensitivity_workflow::{
        self as workflow, EnergySensitivityEventSeal, EnergySensitivityReference,
    },
    persistence, Project, ProjectView,
};
use greenlens_core::{canonical_hash, energy::*};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::Path};

const SCHEMA: &str = "greenlens.energy-sensitivity-session.v1";

fn declaration() -> EnergySensitivityDeclaration {
    let report = synthetic_energy_report().unwrap();
    let mut fine = report.baseline.study.clone();
    fine.zone_timesteps_per_hour = 12;
    declare_energy_sensitivity(
        vec![
            EnergySensitivityTarget {
                alternative_id: "A".into(),
                model: report.baseline.model,
                study_10min: report.baseline.study.clone(),
            },
            EnergySensitivityTarget {
                alternative_id: "B".into(),
                model: report.proposed.model,
                study_10min: report.proposed.study,
            },
        ],
        greenlens_energyplus::method_identity(&report.baseline.study).unwrap(),
        greenlens_energyplus::method_identity(&fine).unwrap(),
    )
    .unwrap()
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> String {
    let mut bytes = serde_json::to_vec_pretty(value).unwrap();
    bytes.push(b'\n');
    fs::write(path, &bytes).unwrap();
    persistence::bytes_hash(&bytes)
}

fn append(root: &Path, reference: &mut EnergySensitivityReference, event: Value) {
    let sequence = reference.events.len() as u8 + 1;
    let file = json!({
        "schema_version": SCHEMA, "sequence": sequence,
        "plan_sha256": reference.plan_sha256,
        "previous_file_sha256": reference.events.last().map(|e| &e.file_sha256).unwrap_or(&reference.plan_file_sha256),
        "recorded_utc": "2026-01-01T01:00:00Z", "event": event,
    });
    let hash = write_json(
        &root
            .join(&reference.path)
            .join(format!("event-{sequence:03}.json")),
        &file,
    );
    reference.events.push(EnergySensitivityEventSeal {
        sequence,
        file_sha256: hash,
    });
}

/// A valid, inactive historical declaration plus an explicitly synthetic
/// cancellation receipt. There are no weather assets or native run records.
fn fixture() -> (tempfile::TempDir, ProjectView, EnergySensitivityReference) {
    let root = tempfile::tempdir().unwrap();
    let mut project = crate::energy_services::example_document();
    let declaration = declaration();
    let plan = EnergySensitivityPlan {
        schema_version: "greenlens.energy-sensitivity-plan.v1".into(),
        created_utc: "2026-01-01T00:00:00Z".into(),
        declaration_sha256: declaration.identity().unwrap(),
        current_inputs_sha256: sensitivity::current_inputs_identity(
            &project,
            &["A".into(), "B".into()],
        )
        .unwrap(),
        slots: declaration
            .slots
            .iter()
            .map(|s| EnergySensitivityPlannedSlot {
                slot_id: s.id.clone(),
                reuse: None,
            })
            .collect(),
        declaration,
        execution_authorized: false,
        reuse_policy: sensitivity::REUSE_POLICY.into(),
    };
    let id = "00000000-0000-4000-8000-000000000010".to_string();
    let mut reference = EnergySensitivityReference {
        path: format!("energy-assessments/{id}"),
        id,
        plan_sha256: plan.identity().unwrap(),
        plan_file_sha256: String::new(),
        events: vec![],
    };
    fs::create_dir_all(root.path().join(&reference.path)).unwrap();
    reference.plan_file_sha256 = write_json(
        &root.path().join(&reference.path).join("plan.json"),
        &json!({
            "schema_version": SCHEMA, "project_id": project.id, "plan": plan,
        }),
    );
    append(
        root.path(),
        &mut reference,
        json!({"kind":"reserved", "slot_id":"A10"}),
    );
    let reason = "Synthetic cancellation storage fixture; no worker or engine was started.";
    append(
        root.path(),
        &mut reference,
        json!({
            "kind":"stopped", "slot_id":"A10", "job_id":null,
            "interruption":"cancelled", "reason":reason, "status_sha256":null, "process":null,
        }),
    );
    let interruption = EnergySensitivityInterruption {
        slot_id: "A10".into(),
        kind: EnergySensitivityInterruptionKind::Cancelled,
        receipt_sha256: reference.events[1].file_sha256.clone(),
        reason: reason.into(),
    };
    let result = sensitivity::evaluate(
        root.path(),
        &project,
        &plan,
        &BTreeMap::new(),
        &[interruption],
    )
    .unwrap();
    append(
        root.path(),
        &mut reference,
        json!({"kind":"finalized", "result":result}),
    );
    project
        .energy
        .as_mut()
        .unwrap()
        .workbench
        .get_or_insert_with(Default::default)
        .sensitivity
        .push(reference.clone());
    let view = crate::save_project(root.path(), &project, None).unwrap();
    (root, view, reference)
}

fn inventory(root: &Path, project: &Project) -> BTreeMap<String, Vec<u8>> {
    let mut members = workflow::evidence_members(root, project).unwrap();
    members.push("project.json".into());
    members
        .into_iter()
        .map(|p| (p.clone(), fs::read(root.join(p)).unwrap()))
        .collect()
}

#[test]
fn sensitivity_only_document_requires_source_for_save_as() {
    let (_source, view, _) = fixture();
    let target = tempfile::tempdir().unwrap();
    let energy = view.project.energy.as_ref().unwrap();
    assert!(energy.runs.is_empty() && energy.weather.is_empty() && energy.attachments.is_empty());
    let error = crate::save_project_as(None, target.path(), &view.project, None).unwrap_err();
    assert!(error.to_string().contains("source directory"));
    assert!(!target.path().join("project.json").exists());
}

#[test]
fn save_as_copies_exact_receipts_and_reopens_without_original_source() {
    let (source, view, reference) = fixture();
    let before = inventory(source.path(), &view.project);
    let target = tempfile::tempdir().unwrap();
    let copy = crate::save_project_as(
        Some(source.path()),
        target.path(),
        &view.project,
        Some(&view.revision),
    )
    .unwrap();
    assert_eq!(inventory(source.path(), &view.project), before);
    assert_eq!(inventory(target.path(), &copy.project), before);
    drop(source);
    let reopened = crate::open_project(target.path()).unwrap();
    let session = workflow::inspect(target.path(), &reopened.project, &reference.id).unwrap();
    assert_eq!(
        serde_json::to_value(&reopened.energy_sensitivity_sessions[&reference.id]).unwrap(),
        serde_json::to_value(&session).unwrap(),
        "Open returns the same checked session without requiring panel inspection"
    );
    assert!(reopened
        .project_verification
        .as_ref()
        .unwrap()
        .progress
        .numeric_passes
        .is_empty());
    assert!(session.finalized && session.next_slot_id.is_none());
    let result = session.result.unwrap();
    assert!(!result.assessment.all_declared_runs_integrity_checked);
    assert!(
        !result.assessment.numerical_sensitivity_passed
            && !result.assessment.qualification_promoted
    );
    assert_eq!(
        result.assessment.interruptions[0].kind,
        EnergySensitivityInterruptionKind::Cancelled
    );

    // Accepted design edits change currentness, not retained historical bytes.
    let mut changed = reopened.project.clone();
    changed
        .energy
        .as_mut()
        .unwrap()
        .designs
        .get_mut("A")
        .unwrap()
        .model
        .zones[0]
        .cooling_system
        .as_mut()
        .unwrap()
        .supply_fan
        .total_efficiency = 0.7;
    let changed = crate::save_project(target.path(), &changed, Some(&reopened.revision)).unwrap();
    let result = workflow::inspect(target.path(), &changed.project, &reference.id)
        .unwrap()
        .result
        .unwrap();
    assert!(!result.current_inputs_match);
    for (path, bytes) in before.iter().filter(|(p, _)| p.as_str() != "project.json") {
        assert_eq!(&fs::read(target.path().join(path)).unwrap(), bytes);
    }
}

#[test]
fn tampered_receipt_blocks_open_save_copy_and_export_selection() {
    let (source, view, reference) = fixture();
    let manifest = fs::read(source.path().join("project.json")).unwrap();
    let path = source.path().join(&reference.path).join("event-002.json");
    let mut bytes = fs::read(&path).unwrap();
    bytes.push(b' '); // Even semantically equivalent JSON violates the raw seal.
    fs::write(&path, bytes).unwrap();
    assert!(crate::open_project(source.path()).is_err());
    assert!(crate::save_project(source.path(), &view.project, Some(&view.revision)).is_err());
    let target = tempfile::tempdir().unwrap();
    assert!(crate::save_project_as(
        Some(source.path()),
        target.path(),
        &view.project,
        Some(&view.revision)
    )
    .is_err());
    assert!(!target.path().join("project.json").exists());
    // The mismatching pair cannot bypass validation of an excluded session.
    assert!(
        workflow::export_selection(source.path(), &view.project, "other-A", "other-B").is_err()
    );
    assert_eq!(
        fs::read(source.path().join("project.json")).unwrap(),
        manifest
    );
}

#[test]
fn unsafe_reference_and_extra_receipt_are_rejected_without_overwrite() {
    let (source, view, reference) = fixture();
    let mut bad = view.project.clone();
    bad.energy
        .as_mut()
        .unwrap()
        .workbench
        .as_mut()
        .unwrap()
        .sensitivity[0]
        .path = "../outside".into();
    assert!(
        workflow::copy_evidence(source.path(), tempfile::tempdir().unwrap().path(), &bad).is_err()
    );
    assert!(crate::validate_project(&bad).is_err());
    let target = tempfile::tempdir().unwrap();
    fs::create_dir_all(target.path().join(&reference.path)).unwrap();
    let occupied = target.path().join(&reference.path).join("plan.json");
    fs::write(&occupied, b"unrelated existing bytes").unwrap();
    assert!(workflow::copy_evidence(source.path(), target.path(), &view.project).is_err());
    assert_eq!(fs::read(occupied).unwrap(), b"unrelated existing bytes");
    let target = tempfile::tempdir().unwrap();
    fs::write(target.path().join("energy-assessments"), b"not a directory").unwrap();
    assert!(workflow::copy_evidence(source.path(), target.path(), &view.project).is_err());
    assert_eq!(
        fs::read(target.path().join("energy-assessments")).unwrap(),
        b"not a directory"
    );
    fs::write(
        source.path().join(&reference.path).join("event-004.json"),
        b"{}",
    )
    .unwrap();
    assert!(crate::open_project(source.path()).is_err());
    assert!(workflow::evidence_members(source.path(), &view.project).is_err());
}

/// Policy-only summaries: these never enter read_run or a project manifest.
fn selection_result(fail: bool, fine_ids: u16) -> sensitivity::EnergySensitivityResult {
    let declaration = declaration();
    let report = synthetic_energy_report().unwrap();
    let runs = declaration
        .slots
        .iter()
        .map(|slot| {
            let mut totals = if slot.alternative_id == "A" {
                report.baseline.totals.clone()
            } else {
                report.proposed.totals.clone()
            };
            totals.interval_count = 8760 * usize::from(slot.zone_timesteps_per_hour);
            totals.interval_seconds = 3600 / u32::from(slot.zone_timesteps_per_hour);
            // Use internally consistent comfort percentages; no native acceptance.
            for c in &mut totals.zone_comfort {
                c.cooling_unmet_hours = 0.;
                c.unmet_percent = Some(0.);
            }
            if fail && slot.id == "B10" {
                totals.annual.facility_kwh *= 2.;
            }
            let number = match slot.id.as_str() {
                "A10" => 110,
                "B10" => 210,
                "A5" => fine_ids,
                "B5" => fine_ids + 100,
                _ => unreachable!(),
            };
            EnergySensitivityRun {
                slot_id: slot.id.clone(),
                run_id: format!("00000000-0000-4000-8000-{number:012}"),
                record_sha256: "a".repeat(64),
                physical_input_sha256: slot.physical_input_sha256.clone(),
                execution_input_sha256: slot.execution_input_sha256.clone(),
                method: slot.method.clone(),
                totals,
            }
        })
        .collect::<Vec<_>>();
    sensitivity::EnergySensitivityResult {
        schema_version: "greenlens.energy-sensitivity-result.v1".into(),
        plan_sha256: "b".repeat(64),
        current_inputs_match: true,
        assessment: assess_energy_sensitivity(&declaration, &runs, &[]).unwrap(),
        evidence_scope: "Synthetic policy-only summaries; no native evidence".into(),
    }
}

#[test]
fn export_keeps_first_exact_pair_even_if_failed_and_later_pair_passes() {
    let failed = selection_result(true, 115);
    let passed = selection_result(false, 125);
    assert!(!failed.assessment.numerical_sensitivity_passed);
    assert!(passed.assessment.numerical_sensitivity_passed);
    let pair = (
        "00000000-0000-4000-8000-000000000110",
        "00000000-0000-4000-8000-000000000210",
    );
    let mut wrong = passed.clone();
    wrong
        .assessment
        .runs
        .iter_mut()
        .find(|r| r.slot_id == "A10")
        .unwrap()
        .run_id = "different-pair".into();
    let sessions = vec![
        ("unfinalized".into(), None),
        ("wrong".into(), Some(wrong)),
        ("first-failed".into(), Some(failed)),
        ("later-pass".into(), Some(passed)),
    ];
    let before = canonical_hash(&sessions).unwrap();
    let selected = workflow::select_export_sessions(pair.0, pair.1, &sessions).unwrap();
    assert_eq!(
        selected.included_session_id.as_deref(),
        Some("first-failed")
    );
    assert_eq!(
        selected.excluded_session_ids,
        vec!["unfinalized", "wrong", "later-pass"]
    );
    assert_eq!(
        selected.included_run_ids,
        [110, 115, 210, 215].map(|n| format!("00000000-0000-4000-8000-{n:012}"))
    );
    let reversed = workflow::select_export_sessions(pair.1, pair.0, &sessions).unwrap();
    assert!(reversed.included_session_id.is_none());
    assert_eq!(reversed.included_run_ids.len(), 2);
    assert_eq!(canonical_hash(&sessions).unwrap(), before);
}
