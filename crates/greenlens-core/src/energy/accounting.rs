//! Unit conversion and streaming accounting over the declared weather-run axis.
//! Native channel selection remains the separately versioned adapter's job.
use super::{
    finite, identifier, interval_timestamp, invalid, range, text, EnergyTimestamp, Sum,
    ENERGY_INTERVALS,
};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const JOULES_PER_KWH: f64 = 3_600_000.;
pub const COMFORT_COOLING_TOLERANCE_C: f64 = 0.2;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EnergyEndUse {
    CoolingSystem,
    SupplyFans,
    VentilationExhaustFans,
    InteriorLighting,
    ExteriorLighting,
    PlugEquipment,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Fuel {
    Electricity,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessClassification {
    Process,
    NonProcess,
    Unclassified,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MeterLedgerEntry {
    pub end_use: EnergyEndUse,
    pub fuel: Fuel,
    pub native_channels: Vec<String>,
    pub boundary: String,
    pub classification: ProcessClassification,
    pub classification_source: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MeterLedger {
    pub version: String,
    pub facility_native_channel: String,
    pub entries: Vec<MeterLedgerEntry>,
}
impl MeterLedger {
    pub fn validate(&self) -> Result<()> {
        text("meter.version", &self.version)?;
        text(
            "meter.facility_native_channel",
            &self.facility_native_channel,
        )?;
        if self.entries.is_empty() || self.entries.len() > 6 {
            return Err(invalid(
                "meter.entries",
                "Expected 1–6 disjoint electrical end uses",
            ));
        }
        let mut uses = BTreeSet::new();
        let mut channels = BTreeSet::from([self.facility_native_channel.as_str()]);
        for entry in &self.entries {
            if !uses.insert(entry.end_use) {
                return Err(invalid("meter.entries", "Duplicate end use"));
            }
            text("meter.boundary", &entry.boundary)?;
            if entry.native_channels.is_empty() || entry.native_channels.len() > 64 {
                return Err(invalid(
                    "meter.native_channels",
                    "Expected 1–64 explicitly named component channels",
                ));
            }
            for channel in &entry.native_channels {
                text("meter.native_channel", channel)?;
                if !channels.insert(channel) {
                    return Err(invalid(
                        "meter.native_channels",
                        "Facility, aggregate or component channel is counted more than once",
                    ));
                }
            }
            match (&entry.classification, &entry.classification_source) {
                (ProcessClassification::Unclassified, None) => (),
                (_, Some(source)) => text("meter.classification_source", source)?,
                _ => {
                    return Err(invalid(
                        "meter.classification_source",
                        "A process/non-process classification requires an explicit source",
                    ))
                }
            }
        }
        Ok(())
    }
}

pub fn joules_to_kwh(joules: f64) -> Result<f64> {
    range("energy.joules", joules, 0., f64::MAX)?;
    Ok(joules / JOULES_PER_KWH)
}
pub fn interval_demand_kw(joules: f64, seconds: u32) -> Result<f64> {
    if seconds == 0 {
        return Err(invalid(
            "energy.interval_seconds",
            "Expected a positive interval duration",
        ));
    }
    range("energy.joules", joules, 0., f64::MAX)?;
    Ok(joules / f64::from(seconds) / 1000.)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Savings {
    pub absolute: f64,
    pub percent: Option<f64>,
}
pub fn savings(baseline: f64, proposed: f64) -> Result<Savings> {
    range("comparison.baseline", baseline, 0., f64::MAX)?;
    range("comparison.proposed", proposed, 0., f64::MAX)?;
    let absolute = baseline - proposed;
    let percent = if baseline == 0. {
        None
    } else {
        Some((absolute / baseline) * 100.)
    };
    finite("comparison.absolute", absolute)?;
    if let Some(v) = percent {
        finite("comparison.percent", v)?;
    }
    Ok(Savings { absolute, percent })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EmissionUnit {
    KgCo2,
    KgCo2e,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CarbonFactor {
    pub id: String,
    pub fuel: Fuel,
    pub region: String,
    pub year: u16,
    pub kg_per_kwh: f64,
    pub unit: EmissionUnit,
    pub source: String,
    pub version: String,
    pub synthetic: bool,
}
impl CarbonFactor {
    pub fn validate(&self) -> Result<()> {
        identifier("carbon.id", &self.id)?;
        text("carbon.region", &self.region)?;
        text("carbon.source", &self.source)?;
        text("carbon.version", &self.version)?;
        if !(1900..=9999).contains(&self.year) {
            return Err(invalid(
                "carbon.year",
                "Expected explicit four-digit factor year",
            ));
        }
        range("carbon.kg_per_kwh", self.kg_per_kwh, 0., f64::MAX)
    }
    pub fn emissions(&self, kwh: f64) -> Result<f64> {
        self.validate()?;
        range("carbon.energy_kwh", kwh, 0., f64::MAX)?;
        let v = kwh * self.kg_per_kwh;
        finite("carbon.emissions", v)?;
        Ok(v)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ComfortSample {
    pub zone_id: String,
    pub occupied: bool,
    pub air_temperature_c: f64,
    pub cooling_setpoint_c: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyInterval {
    pub index: usize,
    pub facility_j: f64,
    pub end_uses_j: BTreeMap<EnergyEndUse, f64>,
    pub comfort: Vec<ComfortSample>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyPeriodTotal {
    pub facility_kwh: f64,
    pub end_uses_kwh: BTreeMap<EnergyEndUse, f64>,
    pub reconciliation_residual_kwh: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CoincidentDemand {
    pub timestamp: EnergyTimestamp,
    pub facility_kw: f64,
    pub end_use_contributions_kw: BTreeMap<EnergyEndUse, f64>,
    pub reconciliation_residual_kw: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ComfortSummary {
    pub zone_id: Option<String>,
    pub occupied_hours: f64,
    pub cooling_unmet_hours: f64,
    pub unmet_percent: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyTotals {
    pub interval_count: usize,
    pub interval_seconds: u32,
    pub annual: EnergyPeriodTotal,
    pub monthly: Vec<EnergyPeriodTotal>,
    pub demand_interval_seconds: u32,
    pub coincident_peak: CoincidentDemand,
    pub zone_comfort: Vec<ComfortSummary>,
    pub building_comfort_union: ComfortSummary,
    pub comfort_definition: String,
    pub maximum_interval_reconciliation_residual_kw: f64,
}

#[derive(Default, Clone)]
struct Period {
    facility: Sum,
    uses: BTreeMap<EnergyEndUse, Sum>,
}
impl Period {
    fn push(&mut self, facility: f64, uses: &BTreeMap<EnergyEndUse, f64>) {
        self.facility.add(facility);
        for (key, &value) in uses {
            self.uses.entry(*key).or_default().add(value);
        }
    }
    fn finish(&self) -> Result<EnergyPeriodTotal> {
        let facility_kwh = joules_to_kwh(self.facility.value())?;
        let end_uses_kwh = self
            .uses
            .iter()
            .map(|(&k, v)| Ok((k, joules_to_kwh(v.value())?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let mut end_sum = Sum::default();
        for &v in end_uses_kwh.values() {
            end_sum.add(v);
        }
        let residual = facility_kwh - end_sum.value();
        finite("energy.residual_kwh", residual)?;
        Ok(EnergyPeriodTotal {
            facility_kwh,
            end_uses_kwh,
            reconciliation_residual_kwh: residual,
        })
    }
    fn checked_with(&self, facility: f64, uses: &BTreeMap<EnergyEndUse, f64>) -> Result<Self> {
        let mut next = self.clone();
        next.push(facility, uses);
        finite("energy.accumulated_j", next.facility.value())?;
        for value in next.uses.values() {
            finite("energy.accumulated_component_j", value.value())?;
        }
        Ok(next)
    }
}
/// Bounded accumulator: only monthly totals, one peak and per-zone counters are
/// kept. Caller retains the validated 10-minute data in bounded immutable chunks.
pub struct EnergyAccumulator {
    count: usize,
    uses: BTreeSet<EnergyEndUse>,
    annual: Period,
    months: [Period; 12],
    peak: Option<CoincidentDemand>,
    comfort: BTreeMap<String, (usize, usize)>,
    building: (usize, usize),
    max_residual_kw: f64,
    interval_seconds: u32,
    pending_demand: Option<(f64, BTreeMap<EnergyEndUse, f64>)>,
}
impl EnergyAccumulator {
    pub fn new(ledger: &MeterLedger, conditioned_zone_ids: &[String]) -> Result<Self> {
        Self::with_interval_seconds(ledger, conditioned_zone_ids, 600)
    }
    pub fn with_interval_seconds(
        ledger: &MeterLedger,
        conditioned_zone_ids: &[String],
        interval_seconds: u32,
    ) -> Result<Self> {
        if !matches!(interval_seconds, 300 | 600) {
            return Err(invalid(
                "energy.interval_seconds",
                "Only 300 or 600 seconds are supported; demand is always aggregated to 600 seconds",
            ));
        }
        ledger.validate()?;
        if conditioned_zone_ids.len() > super::MAX_THERMAL_ZONES {
            return Err(invalid("comfort.zones", "At most eight zones"));
        }
        let mut comfort = BTreeMap::new();
        for id in conditioned_zone_ids {
            identifier("comfort.zone_id", id)?;
            if comfort.insert(id.clone(), (0, 0)).is_some() {
                return Err(invalid("comfort.zone_id", "Duplicate zone"));
            }
        }
        Ok(Self {
            count: 0,
            uses: ledger.entries.iter().map(|e| e.end_use).collect(),
            annual: Period::default(),
            months: std::array::from_fn(|_| Period::default()),
            peak: None,
            comfort,
            building: (0, 0),
            max_residual_kw: 0.,
            interval_seconds,
            pending_demand: None,
        })
    }
    pub fn push(&mut self, row: &EnergyInterval) -> Result<()> {
        let factor = 600 / self.interval_seconds;
        if row.index != self.count || self.count >= ENERGY_INTERVALS * factor as usize {
            return Err(invalid(
                "energy.index",
                "Expected every weather interval exactly once in order",
            ));
        }
        if row.end_uses_j.keys().copied().collect::<BTreeSet<_>>() != self.uses {
            return Err(invalid(
                "energy.end_uses",
                "Missing, extra or differently named meter components",
            ));
        }
        range("energy.facility_j", row.facility_j, 0., f64::MAX)?;
        let mut sum = Sum::default();
        for &value in row.end_uses_j.values() {
            range("energy.component_j", value, 0., f64::MAX)?;
            sum.add(value);
        }
        let facility_kw = interval_demand_kw(row.facility_j, self.interval_seconds)?;
        let residual_kw =
            (row.facility_j - sum.value()) / (f64::from(self.interval_seconds) * 1000.);
        finite("energy.residual_kw", residual_kw)?;
        if residual_kw.abs() > 0.001_f64.max(0.0001 * facility_kw) {
            return Err(invalid(
                "energy.reconciliation",
                "Facility/component interval demand does not reconcile within max(0.001 kW, 0.01%)",
            ));
        }
        let mut seen = BTreeSet::new();
        let mut occupied_any = false;
        let mut unmet_any = false;
        let mut changes = Vec::new();
        for item in &row.comfort {
            if !self.comfort.contains_key(&item.zone_id) || !seen.insert(&item.zone_id) {
                return Err(invalid(
                    "comfort.zone_id",
                    "Expected each conditioned zone exactly once",
                ));
            }
            range(
                "comfort.air_temperature_c",
                item.air_temperature_c,
                -100.,
                200.,
            )?;
            range(
                "comfort.cooling_setpoint_c",
                item.cooling_setpoint_c,
                -50.,
                100.,
            )?;
            let unmet = item.occupied
                && item.air_temperature_c > item.cooling_setpoint_c + COMFORT_COOLING_TOLERANCE_C;
            occupied_any |= item.occupied;
            unmet_any |= unmet;
            changes.push((&item.zone_id, item.occupied, unmet));
        }
        if seen.len() != self.comfort.len() {
            return Err(invalid(
                "comfort.zones",
                "Missing conditioned-zone diagnostic",
            ));
        }
        let timestamp = interval_timestamp(row.index / factor as usize)?;
        let next_annual = self.annual.checked_with(row.facility_j, &row.end_uses_j)?;
        let month = usize::from(timestamp.month - 1);
        let next_month = self.months[month].checked_with(row.facility_j, &row.end_uses_j)?;
        let demand = if self.interval_seconds == 600 {
            Some((row.facility_j, row.end_uses_j.clone()))
        } else if let Some((previous, components)) = &self.pending_demand {
            let mut components = components.clone();
            for (k, v) in &row.end_uses_j {
                let value = components.get_mut(k).expect("same ledger");
                *value += v;
                finite("energy.paired_demand_component", *value)?;
            }
            let total = previous + row.facility_j;
            finite("energy.paired_demand", total)?;
            Some((total, components))
        } else {
            None
        };
        // No accumulator mutation occurs before all row checks succeed.
        for (id, occupied, unmet) in changes {
            let v = self.comfort.get_mut(id).expect("checked zone");
            v.0 += usize::from(occupied);
            v.1 += usize::from(unmet);
        }
        self.building.0 += usize::from(occupied_any);
        self.building.1 += usize::from(unmet_any);
        self.annual = next_annual;
        self.months[month] = next_month;
        self.pending_demand = if self.interval_seconds == 300 && demand.is_none() {
            Some((row.facility_j, row.end_uses_j.clone()))
        } else {
            None
        };
        if let Some((facility_j, components)) = demand {
            let kw = facility_j / 600_000.;
            let mut sum = Sum::default();
            for &v in components.values() {
                sum.add(v);
            }
            if self.peak.as_ref().is_none_or(|p| kw > p.facility_kw) {
                self.peak = Some(CoincidentDemand {
                    timestamp,
                    facility_kw: kw,
                    end_use_contributions_kw: components
                        .iter()
                        .map(|(&k, &v)| (k, v / 600_000.))
                        .collect(),
                    reconciliation_residual_kw: (facility_j - sum.value()) / 600_000.,
                });
            }
        }
        self.max_residual_kw = self.max_residual_kw.max(residual_kw.abs());
        self.count += 1;
        Ok(())
    }
    pub fn finish(self) -> Result<EnergyTotals> {
        if self.count != ENERGY_INTERVALS * (600 / self.interval_seconds) as usize
            || self.pending_demand.is_some()
        {
            return Err(invalid("energy.intervals","A completed annual result requires all 52,560 ten-minute or 105,120 five-minute intervals"));
        }
        let annual = self.annual.finish()?;
        if annual.reconciliation_residual_kwh.abs() > 0.01_f64.max(0.0001 * annual.facility_kwh) {
            return Err(invalid(
                "energy.reconciliation",
                "Annual facility/component energy does not reconcile within max(0.01 kWh, 0.01%)",
            ));
        }
        let summary = |zone_id, counts: (usize, usize)| ComfortSummary {
            zone_id,
            occupied_hours: counts.0 as f64 * f64::from(self.interval_seconds) / 3600.,
            cooling_unmet_hours: counts.1 as f64 * f64::from(self.interval_seconds) / 3600.,
            unmet_percent: if counts.0 == 0 {
                None
            } else {
                Some(100. * counts.1 as f64 / counts.0 as f64)
            },
        };
        Ok(EnergyTotals{interval_count:self.count,interval_seconds:self.interval_seconds,demand_interval_seconds:600,annual,monthly:self.months.iter().map(Period::finish).collect::<Result<_>>()?,coincident_peak:self.peak.expect("complete year"),zone_comfort:self.comfort.into_iter().map(|(id,v)|summary(Some(id),v)).collect(),building_comfort_union:summary(None,self.building),comfort_definition:"Occupied when the declared occupancy is nonzero; air temperature > cooling setpoint + 0.2 C. Comfort uses source intervals without averaging excursions; building numerator and denominator are interval unions, not sums of zones.".into(),maximum_interval_reconciliation_residual_kw:self.max_residual_kw})
    }
}
