//! One typed report dataset for HTML, PDF, tables and machine-readable output.
//! The runner supplies validated immutable evidence. Deserializing this DTO is
//! not an integrity check or authority to assign scientific qualification.
use super::{
    finite, hash, identifier, invalid, positive, savings, text, validate_thermal_model,
    CarbonFactor, EnergyEndUse, EnergyInputComparison, EnergyMethodIdentity, EnergyStudy,
    EnergyTotals, EnergyWeatherBinding, InputSource, Savings, ThermalModel, ENERGY_REPORT_SCHEMA,
};
use crate::{canonical_hash, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnergyUnit {
    Kwh,
    Kw,
    KwhPerM2,
    M2,
    Hours,
    Percent,
    KgCo2,
    KgCo2e,
    Celsius,
    Watts,
    M3PerSecond,
    Dimensionless,
}
impl EnergyUnit {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Kwh => "kWh",
            Self::Kw => "kW",
            Self::KwhPerM2 => "kWh/m²",
            Self::M2 => "m²",
            Self::Hours => "h",
            Self::Percent => "%",
            Self::KgCo2 => "kgCO2",
            Self::KgCo2e => "kgCO2e",
            Self::Celsius => "°C",
            Self::Watts => "W",
            Self::M3PerSecond => "m³/s",
            Self::Dimensionless => "1",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReportCell {
    Value {
        value: f64,
        unit: EnergyUnit,
        decimal_places: u8,
        display: String,
    },
    NotApplicable {
        reason: String,
    },
    Missing {
        reason: String,
    },
    Unsupported {
        reason: String,
    },
}
impl ReportCell {
    pub fn value(value: f64, unit: EnergyUnit, decimal_places: u8) -> Result<Self> {
        finite("report.value", value)?;
        if decimal_places > 6 {
            return Err(invalid(
                "report.decimal_places",
                "At most six presentation decimal places",
            ));
        }
        // Formatting never mutates the retained full-precision value.
        let display = format!(
            "{:.*}",
            usize::from(decimal_places),
            if value == 0. { 0. } else { value }
        );
        Ok(Self::Value {
            value,
            unit,
            decimal_places,
            display,
        })
    }
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Value {
                value,
                unit,
                decimal_places,
                display,
            } => {
                if Self::value(*value, *unit, *decimal_places)? != *self {
                    return Err(invalid(
                        "report.display",
                        format!(
                            "Display text {display:?} differs from authoritative Rust formatting"
                        ),
                    ));
                }
            }
            Self::NotApplicable { reason }
            | Self::Missing { reason }
            | Self::Unsupported { reason } => text("report.reason", reason)?,
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnergyExecutionStatus {
    Completed,
    Failed,
    Cancelled,
    NotExecuted,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnergyIntegrityScope {
    NotChecked,
    MetadataChecked,
    CompleteRetainedEvidenceChecked,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnergyInputRelation {
    Current,
    Changed,
    NotCompared,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnergyQualificationStatus {
    Unassessed,
    Failed,
    LimitedQualified,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyQualification {
    pub status: EnergyQualificationStatus,
    pub method: EnergyMethodIdentity,
    pub registry_version: String,
    pub reason: String,
    pub tested_scope: String,
    pub applicable_physical_input_sha256: Vec<String>,
    pub evidence_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyAssessment {
    pub execution: EnergyExecutionStatus,
    pub evidence_integrity: EnergyIntegrityScope,
    pub current_physical_inputs: EnergyInputRelation,
    pub current_processed_results: EnergyInputRelation,
    pub current_derived_results: EnergyInputRelation,
    pub scientific: EnergyQualification,
}
impl EnergyAssessment {
    pub fn validate(&self, physical: &str, method: &EnergyMethodIdentity) -> Result<()> {
        let q = &self.scientific;
        q.method.validate()?;
        text("qualification.registry_version", &q.registry_version)?;
        text("qualification.reason", &q.reason)?;
        text("qualification.scope", &q.tested_scope)?;
        if &q.method != method {
            return Err(invalid(
                "qualification.method",
                "Assessment must bind this exact full method/engine/recipe/numerical identity",
            ));
        }
        for h in &q.applicable_physical_input_sha256 {
            hash("qualification.scope_hash", h)?;
        }
        for id in &q.evidence_ids {
            text("qualification.evidence_id", id)?;
        }
        if q.status == EnergyQualificationStatus::LimitedQualified
            && (!q
                .applicable_physical_input_sha256
                .iter()
                .any(|h| h == physical)
                || q.evidence_ids.is_empty())
        {
            return Err(invalid("qualification.scope","Limited qualification requires this exact input scope and evidence; never infer it from another profile or daylight result"));
        }
        if q.status == EnergyQualificationStatus::Failed && q.evidence_ids.is_empty() {
            return Err(invalid(
                "qualification.evidence",
                "A failed scientific assessment requires identified evidence",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyEvidence {
    pub id: String,
    pub relative_path: String,
    pub sha256: String,
    pub caption: String,
    pub included: bool,
    pub rights: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyReportAsset {
    pub id: String,
    pub relative_path: String,
    pub sha256: String,
    pub caption: String,
    pub alt_text: String,
    pub model_input_sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyReportCase {
    pub id: String,
    pub label: String,
    pub model_id: String,
    /// Immutable inputs retained with this run, never the current editor model.
    pub model: ThermalModel,
    pub study: EnergyStudy,
    pub weather_summary: EnergyWeatherSummary,
    pub model_input_sha256: String,
    pub run_id: String,
    pub record_sha256: String,
    pub physical_input_sha256: String,
    pub processing_sha256: String,
    pub method: EnergyMethodIdentity,
    pub assessment: EnergyAssessment,
    pub gross_floor_area_m2: f64,
    pub conditioned_floor_area_m2: f64,
    pub totals: EnergyTotals,
    pub meter_ledger: super::MeterLedger,
    pub diagnostics: EnergyReportDiagnostics,
    pub carbon_factor: Option<CarbonFactor>,
    pub limitations: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyReportDiagnostics {
    pub source_record_sha256: String,
    pub messages: Vec<String>,
    /// None means unavailable, never zero warnings/severe errors.
    pub warning_count: Option<u32>,
    pub severe_count: Option<u32>,
    /// Source-bound diagnostic text, not an accuracy/qualification label.
    pub convergence_disclosure: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyWeatherSummary {
    pub binding: EnergyWeatherBinding,
    pub location: crate::weather::WeatherLocation,
    pub source: InputSource,
    pub year_hours: u16,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyComparisonRow {
    pub end_use: EnergyEndUse,
    pub baseline_kwh: f64,
    pub proposed_kwh: f64,
    pub energy_savings: Savings,
    pub baseline_coincident_kw: f64,
    pub proposed_coincident_kw: f64,
    pub demand_savings: Savings,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyComparison {
    pub input_relation: EnergyInputComparison,
    pub rows: Vec<EnergyComparisonRow>,
    pub facility_energy_savings: Savings,
    pub facility_peak_savings: Savings,
    pub lighting_service_equivalence: String,
    pub savings_claim_limitations: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyReportData {
    pub schema_version: String,
    pub id: String,
    pub title: String,
    pub revision: String,
    pub scope: String,
    pub baseline: EnergyReportCase,
    pub proposed: EnergyReportCase,
    pub comparison: EnergyComparison,
    pub assets: Vec<EnergyReportAsset>,
    pub evidence: Vec<EnergyEvidence>,
    pub limitations: Vec<String>,
}
fn relative_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.len() > 512
        || path.contains(['\\', ':'])
        || path.starts_with('/')
        || path
            .split('/')
            .any(|c| c.is_empty() || c == "." || c == "..")
    {
        return Err(invalid(
            "report.relative_path",
            "Expected a safe relative evidence path",
        ));
    }
    Ok(())
}
pub fn compare_energy_totals(
    a: &EnergyTotals,
    b: &EnergyTotals,
    input_relation: EnergyInputComparison,
    lighting_service_equivalence: String,
) -> Result<EnergyComparison> {
    text(
        "comparison.lighting_service_equivalence",
        &lighting_service_equivalence,
    )?;
    if a.annual
        .end_uses_kwh
        .keys()
        .ne(b.annual.end_uses_kwh.keys())
    {
        return Err(invalid(
            "comparison.meters",
            "End-use boundaries must match exactly",
        ));
    }
    let mut rows = Vec::new();
    for (&end_use, &baseline_kwh) in &a.annual.end_uses_kwh {
        let proposed_kwh = b.annual.end_uses_kwh[&end_use];
        let baseline_coincident_kw = *a
            .coincident_peak
            .end_use_contributions_kw
            .get(&end_use)
            .ok_or_else(|| invalid("comparison.peak", "Missing coincident contribution"))?;
        let proposed_coincident_kw = *b
            .coincident_peak
            .end_use_contributions_kw
            .get(&end_use)
            .ok_or_else(|| invalid("comparison.peak", "Missing coincident contribution"))?;
        rows.push(EnergyComparisonRow {
            end_use,
            baseline_kwh,
            proposed_kwh,
            energy_savings: savings(baseline_kwh, proposed_kwh)?,
            baseline_coincident_kw,
            proposed_coincident_kw,
            demand_savings: savings(baseline_coincident_kw, proposed_coincident_kw)?,
        });
    }
    let mut limitations = input_relation.service_warnings.clone();
    limitations.push("Lighting power differences do not establish equal illumination; the separately declared service evidence remains necessary.".into());
    Ok(EnergyComparison {
        input_relation,
        rows,
        facility_energy_savings: savings(a.annual.facility_kwh, b.annual.facility_kwh)?,
        facility_peak_savings: savings(
            a.coincident_peak.facility_kw,
            b.coincident_peak.facility_kw,
        )?,
        lighting_service_equivalence,
        savings_claim_limitations: limitations,
    })
}
impl EnergyReportCase {
    pub fn validate(&self) -> Result<()> {
        for (name, value) in [("case.id", &self.id), ("case.model_id", &self.model_id)] {
            identifier(name, value)?;
        }
        text("case.run_id", &self.run_id)?;
        text("case.label", &self.label)?;
        for h in [
            &self.model_input_sha256,
            &self.record_sha256,
            &self.physical_input_sha256,
            &self.processing_sha256,
        ] {
            hash("report.hash", h)?;
        }
        self.study.validate(&self.model)?;
        let validated = validate_thermal_model(&self.model, &self.study.calendar)?;
        if validated.model_input_sha256 != self.model_input_sha256
            || self.study.physical_input_hash(&self.model)? != self.physical_input_sha256
            || validated.gross_floor_area_m2 != self.gross_floor_area_m2
            || validated.conditioned_floor_area_m2 != self.conditioned_floor_area_m2
        {
            return Err(invalid("report.snapshot", "Model, study, physical identity and reported areas must describe the same retained inputs"));
        }
        let weather = &self.weather_summary;
        text("report.weather.source", &weather.source.citation)?;
        if weather.year_hours != 8760
            || weather.binding != self.study.weather
            || weather.location.latitude_deg_north != self.model.site.latitude_deg_north
            || weather.location.longitude_deg_east != self.model.site.longitude_deg_east
            || weather.location.elevation_m != self.model.site.elevation_m
            || weather.location.utc_offset_minutes != self.study.calendar.utc_offset_minutes
        {
            return Err(invalid(
                "report.weather",
                "Weather summary must bind the retained study, site and fixed calendar",
            ));
        }
        self.method.validate()?;
        self.meter_ledger.validate()?;
        if self
            .meter_ledger
            .entries
            .iter()
            .map(|e| e.end_use)
            .collect::<std::collections::BTreeSet<_>>()
            != self.totals.annual.end_uses_kwh.keys().copied().collect()
        {
            return Err(invalid(
                "report.meter_ledger",
                "Report component boundaries must match the retained meter ledger",
            ));
        }
        if self.diagnostics.source_record_sha256 != self.record_sha256
            || self.diagnostics.messages.len() > 256
        {
            return Err(invalid(
                "report.diagnostics",
                "Diagnostics must bind this retained record and remain bounded",
            ));
        }
        text(
            "report.diagnostics.convergence",
            &self.diagnostics.convergence_disclosure,
        )?;
        for message in &self.diagnostics.messages {
            text("report.diagnostics.message", message)?;
        }
        self.assessment
            .validate(&self.physical_input_sha256, &self.method)?;
        positive("report.gross_floor_area", self.gross_floor_area_m2)?;
        super::range(
            "report.conditioned_floor_area",
            self.conditioned_floor_area_m2,
            0.,
            self.gross_floor_area_m2,
        )?;
        if let Some(f) = &self.carbon_factor {
            f.validate()?;
        }
        if self.totals.monthly.len() != 12
            || !matches!(
                (self.totals.interval_count, self.totals.interval_seconds),
                (52_560, 600) | (105_120, 300)
            )
            || self.totals.demand_interval_seconds != 600
            || self.totals.interval_seconds != 3600 / u32::from(self.study.zone_timesteps_per_hour)
        {
            return Err(invalid(
                "report.time_axis",
                "Expected complete annual energy and aligned ten-minute demand",
            ));
        }
        // Full retained-series validation belongs to the runner. This crosscheck
        // prevents a separately hand-entered annual/monthly summary in the DTO.
        let mut total = super::Sum::default();
        for m in &self.totals.monthly {
            super::range("report.monthly_kwh", m.facility_kwh, 0., f64::MAX)?;
            total.add(m.facility_kwh);
        }
        let annual = self.totals.annual.facility_kwh;
        super::range("report.annual_kwh", annual, 0., f64::MAX)?;
        if (total.value() - annual).abs() > 1e-9_f64.max(annual.abs() * 1e-12) {
            return Err(invalid(
                "report.annual_monthly",
                "Monthly total differs from retained annual value",
            ));
        }
        let uses: std::collections::BTreeSet<_> =
            self.totals.annual.end_uses_kwh.keys().copied().collect();
        if uses.is_empty() || uses.len() > 6 {
            return Err(invalid(
                "report.end_uses",
                "Expected declared electrical end uses",
            ));
        }
        for p in std::iter::once(&self.totals.annual).chain(&self.totals.monthly) {
            if p.end_uses_kwh
                .keys()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                != uses
            {
                return Err(invalid(
                    "report.end_uses",
                    "Monthly and annual component boundaries differ",
                ));
            }
            let mut sum = super::Sum::default();
            for &v in p.end_uses_kwh.values() {
                super::range("report.component", v, 0., f64::MAX)?;
                sum.add(v);
            }
            finite("report.residual", p.reconciliation_residual_kwh)?;
            let residual = p.facility_kwh - sum.value();
            if !residual.is_finite()
                || (residual - p.reconciliation_residual_kwh).abs()
                    > 1e-9_f64.max(p.facility_kwh * 1e-12)
                || residual.abs() > 0.01_f64.max(p.facility_kwh * 0.0001)
            {
                return Err(invalid(
                    "report.reconciliation",
                    "Facility and components do not reconcile",
                ));
            }
        }
        for u in &uses {
            let mut sum = super::Sum::default();
            for m in &self.totals.monthly {
                sum.add(m.end_uses_kwh[u]);
            }
            let year = self.totals.annual.end_uses_kwh[u];
            if (sum.value() - year).abs() > 1e-9_f64.max(year * 1e-12) {
                return Err(invalid(
                    "report.component_monthly",
                    "Monthly end use does not sum to its annual value",
                ));
            }
        }
        let peak = &self.totals.coincident_peak;
        super::range("report.peak", peak.facility_kw, 0., f64::MAX)?;
        if peak.timestamp != super::interval_timestamp(peak.timestamp.interval_index)?
            || peak
                .end_use_contributions_kw
                .keys()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                != uses
        {
            return Err(invalid(
                "report.peak",
                "Peak timestamp or contribution identities differ",
            ));
        }
        let mut sum = super::Sum::default();
        for &v in peak.end_use_contributions_kw.values() {
            super::range("report.peak_component", v, 0., f64::MAX)?;
            sum.add(v);
        }
        finite("report.peak_residual", peak.reconciliation_residual_kw)?;
        let residual = peak.facility_kw - sum.value();
        if !residual.is_finite()
            || (residual - peak.reconciliation_residual_kw).abs()
                > 1e-9_f64.max(peak.facility_kw * 1e-12)
            || residual.abs() > 0.001_f64.max(peak.facility_kw * 0.0001)
        {
            return Err(invalid(
                "report.peak_reconciliation",
                "Coincident contributions do not reconcile",
            ));
        }
        super::range(
            "report.maximum_interval_residual",
            self.totals.maximum_interval_reconciliation_residual_kw,
            0.,
            f64::MAX,
        )?;
        let mut expected: std::collections::BTreeSet<_> = self
            .model
            .zones
            .iter()
            .filter(|z| z.cooling_system.is_some())
            .map(|z| z.id.as_str())
            .collect();
        for c in &self.totals.zone_comfort {
            if !c.zone_id.as_deref().is_some_and(|id| expected.remove(id)) {
                return Err(invalid(
                    "report.comfort",
                    "Each conditioned zone must appear exactly once",
                ));
            }
        }
        if !expected.is_empty() || self.totals.building_comfort_union.zone_id.is_some() {
            return Err(invalid(
                "report.comfort",
                "Missing zone or invalid union scope",
            ));
        }
        for c in self
            .totals
            .zone_comfort
            .iter()
            .chain(std::iter::once(&self.totals.building_comfort_union))
        {
            super::range("report.occupied_hours", c.occupied_hours, 0., 8760.)?;
            super::range(
                "report.unmet_hours",
                c.cooling_unmet_hours,
                0.,
                c.occupied_hours,
            )?;
            let expected = if c.occupied_hours == 0. {
                None
            } else {
                Some(100. * c.cooling_unmet_hours / c.occupied_hours)
            };
            match (expected, c.unmet_percent) {
                (None, None) => (),
                (Some(a), Some(b)) if b.is_finite() && (a - b).abs() <= 1e-9 => (),
                _ => {
                    return Err(invalid(
                        "report.unmet_percent",
                        "Percentage must use the occupied denominator; no occupancy is undefined",
                    ))
                }
            }
        }
        text("report.comfort_definition", &self.totals.comfort_definition)?;
        for l in &self.limitations {
            text("report.case_limitation", l)?;
        }
        Ok(())
    }
    pub fn eui_gross_kwh_m2(&self) -> Result<f64> {
        self.validate()?;
        let x = self.totals.annual.facility_kwh / self.gross_floor_area_m2;
        finite("report.eui", x)?;
        Ok(x)
    }
    pub fn emissions_cell(&self) -> Result<ReportCell> {
        match &self.carbon_factor {
            Some(f) => ReportCell::value(
                f.emissions(self.totals.annual.facility_kwh)?,
                match f.unit {
                    super::EmissionUnit::KgCo2 => EnergyUnit::KgCo2,
                    super::EmissionUnit::KgCo2e => EnergyUnit::KgCo2e,
                },
                2,
            ),
            None => Ok(ReportCell::Missing {
                reason: "No source-bound carbon factor; energy results remain available.".into(),
            }),
        }
    }
}
impl EnergyReportData {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != ENERGY_REPORT_SCHEMA {
            return Err(invalid(
                "energy_report.schema_version",
                "Unsupported report version",
            ));
        }
        identifier("report.id", &self.id)?;
        for (name, value) in [
            ("report.title", &self.title),
            ("report.revision", &self.revision),
            ("report.scope", &self.scope),
        ] {
            text(name, value)?;
        }
        self.baseline.validate()?;
        self.proposed.validate()?;
        if self.baseline.id == self.proposed.id {
            return Err(invalid(
                "report.cases",
                "Baseline and proposed case IDs must differ",
            ));
        }
        let input_relation = super::compare_energy_inputs(
            &self.baseline.model,
            &self.baseline.study,
            &self.proposed.model,
            &self.proposed.study,
        )?;
        let recomputed = compare_energy_totals(
            &self.baseline.totals,
            &self.proposed.totals,
            input_relation,
            self.comparison.lighting_service_equivalence.clone(),
        )?;
        if recomputed != self.comparison {
            return Err(invalid("report.comparison","Comparison must be derived from the same case totals; hand-entered divergent values are rejected"));
        }
        let mut seen = std::collections::BTreeSet::new();
        for a in &self.assets {
            identifier("asset.id", &a.id)?;
            if !seen.insert(&a.id) {
                return Err(invalid("assets.id", "Duplicate asset"));
            }
            relative_path(&a.relative_path)?;
            hash("asset.sha256", &a.sha256)?;
            hash("asset.model_input_sha256", &a.model_input_sha256)?;
            if a.model_input_sha256 != self.baseline.model_input_sha256
                && a.model_input_sha256 != self.proposed.model_input_sha256
            {
                return Err(invalid("asset.model_input_sha256", "Illustration must bind a retained report model, not another or current mutable model"));
            }
            text("asset.caption", &a.caption)?;
            text("asset.alt_text", &a.alt_text)?;
        }
        for e in &self.evidence {
            identifier("evidence.id", &e.id)?;
            if !seen.insert(&e.id) {
                return Err(invalid("evidence.id", "Duplicate evidence ID"));
            }
            relative_path(&e.relative_path)?;
            hash("evidence.sha256", &e.sha256)?;
            text("evidence.caption", &e.caption)?;
            text("evidence.rights", &e.rights)?;
        }
        for l in &self.limitations {
            text("report.limitation", l)?;
        }
        Ok(())
    }
    pub fn dataset_sha256(&self) -> Result<String> {
        self.validate()?;
        canonical_hash(self)
    }
}

/// A deterministic view of the validated dataset. Renderers print these cells;
/// they do not divide, round, sum or recompute savings themselves.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyReportPresentation {
    pub baseline: EnergyCasePresentation,
    pub proposed: EnergyCasePresentation,
    pub end_uses: Vec<EnergyComparisonPresentation>,
    pub facility_energy_savings: ReportSavings,
    pub facility_peak_savings: ReportSavings,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyCasePresentation {
    pub case_id: String,
    pub annual: EnergyPeriodPresentation,
    pub monthly: Vec<EnergyPeriodPresentation>,
    pub peak_kw: ReportCell,
    pub peak_end_uses_kw: std::collections::BTreeMap<EnergyEndUse, ReportCell>,
    pub peak_timestamp: super::EnergyTimestamp,
    pub peak_reconciliation_residual: ReportCell,
    pub maximum_interval_reconciliation_residual: ReportCell,
    pub gross_floor_area: ReportCell,
    pub conditioned_floor_area: ReportCell,
    pub gross_eui: ReportCell,
    pub conditioned_eui: ReportCell,
    pub emissions: ReportCell,
    pub comfort: Vec<EnergyComfortPresentation>,
    pub building_comfort_union: EnergyComfortPresentation,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyPeriodPresentation {
    pub facility: ReportCell,
    pub end_uses: std::collections::BTreeMap<EnergyEndUse, ReportCell>,
    pub reconciliation_residual: ReportCell,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyComfortPresentation {
    pub zone_id: Option<String>,
    pub occupied_hours: ReportCell,
    pub cooling_unmet_hours: ReportCell,
    pub unmet_percent: ReportCell,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReportSavings {
    pub absolute: ReportCell,
    pub percent: ReportCell,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyComparisonPresentation {
    pub end_use: EnergyEndUse,
    pub baseline_kwh: ReportCell,
    pub proposed_kwh: ReportCell,
    pub energy_savings: ReportSavings,
    pub baseline_coincident_kw: ReportCell,
    pub proposed_coincident_kw: ReportCell,
    pub demand_savings: ReportSavings,
}
fn optional_percent(value: Option<f64>, reason: &str) -> Result<ReportCell> {
    match value {
        Some(v) => ReportCell::value(v, EnergyUnit::Percent, 2),
        None => Ok(ReportCell::NotApplicable {
            reason: reason.into(),
        }),
    }
}
fn savings_cells(s: &Savings, unit: EnergyUnit) -> Result<ReportSavings> {
    Ok(ReportSavings {
        absolute: ReportCell::value(s.absolute, unit, 2)?,
        percent: optional_percent(
            s.percent,
            "Baseline is zero; percentage savings are undefined.",
        )?,
    })
}
fn period_cells(p: &super::EnergyPeriodTotal) -> Result<EnergyPeriodPresentation> {
    Ok(EnergyPeriodPresentation {
        facility: ReportCell::value(p.facility_kwh, EnergyUnit::Kwh, 2)?,
        end_uses: p
            .end_uses_kwh
            .iter()
            .map(|(&k, &v)| Ok((k, ReportCell::value(v, EnergyUnit::Kwh, 2)?)))
            .collect::<Result<_>>()?,
        reconciliation_residual: ReportCell::value(
            p.reconciliation_residual_kwh,
            EnergyUnit::Kwh,
            6,
        )?,
    })
}
fn comfort_cells(c: &super::ComfortSummary) -> Result<EnergyComfortPresentation> {
    Ok(EnergyComfortPresentation {
        zone_id: c.zone_id.clone(),
        occupied_hours: ReportCell::value(c.occupied_hours, EnergyUnit::Hours, 2)?,
        cooling_unmet_hours: ReportCell::value(c.cooling_unmet_hours, EnergyUnit::Hours, 2)?,
        unmet_percent: optional_percent(
            c.unmet_percent,
            "No occupied intervals; percentage unmet hours are undefined.",
        )?,
    })
}
impl EnergyReportCase {
    pub fn presentation(&self) -> Result<EnergyCasePresentation> {
        self.validate()?;
        Ok(EnergyCasePresentation {
            case_id: self.id.clone(),
            annual: period_cells(&self.totals.annual)?,
            monthly: self
                .totals
                .monthly
                .iter()
                .map(period_cells)
                .collect::<Result<_>>()?,
            peak_kw: ReportCell::value(self.totals.coincident_peak.facility_kw, EnergyUnit::Kw, 3)?,
            peak_end_uses_kw: self
                .totals
                .coincident_peak
                .end_use_contributions_kw
                .iter()
                .map(|(&k, &v)| Ok((k, ReportCell::value(v, EnergyUnit::Kw, 3)?)))
                .collect::<Result<_>>()?,
            peak_timestamp: self.totals.coincident_peak.timestamp,
            peak_reconciliation_residual: ReportCell::value(
                self.totals.coincident_peak.reconciliation_residual_kw,
                EnergyUnit::Kw,
                6,
            )?,
            maximum_interval_reconciliation_residual: ReportCell::value(
                self.totals.maximum_interval_reconciliation_residual_kw,
                EnergyUnit::Kw,
                6,
            )?,
            gross_floor_area: ReportCell::value(self.gross_floor_area_m2, EnergyUnit::M2, 2)?,
            conditioned_floor_area: ReportCell::value(
                self.conditioned_floor_area_m2,
                EnergyUnit::M2,
                2,
            )?,
            gross_eui: ReportCell::value(
                self.totals.annual.facility_kwh / self.gross_floor_area_m2,
                EnergyUnit::KwhPerM2,
                2,
            )?,
            conditioned_eui: if self.conditioned_floor_area_m2 == 0. {
                ReportCell::NotApplicable {
                    reason: "No conditioned floor area.".into(),
                }
            } else {
                ReportCell::value(
                    self.totals.annual.facility_kwh / self.conditioned_floor_area_m2,
                    EnergyUnit::KwhPerM2,
                    2,
                )?
            },
            emissions: self.emissions_cell()?,
            comfort: self
                .totals
                .zone_comfort
                .iter()
                .map(comfort_cells)
                .collect::<Result<_>>()?,
            building_comfort_union: comfort_cells(&self.totals.building_comfort_union)?,
        })
    }
}
impl EnergyReportData {
    pub fn presentation(&self) -> Result<EnergyReportPresentation> {
        self.validate()?;
        Ok(EnergyReportPresentation {
            baseline: self.baseline.presentation()?,
            proposed: self.proposed.presentation()?,
            end_uses: self
                .comparison
                .rows
                .iter()
                .map(|r| {
                    Ok(EnergyComparisonPresentation {
                        end_use: r.end_use,
                        baseline_kwh: ReportCell::value(r.baseline_kwh, EnergyUnit::Kwh, 2)?,
                        proposed_kwh: ReportCell::value(r.proposed_kwh, EnergyUnit::Kwh, 2)?,
                        energy_savings: savings_cells(&r.energy_savings, EnergyUnit::Kwh)?,
                        baseline_coincident_kw: ReportCell::value(
                            r.baseline_coincident_kw,
                            EnergyUnit::Kw,
                            3,
                        )?,
                        proposed_coincident_kw: ReportCell::value(
                            r.proposed_coincident_kw,
                            EnergyUnit::Kw,
                            3,
                        )?,
                        demand_savings: savings_cells(&r.demand_savings, EnergyUnit::Kw)?,
                    })
                })
                .collect::<Result<_>>()?,
            facility_energy_savings: savings_cells(
                &self.comparison.facility_energy_savings,
                EnergyUnit::Kwh,
            )?,
            facility_peak_savings: savings_cells(
                &self.comparison.facility_peak_savings,
                EnergyUnit::Kw,
            )?,
        })
    }
}
