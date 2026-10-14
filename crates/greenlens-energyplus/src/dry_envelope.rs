//! Exact consumer boundary for the separately versioned restricted EPW policy.
//! Imported epJSON is never accepted; this guard also catches future additions
//! to the owned generator that require a fresh weather suitability decision.
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
pub const METHOD_ID: &str = "greenlens.energyplus.dry-envelope-dx/1";
const ALLOWED: &[&str] = &[
    "Version",
    "Building",
    "Timestep",
    "HeatBalanceAlgorithm",
    "SurfaceConvectionAlgorithm:Inside",
    "SurfaceConvectionAlgorithm:Outside",
    "ShadowCalculation",
    "ConvergenceLimits",
    "SimulationControl",
    "RunPeriod",
    "GlobalGeometryRules",
    "ScheduleTypeLimits",
    "Schedule:Constant",
    "Schedule:Compact",
    "Material",
    "Construction",
    "WindowMaterial:SimpleGlazingSystem",
    "Curve:Quadratic",
    "Curve:Cubic",
    "Curve:Biquadratic",
    "Zone",
    "BuildingSurface:Detailed",
    "FenestrationSurface:Detailed",
    "Shading:Building:Detailed",
    "People",
    "Lights",
    "ElectricEquipment",
    "Exterior:Lights",
    "ZoneInfiltration:DesignFlowRate",
    "ZoneVentilation:DesignFlowRate",
    "ZoneControl:Thermostat",
    "ThermostatSetpoint:SingleCooling",
    "ZoneHVAC:EquipmentConnections",
    "ZoneHVAC:EquipmentList",
    "ZoneHVAC:WindowAirConditioner",
    "Fan:SystemModel",
    "Coil:Cooling:DX:SingleSpeed",
    "OutdoorAir:Mixer",
    "OutdoorAir:Node",
    "Output:Diagnostics",
    "Output:Meter",
    "Output:Variable",
    "Output:VariableDictionary",
    "OutputControl:Files",
    "Site:GroundReflectance",
];
pub fn ground_reflectance() -> Value {
    json!({"GreenLens prescribed dry ground":{"january_ground_reflectance":0.2,"february_ground_reflectance":0.2,"march_ground_reflectance":0.2,"april_ground_reflectance":0.2,"may_ground_reflectance":0.2,"june_ground_reflectance":0.2,"july_ground_reflectance":0.2,"august_ground_reflectance":0.2,"september_ground_reflectance":0.2,"october_ground_reflectance":0.2,"november_ground_reflectance":0.2,"december_ground_reflectance":0.2}})
}
pub fn validate(document: &Value) -> Result<()> {
    let objects = document
        .as_object()
        .context("Owned epJSON must be an object")?;
    for kind in objects.keys() {
        ensure!(
            ALLOWED.contains(&kind.as_str()),
            "Restricted dry-envelope weather has an unreviewed consumer: {kind}"
        );
    }
    let periods = objects
        .get("RunPeriod")
        .and_then(Value::as_object)
        .context("Missing explicit weather period")?;
    ensure!(
        periods.len() == 1,
        "Restricted weather requires exactly one owned annual run period"
    );
    for period in periods.values() {
        ensure!(
            period["use_weather_file_rain_indicators"] == "No"
                && period["use_weather_file_snow_indicators"] == "No",
            "Restricted weather requires rain/snow surface effects explicitly disabled"
        );
    }
    ensure!(objects.get("Site:GroundReflectance")==Some(&ground_reflectance()),"Restricted weather requires explicit monthly ground reflectance0.2, not missing EPW albedo");
    if let Some(surfaces) = objects
        .get("BuildingSurface:Detailed")
        .and_then(Value::as_object)
    {
        for surface in surfaces.values() {
            ensure!(
                matches!(
                    surface["outside_boundary_condition"].as_str(),
                    Some("Outdoors" | "Adiabatic" | "Surface")
                ),
                "Restricted weather excludes ground/water/other-side surface consumers"
            );
        }
    }
    if let Some(coils) = objects
        .get("Coil:Cooling:DX:SingleSpeed")
        .and_then(Value::as_object)
    {
        for coil in coils.values() {
            ensure!(
                coil["condenser_type"] == "AirCooled",
                "Restricted weather excludes water/evaporative condenser consumers"
            );
        }
    }
    Ok(())
}
pub fn recipe() -> Result<Vec<u8>> {
    // The original recipe remains embedded verbatim as a JSON value. This
    // wrapper is a new recipe identity, never a rewrite of historical bytes.
    let base: Value = serde_json::from_slice(super::recipe_bytes())?;
    Ok(serde_json::to_vec(
        &json!({"schema":"greenlens.energyplus-dry-envelope-recipe.v1","method":METHOD_ID,"base_recipe":base,"weather_profile":greenlens_core::energy::weather::ENERGY_DRY_ENVELOPE_PROFILE,"weather_missing_policy":"Exact999 albedo/precipitation sentinels preserved; unmodeled consumer fields, never substituted observations; other fields remain strict","consumer_allowlist":ALLOWED,"ground_reflectance":ground_reflectance(),"rain_snow_surface_effects":"No","source_review":"EnergyPlus4bd7a1f26fb749a83257855aba6760e26c8a3c8f WeatherManager/WaterManager/PVWatts; docs/v0.5-real-weather-protocol.md"}),
    )?)
}
#[cfg(test)]
mod tests {
    use super::*;
    use greenlens_core::energy::*;
    #[test]
    fn excluded_consumers_flags_and_ground_changes_reject() {
        let f = synthetic_energy_report().unwrap();
        let mut s = f.baseline.study;
        s.weather.profile_id = weather::ENERGY_DRY_ENVELOPE_PROFILE.into();
        let bytes = crate::render_model(&f.baseline.model, &s).unwrap();
        let doc: Value = serde_json::from_slice(&bytes).unwrap();
        validate(&doc).unwrap();
        for excluded in [
            "Generator:PVWatts",
            "Material:RoofVegetation",
            "WaterUse:Storage",
            "Foundation:Kiva",
            "EnergyManagementSystem:Program",
        ] {
            let mut changed = doc.clone();
            changed[excluded] = json!({"Excluded":{}});
            assert!(validate(&changed).is_err(), "{excluded}");
        }
        for flag in [
            "use_weather_file_rain_indicators",
            "use_weather_file_snow_indicators",
        ] {
            let mut changed = doc.clone();
            changed["RunPeriod"]["Annual"][flag] = json!("Yes");
            assert!(validate(&changed).is_err());
        }
        let mut changed = doc.clone();
        changed["Site:GroundReflectance"]["GreenLens prescribed dry ground"]
            ["january_ground_reflectance"] = json!(0.3);
        assert!(validate(&changed).is_err());
        let old = crate::method_identity(&f.proposed.study).unwrap();
        let new = crate::method_identity(&s).unwrap();
        assert_ne!(old.method_id, new.method_id);
        assert_ne!(old.recipe_sha256, new.recipe_sha256);
        assert_eq!(
            crate::recipe_for_study(&f.proposed.study).unwrap(),
            crate::recipe_bytes()
        );
    }
}
