//! Independent analytical fixtures and native adapter integration evidence.
use super::*;
use greenlens_core::{validate_model, Model, SurfaceMaterial};
use serde_json::{json, Value};
use std::{
    f64::consts::PI,
    fs::File,
    process::{Command, Stdio},
};

fn point_text(points: &[[f64; 3]]) -> String {
    points
        .iter()
        .map(|p| format!("{} {} {} 0 0 1\n", p[0], p[1], p[2]))
        .collect()
}

/// Midpoint area quadrature, derived from projected solid angle. Independent
/// of Radiance scene traversal, material code and sampling implementation.
pub fn aperture_quadrature(sensor: [f64; 3], divisions: usize, exterior_lux: f64) -> f64 {
    let dx = 1.8 / divisions as f64;
    let dz = 1.5 / divisions as f64;
    let zenith = 9.0 * exterior_lux / (7.0 * PI);
    let mut integral = 0.0;
    for x in 0..divisions {
        for z in 0..divisions {
            let vector = [
                1.1 + (x as f64 + 0.5) * dx - sensor[0],
                -sensor[1],
                0.9 + (z as f64 + 0.5) * dz - sensor[2],
            ];
            let r2 = vector.iter().map(|v| v * v).sum::<f64>();
            let distance = r2.sqrt();
            let omega_z = vector[2] / distance;
            if omega_z > 0.0 {
                integral +=
                    zenith * (1.0 + 2.0 * omega_z) / 3.0 * omega_z * (vector[1] / distance).abs()
                        / r2
                        * dx
                        * dz;
            }
        }
    }
    integral
}

fn black_fixture(sealed: bool) -> Result<String> {
    let mut model = validate_model(&Model::default())?;
    if !sealed {
        model
            .surfaces
            .retain(|s| s.material != SurfaceMaterial::Glass);
    }
    for surface in &mut model.surfaces {
        surface.material = SurfaceMaterial::Wall;
        surface.reflectance = Some(0.0);
    }
    scene_text(&model)
}

fn analytical_profile() -> Profile {
    Profile {
        name: "analytical-fixture-v1".into(),
        ambient_bounces: 1,
        ambient_divisions: 131072,
        ambient_supersamples: 0,
        ambient_accuracy: 0.0,
        ambient_resolution: 256,
        limit_weight: 0.000001,
        limit_reflections: 12,
    }
}

