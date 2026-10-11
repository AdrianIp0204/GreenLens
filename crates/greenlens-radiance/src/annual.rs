//! Native fixed-geometry daylight coefficients and explicit solar transport.
//! The ground is a luminous boundary only in the diffuse/ground coefficient
//! path. The solar octree has no external ground surface or luminous dome.
#[path = "annual_direct.rs"]
mod direct;
#[path = "annual_matrix.rs"]
mod matrix;
pub use direct::{
    DirectBatch, DIRECT_POLICY as EXPLICIT_DIRECT_POLICY, LEGACY_DIRECT_POLICY,
    MAX_DIRECT_BATCH_RAYS,
};

use crate::annual_primary::{
    ExternalPrimaryContract, PrimaryPurpose, PrimaryStreamRecord, PrimaryStreams,
    PRIMARY_STREAMS_SCHEMA_VERSION,
};
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
    time::{Duration, Instant},
};

pub const LEGACY_ADAPTER_VERSION: &str = "radiance-climate-direct-corrected/2";
pub const LEGACY_METHOD_VERSION: &str =
    "perez-epw-dewpoint3-reinhart-patch-indirect-finite-direct/2";
pub const LEGACY_RECIPE_SCHEMA_VERSION: &str = "greenlens.annual-recipe.v2";
pub const EXPLICIT_ADAPTER_VERSION: &str = "radiance-climate-explicit-indirect-finite-direct/3";
pub const EXPLICIT_METHOD_VERSION: &str =
    "perez-epw-dewpoint3-reinhart-explicit-indirect-finite-direct/3";
pub const EXPLICIT_RECIPE_SCHEMA_VERSION: &str = "greenlens.annual-recipe.v3";
pub const REPEATED_ADAPTER_VERSION: &str = "radiance-climate-repeated-direct-corrected/4";
pub const REPEATED_METHOD_VERSION: &str =
    "perez-epw-dewpoint3-reinhart-repeated-patch-indirect-finite-direct/4";
