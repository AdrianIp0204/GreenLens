//! Portable climate metadata and evidence. Readers revalidate retained inputs;
//! recorded engine arguments are evidence, never executable import recipes.
use crate::{
    persistence::{
        bytes_hash, promote_run, promote_weather, read_json, regular_child, verify_seal, Checksums,
        MAX_EVIDENCE_FILE_BYTES,
    },
    timeseries::{SeriesReader, TimeSeriesManifest},
    weather::{self, WeatherAsset},
    Alternative,
};
use anyhow::{ensure, Context, Result};
use greenlens_core::{
    canonical_hash,
    study::{NumericalProfile, SensorMetrics, Study, StudyMode, WeatherBinding, HOURS_PER_YEAR},
    validate_model, Model, ValidatedModel,
};
use greenlens_radiance::{annual, ProcessRecord};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs::File,
    io::Read,
    path::Path,
};
use uuid::Uuid;

pub const PROJECT_SCHEMA: &str = "greenlens.climate-project.v0.3";
pub const RUN_SCHEMA: &str = "greenlens.climate-run.v0.3";
pub const ANNUAL_LABEL: &str =
    "Annual climate-based daylight illuminance and occupancy-weighted metrics.";
pub const HOUR_LABEL: &str = "Selected-hour daylight illuminance from imported weather.";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WeatherReference {
    pub id: String,
    pub path: String,
    pub binding: WeatherBinding,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClimateProject {
    pub schema_version: String,
    pub weather: Vec<WeatherReference>,
    pub studies: BTreeMap<String, Study>,
    pub runs: Vec<ClimateRunSummary>,
}
impl Default for ClimateProject {
    fn default() -> Self {
        Self {
            schema_version: PROJECT_SCHEMA.into(),
            weather: vec![],
            studies: ["A", "B"]
                .into_iter()
                .map(|id| (id.into(), Study::default()))
                .collect(),
            runs: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ClimateRunSummary {
    pub id: String,
    pub alternative_id: String,
    pub path: String,
    pub physical_input_hash: String,
    pub comparison_fingerprint: String,
    /// Bind the exact immutable result/provenance, independently of its inputs.
    pub record_sha256: String,
}

/// Deliberately contains no time-by-sensor matrix: use the bounded series reader.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClimateRunRecord {
    pub schema_version: String,
    pub id: String,
    pub alternative_id: String,
    pub model: Model,
    pub validated: ValidatedModel,
    pub study: Study,
    pub weather_asset: WeatherAsset,
    pub series: TimeSeriesManifest,
    pub metrics: Option<Vec<SensorMetrics>>,
    pub provenance: Value,
}

fn uuid(id: &str) -> Result<()> {
    ensure!(
        Uuid::parse_str(id)?.to_string() == id,
        "Climate identifiers must be canonical lowercase UUIDs"
    );
    Ok(())
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn alternative(id: &str) -> Result<()> {
    ensure!(["A", "B"].contains(&id), "Unknown climate alternative");
    Ok(())
}
fn binding(study: &Study) -> Option<&WeatherBinding> {
    match &study.calculation {
        StudyMode::StaticOvercast => None,
        StudyMode::WeatherHour { weather, .. } | StudyMode::Annual { weather, .. } => Some(weather),
    }
}
fn validate_reference(reference: &WeatherReference) -> Result<()> {
    uuid(&reference.id)?;
    ensure!(
        reference.path == format!("weather/{}", reference.id),
        "Invalid weather reference"
    );
    reference.binding.validate()?;
    Ok(())
}
fn validate_summary(summary: &ClimateRunSummary) -> Result<()> {
    uuid(&summary.id)?;
    alternative(&summary.alternative_id)?;
    ensure!(
        summary.path == format!("runs/{}", summary.id)
            && hash(&summary.physical_input_hash)
            && hash(&summary.comparison_fingerprint)
            && hash(&summary.record_sha256),
        "Invalid climate run reference or fingerprint"
    );
    Ok(())
}

pub fn validate_project_extension(
    project: &ClimateProject,
    alternatives: &[Alternative],
) -> Result<()> {
    validate_extension_references(project)?;
    ensure!(alternatives.len() == 2, "Expected alternatives A and B");
    for id in ["A", "B"] {
        let models = alternatives
            .iter()
            .filter(|a| a.id == id)
            .collect::<Vec<_>>();
        ensure!(models.len() == 1, "Expected one alternative {id}");
        let study = &project.studies[id];
        study.validate(&models[0].model)?;
    }
    Ok(())
}

fn validate_extension_references(project: &ClimateProject) -> Result<()> {
    ensure!(
        project.schema_version == PROJECT_SCHEMA,
        "Unsupported climate project version"
    );
    ensure!(
        project.studies.keys().map(String::as_str).eq(["A", "B"]),
        "Climate studies must define exactly A and B"
    );
    let mut ids = HashSet::new();
    for weather in &project.weather {
        validate_reference(weather)?;
        ensure!(
            ids.insert(&weather.id),
            "Duplicate project weather identifier"
        );
    }
    for study in project.studies.values() {
        // Full project validation also checks the actual alternative. Here the
        // default geometry exercises the schema, time and metric constraints.
        study.validate(&Model::default())?;
        if let Some(binding) = binding(study) {
            ensure!(
                project.weather.iter().any(|item| &item.binding == binding),
                "Current study weather is missing from portable project assets"
            );
        }
    }
    ids.clear();
    for run in &project.runs {
        validate_summary(run)?;
        ensure!(ids.insert(&run.id), "Duplicate climate run identifier");
    }
    Ok(())
}

pub fn run_fingerprint(record: &ClimateRunRecord) -> Result<String> {
    Ok(canonical_hash(&json!({
        "input": record.study.physical_input_hash(&record.model)?,
        "engine": record.provenance["engine"], "profile": record.provenance["profile"]
    }))?)
}
pub fn comparison_fingerprint(record: &ClimateRunRecord) -> Result<String> {
    Ok(canonical_hash(&json!({
        "conditions": record.study.comparison_input_hash(&record.model)?,
        "engine": record.provenance["engine"], "profile": record.provenance["profile"],
        "metrics": record.study.metrics
    }))?)
}
pub fn physical_result_fingerprint(record: &ClimateRunRecord) -> Result<String> {
    Ok(canonical_hash(&json!({
        "run_fingerprint": run_fingerprint(record)?, "series_identity": record.series.identity()?
    }))?)
}

fn utc(text: &Value) -> Result<chrono::DateTime<chrono::FixedOffset>> {
    let time =
        chrono::DateTime::parse_from_rfc3339(text.as_str().context("Missing UTC timestamp")?)?;
    ensure!(
        time.offset().local_minus_utc() == 0,
        "Timestamp must be UTC"
    );
    Ok(time)
}
/// Validate metadata only. Finalization and imports additionally call read_run_directory.
pub fn validate_record(record: &ClimateRunRecord) -> Result<()> {
    uuid(&record.id)?;
    alternative(&record.alternative_id)?;
    ensure!(
        record.schema_version == RUN_SCHEMA,
        "Unsupported climate run version"
    );
    record.study.validate(&record.model)?;
    let weather =
        binding(&record.study).context("Static evidence uses the unchanged legacy run contract")?;
    let validated = validate_model(&record.model)?;
    ensure!(
        canonical_hash(&validated)? == canonical_hash(&record.validated)?,
        "Stored climate geometry differs from authoritative model"
    );
    ensure!(
        record.weather_asset.schema_version == "greenlens.weather-asset.v0.3"
            && &record.weather_asset.binding == weather,
        "Study and retained weather identity differ"
    );
    record.weather_asset.source.validate()?;
    record.series.validate()?;
    ensure!(
        record
            .series
            .sensor_ids
            .iter()
            .map(String::as_str)
            .eq(validated.sensor_grid.sensors.iter().map(|s| s.id.as_str())),
        "Climate sensor IDs/order differ from authoritative geometry"
    );
    let annual = matches!(record.study.calculation, StudyMode::Annual { .. });
    ensure!(
        record.series.interval_count == if annual { HOURS_PER_YEAR } else { 1 },
        "Study and retained interval shape differ"
    );
    if annual {
        let metrics = record
            .metrics
            .as_ref()
            .context("Completed annual evidence needs metrics")?;
        ensure!(
            metrics.len() == record.series.sensor_ids.len(),
            "Metric sensor shape differs"
        );
        for m in metrics {
            ensure!(
                [
                    m.da_percent,
                    m.cda_percent,
                    m.udi_percent,
                    m.underlit_percent,
                    m.overlit_percent
                ]
                .iter()
                .all(|v| v.is_finite() && (0.0..=100.0 + 1e-9).contains(v)),
                "Invalid annual metric values"
            );
            ensure!(
                (m.udi_percent + m.underlit_percent + m.overlit_percent - 100.0).abs() <= 1e-9,
                "UDI partition is incomplete"
            );
        }
    } else {
        ensure!(
            record.metrics.is_none(),
            "Selected-hour evidence cannot contain annual metrics"
        );
    }
    let p = &record.provenance;
    ensure!(
        p["schema_version"] == RUN_SCHEMA
            && p["state"] == "completed"
            && p["units"] == "lx"
            && p["calculation_label"] == if annual { ANNUAL_LABEL } else { HOUR_LABEL },
        "Incomplete climate provenance"
    );
    ensure!(
        p["application_version"]
            .as_str()
            .is_some_and(|v| !v.trim().is_empty()
                && v.len() <= 120
                && !v.chars().any(char::is_control))
            && p["adapter_version"] == annual::ADAPTER_VERSION,
        "Missing application/adapter version provenance"
    );
    ensure!(
        p["child_environment"] == json!({"RAYPATH":".;<compatible-engine>/lib"}),
        "Missing or unsupported portable child environment provenance"
    );
    ensure!(
        p["input_hash"] == validated.normalized_input_hash
            && p["sensor_hash"] == validated.sensor_hash
            && p["physical_input_hash"] == record.study.physical_input_hash(&record.model)?
            && p["time_axis_sha256"] == weather.time_axis_sha256,
        "Climate input, sensor or time-axis provenance mismatch"
    );
    let engine: annual::AnnualEngineIdentity = serde_json::from_value(p["engine"].clone())?;
    engine.validate()?;
    let profile: annual::AnnualProfile = serde_json::from_value(p["profile"].clone())?;
    profile.validate()?;
    let selected_profile = match record.study.calculation {
        StudyMode::WeatherHour { profile, .. } | StudyMode::Annual { profile, .. } => profile,
        StudyMode::StaticOvercast => unreachable!("rejected above"),
    };
    ensure!(
        profile
            == match selected_profile {
                NumericalProfile::Shipping => annual::AnnualProfile::shipping(),
                NumericalProfile::Reference => annual::AnnualProfile::reference(),
            },
        "Study and engine numerical profile differ"
    );
    let start = utc(&p["started_utc"])?;
    let finish = utc(&p["finished_utc"])?;
    ensure!(finish >= start, "Run finished before it started");
    let processes: Vec<ProcessRecord> = serde_json::from_value(p["processes"].clone())?;
    ensure!(!processes.is_empty(), "Missing climate process provenance");
    let mut previous = start;
    for process in &processes {
        let a = utc(&json!(process.started_utc))?;
        let b = utc(&json!(process.finished_utc))?;
        ensure!(
            process.exit_code == 0 && a >= previous && b >= a && b <= finish,
            "Failed or invalid climate process chronology"
        );
        previous = b;
    }
    ensure!(
        p["run_fingerprint"] == run_fingerprint(record)?
            && p["comparison_fingerprint"] == comparison_fingerprint(record)?
            && p["physical_result_fingerprint"] == physical_result_fingerprint(record)?,
        "Climate run/result/comparison fingerprint mismatch"
    );
    let metric_identity = if annual {
        json!(record
            .study
            .metrics
            .identity(&physical_result_fingerprint(record)?)?)
    } else {
        Value::Null
    };
    ensure!(
        p["metric_identity"] == metric_identity,
        "Metric settings/result identity mismatch"
    );
    Ok(())
}

pub fn summary(record: &ClimateRunRecord) -> Result<ClimateRunSummary> {
    validate_record(record)?;
    Ok(ClimateRunSummary {
        id: record.id.clone(),
        alternative_id: record.alternative_id.clone(),
        path: format!("runs/{}", record.id),
        physical_input_hash: record.study.physical_input_hash(&record.model)?,
        comparison_fingerprint: comparison_fingerprint(record)?,
        record_sha256: canonical_hash(record)?,
    })
}
pub fn append_run(project: &mut ClimateProject, record: &ClimateRunRecord) -> Result<()> {
    let candidate = summary(record)?;
    if let Some(existing) = project.runs.iter().find(|r| r.id == record.id) {
        ensure!(existing == &candidate, "Climate run identifier conflict");
    } else {
        project.runs.push(candidate);
    }
    Ok(())
}

fn bytes(root: &Path, name: &str) -> Result<Vec<u8>> {
    let file = File::open(regular_child(root, name)?)?;
    ensure!(
        file.metadata()?.len() <= MAX_EVIDENCE_FILE_BYTES,
        "Climate evidence file exceeds bound"
    );
    let mut data = Vec::new();
    file.take(MAX_EVIDENCE_FILE_BYTES + 1)
        .read_to_end(&mut data)?;
    ensure!(
        data.len() as u64 <= MAX_EVIDENCE_FILE_BYTES,
        "Climate evidence grew beyond bound"
    );
    Ok(data)
}

/// All known retained members; callers may seal exactly this list after writing.
pub fn evidence_files(record: &ClimateRunRecord) -> Vec<String> {
    let mut files = [
        "results.json",
        "input.json",
        "study.json",
        "provenance.json",
        "annual-scene.rad",
        "annual-sensors.pts",
        "annual-weather.epw",
        "annual-receiver.rad",
        "annual-suns.rad",
        "annual-sun-modifiers.txt",
        "annual-recipe.json",
        "annual-logs.json",
        "annual-resources.json",
        "series/series.json",
        "weather/asset.json",
        "weather/weather.json",
        "weather/original.epw",
        "weather/normalized.epw",
        "weather/checksums.json",
    ]
    .map(String::from)
    .to_vec();
    files.extend(
        record
            .series
            .chunks
            .iter()
            .map(|c| format!("series/{}", c.file)),
    );
    files
}

fn expected_processes(
    record: &ClimateRunRecord,
    recipe: &annual::AnnualRecipe,
) -> Vec<(String, Vec<String>)> {
    let strings = |v: &[&str]| v.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
    let ground = record.model.sky.ground_reflectance.to_string();
    let mut result = vec![
        (
            "gendaymtx.exe".into(),
            vec![
                "-O0".into(),
                "-m".into(),
                recipe.profile.sky_basis_mf.to_string(),
                "-c".into(),
                "1".into(),
                "1".into(),
                "1".into(),
                "-g".into(),
                ground.clone(),
                ground.clone(),
                ground,
                "-s".into(),
                "annual-weather.epw".into(),
            ],
        ),
        (
            "gendaymtx.exe".into(),
            strings(&[
                "-O0",
                "-n",
                "-D",
                "annual-suns.rad",
                "-M",
                "annual-sun-modifiers.txt",
                "annual-weather.epw",
            ]),
        ),
        (
            "oconv.exe".into(),
            strings(&["-f", "annual-scene.rad", "annual-receiver.rad"]),
        ),
    ];
    let mut diffuse = recipe.profile.arguments(recipe.sensor_count);
    diffuse.extend([
        "-V-".into(),
        "-e".into(),
        format!("MF:{}", recipe.profile.sky_basis_mf),
        "-f".into(),
        "reinhart.cal".into(),
        "-b".into(),
        "rbin".into(),
        "-bn".into(),
        "Nrbins".into(),
        "-m".into(),
        "sky_glow".into(),
        "annual-diffuse.oct".into(),
    ]);
    result.push(("rcontrib.exe".into(), diffuse));
    result.push((
        "dctimestep.exe".into(),
        strings(&["-oa", "annual-coefficients.mtx", "annual-sky.mtx"]),
    ));
    if !recipe.solar_interval_indices.is_empty() {
        result.push((
            "oconv.exe".into(),
            strings(&["-f", "annual-scene.rad", "annual-suns.rad"]),
        ));
        let mut solar = recipe.profile.solar_arguments(recipe.sensor_count);
        solar.extend(strings(&[
            "-V+",
            "-M",
            "annual-sun-modifiers.txt",
            "annual-solar.oct",
        ]));
        result.push(("rcontrib.exe".into(), solar));
    }
    result
}

/// Zero retained DNI and DHI must produce exact zero in this passive daylight
/// model. This is an input/output invariant, not an astronomical night test.
/// Call only with weather reparsed from retained bytes; shape/finite checks are
/// separately enforced by the worker or series reader before acceptance.
pub(crate) fn validate_zero_solar_output(
    weather: &greenlens_core::weather::WeatherData,
    interval: usize,
    values: impl IntoIterator<Item = f64>,
) -> Result<()> {
    let hour = weather
        .hours
        .get(interval)
        .context("Output weather interval is missing")?;
    ensure!(
        hour.index == interval,
        "Output weather interval mapping differs"
    );
    if hour.direct_normal_wh_m2 == 0.0 && hour.diffuse_horizontal_wh_m2 == 0.0 {
        ensure!(values.into_iter().all(|value| value == 0.0), "Nonzero daylight output for zero DNI/DHI at weather interval {interval}; result was not accepted or changed");
    }
    Ok(())
}

pub fn read_run_directory(dir: &Path) -> Result<ClimateRunRecord> {
    // Reject unknown/excess members before reading any potentially large file.
    let sums: Checksums = read_json(&regular_child(dir, "checksums.json")?)?;
    let record: ClimateRunRecord = read_json(&regular_child(dir, "results.json")?)?;
    validate_record(&record)?;
    let expected: BTreeSet<_> = evidence_files(&record).into_iter().collect();
    ensure!(
        sums.schema_version == "0.1.0"
            && sums.sha256.keys().cloned().collect::<BTreeSet<_>>() == expected
            && sums.sha256.values().all(|s| hash(s)),
        "Climate evidence seal does not cover exactly the required members"
    );
    verify_seal(dir)?;
    ensure!(
        read_json::<Value>(&regular_child(dir, "input.json")?)?
            == serde_json::to_value(&record.model)?
            && read_json::<Value>(&regular_child(dir, "study.json")?)?
                == serde_json::to_value(&record.study)?,
        "Climate input/study snapshot mismatch"
    );
    ensure!(
        read_json::<Value>(&regular_child(dir, "provenance.json")?)? == record.provenance,
        "Climate provenance snapshot mismatch"
    );
    let (asset, weather) = weather::open(&dir.join("weather"))?;
    ensure!(
        asset == record.weather_asset,
        "Embedded weather differs from run snapshot"
    );
    let reader = SeriesReader::open(&dir.join("series"))?;
    ensure!(
        reader.manifest() == &record.series,
        "Climate series manifest differs from run snapshot"
    );
    reader.visit_hours(|position, values| {
        let interval = match record.study.calculation {
            StudyMode::WeatherHour { interval_index, .. } => interval_index,
            _ => position,
        };
        validate_zero_solar_output(&weather.data, interval, values.iter().flatten().copied())
    })?;
    if let Some(stored) = &record.metrics {
        let derived = reader.metrics(&record.study.metrics)?;
        for (a, b) in stored.iter().zip(derived) {
            ensure!(
                [
                    a.da_percent - b.da_percent,
                    a.cda_percent - b.cda_percent,
                    a.udi_percent - b.udi_percent,
                    a.underlit_percent - b.underlit_percent,
                    a.overlit_percent - b.overlit_percent
                ]
                .iter()
                .all(|d| d.abs() <= 1e-9),
                "Stored metrics differ from retained illuminance/settings"
            );
        }
    }
    let recipe: annual::AnnualRecipe = read_json(&regular_child(dir, "annual-recipe.json")?)?;
    ensure!(
        recipe.schema_version == "greenlens.annual-recipe.v1"
            && recipe.method_version == annual::METHOD_VERSION,
        "Unsupported annual recipe"
    );
    ensure!(
        recipe.ordering == annual::RESULT_ORDERING
            && recipe.units == annual::RESULT_UNITS
            && recipe.time_policy == annual::TIME_POLICY
            && recipe.ground_policy == annual::GROUND_POLICY
            && bytes(dir, "annual-receiver.rad")? == annual::RECEIVER_TEXT.as_bytes(),
        "Unsupported receiver, units, ordering or climate time/ground policy"
    );
    ensure!(
        serde_json::to_value(&recipe.engine)? == record.provenance["engine"]
            && serde_json::to_value(&recipe.profile)? == record.provenance["profile"]
            && serde_json::to_value(&recipe.processes)? == record.provenance["processes"]
            && read_json::<Value>(&regular_child(dir, "annual-logs.json")?)?
                == record.provenance["processes"],
        "Annual recipe/logs and provenance disagree"
    );
    let selected = match record.study.calculation {
        StudyMode::WeatherHour { interval_index, .. } => Some(interval_index),
        _ => None,
    };
    let indices = selected.map_or_else(|| (0..HOURS_PER_YEAR).collect(), |i| vec![i]);
    let (generated, context_indices) = annual::weather_window(&weather.normalized_epw, selected)?;
    ensure!(
        recipe.interval_indices == indices
            && recipe.generated_weather_interval_indices == context_indices
            && recipe.sensor_count == record.series.sensor_ids.len()
            && recipe.sky_rows
                == 144 * recipe.profile.sky_basis_mf * recipe.profile.sky_basis_mf + 2,
        "Recipe time, sensor or sky shape mismatch"
    );
    ensure!(
        recipe.normalized_weather_sha256 == asset.binding.normalized_sha256
            && recipe.generated_weather_sha256 == bytes_hash(&generated)
            && bytes(dir, "annual-weather.epw")? == generated,
        "Generated weather does not match retained source interpretation"
    );
    let scene = greenlens_radiance::scene_text(&record.validated)?;
    let sensors = record
        .validated
        .sensor_grid
        .sensors
        .iter()
        .map(|s| {
            format!(
                "{} {} {} {} {} {}\n",
                s.position[0],
                s.position[1],
                s.position[2],
                s.direction[0],
                s.direction[1],
                s.direction[2]
            )
        })
        .collect::<String>();
    ensure!(
        recipe.scene_sha256 == bytes_hash(scene.as_bytes())
            && recipe.sensor_sha256 == bytes_hash(sensors.as_bytes())
            && bytes(dir, "annual-scene.rad")? == scene.as_bytes()
            && bytes(dir, "annual-sensors.pts")? == sensors.as_bytes(),
        "Scene/sensors differ from authoritative geometry"
    );
    let modifier_bytes = bytes(dir, "annual-sun-modifiers.txt")?;
    let mut sun_indices = Vec::new();
    for name in std::str::from_utf8(&modifier_bytes)?
        .lines()
        .filter(|s| !s.trim().is_empty())
    {
        let minute: usize = name
            .trim()
            .strip_prefix("solar")
            .context("Unknown solar modifier")?
            .parse()?;
        ensure!(
            minute >= 30 && (minute - 30).is_multiple_of(60),
            "Invalid solar interval midpoint"
        );
        let index = (minute - 30) / 60;
        ensure!(
            context_indices.binary_search(&index).is_ok()
                && sun_indices.last().is_none_or(|previous| *previous < index),
            "Invalid/duplicate solar interval order"
        );
        sun_indices.push(index);
    }
    ensure!(
        sun_indices == recipe.solar_interval_indices,
        "Solar modifier mapping differs from recipe"
    );
    let expected = expected_processes(&record, &recipe);
    ensure!(
        recipe.processes.len() == expected.len()
            && recipe
                .processes
                .iter()
                .zip(expected)
                .all(|(p, (exe, args))| p.executable == exe && p.arguments == args),
        "Recorded commands differ from recognized adapter recipe"
    );
    let resources: annual::AnnualResources =
        read_json(&regular_child(dir, "annual-resources.json")?)?;
    resources.validate()?;
    ensure!(
        resources.processes.len() == recipe.processes.len()
            && resources
                .processes
                .iter()
                .zip(&recipe.processes)
                .all(|(m, p)| m.executable == p.executable && m.started_utc == p.started_utc)
            && utc(&json!(resources.started_utc))? >= utc(&record.provenance["started_utc"])?
            && utc(&json!(resources.finished_utc))? <= utc(&record.provenance["finished_utc"])?,
        "Resource observations do not correspond to retained processes/run"
    );
    ensure!(
        recipe.child_environment == BTreeMap::from([("RAYPATH".into(), ".;<engine>/lib".into())]),
        "Unexpected child environment configuration"
    );
    Ok(record)
}

pub fn read_run(root: &Path, summary: &ClimateRunSummary) -> Result<ClimateRunRecord> {
    validate_summary(summary)?;
    regular_child(root, &format!("{}/checksums.json", summary.path))?;
    let record = read_run_directory(&root.join(&summary.path))?;
    ensure!(
        self::summary(&record)? == *summary,
        "Climate project summary differs from retained run"
    );
    Ok(record)
}

/// Bounded display query after full project open/import verification. This checks
/// the sealed record and series metadata, not every retained weather/chunk file.
/// It must never replace read_run when accepting or finalizing evidence.
pub fn read_result_metadata(root: &Path, summary: &ClimateRunSummary) -> Result<ClimateRunRecord> {
    validate_summary(summary)?;
    let seal_path = regular_child(root, &format!("{}/checksums.json", summary.path))?;
    let sums: Checksums = read_json(&seal_path)?;
    ensure!(sums.schema_version == "0.1.0", "Unsupported climate seal");
    let dir = root.join(&summary.path);
    let result_bytes = bytes(&dir, "results.json")?;
    ensure!(
        sums.sha256.get("results.json") == Some(&bytes_hash(&result_bytes)),
        "Climate metadata checksum mismatch"
    );
    let record: ClimateRunRecord = serde_json::from_slice(&result_bytes)?;
    validate_record(&record)?;
    let expected: BTreeSet<_> = evidence_files(&record).into_iter().collect();
    ensure!(
        sums.sha256.keys().cloned().collect::<BTreeSet<_>>() == expected
            && sums.sha256.values().all(|s| hash(s)),
        "Climate metadata seal coverage or hashes are invalid"
    );
    ensure!(
        self::summary(&record)? == *summary,
        "Climate project summary differs from retained run"
    );
    let series_bytes = bytes(&dir, "series/series.json")?;
    ensure!(
        sums.sha256.get("series/series.json") == Some(&bytes_hash(&series_bytes)),
        "Climate series metadata checksum mismatch"
    );
    let series: TimeSeriesManifest = serde_json::from_slice(&series_bytes)?;
    ensure!(
        series == record.series,
        "Series metadata differs from result snapshot"
    );
    Ok(record)
}

/// The reader validates only requested chunks, keeping hourly IPC bounded.
pub fn open_series_for_run(
    root: &Path,
    summary: &ClimateRunSummary,
) -> Result<(ClimateRunRecord, SeriesReader)> {
    let record = read_result_metadata(root, summary)?;
    let reader = SeriesReader::open(&root.join(&summary.path).join("series"))?;
    ensure!(
        reader.manifest() == &record.series,
        "Series changed during display query"
    );
    Ok((record, reader))
}

pub fn verify_project_evidence(root: &Path, project: &ClimateProject) -> Result<()> {
    validate_extension_references(project)?;
    for reference in &project.weather {
        regular_child(root, &format!("{}/checksums.json", reference.path))?;
        let (asset, _) = weather::open(&root.join(&reference.path))?;
        ensure!(
            asset.binding == reference.binding,
            "Project weather reference differs from retained asset"
        );
    }
    for summary in &project.runs {
        read_run(root, summary)?;
    }
    Ok(())
}

/// Caller owns source/destination project locks. Source revision is not required
/// for nondestructive Save As, but every referenced immutable asset is verified.
pub fn copy_project_evidence(
    source: &Path,
    destination: &Path,
    project: &ClimateProject,
) -> Result<()> {
    verify_project_evidence(source, project)?;
    for reference in &project.weather {
        promote_weather(&source.join(&reference.path), destination, &reference.id)?;
    }
    for summary in &project.runs {
        promote_run(&source.join(&summary.path), destination, &summary.id)?;
    }
    verify_project_evidence(destination, project)
}

/// Copy an already imported/verified weather asset while holding the project lock.
pub fn import_project_weather(
    source: &Path,
    destination: &Path,
    id: &str,
) -> Result<WeatherReference> {
    uuid(id)?;
    let (asset, _) = weather::open(source)?;
    promote_weather(source, destination, id)?;
    let reference = WeatherReference {
        id: id.into(),
        path: format!("weather/{id}"),
        binding: asset.binding,
    };
    let (copied, _) = weather::open(&destination.join(&reference.path))?;
    ensure!(
        copied.binding == reference.binding,
        "Copied weather identity mismatch"
    );
    Ok(reference)
}

#[cfg(test)]
#[path = "climate_tests.rs"]
pub(crate) mod tests;
