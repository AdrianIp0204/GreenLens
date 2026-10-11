//! Climate-study identity and metric arithmetic; independent of engine execution.
use crate::{canonical_hash, validate_model, CoreError, Model, Result};
use serde::{Deserialize, Serialize};

pub const STUDY_SCHEMA: &str = "greenlens.study.v0.3";
pub const METRIC_SCHEMA: &str = "greenlens.metrics.v0.3";
pub const HOURS_PER_YEAR: usize = 8760;
pub const MAX_ANNUAL_SENSORS: usize = 256;

fn invalid(field: &str, message: &str) -> CoreError {
    CoreError::Validation {
        field: field.into(),
        message: message.into(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WeatherBinding {
    pub original_sha256: String,
    pub normalized_sha256: String,
    pub time_axis_sha256: String,
    pub interpretation: String,
}
impl WeatherBinding {
    pub fn validate(&self) -> Result<()> {
        for value in [
            &self.original_sha256,
            &self.normalized_sha256,
            &self.time_axis_sha256,
        ] {
            if value.len() != 64
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(invalid("weather", "Expected a lowercase SHA-256 identity"));
            }
        }
        if self.interpretation.is_empty() || self.interpretation.len() > 120 {
            return Err(invalid(
                "weather.interpretation",
                "Expected a versioned supported interpretation identifier",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NumericalProfile {
    Shipping,
    Reference,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum StudyMode {
    /// Routes to the unchanged legacy gensky/rtrace adapter and identity.
    StaticOvercast,
    WeatherHour {
        weather: WeatherBinding,
        interval_index: usize,
        profile: NumericalProfile,
    },
    Annual {
        weather: WeatherBinding,
        profile: NumericalProfile,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Study {
    pub schema_version: String,
    pub calculation: StudyMode,
    pub metrics: MetricSettings,
}
impl Default for Study {
    fn default() -> Self {
        Self {
            schema_version: STUDY_SCHEMA.into(),
            calculation: StudyMode::StaticOvercast,
            metrics: MetricSettings::default(),
        }
    }
}
impl Study {
    pub fn validate(&self, model: &Model) -> Result<()> {
        if self.schema_version != STUDY_SCHEMA {
            return Err(invalid("study.schema_version", "Unsupported study version"));
        }
        let validated = validate_model(model)?;
        self.metrics.weights()?;
        match &self.calculation {
            StudyMode::StaticOvercast => (),
            StudyMode::WeatherHour {
                weather,
                interval_index,
                ..
            } => {
                weather.validate()?;
                if *interval_index >= HOURS_PER_YEAR {
                    return Err(invalid(
                        "interval_index",
                        "Expected 0–8759 in the imported non-leap weather year",
                    ));
                }
                if validated.sensor_grid.sensors.len() > MAX_ANNUAL_SENSORS {
                    return Err(invalid(
                        "sensors",
                        "Weather studies support at most 256 sensors; increase grid spacing",
                    ));
                }
            }
            StudyMode::Annual { weather, .. } => {
                weather.validate()?;
                if validated.sensor_grid.sensors.len() > MAX_ANNUAL_SENSORS {
                    return Err(invalid(
                        "sensors",
                        "Annual studies support at most 256 sensors; increase grid spacing",
                    ));
                }
            }
        }
        Ok(())
    }

    /// Annual geometry identity excludes the irrelevant prescribed overcast intensity.
    /// Actual engine/settings identity is bound by the adapter when creating a run.
    pub fn physical_input_hash(&self, model: &Model) -> Result<String> {
        self.validate(model)?;
        if matches!(self.calculation, StudyMode::StaticOvercast) {
            return Ok(validate_model(model)?.scientific_input_hash);
        }
        canonical_hash(&(
            STUDY_SCHEMA,
            &self.calculation,
            &model.room,
            &model.window,
            &model.materials,
            &model.overhang,
            &model.obstruction,
            &model.sensors,
            model.sky.ground_reflectance,
        ))
    }

    pub fn comparison_input_hash(&self, model: &Model) -> Result<String> {
        self.validate(model)?;
        let validated = validate_model(model)?;
        if matches!(self.calculation, StudyMode::StaticOvercast) {
            return Ok(validated.comparison_conditions_hash);
        }
        canonical_hash(&(
            STUDY_SCHEMA,
            &self.calculation,
            &model.room,
            &validated.sensor_hash,
            &model.obstruction,
            model.sky.ground_reflectance,
            model.materials.ceiling_reflectance,
            model.materials.wall_reflectance,
            model.materials.floor_reflectance,
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Occupancy {
    /// Local-standard-clock minutes; end may be 1440. Wrap past midnight allowed.
    Daily {
        start_minute: u16,
        end_minute: u16,
    },
    Mask {
        weights: Vec<f64>,
    },
}
impl Default for Occupancy {
    fn default() -> Self {
        Self::Daily {
            start_minute: 480,
            end_minute: 1080,
        }
    }
}
impl Occupancy {
    pub fn weights(&self) -> Result<Vec<f64>> {
        let weights = match self {
            Self::Mask { weights } => {
                if weights.len() != HOURS_PER_YEAR {
                    return Err(invalid(
                        "occupancy",
                        "Mask must have exactly 8,760 hourly weights",
                    ));
                }
                weights.clone()
            }
            Self::Daily {
                start_minute,
                end_minute,
            } => {
                let start = usize::from(*start_minute);
                let end = usize::from(*end_minute);
                if start >= 1440 || end > 1440 || start == end {
                    return Err(invalid("occupancy", "Daily interval needs distinct times in 00:00–24:00; start must be before 24:00"));
                }
                (0..HOURS_PER_YEAR)
                    .map(|index| {
                        let a = (index % 24) * 60;
                        let b = a + 60;
                        let overlap = |lo: usize, hi: usize| b.min(hi).saturating_sub(a.max(lo));
                        let minutes = if start < end {
                            overlap(start, end)
                        } else {
                            overlap(start, 1440) + overlap(0, end)
                        };
                        minutes as f64 / 60.0
                    })
                    .collect()
            }
        };
        if weights
            .iter()
            .any(|w| !w.is_finite() || !(0.0..=1.0).contains(w))
        {
            return Err(invalid(
                "occupancy",
                "Every weight must be finite and between 0 and 1",
            ));
        }
        let mut sum = CompensatedSum::default();
        for &weight in &weights {
            sum.add(weight);
        }
        if sum.value() <= 0.0 {
            return Err(invalid(
                "occupancy",
                "At least one occupied interval is required",
            ));
        }
        Ok(weights)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MetricSettings {
    pub da_threshold_lux: f64,
    pub udi_lower_lux: f64,
    pub udi_upper_lux: f64,
    pub occupancy: Occupancy,
}
impl Default for MetricSettings {
    fn default() -> Self {
        Self {
            da_threshold_lux: 300.0,
            udi_lower_lux: 100.0,
            udi_upper_lux: 3000.0,
            occupancy: Occupancy::default(),
        }
    }
}
impl MetricSettings {
    pub fn weights(&self) -> Result<Vec<f64>> {
        if !self.da_threshold_lux.is_finite() || self.da_threshold_lux <= 0.0 {
            return Err(invalid(
                "DA threshold",
                "Must be finite and greater than zero lux",
            ));
        }
        if !self.udi_lower_lux.is_finite()
            || !self.udi_upper_lux.is_finite()
            || self.udi_lower_lux < 0.0
            || self.udi_upper_lux <= self.udi_lower_lux
        {
            return Err(invalid(
                "UDI bounds",
                "Require finite 0 <= lower < upper, in lux",
            ));
        }
        self.occupancy.weights()
    }
    /// Bind the immutable physical-result checksum, not just its input recipe.
    pub fn identity(&self, physical_result_sha256: &str) -> Result<String> {
        self.weights()?;
        if physical_result_sha256.len() != 64
            || !physical_result_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid(
                "physical result identity",
                "Expected a lowercase SHA-256 checksum of immutable physical results",
            ));
        }
        canonical_hash(&(METRIC_SCHEMA, physical_result_sha256, self))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SensorMetrics {
    pub da_percent: f64,
    pub cda_percent: f64,
    pub udi_percent: f64,
    pub underlit_percent: f64,
    pub overlit_percent: f64,
}

#[derive(Clone, Copy, Default)]
struct CompensatedSum {
    sum: f64,
    correction: f64,
}
impl CompensatedSum {
    fn add(&mut self, value: f64) {
        let y = value - self.correction;
        let t = self.sum + y;
        self.correction = (t - self.sum) - y;
        self.sum = t;
    }
    fn value(self) -> f64 {
        self.sum
    }
}

/// Consume one time-major sensor vector at a time. Missing/nighttime data is never inferred.
pub struct MetricAccumulator {
    settings: MetricSettings,
    weights: Vec<f64>,
    totals: Vec<[CompensatedSum; 5]>,
    index: usize,
}
impl MetricAccumulator {
    pub fn new(sensor_count: usize, settings: &MetricSettings) -> Result<Self> {
        if !(1..=MAX_ANNUAL_SENSORS).contains(&sensor_count) {
            return Err(invalid("sensor_count", "Expected 1–256 sensors"));
        }
        let mut weights = settings.weights()?;
        // Scale-invariant metrics must also work for positive subnormal masks.
        // Scaling first keeps the denominator >= 1 and avoids 100/tiny overflow.
        let maximum = weights.iter().copied().fold(0.0_f64, f64::max);
        for weight in &mut weights {
            *weight /= maximum;
        }
        Ok(Self {
            settings: settings.clone(),
            weights,
            totals: vec![[CompensatedSum::default(); 5]; sensor_count],
            index: 0,
        })
    }
    pub fn push_hour(&mut self, lux: &[f64]) -> Result<()> {
        if self.index >= HOURS_PER_YEAR
            || lux.len() != self.totals.len()
            || lux.iter().any(|v| !v.is_finite() || *v < 0.0)
        {
            return Err(invalid("annual output", "Expected exactly 8,760 ordered vectors of finite nonnegative lux with the declared sensor count"));
        }
        let w = self.weights[self.index];
        for (values, &e) in self.totals.iter_mut().zip(lux) {
            values[0].add(if e >= self.settings.da_threshold_lux {
                w
            } else {
                0.0
            });
            values[1].add(w * (e / self.settings.da_threshold_lux).min(1.0));
            values[2].add(
                if e >= self.settings.udi_lower_lux && e <= self.settings.udi_upper_lux {
                    w
                } else {
                    0.0
                },
            );
            values[3].add(if e < self.settings.udi_lower_lux {
                w
            } else {
                0.0
            });
            values[4].add(if e > self.settings.udi_upper_lux {
                w
            } else {
                0.0
            });
        }
        self.index += 1;
        Ok(())
    }
    pub fn finish(self) -> Result<Vec<SensorMetrics>> {
        if self.index != HOURS_PER_YEAR {
            return Err(invalid(
                "annual output",
                "Incomplete time series; 8,760 intervals are required before deriving metrics",
            ));
        }
        let mut denominator = CompensatedSum::default();
        for w in self.weights {
            denominator.add(w);
        }
        let percent = |value: CompensatedSum| value.value() / denominator.value() * 100.0;
        Ok(self
            .totals
            .into_iter()
            .map(|v| SensorMetrics {
                da_percent: percent(v[0]),
                cda_percent: percent(v[1]),
                udi_percent: percent(v[2]),
                underlit_percent: percent(v[3]),
                overlit_percent: percent(v[4]),
            })
            .collect())
    }
}
