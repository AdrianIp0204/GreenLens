//! Explicit native bridge, isolated from production annual methods/profiles.
//! Raw outputs, all failures and process receipts stay in a fresh local directory.
use super::*;
use crate::{annual::AnnualEngineIdentity, annual_stream, process, scene_text, Engine};
use chrono::Utc;
use greenlens_core::{sha256, validate_model, Model, SurfaceMaterial};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    f64::consts::PI,
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};

const RECEIVER: &str =
    "void glow sky_glow\n0\n0\n4 100 100 100 0\nsky_glow source dome\n0\n0\n4 0 0 1 360\n";
const BINS: usize = 578;
const SENSOR_INDICES: [usize; 2] = [2, 22];

fn receiver(name: &str) -> String {
    if name.starts_with("aperture") {
        RECEIVER.replace("0 0 1 360", "0 0 1 180")
    } else {
        RECEIVER.into()
    }
}

fn save(path: &Path, value: &Value) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn model(b: bool) -> Model {
    let mut model = if b {
        Model::alternative_b()
    } else {
        Model::default()
    };
    model.sensors.requested_spacing_m = 1.0;
    model
}

fn fixture(name: &str) -> Result<String> {
    if name == "uniform" {
        return Ok(String::new());
    }
    let mut validated = validate_model(&model(name.ends_with('B')))?;
    if name.starts_with("aperture") {
        validated
            .surfaces
            .retain(|surface| surface.material != SurfaceMaterial::Glass);
    }
    if name == "black" || name.starts_with("aperture") {
        for surface in &mut validated.surfaces {
            if surface.material != SurfaceMaterial::Glass {
                surface.reflectance = Some(0.0);
            }
        }
    }
    scene_text(&validated)
}

fn arguments(count: usize, direct: bool) -> Vec<String> {
    let (sampling, bounces) = if count == 16_384 { (64, 7) } else { (128, 8) };
    let mut args: Vec<String> = [
        "-n",
        "1",
        "-faa",
        "-y",
        "2",
        "-ad",
        &sampling.to_string(),
        "-as",
        "0",
        "-aa",
        "0",
        "-ar",
        "256",
        "-av",
        "0",
        "0",
        "0",
        "-lw",
        "1e-9",
        "-lr",
        "-16",
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
        "-V+",
        "-e",
        "MF:2",
        "-f",
        "reinhart.cal",
        "-b",
        "rbin",
        "-bn",
        "Nrbins",
        "-m",
        "sky_glow",
    ]
    .map(String::from)
    .into();
    args.extend([
        "-ab".into(),
        if direct { 0 } else { bounces }.to_string(),
        "-c".into(),
        count.to_string(),
        "fixture.oct".into(),
    ]);
    args
}

fn matrix(path: &Path) -> Result<Vec<f64>> {
    ensure!(
        fs::metadata(path)?.len() <= 2 * 1024 * 1024,
        "Bridge matrix size bound"
    );
    let text = fs::read_to_string(path)?.replace("\r\n", "\n");
    let (header, values) = text.split_once("\n\n").context("Missing matrix header")?;
    ensure!(header.starts_with("#?RADIANCE\n"), "Wrong matrix signature");
    for (key, expected) in [
        ("NROWS", "2"),
        ("NCOLS", "578"),
        ("NCOMP", "3"),
        ("FORMAT", "ascii"),
    ] {
        let fields: Vec<_> = header
            .lines()
            .filter_map(|line| line.strip_prefix(&format!("{key}=")))
            .collect();
        ensure!(fields == [expected], "Wrong or duplicate matrix {key}");
    }
    let values: Vec<f64> = values
        .split_whitespace()
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()?;
    ensure!(values.len() == 2 * BINS * 3, "Wrong bridge matrix count");
    ensure!(
        values.iter().all(|v| v.is_finite() && *v >= 0.0),
        "Invalid bridge coefficient"
    );
    Ok(values)
}

