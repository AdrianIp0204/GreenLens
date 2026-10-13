//! Owned EnergyPlus 26.2 adapter. Imported epJSON/commands are never executable.
//! Retained inspection uses the compiled contract without an installed engine.
use anyhow::{ensure, Context, Result};
use greenlens_core::energy::*;
use greenlens_process::{file_hash, Limits, NativeRequest, ProcessOutcome};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

mod model;
mod output;
pub use output::{channels, decode_interval, ledger, Channel, Decoder, Diagnostics};

pub const METHOD_ID: &str = "greenlens.energyplus.explicit-dx/1";
pub const ENGINE_VERSION: &str = "26.2.0";
const INVENTORY: &str = include_str!("../../../dependencies/energyplus.json");
const MAX_FILE: u64 = 64 * 1024 * 1024;
// Fixed bytes bind conventions rather than any A/B model or local pathname.
const RECIPE: &str = concat!(
    "{\"schema\":\"greenlens.energyplus-recipe.v1\",",
    "\"method\":\"greenlens.energyplus.explicit-dx/1\",\"engine\":\"26.2.0\",",
    "\"topology\":\"recirculating WindowAirConditioner; blow-through Fan:SystemModel; air-cooled Coil:Cooling:DX:SingleSpeed; separate balanced ZoneVentilation; infiltration Standard density\",",
    "\"geometry\":\"validated world vertices; north_axis=0; insulated-ground is adiabatic; SimpleGlazingSystem no explicit frame\",",
    "\"schedules\":\"expanded explicit nonleap calendar; compact day/value RLE; no interpolation; five-minute values repeat ten-minute inputs\",",
    "\"numerics\":\"CTF; TARP inside; DOE-2 outside; FullExterior solar; PolygonClipping periodic20; warmup6..50; convergence0.04/0.4; HVACmin1minute max20iterations; no sizing; no DST/rain/snow indicators\",",
    "\"hvac\":\"rated19.44C wetbulb/35C outdoor; compressor minimum -20C; crankcase/basin/latent-degradation disabled; continuous supply fan while available; cooling convergence0.001; explicit curves and power\",",
    "\"outputs\":\"facility/cooling/fans/interiorlights/exteriorlights/interiorequipment J; zone C/native scheduled cooling C/operative thermostat C/supplyfan J/ventfan J/deliveredcooling J; exact expanded occupancy; distinct supply+vent sum reconciles Fans\",",
    "\"comfort\":\"native Schedule Value equals authoritative cooling schedule within1e-7C; operative SingleCooling cutout lies in [schedule-deadband,schedule]; comfort uses schedule, never adjusted cutout\",",
    "\"axis\":\"Jan1 interval-end local standard, 52560x10min or105120x5min; no missing/duplicate/nonfinite/underflow\",",
    "\"arguments\":[\"--jobs\",\"1\",\"--weather\",\"weather.epw\",\"--output-directory\",\".\",\"--output-prefix\",\"eplus\",\"../model.epJSON\"],",
    "\"limits\":{\"seconds\":1800,\"memory\":2147483648,\"scratch\":1073741824,\"file\":134217728},",
    "\"retention\":\"all requested native numeric columns losslessly parsed to f64; exact CSV hash retained; occupancy is input-derived; warnings/err/end/process retained\"}\n"
);
pub fn recipe_bytes() -> &'static [u8] {
    RECIPE.as_bytes()
}
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub root: PathBuf,
}
impl EngineConfig {
    pub fn from_directory(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub fn discover() -> Result<Self> {
        if let Some(root) = std::env::var_os("GREENLENS_ENERGYPLUS_DIR") {
            return Ok(Self::from_directory(root));
        }
        let local = std::env::var_os("LOCALAPPDATA")
            .context("LOCALAPPDATA unavailable; configure EnergyPlus directory")?;
        Ok(Self::from_directory(PathBuf::from(local).join("GreenLens/engines/energyplus-26.2.0/package/EnergyPlus-26.2.0-4bd7a1f26f-Windows-x86_64")))
    }
    pub fn verify(&self) -> Result<()> {
        let data: Value = serde_json::from_str(INVENTORY)?;
        for (name, expected) in data["critical_files"]
            .as_object()
            .context("Compiled engine inventory")?
        {
            let path = self.root.join(name);
            regular_file(&path)?;
            ensure!(
                fs::metadata(&path)?.len()
                    == expected["bytes"].as_u64().context("Inventory length")?
                    && file_hash(&path)?
                        == expected["sha256"].as_str().context("Inventory digest")?,
                "EnergyPlus runtime file differs: {name}; reinstall the pinned package"
            );
        }
        Ok(())
    }
}
pub fn method_identity(study: &EnergyStudy) -> Result<EnergyMethodIdentity> {
    ensure!(
        matches!(study.zone_timesteps_per_hour, 6 | 12),
        "Unsupported energy timestep"
    );
    let inventory: Value = serde_json::from_str(INVENTORY)?;
    let settings = json!({"zone_timesteps_per_hour":study.zone_timesteps_per_hour,"recipe_sha256":sha256(recipe_bytes()),"runtime_inventory_sha256":sha256(INVENTORY.as_bytes())});
    Ok(EnergyMethodIdentity {
        method_id: METHOD_ID.into(),
        adapter_version: "1".into(),
        engine_version: ENGINE_VERSION.into(),
        engine_package_sha256: inventory["sha256"].as_str().unwrap().into(),
        engine_executable_sha256: inventory["critical_files"]["energyplus.exe"]["sha256"]
            .as_str()
            .unwrap()
            .into(),
        engine_schema_sha256: inventory["critical_files"]["Energy+.schema.epJSON"]["sha256"]
            .as_str()
            .unwrap()
            .into(),
        recipe_sha256: sha256(recipe_bytes()),
        numerical_settings_sha256: greenlens_core::canonical_hash(&settings)?,
    })
}
#[derive(Debug, Clone)]
pub struct PreparedRun {
    pub method: EnergyMethodIdentity,
    pub ledger: MeterLedger,
    pub channels: Vec<Channel>,
    pub stage: PathBuf,
    model: ThermalModel,
    study: EnergyStudy,
    inputs: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub process: ProcessOutcome,
    pub diagnostics: Diagnostics,
}

pub fn prepare(
    config: &EngineConfig,
    model: &ThermalModel,
    study: &EnergyStudy,
    normalized_epw: &[u8],
    stage: &Path,
) -> Result<PreparedRun> {
    study.validate(model)?;
    config.verify()?;
    ensure!(
        sha256(normalized_epw) == study.weather.normalized_sha256,
        "Normalized energy weather hash differs"
    );
    let weather = weather::parse_energy_epw(normalized_epw)?;
    ensure!(
        weather.normalized_epw == normalized_epw
            && weather.data.time_axis_sha256 == study.weather.time_axis_sha256
            && weather.data.profile_id == study.weather.profile_id,
        "Energy weather normalization/time/profile differs"
    );
    ensure!(
        weather.data.location.latitude_deg_north == model.site.latitude_deg_north
            && weather.data.location.longitude_deg_east == model.site.longitude_deg_east
            && weather.data.location.elevation_m == model.site.elevation_m
            && weather.data.location.utc_offset_minutes == study.calendar.utc_offset_minutes,
        "Energy weather/model site or local standard offset differs"
    );
    let bytes = render_model(model, study)?;
    ensure!(
        bytes.len() as u64 <= MAX_FILE && normalized_epw.len() as u64 <= MAX_FILE,
        "Generated energy input exceeds the portable 64 MiB entry bound"
    );
    ensure!(
        !stage.exists() || fs::read_dir(stage)?.next().is_none(),
        "Energy stage must be fresh and empty"
    );
    fs::create_dir_all(stage.join("engine"))?;
    write_new(&stage.join("model.epJSON"), &bytes)?;
    write_new(&stage.join("recipe.json"), recipe_bytes())?;
    write_new(&stage.join("engine/weather.epw"), normalized_epw)?;
    let mut inputs = BTreeMap::new();
    for name in ["model.epJSON", "recipe.json", "engine/weather.epw"] {
        inputs.insert(name.into(), file_hash(&stage.join(name))?);
    }
    Ok(PreparedRun {
        method: method_identity(study)?,
        ledger: ledger(model)?,
        channels: channels(model),
        stage: stage.into(),
        model: model.clone(),
        study: study.clone(),
        inputs,
    })
}
/// Deterministic trusted renderer, also used for engine-free retained validation.
pub fn render_model(model: &ThermalModel, study: &EnergyStudy) -> Result<Vec<u8>> {
    study.validate(model)?;
    let bytes = serde_json::to_vec(&model::generate(model, study)?)?;
    ensure!(
        bytes.len() as u64 <= MAX_FILE,
        "Expanded schedules/model exceed portable epJSON size; simplify schedules or model"
    );
    Ok(bytes)
}
pub fn validate_retained_contract(
    model: &ThermalModel,
    study: &EnergyStudy,
    method: &EnergyMethodIdentity,
    actual_ledger: &MeterLedger,
    actual_channels: &[Channel],
    epjson: &[u8],
    recipe: &[u8],
) -> Result<()> {
    ensure!(
        method == &method_identity(study)?
            && actual_ledger == &ledger(model)?
            && actual_channels == channels(model)
            && recipe == recipe_bytes()
            && epjson == render_model(model, study)?,
        "Retained energy method, output map or owned model differs from compiled contract"
    );
    Ok(())
}
fn verify_inputs(prepared: &PreparedRun) -> Result<()> {
    for (name, hash) in &prepared.inputs {
        regular_file(&prepared.stage.join(name))?;
        ensure!(
            file_hash(&prepared.stage.join(name))? == *hash,
            "Energy input changed: {name}"
        );
    }
    Ok(())
}
pub fn execute<P, V>(
    prepared: &PreparedRun,
    config: &EngineConfig,
    cancel: &AtomicBool,
    mut progress: P,
    mut visitor: V,
) -> Result<ExecutionResult>
where
    P: FnMut(&str),
    V: FnMut(&EnergyInterval, &[f64]) -> Result<()>,
{
    config.verify()?;
    verify_inputs(prepared)?;
    let engine_members: Vec<_> = fs::read_dir(prepared.stage.join("engine"))?
        .map(|e| e.map(|e| e.file_name()))
        .collect::<std::io::Result<_>>()?;
    ensure!(engine_members.len() == 1 && engine_members[0] == "weather.epw", "Native stage must contain only the owned weather input before launch; INI, prior outputs and other files are prohibited");
    ensure!(!cancel.load(Ordering::SeqCst), "Energy run cancelled");
    progress("Executing the declared EnergyPlus weather year");
    let args = [
        "--jobs",
        "1",
        "--weather",
        "weather.epw",
        "--output-directory",
        ".",
        "--output-prefix",
        "eplus",
        "../model.epJSON",
    ]
    .map(String::from)
    .to_vec();
    let request = NativeRequest {
        executable: config.root.join("energyplus.exe"),
        arguments: args,
        directory: prepared.stage.join("engine"),
        child_environment: BTreeMap::from([("OMP_NUM_THREADS".into(), "1".into())]),
        child_environment_remove: vec![
            "PYTHONHOME".into(),
            "PYTHONPATH".into(),
            "ENERGYPLUS_WEATHER".into(),
            "ENERGYPLUS_OUTPUT".into(),
        ],
        limits: Limits {
            timeout: Duration::from_secs(1800),
            max_scratch_bytes: 1_073_741_824,
            max_file_bytes: 134_217_728,
            max_observed_working_set_bytes: 2_147_483_648,
        },
    };
    let process = greenlens_process::execute(&request, cancel)?;
    verify_inputs(prepared)?;
    config.verify()?;
    progress("Validating complete interval outputs and disjoint electricity meters");
    let result = output::read_outputs(prepared, cancel, &mut visitor);
    match result {
        Ok(diagnostics) => {
            write_new(
                &prepared.stage.join("diagnostics.json"),
                &serde_json::to_vec_pretty(&diagnostics)?,
            )?;
            Ok(ExecutionResult {
                process,
                diagnostics,
            })
        }
        Err(error) => {
            write_new(
                &prepared.stage.join("diagnostics.json"),
                &serde_json::to_vec_pretty(
                    &json!({"schema":"greenlens.energy-diagnostics.v1","completed":false,"error":format!("{error:#}")}),
                )?,
            )?;
            Err(error)
        }
    }
}
pub(crate) fn regular_file(path: &Path) -> Result<()> {
    let m = fs::symlink_metadata(path)
        .with_context(|| format!("Missing energy file {}", path.display()))?;
    ensure!(
        m.is_file() && !m.file_type().is_symlink(),
        "Energy input/output must be a regular file"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            m.file_attributes() & 0x400 == 0,
            "Energy input/output cannot be a reparse point"
        );
    }
    Ok(())
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn inputs() -> (ThermalModel, EnergyStudy) {
        let model = synthetic_four_zone_model(false);
        let data = weather::parse_energy_epw(&weather::synthetic_energy_epw())
            .unwrap()
            .data;
        let study = EnergyStudy {
            schema_version: ENERGY_STUDY_SCHEMA.into(),
            weather: data.binding(),
            calendar: synthetic_energy_calendar(),
            zone_timesteps_per_hour: 6,
        };
        (model, study)
    }
    #[test]
    fn owned_recipe_is_common_to_alternatives_and_timestep_identity_is_separate() {
        let (a, s) = inputs();
        let b = synthetic_four_zone_model(true);
        assert_ne!(render_model(&a, &s).unwrap(), render_model(&b, &s).unwrap());
        assert_eq!(ledger(&a).unwrap(), ledger(&b).unwrap());
        let method = method_identity(&s).unwrap();
        let mut five = s.clone();
        five.zone_timesteps_per_hour = 12;
        assert_eq!(
            method.recipe_sha256,
            method_identity(&five).unwrap().recipe_sha256
        );
        assert_ne!(
            method.numerical_settings_sha256,
            method_identity(&five).unwrap().numerical_settings_sha256
        );
        let bytes = render_model(&a, &s).unwrap();
        validate_retained_contract(
            &a,
            &s,
            &method,
            &ledger(&a).unwrap(),
            &channels(&a),
            &bytes,
            recipe_bytes(),
        )
        .unwrap();
        assert!(validate_retained_contract(
            &b,
            &s,
            &method,
            &ledger(&a).unwrap(),
            &channels(&a),
            &bytes,
            recipe_bytes()
        )
        .is_err());
    }
    #[test]
    fn exact_expanded_calendar_survives_native_compact_schedule_mapping() {
        let (m, s) = inputs();
        let doc: Value = serde_json::from_slice(&render_model(&m, &s).unwrap()).unwrap();
        for (i, schedule) in m.schedules.iter().enumerate() {
            let fields = doc["Schedule:Compact"][format!("SCH{:03}", i + 1)]["data"]
                .as_array()
                .unwrap();
            let mut expanded = Vec::new();
            let mut end_day = 0;
            let mut previous_day = 0;
            let mut profile = Vec::new();
            let mut until = 0;
            for item in fields {
                let field = &item["field"];
                if let Some(text) = field.as_str() {
                    if let Some(date) = text.strip_prefix("Through: ") {
                        if !profile.is_empty() {
                            assert_eq!(profile.len(), 144);
                            for _ in previous_day..end_day {
                                expanded.extend_from_slice(&profile)
                            }
                            previous_day = end_day;
                            profile.clear();
                        }
                        let d: Vec<u8> = date.split('/').map(|s| s.parse().unwrap()).collect();
                        end_day = MonthDay {
                            month: d[0],
                            day: d[1],
                        }
                        .day_index()
                        .unwrap()
                            + 1;
                    } else if let Some(time) = text.strip_prefix("Until: ") {
                        let v: Vec<usize> = time.split(':').map(|s| s.parse().unwrap()).collect();
                        until = (v[0] * 60 + v[1]) / 10;
                    } else {
                        assert!(text == "For: AllDays" || text == "Interpolate: No");
                    }
                } else {
                    profile.resize(until, field.as_f64().unwrap());
                }
            }
            assert_eq!(profile.len(), 144);
            for _ in previous_day..end_day {
                expanded.extend_from_slice(&profile)
            }
            assert_eq!(expanded, expand_schedule(schedule, &s.calendar).unwrap());
        }
    }
    #[test]
    fn rotated_world_faces_are_not_rotated_twice_and_ground_is_explicit() {
        let (mut m, s) = inputs();
        m.north_rotation_deg = 37.;
        let valid = validate_thermal_model(&m, &s.calendar).unwrap();
        let doc: Value = serde_json::from_slice(&render_model(&m, &s).unwrap()).unwrap();
        assert_eq!(doc["Building"]["GreenLens"]["north_axis"], 0);
        for (i, geom) in valid.surfaces.iter().enumerate() {
            let v = doc["BuildingSurface:Detailed"][format!("SUR{:03}", i + 1)]["vertices"]
                .as_array()
                .unwrap();
            for (j, p) in geom.vertices_m.iter().enumerate() {
                assert_eq!(v[j]["vertex_x_coordinate"].as_f64().unwrap(), p[0]);
                assert_eq!(v[j]["vertex_y_coordinate"].as_f64().unwrap(), p[1]);
            }
        }
        assert!(doc.get("ZoneHVAC:IdealLoadsAirSystem").is_none());
        assert!(doc.get("Material:NoMass").is_none());
        assert!(doc.get("SurfaceProperty:ConvectionCoefficients").is_none());
        assert_eq!(
            doc["RunPeriod"]["Annual"]["use_weather_file_rain_indicators"],
            "No"
        );
    }
    #[test]
    fn cooling_schedule_is_distinct_from_native_cutout_and_strictly_checked() {
        let (mut m, s) = inputs();
        // This regression deliberately retains the original nonzero-cutout
        // behavior independently of the current synthetic example defaults.
        for zone in &mut m.zones {
            zone.cooling_system.as_mut().unwrap().cooling_deadband_k = 0.2;
        }
        let decoder = Decoder::new(&m, &s).unwrap();
        let mut values = vec![0.; decoder.channels.len()];
        for (i, c) in decoder.channels.iter().enumerate() {
            if c.id.ends_with("setpoint_c") {
                values[i] = 24.;
            }
        }
        let schedule = decoder
            .channels
            .iter()
            .position(|c| c.id == "zone_002_setpoint_c")
            .unwrap();
        let operative = decoder
            .channels
            .iter()
            .position(|c| c.id == "zone_002_operative_setpoint_c")
            .unwrap();
        // Actual first annual failure was interval99/Z002: scheduled24, cutout23.8.
        // Native HVAC substep averaging can also report intermediate23.9.
        for cutout in [24., 23.9, 23.8] {
            values[operative] = cutout;
            let row = decoder.decode(99, &values).unwrap();
            assert_eq!(row.comfort[1].cooling_setpoint_c, 24.);
        }
        values[schedule] = 23.8;
        let error = decoder.decode(99, &values).unwrap_err().to_string();
        assert!(
            error.contains("interval 99")
                && error.contains(&m.zones[1].id)
                && error.contains("actual 23.8")
                && error.contains("expected 24")
        );
        values[schedule] = 24.;
        for wrong in [23.79, 24.01] {
            values[operative] = wrong;
            assert!(decoder.decode(99, &values).is_err());
        }
        let doc: Value = serde_json::from_slice(&render_model(&m, &s).unwrap()).unwrap();
        let schedules: Vec<_> = doc["Output:Variable"]
            .as_object()
            .unwrap()
            .values()
            .filter(|v| v["variable_name"] == "Schedule Value")
            .collect();
        assert_eq!(schedules.len(), 1, "shared schedule requested only once");
        for (i, _) in m.zones.iter().enumerate() {
            let source = &doc["ThermostatSetpoint:SingleCooling"]
                [format!("Z{:03} Setpoint", i + 1)]["setpoint_temperature_schedule_name"];
            assert_eq!(&schedules[0]["key_value"], source);
            assert_eq!(
                doc["ZoneControl:Thermostat"][format!("Z{:03} Thermostat", i + 1)]
                    ["temperature_difference_between_cutout_and_setpoint"],
                0.2
            );
        }
        for zone in &mut m.zones {
            zone.cooling_system.as_mut().unwrap().cooling_deadband_k = 0.;
        }
        let no_cutout = Decoder::new(&m, &s).unwrap();
        values[operative] = 23.9;
        assert!(no_cutout.decode(99, &values).is_err());
        values[operative] = 24.;
        assert!(no_cutout.decode(99, &values).is_ok());
    }
    #[test]
    fn wrong_ratings_and_retained_occupancy_reject() {
        let (mut m, s) = inputs();
        m.zones[0]
            .cooling_system
            .as_mut()
            .unwrap()
            .rated_outdoor_dry_bulb_c = 30.;
        assert!(render_model(&m, &s).is_err());
        let (m, s) = inputs();
        let decoder = Decoder::new(&m, &s).unwrap();
        let mut v = vec![0.; decoder.channels.len()];
        for (i, c) in decoder.channels.iter().enumerate() {
            if c.id.ends_with("setpoint_c") {
                v[i] = 24.;
            }
        }
        // Jan1 is the explicit zero-occupancy holiday in this original example.
        decoder.decode(0, &v).unwrap();
        let occ = decoder
            .channels
            .iter()
            .position(|c| c.id.ends_with("occupancy"))
            .unwrap();
        v[occ] = 0.5;
        assert!(decoder.decode(0, &v).is_err());
        v[occ] = 0.;
        v[0] = f64::NAN;
        assert!(decoder.decode(0, &v).is_err());
    }
}
