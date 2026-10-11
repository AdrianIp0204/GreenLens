//! Pinned native Radiance adapter. This module owns subprocess execution;
//! physical model validation and unit conversions remain in greenlens-core.
pub mod annual;
#[cfg(windows)]
pub mod annual_stream;
mod process;
pub mod science;
pub use process::ProcessRecord;

use anyhow::{bail, Context, Result};
use greenlens_core::ValidatedModel;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

pub const ADAPTER_VERSION: &str = "radiance-static-overcast/1";
pub const ENGINE_VERSION: &str = "RADIANCE 6.0.2 2026-02-02 LBNL (6.0.c1700d56cc)";
pub const COMPACT_FILES: &[&str] = &[
    "sky.rad",
    "scene.rad",
    "sensors.pts",
    "raw.rgb",
    "logs.json",
    "recipe.json",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EngineIdentity {
    pub version: String,
    pub adapter_version: String,
    pub executable_sha256: BTreeMap<String, String>,
    pub library_sha256: BTreeMap<String, String>,
    pub fingerprint: String,
}

#[derive(Debug, Clone)]
pub struct Engine {
    root: PathBuf,
    identity: EngineIdentity,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

// Radiance's C path parser rejects the extended prefix produced by Rust's
// canonicalize on Windows. Keep the resolved path but use Win32/UNC spelling
// for its child-only library search path and executable arguments.
fn engine_runtime_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path
}

impl Engine {
    pub fn discover(root: Option<&Path>) -> Result<Self> {
        let root = match root {
            Some(path) => path.to_path_buf(),
            None => std::env::var_os("GREENLENS_RADIANCE")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default())
                        .join("GreenLens/engines/radiance-6.0.2/package")
                }),
        };
        if !root.join("bin/rtrace.exe").is_file() {
            bail!("Radiance is unavailable. Install the pinned engine using scripts/install-radiance.ps1 or choose its package directory.");
        }
        let root = engine_runtime_path(root.canonicalize()?);
        let mut executable_sha256 = BTreeMap::new();
        for (name, expected) in [
            (
                "gensky.exe",
                "72745cf8f263d34549d0c01d86a7e04b5cb2558cab483d8f27417c95913409f8",
            ),
            (
                "oconv.exe",
                "e0bc7240ee2645d6590c33ef5a6a783f4bfe81246dce23bfefdc66966cba60a2",
            ),
            (
                "rtrace.exe",
                "edb01ac56b31ad79263c1f42a686413c0d369829f460e83a170340a6f821b5c9",
            ),
        ] {
            let actual = digest(
                &fs::read(root.join("bin").join(name))
                    .with_context(|| format!("Read required engine executable {name}"))?,
            );
            if actual != expected {
                bail!("Engine {name} does not match the tested Radiance 6.0.2 artifact. Reinstall the pinned package.");
            }
            executable_sha256.insert(name.into(), actual);
        }
        let mut library_sha256 = BTreeMap::new();
        fn visit(base: &Path, path: &Path, out: &mut BTreeMap<String, String>) -> Result<()> {
            for item in fs::read_dir(path)? {
                let item = item?;
                if item.file_type()?.is_symlink() {
                    bail!("Engine library must not contain symbolic links");
                }
                if item.file_type()?.is_dir() {
                    visit(base, &item.path(), out)?;
                } else {
                    out.insert(
                        item.path()
                            .strip_prefix(base)?
                            .to_string_lossy()
                            .replace('\\', "/"),
                        digest(&fs::read(item.path())?),
                    );
                }
            }
            Ok(())
        }
        visit(&root, &root.join("lib"), &mut library_sha256)?;
        for required in ["lib/rayinit.cal", "lib/skybright.cal"] {
            if !library_sha256.contains_key(required) {
                bail!("Required Radiance library missing: {required}");
            }
        }
        for (name, expected) in [
            (
                "lib/rayinit.cal",
                "293b321513e9fedce90ad6e88f2ef00d6e9184d0581e8fdeb111fd05fa32ae60",
            ),
            (
                "lib/skybright.cal",
                "323d4762faa32b6a0ee086d97df6b04ba39416385af927247265de6d428fe454",
            ),
        ] {
            if library_sha256.get(name).map(String::as_str) != Some(expected) {
                bail!("Critical Radiance library {name} differs from the pinned package");
            }
        }
        library_sha256.insert(
            "bin/vcruntime140.dll".into(),
            digest(&fs::read(root.join("bin/vcruntime140.dll"))?),
        );
        let fingerprint = digest(&serde_json::to_vec(&(
            ENGINE_VERSION,
            ADAPTER_VERSION,
            &executable_sha256,
            &library_sha256,
        ))?);
        Ok(Self {
            root,
            identity: EngineIdentity {
                version: ENGINE_VERSION.into(),
                adapter_version: ADAPTER_VERSION.into(),
                executable_sha256,
                library_sha256,
                fingerprint,
            },
        })
    }
    pub fn identity(&self) -> &EngineIdentity {
        &self.identity
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    pub ambient_bounces: u32,
    pub ambient_divisions: u32,
    pub ambient_supersamples: u32,
    pub ambient_accuracy: f64,
    pub ambient_resolution: u32,
    pub limit_weight: f64,
    pub limit_reflections: u32,
}
impl Profile {
    pub fn shipping() -> Self {
        Self {
            name: "shipping-v1".into(),
            ambient_bounces: 5,
            ambient_divisions: 4096,
            ambient_supersamples: 1024,
            ambient_accuracy: 0.075,
            ambient_resolution: 192,
            limit_weight: 0.00005,
            limit_reflections: 12,
        }
    }
    pub fn reference() -> Self {
        Self {
            name: "reference-v1".into(),
            ambient_bounces: 6,
            ambient_divisions: 8192,
            ambient_supersamples: 2048,
            ambient_accuracy: 0.05,
            ambient_resolution: 256,
            limit_weight: 0.00001,
            limit_reflections: 12,
        }
    }
    pub fn arguments(&self) -> Vec<String> {
        let values = [
            "-I+".into(),
            "-h".into(),
            "-faa".into(),
            "-ov".into(),
            "-n".into(),
            "1".into(),
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
            "0".into(),
            "0".into(),
            "0".into(),
            "-lw".into(),
            self.limit_weight.to_string(),
            "-lr".into(),
            self.limit_reflections.to_string(),
            "-dt".into(),
            "0".into(),
            "-dc".into(),
            "1".into(),
            "-dj".into(),
            "0".into(),
            "-ds".into(),
            "0.05".into(),
            "-st".into(),
            "0".into(),
            "-ss".into(),
            "0".into(),
            "-u-".into(),
            "scene.oct".into(),
        ];
        values.to_vec()
    }
    pub fn validate(&self) -> Result<()> {
        if self != &Self::shipping() && self != &Self::reference() {
            bail!("Unknown numerical profile. Only the frozen shipping/reference profiles are accepted for project replay.");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineOutput {
    pub lux: Vec<f64>,
    pub raw_rgb: Vec<[f64; 3]>,
    pub processes: Vec<ProcessRecord>,
    pub identity: EngineIdentity,
    pub profile: Profile,
}

pub fn parse_rgb(text: &str, expected: usize) -> Result<Vec<[f64; 3]>> {
    let rows: Vec<_> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if rows.len() != expected {
        bail!("Radiance returned {} sensor rows; expected {expected}. Partial results are not accepted.", rows.len());
    }
    rows.into_iter()
        .enumerate()
        .map(|(i, line)| {
            let values: Vec<f64> = line
                .split_whitespace()
                .map(str::parse)
                .collect::<std::result::Result<_, _>>()
                .with_context(|| format!("Malformed RGB output at sensor {}", i + 1))?;
            if values.len() != 3 || values.iter().any(|v| !v.is_finite() || *v < 0.0) {
                bail!(
                    "Invalid RGB output at sensor {}; expected three finite nonnegative values",
                    i + 1
                );
            }
            Ok([values[0], values[1], values[2]])
        })
        .collect()
}

pub const DOME: &str = "\nskyfunc glow sky_glow\n0\n0\n4 1 1 1 0\nsky_glow source upper_sky\n0\n0\n4 0 0 1 180\nskyfunc glow ground_glow\n0\n0\n4 1 1 1 0\nground_glow source lower_ground\n0\n0\n4 0 0 -1 180\n";

/// Bounded native diagnostic: prescribed 10,000 lx overcast sky and one unobstructed sensor.
pub fn diagnose(engine: &Engine, scratch: &Path, cancel: &AtomicBool) -> Result<EngineOutput> {
    run_scene(
        engine,
        "# unobstructed native diagnostic\n",
        "0 0 0.8 0 0 1\n",
        1,
        10000.0,
        0.2,
        &Profile::shipping(),
        scratch,
        cancel,
        |_| {},
    )
}

/// Execute already validated generated scene/sensors, also used by analytical
/// fixtures. Callers cannot supply shell directives in either scene text.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_scene(
    engine: &Engine,
    scene: &str,
    sensors: &str,
    sensor_count: usize,
    exterior_lux: f64,
    ground: f64,
    profile: &Profile,
    scratch: &Path,
    cancel: &AtomicBool,
    on_log: impl Fn(&str),
) -> Result<EngineOutput> {
    if scene.lines().any(|line| line.trim_start().starts_with('!')) {
        bail!("Shell directives are prohibited in Radiance scenes");
    }
    fs::create_dir_all(scratch)?;
    fs::write(scratch.join("scene.rad"), scene)?;
    fs::write(scratch.join("sensors.pts"), sensors)?;
    on_log("Preparing prescribed overcast sky");
    let sky_args = vec![
        "-ang".into(),
        "45".into(),
        "0".into(),
        "-c".into(),
        "-B".into(),
        (exterior_lux / 179.0).to_string(),
        "-g".into(),
        ground.to_string(),
    ];
    let mut records = vec![process::execute(
        &engine.root,
        "gensky",
        &sky_args,
        scratch,
        None,
        &scratch.join("sky.rad"),
        cancel,
    )?];
    let mut sky = fs::read_to_string(scratch.join("sky.rad"))?;
    // Upstream prints its executable path in a comment; sanitize personal paths.
    sky = sky
        .lines()
        .filter(|line| !line.starts_with("# ") || !line.contains("gensky"))
        .collect::<Vec<_>>()
        .join("\n");
    sky.push_str(DOME);
    fs::write(scratch.join("sky.rad"), &sky)?;
    trace_prepared(
        engine,
        sensor_count,
        profile,
        scratch,
        cancel,
        on_log,
        &mut records,
    )
}

fn trace_prepared(
    engine: &Engine,
    sensor_count: usize,
    profile: &Profile,
    scratch: &Path,
    cancel: &AtomicBool,
    on_log: impl Fn(&str),
    records: &mut Vec<ProcessRecord>,
) -> Result<EngineOutput> {
    on_log("Compiling generated geometry and luminous hemispheres");
    records.push(process::execute(
        &engine.root,
        "oconv",
        &["-f".into(), "sky.rad".into(), "scene.rad".into()],
        scratch,
        None,
        &scratch.join("scene.oct"),
        cancel,
    )?);
    on_log("Running native Radiance illuminance calculation (one worker)");
    records.push(process::execute(
        &engine.root,
        "rtrace",
        &profile.arguments(),
        scratch,
        Some(&scratch.join("sensors.pts")),
        &scratch.join("raw.rgb"),
        cancel,
    )?);
    if cancel.load(Ordering::SeqCst) {
        bail!("Simulation cancelled; completed result not promoted");
    }
    let raw_rgb = parse_rgb(&fs::read_to_string(scratch.join("raw.rgb"))?, sensor_count)?;
    let lux = raw_rgb
        .iter()
        .map(|v| greenlens_core::rgb_to_lux(*v))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    fs::write(
        scratch.join("logs.json"),
        serde_json::to_vec_pretty(&records)?,
    )?;
    fs::write(
        scratch.join("recipe.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"schema_version":"1","adapter_version":ADAPTER_VERSION,"engine":engine.identity,"profile":profile,"child_environment":{"RAYPATH":".;<engine>/lib"},"processes":records,"instruction":"Regenerate scene.oct using validated immutable model through GreenLens replay. Do not execute package-supplied commands."}),
        )?,
    )?;
    Ok(EngineOutput {
        lux,
        raw_rgb,
        processes: records.clone(),
        identity: engine.identity.clone(),
        profile: profile.clone(),
    })
}

pub fn simulate(
    engine: &Engine,
    model: &ValidatedModel,
    profile: &Profile,
    scratch: &Path,
    cancel: &AtomicBool,
    on_log: impl Fn(&str),
) -> Result<EngineOutput> {
    profile.validate()?;
    // Never trust deserialized derived surfaces or sensor arrays. Regenerate
    // them from the validated versioned model before any engine execution.
    let model = greenlens_core::validate_model(&model.model)?;
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
    run_scene(
        engine,
        &scene,
        &sensors,
        model.sensor_grid.sensors.len(),
        model.model.sky.exterior_horizontal_illuminance_lux,
        model.model.sky.ground_reflectance,
        profile,
        scratch,
        cancel,
        on_log,
    )
}

pub fn scene_text(model: &ValidatedModel) -> Result<String> {
    use greenlens_core::SurfaceMaterial;
    let transmissivity =
        greenlens_core::glazing_transmissivity(model.model.materials.visible_transmittance)?;
    let mut text =
        String::from("# GreenLens generated validated metre geometry; no embedded commands\n");
    for (i, surface) in model.surfaces.iter().enumerate() {
        if surface.material == SurfaceMaterial::Glass {
            text.push_str(&format!("void glass material_{i}\n0\n0\n4 {transmissivity} {transmissivity} {transmissivity} 1.52\n"));
        } else {
            let reflectance = surface
                .reflectance
                .context("Opaque surface lacks reflectance")?;
            text.push_str(&format!("void plastic material_{i}\n0\n0\n5 {reflectance} {reflectance} {reflectance} 0 0\n"));
        }
        text.push_str(&format!(
            "material_{i} polygon surface_{i}\n0\n0\n{}\n",
            surface.vertices.len() * 3
        ));
        for vertex in &surface.vertices {
            text.push_str(&format!("{} {} {}\n", vertex[0], vertex[1], vertex[2]));
        }
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[cfg(windows)]
    fn radiance_runtime_paths_drop_unsupported_extended_prefixes() {
        assert_eq!(
            engine_runtime_path(PathBuf::from(r"\\?\C:\engine folder\package")),
            PathBuf::from(r"C:\engine folder\package")
        );
        assert_eq!(
            engine_runtime_path(PathBuf::from(r"\\?\UNC\host\share\package")),
            PathBuf::from(r"\\host\share\package")
        );
    }
    #[test]
    fn malformed_and_truncated_outputs_fail() {
        for value in [
            "1 2",
            "1 2 3 4",
            "NaN 0 0",
            "inf 0 0",
            "-1 0 0",
            "",
            "1 2 3\n1 2 3",
        ] {
            assert!(parse_rgb(value, 1).is_err(), "{value}");
        }
        assert_eq!(parse_rgb("1 2 3\n", 1).unwrap(), vec![[1.0, 2.0, 3.0]]);
    }
    #[test]
    fn missing_engine_fails_clearly() {
        assert!(
            Engine::discover(Some(Path::new("definitely-missing-greenlens-engine")))
                .unwrap_err()
                .to_string()
                .contains("unavailable")
        );
    }
    #[test]
    fn tampered_profile_rejected() {
        let mut p = Profile::shipping();
        p.ambient_divisions = 1;
        assert!(p.validate().is_err());
    }
}
