use super::{identifier, invalid, range, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const DAYS_PER_YEAR: usize = 365;
pub const STEPS_PER_DAY: usize = 144;
pub const MONTH_DAYS: [usize; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}
impl Weekday {
    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnergyCalendar {
    /// Explicit synthetic non-leap calendar. EPW source years are not inferred
    /// to be this calendar; no operating-system timezone or DST is consulted.
    pub start_weekday: Weekday,
    pub holidays: Vec<MonthDay>,
    pub utc_offset_minutes: i16,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct MonthDay {
    pub month: u8,
    pub day: u8,
}
impl MonthDay {
    pub fn day_index(self) -> Result<usize> {
        if !(1..=12).contains(&self.month)
            || self.day == 0
            || usize::from(self.day) > MONTH_DAYS[usize::from(self.month - 1)]
        {
            return Err(invalid(
                "calendar.date",
                "Expected a valid non-leap month/day",
            ));
        }
        Ok(MONTH_DAYS[..usize::from(self.month - 1)]
            .iter()
            .sum::<usize>()
            + usize::from(self.day - 1))
    }
    pub fn from_day_index(index: usize) -> Result<Self> {
        if index >= DAYS_PER_YEAR {
            return Err(invalid("calendar.index", "Expected 0–364"));
        }
        let mut remainder = index;
        for (month, &days) in MONTH_DAYS.iter().enumerate() {
            if remainder < days {
                return Ok(Self {
                    month: (month + 1) as u8,
                    day: (remainder + 1) as u8,
                });
            }
            remainder -= days;
        }
        unreachable!("bounded non-leap date")
    }
}
impl EnergyCalendar {
    pub fn validate(&self) -> Result<()> {
        if !(-720..=840).contains(&self.utc_offset_minutes) {
            return Err(invalid(
                "calendar.utc_offset_minutes",
                "Expected fixed local standard offset −720 through 840",
            ));
        }
        let mut seen = BTreeSet::new();
        for date in &self.holidays {
            if !seen.insert(date.day_index()?) {
                return Err(invalid("calendar.holidays", "Duplicate holiday"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleKind {
    Fraction,
    ActivityWattsPerPerson,
    TemperatureCelsius,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DayProfile {
    /// Exactly 144 piecewise-constant local-clock ten-minute intervals, starting
    /// at 00:00. Finer native timestep repeats each value; no interpolation.
    pub values: Vec<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HolidayOverride {
    pub date: MonthDay,
    pub profile: DayProfile,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Schedule {
    pub id: String,
    pub kind: ScheduleKind,
    pub weekday: DayProfile,
    pub saturday: DayProfile,
    pub sunday: DayProfile,
    /// Every declared calendar holiday requires an explicit override. A missing
    /// override must not silently become an ordinary weekday or zero schedule.
    pub holidays: Vec<HolidayOverride>,
}
impl DayProfile {
    pub fn constant(value: f64) -> Self {
        Self {
            values: vec![value; STEPS_PER_DAY],
        }
    }
    fn validate(&self, kind: ScheduleKind) -> Result<()> {
        if self.values.len() != STEPS_PER_DAY {
            return Err(invalid(
                "schedule.values",
                "Expected exactly 144 ten-minute values",
            ));
        }
        for &v in &self.values {
            match kind {
                ScheduleKind::Fraction => range("schedule.fraction", v, 0., 1.)?,
                ScheduleKind::ActivityWattsPerPerson => {
                    range("schedule.activity_w_per_person", v, 0., 2000.)?
                }
                ScheduleKind::TemperatureCelsius => range("schedule.temperature_c", v, -50., 100.)?,
            }
        }
        Ok(())
    }
}
impl Schedule {
    pub fn validate(&self, calendar: &EnergyCalendar) -> Result<()> {
        identifier("schedule.id", &self.id)?;
        calendar.validate()?;
        for p in [&self.weekday, &self.saturday, &self.sunday] {
            p.validate(self.kind)?;
        }
        let required = calendar
            .holidays
            .iter()
            .map(|d| d.day_index())
            .collect::<Result<BTreeSet<_>>>()?;
        let mut seen = BTreeSet::new();
        for item in &self.holidays {
            if !seen.insert(item.date.day_index()?) {
                return Err(invalid("schedule.holidays", "Duplicate dated override"));
            }
            item.profile.validate(self.kind)?;
        }
        if required != seen {
            return Err(invalid(
                "schedule.holidays",
                "Provide exactly one explicit override for every calendar holiday",
            ));
        }
        Ok(())
    }
}
pub fn expand_schedule(schedule: &Schedule, calendar: &EnergyCalendar) -> Result<Vec<f64>> {
    schedule.validate(calendar)?;
    let mut out = Vec::with_capacity(super::ENERGY_INTERVALS);
    for day in 0..DAYS_PER_YEAR {
        let date = MonthDay::from_day_index(day)?;
        let profile = if let Some(holiday) = schedule.holidays.iter().find(|h| h.date == date) {
            &holiday.profile
        } else {
            match (calendar.start_weekday.index() + day) % 7 {
                5 => &schedule.saturday,
                6 => &schedule.sunday,
                _ => &schedule.weekday,
            }
        };
        out.extend_from_slice(&profile.values);
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnergyTimestamp {
    pub interval_index: usize,
    pub month: u8,
    pub day: u8,
    pub start_minute: u16,
    pub end_minute: u16,
}
pub fn interval_timestamp(index: usize) -> Result<EnergyTimestamp> {
    if index >= super::ENERGY_INTERVALS {
        return Err(invalid("energy.interval_index", "Expected 0–52559"));
    }
    let date = MonthDay::from_day_index(index / STEPS_PER_DAY)?;
    let start = ((index % STEPS_PER_DAY) * 10) as u16;
    Ok(EnergyTimestamp {
        interval_index: index,
        month: date.month,
        day: date.day,
        start_minute: start,
        end_minute: start + 10,
    })
}
