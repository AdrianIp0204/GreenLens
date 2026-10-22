//! Separately declared numerical sensitivity. These supplements never alter a
//! run, the scientific registry, or the frozen v0.4 report comparison policy.
use super::*;
use crate::{canonical_hash, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const ENERGY_SENSITIVITY_SCHEMA: &str = "greenlens.energy-sensitivity-declaration.v1";
pub const ENERGY_SENSITIVITY_POLICY: &str = "greenlens.energy-timestep-sensitivity/1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergySensitivityTarget {
    pub alternative_id: String,
    pub model: ThermalModel,
    /// Explicit ten-minute base; the fine study changes only this timestep.
    pub study_10min: EnergyStudy,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergySensitivitySlot {
    pub id: String,
    pub alternative_id: String,
    pub zone_timesteps_per_hour: u8,
    pub model_input_sha256: String,
    pub physical_input_sha256: String,
    pub execution_input_sha256: String,
    pub method: EnergyMethodIdentity,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergySensitivityDeclaration {
    pub schema_version: String,
    pub policy_id: String,
    pub targets: Vec<EnergySensitivityTarget>,
    /// Stable order: selected A/B ten-minute slots, then their five-minute slots.
    pub slots: Vec<EnergySensitivitySlot>,
    pub annual_electricity_limit_percent: f64,
    pub common_peak_limit_percent: f64,
    pub occupied_cooling_unmet_limit_percent: f64,
    pub cooling_tolerance_c: f64,
    pub denominator_policy: String,
    pub comfort_policy: String,
}
const DENOMINATOR: &str = "Absolute ten-minus-five difference divided by the absolute five-minute reference; both zero is zero difference; nonzero versus zero fails without an infinite JSON number.";
const COMFORT: &str = "Each conditioned zone in every declared run: occupied air temperature > scheduled cooling setpoint + 0.2 C, at source resolution; occupied-unmet <=1%. No occupied hours does not establish this predicate. Building-union comfort is descriptive only.";

pub fn declare_energy_sensitivity(
    targets: Vec<EnergySensitivityTarget>,
    ten: EnergyMethodIdentity,
    five: EnergyMethodIdentity,
) -> Result<EnergySensitivityDeclaration> {
    let slots = expected_slots(&targets, &ten, &five)?;
    let result = EnergySensitivityDeclaration {
        schema_version: ENERGY_SENSITIVITY_SCHEMA.into(),
        policy_id: ENERGY_SENSITIVITY_POLICY.into(),
        targets,
        slots,
        annual_electricity_limit_percent: 1.,
        common_peak_limit_percent: 3.,
        occupied_cooling_unmet_limit_percent: 1.,
        cooling_tolerance_c: COMFORT_COOLING_TOLERANCE_C,
        denominator_policy: DENOMINATOR.into(),
        comfort_policy: COMFORT.into(),
    };
    result.validate()?;
    Ok(result)
}
fn expected_slots(
    targets: &[EnergySensitivityTarget],
    ten: &EnergyMethodIdentity,
    five: &EnergyMethodIdentity,
) -> Result<Vec<EnergySensitivitySlot>> {
    if targets.is_empty()
        || targets.len() > 2
        || targets
            .iter()
            .map(|t| t.alternative_id.as_str())
            .collect::<Vec<_>>()
            != ["A", "B"]
                .into_iter()
                .filter(|id| targets.iter().any(|t| t.alternative_id == *id))
                .collect::<Vec<_>>()
    {
        return Err(invalid(
            "sensitivity.targets",
            "Explicitly select unique A and/or B targets in A/B order",
        ));
    }
    ten.validate()?;
    five.validate()?;
    let mut normalized = five.clone();
    normalized.numerical_settings_sha256 = ten.numerical_settings_sha256.clone();
    if normalized != *ten || ten.numerical_settings_sha256 == five.numerical_settings_sha256 {
        return Err(invalid("sensitivity.methods", "Ten-/five-minute methods require the same complete adapter/engine/recipe identity and distinct declared numerical settings"));
    }
    for target in targets {
        target.study_10min.validate(&target.model)?;
        if target.study_10min.zone_timesteps_per_hour != 6 {
            return Err(invalid(
                "sensitivity.study",
                "Base study must explicitly use ten-minute intervals",
            ));
        }
    }
    if targets.len() == 2
        && !compare_energy_inputs(
            &targets[0].model,
            &targets[0].study_10min,
            &targets[1].model,
            &targets[1].study_10min,
        )?
        .comparable_geometry_weather_calendar
    {
        return Err(invalid("sensitivity.pair", "Pair declaration requires unchanged A/B geometry, weather, site and calendar compatibility"));
    }
    let mut slots = Vec::new();
    for (steps, method) in [(6, ten), (12, five)] {
        for target in targets {
            let mut study = target.study_10min.clone();
            study.zone_timesteps_per_hour = steps;
            slots.push(EnergySensitivitySlot {
                id: format!("{}{}", target.alternative_id, 60 / steps),
                alternative_id: target.alternative_id.clone(),
                zone_timesteps_per_hour: steps,
                model_input_sha256: validate_thermal_model(&target.model, &study.calendar)?
                    .model_input_sha256,
                physical_input_sha256: study.physical_input_hash(&target.model)?,
                execution_input_sha256: study.execution_input_hash(&target.model, method)?,
                method: method.clone(),
            });
        }
    }
    Ok(slots)
}
impl EnergySensitivityDeclaration {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != ENERGY_SENSITIVITY_SCHEMA
            || self.policy_id != ENERGY_SENSITIVITY_POLICY
            || self.annual_electricity_limit_percent != 1.
            || self.common_peak_limit_percent != 3.
            || self.occupied_cooling_unmet_limit_percent != 1.
            || self.cooling_tolerance_c != COMFORT_COOLING_TOLERANCE_C
            || self.denominator_policy != DENOMINATOR
            || self.comfort_policy != COMFORT
        {
            return Err(invalid(
                "sensitivity.policy",
                "Unsupported or altered sensitivity policy/criteria",
            ));
        }
        let ten = self
            .slots
            .iter()
            .find(|s| s.zone_timesteps_per_hour == 6)
            .ok_or_else(|| invalid("sensitivity.slots", "Missing ten-minute declaration"))?;
        let five = self
            .slots
            .iter()
            .find(|s| s.zone_timesteps_per_hour == 12)
            .ok_or_else(|| invalid("sensitivity.slots", "Missing five-minute declaration"))?;
        if self.slots != expected_slots(&self.targets, &ten.method, &five.method)? {
            return Err(invalid("sensitivity.slots","Slots, methods, order or complete input identities differ from the declared studies"));
        }
        Ok(())
    }
    pub fn identity(&self) -> Result<String> {
        self.validate()?;
        canonical_hash(self)
    }
    pub fn slot_inputs(&self, id: &str) -> Result<(&ThermalModel, EnergyStudy)> {
        self.validate()?;
        let slot = self
            .slots
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| invalid("sensitivity.slot", "Unknown declared slot"))?;
        let target = self
            .targets
            .iter()
            .find(|t| t.alternative_id == slot.alternative_id)
            .expect("validated slot target");
        let mut study = target.study_10min.clone();
        study.zone_timesteps_per_hour = slot.zone_timesteps_per_hour;
        Ok((&target.model, study))
    }
}