fn direct_replay(engine: &Engine, dir: &Path, profile: &Profile) -> Result<Vec<f64>> {
    for (name, args, input, out) in [
        (
            "oconv",
            vec!["-f".into(), "sky.rad".into(), "scene.rad".into()],
            None,
            "direct.oct",
        ),
        (
            "rtrace",
            {
                let mut args = profile.arguments();
                *args.last_mut().unwrap() = "direct.oct".into();
                args
            },
            Some("sensors.pts"),
            "direct.rgb",
        ),
    ] {
        let mut command = Command::new(engine.root.join(format!("bin/{name}.exe")));
        command
            .args(args)
            .current_dir(dir)
            .env(
                "RAYPATH",
                format!(".;{}", engine.root.join("lib").display()),
            )
            .stdin(match input {
                Some(file) => Stdio::from(File::open(dir.join(file))?),
                None => Stdio::null(),
            })
            .stdout(File::create(dir.join(out))?)
            .stderr(File::create(dir.join(format!("direct-{name}.stderr")))?);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let mut child = command.spawn()?;
        #[cfg(windows)]
        let _job = match process::WindowsJob::attach(&child) {
            Ok(job) => job,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let status = child.wait()?;
        if !status.success() {
            bail!("Independent direct {name} invocation exited {status}");
        }
    }
    let sensors = fs::read_to_string(dir.join("sensors.pts"))?.lines().count();
    parse_rgb(&fs::read_to_string(dir.join("direct.rgb"))?, sensors)?
        .iter()
        .map(|v| greenlens_core::rgb_to_lux(*v).map_err(Into::into))
        .collect()
}

pub fn run_suite(output: &Path, fixtures_only: bool) -> Result<Value> {
    let engine = Engine::discover(None).context(
        "Required real-engine suite cannot run; engine absence is a failure, not a skip",
    )?;
    fs::create_dir_all(output)?;
    let cancel = AtomicBool::new(false);
    let profile = analytical_profile();
    let mut checks = Vec::<Value>::new();
    let uniform_dir = output.join("B uniform hemisphere");
    fs::create_dir_all(&uniform_dir)?;
    fs::write(uniform_dir.join("scene.rad"), "# no opaque geometry\n")?;
    fs::write(uniform_dir.join("sensors.pts"), "0 0 0 0 0 1\n")?;
    fs::write(
        uniform_dir.join("sky.rad"),
        "void glow uniform\n0\n0\n4 1 1 1 0\nuniform source hemisphere\n0\n0\n4 0 0 1 180\n",
    )?;
    let uniform = trace_prepared(
        &engine,
        1,
        &profile,
        &uniform_dir,
        &cancel,
        |m| eprintln!("B: {m}"),
        &mut vec![],
    )?;
    let expected = 179.0 * PI;
    let relative = (uniform.lux[0] - expected).abs() / expected;
    checks.push(json!({"id":"B","name":"Uniform hemisphere E=pi*L","expected_lux":expected,"actual_lux":uniform.lux[0],"relative_error":relative,"tolerance_relative":0.01,"passed":relative<=0.01}));
    let exterior = run_scene(
        &engine,
        "# unobstructed\n",
        "0 0 0.8 0 0 1\n",
        1,
        10000.0,
        0.2,
        &profile,
        &output.join("C overcast"),
        &cancel,
        |m| eprintln!("C: {m}"),
    )?;
    let relative = (exterior.lux[0] - 10000.0).abs() / 10000.0;
    checks.push(json!({"id":"C","name":"Prescribed overcast exterior normalization","expected_lux":10000.0,"actual_lux":exterior.lux[0],"relative_error":relative,"tolerance_relative":0.01,"passed":relative<=0.01}));
    let points = [
        [2.0, 0.5, 0.8],
        [2.0, 1.5, 0.8],
        [2.0, 3.0, 0.8],
        [1.0, 3.0, 0.8],
        [3.0, 3.0, 0.8],
    ];
    let sensors = point_text(&points);
    let black = run_scene(
        &engine,
        &black_fixture(true)?,
        &sensors,
        points.len(),
        10000.0,
        0.0,
        &profile,
        &output.join("D sealed black"),
        &cancel,
        |m| eprintln!("D: {m}"),
    )?;
    checks.push(json!({"id":"D","name":"Sealed black enclosure","actual_lux":black.lux,"tolerance_lux":0.1,"passed":black.lux.iter().all(|v|*v<0.1)}));
    let opening = run_scene(
        &engine,
        &black_fixture(false)?,
        &sensors,
        points.len(),
        10000.0,
        0.0,
        &profile,
        &output.join("E aperture"),
        &cancel,
        |m| eprintln!("E: {m}"),
    )?;
    let mut rows = Vec::new();
    let mut pass = true;
    for (i, point) in points.iter().enumerate() {
        let q128 = aperture_quadrature(*point, 128, 10000.0);
        let q256 = aperture_quadrature(*point, 256, 10000.0);
        let q512 = aperture_quadrature(*point, 512, 10000.0);
        let convergence = (q512 - q256).abs() / q512;
        let previous_convergence = (q256 - q128).abs() / q256;
        let delta = (opening.lux[i] - q512).abs();
        let tolerance = 0.5_f64.max(0.03 * q512);
        let passed = convergence <= 0.001 && delta <= tolerance;
        pass &= passed;
        rows.push(json!({"sensor":point,"quadrature_128_lux":q128,"quadrature_256_lux":q256,"quadrature_512_lux":q512,"quadrature_relative_change_128_to_256":previous_convergence,"quadrature_relative_change":convergence,"radiance_lux":opening.lux[i],"absolute_error_lux":delta,"tolerance_lux":tolerance,"passed":passed}));
    }
    checks.push(json!({"id":"E","name":"Independent black room aperture integration","rows":rows,"quadrature_tolerance_relative":0.001,"passed":pass}));
    let scaled = run_scene(
        &engine,
        &black_fixture(false)?,
        &sensors,
        points.len(),
        20000.0,
        0.0,
        &profile,
        &output.join("scaling"),
        &cancel,
        |m| eprintln!("Scaling: {m}"),
    )?;
    let ratios: Vec<f64> = opening
        .lux
        .iter()
        .zip(&scaled.lux)
        .map(|(a, b)| b / a)
        .collect();
    checks.push(json!({"id":"scaling","name":"Sky intensity scaling x2","ratios":ratios,"relative_tolerance":0.002,"passed":ratios.iter().all(|r|(r/2.0-1.0).abs()<=0.002)}));
    let symmetry =
        (opening.lux[3] - opening.lux[4]).abs() / ((opening.lux[3] + opening.lux[4]) / 2.0);
    checks.push(json!({"id":"symmetry","name":"Black aperture mirror sensors","relative_difference":symmetry,"tolerance_relative":0.03,"passed":symmetry<=0.03}));
    let bad_dir = output.join("nonzero exit");
    let failed = run_scene(
        &engine,
        "not valid radiance syntax\n",
        "0 0 0.8 0 0 1\n",
        1,
        10000.0,
        0.0,
        &profile,
        &bad_dir,
        &cancel,
        |_| {},
    );
    let failed_log: Value = serde_json::from_slice(&fs::read(bad_dir.join("oconv.process.json"))?)?;
    checks.push(json!({"id":"nonzero_exit","name":"Malformed scene fails and retains native exit record","exit_code":failed_log["exit_code"],"passed":failed.is_err() && failed_log["exit_code"]!=0 && !bad_dir.join("recipe.json").exists()}));
    let cancel_dir = output.join("actual engine cancellation");
    fs::create_dir_all(&cancel_dir)?;
    let started_file = cancel_dir.join("rtrace.started.json");
    if started_file.exists() {
        fs::remove_file(&started_file)?;
    }
    let cancel_request = AtomicBool::new(false);
    let cancelled = std::thread::scope(|scope| {
        scope.spawn(|| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !started_file.exists() && std::time::Instant::now() < until {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
            cancel_request.store(true, Ordering::SeqCst);
        });
        simulate(
            &engine,
            &validate_model(&Model::default())?,
            &Profile::reference(),
            &cancel_dir,
            &cancel_request,
            |_| {},
        )
    });
    let cancel_message = cancelled.err().map(|e| e.to_string()).unwrap_or_default();
    checks.push(json!({"id":"cancellation","name":"Actual rtrace cancelled without finalized evidence","diagnostic":cancel_message,"passed":cancel_message.contains("cancelled during rtrace") && !cancel_dir.join("recipe.json").exists()}));
    if !fixtures_only {
        let model = validate_model(&Model::default())?;
        let shipping_dir = output.join("A F shipping");
        let shipping = simulate(
            &engine,
            &model,
            &Profile::shipping(),
            &shipping_dir,
            &cancel,
            |m| eprintln!("A/F shipping: {m}"),
        )?;
        let direct = direct_replay(&engine, &shipping_dir, &Profile::shipping())?;
        let integration_deltas: Vec<f64> = shipping
            .lux
            .iter()
            .zip(&direct)
            .map(|(a, b)| (a - b).abs())
            .collect();
        let integration_pass = integration_deltas
            .iter()
            .zip(&direct)
            .all(|(delta, b)| *delta <= 0.1_f64.max(0.001 * b));
        checks.push(json!({"id":"A","name":"Adapter vs direct pinned-engine invocation (integration verification)","sensor_count":direct.len(),"absolute_deltas_lux":integration_deltas,"passed":integration_pass}));
        let reference = simulate(
            &engine,
            &model,
            &Profile::reference(),
            &output.join("F reference"),
            &cancel,
            |m| eprintln!("F reference: {m}"),
        )?;
        let deltas: Vec<f64> = shipping
            .lux
            .iter()
            .zip(&reference.lux)
            .map(|(a, b)| (a - b).abs())
            .collect();
        let mean_error = deltas.iter().sum::<f64>() / reference.lux.iter().sum::<f64>();
        let per_sensor_pass = deltas
            .iter()
            .zip(&reference.lux)
            .all(|(delta, b)| *delta <= 2.0_f64.max(0.05 * b));
        checks.push(json!({"id":"F","name":"Shipping vs higher-quality profile convergence","shipping_profile":Profile::shipping(),"reference_profile":Profile::reference(),"sensor_count":deltas.len(),"shipping_lux":shipping.lux,"reference_lux":reference.lux,"absolute_deltas_lux":deltas,"mean_absolute_difference_divided_by_reference_mean":mean_error,"mean_relative_tolerance":0.02,"passed":mean_error<=0.02&&per_sensor_pass}));
    }
    let passed = checks.iter().all(|c| c["passed"] == true);
    let report = json!({"schema_version":"greenlens.science.v0.1","verified_utc":chrono::Utc::now().to_rfc3339(),"native_platform":std::env::consts::OS,"engine":engine.identity(),"fixture_profile":profile,"fixtures_only":fixtures_only,"all_executed_checks_passed":passed,"release_acceptance_complete":false,"limitations":"Analytical fixtures establish limited behavior; integration verification is not independent physical validation. Export replay and desktop acceptance are separate gates.","checks":checks});
    fs::write(
        output.join("science-report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "Scientific report: {}",
        output.join("science-report.json").display()
    );
    for check in report["checks"].as_array().unwrap() {
        println!(
            "{}: {}",
            check["id"],
            if check["passed"] == true {
                "PASS"
            } else {
                "FAIL"
            }
        );
    }
    if !passed {
        bail!("Scientific acceptance failed; see science-report.json. Tolerances have not been changed.");
    }
    Ok(report)
}