// Independent area integral: integrate L*omega_z*abs(omega_y)/r^2 over
// the clear aperture. The x integral is analytic; z uses convergent midpoint
// integration split at every projected-overhang clipping transition. No primary
// directions, ray intersections, Radiance values or generator are used here.
fn primitive(u: f64, c2: f64) -> f64 {
    u / (2.0 * c2 * (u * u + c2)) + (u / c2.sqrt()).atan() / (2.0 * c2.powf(1.5))
}
fn aperture_integral(sensor: Vec3, shaded: bool, divisions: usize) -> f64 {
    let [sx, sy, sz] = sensor;
    let (left, right, bottom, top) = (1.1, 2.9, 0.9_f64.max(sz), 2.4);
    let rise = top - sz;
    let mut breaks = vec![bottom, top];
    if shaded {
        breaks.push(sz + rise * sy / (sy + 0.6));
        for edge in [left, right] {
            if edge != sx {
                for window_edge in [left, right] {
                    breaks.push(sz + rise * (window_edge - sx) / (edge - sx));
                }
            }
        }
    }
    breaks.retain(|v| *v >= bottom && *v <= top);
    breaks.sort_by(f64::total_cmp);
    breaks.dedup();
    let strip = |lo: f64, hi: f64, dz: f64| {
        if hi <= lo || dz <= 0.0 {
            return 0.0;
        }
        let c2 = sy * sy + dz * dz;
        sy * dz * (primitive(hi - sx, c2) - primitive(lo - sx, c2))
    };
    let mut integral = 0.0;
    for interval in breaks.windows(2) {
        let step = (interval[1] - interval[0]) / divisions as f64;
        for i in 0..divisions {
            let dz = interval[0] + (i as f64 + 0.5) * step - sz;
            let mut value = strip(left, right, dz);
            if shaded && dz >= rise * sy / (sy + 0.6) && dz <= rise {
                let ratio = dz / rise;
                let a = sx + (left - sx) * ratio;
                let b = sx + (right - sx) * ratio;
                value -= strip(left.max(a), right.min(b), dz);
            }
            integral += value * step;
        }
    }
    integral * 10_000.0 / PI
}

#[test]
fn independent_bridge_aperture_integral_converges_before_engine_execution() {
    let validated = validate_model(&model(false)).unwrap();
    assert_eq!(validated.sensor_grid.sensors.len(), 24);
    for index in SENSOR_INDICES {
        let mut p = validated.sensor_grid.sensors[index].position;
        p[2] += ORIGIN_OFFSET_M;
        for shaded in [false, true] {
            let low = aperture_integral(p, shaded, 1024);
            let high = aperture_integral(p, shaded, 4096);
            assert!(high.is_finite() && high > 0.0);
            assert!((low - high).abs() / high <= 0.001);
            if shaded {
                assert!(high < aperture_integral(p, false, 4096));
            }
        }
    }
}

fn run_stream(
    engine: &Engine,
    root: &Path,
    name: &str,
    count: usize,
    direct: bool,
    sensors: &[Sensor],
) -> Result<(Vec<f64>, annual_stream::StreamOutcome)> {
    let directory = root.join(format!(
        "{name}-{count}-{}",
        if direct { "direct" } else { "full" }
    ));
    fs::create_dir(&directory)?;
    fs::copy(
        root.join(name).join("fixture.oct"),
        directory.join("fixture.oct"),
    )?;
    let producer = HammersleyPrimary::new(sensors, count)?;
    let request = annual_stream::StreamRequest {
        executable: engine.root().join("bin/rcontrib.exe"),
        arguments: arguments(count, direct),
        directory: directory.clone(),
        child_environment: BTreeMap::from([(
            "RAYPATH".into(),
            format!(".;{}", engine.root().join("lib").display()),
        )]),
        expected_records: producer.expected_records(),
        limits: annual_stream::StreamLimits {
            deadline: Duration::from_secs(1800),
            max_input_bytes: producer.expected_records() * MAX_RECORD_BYTES as u64,
            ..Default::default()
        },
    };
    let outcome = annual_stream::execute(&request, producer, &AtomicBool::new(false))?;
    ensure!(
        outcome.input.records == request.expected_records
            && outcome.telemetry.atomic_job_assignment,
        "Incomplete native bridge receipt"
    );
    let values = matrix(&directory.join("stream-stdout.bin"))?;
    Ok((values, outcome))
}

