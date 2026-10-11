//! Native fixed-geometry daylight coefficients and explicit solar transport.
//! The ground is a luminous boundary only in the diffuse/ground coefficient
//! path. The solar octree has no external ground surface or luminous dome.
#[path = "annual_direct.rs"]
mod direct;
#[path = "annual_matrix.rs"]
mod matrix;
pub use direct::{DirectBatch, DIRECT_POLICY, MAX_DIRECT_BATCH_RAYS};

pub use crate::process::AnnualProcessResources;
use crate::{digest, process, scene_text, Engine, ProcessRecord, ENGINE_VERSION};
use anyhow::{ensure, Context, Result};
use chrono::Utc;
use greenlens_core::ValidatedModel;
use matrix::MatrixReader;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

pub const LEGACY_ADAPTER_VERSION: &str = "radiance-climate-direct-corrected/2";
pub const LEGACY_METHOD_VERSION: &str =
    "perez-epw-dewpoint3-reinhart-patch-indirect-finite-direct/2";
pub const LEGACY_RECIPE_SCHEMA_VERSION: &str = "greenlens.annual-recipe.v2";
pub const ADAPTER_VERSION: &str = "radiance-climate-explicit-indirect-finite-direct/3";
pub const METHOD_VERSION: &str = "perez-epw-dewpoint3-reinhart-explicit-indirect-finite-direct/3";
pub const RECIPE_SCHEMA_VERSION: &str = "greenlens.annual-recipe.v3";
pub const MAX_SENSORS: usize = 256;
pub const HOURS: usize = 8760;

