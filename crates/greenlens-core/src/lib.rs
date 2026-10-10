//! Versioned, engine-independent scientific contracts for GreenLens v0.1.
//! All coordinates use metres, +X east, +Y north and +Z up.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const MODEL_SCHEMA_VERSION: &str = "greenlens.model.v0.1";
pub const CALCULATION_LABEL: &str =
    "Static daylight illuminance under a prescribed CIE standard overcast sky.";
pub const MAX_SENSORS: usize = 10_000;
pub type Vec3 = [f64; 3];

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("{field}: {message}")]
    Validation { field: String, message: String },
    #[error("incompatible comparison: {0}")]
    IncompatibleComparison(String),
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, CoreError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub schema_version: String,
    pub name: String,
    pub room: Room,
    pub window: Window,
    pub materials: Materials,
    pub overhang: Option<Overhang>,
    pub obstruction: Option<Obstruction>,
    pub sensors: SensorSettings,
    pub sky: Sky,
    #[serde(default)]
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Room {
    pub width_m: f64,
    pub depth_m: f64,
    pub height_m: f64,
    /// Clockwise rotation from the unrotated local axes, about local origin.
    pub rotation_deg: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub width_m: f64,
    pub height_m: f64,
    pub sill_m: f64,
    pub center_x_m: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Materials {
    pub ceiling_reflectance: f64,
    pub wall_reflectance: f64,
    pub floor_reflectance: f64,
    pub visible_transmittance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Overhang {
    pub projection_m: f64,
    pub side_extension_m: f64,
    pub gap_above_window_m: f64,
    pub reflectance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Obstruction {
    pub center_x_m: f64,
    pub width_m: f64,
    pub depth_m: f64,
    pub height_m: f64,
    /// Clear perpendicular distance from the south facade to the nearest face.
    pub distance_m: f64,
    pub reflectance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SensorSettings {
    pub workplane_height_m: f64,
    pub requested_spacing_m: f64,
    pub wall_inset_m: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Sky {
    pub exterior_horizontal_illuminance_lux: f64,
    pub ground_reflectance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub location: Option<String>,
    /// Display-only IANA timezone identifier; never used to generate this sky.
    pub iana_timezone: Option<String>,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            schema_version: MODEL_SCHEMA_VERSION.into(),
            name: "Synthetic room — Alternative A".into(),
            room: Room {
                width_m: 4.0,
                depth_m: 6.0,
                height_m: 3.0,
                rotation_deg: 0.0,
            },
            window: Window {
                width_m: 1.8,
                height_m: 1.5,
                sill_m: 0.9,
                center_x_m: 2.0,
            },
            materials: Materials {
                ceiling_reflectance: 0.8,
                wall_reflectance: 0.5,
                floor_reflectance: 0.2,
                visible_transmittance: 0.6,
            },
            overhang: None,
            obstruction: None,
            sensors: SensorSettings {
                workplane_height_m: 0.8,
                requested_spacing_m: 0.5,
                wall_inset_m: 0.25,
            },
            sky: Sky {
                exterior_horizontal_illuminance_lux: 10_000.0,
                ground_reflectance: 0.2,
            },
            metadata: None,
        }
    }
}

impl Model {
    pub fn alternative_b() -> Self {
        Self {
            name: "Synthetic room — Alternative B".into(),
            overhang: Some(Overhang {
                projection_m: 0.6,
                side_extension_m: 0.0,
                gap_above_window_m: 0.0,
                reflectance: 0.5,
            }),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceMaterial {
    Floor,
    Wall,
    Ceiling,
    Glass,
    Overhang,
    Obstruction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Surface {
    pub id: String,
    pub material: SurfaceMaterial,
    pub vertices: Vec<Vec3>,
    pub reflectance: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Sensor {
    pub id: String,
    pub position: Vec3,
    pub direction: Vec3,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SensorGrid {
    pub sensors: Vec<Sensor>,
    pub columns: usize,
    pub rows: usize,
    pub actual_spacing_x_m: f64,
    pub actual_spacing_y_m: f64,
    pub cell_area_m2: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatedModel {
    pub model: Model,
    pub surfaces: Vec<Surface>,
    pub sensor_grid: SensorGrid,
    pub normalized_input_hash: String,
    pub scientific_input_hash: String,
    pub sensor_hash: String,
    pub comparison_conditions_hash: String,
}

fn invalid(field: &str, message: impl Into<String>) -> CoreError {
    CoreError::Validation {
        field: field.into(),
        message: message.into(),
    }
}

fn bound(field: &str, value: f64, min: f64, max: f64) -> Result<()> {
    if !value.is_finite() || value < min || value > max {
        return Err(invalid(
            field,
            format!("must be finite and between {min} and {max}; received {value}"),
        ));
    }
    Ok(())
}

fn validate_inputs(m: &Model) -> Result<()> {
    if m.schema_version != MODEL_SCHEMA_VERSION {
        return Err(invalid(
            "schema_version",
            format!("expected {MODEL_SCHEMA_VERSION}"),
        ));
    }
    if m.name.trim().is_empty()
        || m.name.chars().count() > 120
        || m.name.chars().any(char::is_control)
    {
        return Err(invalid("name", "must contain 1–120 printable characters"));
    }
    for (field, value, min, max) in [
        ("room.width_m", m.room.width_m, 0.5, 100.0),
        ("room.depth_m", m.room.depth_m, 0.5, 100.0),
        ("room.height_m", m.room.height_m, 0.5, 20.0),
        ("room.rotation_deg", m.room.rotation_deg, 0.0, 359.999999),
        ("window.width_m", m.window.width_m, 0.05, 99.98),
        ("window.height_m", m.window.height_m, 0.05, 19.98),
        ("window.sill_m", m.window.sill_m, 0.01, 19.94),
        ("window.center_x_m", m.window.center_x_m, 0.0, 100.0),
        (
            "materials.ceiling_reflectance",
            m.materials.ceiling_reflectance,
            0.0,
            0.95,
        ),
        (
            "materials.wall_reflectance",
            m.materials.wall_reflectance,
            0.0,
            0.95,
        ),
        (
            "materials.floor_reflectance",
            m.materials.floor_reflectance,
            0.0,
            0.95,
        ),
        (
            "materials.visible_transmittance",
            m.materials.visible_transmittance,
            0.01,
            0.9,
        ),
        (
            "sensors.workplane_height_m",
            m.sensors.workplane_height_m,
            0.05,
            19.95,
        ),
        (
            "sensors.requested_spacing_m",
            m.sensors.requested_spacing_m,
            0.05,
            10.0,
        ),
        ("sensors.wall_inset_m", m.sensors.wall_inset_m, 0.01, 49.99),
        (
            "sky.exterior_horizontal_illuminance_lux",
            m.sky.exterior_horizontal_illuminance_lux,
            1.0,
            200_000.0,
        ),
        (
            "sky.ground_reflectance",
            m.sky.ground_reflectance,
            0.0,
            0.95,
        ),
    ] {
        bound(field, value, min, max)?;
    }
    let left = m.window.center_x_m - m.window.width_m / 2.0;
    let right = m.window.center_x_m + m.window.width_m / 2.0;
    if left < 0.01 - 1e-10
        || right > m.room.width_m - 0.01 + 1e-10
        || m.window.sill_m + m.window.height_m > m.room.height_m - 0.01 + 1e-10
    {
        return Err(invalid("window", "opening must fit inside the south facade with at least 0.01 m of opaque wall on every edge"));
    }
    if m.sensors.workplane_height_m > m.room.height_m - 0.05 {
        return Err(invalid(
            "sensors.workplane_height_m",
            "must remain at least 0.05 m below the ceiling",
        ));
    }
    if 2.0 * m.sensors.wall_inset_m >= m.room.width_m.min(m.room.depth_m) {
        return Err(invalid(
            "sensors.wall_inset_m",
            "twice the inset must be smaller than both room width and depth",
        ));
    }
    if let Some(o) = &m.overhang {
        for (field, value, min, max) in [
            ("overhang.projection_m", o.projection_m, 0.01, 10.0),
            ("overhang.side_extension_m", o.side_extension_m, 0.0, 10.0),
            (
                "overhang.gap_above_window_m",
                o.gap_above_window_m,
                0.0,
                10.0,
            ),
            ("overhang.reflectance", o.reflectance, 0.0, 0.95),
        ] {
            bound(field, value, min, max)?;
        }
        if m.window.sill_m + m.window.height_m + o.gap_above_window_m > m.room.height_m {
            return Err(invalid(
                "overhang.gap_above_window_m",
                "overhang must not be above the room roof",
            ));
        }
    }
    if let Some(o) = &m.obstruction {
        for (field, value, min, max) in [
            ("obstruction.center_x_m", o.center_x_m, -100.0, 200.0),
            ("obstruction.width_m", o.width_m, 0.05, 100.0),
            ("obstruction.depth_m", o.depth_m, 0.05, 100.0),
            ("obstruction.height_m", o.height_m, 0.05, 100.0),
            ("obstruction.distance_m", o.distance_m, 0.05, 100.0),
            ("obstruction.reflectance", o.reflectance, 0.0, 0.95),
        ] {
            bound(field, value, min, max)?;
        }
        if let Some(h) = &m.overhang {
            let x_overlap = o.center_x_m + o.width_m / 2.0 >= left - h.side_extension_m
                && o.center_x_m - o.width_m / 2.0 <= right + h.side_extension_m;
            if x_overlap
                && o.height_m >= m.window.sill_m + m.window.height_m + h.gap_above_window_m
                && o.distance_m < h.projection_m + 0.01
            {
                return Err(invalid("obstruction.distance_m", "obstruction must clear the overhang by at least 0.01 m when their extents intersect"));
            }
        }
    }
    if let Some(meta) = &m.metadata {
        for (field, value) in [
            ("metadata.location", &meta.location),
            ("metadata.iana_timezone", &meta.iana_timezone),
        ] {
            if let Some(s) = value {
                if s.chars().count() > 120 || s.chars().any(char::is_control) {
                    return Err(invalid(field, "must contain at most 120 printable characters; metadata does not affect this calculation"));
                }
            }
        }
    }
    glazing_transmissivity(m.materials.visible_transmittance)?;
    Ok(())
}

/// Rotate local X/Y clockwise as seen from above. Local origin is fixed.
pub fn local_to_world(point: Vec3, rotation_deg: f64) -> Vec3 {
    let (s, c) = rotation_deg.to_radians().sin_cos();
    [
        clean_zero(point[0] * c + point[1] * s),
        clean_zero(-point[0] * s + point[1] * c),
        point[2],
    ]
}

pub fn world_to_local(point: Vec3, rotation_deg: f64) -> Vec3 {
    local_to_world(point, -rotation_deg)
}

/// Three.js uses Y up; this proper rotation retains a right-handed frame.
pub fn world_to_renderer(point: Vec3) -> Vec3 {
    [point[0], point[2], -point[1]]
}

pub fn south_facade_azimuth_deg(rotation_deg: f64) -> f64 {
    (180.0 + rotation_deg).rem_euclid(360.0)
}

fn clean_zero(v: f64) -> f64 {
    if v == 0.0 {
        0.0
    } else {
        v
    }
}

fn grid(m: &Model) -> Result<SensorGrid> {
    let width = m.room.width_m - 2.0 * m.sensors.wall_inset_m;
    let depth = m.room.depth_m - 2.0 * m.sensors.wall_inset_m;
    let columns = (width / m.sensors.requested_spacing_m).ceil() as usize;
    let rows = (depth / m.sensors.requested_spacing_m).ceil() as usize;
    if columns == 0 || rows == 0 || columns.saturating_mul(rows) > MAX_SENSORS {
        return Err(invalid("sensors.requested_spacing_m", format!("requested grid has {} sensors; maximum is {MAX_SENSORS}. Increase spacing or inset", columns.saturating_mul(rows))));
    }
    let dx = width / columns as f64;
    let dy = depth / rows as f64;
    let mut sensors = Vec::with_capacity(columns * rows);
    for row in 0..rows {
        for col in 0..columns {
            let local = [
                m.sensors.wall_inset_m + (col as f64 + 0.5) * dx,
                m.sensors.wall_inset_m + (row as f64 + 0.5) * dy,
                m.sensors.workplane_height_m,
            ];
            if local[0] <= 0.0
                || local[0] >= m.room.width_m
                || local[1] <= 0.0
                || local[1] >= m.room.depth_m
                || local[2] <= 0.0
                || local[2] >= m.room.height_m
            {
                return Err(invalid(
                    "sensors",
                    "generated sensor is outside the open room interior",
                ));
            }
            sensors.push(Sensor {
                id: format!("s{:06}", sensors.len() + 1),
                position: local_to_world(local, m.room.rotation_deg),
                direction: [0.0, 0.0, 1.0],
            });
        }
    }
    Ok(SensorGrid {
        sensors,
        columns,
        rows,
        actual_spacing_x_m: dx,
        actual_spacing_y_m: dy,
        cell_area_m2: dx * dy,
    })
}

pub fn generate_sensors(m: &Model) -> Result<SensorGrid> {
    validate_inputs(m)?;
    grid(m)
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: Vec3) -> f64 {
    dot(a, a).sqrt()
}

/// Validate a simple convex planar polygon, including consistent winding.
pub fn validate_polygon(vertices: &[Vec3], expected_normal: Vec3) -> Result<()> {
    if vertices.len() < 3 || vertices.iter().flatten().any(|v| !v.is_finite()) {
        return Err(invalid(
            "geometry",
            "polygon needs at least three finite vertices",
        ));
    }
    if expected_normal.iter().any(|v| !v.is_finite()) || norm(expected_normal) < 1e-12 {
        return Err(invalid(
            "geometry",
            "expected polygon normal must be finite and nonzero",
        ));
    }
    let n = cross(sub(vertices[1], vertices[0]), sub(vertices[2], vertices[0]));
    if norm(n) < 1e-10 || dot(n, expected_normal) <= 0.0 {
        return Err(invalid(
            "geometry",
            "polygon is degenerate or has reversed winding",
        ));
    }
    for i in 0..vertices.len() {
        let a = vertices[i];
        let b = vertices[(i + 1) % vertices.len()];
        let c = vertices[(i + 2) % vertices.len()];
        if norm(sub(b, a)) < 1e-8 || dot(cross(sub(b, a), sub(c, b)), n) <= 0.0 {
            return Err(invalid(
                "geometry",
                "polygon contains overlapping edges, is nonconvex or has inconsistent winding",
            ));
        }
        if dot(sub(a, vertices[0]), n).abs() / norm(n) > 1e-7 {
            return Err(invalid("geometry", "polygon must be planar"));
        }
    }
    Ok(())
}

fn geometry(m: &Model) -> Result<Vec<Surface>> {
    let (w, d, h) = (m.room.width_m, m.room.depth_m, m.room.height_m);
    let mut surfaces = Vec::new();
    let mut add = |id: &str,
                   material: SurfaceMaterial,
                   reflectance: Option<f64>,
                   vertices: Vec<Vec3>,
                   normal: Vec3|
     -> Result<()> {
        validate_polygon(&vertices, normal)?;
        let vertices: Vec<_> = vertices
            .into_iter()
            .map(|v| local_to_world(v, m.room.rotation_deg))
            .collect();
        validate_polygon(&vertices, local_to_world(normal, m.room.rotation_deg))?;
        surfaces.push(Surface {
            id: id.into(),
            material,
            vertices,
            reflectance,
        });
        Ok(())
    };
    add(
        "floor",
        SurfaceMaterial::Floor,
        Some(m.materials.floor_reflectance),
        vec![[0., 0., 0.], [w, 0., 0.], [w, d, 0.], [0., d, 0.]],
        [0., 0., 1.],
    )?;
    add(
        "ceiling",
        SurfaceMaterial::Ceiling,
        Some(m.materials.ceiling_reflectance),
        vec![[0., 0., h], [0., d, h], [w, d, h], [w, 0., h]],
        [0., 0., -1.],
    )?;
    add(
        "west_wall",
        SurfaceMaterial::Wall,
        Some(m.materials.wall_reflectance),
        vec![[0., 0., 0.], [0., d, 0.], [0., d, h], [0., 0., h]],
        [1., 0., 0.],
    )?;
    add(
        "east_wall",
        SurfaceMaterial::Wall,
        Some(m.materials.wall_reflectance),
        vec![[w, 0., 0.], [w, 0., h], [w, d, h], [w, d, 0.]],
        [-1., 0., 0.],
    )?;
    add(
        "north_wall",
        SurfaceMaterial::Wall,
        Some(m.materials.wall_reflectance),
        vec![[0., d, 0.], [w, d, 0.], [w, d, h], [0., d, h]],
        [0., -1., 0.],
    )?;
    let (xl, xr, zb, zt) = (
        m.window.center_x_m - m.window.width_m / 2.,
        m.window.center_x_m + m.window.width_m / 2.,
        m.window.sill_m,
        m.window.sill_m + m.window.height_m,
    );
    // Disjoint rectangles tile the facade except for the glass aperture.
    for (id, x0, x1, z0, z1) in [
        ("south_below", 0., w, 0., zb),
        ("south_above", 0., w, zt, h),
        ("south_left", 0., xl, zb, zt),
        ("south_right", xr, w, zb, zt),
    ] {
        add(
            id,
            SurfaceMaterial::Wall,
            Some(m.materials.wall_reflectance),
            vec![[x0, 0., z0], [x0, 0., z1], [x1, 0., z1], [x1, 0., z0]],
            [0., 1., 0.],
        )?;
    }
    add(
        "window",
        SurfaceMaterial::Glass,
        None,
        vec![[xl, 0., zb], [xr, 0., zb], [xr, 0., zt], [xl, 0., zt]],
        [0., -1., 0.],
    )?;
    if let Some(o) = &m.overhang {
        let z = zt + o.gap_above_window_m;
        let x0 = xl - o.side_extension_m;
        let x1 = xr + o.side_extension_m;
        add(
            "overhang",
            SurfaceMaterial::Overhang,
            Some(o.reflectance),
            vec![
                [x0, -o.projection_m, z],
                [x1, -o.projection_m, z],
                [x1, 0., z],
                [x0, 0., z],
            ],
            [0., 0., 1.],
        )?;
    }
    if let Some(o) = &m.obstruction {
        let (x0, x1, y0, y1, z) = (
            o.center_x_m - o.width_m / 2.,
            o.center_x_m + o.width_m / 2.,
            -o.distance_m - o.depth_m,
            -o.distance_m,
            o.height_m,
        );
        for (id, v, n) in [
            (
                "obstruction_bottom",
                vec![[x0, y0, 0.], [x0, y1, 0.], [x1, y1, 0.], [x1, y0, 0.]],
                [0., 0., -1.],
            ),
            (
                "obstruction_top",
                vec![[x0, y0, z], [x1, y0, z], [x1, y1, z], [x0, y1, z]],
                [0., 0., 1.],
            ),
            (
                "obstruction_west",
                vec![[x0, y0, 0.], [x0, y0, z], [x0, y1, z], [x0, y1, 0.]],
                [-1., 0., 0.],
            ),
            (
                "obstruction_east",
                vec![[x1, y0, 0.], [x1, y1, 0.], [x1, y1, z], [x1, y0, z]],
                [1., 0., 0.],
            ),
            (
                "obstruction_south",
                vec![[x0, y0, 0.], [x1, y0, 0.], [x1, y0, z], [x0, y0, z]],
                [0., -1., 0.],
            ),
            (
                "obstruction_north",
                vec![[x0, y1, 0.], [x0, y1, z], [x1, y1, z], [x1, y1, 0.]],
                [0., 1., 0.],
            ),
        ] {
            add(id, SurfaceMaterial::Obstruction, Some(o.reflectance), v, n)?;
        }
    }
    Ok(surfaces)
}

pub fn generate_geometry(m: &Model) -> Result<Vec<Surface>> {
    validate_inputs(m)?;
    geometry(m)
}

/// Canonical JSON recursively sorts object keys and normalizes negative zero.
/// Array order is significant (in particular, sensor order).
pub fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    fn normalize(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Number(n) if n.as_f64() == Some(0.) => {
                *n = serde_json::Number::from_f64(0.).expect("finite zero")
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(normalize),
            serde_json::Value::Object(o) => {
                let mut entries: Vec<_> = std::mem::take(o).into_iter().collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                for (key, mut value) in entries {
                    normalize(&mut value);
                    o.insert(key, value);
                }
            }
            _ => (),
        }
    }
    let mut value = serde_json::to_value(value)?;
    normalize(&mut value);
    Ok(serde_json::to_vec(&value)?)
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn canonical_hash<T: Serialize>(value: &T) -> Result<String> {
    Ok(sha256(&canonical_json(value)?))
}

fn scientific_input(m: &Model) -> Result<serde_json::Value> {
    let mut v = serde_json::to_value(m)?;
    if let Some(o) = v.as_object_mut() {
        o.remove("name");
        o.remove("metadata");
    }
    Ok(v)
}

/// Model conditions that must match for A/B. Engine identity and numerical
/// settings must additionally match in the application service.
pub fn comparison_conditions(m: &Model) -> Result<serde_json::Value> {
    validate_inputs(m)?;
    Ok(
        serde_json::json!({"schema_version":MODEL_SCHEMA_VERSION,"room":m.room,
        "sensor_grid":grid(m)?,"sky":m.sky,"obstruction":m.obstruction,
        "reflectances":{"ceiling":m.materials.ceiling_reflectance,"walls":m.materials.wall_reflectance,"floor":m.materials.floor_reflectance}}),
    )
}

pub fn validate_model(m: &Model) -> Result<ValidatedModel> {
    validate_inputs(m)?;
    let sensor_grid = grid(m)?;
    let surfaces = geometry(m)?;
    Ok(ValidatedModel {
        model: m.clone(),
        surfaces,
        normalized_input_hash: canonical_hash(m)?,
        scientific_input_hash: canonical_hash(&scientific_input(m)?)?,
        sensor_hash: canonical_hash(&sensor_grid.sensors)?,
        comparison_conditions_hash: canonical_hash(&comparison_conditions(m)?)?,
        sensor_grid,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignDifference {
    pub field: String,
    pub a: serde_json::Value,
    pub b: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonConditions {
    pub conditions_hash: String,
    pub design_differences: Vec<DesignDifference>,
}

pub fn compare_models(a: &Model, b: &Model) -> Result<ComparisonConditions> {
    let va = validate_model(a)?;
    let vb = validate_model(b)?;
    if va.comparison_conditions_hash != vb.comparison_conditions_hash {
        let ca = comparison_conditions(a)?;
        let cb = comparison_conditions(b)?;
        let changed: Vec<_> = ["room", "sensor_grid", "sky", "obstruction", "reflectances"]
            .into_iter()
            .filter(|k| ca[*k] != cb[*k])
            .collect();
        return Err(CoreError::IncompatibleComparison(format!("these required conditions differ: {}. Only window, glazing and overhang differences are allowed; sensors are never resampled",changed.join(", "))));
    }
    let mut design_differences = Vec::new();
    for (field, av, bv) in [
        (
            "window",
            serde_json::to_value(&a.window)?,
            serde_json::to_value(&b.window)?,
        ),
        (
            "visible_transmittance",
            serde_json::json!(a.materials.visible_transmittance),
            serde_json::json!(b.materials.visible_transmittance),
        ),
        (
            "overhang",
            serde_json::to_value(&a.overhang)?,
            serde_json::to_value(&b.overhang)?,
        ),
    ] {
        if av != bv {
            design_differences.push(DesignDifference {
                field: field.into(),
                a: av,
                b: bv,
            });
        }
    }
    Ok(ComparisonConditions {
        conditions_hash: va.comparison_conditions_hash,
        design_differences,
    })
}

pub fn result_is_stale(historical_scientific_hash: &str, current: &Model) -> Result<bool> {
    Ok(validate_model(current)?.scientific_input_hash != historical_scientific_hash)
}

pub fn glazing_transmissivity(t: f64) -> Result<f64> {
    bound("materials.visible_transmittance", t, 0.01, 0.90)?;
    let tn = ((0.840_252_843_5 + 0.007_252_223_9 * t * t).sqrt() - 0.916_653_066_1)
        / (0.003_626_111_9 * t);
    if !tn.is_finite() || !(0.0..=1.0).contains(&tn) {
        return Err(invalid(
            "materials.visible_transmittance",
            "converted Radiance glass transmissivity is outside [0,1]",
        ));
    }
    Ok(tn)
}

/// Pinned Radiance RGB irradiance convention, not broadband solar efficacy.
pub fn rgb_to_lux(rgb: Vec3) -> Result<f64> {
    if rgb.iter().any(|v| !v.is_finite() || *v < 0.) {
        return Err(invalid(
            "rgb",
            "irradiance channels must be finite and nonnegative",
        ));
    }
    let lux = 179.0 * (0.265_074_126 * rgb[0] + 0.670_114_631 * rgb[1] + 0.064_811_243 * rgb[2]);
    if !lux.is_finite() {
        return Err(invalid("rgb", "irradiance overflows lux conversion"));
    }
    Ok(lux)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Statistics {
    pub min: f64,
    pub mean: f64,
    pub max: f64,
    pub count: usize,
}

/// Arithmetic mean of equal-area cells; permits signed values for A/B deltas.
pub fn statistics(values: &[f64]) -> Result<Statistics> {
    if values.is_empty() || values.iter().any(|v| !v.is_finite()) {
        return Err(invalid(
            "results",
            "statistics require nonempty finite values",
        ));
    }
    let (mut sum, mut compensation) = (0., 0.);
    for value in values {
        let corrected = *value - compensation;
        let updated = sum + corrected;
        compensation = (updated - sum) - corrected;
        sum = updated;
    }
    let mean = sum / values.len() as f64;
    if !mean.is_finite() {
        return Err(invalid("results", "statistics overflow"));
    }
    Ok(Statistics {
        min: values.iter().copied().fold(f64::INFINITY, f64::min),
        mean,
        max: values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        count: values.len(),
    })
}

pub fn validate_lux(values: &[f64], expected: usize) -> Result<()> {
    if values.len() != expected || expected == 0 {
        return Err(invalid(
            "results",
            format!(
                "expected {expected} sensor values; received {}",
                values.len()
            ),
        ));
    }
    if values.iter().any(|v| !v.is_finite() || *v < 0.) {
        return Err(invalid(
            "results",
            "lux values must be finite and nonnegative",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} != {b}");
    }
    #[test]
    fn default_equal_cells_are_centered_stable_and_contained() {
        let v = validate_model(&Model::default()).unwrap();
        let g = v.sensor_grid;
        assert_eq!((g.columns, g.rows, g.sensors.len()), (7, 11, 77));
        close(g.actual_spacing_x_m, 0.5);
        close(g.actual_spacing_y_m, 0.5);
        close(g.cell_area_m2, 0.25);
        assert_eq!(
            g.sensors[0],
            Sensor {
                id: "s000001".into(),
                position: [0.5, 0.5, 0.8],
                direction: [0., 0., 1.]
            }
        );
        assert_eq!(g.sensors[76].position, [3.5, 5.5, 0.8]);
        let mut m = Model::default();
        m.sensors.requested_spacing_m = 0.6;
        let g = generate_sensors(&m).unwrap();
        assert_eq!((g.columns, g.rows), (6, 10));
        assert!(g.actual_spacing_x_m <= 0.6 && g.actual_spacing_y_m <= 0.6);
        close(g.cell_area_m2 * (g.sensors.len() as f64), 3.5 * 5.5);
    }
    #[test]
    fn facade_is_split_without_opaque_aperture() {
        let m = Model::default();
        let surfaces = generate_geometry(&m).unwrap();
        let south: Vec<_> = surfaces
            .iter()
            .filter(|s| s.id.starts_with("south_"))
            .collect();
        assert_eq!(south.len(), 4);
        let mut area = 0.;
        for s in south {
            let n = cross(
                sub(s.vertices[1], s.vertices[0]),
                sub(s.vertices[3], s.vertices[0]),
            );
            area += norm(n);
            let min_x = s
                .vertices
                .iter()
                .map(|p| p[0])
                .fold(f64::INFINITY, f64::min);
            let max_x = s
                .vertices
                .iter()
                .map(|p| p[0])
                .fold(f64::NEG_INFINITY, f64::max);
            let min_z = s
                .vertices
                .iter()
                .map(|p| p[2])
                .fold(f64::INFINITY, f64::min);
            let max_z = s
                .vertices
                .iter()
                .map(|p| p[2])
                .fold(f64::NEG_INFINITY, f64::max);
            assert!(
                max_x <= 1.1 + 1e-12
                    || min_x >= 2.9 - 1e-12
                    || max_z <= 0.9 + 1e-12
                    || min_z >= 2.4 - 1e-12
            );
        }
        close(area, 4. * 3. - 1.8 * 1.5);
        assert_eq!(
            surfaces
                .iter()
                .filter(|s| s.material == SurfaceMaterial::Glass)
                .count(),
            1
        );
    }
    #[test]
    fn coordinates_rotation_renderer_and_window_direction() {
        close(south_facade_azimuth_deg(0.), 180.);
        close(south_facade_azimuth_deg(90.), 270.);
        let p = local_to_world([0., -1., 0.], 90.);
        close(p[0], -1.);
        close(p[1], 0.);
        let local = [1.2, 3.4, 5.6];
        let recovered = world_to_local(local_to_world(local, 37.), 37.);
        for i in 0..3 {
            close(recovered[i], local[i]);
        }
        assert_eq!(world_to_renderer([1., 2., 3.]), [1., 3., -2.]);
        let glass = generate_geometry(&Model::default())
            .unwrap()
            .into_iter()
            .find(|s| s.material == SurfaceMaterial::Glass)
            .unwrap();
        assert!(
            cross(
                sub(glass.vertices[1], glass.vertices[0]),
                sub(glass.vertices[2], glass.vertices[0])
            )[1] < 0.
        );
    }
    #[test]
    fn rejects_invalid_geometry_and_bounds() {
        let mut m = Model::default();
        m.room.width_m = f64::NAN;
        assert!(validate_model(&m).is_err());
        m = Model::default();
        m.window.width_m = 4.;
        assert!(validate_model(&m).is_err());
        m = Model::default();
        m.window.sill_m = 2.;
        assert!(validate_model(&m).is_err());
        m = Model::default();
        m.sensors.wall_inset_m = 2.;
        assert!(validate_model(&m).is_err());
        m = Model::default();
        m.sensors.workplane_height_m = 3.;
        assert!(validate_model(&m).is_err());
        m = Model::default();
        m.room.width_m = 100.;
        m.room.depth_m = 100.;
        m.sensors.requested_spacing_m = 0.05;
        assert!(validate_model(&m)
            .unwrap_err()
            .to_string()
            .contains("maximum is 10000"));
        m = Model::default();
        m.overhang = Some(Overhang {
            projection_m: 0.6,
            side_extension_m: 0.,
            gap_above_window_m: 0.,
            reflectance: 0.5,
        });
        m.obstruction = Some(Obstruction {
            center_x_m: 2.,
            width_m: 2.,
            depth_m: 1.,
            height_m: 3.,
            distance_m: 0.2,
            reflectance: 0.2,
        });
        assert!(validate_model(&m).is_err());
        m.obstruction.as_mut().unwrap().distance_m = 1.;
        assert!(validate_model(&m).is_ok());
    }
    #[test]
    fn obstruction_overhang_edge_contact_is_rejected_but_detached_cases_remain_valid() {
        let mut model = Model::alternative_b();
        model.obstruction = Some(Obstruction {
            center_x_m: 3.4,
            width_m: 1.,
            depth_m: 0.2,
            height_m: 3.,
            distance_m: 0.2,
            reflectance: 0.2,
        });
        // The box face touches x=2.9 (or x=1.1 on the opposite side)
        // across the overhang. Boundary-only contact is not detached.
        for rotation in [0., 37.] {
            model.room.rotation_deg = rotation;
            for center_x in [3.4, 0.6] {
                for height in [3., model.window.sill_m + model.window.height_m] {
                    let obstruction = model.obstruction.as_mut().unwrap();
                    obstruction.center_x_m = center_x;
                    obstruction.height_m = height;
                    assert!(validate_model(&model)
                        .unwrap_err()
                        .to_string()
                        .contains("clear the overhang"));
                }
            }
        }
        for (center_x, height, distance) in [
            (3.400001, 3., 0.2),
            (0.599999, 3., 0.2),
            (2., 2.399999, 0.2),
            (3.4, 3., 0.61),
            (2., 3., 0.61),
        ] {
            let obstruction = model.obstruction.as_mut().unwrap();
            obstruction.center_x_m = center_x;
            obstruction.height_m = height;
            obstruction.distance_m = distance;
            assert!(validate_model(&model).is_ok());
        }
    }
    #[test]
    fn polygon_rejects_reversal_degeneracy_nonplanarity_and_crossing() {
        let good = vec![[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]];
        assert!(validate_polygon(&good, [0., 0., 1.]).is_ok());
        assert!(validate_polygon(&good, [0., 0., -1.]).is_err());
        assert!(
            validate_polygon(&[[0., 0., 0.], [0., 0., 0.], [1., 1., 0.]], [0., 0., 1.]).is_err()
        );
        assert!(validate_polygon(
            &[[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.1]],
            [0., 0., 1.]
        )
        .is_err());
        assert!(validate_polygon(
            &[[0., 0., 0.], [1., 1., 0.], [1., 0., 0.], [0., 1., 0.]],
            [0., 0., 1.]
        )
        .is_err());
    }
    #[test]
    fn glazing_and_rgb_conventions() {
        close(glazing_transmissivity(0.6).unwrap(), 0.6540474888954341);
        for t in [0.01, 0.6, 0.9] {
            assert!((0.0..=1.0).contains(&glazing_transmissivity(t).unwrap()));
        }
        for t in [0., 0.91, f64::INFINITY, f64::NAN] {
            assert!(glazing_transmissivity(t).is_err());
        }
        close(rgb_to_lux([1., 1., 1.]).unwrap(), 179.);
        close(rgb_to_lux([2., 2., 2.]).unwrap(), 358.);
        assert!(rgb_to_lux([-1., 0., 0.]).is_err());
        assert!(rgb_to_lux([f64::NAN, 0., 0.]).is_err());
    }
    #[test]
    fn fingerprints_comparison_and_staleness() {
        let a = Model::default();
        let va = validate_model(&a).unwrap();
        let b = Model::alternative_b();
        let vb = validate_model(&b).unwrap();
        assert_eq!(va.sensor_hash, vb.sensor_hash);
        assert_ne!(va.scientific_input_hash, vb.scientific_input_hash);
        assert_eq!(va.comparison_conditions_hash, vb.comparison_conditions_hash);
        assert_eq!(compare_models(&a, &b).unwrap().design_differences.len(), 1);
        let mut renamed = a.clone();
        renamed.name = "Different display name".into();
        renamed.metadata = Some(Metadata {
            location: Some("Synthetic example".into()),
            iana_timezone: Some("Etc/UTC".into()),
        });
        assert!(!result_is_stale(&va.scientific_input_hash, &renamed).unwrap());
        assert!(result_is_stale(&va.scientific_input_hash, &b).unwrap());
        let mut c = b.clone();
        c.sky.exterior_horizontal_illuminance_lux = 9999.;
        assert!(compare_models(&a, &c)
            .unwrap_err()
            .to_string()
            .contains("sky"));
        c = b.clone();
        c.room.rotation_deg = 1.;
        assert!(compare_models(&a, &c).is_err());
        c = b.clone();
        c.materials.wall_reflectance = 0.6;
        assert!(compare_models(&a, &c).is_err());
        c = b;
        c.materials.visible_transmittance = 0.7;
        c.window.width_m = 2.;
        assert!(compare_models(&a, &c).is_ok());
    }
    #[test]
    fn statistics_and_result_validation() {
        assert_eq!(
            statistics(&[-1., 0., 4.]).unwrap(),
            Statistics {
                min: -1.,
                mean: 1.,
                max: 4.,
                count: 3
            }
        );
        assert!(statistics(&[]).is_err());
        assert!(statistics(&[f64::NAN]).is_err());
        assert!(validate_lux(&[0., 1.], 2).is_ok());
        assert!(validate_lux(&[1.], 2).is_err());
        assert!(validate_lux(&[-1.], 1).is_err());
    }
    #[test]
    fn normalized_hash_ignores_negative_zero_and_object_key_order() {
        assert_eq!(
            canonical_hash(&serde_json::json!({"z":-0.0,"a":1.})).unwrap(),
            canonical_hash(&serde_json::json!({"a":1.,"z":0.0})).unwrap()
        );
    }
    #[test]
    fn model_contract_rejects_unknown_fields() {
        let mut value = serde_json::to_value(Model::default()).unwrap();
        value["weather_file"] = serde_json::json!("not-supported.epw");
        assert!(serde_json::from_value::<Model>(value).is_err());
    }
    #[test]
    fn checked_in_synthetic_examples_match_code_defaults() {
        let a: Model =
            serde_json::from_str(include_str!("../../../examples/synthetic-a.json")).unwrap();
        let b: Model =
            serde_json::from_str(include_str!("../../../examples/synthetic-b.json")).unwrap();
        assert_eq!(a, Model::default());
        assert_eq!(b, Model::alternative_b());
        validate_model(&a).unwrap();
        validate_model(&b).unwrap();
    }
    #[test]
    fn default_sensor_grid_has_left_right_symmetry() {
        let m = Model::default();
        let grid = generate_sensors(&m).unwrap();
        for row in 0..grid.rows {
            for col in 0..grid.columns {
                let a = &grid.sensors[row * grid.columns + col];
                let b = &grid.sensors[row * grid.columns + (grid.columns - 1 - col)];
                close(a.position[0] + b.position[0], m.room.width_m);
                close(a.position[1], b.position[1]);
                close(a.position[2], b.position[2]);
            }
        }
    }
    #[test]
    fn glazing_conversion_is_monotonic_over_supported_interval() {
        let mut previous = 0.;
        for i in 1..=90 {
            let converted = glazing_transmissivity(i as f64 / 100.).unwrap();
            assert!(converted > previous);
            previous = converted;
        }
    }

    #[test]
    fn computed_geometry_and_hashes_survive_json_roundtrip() {
        for rotation in [0.0, 17.3, 37.123456, 90.0, 173.555, 270.0, 359.999999] {
            let mut model = Model::default();
            model.room.width_m = 4.123456789;
            model.room.depth_m = 6.87654321;
            model.room.rotation_deg = rotation;
            let original = validate_model(&model).unwrap();
            let bytes = serde_json::to_vec(&original).unwrap();
            let loaded: ValidatedModel = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                canonical_hash(&original).unwrap(),
                canonical_hash(&loaded).unwrap()
            );
            assert_eq!(
                canonical_hash(&loaded).unwrap(),
                canonical_hash(&validate_model(&loaded.model).unwrap()).unwrap()
            );
        }
    }
}