/// A caller must populate this only after a full immutable run read/reprocess.
/// Parsing a JSON claim alone is not evidence-integrity verification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergySensitivityRun {
    pub slot_id: String,
    pub run_id: String,
    pub record_sha256: String,
    pub physical_input_sha256: String,
    pub execution_input_sha256: String,
    pub method: EnergyMethodIdentity,
    pub totals: EnergyTotals,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnergySensitivityInterruptionKind {
    Failed,
    Cancelled,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergySensitivityInterruption {
    pub slot_id: String,
    pub kind: EnergySensitivityInterruptionKind,
    pub receipt_sha256: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergySensitivityCriterion {
    pub id: String,
    pub alternative_id: String,
    pub slot_ids: Vec<String>,
    pub quantity: String,
    pub zone_id: Option<String>,
    pub ten_minute: Option<f64>,
    pub five_minute_reference: Option<f64>,
    pub measured_percent: Option<f64>,
    pub limit_percent: f64,
    pub passed: bool,
    pub explanation: String,
}
/// Presentation semantics only: this does not verify retained runs, evaluate a
/// criterion, or change its serialized representation or outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnergySensitivityCriterionSemantics {
    PairedAnnual,
    PairedPeak,
    SingleRunComfort,
    Unsupported,
}
impl EnergySensitivityCriterion {
    pub fn semantics_v1(&self) -> EnergySensitivityCriterionSemantics {
        use EnergySensitivityCriterionSemantics::{
            PairedAnnual, PairedPeak, SingleRunComfort, Unsupported,
        };
        // Recognition is bound to the existing v1 generator's frozen limits;
        // a changed policy must not inherit its presentation semantics.
        let expected_limit = match self.quantity.as_str() {
            "annual_facility_kwh" | "occupied_cooling_unmet_percent" => 1.,
            "common_ten_minute_peak_kw" => 3.,
            _ => return Unsupported,
        };
        if !matches!(self.alternative_id.as_str(), "A" | "B")
            || self.limit_percent != expected_limit
            || [
                self.ten_minute,
                self.five_minute_reference,
                self.measured_percent,
            ]
            .into_iter()
            .flatten()
            .any(|value| !value.is_finite())
        {
            return Unsupported;
        }
        let ten_id = format!("{}10", self.alternative_id);
        let five_id = format!("{}5", self.alternative_id);
        match self.quantity.as_str() {
            "annual_facility_kwh" | "common_ten_minute_peak_kw"
                if self.zone_id.is_none()
                    && self.id == format!("{}:{}", self.alternative_id, self.quantity)
                    && self
                        .slot_ids
                        .iter()
                        .map(String::as_str)
                        .eq([ten_id.as_str(), five_id.as_str()]) =>
            {
                // Missing required paired measurements are still paired checks;
                // callers must disclose Missing rather than infer N/A.
                if self.quantity == "annual_facility_kwh" {
                    PairedAnnual
                } else {
                    PairedPeak
                }
            }
            "occupied_cooling_unmet_percent"
                if self.ten_minute.is_none() && self.five_minute_reference.is_none() =>
            {
                let (Some(zone), [slot]) = (self.zone_id.as_deref(), self.slot_ids.as_slice())
                else {
                    return Unsupported;
                };
                if identifier("sensitivity.zone_id", zone).is_err()
                    || (slot != &ten_id && slot != &five_id)
                    || self.id != format!("{slot}:comfort:{zone}")
                {
                    return Unsupported;
                }
                SingleRunComfort
            }
            _ => Unsupported,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergySensitivityAssessment {
    pub schema_version: String,
    pub declaration_sha256: String,
    pub all_declared_runs_integrity_checked: bool,
    pub numerical_sensitivity_passed: bool,
    pub missing_slots: Vec<String>,
    pub interruptions: Vec<EnergySensitivityInterruption>,
    pub runs: Vec<EnergySensitivityRun>,
    pub criteria: Vec<EnergySensitivityCriterion>,
    pub building_comfort_diagnostics: BTreeMap<String, ComfortSummary>,
    pub qualification_promoted: bool,
    pub limitations: Vec<String>,
}
fn validate_run(
    declaration: &EnergySensitivityDeclaration,
    run: &EnergySensitivityRun,
) -> Result<()> {
    text("sensitivity.run_id", &run.run_id)?;
    hash("sensitivity.record_sha256", &run.record_sha256)?;
    let slot = declaration
        .slots
        .iter()
        .find(|s| s.id == run.slot_id)
        .ok_or_else(|| invalid("sensitivity.run", "Run references an undeclared slot"))?;
    if run.physical_input_sha256 != slot.physical_input_sha256
        || run.execution_input_sha256 != slot.execution_input_sha256
        || run.method != slot.method
    {
        return Err(invalid(
            "sensitivity.run",
            "Exact input/engine/recipe/numerical identities do not match the declared slot",
        ));
    }
    let totals = &run.totals;
    if totals.interval_count != 8760 * usize::from(slot.zone_timesteps_per_hour)
        || totals.interval_seconds != 3600 / u32::from(slot.zone_timesteps_per_hour)
        || totals.demand_interval_seconds != 600
        || totals.monthly.len() != 12
        || interval_timestamp(totals.coincident_peak.timestamp.interval_index)?
            != totals.coincident_peak.timestamp
    {
        return Err(invalid(
            "sensitivity.shape",
            "Expected complete year, twelve months and canonical common-ten-minute peak",
        ));
    }
    range(
        "sensitivity.annual_kwh",
        totals.annual.facility_kwh,
        0.,
        f64::MAX,
    )?;
    range(
        "sensitivity.peak_kw",
        totals.coincident_peak.facility_kw,
        0.,
        f64::MAX,
    )?;
    let (model, _) = declaration.slot_inputs(&run.slot_id)?;
    let expected = model
        .zones
        .iter()
        .filter(|z| z.cooling_system.is_some())
        .map(|z| z.id.as_str())
        .collect::<BTreeSet<_>>();
    let actual = totals
        .zone_comfort
        .iter()
        .filter_map(|c| c.zone_id.as_deref())
        .collect::<BTreeSet<_>>();
    if expected != actual
        || totals.zone_comfort.len() != expected.len()
        || totals.building_comfort_union.zone_id.is_some()
    {
        return Err(invalid(
            "sensitivity.comfort",
            "Expected every conditioned zone exactly once and a separate building-union diagnostic",
        ));
    }
    for c in totals
        .zone_comfort
        .iter()
        .chain([&totals.building_comfort_union])
    {
        range("sensitivity.occupied_hours", c.occupied_hours, 0., 8760.)?;
        range(
            "sensitivity.unmet_hours",
            c.cooling_unmet_hours,
            0.,
            c.occupied_hours,
        )?;
        if c.occupied_hours == 0. {
            if c.unmet_percent.is_some() {
                return Err(invalid(
                    "sensitivity.comfort",
                    "Zero occupancy requires explicit absent percentage",
                ));
            }
        } else {
            let percent = c.unmet_percent.ok_or_else(|| {
                invalid("sensitivity.comfort", "Missing occupied-unmet percentage")
            })?;
            range("sensitivity.unmet_percent", percent, 0., 100.)?;
            if (percent - 100. * c.cooling_unmet_hours / c.occupied_hours).abs() > 1e-10 {
                return Err(invalid(
                    "sensitivity.comfort",
                    "Comfort percentage differs from retained hour counts",
                ));
            }
        }
    }
    Ok(())
}
fn relative_difference(ten: f64, five: f64) -> Option<f64> {
    if five == 0. {
        return if ten == 0. { Some(0.) } else { None };
    }
    let v = (ten - five).abs() / five.abs();
    v.is_finite().then_some(v)
}
pub fn assess_energy_sensitivity(
    declaration: &EnergySensitivityDeclaration,
    runs: &[EnergySensitivityRun],
    interruptions: &[EnergySensitivityInterruption],
) -> Result<EnergySensitivityAssessment> {
    declaration.validate()?;
    let mut by_slot = BTreeMap::new();
    let mut run_ids = BTreeSet::new();
    for run in runs {
        validate_run(declaration, run)?;
        if by_slot.insert(run.slot_id.as_str(), run).is_some() || !run_ids.insert(&run.run_id) {
            return Err(invalid(
                "sensitivity.runs",
                "Duplicate slot/run identity; favorable rerun selection is not supported",
            ));
        }
    }
    let mut interrupted = BTreeSet::new();
    for item in interruptions {
        if !declaration.slots.iter().any(|s| s.id == item.slot_id)
            || !interrupted.insert(&item.slot_id)
        {
            return Err(invalid(
                "sensitivity.interruptions",
                "Expected at most one retained interruption per declared slot",
            ));
        }
        hash("sensitivity.interruption_receipt", &item.receipt_sha256)?;
        text("sensitivity.interruption_reason", &item.reason)?;
    }
    let mut criteria = Vec::new();
    for target in &declaration.targets {
        let ten_id = format!("{}10", target.alternative_id);
        let five_id = format!("{}5", target.alternative_id);
        if let (Some(ten), Some(five)) =
            (by_slot.get(ten_id.as_str()), by_slot.get(five_id.as_str()))
        {
            for (quantity, a, b, limit) in [
                (
                    "annual_facility_kwh",
                    ten.totals.annual.facility_kwh,
                    five.totals.annual.facility_kwh,
                    1.,
                ),
                (
                    "common_ten_minute_peak_kw",
                    ten.totals.coincident_peak.facility_kw,
                    five.totals.coincident_peak.facility_kw,
                    3.,
                ),
            ] {
                let ratio = relative_difference(a, b);
                let measured = ratio.and_then(|v| (v * 100.).is_finite().then_some(v * 100.));
                criteria.push(EnergySensitivityCriterion{id:format!("{}:{quantity}",target.alternative_id),alternative_id:target.alternative_id.clone(),slot_ids:vec![ten_id.clone(),five_id.clone()],quantity:quantity.into(),zone_id:None,
                    ten_minute:Some(a),five_minute_reference:Some(b),measured_percent:measured,limit_percent:limit,passed:ratio.is_some_and(|p|p<=limit/100.),
                    explanation:if measured.is_some(){DENOMINATOR.into()}else{"Nonzero coarse result versus zero fine reference, or a non-representable relative ratio: criterion fails; no infinite JSON value is emitted.".into()}});
            }
        }
    }
    let mut building = BTreeMap::new();
    for slot in &declaration.slots {
        if let Some(run) = by_slot.get(slot.id.as_str()) {
            building.insert(slot.id.clone(), run.totals.building_comfort_union.clone());
            for c in &run.totals.zone_comfort {
                let zone = c.zone_id.clone().expect("validated conditioned zone");
                criteria.push(EnergySensitivityCriterion {
                    id: format!("{}:comfort:{zone}", slot.id),
                    alternative_id: slot.alternative_id.clone(),
                    slot_ids: vec![slot.id.clone()],
                    quantity: "occupied_cooling_unmet_percent".into(),
                    zone_id: Some(zone),
                    ten_minute: None,
                    five_minute_reference: None,
                    measured_percent: c.unmet_percent,
                    limit_percent: 1.,
                    passed: c.unmet_percent.is_some_and(|p| p <= 1.),
                    explanation: COMFORT.into(),
                });
            }
        }
    }
    let missing = declaration
        .slots
        .iter()
        .filter(|s| !by_slot.contains_key(s.id.as_str()))
        .map(|s| s.id.clone())
        .collect::<Vec<_>>();
    let complete = missing.is_empty() && interruptions.is_empty();
    Ok(EnergySensitivityAssessment {schema_version:"greenlens.energy-sensitivity-assessment.v1".into(),declaration_sha256:declaration.identity()?,
        all_declared_runs_integrity_checked:complete,numerical_sensitivity_passed:complete && criteria.iter().all(|c|c.passed),missing_slots:missing,interruptions:interruptions.to_vec(),
        runs:declaration.slots.iter().filter_map(|s|by_slot.get(s.id.as_str()).map(|r|(**r).clone())).collect(),criteria,building_comfort_diagnostics:building,qualification_promoted:false,
        limitations:vec!["Only the declared ten-/five-minute sensitivity predicates are evaluated. Passing does not establish calibrated accuracy, operating curve-domain coverage, service equivalence or certification.".into(),
            "No result from this assessment is added to the scientific registry. Historical failed/unassessed run qualification remains independently visible.".into(),
            "Every missing, failed or cancelled declared execution prevents a completed passing assessment; no automatic tuning or favorable retry is supported.".into()]})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn comfort_criterion() -> EnergySensitivityCriterion {
        EnergySensitivityCriterion {
            id: "A10:comfort:zone1".into(),
            alternative_id: "A".into(),
            slot_ids: vec!["A10".into()],
            quantity: "occupied_cooling_unmet_percent".into(),
            zone_id: Some("zone1".into()),
            ten_minute: None,
            five_minute_reference: None,
            measured_percent: Some(0.5),
            limit_percent: 1.,
            passed: true,
            explanation: "Original synthetic criterion.".into(),
        }
    }
    fn paired_criterion(quantity: &str) -> EnergySensitivityCriterion {
        EnergySensitivityCriterion {
            id: format!("B:{quantity}"),
            alternative_id: "B".into(),
            slot_ids: vec!["B10".into(), "B5".into()],
            quantity: quantity.into(),
            zone_id: None,
            ten_minute: Some(100.),
            five_minute_reference: Some(100.),
            measured_percent: Some(0.),
            limit_percent: if quantity == "annual_facility_kwh" {
                1.
            } else {
                3.
            },
            passed: true,
            explanation: "Original synthetic paired criterion.".into(),
        }
    }
    #[test]
    fn criterion_semantics_preserve_generated_assessment_and_legacy_encoding() {
        let (declaration, runs) = fixture();
        let assessment = assess_energy_sensitivity(&declaration, &runs, &[]).unwrap();
        let before = serde_json::to_vec(&assessment).unwrap();
        let measurements = assessment
            .criteria
            .iter()
            .map(|c| (c.measured_percent, c.limit_percent, c.passed))
            .collect::<Vec<_>>();
        for criterion in &assessment.criteria {
            let expected = match criterion.quantity.as_str() {
                "annual_facility_kwh" => EnergySensitivityCriterionSemantics::PairedAnnual,
                "common_ten_minute_peak_kw" => EnergySensitivityCriterionSemantics::PairedPeak,
                "occupied_cooling_unmet_percent" => {
                    EnergySensitivityCriterionSemantics::SingleRunComfort
                }
                _ => panic!("Unexpected generated criterion"),
            };
            assert_eq!(criterion.semantics_v1(), expected);
        }
        assert_eq!(serde_json::to_vec(&assessment).unwrap(), before);
        assert_eq!(
            assessment
                .criteria
                .iter()
                .map(|c| (c.measured_percent, c.limit_percent, c.passed))
                .collect::<Vec<_>>(),
            measurements
        );
        let criterion = comfort_criterion();
        let bytes = serde_json::to_string(&criterion).unwrap();
        assert_eq!(
            bytes,
            r#"{"id":"A10:comfort:zone1","alternative_id":"A","slot_ids":["A10"],"quantity":"occupied_cooling_unmet_percent","zone_id":"zone1","ten_minute":null,"five_minute_reference":null,"measured_percent":0.5,"limit_percent":1.0,"passed":true,"explanation":"Original synthetic criterion."}"#
        );
        let decoded: EnergySensitivityCriterion = serde_json::from_str(&bytes).unwrap();
        assert_eq!(decoded, criterion);
        assert_eq!(
            decoded.semantics_v1(),
            EnergySensitivityCriterionSemantics::SingleRunComfort
        );
    }
    #[test]
    fn paired_semantics_keep_absent_measurements_required() {
        for (quantity, semantics) in [
            (
                "annual_facility_kwh",
                EnergySensitivityCriterionSemantics::PairedAnnual,
            ),
            (
                "common_ten_minute_peak_kw",
                EnergySensitivityCriterionSemantics::PairedPeak,
            ),
        ] {
            for (ten, five) in [
                (Some(100.), Some(100.)),
                (None, Some(100.)),
                (Some(100.), None),
                (None, None),
            ] {
                let mut criterion = paired_criterion(quantity);
                criterion.ten_minute = ten;
                criterion.five_minute_reference = five;
                criterion.measured_percent = None;
                criterion.passed = false;
                assert_eq!(criterion.semantics_v1(), semantics);
            }
        }
    }
    #[test]
    fn criterion_semantics_require_exact_frozen_limits_without_mutating_data() {
        for (criterion, expected) in [
            (
                paired_criterion("annual_facility_kwh"),
                EnergySensitivityCriterionSemantics::PairedAnnual,
            ),
            (
                paired_criterion("common_ten_minute_peak_kw"),
                EnergySensitivityCriterionSemantics::PairedPeak,
            ),
            (
                comfort_criterion(),
                EnergySensitivityCriterionSemantics::SingleRunComfort,
            ),
        ] {
            assert_eq!(criterion.semantics_v1(), expected);
            let frozen = criterion.limit_percent;
            for limit in [
                frozen + 1.,
                -frozen,
                0.,
                f64::from_bits(frozen.to_bits() - 1),
                f64::from_bits(frozen.to_bits() + 1),
            ] {
                let mut altered = criterion.clone();
                altered.limit_percent = limit;
                let before = serde_json::to_vec(&altered).unwrap();
                assert_eq!(
                    altered.semantics_v1(),
                    EnergySensitivityCriterionSemantics::Unsupported
                );
                assert_eq!(serde_json::to_vec(&altered).unwrap(), before);
            }
        }
    }
    #[test]
    fn paired_semantics_reject_malformed_ids_zones_and_slot_order() {
        for quantity in ["annual_facility_kwh", "common_ten_minute_peak_kw"] {
            let criterion = paired_criterion(quantity);
            for slots in [
                vec![],
                vec!["B10"],
                vec!["B10", "B10"],
                vec!["B5", "B10"],
                vec!["B10", "A5"],
                vec!["B10", "B15"],
                vec!["B10", "B5", "B5"],
            ] {
                let mut altered = criterion.clone();
                altered.slot_ids = slots.into_iter().map(str::to_owned).collect();
                assert_eq!(
                    altered.semantics_v1(),
                    EnergySensitivityCriterionSemantics::Unsupported
                );
            }
            let mut altered = criterion.clone();
            altered.id = format!("A:{quantity}");
            assert_eq!(
                altered.semantics_v1(),
                EnergySensitivityCriterionSemantics::Unsupported
            );
            let mut altered = criterion.clone();
            altered.zone_id = Some("zone1".into());
            assert_eq!(
                altered.semantics_v1(),
                EnergySensitivityCriterionSemantics::Unsupported
            );
            let mut altered = criterion.clone();
            altered.alternative_id = "C".into();
            altered.id = format!("C:{quantity}");
            altered.slot_ids = vec!["C10".into(), "C5".into()];
            assert_eq!(
                altered.semantics_v1(),
                EnergySensitivityCriterionSemantics::Unsupported
            );
        }
    }
    #[test]
    fn single_run_comfort_requires_exact_zone_slot_and_absent_pair() {
        for alternative in ["A", "B"] {
            for interval in ["10", "5"] {
                let mut criterion = comfort_criterion();
                let slot = format!("{alternative}{interval}");
                criterion.alternative_id = alternative.into();
                criterion.slot_ids = vec![slot.clone()];
                criterion.id = format!("{slot}:comfort:zone1");
                criterion.measured_percent = None;
                criterion.passed = false;
                assert_eq!(
                    criterion.semantics_v1(),
                    EnergySensitivityCriterionSemantics::SingleRunComfort
                );
            }
        }
        let criterion = comfort_criterion();
        for slots in [
            vec![],
            vec!["A10", "A5"],
            vec!["A10", "A10"],
            vec!["B10"],
            vec!["A15"],
        ] {
            let mut altered = criterion.clone();
            altered.slot_ids = slots.into_iter().map(str::to_owned).collect();
            assert_eq!(
                altered.semantics_v1(),
                EnergySensitivityCriterionSemantics::Unsupported
            );
        }
        for zone in [None, Some(""), Some("zone:1"), Some("zone 1"), Some("区域")] {
            let mut altered = criterion.clone();
            altered.zone_id = zone.map(str::to_owned);
            altered.id = format!("A10:comfort:{}", zone.unwrap_or_default());
            assert_eq!(
                altered.semantics_v1(),
                EnergySensitivityCriterionSemantics::Unsupported
            );
        }
        let mut altered = criterion.clone();
        altered.zone_id = Some("z".repeat(81));
        altered.id = format!("A10:comfort:{}", "z".repeat(81));
        assert_eq!(
            altered.semantics_v1(),
            EnergySensitivityCriterionSemantics::Unsupported
        );
        let mut altered = criterion.clone();
        altered.id = "A10:comfort:zone2".into();
        assert_eq!(
            altered.semantics_v1(),
            EnergySensitivityCriterionSemantics::Unsupported
        );
        let mut altered = criterion.clone();
        altered.alternative_id = "B".into();
        assert_eq!(
            altered.semantics_v1(),
            EnergySensitivityCriterionSemantics::Unsupported
        );
        let mut altered = criterion.clone();
        altered.ten_minute = Some(0.);
        assert_eq!(
            altered.semantics_v1(),
            EnergySensitivityCriterionSemantics::Unsupported
        );
        let mut altered = criterion;
        altered.five_minute_reference = Some(0.);
        assert_eq!(
            altered.semantics_v1(),
            EnergySensitivityCriterionSemantics::Unsupported
        );
    }
    #[test]
    fn unknown_quantities_and_nonfinite_measurements_never_acquire_na_semantics() {
        for mut criterion in [comfort_criterion(), paired_criterion("annual_facility_kwh")] {
            criterion.quantity = "unknown_quantity".into();
            criterion.ten_minute = None;
            criterion.five_minute_reference = None;
            assert_eq!(
                criterion.semantics_v1(),
                EnergySensitivityCriterionSemantics::Unsupported
            );
        }
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut criterion = comfort_criterion();
            criterion.measured_percent = Some(value);
            assert_eq!(
                criterion.semantics_v1(),
                EnergySensitivityCriterionSemantics::Unsupported
            );
            for field in 0..4 {
                let mut criterion = paired_criterion("annual_facility_kwh");
                match field {
                    0 => criterion.ten_minute = Some(value),
                    1 => criterion.five_minute_reference = Some(value),
                    2 => criterion.measured_percent = Some(value),
                    _ => criterion.limit_percent = value,
                }
                assert_eq!(
                    criterion.semantics_v1(),
                    EnergySensitivityCriterionSemantics::Unsupported
                );
            }
        }
    }
    fn fixture() -> (EnergySensitivityDeclaration, Vec<EnergySensitivityRun>) {
        let r = synthetic_energy_report().unwrap();
        let ten = r.baseline.method.clone();
        let mut five = ten.clone();
        five.numerical_settings_sha256 = "b".repeat(64);
        let targets = [("A", &r.baseline), ("B", &r.proposed)]
            .into_iter()
            .map(|(id, c)| EnergySensitivityTarget {
                alternative_id: id.into(),
                model: c.model.clone(),
                study_10min: c.study.clone(),
            })
            .collect();
        let declaration = declare_energy_sensitivity(targets, ten, five).unwrap();
        let runs = declaration
            .slots
            .iter()
            .map(|s| {
                let c = if s.alternative_id == "A" {
                    &r.baseline
                } else {
                    &r.proposed
                };
                let mut totals = c.totals.clone();
                totals.interval_seconds = 3600 / u32::from(s.zone_timesteps_per_hour);
                totals.interval_count = 8760 * usize::from(s.zone_timesteps_per_hour);
                EnergySensitivityRun {
                    slot_id: s.id.clone(),
                    run_id: format!("synthetic_{}", s.id),
                    record_sha256: "c".repeat(64),
                    physical_input_sha256: s.physical_input_sha256.clone(),
                    execution_input_sha256: s.execution_input_sha256.clone(),
                    method: s.method.clone(),
                    totals,
                }
            })
            .collect();
        (declaration, runs)
    }
    #[test]
    fn fixed_slots_methods_and_policy_reject_scope_mutation() {
        let (d, r) = fixture();
        assert_eq!(
            d.slots.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            vec!["A10", "B10", "A5", "B5"]
        );
        let mut changed = d.clone();
        changed.common_peak_limit_percent = 3.1;
        assert!(changed.validate().is_err());
        let mut changed = d.clone();
        changed.slots.swap(0, 1);
        assert!(changed.validate().is_err());
        let mut changed = d.clone();
        changed.targets[1].model.zones[0]
            .cooling_system
            .as_mut()
            .unwrap()
            .gross_rated_cop += 0.1;
        assert!(changed.validate().is_err());
        let mut changed = r.clone();
        changed[0].method.recipe_sha256 = "f".repeat(64);
        assert!(assess_energy_sensitivity(&d, &changed, &[]).is_err());
        let mut changed = r.clone();
        changed.push(r[0].clone());
        assert!(assess_energy_sensitivity(&d, &changed, &[]).is_err());
    }
    #[test]
    fn exact_fine_denominator_all_criteria_and_failures_retained() {
        let (d, mut r) = fixture();
        for run in &mut r {
            run.totals.annual.facility_kwh = 100.;
            run.totals.coincident_peak.facility_kw = 100.;
        }
        r[0].totals.annual.facility_kwh = 101.;
        r[1].totals.coincident_peak.facility_kw = 103.;
        let pass = assess_energy_sensitivity(&d, &r, &[]).unwrap();
        assert!(pass.numerical_sensitivity_passed);
        assert_eq!(pass.criteria.len(), 20);
        assert!(!pass.qualification_promoted);
        r[1].totals.coincident_peak.facility_kw = 103.0001;
        let fail = assess_energy_sensitivity(&d, &r, &[]).unwrap();
        assert!(!fail.numerical_sensitivity_passed);
        assert_eq!(fail.criteria.iter().filter(|c| !c.passed).count(), 1);
        assert_eq!(fail.runs.len(), 4);
        let partial = assess_energy_sensitivity(&d, &r[..3], &[]).unwrap();
        assert_eq!(partial.missing_slots, vec!["B5"]);
        assert!(!partial.all_declared_runs_integrity_checked);
        let cancelled = assess_energy_sensitivity(
            &d,
            &r,
            &[EnergySensitivityInterruption {
                slot_id: "B5".into(),
                kind: EnergySensitivityInterruptionKind::Cancelled,
                receipt_sha256: "d".repeat(64),
                reason: "Explicit cancellation receipt".into(),
            }],
        )
        .unwrap();
        assert!(!cancelled.all_declared_runs_integrity_checked);
    }
    #[test]
    fn zero_reference_no_occupancy_and_legacy_bytes_are_explicit() {
        let (d, mut r) = fixture();
        let before = serde_json::to_vec(&r).unwrap();
        assess_energy_sensitivity(&d, &r, &[]).unwrap();
        assert_eq!(serde_json::to_vec(&r).unwrap(), before);
        r[0].totals.annual.facility_kwh = 1.;
        r[2].totals.annual.facility_kwh = 0.;
        r[0].totals.zone_comfort[0] = ComfortSummary {
            zone_id: r[0].totals.zone_comfort[0].zone_id.clone(),
            occupied_hours: 0.,
            cooling_unmet_hours: 0.,
            unmet_percent: None,
        };
        let a = assess_energy_sensitivity(&d, &r, &[]).unwrap();
        assert!(!a.numerical_sensitivity_passed);
        assert_eq!(a.criteria.iter().filter(|c| !c.passed).count(), 2);
        serde_json::to_vec(&a).unwrap();
        let legacy = synthetic_energy_report().unwrap();
        let bytes = serde_json::to_vec(&legacy).unwrap();
        legacy.validate().unwrap();
        assert_eq!(bytes, serde_json::to_vec(&legacy).unwrap());
    }
}
