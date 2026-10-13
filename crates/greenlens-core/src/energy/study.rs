pub use super::weather::EnergyWeatherBinding;
use super::{
    hash, invalid, text, validate_thermal_model, CarbonFactor, EnergyCalendar, MeterLedger,
    ThermalModel, ENERGY_STUDY_SCHEMA,
};
use crate::{canonical_hash, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyStudy {
    pub schema_version: String,
    pub weather: EnergyWeatherBinding,
    pub calendar: EnergyCalendar,
    pub zone_timesteps_per_hour: u8,
}
/// A digest binds the adapter's complete typed numerical settings and complete
/// recipe, not a display-profile name. The adapter/runner must validate those
/// retained objects before constructing this identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnergyMethodIdentity {
    pub method_id: String,
    pub adapter_version: String,
    pub engine_version: String,
    pub engine_package_sha256: String,
    pub engine_executable_sha256: String,
    pub engine_schema_sha256: String,
    pub recipe_sha256: String,
    pub numerical_settings_sha256: String,
}
impl EnergyMethodIdentity {
    pub fn validate(&self) -> Result<()> {
        for (n, v) in [
            ("method.id", &self.method_id),
            ("method.adapter", &self.adapter_version),
            ("method.engine_version", &self.engine_version),
        ] {
            text(n, v)?;
        }
        for (n, v) in [
            ("method.package", &self.engine_package_sha256),
            ("method.executable", &self.engine_executable_sha256),
            ("method.schema", &self.engine_schema_sha256),
            ("method.recipe", &self.recipe_sha256),
            ("method.numerics", &self.numerical_settings_sha256),
        ] {
            hash(n, v)?;
        }
        Ok(())
    }
}
impl EnergyStudy {
    pub fn validate(&self, model: &ThermalModel) -> Result<()> {
        if self.schema_version != ENERGY_STUDY_SCHEMA {
            return Err(invalid(
                "energy_study.schema_version",
                "Unsupported energy study version",
            ));
        }
        self.weather.validate()?;
        self.calendar.validate()?;
        if !matches!(self.zone_timesteps_per_hour, 6 | 12) {
            return Err(invalid(
                "energy_study.zone_timesteps_per_hour",
                "Only declared ten-minute (6) and five-minute (12) zone timesteps are supported",
            ));
        }
        validate_thermal_model(model, &self.calendar)?;
        Ok(())
    }
    pub fn physical_input_hash(&self, model: &ThermalModel) -> Result<String> {
        self.validate(model)?;
        canonical_hash(&(
            ENERGY_STUDY_SCHEMA,
            validate_thermal_model(model, &self.calendar)?.model_input_sha256,
            self,
        ))
    }
    pub fn execution_input_hash(
        &self,
        model: &ThermalModel,
        method: &EnergyMethodIdentity,
    ) -> Result<String> {
        method.validate()?;
        canonical_hash(&(
            "greenlens.energy-execution-input.v1",
            self.physical_input_hash(model)?,
            method,
        ))
    }
}
pub fn processing_identity(result_sha256: &str, ledger: &MeterLedger) -> Result<String> {
    hash("result_sha256", result_sha256)?;
    ledger.validate()?;
    canonical_hash(&(
        "greenlens.energy-processing.v1",
        result_sha256,
        ledger,
        "joules/3600000;ten-minute-demand;comfort-union-0.2C",
    ))
}
pub fn carbon_identity(processed_sha256: &str, factor: &CarbonFactor) -> Result<String> {
    hash("processed_sha256", processed_sha256)?;
    factor.validate()?;
    canonical_hash(&("greenlens.energy-carbon.v1", processed_sha256, factor))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnergyInputComparison {
    pub comparable_geometry_weather_calendar: bool,
    pub unchanged_service_inputs: bool,
    pub differences: Vec<String>,
    pub service_warnings: Vec<String>,
}
pub fn compare_energy_inputs(
    a: &ThermalModel,
    sa: &EnergyStudy,
    b: &ThermalModel,
    sb: &EnergyStudy,
) -> Result<EnergyInputComparison> {
    sa.validate(a)?;
    sb.validate(b)?;
    let zoning = |m: &ThermalModel| {
        m.zones
            .iter()
            .map(|z| (z.id.clone(), z.storey, z.origin_m, z.size_m))
            .collect::<Vec<_>>()
    };
    let compatible = canonical_hash(&zoning(a))? == canonical_hash(&zoning(b))?
        && a.site == b.site
        && sa.weather == sb.weather
        && sa.calendar == sb.calendar
        && sa.zone_timesteps_per_hour == sb.zone_timesteps_per_hour;
    let mut differences = Vec::new();
    let mut warnings = Vec::new();
    if !compatible {
        warnings.push("Zoning, floor area, site, weather, calendar or timestep differs; no like-for-like savings claim.".into());
    }
    if a.north_rotation_deg != b.north_rotation_deg {
        differences.push("Building orientation differs.".into());
    }
    if a.materials != b.materials || a.constructions != b.constructions {
        differences.push("Opaque thermal materials or constructions differ.".into());
    }
    if a.curves != b.curves {
        differences.push("Explicit HVAC performance curves differ.".into());
    }
    if a.exterior_lighting_w != b.exterior_lighting_w
        || a.exterior_lighting_schedule != b.exterior_lighting_schedule
    {
        differences.push("Exterior lighting input or assigned schedule differs.".into());
    }
    if assigned_schedule(a, &a.exterior_lighting_schedule)
        != assigned_schedule(b, &b.exterior_lighting_schedule)
    {
        warnings.push("Exterior lighting service schedule differs.".into());
    }
    if a.glazing != b.glazing
        || a.zones
            .iter()
            .map(|z| &z.windows)
            .ne(b.zones.iter().map(|z| &z.windows))
        || a.detached_shading != b.detached_shading
    {
        differences.push("Glazing, openings or fixed shading differs.".into());
    }
    let assigned = |m: &ThermalModel, id: &str| m.schedules.iter().find(|s| s.id == id).cloned();
    for az in &a.zones {
        if let Some(bz) = b.zones.iter().find(|z| z.id == az.id) {
            if az.surfaces != bz.surfaces {
                differences.push(format!(
                    "Zone {} boundary or construction assignments differ.",
                    az.id
                ));
            }
            if az.gains != bz.gains {
                differences.push(format!("Zone {} gains differ.", az.id));
            }
            if az.cooling_system != bz.cooling_system {
                differences.push(format!(
                    "Zone {} cooling/fan/ventilation system differs.",
                    az.id
                ));
            }
            let ga = &az.gains;
            let gb = &bz.gains;
            if ga.people_per_floor_m2 != gb.people_per_floor_m2
                || assigned(a, &ga.occupancy_schedule) != assigned(b, &gb.occupancy_schedule)
                || assigned(a, &ga.activity_schedule) != assigned(b, &gb.activity_schedule)
            {
                warnings.push(format!(
                    "Zone {} occupancy/activity service inputs differ.",
                    az.id
                ));
            }
            if assigned(a, &ga.lighting_schedule) != assigned(b, &gb.lighting_schedule)
                || assigned(a, &ga.equipment_schedule) != assigned(b, &gb.equipment_schedule)
            {
                warnings.push(format!(
                    "Zone {} lighting/equipment service schedules differ.",
                    az.id
                ));
            }
            match (&az.cooling_system, &bz.cooling_system) {
                (Some(x), Some(y)) => {
                    if assigned(a, &x.cooling_setpoint_schedule)
                        != assigned(b, &y.cooling_setpoint_schedule)
                        || assigned(a, &x.availability_schedule)
                            != assigned(b, &y.availability_schedule)
                        || x.balanced_ventilation != y.balanced_ventilation
                        || assigned(a, &x.balanced_ventilation.fraction_schedule)
                            != assigned(b, &y.balanced_ventilation.fraction_schedule)
                        || x.cooling_deadband_k != y.cooling_deadband_k
                    {
                        warnings.push(format!(
                            "Zone {} conditioning/ventilation service inputs differ.",
                            az.id
                        ));
                    }
                }
                (None, None) => (),
                _ => warnings.push(format!("Zone {} conditioning scope differs.", az.id)),
            }
        }
    }
    if a.schedules != b.schedules {
        differences.push(
            "Operating schedule definitions differ; schedules are physical energy inputs.".into(),
        );
    }
    Ok(EnergyInputComparison {
        comparable_geometry_weather_calendar: compatible,
        unchanged_service_inputs: warnings.is_empty(),
        differences,
        service_warnings: warnings,
    })
}
fn assigned_schedule<'a>(model: &'a ThermalModel, id: &str) -> Option<&'a super::Schedule> {
    model.schedules.iter().find(|s| s.id == id)
}
pub fn validate_weather_context(
    model: &ThermalModel,
    study: &EnergyStudy,
    data: &super::weather::EnergyWeatherData,
) -> Result<()> {
    study.validate(model)?;
    if study.weather != data.binding() {
        return Err(invalid(
            "energy_weather.binding",
            "Weather metadata does not match the study's complete energy binding",
        ));
    }
    if study.calendar.utc_offset_minutes != data.location.utc_offset_minutes
        || model.site.latitude_deg_north != data.location.latitude_deg_north
        || model.site.longitude_deg_east != data.location.longitude_deg_east
        || model.site.elevation_m != data.location.elevation_m
    {
        return Err(invalid("energy_weather.site","Model site and fixed calendar offset must match the admitted weather location; no silent location override"));
    }
    Ok(())
}