fn solar_generation_arguments(profile: &AnnualProfile, legacy: bool) -> Vec<String> {
    let mut arguments = vec!["-O0".into()];
    if legacy {
        arguments.extend(["-m".into(), profile.sky_basis_mf.to_string(), "-d".into()]);
    } else {
        arguments.push("-n".into());
    }
    arguments.extend(
        [
            "-D",
            "annual-suns.rad",
            "-M",
            "annual-sun-modifiers.txt",
            "annual-weather.epw",
        ]
        .map(String::from),
    );
    arguments
}
pub const MAX_SCRATCH_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const RECEIVER_TEXT: &str = "void glow sky_glow\n0\n0\n4 1 1 1 0\nsky_glow source upper_sky\n0\n0\n4 0 0 1 180\nsky_glow source lower_ground\n0\n0\n4 0 0 -1 180\n";
pub const RESULT_ORDERING: &str = "time_sensor_RGB; engine matrices sensor_rows_time_columns_RGB; sky patch_rows_time_columns_RGB";
pub const RESULT_UNITS: &str = "visible Radiance RGB irradiance W/m2; illuminance=179*(0.265074126R+0.670114631G+0.064811243B) lx";
pub const TIME_POLICY: &str = "EPW hourly midpoint hour-0.5, fixed standard time; no -u night removal or -i horizon adjustment; selected hour retains preceding two dew-point records; sun exported when any part of0.533-degree disc is above horizon";
pub const LEGACY_GROUND_POLICY: &str = "Uniform luminous ground includes diffuse and direct-horizontal reflected radiance in the -s sky matrix once; -d solar matrix has zero ground and diffuse sky. Reflected solar is full minus direct-only patch transport, gated to emitted solar-source intervals; no external ground geometry.";
pub const GROUND_POLICY: &str = "Uniform luminous ground includes diffuse and direct-horizontal reflected radiance in the -s sky matrix once. Solar-only explicit source transport has no luminous dome or external ground geometry. Reflected solar is full explicit-source transport minus identical ab0 centroid-direct transport; finite-disc directional transmission replaces only direct sun. The emitted-source horizon policy applies to both solar terms.";
pub const RESOURCE_MEASUREMENT_POLICY: &str = "Native Windows GetProcessMemoryInfo: lifetime peak working set of each engine child; simultaneous runner+child working sets sampled every20ms (shared pages counted in both processes); complete owned scratch tree file bytes sampled during each child, after assembly, and after retained series creation. Observed working-set sum is not total private memory. One worker; no engine subprocess descendants expected.";
const EXECUTABLES: [(&str, &str); 5] = [
    (
        "rtrace.exe",
        "edb01ac56b31ad79263c1f42a686413c0d369829f460e83a170340a6f821b5c9",
    ),
    (
        "gendaymtx.exe",
        "07a022f4569c7815bba868148add3706aa0e120622f0e8e7c7029e7947af527d",
    ),
    (
        "rcontrib.exe",
        "0e622eb168ce5ba30bfad2d154bf64bc4555d1663f1d20c9c1a833f947816975",
    ),
    (
        "dctimestep.exe",
        "ccd4ee7b89c73f752419642dc10f93e13b6c75b407f0e12775f5056ec7f1ab05",
    ),
    (
        "oconv.exe",
        "e0bc7240ee2645d6590c33ef5a6a783f4bfe81246dce23bfefdc66966cba60a2",
    ),
];
const LIBRARIES: [(&str, &str); 3] = [
    (
        "lib/rayinit.cal",
        "293b321513e9fedce90ad6e88f2ef00d6e9184d0581e8fdeb111fd05fa32ae60",
    ),
    (
        "lib/skybright.cal",
        "323d4762faa32b6a0ee086d97df6b04ba39416385af927247265de6d428fe454",
    ),
    (
        "lib/reinhart.cal",
        "1517d597e34673bd8c5cfcea79850520ac272c7f5229df22401537f0b7c82934",
    ),
];
const PACKAGE_SHA256: &str = "c4f02efcb4a33f4a43b7ce0ba58100544abc317a1be61f7b0425a64a66eb8a09";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AnnualProfile {
    pub name: String,
    pub sky_basis_mf: usize,
    pub ambient_bounces: u32,
    pub ambient_divisions: u32,
    pub direct_ambient_divisions: u32,
    pub direct_solar_samples: usize,
    pub ambient_supersamples: u32,
    pub ambient_accuracy: f64,
    pub ambient_resolution: u32,
    pub ambient_value: [f64; 3],
    pub limit_weight: f64,
    pub ray_depth: u32,
    pub direct_threshold: f64,
    pub direct_certainty: f64,
    pub direct_jitter: f64,
    pub direct_size: f64,
    pub specular_threshold: f64,
    pub specular_sampling: f64,
    pub uncorrelated_random_sampling: bool,
    pub workers: usize,
    /// Absent in immutable method-2 profiles. Omission preserves their exact
    /// canonical serialization; method-3 profiles bind explicit solar sampling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solar_ambient_divisions: Option<u32>,
}
impl AnnualProfile {
    /// Candidate numerical settings; release freeze requires the recorded suite.
    pub fn shipping() -> Self {
        Self {
            ambient_divisions: 1_048_576,
            direct_ambient_divisions: 1_048_576,
            solar_ambient_divisions: Some(8192),
            ..Self::legacy_shipping()
        }
    }
    fn legacy_shipping() -> Self {
        Self {
            name: "shipping".into(),
            sky_basis_mf: 2,
            ambient_bounces: 8,
            ambient_divisions: 131072,
            direct_ambient_divisions: 131072,
            direct_solar_samples: 1024,
            ambient_supersamples: 0,
            ambient_accuracy: 0.0,
            ambient_resolution: 256,
            ambient_value: [0.0; 3],
            limit_weight: 1e-9,
            ray_depth: 16,
            direct_threshold: 0.0,
            direct_certainty: 1.0,
            direct_jitter: 0.0,
            direct_size: 0.05,
            specular_threshold: 0.0,
            specular_sampling: 0.0,
            uncorrelated_random_sampling: false,
            workers: 1,
            solar_ambient_divisions: None,
        }
    }
    pub fn reference() -> Self {
        Self {
            name: "reference".into(),
            sky_basis_mf: 4,
            ambient_bounces: 9,
            ambient_divisions: 2_097_152,
            direct_ambient_divisions: 2_097_152,
            direct_solar_samples: 4096,
            solar_ambient_divisions: Some(16384),
            ..Self::shipping()
        }
    }
    fn legacy_reference() -> Self {
        Self {
            name: "reference".into(),
            sky_basis_mf: 4,
            ambient_bounces: 9,
            ambient_divisions: 262144,
            direct_ambient_divisions: 262144,
            direct_solar_samples: 4096,
            ..Self::legacy_shipping()
        }
    }
    pub fn shipping_for_method(method: &str) -> Result<Self> {
        match method {
            METHOD_VERSION => Ok(Self::shipping()),
            LEGACY_METHOD_VERSION => Ok(Self::legacy_shipping()),
            _ => anyhow::bail!("Unsupported climate transport method"),
        }
    }
    pub fn reference_for_method(method: &str) -> Result<Self> {
        match method {
            METHOD_VERSION => Ok(Self::reference()),
            LEGACY_METHOD_VERSION => Ok(Self::legacy_reference()),
            _ => anyhow::bail!("Unsupported climate transport method"),
        }
    }
    pub fn method(&self) -> Result<&'static str> {
        for method in [METHOD_VERSION, LEGACY_METHOD_VERSION] {
            if self == &Self::shipping_for_method(method)?
                || self == &Self::reference_for_method(method)?
            {
                return Ok(method);
            }
        }
        anyhow::bail!(
            "Unsupported climate numerical profile; replay requires a recognized complete profile"
        )
    }
    pub fn validate_for_method(&self, method: &str) -> Result<()> {
        ensure!(
            self.method()? == method,
            "Profile and transport method differ"
        );
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        self.method()?;
        Ok(())
    }
    pub fn arguments(&self, sensors: usize) -> Vec<String> {
        [
            "-I+".into(),
            "-n".into(),
            self.workers.to_string(),
            "-faa".into(),
            "-y".into(),
            sensors.to_string(),
            "-ab".into(),
            self.ambient_bounces.to_string(),
            "-ad".into(),
            self.ambient_divisions.to_string(),
            "-as".into(),
            self.ambient_supersamples.to_string(),
            "-aa".into(),
            self.ambient_accuracy.to_string(),
            "-ar".into(),
            self.ambient_resolution.to_string(),
            "-av".into(),
            self.ambient_value[0].to_string(),
            self.ambient_value[1].to_string(),
            self.ambient_value[2].to_string(),
            "-lw".into(),
            self.limit_weight.to_string(),
            "-lr".into(),
            self.ray_depth.to_string(),
            "-dt".into(),
            self.direct_threshold.to_string(),
            "-dc".into(),
            self.direct_certainty.to_string(),
            "-dj".into(),
            self.direct_jitter.to_string(),
            "-ds".into(),
            self.direct_size.to_string(),
            "-st".into(),
            self.specular_threshold.to_string(),
            "-ss".into(),
            self.specular_sampling.to_string(),
            if self.uncorrelated_random_sampling {
                "-u+".into()
            } else {
                "-u-".into()
            },
        ]
        .to_vec()
    }
    pub fn direct_arguments(&self, sensors: usize) -> Vec<String> {
        let mut arguments = self.arguments(sensors);
        let position = arguments
            .iter()
            .position(|argument| argument == "-ad")
            .expect("complete profile includes -ad");
        arguments[position + 1] = self.direct_ambient_divisions.to_string();
        let position = arguments
            .iter()
            .position(|argument| argument == "-ab")
            .expect("complete profile includes -ab");
        arguments[position + 1] = "1".into();
        arguments
    }
    pub fn solar_arguments(&self, sensors: usize, direct: bool) -> Vec<String> {
        let mut arguments = self.arguments(sensors);
        let position = arguments
            .iter()
            .position(|argument| argument == "-ad")
            .expect("complete profile includes -ad");
        arguments[position + 1] = self
            .solar_ambient_divisions
            .expect("explicit solar arguments require a validated method-3 profile")
            .to_string();
        if direct {
            let position = arguments
                .iter()
                .position(|argument| argument == "-ab")
                .expect("complete profile includes -ab");
            arguments[position + 1] = "0".into();
        }
        arguments.extend(
            ["-V+", "-M", "annual-sun-modifiers.txt", "annual-solar.oct"].map(String::from),
        );
        arguments
    }
    pub fn transmission_arguments(&self) -> Vec<String> {
        [
            "-h",
            "-faa",
            "-ov",
            "-n",
            "1",
            "-ab",
            "0",
            "-ad",
            "1",
            "-as",
            "0",
            "-aa",
            "0",
            "-av",
            "0",
            "0",
            "0",
            "-lw",
            "1e-9",
            "-lr",
            "16",
            "-dt",
            "0",
            "-dc",
            "1",
            "-dj",
            "0",
            "-ds",
            "0.05",
            "-st",
            "0",
            "-ss",
            "0",
            "-u-",
            "annual-diffuse.oct",
        ]
        .map(String::from)
        .to_vec()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AnnualEngineIdentity {
    pub version: String,
    pub adapter_version: String,
    pub method_version: String,
    pub package_sha256: String,
    pub executable_sha256: BTreeMap<String, String>,
    pub library_sha256: BTreeMap<String, String>,
    pub fingerprint: String,
}
impl AnnualEngineIdentity {
    /// Validate retained provenance without requiring an engine installation.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == ENGINE_VERSION
                && ((self.adapter_version == ADAPTER_VERSION
                    && self.method_version == METHOD_VERSION)
                    || (self.adapter_version == LEGACY_ADAPTER_VERSION
                        && self.method_version == LEGACY_METHOD_VERSION))
                && self.package_sha256 == PACKAGE_SHA256,
            "Unsupported annual engine identity"
        );
        ensure!(
            self.executable_sha256.len() == EXECUTABLES.len(),
            "Unexpected annual executable inventory"
        );
        for (name, expected) in EXECUTABLES {
            ensure!(
                self.executable_sha256.get(name).map(String::as_str) == Some(expected),
                "Unrecognized annual executable digest: {name}"
            );
        }
        for (name, expected) in LIBRARIES {
            ensure!(
                self.library_sha256.get(name).map(String::as_str) == Some(expected),
                "Unrecognized annual library digest: {name}"
            );
        }
        ensure!(
            self.library_sha256
                .iter()
                .all(|(name, hash)| !name.is_empty()
                    && !name.contains('\\')
                    && !name.starts_with('/')
                    && !name.contains(':')
                    && !name.split('/').any(|part| part == ".." || part.is_empty())
                    && hash.len() == 64
                    && hash
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())),
            "Malformed annual library inventory"
        );
        let mut unhashed = self.clone();
        unhashed.fingerprint.clear();
        ensure!(
            self.fingerprint == greenlens_core::canonical_hash(&unhashed)?,
            "Annual engine fingerprint mismatch"
        );
        Ok(())
    }
    pub fn discover(engine: &Engine) -> Result<Self> {
        Self::discover_for_method(engine, METHOD_VERSION)
    }
    pub fn discover_for_method(engine: &Engine, method: &str) -> Result<Self> {
        let adapter = match method {
            METHOD_VERSION => ADAPTER_VERSION,
            LEGACY_METHOD_VERSION => LEGACY_ADAPTER_VERSION,
            _ => anyhow::bail!("Unsupported annual transport method"),
        };
        let mut executable_sha256 = BTreeMap::new();
        for (name, expected) in EXECUTABLES {
            let actual=digest(&fs::read(engine.root().join("bin").join(name)).with_context(|| format!("Required annual engine {name} is unavailable; reinstall pinned Radiance 6.0.2"))?);
            ensure!(
                actual == expected,
                "Annual executable {name} does not match pinned Radiance 6.0.2"
            );
            executable_sha256.insert(name.into(), actual);
        }
        let expected = "1517d597e34673bd8c5cfcea79850520ac272c7f5229df22401537f0b7c82934";
        ensure!(
            digest(&fs::read(engine.root().join("lib/reinhart.cal"))?) == expected,
            "Annual Reinhart library differs from pinned Windows package"
        );
        let mut result = Self {
            version: ENGINE_VERSION.into(),
            adapter_version: adapter.into(),
            method_version: method.into(),
            package_sha256: PACKAGE_SHA256.into(),
            executable_sha256,
            library_sha256: engine.identity().library_sha256.clone(),
            fingerprint: String::new(),
        };
        result.fingerprint = greenlens_core::canonical_hash(&result)?;
        result.validate()?;
        Ok(result)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnualRecipe {
    pub schema_version: String,
    pub method_version: String,
    pub engine: AnnualEngineIdentity,
    pub profile: AnnualProfile,
    pub normalized_weather_sha256: String,
    pub generated_weather_sha256: String,
    pub sensor_sha256: String,
    pub scene_sha256: String,
    pub sensor_count: usize,
    pub interval_indices: Vec<usize>,
    pub generated_weather_interval_indices: Vec<usize>,
    pub solar_interval_indices: Vec<usize>,
    pub sky_rows: usize,
    pub ordering: String,
    pub units: String,
    pub time_policy: String,
    pub ground_policy: String,
    pub direct_policy: String,
    pub direct_batches: Vec<DirectBatch>,
    pub child_environment: BTreeMap<String, String>,
    pub processes: Vec<ProcessRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnualResources {
    pub schema_version: String,
    pub measurement_policy: String,
    pub started_utc: String,
    pub finished_utc: String,
    pub total_wall_ms: u128,
    pub peak_engine_working_set_bytes: u64,
    pub peak_observed_runner_plus_engine_working_set_bytes: u64,
    pub peak_scratch_bytes: u64,
    pub max_scratch_bytes: u64,
    pub max_matrix_bytes: u64,
    pub max_observed_working_set_bytes: u64,
    pub processes: Vec<AnnualProcessResources>,
}
impl AnnualResources {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == "greenlens.annual-resources.v1"
                && self.measurement_policy == RESOURCE_MEASUREMENT_POLICY,
            "Unsupported annual resource measurement policy"
        );
        ensure!(
            self.max_scratch_bytes == process::ANNUAL_MAX_SCRATCH_BYTES
                && self.max_matrix_bytes == process::ANNUAL_MAX_FILE_BYTES
                && self.max_observed_working_set_bytes
                    == process::ANNUAL_MAX_OBSERVED_WORKING_SET_BYTES,
            "Unrecognized annual resource limits"
        );
        let start = chrono::DateTime::parse_from_rfc3339(&self.started_utc)?;
        let finish = chrono::DateTime::parse_from_rfc3339(&self.finished_utc)?;
        ensure!(
            finish >= start
                && start.offset().local_minus_utc() == 0
                && finish.offset().local_minus_utc() == 0,
            "Invalid annual resource UTC timestamps"
        );
        ensure!(
            (5..=8 + HOURS).contains(&self.processes.len()),
            "Incomplete annual process resource measurements"
        );
        let mut child_peak = 0;
        let mut combined_peak = 0;
        let mut scratch_peak = 0;
        for process in &self.processes {
            let a = chrono::DateTime::parse_from_rfc3339(&process.started_utc)?;
            let b = chrono::DateTime::parse_from_rfc3339(&process.finished_utc)?;
            ensure!(
                b >= a
                    && a >= start
                    && b <= finish
                    && process.samples > 0
                    && process.elapsed_ms <= self.total_wall_ms
                    && process.output_bytes <= self.max_matrix_bytes
                    && EXECUTABLES
                        .iter()
                        .any(|(name, _)| *name == process.executable),
                "Invalid annual child resource measurements"
            );
            child_peak = child_peak.max(process.peak_child_working_set_bytes);
            combined_peak =
                combined_peak.max(process.peak_observed_runner_plus_child_working_set_bytes);
            scratch_peak = scratch_peak.max(process.peak_scratch_bytes);
        }
        ensure!(
            self.peak_engine_working_set_bytes == child_peak
                && self.peak_observed_runner_plus_engine_working_set_bytes == combined_peak
                && self.peak_scratch_bytes >= scratch_peak
                && self.peak_scratch_bytes <= self.max_scratch_bytes
                && combined_peak <= self.max_observed_working_set_bytes,
            "Annual resource aggregate or limit mismatch"
        );
        Ok(())
    }
    fn refresh(&mut self, start: Instant, scratch: &Path) -> Result<()> {
        self.finished_utc = Utc::now().to_rfc3339();
        self.total_wall_ms = start.elapsed().as_millis();
        self.peak_engine_working_set_bytes = self
            .processes
            .iter()
            .map(|p| p.peak_child_working_set_bytes)
            .max()
            .unwrap_or(0);
        self.peak_observed_runner_plus_engine_working_set_bytes = self
            .processes
            .iter()
            .map(|p| p.peak_observed_runner_plus_child_working_set_bytes)
            .max()
            .unwrap_or(0);
        self.peak_scratch_bytes = self
            .peak_scratch_bytes
            .max(
                self.processes
                    .iter()
                    .map(|p| p.peak_scratch_bytes)
                    .max()
                    .unwrap_or(0),
            )
            .max(check_scratch(scratch)?);
        fs::write(
            scratch.join("annual-resources.json"),
            serde_json::to_vec_pretty(self)?,
        )?;
        Ok(())
    }
    /// Call after the application writes retained chunks into the owned job
    /// directory, before sealing/copying this telemetry into portable evidence.
    pub fn record_final_scratch(&mut self, scratch: &Path) -> Result<()> {
        self.peak_scratch_bytes = self.peak_scratch_bytes.max(check_scratch(scratch)?);
        fs::write(
            scratch.join("annual-resources.json"),
            serde_json::to_vec_pretty(self)?,
        )?;
        self.validate()
    }
}

