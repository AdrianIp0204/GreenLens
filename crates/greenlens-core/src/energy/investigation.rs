//! Additive navigation of retained results. All ranges use the unchanged
//! non-leap, local-standard ten-minute axis; no values are downsampled.
use super::*;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const ENERGY_INVESTIGATION_SCHEMA: &str = "greenlens.energy-investigation.v1";
/// Seven days on the common axis: at most 2016 five-minute source rows.
pub const MAX_INVESTIGATION_INTERVALS: usize = 1008;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnergyLocalTime {
    pub date: MonthDay,
    /// Exact local-standard minute, aligned to ten minutes. 1440 is accepted
    /// only as an exclusive range endpoint, never as an interval start.
    pub minute: u16,
}
impl EnergyLocalTime {
    pub fn interval_index(self, exclusive_endpoint: bool) -> Result<usize> {
        let day = self.date.day_index()?;
        if self.minute > 1440
            || !self.minute.is_multiple_of(10)
            || (!exclusive_endpoint && self.minute == 1440)
        {
            return Err(invalid(
                "investigation.time",
                "Use a ten-minute local-standard boundary; 24:00 is only an exclusive range end",
            ));
        }
        Ok(day * STEPS_PER_DAY + usize::from(self.minute / 10))
    }
    pub fn shift_day(self, days: i16) -> Result<Self> {
        self.interval_index(false)?;
        let day = self.date.day_index()? as i32 + i32::from(days);
        if !(0..DAYS_PER_YEAR as i32).contains(&day) {
            return Err(invalid(
                "investigation.date",
                "Day navigation does not wrap across the retained non-leap year",
            ));
        }
        Ok(Self {
            date: MonthDay::from_day_index(day as usize)?,
            minute: self.minute,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EnergyNavigationRequest {
    At {
        time: EnergyLocalTime,
    },
    Day {
        date: MonthDay,
    },
    Range {
        start: EnergyLocalTime,
        end: EnergyLocalTime,
    },
    /// Returns the complete day containing the retained facility peak.
    FacilityPeak,
    /// The service searches the remaining year in bounded chunks and returns
    /// the first matching common interval, or an explicit not-found result.
    OccupiedUnmet {
        start: EnergyLocalTime,
        zone_id: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnergyQueryWindow {
    pub start_interval: usize,
    pub count: usize,
    pub focus_interval: Option<usize>,
}
impl EnergyQueryWindow {
    pub fn validate(&self) -> Result<()> {
        if !(1..=MAX_INVESTIGATION_INTERVALS).contains(&self.count)
            || self.start_interval >= ENERGY_INTERVALS
            || self.count > ENERGY_INTERVALS - self.start_interval
            || self
                .focus_interval
                .is_some_and(|i| i < self.start_interval || i >= self.start_interval + self.count)
        {
            return Err(invalid(
                "investigation.range",
                "Request 1–1008 complete ten-minute intervals within the retained year",
            ));
        }
        Ok(())
    }
    pub fn native_range(&self, interval_minutes: u16) -> Result<(usize, usize)> {
        self.validate()?;
        let factor = source_factor(interval_minutes)?;
        Ok((self.start_interval * factor, self.count * factor))
    }
}

/// For an unmet search this resolves the first bounded search chunk only.
/// The runner continues sequentially without expanding the response limit.
pub fn investigation_window(
    request: &EnergyNavigationRequest,
    peak: &EnergyTimestamp,
) -> Result<EnergyQueryWindow> {
    let (start, end, focus) = match request {
        EnergyNavigationRequest::At { time } => {
            let i = time.interval_index(false)?;
            (i, i + 1, Some(i))
        }
        EnergyNavigationRequest::Day { date } => {
            let i = date.day_index()? * STEPS_PER_DAY;
            (i, i + STEPS_PER_DAY, None)
        }
        EnergyNavigationRequest::Range { start, end } => (
            start.interval_index(false)?,
            end.interval_index(true)?,
            None,
        ),
        EnergyNavigationRequest::FacilityPeak => {
            if interval_timestamp(peak.interval_index)? != *peak {
                return Err(invalid(
                    "investigation.peak",
                    "Retained peak timestamp is inconsistent with the canonical time axis",
                ));
            }
            let i = peak.interval_index / STEPS_PER_DAY * STEPS_PER_DAY;
            (i, i + STEPS_PER_DAY, Some(peak.interval_index))
        }
        EnergyNavigationRequest::OccupiedUnmet { start, zone_id } => {
            if let Some(id) = zone_id {
                identifier("investigation.zone_id", id)?;
            }
            let i = start.interval_index(false)?;
            (
                i,
                (i + MAX_INVESTIGATION_INTERVALS).min(ENERGY_INTERVALS),
                None,
            )
        }
    };
    if end <= start {
        return Err(invalid(
            "investigation.range",
            "Range end must be after its start; no implicit year wrap",
        ));
    }
    let window = EnergyQueryWindow {
        start_interval: start,
        count: end - start,
        focus_interval: focus,
    };
    window.validate()?;
    Ok(window)
}
fn source_factor(minutes: u16) -> Result<usize> {
    match minutes {
        10 => Ok(1),
        5 => Ok(2),
        _ => Err(invalid(
            "investigation.interval_minutes",
            "Only retained five- and ten-minute source intervals are supported",
        )),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyDemandPoint {
    pub timestamp: EnergyTimestamp,
    pub facility_kw: f64,
    pub end_uses_kw: BTreeMap<EnergyEndUse, f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnergySourceTimestamp {
    pub source_interval_index: usize,
    pub common_interval_index: usize,
    pub date: MonthDay,
    pub start_minute: u16,
    pub end_minute: u16,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyComfortInterval {
    pub timestamp: EnergySourceTimestamp,
    pub samples: Vec<ComfortSample>,
    pub occupied_unmet_zone_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyInvestigationData {
    pub window: EnergyQueryWindow,
    pub source_interval_minutes: u16,
    pub demand_10min: Vec<EnergyDemandPoint>,
    /// Source resolution is retained, so five-minute excursions cannot vanish
    /// inside an averaged ten-minute temperature.
    pub comfort: Vec<EnergyComfortInterval>,
}

pub fn investigation_data(
    window: EnergyQueryWindow,
    interval_minutes: u16,
    rows: &[EnergyInterval],
    conditioned_zones: &[String],
) -> Result<EnergyInvestigationData> {
    let (native_start, native_count) = window.native_range(interval_minutes)?;
    if rows.len() != native_count {
        return Err(invalid(
            "investigation.rows",
            "Missing or excess source rows",
        ));
    }
    let factor = source_factor(interval_minutes)?;
    let zones: BTreeSet<_> = conditioned_zones.iter().cloned().collect();
    if zones.len() != conditioned_zones.len() || zones.len() > MAX_THERMAL_ZONES {
        return Err(invalid(
            "investigation.zones",
            "Expected unique conditioned zone IDs",
        ));
    }
    for id in &zones {
        identifier("investigation.zone_id", id)?;
    }
    let keys = rows[0].end_uses_j.keys().copied().collect::<BTreeSet<_>>();
    let mut comfort = Vec::with_capacity(native_count);
    for (offset, row) in rows.iter().enumerate() {
        if row.index != native_start + offset
            || row.end_uses_j.keys().copied().collect::<BTreeSet<_>>() != keys
        {
            return Err(invalid(
                "investigation.rows",
                "Source interval order or end-use membership differs",
            ));
        }
        range("investigation.facility_j", row.facility_j, 0., f64::MAX)?;
        for &value in row.end_uses_j.values() {
            range("investigation.component_j", value, 0., f64::MAX)?;
        }
        let mut seen = BTreeSet::new();
        let mut unmet = Vec::new();
        for sample in &row.comfort {
            if !seen.insert(sample.zone_id.clone()) {
                return Err(invalid(
                    "investigation.comfort",
                    "Duplicate conditioned-zone sample",
                ));
            }
            range(
                "investigation.temperature_c",
                sample.air_temperature_c,
                -100.,
                200.,
            )?;
            range(
                "investigation.setpoint_c",
                sample.cooling_setpoint_c,
                -50.,
                100.,
            )?;
            if sample.occupied
                && sample.air_temperature_c
                    > sample.cooling_setpoint_c + COMFORT_COOLING_TOLERANCE_C
            {
                unmet.push(sample.zone_id.clone());
            }
        }
        if seen != zones {
            return Err(invalid(
                "investigation.comfort",
                "Expected every conditioned-zone sample exactly once",
            ));
        }
        let common = row.index / factor;
        let timestamp = interval_timestamp(common)?;
        let start_minute = timestamp.start_minute + (row.index % factor) as u16 * interval_minutes;
        comfort.push(EnergyComfortInterval {
            timestamp: EnergySourceTimestamp {
                source_interval_index: row.index,
                common_interval_index: common,
                date: MonthDay {
                    month: timestamp.month,
                    day: timestamp.day,
                },
                start_minute,
                end_minute: start_minute + interval_minutes,
            },
            samples: row.comfort.clone(),
            occupied_unmet_zone_ids: unmet,
        });
    }
    let mut demand_10min = Vec::with_capacity(window.count);
    for (offset, group) in rows.chunks_exact(factor).enumerate() {
        // Same two-value addition and conversion as the retained accumulator.
        let mut facility = 0.;
        let mut components = BTreeMap::new();
        for row in group {
            facility += row.facility_j;
            for (&key, &value) in &row.end_uses_j {
                *components.entry(key).or_insert(0.) += value;
            }
        }
        let end_uses_kw = components
            .into_iter()
            .map(|(key, value)| Ok((key, interval_demand_kw(value, 600)?)))
            .collect::<Result<_>>()?;
        demand_10min.push(EnergyDemandPoint {
            timestamp: interval_timestamp(window.start_interval + offset)?,
            facility_kw: interval_demand_kw(facility, 600)?,
            end_uses_kw,
        });
    }
    Ok(EnergyInvestigationData {
        window,
        source_interval_minutes: interval_minutes,
        demand_10min,
        comfort,
    })
}

/// Returns a common interval but retains the original subinterval in data.
pub fn first_occupied_unmet(
    data: &EnergyInvestigationData,
    zone_id: Option<&str>,
) -> Option<usize> {
    data.comfort
        .iter()
        .find(|row| match zone_id {
            Some(id) => row.occupied_unmet_zone_ids.iter().any(|z| z == id),
            None => !row.occupied_unmet_zone_ids.is_empty(),
        })
        .map(|row| row.timestamp.common_interval_index)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyDemandScale {
    pub minimum_kw: f64,
    pub maximum_kw: f64,
}
/// One exact shared extent for both plots; a zero-only extent is explicitly
/// [0,0], leaving visual padding to the view without inventing a result value.
pub fn shared_demand_scale(
    a: &[EnergyDemandPoint],
    b: &[EnergyDemandPoint],
) -> Result<EnergyDemandScale> {
    if a.len() != b.len()
        || a.is_empty()
        || a.len() > MAX_INVESTIGATION_INTERVALS
        || a.iter().zip(b).any(|(x, y)| x.timestamp != y.timestamp)
    {
        return Err(invalid(
            "investigation.alignment",
            "A/B points require identical explicit timestamps and a bounded nonempty range",
        ));
    }
    let mut maximum_kw = 0_f64;
    for point in a.iter().chain(b) {
        if interval_timestamp(point.timestamp.interval_index)? != point.timestamp {
            return Err(invalid(
                "investigation.timestamp",
                "Timestamp fields do not match interval index",
            ));
        }
        for &v in std::iter::once(&point.facility_kw).chain(point.end_uses_kw.values()) {
            range("investigation.demand_kw", v, 0., f64::MAX)?;
            maximum_kw = maximum_kw.max(v);
        }
    }
    Ok(EnergyDemandScale {
        minimum_kw: 0.,
        maximum_kw,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn time(month: u8, day: u8, minute: u16) -> EnergyLocalTime {
        EnergyLocalTime {
            date: MonthDay { month, day },
            minute,
        }
    }
    fn row(index: usize, j: f64, t: f64) -> EnergyInterval {
        EnergyInterval {
            index,
            facility_j: j,
            end_uses_j: [(EnergyEndUse::CoolingSystem, j)].into(),
            comfort: vec![ComfortSample {
                zone_id: "z".into(),
                occupied: true,
                air_temperature_c: t,
                cooling_setpoint_c: 24.,
            }],
        }
    }
    #[test]
    fn exact_dates_endpoints_and_nonleap_ranges() {
        let peak = interval_timestamp(ENERGY_INTERVALS - 1).unwrap();
        assert_eq!(time(3, 1, 0).interval_index(false).unwrap(), 59 * 144);
        assert!(time(2, 29, 0).interval_index(false).is_err());
        assert!(time(1, 1, 5).interval_index(false).is_err());
        assert!(time(12, 31, 1440).interval_index(false).is_err());
        let w = investigation_window(
            &EnergyNavigationRequest::Range {
                start: time(12, 31, 1430),
                end: time(12, 31, 1440),
            },
            &peak,
        )
        .unwrap();
        assert_eq!((w.start_interval, w.count), (52559, 1));
        assert_eq!(w.native_range(5).unwrap(), (105118, 2));
        assert_eq!(time(2, 28, 60).shift_day(1).unwrap(), time(3, 1, 60));
        assert!(time(1, 1, 0).shift_day(-1).is_err());
        assert!(investigation_window(
            &EnergyNavigationRequest::Range {
                start: time(1, 1, 0),
                end: time(1, 8, 10)
            },
            &peak
        )
        .is_err());
        assert_eq!(
            investigation_window(
                &EnergyNavigationRequest::Range {
                    start: time(1, 1, 0),
                    end: time(1, 8, 0)
                },
                &peak
            )
            .unwrap()
            .count,
            1008
        );
        let w = investigation_window(&EnergyNavigationRequest::FacilityPeak, &peak).unwrap();
        assert_eq!(
            (w.start_interval, w.count, w.focus_interval),
            (52416, 144, Some(52559))
        );
        let mut forged = peak;
        forged.day = 30;
        assert!(investigation_window(&EnergyNavigationRequest::FacilityPeak, &forged).is_err());
    }
    #[test]
    fn five_minute_energy_is_paired_but_comfort_excursions_are_not_averaged() {
        let window = EnergyQueryWindow {
            start_interval: 143,
            count: 2,
            focus_interval: None,
        };
        let data = investigation_data(
            window,
            5,
            &[
                row(286, 0., 24.),
                row(287, 1_200_000., 24.3),
                row(288, 300_000., 24.2),
                row(289, 300_000., 23.),
            ],
            &["z".into()],
        )
        .unwrap();
        assert_eq!(
            data.demand_10min
                .iter()
                .map(|p| p.facility_kw)
                .collect::<Vec<_>>(),
            vec![2., 1.]
        );
        assert_eq!(data.comfort[1].timestamp.end_minute, 1440);
        assert_eq!(
            data.comfort[2].timestamp.date,
            MonthDay { month: 1, day: 2 }
        );
        assert_eq!(data.comfort[2].timestamp.start_minute, 0);
        assert_eq!(first_occupied_unmet(&data, None), Some(143));
        assert!(data.comfort[2].occupied_unmet_zone_ids.is_empty());
        let coarse = investigation_data(
            window,
            10,
            &[row(143, 1_200_000., 24.), row(144, 600_000., 24.)],
            &["z".into()],
        )
        .unwrap();
        assert_eq!(coarse.demand_10min, data.demand_10min);
        assert_eq!(
            shared_demand_scale(&data.demand_10min, &coarse.demand_10min)
                .unwrap()
                .maximum_kw,
            2.
        );
        let mut wrong = coarse.demand_10min.clone();
        wrong[0].timestamp = interval_timestamp(142).unwrap();
        assert!(shared_demand_scale(&data.demand_10min, &wrong).is_err());
    }
    #[test]
    fn malformed_order_shape_zones_and_nonfinite_values_reject() {
        let w = EnergyQueryWindow {
            start_interval: 0,
            count: 1,
            focus_interval: None,
        };
        assert!(investigation_data(w, 5, &[row(0, 1., 24.)], &["z".into()]).is_err());
        assert!(investigation_data(w, 10, &[row(1, 1., 24.)], &["z".into()]).is_err());
        assert!(investigation_data(w, 10, &[row(0, f64::NAN, 24.)], &["z".into()]).is_err());
        assert!(investigation_data(w, 10, &[row(0, 1., 24.)], &["wrong".into()]).is_err());
        assert!(investigation_data(w, 10, &[row(0, 1., 24.)], &["z".into(), "z".into()]).is_err());
        assert!(investigation_data(
            w,
            5,
            &[row(0, f64::MAX, 24.), row(1, f64::MAX, 24.)],
            &["z".into()]
        )
        .is_err());
        let mut unoccupied = row(0, 1., 40.);
        unoccupied.comfort[0].occupied = false;
        assert_eq!(
            first_occupied_unmet(
                &investigation_data(w, 10, &[unoccupied], &["z".into()]).unwrap(),
                None
            ),
            None
        );
    }
}