pub const REPEATED_RECIPE_SCHEMA_VERSION: &str = "greenlens.annual-recipe.v4";
pub const ADAPTER_VERSION: &str = "radiance-climate-paired-primary-finite-direct/5";
pub const METHOD_VERSION: &str = "perez-epw-dewpoint3-reinhart-hammersley-indirect-finite-direct/5";
pub const RECIPE_SCHEMA_VERSION: &str = "greenlens.annual-recipe.v5";
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
pub const EXPLICIT_GROUND_POLICY: &str = "Uniform luminous ground includes diffuse and direct-horizontal reflected radiance in the -s sky matrix once. Solar-only explicit source transport has no luminous dome or external ground geometry. Reflected solar is full explicit-source transport minus identical ab0 centroid-direct transport; finite-disc directional transmission replaces only direct sun. The emitted-source horizon policy applies to both solar terms.";
pub const REPEATED_GROUND_POLICY: &str = "Uniform luminous ground includes diffuse and direct-horizontal reflected radiance in the -s sky matrix once; -d solar matrix has zero ground and diffuse sky. Reflected solar is full minus direct-only patch transport, gated to emitted solar-source intervals; no external ground geometry. Coefficients use a scaled receiver and contiguous repeated primaries with negative-depth Russian roulette; divide each coefficient product by the recorded receiver scale exactly once. Finite-direct transmission retains a separate unit receiver.";
pub const REPEATED_DIRECT_POLICY: &str = "Equal-solid-angle midpoint solar-disc quadrature: uniform cos(theta) rings and azimuth; shipping 32x32, reference 64x64. Normalize printed solar direction; horizon clip each ray at world z>0. Ray origin is sensor+0.0001m*normal, matching pinned immediate-irradiance evaluation. Cull each ray only if it misses the authoritative glass polygon, with inclusive 1e-9m edge allowance; Radiance traces glazing/shading/context at ab0 with a unit receiver. Reflected solar is approximated by scaled repeated full minus direct-only patch transport, normalized once by the receiver scale and included only for emitted solar-source intervals. No negative corrected RGB is silently clamped.";
pub const LEGACY_RESOURCE_MEASUREMENT_POLICY: &str = "Native Windows GetProcessMemoryInfo: lifetime peak working set of each engine child; simultaneous runner+child working sets sampled every20ms (shared pages counted in both processes); complete owned scratch tree file bytes sampled during each child, after assembly, and after retained series creation. Observed working-set sum is not total private memory. One worker; no engine subprocess descendants expected.";
pub const FILE_RESOURCE_MEASUREMENT_POLICY: &str = "Native Windows GetProcessMemoryInfo: lifetime peak working set of each engine child; simultaneous runner+child working sets sampled in a loop with a complete owned scratch tree scan and a 20ms sleep between iterations (shared pages counted in both processes). Scan and scheduling time add to the interval; this is not fixed-20ms sampling. Scratch file sizes are queried from opened file handles during each child, after assembly, and after retained series creation. Observed working-set sum is not total private memory. One worker; no engine subprocess descendants expected.";
pub const GROUND_POLICY: &str = "Uniform luminous ground includes diffuse and direct-horizontal reflected radiance in the -s sky matrix once; -d solar matrix has zero ground and diffuse sky. Reflected solar is full minus ab0 direct-only patch transport on identical external cosine-weighted Hammersley primary streams, gated to emitted solar-source intervals. Radiance -c N returns the coefficient mean; multiply each coefficient product by pi/receiver_scale exactly once with no further division by N. No external ground geometry; finite-direct transmission retains a separate unit receiver.";
pub const DIRECT_POLICY: &str = "Equal-solid-angle midpoint solar-disc quadrature: uniform cos(theta) rings and azimuth; shipping 32x32, reference 64x64. Normalize printed solar direction; horizon clip each ray at world z>0. Ray origin is sensor+0.0001m*normal, matching pinned immediate-irradiance evaluation. Cull finite-solar rays only when they miss the authoritative glass polygon, with inclusive 1e-9m edge allowance; Radiance traces glazing/shading/context at ab0 with a unit receiver. Reflected solar uses paired full minus ab0 patch coefficients from uncropped external Hammersley hemispheres, normalized once by pi/receiver_scale and gated to emitted solar-source intervals. Negative coefficient residuals and corrected RGB are rejected, never clamped.";
pub const RESOURCE_MEASUREMENT_POLICY: &str = "Native Windows GetProcessMemoryInfo: lifetime peak working set of each engine child and sampled runner+child working sets retained in annual process measurements. Streamed coefficient calls additionally retain sampled runner+whole-Job working sets with atomic Job assignment in annual-primary-streams.json; file-backed stages supervise one trusted worker. Complete owned scratch sizes use opened file handles. Sampling loops include scan/query time and a 20ms sleep, not a guaranteed 20ms cadence; shared pages are counted across processes and working sets are not private memory. Every method-5 process has a 7200-second deadline.";
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
    pub ray_depth: i32,
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
    /// Absent in immutable method-2/3 profiles. These two fields jointly bind
    /// the method-4 primary grouping and contribution normalization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coefficient_primary_repetitions: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coefficient_receiver_scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_primary: Option<ExternalPrimaryContract>,
}
impl AnnualProfile {
    /// Candidate numerical settings; release freeze requires the recorded suite.
    fn repeated_shipping() -> Self {
        Self {
            ambient_divisions: 8192,
            direct_ambient_divisions: 8192,
            ray_depth: -16,
            coefficient_primary_repetitions: Some(128),
            coefficient_receiver_scale: Some(100.0),
            ..Self::legacy_shipping()
        }
    }
    pub fn shipping() -> Self {
        Self {
            ambient_bounces: 7,
            ambient_divisions: 64,
            direct_ambient_divisions: 64,
            ray_depth: -16,
            coefficient_receiver_scale: Some(100.0),
            external_primary: Some(ExternalPrimaryContract::new(16_384).expect("compiled count")),
            ..Self::legacy_shipping()
        }
    }
    fn explicit_shipping() -> Self {
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
            coefficient_primary_repetitions: None,
            coefficient_receiver_scale: None,
            external_primary: None,
        }
    }
    fn repeated_reference() -> Self {
        Self {
            name: "reference".into(),
            sky_basis_mf: 4,
            ambient_bounces: 9,
            ambient_divisions: 65536,
            direct_ambient_divisions: 65536,
            direct_solar_samples: 4096,
            coefficient_primary_repetitions: Some(32),
            ..Self::repeated_shipping()
        }
    }
    pub fn reference() -> Self {
        Self {
            name: "reference".into(),
            sky_basis_mf: 4,
            ambient_bounces: 8,
            ambient_divisions: 128,
            direct_ambient_divisions: 128,
            direct_solar_samples: 4096,
            external_primary: Some(ExternalPrimaryContract::new(65_536).expect("compiled count")),
            ..Self::shipping()
        }
    }
    fn explicit_reference() -> Self {
        Self {
            name: "reference".into(),
            sky_basis_mf: 4,
            ambient_bounces: 9,
            ambient_divisions: 2_097_152,
            direct_ambient_divisions: 2_097_152,
            direct_solar_samples: 4096,
            solar_ambient_divisions: Some(16384),
            ..Self::explicit_shipping()
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
            REPEATED_METHOD_VERSION => Ok(Self::repeated_shipping()),
            EXPLICIT_METHOD_VERSION => Ok(Self::explicit_shipping()),
            LEGACY_METHOD_VERSION => Ok(Self::legacy_shipping()),
            _ => anyhow::bail!("Unsupported climate transport method"),
        }
    }
    pub fn reference_for_method(method: &str) -> Result<Self> {
        match method {
            METHOD_VERSION => Ok(Self::reference()),
            REPEATED_METHOD_VERSION => Ok(Self::repeated_reference()),
            EXPLICIT_METHOD_VERSION => Ok(Self::explicit_reference()),
            LEGACY_METHOD_VERSION => Ok(Self::legacy_reference()),
            _ => anyhow::bail!("Unsupported climate transport method"),
        }
    }
    pub fn method(&self) -> Result<&'static str> {
        for method in [
            METHOD_VERSION,
            REPEATED_METHOD_VERSION,
            EXPLICIT_METHOD_VERSION,
            LEGACY_METHOD_VERSION,
        ] {
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
    /// Complete coefficient command. Historical methods retain their original
    /// argument order; method 4 averages contiguous primary repeats per sensor.
    pub fn coefficient_arguments(&self, sensors: usize, direct: bool) -> Vec<String> {
        let mut arguments = if direct {
            self.direct_arguments(sensors)
        } else {
            self.arguments(sensors)
        };
        let repeated = self.coefficient_primary_repetitions;
        if let Some(primary) = &self.external_primary {
            arguments.retain(|argument| argument != "-I+");
            if direct {
                let position = arguments
                    .iter()
                    .position(|argument| argument == "-ab")
                    .expect("compiled profile");
                arguments[position + 1] = "0".into();
            }
            arguments.extend(["-c".into(), primary.count.to_string()]);
        }
        if let Some(repeats) = repeated {
            arguments.extend(["-c".into(), repeats.to_string()]);
        }
        arguments.extend([
            if repeated.is_some() || self.external_primary.is_some() {
                "-V+"
            } else {
                "-V-"
            }
            .into(),
            "-e".into(),
            format!("MF:{}", self.sky_basis_mf),
            "-f".into(),
            "reinhart.cal".into(),
            "-b".into(),
            "rbin".into(),
            "-bn".into(),
            "Nrbins".into(),
            "-m".into(),
            "sky_glow".into(),
            if repeated.is_some() || self.external_primary.is_some() {
                "annual-coefficient.oct"
            } else {
                "annual-diffuse.oct"
            }
            .into(),
        ]);
        arguments
    }
    pub fn coefficient_receiver_text(&self) -> String {
        if self.external_primary.is_some() {
            let scale = self
                .coefficient_receiver_scale
                .expect("compiled external-primary profile");
            return format!("void glow sky_glow\n0\n0\n4 {scale} {scale} {scale} 0\nsky_glow source dome\n0\n0\n4 0 0 1 360\n");
        }
        match self.coefficient_receiver_scale {
            None => RECEIVER_TEXT.into(),
            Some(scale) => {
                RECEIVER_TEXT.replacen("4 1 1 1 0", &format!("4 {scale} {scale} {scale} 0"), 1)
            }
        }
    }
    /// Regenerate bounded engine input from authoritative unique sensor text.
    /// Sensor identity/order remains unchanged; only transport primaries repeat.
    pub fn coefficient_sensor_text(&self, sensors: &str) -> Result<String> {
        self.validate()?;
        ensure!(
            self.external_primary.is_none(),
            "External primaries must use the bounded iterator, not an expanded sensor file"
        );
        let lines: Vec<_> = sensors.lines().collect();
        ensure!(
            (1..=MAX_SENSORS).contains(&lines.len()) && sensors.ends_with('\n'),
            "Invalid authoritative coefficient sensor input"
        );
        for line in &lines {
            let values = line
                .split_whitespace()
                .map(str::parse::<f64>)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            ensure!(
                values.len() == 6 && values.iter().all(|value| value.is_finite()),
                "Coefficient sensor records require six finite coordinates/directions"
            );
        }
        let Some(repeats) = self.coefficient_primary_repetitions else {
            return Ok(sensors.into());
        };
        let mut output = String::new();
        for line in lines {
            for _ in 0..repeats {
                output.push_str(line);
                output.push('\n');
            }
        }
        Ok(output)
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
                    || (self.adapter_version == REPEATED_ADAPTER_VERSION
                        && self.method_version == REPEATED_METHOD_VERSION)
                    || (self.adapter_version == EXPLICIT_ADAPTER_VERSION
                        && self.method_version == EXPLICIT_METHOD_VERSION)
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
            REPEATED_METHOD_VERSION => REPEATED_ADAPTER_VERSION,
            EXPLICIT_METHOD_VERSION => EXPLICIT_ADAPTER_VERSION,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_streams_sha256: Option<String>,
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
    pub fn validate_for_method(&self, method: &str) -> Result<()> {
        AnnualProfile::shipping_for_method(method)?;
        self.validate()?;
        ensure!(
            if method == METHOD_VERSION {
                self.measurement_policy == RESOURCE_MEASUREMENT_POLICY
            } else {
                matches!(
                    self.measurement_policy.as_str(),
                    FILE_RESOURCE_MEASUREMENT_POLICY | LEGACY_RESOURCE_MEASUREMENT_POLICY
                )
            },
            "Resource policy does not match transport method"
        );
        if method == METHOD_VERSION {
            ensure!(
                self.processes.iter().all(|p| p.elapsed_ms <= 7_200_000
                    && p.peak_child_working_set_bytes > 0
                    && p.peak_child_working_set_bytes <= self.max_observed_working_set_bytes
                    && p.peak_observed_runner_plus_child_working_set_bytes > 0),
                "Method-5 process duration or resource observation violates its compiled policy"
            );
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == "greenlens.annual-resources.v1"
                && matches!(
                    self.measurement_policy.as_str(),
                    RESOURCE_MEASUREMENT_POLICY
                        | FILE_RESOURCE_MEASUREMENT_POLICY
                        | LEGACY_RESOURCE_MEASUREMENT_POLICY
                ),
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
            (5..=9 + HOURS).contains(&self.processes.len()),
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

#[derive(Clone, Copy)]
enum AnnualInput<'a> {
    File(&'a str),
    Primary(PrimaryPurpose),
}

fn retain_primary_failure(
    engine: &Engine,
    scratch: &Path,
    purpose: PrimaryPurpose,
    process_index: usize,
    arguments: &[String],
    error: &anyhow::Error,
) -> Result<()> {
    let stage = scratch.join(match purpose {
        PrimaryPurpose::Full => "primary-full",
        PrimaryPurpose::Direct => "primary-direct",
    });
    let mut diagnostic = serde_json::json!({"schema_version":"greenlens.annual-primary-failure.v1","state":"failed","purpose":purpose,"process_index":process_index,"arguments":arguments,"recorded_utc":Utc::now().to_rfc3339(),"error":format!("{error:#}")});
    for (name, key) in [
        ("stream-failure.json", "stream_failure"),
        (
            "stream-success.json",
            "completed_process_before_adapter_validation",
        ),
    ] {
        let file = stage.join(name);
        if file.is_file() {
            let mut data = Vec::new();
            File::open(file)?
                .take(1024 * 1024 + 1)
                .read_to_end(&mut data)?;
            ensure!(
                data.len() <= 1024 * 1024,
                "Primary failure diagnostic exceeds bound"
            );
            diagnostic[key] = serde_json::from_slice(&data)?;
        }
    }
    fn sanitize(value: &mut serde_json::Value, replacements: &[(String, &str)]) {
        match value {
            serde_json::Value::String(text) => {
                for (path, label) in replacements {
                    *text = text.replace(path, label);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    sanitize(item, replacements)
                }
            }
            serde_json::Value::Object(items) => {
                for item in items.values_mut() {
                    sanitize(item, replacements)
                }
            }
            _ => {}
        }
    }
    let replacements = [
        (scratch.to_string_lossy().into_owned(), "<scratch>"),
        (
            scratch
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .to_owned(),
            "<scratch>",
        ),
        (engine.root().to_string_lossy().into_owned(), "<engine>"),
        (
            engine
                .root()
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .to_owned(),
            "<engine>",
        ),
    ];
    sanitize(&mut diagnostic, &replacements);
    fs::write(
        scratch.join("annual-primary-failure.json"),
        serde_json::to_vec_pretty(&diagnostic)?,
    )?;
    Ok(())
}

fn validate_paired_coefficients(
    scratch: &Path,
    sensors: usize,
    sky_rows: usize,
    cancel: &AtomicBool,
) -> Result<()> {
    let mut full = MatrixReader::open(&scratch.join("annual-coefficients.mtx"), sensors, sky_rows)?;
    let mut direct = MatrixReader::open(
        &scratch.join("annual-direct-coefficients.mtx"),
        sensors,
        sky_rows,
    )?;
    for index in 0..sensors * sky_rows {
        if index % sky_rows == 0 {
            cancelled(cancel)?;
        }
        let a = full
            .next_rgb()?
            .context("Incomplete paired full coefficients")?;
        let b = direct
            .next_rgb()?
            .context("Incomplete paired direct coefficients")?;
        ensure!(a.iter().zip(b).all(|(a,b)| (*a-b).is_finite() && *a-b >= 0.0), "Negative/nonfinite paired coefficient residual at flattened sensor/patch {index}; no residual is clamped");
    }
    ensure!(
        full.next_rgb()?.is_none() && direct.next_rgb()?.is_none(),
        "Excess paired coefficients"
    );
    Ok(())
}

#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
fn execute_primary(
    engine: &Engine,
    model: &ValidatedModel,
    profile: &AnnualProfile,
    scratch: &Path,
    args: &[String],
    output: &str,
    purpose: PrimaryPurpose,
    process_index: usize,
    cancel: &AtomicBool,
) -> Result<(ProcessRecord, AnnualProcessResources, PrimaryStreamRecord)> {
    use crate::{
        annual_primary::HammersleyPrimary,
        annual_stream::{execute_in_run, StreamLimits, StreamRequest},
    };
    let contract = profile
        .external_primary
        .as_ref()
        .context("Missing compiled primary contract")?;
    let generator = HammersleyPrimary::new(&model.sensor_grid.sensors, contract.count)?;
    let stage = scratch.join(match purpose {
        PrimaryPurpose::Full => "primary-full",
        PrimaryPurpose::Direct => "primary-direct",
    });
    fs::create_dir(&stage).context("Primary stage already exists or cannot be created")?;
    let request = StreamRequest {
        executable: engine.root().join("bin/rcontrib.exe"),
        arguments: args.into(),
        directory: stage.clone(),
        child_environment: BTreeMap::from([(
            "RAYPATH".into(),
            format!(".;{}", engine.root().join("lib").display()),
        )]),
        expected_records: generator.expected_records(),
        limits: StreamLimits {
            max_input_bytes: generator.expected_records()
                * crate::annual_primary::MAX_RECORD_BYTES as u64,
            ..StreamLimits::default()
        },
    };
    let mut outcome = execute_in_run(&request, generator, cancel, scratch)?;
    // Sanitize the separate library configuration from retained diagnostic text.
    for text in [
        engine.root().to_string_lossy().into_owned(),
        engine
            .root()
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .to_owned(),
    ] {
        outcome.process.stderr = outcome.process.stderr.replace(&text, "<engine>");
    }
    outcome.telemetry.validate()?;
    let matrix = stage.join("stream-stdout.bin");
    MatrixReader::open(
        &matrix,
        model.sensor_grid.sensors.len(),
        144 * profile.sky_basis_mf * profile.sky_basis_mf + 2,
    )?
    .finish()?;
    let matrix_sha256 = direct::file_hash(&matrix)?;
    let output_bytes = File::open(&matrix)?.metadata()?.len();
    let measurement = AnnualProcessResources {
        executable: outcome.process.executable.clone(),
        started_utc: outcome.process.started_utc.clone(),
        finished_utc: outcome.process.finished_utc.clone(),
        elapsed_ms: outcome.process.elapsed_ms,
        peak_child_working_set_bytes: outcome.telemetry.peak_child_working_set_bytes,
        peak_observed_runner_plus_child_working_set_bytes: outcome
            .telemetry
            .sampled_peak_runner_plus_child_working_set_bytes,
        peak_scratch_bytes: outcome.telemetry.peak_scratch_bytes,
        output_bytes,
        samples: outcome.telemetry.samples,
    };
    let evidence = PrimaryStreamRecord {
        purpose,
        process_index,
        input: outcome.input,
        matrix_sha256,
        resources: outcome.telemetry,
    };
    ensure!(
        !scratch.join(output).exists(),
        "Primary matrix destination already exists"
    );
    fs::rename(matrix, scratch.join(output))?;
    Ok((outcome.process, measurement, evidence))
}

#[cfg(not(windows))]
#[allow(clippy::too_many_arguments)]
fn execute_primary(
    _engine: &Engine,
    _model: &ValidatedModel,
    _profile: &AnnualProfile,
    _scratch: &Path,
    _args: &[String],
    _output: &str,
    _purpose: PrimaryPurpose,
    _process_index: usize,
    _cancel: &AtomicBool,
) -> Result<(ProcessRecord, AnnualProcessResources, PrimaryStreamRecord)> {
    anyhow::bail!("Streamed climate transport requires native Windows 10 or later")
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
    let paired = method == METHOD_VERSION;
    let repeated = method == REPEATED_METHOD_VERSION;
    let scaled = repeated || paired;
    let patch_solar = method != EXPLICIT_METHOD_VERSION;
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
    if scaled {
        fs::write(
            scratch.join("annual-coefficient-receiver.rad"),
            profile.coefficient_receiver_text(),
        )?;
        if repeated {
            fs::write(
                scratch.join("annual-coefficient-sensors.pts"),
                profile.coefficient_sensor_text(&sensors)?,
            )?;
        }
    }
    let mut primary_records = Vec::new();
    let mut processes = Vec::new();
    let mut resources = AnnualResources {
        schema_version: "greenlens.annual-resources.v1".into(),
        measurement_policy: if paired {
            RESOURCE_MEASUREMENT_POLICY
        } else {
            FILE_RESOURCE_MEASUREMENT_POLICY
        }
        .into(),
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
    let mut execute = |name: &str,
                       args: Vec<String>,
                       input: Option<AnnualInput<'_>>,
                       output: &str|
     -> Result<()> {
        cancelled(cancel)?;
        let input_file = match input {
            Some(AnnualInput::File(file)) => Some(scratch.join(file)),
            _ => None,
        };
        for suffix in ["resources.json", "process.json"] {
            let path = scratch.join(format!("{name}.{suffix}"));
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
        let result = if let Some(AnnualInput::Primary(purpose)) = input {
            let result = execute_primary(
                engine,
                &model,
                profile,
                scratch,
                &args,
                output,
                purpose,
                processes.len(),
                cancel,
            );
            match result {
                Ok((record, measurement, primary)) => {
                    primary_records.push(primary);
                    Ok((record, measurement))
                }
                Err(error) => {
                    retain_primary_failure(
                        engine,
                        scratch,
                        purpose,
                        processes.len(),
                        &args,
                        &error,
                    )?;
                    Err(error)
                }
            }
        } else if paired {
            process::execute_annual_with_deadline(
                engine.root(),
                name,
                &args,
                scratch,
                input_file.as_deref(),
                &scratch.join(output),
                cancel,
                Duration::from_secs(7200),
            )
        } else {
            process::execute_annual(
                engine.root(),
                name,
                &args,
                scratch,
                input_file.as_deref(),
                &scratch.join(output),
                cancel,
            )
        };
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
    if patch_solar {
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
    if patch_solar {
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
    if scaled {
        execute(
            "oconv",
            ["-f", "annual-scene.rad", "annual-coefficient-receiver.rad"]
                .map(String::from)
                .to_vec(),
            None,
            "annual-coefficient.oct",
        )?;
    }
    let coefficient_input = if paired {
        AnnualInput::Primary(PrimaryPurpose::Full)
    } else if repeated {
        AnnualInput::File("annual-coefficient-sensors.pts")
    } else {
        AnnualInput::File("annual-sensors.pts")
    };
    on_log("Tracing diffuse and ground daylight coefficients; cancellation remains available");
    execute(
        "rcontrib",
        profile.coefficient_arguments(sensor_count, false),
        Some(coefficient_input),
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
    if !sun_indices.is_empty() && patch_solar {
        on_log("Tracing direct-only sky-patch coefficients for solar correction");
        execute(
            "rcontrib",
            profile.coefficient_arguments(sensor_count, true),
            Some(if paired {
                AnnualInput::Primary(PrimaryPurpose::Direct)
            } else {
                coefficient_input
            }),
            "annual-direct-coefficients.mtx",
        )?;
        MatrixReader::open(
            &scratch.join("annual-direct-coefficients.mtx"),
            sensor_count,
            sky_rows,
        )?
        .finish()?;
        if paired {
            validate_paired_coefficients(scratch, sensor_count, sky_rows, cancel)?;
        }
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
    if !sun_indices.is_empty() && !patch_solar {
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
            Some(AnnualInput::File("annual-sensors.pts")),
            "annual-solar-rgb.mtx",
        )?;
        on_log("Tracing the identical centroid-direct solar term for finite-disc replacement");
        execute(
            "rcontrib",
            profile.solar_arguments(sensor_count, true),
            Some(AnnualInput::File("annual-sensors.pts")),
            "annual-solar-direct-rgb.mtx",
        )?;
    }
    cancelled(cancel)?;
    on_log("Validating every RGB value and assembling complete ordered time series");
    // <=256*8760*24 = 53,821,440 bytes. Large sky matrices are streamed and are
    // never retained together with another unbounded in-memory representation.
    let normalization = if paired { std::f64::consts::PI } else { 1.0 }
        / profile.coefficient_receiver_scale.unwrap_or(1.0);
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
                combined[position * sensor_count + sensor] = rgb.map(|value| {
                    if paired {
                        value * normalization
                    } else {
                        value / profile.coefficient_receiver_scale.unwrap_or(1.0)
                    }
                });
            }
        }
        cancelled(cancel)?;
    }
    ensure!(diffuse.next_rgb()?.is_none(), "Excess diffuse output");
    if !sun_indices.is_empty() && patch_solar {
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
                        let reflected = if paired {
                            (rgb[channel] - direct[channel]) * normalization
                        } else {
                            (rgb[channel] - direct[channel])
                                / profile.coefficient_receiver_scale.unwrap_or(1.0)
                        };
                        if scaled {
                            ensure!(reflected.is_finite() && reflected >= 0.0,
                                "Invalid reflected solar correction at sensor {} interval {index}; no negative residual is clamped", sensor + 1);
                        }
                        combined[position * sensor_count + sensor][channel] += reflected;
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
    if !sun_indices.is_empty() && !patch_solar {
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
                Some(AnnualInput::File("annual-direct-rays.pts")),
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
    let primary_streams_sha256 = if paired {
        let streams = PrimaryStreams {
            schema_version: PRIMARY_STREAMS_SCHEMA_VERSION.into(),
            contract: profile
                .external_primary
                .clone()
                .expect("compiled primary profile"),
            sensor_sha256: digest(sensors.as_bytes()),
            sensor_count,
            sky_rows,
            records: primary_records,
        };
        streams.validate(
            &streams.contract,
            sensor_count,
            &streams.sensor_sha256,
            sky_rows,
            !sun_indices.is_empty(),
            &processes,
        )?;
        let bytes = serde_json::to_vec_pretty(&streams)?;
        fs::write(scratch.join("annual-primary-streams.json"), &bytes)?;
        Some(digest(&bytes))
    } else {
        None
    };
    let recipe = AnnualRecipe {
        schema_version: if legacy {
            LEGACY_RECIPE_SCHEMA_VERSION
        } else if method == EXPLICIT_METHOD_VERSION {
            EXPLICIT_RECIPE_SCHEMA_VERSION
        } else if repeated {
            REPEATED_RECIPE_SCHEMA_VERSION
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
        } else if method == EXPLICIT_METHOD_VERSION {
            EXPLICIT_GROUND_POLICY
        } else if repeated {
            REPEATED_GROUND_POLICY
        } else {
            GROUND_POLICY
        }
        .into(),
        direct_policy: if legacy {
            LEGACY_DIRECT_POLICY
        } else if method == EXPLICIT_METHOD_VERSION {
            EXPLICIT_DIRECT_POLICY
        } else if repeated {
            REPEATED_DIRECT_POLICY
        } else {
            DIRECT_POLICY
        }
        .into(),
        direct_batches,
        primary_streams_sha256,
        child_environment: BTreeMap::from([("RAYPATH".into(), ".;<engine>/lib".into())]),
        processes: processes.clone(),
    };
    fs::write(
        scratch.join("annual-recipe.json"),
        serde_json::to_vec_pretty(&recipe)?,
    )?;
    check_scratch(scratch)?;
    resources.refresh(start, scratch)?;
    resources.validate_for_method(method)?;
    cancelled(cancel)?;
    let mut compact_files = [
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
    .to_vec();
    if repeated {
        compact_files.extend(
            [
                "annual-coefficient-receiver.rad",
                "annual-coefficient-sensors.pts",
            ]
            .map(String::from),
        );
    }
    if paired {
        compact_files.extend(
            [
                "annual-coefficient-receiver.rad",
                "annual-primary-streams.json",
            ]
            .map(String::from),
        );
    }
    Ok(AnnualOutput {
        combined_f64,
        sensor_count,
        interval_indices: output_indices,
        identity,
        profile: profile.clone(),
        processes,
        compact_files,
        recipe,
        resources,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resource_policy_preserves_exact_historical_records_and_rejects_unknown_policy() {
        // Synthetic metadata only; this does not claim a native engine run.
        let timestamp = "2026-10-11T00:00:00Z".to_string();
        let measurement = AnnualProcessResources {
            executable: "oconv.exe".into(),
            started_utc: timestamp.clone(),
            finished_utc: timestamp.clone(),
            elapsed_ms: 0,
            peak_child_working_set_bytes: 1,
            peak_observed_runner_plus_child_working_set_bytes: 2,
            peak_scratch_bytes: 3,
            output_bytes: 3,
            samples: 1,
        };
        let mut record = AnnualResources {
            schema_version: "greenlens.annual-resources.v1".into(),
            measurement_policy: RESOURCE_MEASUREMENT_POLICY.into(),
            started_utc: timestamp.clone(),
            finished_utc: timestamp,
            total_wall_ms: 0,
            peak_engine_working_set_bytes: 1,
            peak_observed_runner_plus_engine_working_set_bytes: 2,
            peak_scratch_bytes: 3,
            max_scratch_bytes: process::ANNUAL_MAX_SCRATCH_BYTES,
            max_matrix_bytes: process::ANNUAL_MAX_FILE_BYTES,
            max_observed_working_set_bytes: process::ANNUAL_MAX_OBSERVED_WORKING_SET_BYTES,
            processes: vec![measurement; 5],
        };
        record.validate().unwrap();
        record.validate_for_method(METHOD_VERSION).unwrap();
        let mut excessive = record.clone();
        excessive.total_wall_ms = 7_200_001;
        excessive.processes[0].elapsed_ms = 7_200_001;
        assert!(excessive.validate_for_method(METHOD_VERSION).is_err());
        let mut incomplete = record.clone();
        incomplete.processes[0].peak_child_working_set_bytes = 0;
        assert!(incomplete.validate_for_method(METHOD_VERSION).is_err());
        assert!(record.validate_for_method(REPEATED_METHOD_VERSION).is_err());
        record.measurement_policy = FILE_RESOURCE_MEASUREMENT_POLICY.into();
        record.validate_for_method(REPEATED_METHOD_VERSION).unwrap();
        assert!(record.validate_for_method(METHOD_VERSION).is_err());
        record.measurement_policy = LEGACY_RESOURCE_MEASUREMENT_POLICY.into();
        let original = serde_json::to_vec(&record).unwrap();
        let historical: AnnualResources = serde_json::from_slice(&original).unwrap();
        historical.validate().unwrap();
        assert_eq!(serde_json::to_vec(&historical).unwrap(), original);
        record.measurement_policy = "Measured at a guaranteed 20ms cadence".into();
        assert!(record.validate().is_err());
    }
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
        for profile in [
            AnnualProfile::explicit_shipping(),
            AnnualProfile::explicit_reference(),
        ] {
            assert_eq!(profile.method().unwrap(), EXPLICIT_METHOD_VERSION);
            assert!(profile.validate_for_method(METHOD_VERSION).is_err());
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
    #[test]
    fn repeated_coefficient_input_is_contiguous_and_historical_bytes_are_unchanged() {
        let sensors = "1 2 0.8 0 0 1\n3 4 0.8 0 1 0\n";
        for method in [LEGACY_METHOD_VERSION, EXPLICIT_METHOD_VERSION] {
            let profile = AnnualProfile::shipping_for_method(method).unwrap();
            assert_eq!(profile.coefficient_sensor_text(sensors).unwrap(), sensors);
            assert_eq!(profile.coefficient_receiver_text(), RECEIVER_TEXT);
            let serialized = serde_json::to_value(&profile).unwrap();
            assert!(serialized.get("coefficient_primary_repetitions").is_none());
            assert!(serialized.get("coefficient_receiver_scale").is_none());
            assert_eq!(profile.ray_depth, 16);
            assert!(!profile
                .coefficient_arguments(2, false)
                .contains(&"-c".into()));
        }
        for profile in [
            AnnualProfile::repeated_shipping(),
            AnnualProfile::repeated_reference(),
        ] {
            let repeats = profile.coefficient_primary_repetitions.unwrap();
            let text = profile.coefficient_sensor_text(sensors).unwrap();
            let lines: Vec<_> = text.lines().collect();
            assert_eq!(lines.len(), 2 * repeats);
            assert!(lines[..repeats].iter().all(|line| *line == "1 2 0.8 0 0 1"));
            assert!(lines[repeats..].iter().all(|line| *line == "3 4 0.8 0 1 0"));
            assert!(profile
                .coefficient_receiver_text()
                .contains("4 100 100 100 0\n"));
            assert_eq!(profile.ray_depth, -16);
            assert_eq!(
                profile.transmission_arguments().last().unwrap(),
                "annual-diffuse.oct"
            );
            assert!(profile.coefficient_sensor_text("0 0 NaN 0 0 1\n").is_err());
            assert!(profile
                .coefficient_sensor_text("0 0 0 0 0 1\n".repeat(257).as_str())
                .is_err());
            let mut invalid = profile.clone();
            invalid.coefficient_receiver_scale = None;
            assert!(invalid.validate().is_err());
            let mut invalid = profile.clone();
            invalid.coefficient_primary_repetitions = Some(1);
            assert!(invalid.validate().is_err());
        }
    }
    #[test]
    fn paired_profile_binds_radiance_arguments_and_rejects_expanded_input() {
        for profile in [AnnualProfile::shipping(), AnnualProfile::reference()] {
            assert_eq!(profile.method().unwrap(), METHOD_VERSION);
            let contract = profile.external_primary.as_ref().unwrap();
            let full = profile.coefficient_arguments(24, false);
            let direct = profile.coefficient_arguments(24, true);
            assert!(!full.iter().any(|a| a.starts_with("-I")));
            assert!(!direct.iter().any(|a| a.starts_with("-I")));
            assert!(full.contains(&"-V+".into()));
            let ab = full.iter().position(|a| a == "-ab").unwrap();
            let count = full.iter().position(|a| a == "-c").unwrap();
            assert_eq!(direct[ab + 1], "0");
            assert_eq!(full[count + 1], contract.count.to_string());
            assert_eq!(direct[count + 1], full[count + 1]);
            assert_eq!(full.last().unwrap(), "annual-coefficient.oct");
            let mut comparable = direct.clone();
            comparable[ab + 1] = full[ab + 1].clone();
            assert_eq!(comparable, full);
            assert_eq!(profile.coefficient_receiver_text(), "void glow sky_glow\n0\n0\n4 100 100 100 0\nsky_glow source dome\n0\n0\n4 0 0 1 360\n");
            assert!(profile.coefficient_sensor_text("0 0 0.8 0 0 1\n").is_err());
            assert_eq!(profile.coefficient_primary_repetitions, None);
            assert_eq!(profile.solar_ambient_divisions, None);
            let mut changed = profile.clone();
            changed.external_primary.as_mut().unwrap().count = 262_144;
            assert!(changed.validate().is_err());
            let mut changed = profile.clone();
            changed.external_primary = None;
            assert!(changed.validate().is_err());
        }
        for method in [
            LEGACY_METHOD_VERSION,
            EXPLICIT_METHOD_VERSION,
            REPEATED_METHOD_VERSION,
        ] {
            let profile = AnnualProfile::shipping_for_method(method).unwrap();
            assert!(serde_json::to_value(&profile)
                .unwrap()
                .get("external_primary")
                .is_none());
            assert_ne!(profile.method().unwrap(), METHOD_VERSION);
        }
    }

    #[test]
    fn paired_coefficient_check_rejects_negative_bin_even_when_product_can_compensate() {
        let directory = std::env::temp_dir().join(format!(
            "greenlens-paired-coefficients-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        fs::create_dir(&directory).unwrap();
        let header = "NROWS=1\nNCOLS=2\nNCOMP=3\nFORMAT=ascii\n\n";
        fs::write(
            directory.join("annual-coefficients.mtx"),
            format!("{header}1 1 1 3 3 3\n"),
        )
        .unwrap();
        fs::write(
            directory.join("annual-direct-coefficients.mtx"),
            format!("{header}2 2 2 0 0 0\n"),
        )
        .unwrap();
        assert!(validate_paired_coefficients(&directory, 1, 2, &AtomicBool::new(false)).is_err());
        fs::write(
            directory.join("annual-direct-coefficients.mtx"),
            format!("{header}1 1 1 0 0 0\n"),
        )
        .unwrap();
        validate_paired_coefficients(&directory, 1, 2, &AtomicBool::new(false)).unwrap();
        assert!(validate_paired_coefficients(&directory, 1, 2, &AtomicBool::new(true)).is_err());
    }
    #[test]
    fn primary_failure_summary_is_bounded_sanitized_and_preserves_original_stage() {
        let scratch = std::env::temp_dir().join(format!(
            "greenlens-primary-failure-{}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        fs::create_dir_all(scratch.join("primary-full")).unwrap();
        // Synthetic identity is never used to discover or execute an engine.
        let engine = Engine {
            root: scratch.join("private-engine"),
            identity: crate::EngineIdentity {
                version: "MOCK".into(),
                adapter_version: "MOCK".into(),
                executable_sha256: BTreeMap::new(),
                library_sha256: BTreeMap::new(),
                fingerprint: String::new(),
            },
        };
        let original=serde_json::to_vec(&serde_json::json!({"schema_version":"greenlens.stream-failure.v1","state":"failed","error":format!("failed in {} using {}",scratch.display(),engine.root().display()),"pid":1,"written_bytes":0,"generated_records":0,"process_tree_terminated":true,"cleanup_error":null,"telemetry":null})).unwrap();
        fs::write(scratch.join("primary-full/stream-failure.json"), &original).unwrap();
        retain_primary_failure(
            &engine,
            &scratch,
            PrimaryPurpose::Full,
            4,
            &["-c".into(), "16384".into()],
            &anyhow::anyhow!("failed at {}", scratch.display()),
        )
        .unwrap();
        let bytes = fs::read(scratch.join("annual-primary-failure.json")).unwrap();
        let diagnostic: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(diagnostic["state"], "failed");
        assert_eq!(diagnostic["purpose"], "full");
        assert_eq!(
            diagnostic["stream_failure"]["process_tree_terminated"],
            true
        );
        assert!(!diagnostic["error"]
            .as_str()
            .unwrap()
            .contains(scratch.to_string_lossy().as_ref()));
        assert_eq!(
            fs::read(scratch.join("primary-full/stream-failure.json")).unwrap(),
            original
        );
        fs::write(
            scratch.join("primary-full/stream-failure.json"),
            vec![b'x'; 1024 * 1024 + 1],
        )
        .unwrap();
        assert!(retain_primary_failure(
            &engine,
            &scratch,
            PrimaryPurpose::Full,
            4,
            &[],
            &anyhow::anyhow!("mock")
        )
        .is_err());
        assert_eq!(
            fs::read(scratch.join("annual-primary-failure.json")).unwrap(),
            bytes
        );
    }
    #[cfg(windows)]
    #[test]
    fn synthetic_primary_failure_retains_actual_supervisor_diagnostic() {
        let scratch = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
            .join("GreenLens/verification")
            .join(format!(
                "annual primary failure synthetic 空間 {}",
                Utc::now().timestamp_nanos_opt().unwrap()
            ));
        let engine_root = scratch.join("fixture-engine");
        fs::create_dir_all(engine_root.join("bin")).unwrap();
        // This is a tiny native test executable, not a simulation engine. Listing
        // its tests exits without consuming the generated stdin stream.
        fs::copy(
            std::env::current_exe().unwrap(),
            engine_root.join("bin/rcontrib.exe"),
        )
        .unwrap();
        let engine = Engine {
            root: engine_root,
            identity: crate::EngineIdentity {
                version: "MOCK synthetic test executable".into(),
                adapter_version: "MOCK".into(),
                executable_sha256: BTreeMap::new(),
                library_sha256: BTreeMap::new(),
                fingerprint: String::new(),
            },
        };
        let model = greenlens_core::validate_model(&greenlens_core::Model::default()).unwrap();
        let args = vec!["--list".to_string()];
        let error = execute_primary(
            &engine,
            &model,
            &AnnualProfile::shipping(),
            &scratch,
            &args,
            "annual-coefficients.mtx",
            PrimaryPurpose::Full,
            4,
            &AtomicBool::new(false),
        )
        .unwrap_err();
        retain_primary_failure(&engine, &scratch, PrimaryPurpose::Full, 4, &args, &error).unwrap();
        let diagnostic: serde_json::Value =
            serde_json::from_slice(&fs::read(scratch.join("annual-primary-failure.json")).unwrap())
                .unwrap();
        assert_eq!(diagnostic["state"], "failed");
        assert_eq!(
            diagnostic["stream_failure"]["process_tree_terminated"],
            true
        );
        assert_eq!(
            diagnostic["stream_failure"]["telemetry"]["schema_version"],
            "greenlens.stream-resources.v2"
        );
        assert_eq!(
            diagnostic["stream_failure"]["telemetry"]["atomic_job_assignment"],
            true
        );
        assert!(
            diagnostic["stream_failure"]["generated_records"]
                .as_u64()
                .unwrap()
                < model.sensor_grid.sensors.len() as u64 * 16_384
        );
        assert!(!scratch.join("annual-coefficients.mtx").exists());
        assert!(!scratch.join("annual-primary-streams.json").exists());
        eprintln!(
            "Synthetic primary failure diagnostic: {}",
            scratch.display()
        );
    }
    #[cfg(windows)]
    #[test]
    fn native_repeated_scaled_coefficients_integrate_uniform_radiance() {
        let engine = Engine::discover(None).expect("Required native Radiance is unavailable");
        let root = PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("Native local cache"))
            .join("GreenLens/verification")
            .join(format!(
                "annual repeated coefficients 空間 {}",
                Utc::now().timestamp_nanos_opt().unwrap()
            ));
        fs::create_dir_all(&root).unwrap();
        let cancel = AtomicBool::new(false);
        for profile in [
            AnnualProfile::repeated_shipping(),
            AnnualProfile::repeated_reference(),
        ] {
            let directory = root.join(&profile.name);
            fs::create_dir_all(&directory).unwrap();
            fs::write(
                directory.join("annual-coefficient-receiver.rad"),
                profile.coefficient_receiver_text(),
            )
            .unwrap();
            fs::write(
                directory.join("annual-coefficient-sensors.pts"),
                profile
                    .coefficient_sensor_text("0 0 0 0 0 1\n0 0 0 0 1 0\n")
                    .unwrap(),
            )
            .unwrap();
            process::execute(
                engine.root(),
                "oconv",
                &["-f".into(), "annual-coefficient-receiver.rad".into()],
                &directory,
                None,
                &directory.join("annual-coefficient.oct"),
                &cancel,
            )
            .unwrap();
            process::execute(
                engine.root(),
                "rcontrib",
                &profile.coefficient_arguments(2, true),
                &directory,
                Some(&directory.join("annual-coefficient-sensors.pts")),
                &directory.join("coefficients.mtx"),
                &cancel,
            )
            .unwrap();
            let bins = 144 * profile.sky_basis_mf * profile.sky_basis_mf + 2;
            let mut matrix =
                MatrixReader::open(&directory.join("coefficients.mtx"), 2, bins).unwrap();
            for _ in 0..2 {
                let mut rgb = [0.0; 3];
                for _ in 0..bins {
                    let value = matrix.next_rgb().unwrap().unwrap();
                    for channel in 0..3 {
                        rgb[channel] += value[channel] / 100.0;
                    }
                }
                for value in rgb {
                    assert!((value - std::f64::consts::PI).abs() <= 0.01 * std::f64::consts::PI,
                        "Uniform-sky integral must equal pi after exactly one receiver normalization: {value}");
                }
            }
            assert!(matrix.next_rgb().unwrap().is_none());
        }
    }
}