pub struct AnnualOutput {
    /// GreenLens scratch encoding only: little-endian f64 time/sensor/RGB.
    pub combined_f64: PathBuf,
    pub sensor_count: usize,
    pub interval_indices: Vec<usize>,
    pub identity: AnnualEngineIdentity,
    pub profile: AnnualProfile,
    pub processes: Vec<ProcessRecord>,
    pub compact_files: Vec<String>,
    pub recipe: AnnualRecipe,
    pub resources: AnnualResources,
}
impl AnnualOutput {
    /// Streams one bounded hour across the application-service boundary.
    pub fn visit_hours(
        &self,
        mut visitor: impl FnMut(usize, &[[f64; 3]]) -> Result<()>,
    ) -> Result<()> {
        let mut reader = BufReader::new(File::open(&self.combined_f64)?);
        ensure!(
            reader.get_ref().metadata()?.len()
                == (self.sensor_count * self.interval_indices.len() * 24) as u64,
            "Combined climate scratch shape mismatch"
        );
        let mut values = vec![[0.0; 3]; self.sensor_count];
        for (position, &interval) in self.interval_indices.iter().enumerate() {
            for rgb in &mut values {
                for value in rgb.iter_mut() {
                    let mut bytes = [0; 8];
                    reader.read_exact(&mut bytes)?;
                    *value = f64::from_le_bytes(bytes);
                    ensure!(
                        value.is_finite() && *value >= 0.0,
                        "Invalid combined climate output"
                    );
                }
                greenlens_core::rgb_to_lux(*rgb)?;
            }
            visitor(interval, &values)
                .with_context(|| format!("Retain complete climate interval {position}"))?;
        }
        Ok(())
    }
}