fn bridge(root: &Path) -> Result<()> {
    let engine = Engine::discover(None)
        .context("Required native Radiance unavailable; bridge cannot be skipped")?;
    let identity = AnnualEngineIdentity::discover(&engine)?;
    let validated = validate_model(&model(false))?;
    ensure!(
        validated.sensor_grid.sensors.len() == 24,
        "Wrong synthetic fixture sensor grid"
    );
    let sensors: Vec<_> = SENSOR_INDICES
        .iter()
        .map(|i| validated.sensor_grid.sensors[*i].clone())
        .collect();
    let mut declaration = json!({
        "schema_version":"greenlens.rust-primary-bridge.v1", "declared_utc":Utc::now().to_rfc3339(),
        "scope":"Isolated actual Rust producer/supervisor bridge; no production annual method or accuracy-profile acceptance",
        "generator":GENERATOR_VERSION,"encoding":ENCODING_VERSION,"encoding_policy":ENCODING_POLICY,"rule":RULE,
        "pinned_engine_files":{"version":identity.version,"executables":identity.executable_sha256,"libraries":identity.library_sha256},
        "sensors":sensors,"counts":[16384,65536],"sky_basis_mf":2,"receiver_scale":100,
        "normalization":"pi*coefficient mean/100 once; no second division by primary count",
        "profiles":[{"count":16384,"ad":64,"full_ab":7},{"count":65536,"ad":128,"full_ab":8}],
        "direct_ab":0,"depth":-16,"uniform_relative_tolerance":0.01,
        "aperture_lux":"uniform upper sky,10000lx exterior; black walls, unglazed opening, black lower hemisphere",
        "aperture_tolerance":{"absolute_lux":5,"relative":0.05,"oracle_convergence_relative":0.001},
        "coefficient_residual_policy":"Every full-direct RGB bin nonnegative; black-glass and uniform residuals exactly zero; no clamping",
        "measurement_scope":"Streaming telemetry samples whole-Job plus runner working sets and local execution directory file sizes, including scan/query time plus20ms sleep; no hard20ms cadence. Compilation uses historical child-lifetime peak and runner-plus-child samples. These scopes remain distinct.",
        "source_sha256":{"generator":sha256(include_bytes!("annual_primary.rs")),"bridge":sha256(include_bytes!("annual_primary_native_tests.rs")),"supervisor":sha256(include_bytes!("annual_stream.rs"))},
        "release_acceptance_complete":false
    });
    for (name, source) in [
        (
            "primary-source.rs",
            include_bytes!("annual_primary.rs").as_slice(),
        ),
        (
            "bridge-source.rs",
            include_bytes!("annual_primary_native_tests.rs").as_slice(),
        ),
        (
            "supervisor-source.rs",
            include_bytes!("annual_stream.rs").as_slice(),
        ),
    ] {
        fs::write(root.join(name), source)?;
    }
    let mut inputs = BTreeMap::new();
    let mut compilation = Vec::new();
    for name in ["uniform", "black", "A", "B", "apertureA", "apertureB"] {
        let directory = root.join(name);
        fs::create_dir(&directory)?;
        let scene = fixture(name)?;
        fs::write(directory.join("scene.rad"), &scene)?;
        let receiver = receiver(name);
        fs::write(directory.join("receiver.rad"), &receiver)?;
        inputs.insert(name, json!({"scene_sha256":sha256(scene.as_bytes()),"receiver_sha256":sha256(receiver.as_bytes())}));
    }
    declaration["inputs"] = serde_json::to_value(&inputs)?;
    declaration["arguments"] = json!([
        arguments(16384, false),
        arguments(16384, true),
        arguments(65536, false),
        arguments(65536, true)
    ]);
    save(&root.join("declaration.json"), &declaration)?;
    for name in ["uniform", "black", "A", "B", "apertureA", "apertureB"] {
        let directory = root.join(name);
        let (record, resources) = process::execute_annual(
            engine.root(),
            "oconv",
            &["-f".into(), "scene.rad".into(), "receiver.rad".into()],
            &directory,
            None,
            &directory.join("fixture.oct"),
            &AtomicBool::new(false),
        )?;
        compilation.push(json!({"fixture":name,"process":record,"resources":resources}));
        save(&root.join("compilation.json"), &json!(compilation))?;
    }
    let mut checks = Vec::new();
    for count in [16_384, 65_536] {
        for name in ["uniform", "black", "A", "B", "apertureA", "apertureB"] {
            eprintln!("Rust primary bridge {name} {count}");
            let (direct, direct_outcome) = run_stream(&engine, root, name, count, true, &sensors)?;
            let aperture = name.starts_with("aperture");
            let full = if aperture {
                None
            } else {
                Some(run_stream(&engine, root, name, count, false, &sensors)?)
            };
            let paired = full
                .as_ref()
                .is_none_or(|(_, result)| result.input == direct_outcome.input);
            let mut failures = Vec::new();
            let mut rows = Vec::new();
            for (sensor_index, sensor) in sensors.iter().enumerate() {
                let mut direct_sum = [0.0; 3];
                let mut full_sum = [0.0; 3];
                for patch in 0..BINS {
                    for channel in 0..3 {
                        let index = (sensor_index * BINS + patch) * 3 + channel;
                        direct_sum[channel] += PI * direct[index] / 100.0;
                        if let Some((matrix, _)) = &full {
                            full_sum[channel] += PI * matrix[index] / 100.0;
                            let residual = matrix[index] - direct[index];
                            if residual < 0.0
                                || ((name == "black" || name == "uniform") && residual != 0.0)
                            {
                                failures.push(json!({"sensor_id":sensor.id,"patch":patch,"channel":channel,"coefficient_full":matrix[index],"coefficient_direct":direct[index],"residual":residual}));
                            }
                        }
                    }
                }
                let mut row = json!({"sensor_id":sensor.id,"direct_rgb":direct_sum,"full_rgb":full.as_ref().map(|_|full_sum)});
                if name == "uniform" {
                    let passed = direct_sum
                        .into_iter()
                        .chain(full_sum)
                        .all(|value| (value - PI).abs() <= 0.01 * PI);
                    row["uniform_passed"] = json!(passed);
                    if !passed {
                        failures.push(json!({"sensor_id":sensor.id,"uniform_failed":true}));
                    }
                }
                if aperture {
                    let mut position = sensor.position;
                    position[2] += ORIGIN_OFFSET_M;
                    let low = aperture_integral(position, name == "apertureB", 1024);
                    let reference = aperture_integral(position, name == "apertureB", 4096);
                    let tolerance = 5.0_f64.max(0.05 * reference);
                    // Neutral uniform L=10000/pi is an analytical fixture, not
                    // Radiance's 179 lm/W conversion or a broadband conversion.
                    let lux = direct_sum.map(|v| v * 10_000.0 / PI);
                    let passed = (low - reference).abs() / reference <= 0.001
                        && lux
                            .iter()
                            .all(|value| (*value - reference).abs() <= tolerance);
                    row["aperture"] = json!({"reference_1024":low,"reference_4096":reference,"lux_rgb":lux,"tolerance_lux":tolerance,"passed":passed});
                    if !passed {
                        failures.push(json!({"sensor_id":sensor.id,"aperture_failed":true}));
                    }
                }
                rows.push(row);
            }
            let passed = paired && failures.is_empty();
            let direct_hash = sha256(&fs::read(
                root.join(format!("{name}-{count}-direct/stream-stdout.bin")),
            )?);
            let full_hash = if full.is_some() {
                Some(sha256(&fs::read(
                    root.join(format!("{name}-{count}-full/stream-stdout.bin")),
                )?))
            } else {
                None
            };
            checks.push(json!({"fixture":name,"count":count,"paired_receipts_equal":paired,"rows":rows,"failures":failures,"passed":passed,"direct_output_sha256":direct_hash,"full_output_sha256":full_hash,"direct":direct_outcome,"full":full.map(|(_,outcome)|outcome)}));
            save(
                &root.join("bridge.json"),
                &json!({"declaration":declaration,"checks":checks,"completed_cases":checks.len(),"expected_cases":12,"passed":checks.len()==12 && checks.iter().all(|check|check["passed"]==true),"release_acceptance_complete":false}),
            )?;
        }
    }
    ensure!(
        checks.iter().all(|check| check["passed"] == true),
        "Native bridge failed; all completed cases and raw outputs retained"
    );
    Ok(())
}

#[test]
#[ignore = "explicit isolated native bridge; coordinate one engine slot and invoke with --ignored --exact; no skip counts as acceptance"]
fn native_hammersley_primary_stream_bridge() {
    let root = PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("Native local cache"))
        .join("GreenLens/verification")
        .join(format!(
            "Rust primary bridge 空間 {}",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
    fs::create_dir_all(&root).unwrap();
    eprintln!("Retained Rust primary bridge: {}", root.display());
    let result = bridge(&root);
    if let Err(error) = &result {
        save(&root.join("failure.json"),&json!({"error":format!("{error:#}"),"passed":false,"release_acceptance_complete":false})).unwrap();
    }
    result.unwrap();
}
