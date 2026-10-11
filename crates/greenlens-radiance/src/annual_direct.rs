//! Deterministic finite solar-disc quadrature for the bounded one-aperture room.
//! Geometry only culls rays that cannot reach the aperture. Radiance evaluates
//! all emitted rays, including glazing, overhang and context transmission.
use anyhow::{ensure, Context, Result};
use greenlens_core::{Sensor, SurfaceMaterial, ValidatedModel};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{BufRead, BufReader, BufWriter, Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub const MAX_DIRECT_BATCH_RAYS: usize = 1_048_576;
pub const DIRECT_POLICY: &str = "Equal-solid-angle midpoint solar-disc quadrature: uniform cos(theta) rings and azimuth; shipping 32x32, reference 64x64. Normalize printed solar direction; horizon clip each ray at world z>0. Ray origin is sensor+0.0001m*normal, matching pinned immediate-irradiance evaluation. Cull each ray only if it misses the authoritative glass polygon, with inclusive 1e-9m edge allowance; Radiance traces glazing/shading/context at ab0. Reflected solar is patch-approximated. Solar patch transport is included only for emitted solar-source intervals. No negative corrected RGB is silently clamped.";
const EDGE_EPSILON_M: f64 = 1e-9;
type Vec3 = [f64; 3];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectBatch {
    pub interval_indices: Vec<usize>,
    pub attempted_rays: usize,
    pub emitted_rays: usize,
    pub culled_rays: usize,
    pub input_sha256: String,
    pub output_sha256: String,
}

#[derive(Debug, Clone)]
pub(crate) struct Sun {
    pub interval: usize,
    direction: Vec3,
    radiance: Vec3,
    radius: f64,
}

pub(crate) fn parse_suns(text: &str, expected: &[usize]) -> Result<Vec<Sun>> {
    ensure!(
        text.len() <= 4 * 1024 * 1024,
        "Oversized generated solar definitions"
    );
    let tokens: Vec<&str> = text.split_whitespace().collect();
    ensure!(
        tokens.len() == expected.len() * 19,
        "Unexpected generated solar definition count"
    );
    let mut suns = Vec::with_capacity(expected.len());
    for (tokens, &interval) in tokens.as_chunks::<19>().0.iter().zip(expected) {
        let minute = interval * 60 + 30;
        let modifier = format!("solar{minute}");
        let source = format!("sun{minute}");
        ensure!(
            tokens[0..3] == ["void", "light", modifier.as_str()]
                && tokens[3..6] == ["0", "0", "3"]
                && tokens[9..12] == [modifier.as_str(), "source", source.as_str()]
                && tokens[12..15] == ["0", "0", "4"],
            "Unexpected generated solar primitive"
        );
        let parse = |index: usize| -> Result<f64> {
            let value: f64 = tokens[index]
                .parse()
                .context("Invalid generated solar number")?;
            ensure!(value.is_finite(), "Nonfinite generated solar number");
            Ok(value)
        };
        let radiance = [parse(6)?, parse(7)?, parse(8)?];
        ensure!(
            radiance.iter().all(|v| *v >= 0.0),
            "Negative solar radiance"
        );
        let direction = [parse(15)?, parse(16)?, parse(17)?];
        let length = dot(direction, direction).sqrt();
        ensure!(
            (length - 1.0).abs() <= 2e-6,
            "Invalid printed solar direction"
        );
        let angle = parse(18)?;
        ensure!(
            (angle - 0.533).abs() <= 1e-12,
            "Unsupported solar disc angle"
        );
        suns.push(Sun {
            interval,
            direction: scale(direction, 1.0 / length),
            radiance,
            radius: angle.to_radians() / 2.0,
        });
    }
    Ok(suns)
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: Vec3, k: f64) -> Vec3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn plus(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) struct Aperture {
    vertices: [Vec3; 4],
    normal: Vec3,
}
impl Aperture {
    pub(crate) fn from_model(model: &ValidatedModel) -> Result<Self> {
        let panes: Vec<_> = model
            .surfaces
            .iter()
            .filter(|s| s.material == SurfaceMaterial::Glass)
            .collect();
        ensure!(
            panes.len() == 1 && panes[0].vertices.len() == 4,
            "Finite solar method requires exactly one validated rectangular glass pane"
        );
        Self::new(
            panes[0]
                .vertices
                .clone()
                .try_into()
                .expect("four validated vertices"),
        )
    }
    fn new(vertices: [Vec3; 4]) -> Result<Self> {
        let normal = cross(sub(vertices[1], vertices[0]), sub(vertices[2], vertices[0]));
        let length = dot(normal, normal).sqrt();
        ensure!(
            length.is_finite() && length > 0.0,
            "Invalid aperture geometry"
        );
        Ok(Self {
            vertices,
            normal: scale(normal, 1.0 / length),
        })
    }
    fn hits(&self, origin: Vec3, direction: Vec3) -> bool {
        let denominator = dot(self.normal, direction);
        if denominator == 0.0 {
            return false;
        }
        let distance = dot(self.normal, sub(self.vertices[0], origin)) / denominator;
        if !distance.is_finite() || distance <= 0.0 {
            return false;
        }
        let hit = plus(origin, scale(direction, distance));
        (0..4).all(|i| {
            let edge = sub(self.vertices[(i + 1) % 4], self.vertices[i]);
            dot(cross(edge, sub(hit, self.vertices[i])), self.normal)
                >= -EDGE_EPSILON_M * dot(edge, edge).sqrt()
        })
    }
}

fn directions(sun: &Sun, samples: usize) -> Result<Vec<Vec3>> {
    ensure!(
        samples == 1024 || samples == 4096,
        "Unsupported finite solar sample count"
    );
    let side = if samples == 1024 { 32 } else { 64 };
    let mut u = [sun.direction[1], -sun.direction[0], 0.0];
    let length = dot(u, u).sqrt();
    u = if length < 1e-9 {
        [0.0, 1.0, 0.0]
    } else {
        scale(u, 1.0 / length)
    };
    let v = cross(sun.direction, u);
    let mut directions = Vec::with_capacity(samples);
    for ring in 0..side {
        let mu = 1.0 - (1.0 - sun.radius.cos()) * (ring as f64 + 0.5) / side as f64;
        let radial = (1.0 - mu * mu).sqrt();
        for azimuth in 0..side {
            let phi = std::f64::consts::TAU * (azimuth as f64 + 0.5) / side as f64;
            directions.push(plus(
                scale(sun.direction, mu),
                scale(plus(scale(u, phi.cos()), scale(v, phi.sin())), radial),
            ));
        }
    }
    Ok(directions)
}
fn origin(sensor: &Sensor) -> Vec3 {
    plus(sensor.position, scale(sensor.direction, 0.0001))
}
fn eligible(aperture: &Aperture, sensor: &Sensor, sun: &Sun, direction: Vec3) -> bool {
    direction[2] > 0.0
        && dot(sensor.direction, direction) > 0.0
        && sun.radiance.iter().any(|v| *v > 0.0)
        && aperture.hits(origin(sensor), direction)
}
fn cancelled(cancel: &AtomicBool) -> Result<()> {
    ensure!(
        !cancel.load(Ordering::SeqCst),
        "Climate simulation cancelled"
    );
    Ok(())
}
pub(crate) fn file_hash(path: &Path) -> Result<String> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let size = reader.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub(crate) fn write_batch(
    path: &Path,
    suns: &[Sun],
    sensors: &[Sensor],
    aperture: &Aperture,
    samples: usize,
    cancel: &AtomicBool,
) -> Result<DirectBatch> {
    let attempted = suns
        .len()
        .checked_mul(sensors.len())
        .and_then(|n| n.checked_mul(samples))
        .context("Direct ray count overflow")?;
    ensure!(
        attempted <= MAX_DIRECT_BATCH_RAYS,
        "Direct ray batch exceeds resource bound"
    );
    let mut writer = BufWriter::new(File::create(path)?);
    let mut emitted = 0;
    for sun in suns {
        let directions = directions(sun, samples)?;
        for sensor in sensors {
            cancelled(cancel)?;
            let origin = origin(sensor);
            for &direction in &directions {
                if eligible(aperture, sensor, sun, direction) {
                    writeln!(
                        writer,
                        "{} {} {} {} {} {}",
                        origin[0], origin[1], origin[2], direction[0], direction[1], direction[2]
                    )?;
                    emitted += 1;
                }
            }
        }
    }
    writer.flush()?;
    writer.get_ref().sync_all()?;
    Ok(DirectBatch {
        interval_indices: suns.iter().map(|s| s.interval).collect(),
        attempted_rays: attempted,
        emitted_rays: emitted,
        culled_rays: attempted - emitted,
        input_sha256: file_hash(path)?,
        output_sha256: String::new(),
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn integrate_batch(
    path: &Path,
    suns: &[Sun],
    sensors: &[Sensor],
    aperture: &Aperture,
    samples: usize,
    expected_rays: usize,
    cancel: &AtomicBool,
) -> Result<Vec<[f64; 3]>> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut line = String::new();
    let mut consumed = 0;
    let mut output = Vec::with_capacity(suns.len() * sensors.len());
    for sun in suns {
        let directions = directions(sun, samples)?;
        let weight = std::f64::consts::TAU * (1.0 - sun.radius.cos()) / samples as f64;
        for sensor in sensors {
            cancelled(cancel)?;
            let mut rgb = [0.0; 3];
            for &direction in &directions {
                if !eligible(aperture, sensor, sun, direction) {
                    continue;
                }
                line.clear();
                let size = (&mut reader).take(1025).read_line(&mut line)?;
                ensure!(
                    size > 0 && size <= 1024,
                    "Truncated or oversized direct transmission record"
                );
                let mut values = line.split_whitespace();
                for (channel, value) in rgb.iter_mut().enumerate() {
                    let transmission: f64 =
                        values.next().context("Truncated direct RGB")?.parse()?;
                    ensure!(
                        transmission.is_finite() && (0.0..=1.000001).contains(&transmission),
                        "Invalid bounded direct transmission"
                    );
                    *value += transmission
                        * sun.radiance[channel]
                        * weight
                        * dot(sensor.direction, direction);
                }
                ensure!(
                    values.next().is_none(),
                    "Excess direct transmission components"
                );
                consumed += 1;
            }
            ensure!(
                rgb.iter().all(|v| v.is_finite() && *v >= 0.0),
                "Invalid finite solar integral"
            );
            output.push(rgb);
        }
    }
    ensure!(
        consumed == expected_rays,
        "Direct transmission count mismatch"
    );
    line.clear();
    ensure!(
        reader.read_line(&mut line)? == 0,
        "Excess direct transmission records"
    );
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn native_aperture_culling_matches_all_rays_at_eleven_shadow_points() {
        let engine =
            crate::Engine::discover(None).expect("Required native Radiance is unavailable");
        let scratch =
            std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("Native local cache"))
                .join("GreenLens/verification")
                .join(format!(
                    "finite-disc cull regression 空間 {}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap()
                ));
        std::fs::create_dir_all(&scratch).unwrap();
        let mut scene = String::from("void plastic black\n0\n0\n5 0 0 0 0 0\n");
        let polygons = [
            [
                [-100., -100., 1.],
                [-0.1, -100., 1.],
                [-0.1, 100., 1.],
                [-100., 100., 1.],
            ],
            [
                [0.1, -100., 1.],
                [100., -100., 1.],
                [100., 100., 1.],
                [0.1, 100., 1.],
            ],
            [
                [-0.1, -100., 1.],
                [0.1, -100., 1.],
                [0.1, -0.1, 1.],
                [-0.1, -0.1, 1.],
            ],
            [
                [-0.1, 0.1, 1.],
                [0.1, 0.1, 1.],
                [0.1, 100., 1.],
                [-0.1, 100., 1.],
            ],
        ];
        for (i, vertices) in polygons.iter().enumerate() {
            scene.push_str(&format!("black polygon border{i}\n0\n0\n12\n"));
            for v in vertices {
                scene.push_str(&format!("{} {} {}\n", v[0], v[1], v[2]));
            }
        }
        scene.push_str(super::super::RECEIVER_TEXT);
        std::fs::write(scratch.join("fixture.rad"), scene).unwrap();
        let cancel = AtomicBool::new(false);
        crate::process::execute(
            engine.root(),
            "oconv",
            &["-f".into(), "fixture.rad".into()],
            &scratch,
            None,
            &scratch.join("annual-diffuse.oct"),
            &cancel,
        )
        .unwrap();
        let sun = Sun {
            interval: 0,
            direction: [0., 0., 1.],
            radiance: [1e6; 3],
            radius: 0.533_f64.to_radians() / 2.,
        };
        let mut locations = vec![0., 0.09, 0.1, 0.11, 0.2];
        for fraction in [-0.75, -0.5, -0.25, 0.25, 0.5, 0.75] {
            locations.push(0.1 + (1. - 0.0001) * sun.radius.tan() * fraction);
        }
        let sensors: Vec<_> = locations
            .iter()
            .enumerate()
            .map(|(i, &x)| Sensor {
                id: format!("fixture-{i}"),
                position: [x, 0., 0.],
                direction: [0., 0., 1.],
            })
            .collect();
        let precise = Aperture::new([
            [-0.1, -0.1, 1.],
            [0.1, -0.1, 1.],
            [0.1, 0.1, 1.],
            [-0.1, 0.1, 1.],
        ])
        .unwrap();
        let all = Aperture::new([
            [-100., -100., 1.],
            [100., -100., 1.],
            [100., 100., 1.],
            [-100., 100., 1.],
        ])
        .unwrap();
        let mut integrals = Vec::new();
        for (label, aperture) in [("all", all), ("culled", precise)] {
            let input = scratch.join(format!("{label}.pts"));
            let output = scratch.join(format!("{label}.rgb"));
            let batch = write_batch(
                &input,
                std::slice::from_ref(&sun),
                &sensors,
                &aperture,
                1024,
                &cancel,
            )
            .unwrap();
            if label == "all" {
                assert_eq!(batch.emitted_rays, 11 * 1024);
            } else {
                assert!(batch.emitted_rays < batch.attempted_rays);
            }
            crate::process::execute(
                engine.root(),
                "rtrace",
                &super::super::AnnualProfile::shipping().transmission_arguments(),
                &scratch,
                Some(&input),
                &output,
                &cancel,
            )
            .unwrap();
            integrals.push(
                integrate_batch(
                    &output,
                    std::slice::from_ref(&sun),
                    &sensors,
                    &aperture,
                    1024,
                    batch.emitted_rays,
                    &cancel,
                )
                .unwrap(),
            );
        }
        for (a, b) in integrals[0].iter().zip(&integrals[1]) {
            for channel in 0..3 {
                assert!(
                    (a[channel] - b[channel]).abs() < 1e-12,
                    "Conservative culling changed direct irradiance"
                );
            }
        }
    }
    #[test]
    fn sun_parser_binds_source_names_order_and_geometry() {
        let text = "void light solar30 0 0 3 1 1 1 solar30 source sun30 0 0 4 0 0 1 .533";
        assert_eq!(parse_suns(text, &[0]).unwrap().len(), 1);
        for bad in [
            text.replace("sun30", "sun90"),
            text.replace(".533", "1"),
            text.replace("0 0 1 .533", "0 0 2 .533"),
            text.replace("3 1 1 1", "3 NaN 1 1"),
        ] {
            assert!(parse_suns(&bad, &[0]).is_err());
        }
        assert!(parse_suns(text, &[1]).is_err());
    }
    #[test]
    fn uniform_disc_quadrature_matches_independent_projected_solid_angle() {
        let sun = Sun {
            interval: 0,
            direction: [0., 0., 1.],
            radiance: [1.; 3],
            radius: 0.533_f64.to_radians() / 2.,
        };
        for n in [1024, 4096] {
            let dirs = directions(&sun, n).unwrap();
            assert!(dirs.iter().all(|&d| (dot(d, d) - 1.).abs() < 1e-12));
            let integral = dirs.iter().map(|d| d[2]).sum::<f64>()
                * std::f64::consts::TAU
                * (1. - sun.radius.cos())
                / n as f64;
            let expected = std::f64::consts::PI * sun.radius.sin().powi(2);
            assert!((integral - expected).abs() / expected < 1e-10);
        }
    }
    #[test]
    fn aperture_culling_is_edge_inclusive_and_rotation_invariant() {
        let vertices = [[-1., 0., 0.], [1., 0., 0.], [1., 0., 2.], [-1., 0., 2.]];
        let aperture = Aperture::new(vertices).unwrap();
        assert!(aperture.hits([0., 1., 1.], [1., -1., 0.]));
        assert!(aperture.hits([0., 1., 1.], [1. + 5e-10, -1., 0.]));
        assert!(!aperture.hits([0., 1., 1.], [1. + 2e-9, -1., 0.]));
        assert!(!aperture.hits([0., 1., 1.], [0., 1., 0.]));
        let rotate = |p: Vec3| [-p[1], p[0], p[2]];
        let rotated = Aperture::new(vertices.map(rotate)).unwrap();
        for d in [[1., -1., 0.], [0., -1., 0.4], [2., -1., 0.], [0., 1., 0.]] {
            assert_eq!(
                aperture.hits([0., 1., 1.], d),
                rotated.hits(rotate([0., 1., 1.]), rotate(d))
            );
        }
    }
}
