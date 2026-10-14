//! Separate thermal EPW admission policy. Daylight parsing supplies only the
//! already-tested file/chronology grammar; it cannot attest to thermal fields.
use super::{hash, invalid, range};
use crate::{
    canonical_hash,
    weather::{parse_epw, WeatherLocation, WeatherSourceProfile, WeatherWarning},
    Result,
};
use serde::{Deserialize, Serialize};

pub const ENERGY_WEATHER_SCHEMA: &str = "greenlens.energy-weather.v0.4";
pub const ENERGY_EPW_PROFILE: &str = "greenlens.energy-epw-explicit-fields.v1";
pub const ENERGY_DRY_ENVELOPE_PROFILE: &str = "greenlens.energy-epw-dry-envelope.v1";
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnergyWeatherProfile {
    ExplicitFieldsV1,
    DryEnvelopeV1,
}
impl EnergyWeatherProfile {
    pub fn id(self) -> &'static str {
        match self {
            Self::ExplicitFieldsV1 => ENERGY_EPW_PROFILE,
            Self::DryEnvelopeV1 => ENERGY_DRY_ENVELOPE_PROFILE,
        }
    }
    pub fn from_id(id: &str) -> Result<Self> {
        match id {
            ENERGY_EPW_PROFILE => Ok(Self::ExplicitFieldsV1),
            ENERGY_DRY_ENVELOPE_PROFILE => Ok(Self::DryEnvelopeV1),
            _ => Err(invalid(
                "weather.profile_id",
                "Unsupported energy-specific EPW profile; daylight acceptance is insufficient",
            )),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyWeatherBinding {
    pub original_sha256: String,
    pub normalized_sha256: String,
    pub time_axis_sha256: String,
    pub profile_id: String,
}
impl EnergyWeatherBinding {
    pub fn validate(&self) -> Result<()> {
        for (name, value) in [
            ("weather.original_sha256", &self.original_sha256),
            ("weather.normalized_sha256", &self.normalized_sha256),
            ("weather.time_axis_sha256", &self.time_axis_sha256),
        ] {
            hash(name, value)?;
        }
        EnergyWeatherProfile::from_id(&self.profile_id)?;
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyWeatherHour {
    pub index: usize,
    pub month: u8,
    pub day: u8,
    pub hour_ending: u8,
    pub dry_bulb_c: f64,
    pub dew_point_c: f64,
    pub relative_humidity_percent: f64,
    pub pressure_pa: f64,
    pub horizontal_infrared_wh_m2: f64,
    pub global_horizontal_wh_m2: f64,
    pub direct_normal_wh_m2: f64,
    pub diffuse_horizontal_wh_m2: f64,
    pub wind_direction_deg: f64,
    pub wind_speed_m_s: f64,
    pub total_sky_cover_tenths: f64,
    pub opaque_sky_cover_tenths: f64,
    pub albedo: f64,
    pub snow_depth_cm: f64,
    pub liquid_precipitation_depth_mm: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyWeatherData {
    pub schema_version: String,
    pub original_sha256: String,
    pub normalized_sha256: String,
    pub time_axis_sha256: String,
    pub profile_id: String,
    pub location: WeatherLocation,
    pub original_headers: Vec<String>,
    pub hours: Vec<EnergyWeatherHour>,
    pub warnings: Vec<WeatherWarning>,
}
impl EnergyWeatherData {
    pub fn binding(&self) -> EnergyWeatherBinding {
        EnergyWeatherBinding {
            original_sha256: self.original_sha256.clone(),
            normalized_sha256: self.normalized_sha256.clone(),
            time_axis_sha256: self.time_axis_sha256.clone(),
            profile_id: self.profile_id.clone(),
        }
    }
}
#[derive(Debug, Clone)]
pub struct ParsedEnergyWeather {
    pub data: EnergyWeatherData,
    pub normalized_epw: Vec<u8>,
}
pub fn parse_energy_epw(original: &[u8]) -> Result<ParsedEnergyWeather> {
    parse_energy_epw_with_profile(original, EnergyWeatherProfile::ExplicitFieldsV1)
}
/// The restricted profile retains exact missing sentinel values, never
/// substitutes observations. Its engine adapter must enforce the dry-envelope
/// consumer allowlist. The historical strict entrypoint remains unchanged.
pub fn parse_energy_epw_with_profile(
    original: &[u8],
    profile: EnergyWeatherProfile,
) -> Result<ParsedEnergyWeather> {
    let parsed = parse_epw(original, WeatherSourceProfile::EpwHourEnding)?;
    let original_text =
        std::str::from_utf8(original).map_err(|_| invalid("weather.encoding", "Expected UTF-8"))?;
    let mut hours = Vec::with_capacity(8760);
    let mut missing_albedo = 0;
    let mut missing_precip = 0;
    for (index, line) in original_text.lines().skip(8).enumerate() {
        let fields: Vec<_> = line.split(',').map(str::trim).collect();
        let number = |column: usize, name: &str, low: f64, high: f64| -> Result<f64> {
            let value = fields[column]
                .parse::<f64>()
                .map_err(|_| invalid(name, "Expected a numeric EPW value"))?;
            range(
                &format!("energy_weather.line[{}].{name}", index + 9),
                value,
                low,
                high,
            )?;
            Ok(value)
        };
        let base = &parsed.data.hours[index];
        let dry = number(6, "dry_bulb_c", -70., 70.)?;
        let dew = number(7, "dew_point_c", -70., 70.)?;
        // No silently chosen psychrometric derivation or dew-point rounding
        // allowance. A later admitted exception requires a new provider policy.
        if dew > dry {
            return Err(invalid("energy_weather.dew_point_c","Dew point exceeds dry bulb; review source rather than applying an implicit correction"));
        }
        let radiation = |column, name| -> Result<f64> {
            let value = number(column, name, 0., 9999.)?;
            if value >= 9999. {
                return Err(invalid(
                    name,
                    "Missing radiation sentinel is not a thermal input",
                ));
            }
            Ok(value)
        };
        let total = number(22, "total_sky_cover", 0., 10.)?;
        let opaque = number(23, "opaque_sky_cover", 0., 10.)?;
        if opaque > total {
            return Err(invalid(
                "energy_weather.sky_cover",
                "Opaque cover exceeds total sky cover",
            ));
        }
        let pressure = number(9, "pressure_pa", 31_000., 120_000.)?;
        if pressure <= 31_000. {
            return Err(invalid(
                "energy_weather.pressure_pa",
                "Pressure must exceed 31000 Pa; the engine substitutes lower values",
            ));
        }
        let no_missing_depth = |column, name| -> Result<f64> {
            let value = number(column, name, 0., 999.)?;
            if value >= 999. {
                return Err(invalid(name, "Missing depth sentinel is not admitted, including when rain/snow effects are disabled"));
            }
            Ok(value)
        };
        let unused_sentinel = |column: usize| -> bool {
            profile == EnergyWeatherProfile::DryEnvelopeV1
                && fields[column].parse::<f64>().is_ok_and(|v| v == 999.)
        };
        let albedo = if unused_sentinel(32) {
            missing_albedo += 1;
            999.
        } else {
            number(32, "albedo", 0., 1.)?
        };
        let precipitation = if unused_sentinel(33) {
            missing_precip += 1;
            999.
        } else {
            no_missing_depth(33, "liquid_precipitation_depth_mm")?
        };
        hours.push(EnergyWeatherHour {
            index,
            month: base.month,
            day: base.day,
            hour_ending: base.hour_ending,
            dry_bulb_c: dry,
            dew_point_c: dew,
            relative_humidity_percent: number(8, "relative_humidity", 0., 100.)?,
            pressure_pa: pressure,
            horizontal_infrared_wh_m2: radiation(12, "horizontal_infrared")?,
            global_horizontal_wh_m2: radiation(13, "global_horizontal")?,
            direct_normal_wh_m2: radiation(14, "direct_normal")?,
            diffuse_horizontal_wh_m2: radiation(15, "diffuse_horizontal")?,
            wind_direction_deg: number(20, "wind_direction", 0., 360.)?,
            wind_speed_m_s: number(21, "wind_speed", 0., 40.)?,
            total_sky_cover_tenths: total,
            opaque_sky_cover_tenths: opaque,
            albedo,
            snow_depth_cm: no_missing_depth(30, "snow_depth_cm")?,
            liquid_precipitation_depth_mm: precipitation,
        });
    }
    let axis = canonical_hash(&(
        profile.id(),
        "fixed-local-standard-hour-ending-no-DST",
        &parsed.data.time_axis_sha256,
    ))?;
    let mut warnings = parsed.data.warnings;
    if profile == EnergyWeatherProfile::DryEnvelopeV1 {
        warnings.push(WeatherWarning{code:"energy_dry_envelope_scope".into(),count:8760,message:"Restricted dry-envelope profile: no rain/snow surface effects, water/green-roof/ground/PV/EMS consumers. Exterior ground-reflected solar uses explicitly prescribed monthly reflectance 0.2, not EPW albedo. The adapter enforces this scope; this is not general weather suitability.".into()});
        for (name, count) in [
            ("albedo", missing_albedo),
            ("liquid_precipitation", missing_precip),
        ] {
            if count > 0 {
                warnings.push(WeatherWarning{code:format!("energy_unused_missing_{name}"),count,message:format!("{count} hourly {name} values are exact missing sentinel 999, retained unchanged in weather rows and engine input. They are not zero or observations. EnergyPlus may populate internal weather bookkeeping; this profile excludes its consumers and preserves native diagnostics. Exact affected indices are the rows with this sentinel.")});
            }
        }
    }
    Ok(ParsedEnergyWeather {
        data: EnergyWeatherData {
            schema_version: ENERGY_WEATHER_SCHEMA.into(),
            original_sha256: parsed.data.original_sha256,
            normalized_sha256: parsed.data.normalized_sha256,
            time_axis_sha256: axis,
            profile_id: profile.id().into(),
            location: parsed.data.location,
            original_headers: parsed.data.original_headers,
            hours,
            warnings,
        },
        normalized_epw: parsed.normalized_epw,
    })
}
/// Original, non-climatic repeated-day input. Values are declared artificial;
/// this function is not a weather-data provider or a validated climatic year.
pub fn synthetic_energy_epw() -> Vec<u8> {
    let daylight =
        String::from_utf8(crate::weather::synthetic_epw()).expect("original ASCII fixture");
    let mut lines: Vec<String> = daylight.lines().map(str::to_owned).collect();
    lines[5]="COMMENTS 1,Original non-climatic synthetic thermal fixture; artificial repeated day and complete required thermal fields".into();
    lines[6]="COMMENTS 2,No measured climate claim; fixed standard clock; explicit IR wind sky and albedo; rain and snow effects disabled by the bounded adapter".into();
    for line in &mut lines[8..] {
        let mut fields: Vec<_> = line.split(',').map(str::to_owned).collect();
        fields[8] = "52.5".into();
        fields[30] = "0".into();
        fields[32] = "0.2".into();
        fields[33] = "0".into();
        *line = fields.join(",");
    }
    (lines.join("\n") + "\n").into_bytes()
}