/// Rebuilds the small selected-hour weather context instead of changing the
/// upstream three-hour dew-point average by feeding a single isolated row.
pub fn weather_window(
    normalized_epw: &[u8],
    selected: Option<usize>,
) -> Result<(Vec<u8>, Vec<usize>)> {
    ensure!(
        normalized_epw.len() <= 16 * 1024 * 1024,
        "Weather exceeds bounded input size"
    );
    let text = std::str::from_utf8(normalized_epw).context("Normalized EPW is not UTF-8")?;
    ensure!(
        !text.contains('\0'),
        "Normalized EPW contains a NUL character"
    );
    let lines: Vec<&str> = text.lines().collect();
    ensure!(
        lines.len() == 8 + HOURS,
        "Climate transport requires exactly 8,760 normalized weather rows"
    );
    ensure!(
        lines[0].starts_with("LOCATION,")
            && lines[4].starts_with("HOLIDAYS/DAYLIGHT SAVINGS,")
            && lines[7].starts_with("DATA PERIODS,1,1,"),
        "Expected canonical validated EPW headers"
    );
    let Some(index) = selected else {
        return Ok((normalized_epw.to_vec(), (0..HOURS).collect()));
    };
    ensure!(index < HOURS, "Selected weather interval must be 0–8759");
    let start = index.saturating_sub(2);
    let first: Vec<&str> = lines[start + 8].split(',').collect();
    let last: Vec<&str> = lines[index + 8].split(',').collect();
    ensure!(
        first.len() >= 16 && last.len() >= 16,
        "Malformed normalized weather rows"
    );
    let mut output = lines[..7].join("\n");
    output.push_str(&format!(
        "\nDATA PERIODS,1,1,GreenLens context,Monday,{}/{},{}/{}\n",
        first[1], first[2], last[1], last[2]
    ));
    for line in &lines[start + 8..=index + 8] {
        output.push_str(line);
        output.push('\n');
    }
    Ok((output.into_bytes(), (start..=index).collect()))
}

fn solar_indices(text: &str, weather_indices: &[usize]) -> Result<Vec<usize>> {
    let mut indices = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let minute: usize = line
            .trim()
            .strip_prefix("solar")
            .context("Unknown solar modifier name")?
            .parse()?;
        ensure!(
            minute >= 30 && (minute - 30).is_multiple_of(60),
            "Solar source is not at a canonical hourly midpoint"
        );
        let index = (minute - 30) / 60;
        ensure!(
            weather_indices.binary_search(&index).is_ok(),
            "Solar source lies outside generated weather interval mapping"
        );
        ensure!(
            indices.last().is_none_or(|last| *last < index),
            "Duplicate or out-of-order solar source"
        );
        indices.push(index);
    }
    Ok(indices)
}

/// Check compact generated solar primitives without loading or executing an
/// engine. Retained evidence accepts only the pinned generator's bounded form.
pub fn validate_solar_definitions(text: &str, expected: &[usize]) -> Result<()> {
    direct::parse_suns(text, expected).map(|_| ())
}

fn check_scratch(scratch: &Path) -> Result<u64> {
    let (sum, largest) = process::scratch_sizes(scratch)?;
    ensure!(
        largest <= process::ANNUAL_MAX_FILE_BYTES,
        "Annual scratch file exceeds 512 MiB resource budget"
    );
    ensure!(
        sum <= MAX_SCRATCH_BYTES,
        "Annual scratch exceeds 2 GiB resource budget"
    );
    Ok(sum)
}
fn cancelled(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::SeqCst),
        "Climate simulation cancelled"
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn simulate(
    engine: &Engine,
    model: &ValidatedModel,
    normalized_epw: &[u8],
    selected_interval: Option<usize>,
    profile: &AnnualProfile,
    scratch: &Path,
    cancel: &AtomicBool,
    on_log: impl Fn(&str),
) -> Result<AnnualOutput> {
    let started_utc = Utc::now().to_rfc3339();
    let start = Instant::now();
    let method = profile.method()?;
    let legacy = method == LEGACY_METHOD_VERSION;
    cancelled(cancel)?;
    let model = greenlens_core::validate_model(&model.model)?;
    let sensor_count = model.sensor_grid.sensors.len();
    ensure!(
        (1..=MAX_SENSORS).contains(&sensor_count),
        "Weather studies require at most 256 sensors; increase requested grid spacing"
    );
    let identity = AnnualEngineIdentity::discover_for_method(engine, method)?;
    let (weather, weather_indices) = weather_window(normalized_epw, selected_interval)?;
    let output_indices =
        selected_interval.map_or_else(|| weather_indices.clone(), |index| vec![index]);
    let time_count = weather_indices.len();
    let sky_rows = 144 * profile.sky_basis_mf * profile.sky_basis_mf + 2;
    fs::create_dir_all(scratch)?;
    let scene = scene_text(&model)?;
    let sensors = model
        .sensor_grid
        .sensors
        .iter()
        .map(|sensor| {
            format!(
                "{} {} {} {} {} {}\n",
                sensor.position[0],
                sensor.position[1],
                sensor.position[2],
                sensor.direction[0],
                sensor.direction[1],
                sensor.direction[2]
            )
        })
        .collect::<String>();
    fs::write(scratch.join("annual-scene.rad"), &scene)?;
    fs::write(scratch.join("annual-sensors.pts"), &sensors)?;
    fs::write(scratch.join("annual-weather.epw"), &weather)?;
    fs::write(scratch.join("annual-receiver.rad"), RECEIVER_TEXT)?;
    let mut processes = Vec::new();
    let mut resources = AnnualResources {
        schema_version: "greenlens.annual-resources.v1".into(),
        measurement_policy: RESOURCE_MEASUREMENT_POLICY.into(),
        started_utc,
        finished_utc: String::new(),
        total_wall_ms: 0,
        peak_engine_working_set_bytes: 0,
        peak_observed_runner_plus_engine_working_set_bytes: 0,
        peak_scratch_bytes: 0,
        max_scratch_bytes: process::ANNUAL_MAX_SCRATCH_BYTES,
        max_matrix_bytes: process::ANNUAL_MAX_FILE_BYTES,
        max_observed_working_set_bytes: process::ANNUAL_MAX_OBSERVED_WORKING_SET_BYTES,
        processes: Vec::new(),
    };
    let mut execute =
        |name: &str, args: Vec<String>, input: Option<&str>, output: &str| -> Result<()> {
            cancelled(cancel)?;
            let input = input.map(|file| scratch.join(file));
            for suffix in ["resources.json", "process.json"] {
                let path = scratch.join(format!("{name}.{suffix}"));
                if path.exists() {
                    fs::remove_file(path)?;
                }
            }
            let result = process::execute_annual(
                engine.root(),
                name,
                &args,
                scratch,
                input.as_deref(),
                &scratch.join(output),
                cancel,
            );
            // Keep separate records even when the same executable is invoked twice.
            if let Ok((record, measurement)) = &result {
                processes.push(record.clone());
                resources.processes.push(measurement.clone());
            } else {
                // Cancellation/resource failure records have a separate shape;
                // preserve them in scratch without representing a complete run.
                if let Ok(bytes) = fs::read(scratch.join(format!("{name}.resources.json"))) {
                    if let Ok(measurement) = serde_json::from_slice(&bytes) {
                        resources.processes.push(measurement);
                    }
                }
                if let Ok(bytes) = fs::read(scratch.join(format!("{name}.process.json"))) {
                    if let Ok(record) = serde_json::from_slice::<ProcessRecord>(&bytes) {
                        if record.arguments == args {
                            processes.push(record);
                        }
                    }
                }
            }
            fs::write(
                scratch.join("annual-logs.json"),
                serde_json::to_vec_pretty(&processes)?,
            )?;
            resources.refresh(start, scratch)?;
            result?;
            check_scratch(scratch)?;
            Ok(())
        };
    on_log("Preparing visible Perez sky and luminous ground for every requested weather interval");
    let ground = model.model.sky.ground_reflectance.to_string();
    let sky_arguments = vec![
        "-O0".into(),
        "-m".into(),
        profile.sky_basis_mf.to_string(),
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
    ];
    execute("gendaymtx", sky_arguments, None, "annual-sky.mtx")?;
    if legacy {
        execute(
            "gendaymtx",
            solar_generation_arguments(profile, true),
            None,
            "annual-solar-sky.mtx",
        )?;
    } else {
        // -s zeros suncolor, including exported sources. Generate solar sources
        // separately with -n so no solar matrix or second ground term is used.
        execute(
            "gendaymtx",
            solar_generation_arguments(profile, false),
            None,
            "annual-sun-generation.txt",
        )?;
    }
    let sun_indices = solar_indices(
        &fs::read_to_string(scratch.join("annual-sun-modifiers.txt"))?,
        &weather_indices,
    )?;
    let solar_sources = direct::parse_suns(
        &fs::read_to_string(scratch.join("annual-suns.rad"))?,
        &sun_indices,
    )?;
    MatrixReader::open(&scratch.join("annual-sky.mtx"), sky_rows, time_count)?.finish()?;
    if legacy {
        MatrixReader::open(&scratch.join("annual-solar-sky.mtx"), sky_rows, time_count)?
            .finish()?;
    }
    on_log("Compiling unchanged geometry with diffuse/ground coefficient receiver");
    execute(
        "oconv",
        ["-f", "annual-scene.rad", "annual-receiver.rad"]
            .map(String::from)
            .to_vec(),
        None,
        "annual-diffuse.oct",
    )?;
    let mut args = profile.arguments(sensor_count);
    args.extend([
        "-V-".into(),
        "-e".into(),
        format!("MF:{}", profile.sky_basis_mf),
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
    on_log("Tracing diffuse and ground daylight coefficients; cancellation remains available");
    execute(
        "rcontrib",
        args,
        Some("annual-sensors.pts"),
        "annual-coefficients.mtx",
    )?;
    MatrixReader::open(
        &scratch.join("annual-coefficients.mtx"),
        sensor_count,
        sky_rows,
    )?
    .finish()?;
    on_log("Multiplying validated ASCII daylight and weather matrices");
    execute(
        "dctimestep",
        ["-oa", "annual-coefficients.mtx", "annual-sky.mtx"]
            .map(String::from)
            .to_vec(),
        None,
        "annual-diffuse-rgb.mtx",
    )?;
    if !sun_indices.is_empty() && legacy {
        on_log("Tracing direct-only sky-patch coefficients for solar correction");
        let mut args = profile.direct_arguments(sensor_count);
        args.extend([
            "-V-".into(),
            "-e".into(),
            format!("MF:{}", profile.sky_basis_mf),
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
        execute(
            "rcontrib",
            args,
            Some("annual-sensors.pts"),
            "annual-direct-coefficients.mtx",
        )?;
        MatrixReader::open(
            &scratch.join("annual-direct-coefficients.mtx"),
            sensor_count,
            sky_rows,
        )?
        .finish()?;
        execute(
            "dctimestep",
            ["-oa", "annual-coefficients.mtx", "annual-solar-sky.mtx"]
                .map(String::from)
                .to_vec(),
            None,
            "annual-solar-patch-rgb.mtx",
        )?;
        execute(
            "dctimestep",
            [
                "-oa",
                "annual-direct-coefficients.mtx",
                "annual-solar-sky.mtx",
            ]
            .map(String::from)
            .to_vec(),
            None,
            "annual-direct-patch-rgb.mtx",
        )?;
    }
    if !sun_indices.is_empty() && !legacy {
        on_log("Compiling unchanged geometry with explicit solar sources and no luminous ground");
        execute(
            "oconv",
            ["-f", "annual-scene.rad", "annual-suns.rad"]
                .map(String::from)
                .to_vec(),
            None,
            "annual-solar.oct",
        )?;
        on_log("Tracing explicit reflected-solar transport; full-year jobs can take hours and remain cancellable");
        execute(
            "rcontrib",
            profile.solar_arguments(sensor_count, false),
            Some("annual-sensors.pts"),
            "annual-solar-rgb.mtx",
        )?;
        on_log("Tracing the identical centroid-direct solar term for finite-disc replacement");
        execute(
            "rcontrib",
            profile.solar_arguments(sensor_count, true),
            Some("annual-sensors.pts"),
            "annual-solar-direct-rgb.mtx",
        )?;
    }
    cancelled(cancel)?;
    on_log("Validating every RGB value and assembling complete ordered time series");
    // <=256*8760*24 = 53,821,440 bytes. Large sky matrices are streamed and are
    // never retained together with another unbounded in-memory representation.
    let mut combined = vec![[0.0; 3]; sensor_count * output_indices.len()];
    let mut diffuse = MatrixReader::open(
        &scratch.join("annual-diffuse-rgb.mtx"),
        sensor_count,
        time_count,
    )?;
    for sensor in 0..sensor_count {
        for &index in &weather_indices {
            let rgb = diffuse
                .next_rgb()?
                .context("Incomplete diffuse sensor/time matrix")?;
            if let Ok(position) = output_indices.binary_search(&index) {
                combined[position * sensor_count + sensor] = rgb;
            }
        }
        cancelled(cancel)?;
    }
    ensure!(diffuse.next_rgb()?.is_none(), "Excess diffuse output");
    if !sun_indices.is_empty() && legacy {
        let mut solar = MatrixReader::open(
            &scratch.join("annual-solar-patch-rgb.mtx"),
            sensor_count,
            time_count,
        )?;
        let mut direct_patch = MatrixReader::open(
            &scratch.join("annual-direct-patch-rgb.mtx"),
            sensor_count,
            time_count,
        )?;
        for sensor in 0..sensor_count {
            for &index in &weather_indices {
                let rgb = solar
                    .next_rgb()?
                    .context("Incomplete full solar patch matrix")?;
                let direct = direct_patch
                    .next_rgb()?
                    .context("Incomplete direct solar patch matrix")?;
                // The pinned generator may populate low-altitude solar patches
                // even when it emits no finite solar source. Apply one explicit
                // source-emission horizon policy to both direct and indirect sun.
                if let Ok(position) = output_indices.binary_search(&index) {
                    if sun_indices.binary_search(&index).is_err() {
                        continue;
                    }
                    for channel in 0..3 {
                        combined[position * sensor_count + sensor][channel] +=
                            rgb[channel] - direct[channel];
                    }
                }
            }
            cancelled(cancel)?;
        }
        ensure!(
            solar.next_rgb()?.is_none() && direct_patch.next_rgb()?.is_none(),
            "Excess solar patch output"
        );
    }
    if !sun_indices.is_empty() && !legacy {
        let mut solar = MatrixReader::open(
            &scratch.join("annual-solar-rgb.mtx"),
            sensor_count,
            sun_indices.len(),
        )?;
        let mut direct_solar = MatrixReader::open(
            &scratch.join("annual-solar-direct-rgb.mtx"),
            sensor_count,
            sun_indices.len(),
        )?;
        for sensor in 0..sensor_count {
            for &index in &sun_indices {
                let rgb = solar
                    .next_rgb()?
                    .context("Incomplete explicit full solar matrix")?;
                let direct = direct_solar
                    .next_rgb()?
                    .context("Incomplete explicit direct solar matrix")?;
                if let Ok(position) = output_indices.binary_search(&index) {
                    for channel in 0..3 {
                        combined[position * sensor_count + sensor][channel] +=
                            rgb[channel] - direct[channel];
                    }
                }
            }
            cancelled(cancel)?;
        }
        ensure!(
            solar.next_rgb()?.is_none() && direct_solar.next_rgb()?.is_none(),
            "Excess explicit solar output"
        );
    }
    let aperture = direct::Aperture::from_model(&model)?;
    let selected_sources: Vec<_> = solar_sources
        .into_iter()
        .filter(|sun| output_indices.binary_search(&sun.interval).is_ok())
        .collect();
    let sources_per_batch = MAX_DIRECT_BATCH_RAYS / (sensor_count * profile.direct_solar_samples);
    let mut direct_batches = Vec::new();
    for (batch_number, suns) in selected_sources.chunks(sources_per_batch).enumerate() {
        cancelled(cancel)?;
        let mut batch = direct::write_batch(
            &scratch.join("annual-direct-rays.pts"),
            suns,
            &model.sensor_grid.sensors,
            &aperture,
            profile.direct_solar_samples,
            cancel,
        )?;
        on_log(&format!(
            "Finite solar batch {}: tracing {} of {} directions after exact aperture culling",
            batch_number + 1,
            batch.emitted_rays,
            batch.attempted_rays
        ));
        if batch.emitted_rays > 0 {
            execute(
                "rtrace",
                profile.transmission_arguments(),
                Some("annual-direct-rays.pts"),
                "annual-direct-rgb.txt",
            )?;
        } else {
            fs::write(scratch.join("annual-direct-rgb.txt"), b"")?;
        }
        let direct_values = direct::integrate_batch(
            &scratch.join("annual-direct-rgb.txt"),
            suns,
            &model.sensor_grid.sensors,
            &aperture,
            profile.direct_solar_samples,
            batch.emitted_rays,
            cancel,
        )?;
        batch.output_sha256 = direct::file_hash(&scratch.join("annual-direct-rgb.txt"))?;
        for (source_position, sun) in suns.iter().enumerate() {
            let position = output_indices
                .binary_search(&sun.interval)
                .expect("selected source mapping");
            for sensor in 0..sensor_count {
                for channel in 0..3 {
                    combined[position * sensor_count + sensor][channel] +=
                        direct_values[source_position * sensor_count + sensor][channel];
                }
            }
        }
        direct_batches.push(batch);
    }
    drop(execute);
    // Only regenerable batch files are removed. Their hashes, exact ordering,
    // ray counts and generation policy remain in the compact immutable recipe.
    for name in ["annual-direct-rays.pts", "annual-direct-rgb.txt"] {
        let path = scratch.join(name);
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    let combined_f64 = scratch.join("annual-combined.f64");
    let mut writer = BufWriter::new(File::create(&combined_f64)?);
    for rgb in &combined {
        greenlens_core::rgb_to_lux(*rgb)?;
        for channel in rgb {
            writer.write_all(&channel.to_le_bytes())?;
        }
    }
    writer.flush()?;
    writer.get_ref().sync_all()?;
    let recipe = AnnualRecipe {
        schema_version: if legacy {
            LEGACY_RECIPE_SCHEMA_VERSION
        } else {
            RECIPE_SCHEMA_VERSION
        }
        .into(),
        method_version: method.into(),
        engine: identity.clone(),
        profile: profile.clone(),
        normalized_weather_sha256: digest(normalized_epw),
        generated_weather_sha256: digest(&weather),
        sensor_sha256: digest(sensors.as_bytes()),
        scene_sha256: digest(scene.as_bytes()),
        sensor_count,
        interval_indices: output_indices.clone(),
        generated_weather_interval_indices: weather_indices,
        solar_interval_indices: sun_indices,
        sky_rows,
        ordering: RESULT_ORDERING.into(),
        units: RESULT_UNITS.into(),
        time_policy: TIME_POLICY.into(),
        ground_policy: if legacy {
            LEGACY_GROUND_POLICY
        } else {
            GROUND_POLICY
        }
        .into(),
        direct_policy: DIRECT_POLICY.into(),
        direct_batches,
        child_environment: BTreeMap::from([("RAYPATH".into(), ".;<engine>/lib".into())]),
        processes: processes.clone(),
    };
    fs::write(
        scratch.join("annual-recipe.json"),
        serde_json::to_vec_pretty(&recipe)?,
    )?;
    check_scratch(scratch)?;
    resources.refresh(start, scratch)?;
    resources.validate()?;
    cancelled(cancel)?;
    Ok(AnnualOutput {
        combined_f64,
        sensor_count,
        interval_indices: output_indices,
        identity,
        profile: profile.clone(),
        processes,
        compact_files: [
            "annual-scene.rad",
            "annual-sensors.pts",
            "annual-weather.epw",
            "annual-receiver.rad",
            "annual-suns.rad",
            "annual-sun-modifiers.txt",
            "annual-recipe.json",
            "annual-logs.json",
            "annual-resources.json",
        ]
        .map(String::from)
        .to_vec(),
        recipe,
        resources,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn native_explicit_generator_preserves_nonzero_legacy_solar_sources() {
        let engine = Engine::discover(None).expect("Required native Radiance is unavailable");
        let root = PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("Native local cache"))
            .join("GreenLens/verification")
            .join(format!(
                "annual solar generator regression 空間 {}",
                Utc::now().timestamp_nanos_opt().unwrap()
            ));
        let mut weather = String::from("LOCATION,Synthetic,None,None,GreenLens,000000,0,0,0,0\nDESIGN CONDITIONS,0\nTYPICAL/EXTREME PERIODS,0\nGROUND TEMPERATURES,0\nHOLIDAYS/DAYLIGHT SAVINGS,No,0,0,0\nCOMMENTS 1,Synthetic\nCOMMENTS 2,Test\nDATA PERIODS,1,1,Data,Monday,3/21,3/21\n");
        for hour in 11..=13 {
            weather.push_str(&format!("2001,3,21,{hour},60,?9?9?9?9,25,15,60,101325,0,0,300,800,700,100,0,0,0,0,180,1,5,5,20,77777,9,999999999,20,0.1,0,88,0.2,0,0\n"));
        }
        let cancel = AtomicBool::new(false);
        let mut results = Vec::new();
        for legacy in [true, false] {
            let directory = root.join(if legacy { "legacy" } else { "explicit" });
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join("annual-weather.epw"), &weather).unwrap();
            process::execute(
                engine.root(),
                "gendaymtx",
                &solar_generation_arguments(&AnnualProfile::shipping(), legacy),
                &directory,
                None,
                &directory.join("output.txt"),
                &cancel,
            )
            .unwrap();
            let source = fs::read_to_string(directory.join("annual-suns.rad")).unwrap();
            let modifiers = fs::read_to_string(directory.join("annual-sun-modifiers.txt")).unwrap();
            let expected = [79 * 24 + 10, 79 * 24 + 11, 79 * 24 + 12];
            assert_eq!(solar_indices(&modifiers, &expected).unwrap(), expected);
            direct::parse_suns(&source, &expected).unwrap();
            let tokens: Vec<_> = source.split_whitespace().collect();
            assert!(tokens.as_chunks::<19>().0.iter().all(|sun| sun[6..9]
                .iter()
                .all(|value| value.parse::<f64>().unwrap() > 0.0)));
            results.push((source, modifiers));
        }
        assert_eq!(
            results[0], results[1],
            "Solar source export must be independent of the unused patch matrix"
        );
    }
    fn fixture() -> Vec<u8> {
        let mut data=String::from("LOCATION,Synthetic,None,None,GreenLens,000000,0,0,0,0\nDESIGN CONDITIONS,0\nTYPICAL/EXTREME PERIODS,0\nGROUND TEMPERATURES,0\nHOLIDAYS/DAYLIGHT SAVINGS,No,0,0,0\nCOMMENTS 1,Synthetic\nCOMMENTS 2,Test\nDATA PERIODS,1,1,Data,Monday,1/1,12/31\n");
        for index in 0..HOURS {
            data.push_str(&format!(
                "2001,1,{}, {},60,x,20,{},50,101325,0,0,0,0,0,0\n",
                index / 24 + 1,
                index % 24 + 1,
                index
            ));
        }
        data.into_bytes()
    }
    #[test]
    fn selected_context_preserves_first_second_and_rollover_dew_point_inputs() {
        let input = fixture();
        for (index, expected) in [
            (0, vec![0]),
            (1, vec![0, 1]),
            (2, vec![0, 1, 2]),
            (23, vec![21, 22, 23]),
            (24, vec![22, 23, 24]),
            (8759, vec![8757, 8758, 8759]),
        ] {
            let (bytes, indices) = weather_window(&input, Some(index)).unwrap();
            assert_eq!(indices, expected);
            let source = std::str::from_utf8(&input).unwrap();
            let generated = std::str::from_utf8(&bytes).unwrap();
            assert_eq!(
                generated.lines().skip(8).collect::<Vec<_>>(),
                indices
                    .iter()
                    .map(|i| source.lines().nth(i + 8).unwrap())
                    .collect::<Vec<_>>()
            );
        }
        assert!(weather_window(&input, Some(HOURS)).is_err());
    }
    #[test]
    fn solar_mapping_rejects_missing_midpoint_unknown_duplicate_and_reorder() {
        assert_eq!(
            solar_indices("solar30\nsolar1470\n", &[0, 24]).unwrap(),
            vec![0, 24]
        );
        for text in [
            "solar0\n",
            "solar31\n",
            "sun30\n",
            "solar30\nsolar30\n",
            "solar1470\nsolar30\n",
            "solar90\n",
        ] {
            assert!(solar_indices(text, &[0, 24]).is_err(), "{text}");
        }
    }
    #[test]
    fn profile_rejects_unrecognized_parameters() {
        let mut profile = AnnualProfile::shipping();
        profile.workers = 10;
        assert!(profile.validate().is_err());
    }
    #[test]
    fn historical_profiles_preserve_frozen_serialization_and_hashes() {
        let shipping: serde_json::Value = serde_json::from_str(
            r#"{
            "name":"shipping","sky_basis_mf":2,"ambient_bounces":8,
            "ambient_divisions":131072,"direct_ambient_divisions":131072,
            "direct_solar_samples":1024,"ambient_supersamples":0,
            "ambient_accuracy":0.0,"ambient_resolution":256,"ambient_value":[0.0,0.0,0.0],
            "limit_weight":1e-9,"ray_depth":16,"direct_threshold":0.0,
            "direct_certainty":1.0,"direct_jitter":0.0,"direct_size":0.05,
            "specular_threshold":0.0,"specular_sampling":0.0,
            "uncorrelated_random_sampling":false,"workers":1
        }"#,
        )
        .unwrap();
        let mut reference = shipping.clone();
        for (key, value) in [
            ("name", serde_json::json!("reference")),
            ("sky_basis_mf", serde_json::json!(4)),
            ("ambient_bounces", serde_json::json!(9)),
            ("ambient_divisions", serde_json::json!(262144)),
            ("direct_ambient_divisions", serde_json::json!(262144)),
            ("direct_solar_samples", serde_json::json!(4096)),
        ] {
            reference[key] = value;
        }
        for (profile, frozen) in [
            (
                AnnualProfile::shipping_for_method(LEGACY_METHOD_VERSION).unwrap(),
                shipping,
            ),
            (
                AnnualProfile::reference_for_method(LEGACY_METHOD_VERSION).unwrap(),
                reference,
            ),
        ] {
            assert_eq!(serde_json::to_value(&profile).unwrap(), frozen);
            assert_eq!(
                greenlens_core::canonical_hash(&profile).unwrap(),
                greenlens_core::canonical_hash(&frozen).unwrap()
            );
            assert!(!serde_json::to_value(&profile)
                .unwrap()
                .as_object()
                .unwrap()
                .contains_key("solar_ambient_divisions"));
            assert_eq!(profile.method().unwrap(), LEGACY_METHOD_VERSION);
            assert!(profile.validate_for_method(METHOD_VERSION).is_err());
        }
    }
    #[test]
    fn explicit_profile_binds_solar_counts_and_never_aliases_history() {
        for profile in [AnnualProfile::shipping(), AnnualProfile::reference()] {
            assert_eq!(profile.method().unwrap(), METHOD_VERSION);
            assert!(profile.validate_for_method(LEGACY_METHOD_VERSION).is_err());
            let full = profile.solar_arguments(24, false);
            let direct = profile.solar_arguments(24, true);
            let ad = full.iter().position(|value| value == "-ad").unwrap();
            let ab = full.iter().position(|value| value == "-ab").unwrap();
            assert_eq!(
                full[ad + 1],
                profile.solar_ambient_divisions.unwrap().to_string()
            );
            assert_eq!(direct[ad + 1], full[ad + 1]);
            assert_eq!(direct[ab + 1], "0");
            let mut without_method_marker = profile.clone();
            without_method_marker.solar_ambient_divisions = None;
            assert!(without_method_marker.validate().is_err());
            let mut changed = profile.clone();
            changed.solar_ambient_divisions = Some(2048);
            assert!(changed.validate().is_err());
        }
    }
}
