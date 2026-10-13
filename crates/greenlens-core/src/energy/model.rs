use super::{
    finite, identifier, invalid, positive, range, text, EnergyCalendar, Schedule, ScheduleKind,
    Sum, MAX_THERMAL_ZONES, THERMAL_MODEL_SCHEMA,
};
use crate::{canonical_hash, Result, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const EPS: f64 = 1e-8;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InputSource {
    pub citation: String,
    pub synthetic: bool,
}
impl InputSource {
    fn validate(&self) -> Result<()> {
        text("source.citation", &self.citation)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThermalMaterial {
    pub id: String,
    pub name: String,
    pub thickness_m: f64,
    pub conductivity_w_mk: f64,
    pub density_kg_m3: f64,
    pub specific_heat_j_kgk: f64,
    pub thermal_absorptance: f64,
    pub solar_absorptance: f64,
    pub visible_absorptance: f64,
    pub source: InputSource,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OpaqueConstruction {
    pub id: String,
    pub name: String,
    /// Ordered from the outside of this zone toward its inside. Interzone
    /// partners must have reversed layer sequences, not two unrelated walls.
    pub layers: Vec<String>,
    /// Explicit display U-value convention only; these are not silently emitted
    /// as EnergyPlus boundary convection coefficients.
    pub derived_inside_film_m2k_w: f64,
    pub derived_outside_film_m2k_w: f64,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GlazingBasis {
    WholeWindow,
    GlassWithoutFrame,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThermalGlazing {
    pub id: String,
    pub name: String,
    pub u_value_w_m2k: f64,
    pub shgc: f64,
    pub visible_transmittance: f64,
    pub basis: GlazingBasis,
    pub source: InputSource,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ThermalFace {
    West,
    East,
    South,
    North,
    Floor,
    Roof,
}
impl ThermalFace {
    pub const ALL: [Self; 6] = [
        Self::West,
        Self::East,
        Self::South,
        Self::North,
        Self::Floor,
        Self::Roof,
    ];
    fn axis(self) -> usize {
        match self {
            Self::West | Self::East => 0,
            Self::South | Self::North => 1,
            Self::Floor | Self::Roof => 2,
        }
    }
    fn high(self) -> bool {
        matches!(self, Self::East | Self::North | Self::Roof)
    }
    pub fn opposite(self) -> Self {
        match self {
            Self::West => Self::East,
            Self::East => Self::West,
            Self::South => Self::North,
            Self::North => Self::South,
            Self::Floor => Self::Roof,
            Self::Roof => Self::Floor,
        }
    }
    pub fn local_normal(self) -> Vec3 {
        let mut v = [0.; 3];
        v[self.axis()] = if self.high() { 1. } else { -1. };
        v
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ThermalBoundary {
    Outdoors,
    Adiabatic {
        reason: String,
    },
    /// Initial release approximation, not a ground-coupled heat-transfer model.
    InsulatedGround {
        reason: String,
    },
    Interzone {
        zone_id: String,
        surface_id: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThermalSurface {
    pub id: String,
    pub face: ThermalFace,
    pub construction_id: String,
    pub boundary: ThermalBoundary,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThermalWindow {
    pub id: String,
    pub surface_id: String,
    pub glazing_id: String,
    /// Local face rectangle coordinates. U/V axes are the positive remaining
    /// building axes in increasing index order; independent of outward winding.
    pub offset_u_m: f64,
    pub offset_v_m: f64,
    pub width_m: f64,
    pub height_m: f64,
    pub overhang: Option<ThermalOverhang>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThermalOverhang {
    pub projection_m: f64,
    pub side_extension_m: f64,
    pub gap_above_m: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThermalBoxShade {
    pub id: String,
    pub origin_m: Vec3,
    pub size_m: Vec3,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CurveForm {
    Quadratic {
        coefficients: [f64; 3],
        x: [f64; 2],
    },
    Cubic {
        coefficients: [f64; 4],
        x: [f64; 2],
    },
    Biquadratic {
        coefficients: [f64; 6],
        x: [f64; 2],
        y: [f64; 2],
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PerformanceCurve {
    pub id: String,
    pub form: CurveForm,
    pub output_limits: [f64; 2],
    pub source: InputSource,
}
impl PerformanceCurve {
    fn validate(&self) -> Result<()> {
        identifier("curve.id", &self.id)?;
        self.source.validate()?;
        let limits = |name: &str, v: [f64; 2]| -> Result<()> {
            finite(name, v[0])?;
            finite(name, v[1])?;
            if v[0] >= v[1] {
                return Err(invalid(
                    name,
                    "Lower domain/output bound must be less than upper",
                ));
            }
            Ok(())
        };
        limits("curve.output_limits", self.output_limits)?;
        match &self.form {
            CurveForm::Quadratic { coefficients, x } => {
                for &v in coefficients {
                    finite("curve.coefficient", v)?;
                }
                limits("curve.x", *x)?;
            }
            CurveForm::Cubic { coefficients, x } => {
                for &v in coefficients {
                    finite("curve.coefficient", v)?;
                }
                limits("curve.x", *x)?;
            }
            CurveForm::Biquadratic { coefficients, x, y } => {
                for &v in coefficients {
                    finite("curve.coefficient", v)?;
                }
                limits("curve.x", *x)?;
                limits("curve.y", *y)?;
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SupplyFan {
    pub pressure_rise_pa: f64,
    pub total_efficiency: f64,
    pub motor_efficiency: f64,
    pub motor_in_air_fraction: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BalancedVentilation {
    /// Equal outdoor intake and exhaust, one shared fractional schedule.
    pub design_flow_m3_s: f64,
    pub fraction_schedule: String,
    pub fan_pressure_pa: f64,
    pub total_efficiency: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DxCoolingSystem {
    pub id: String,
    pub gross_rated_capacity_w: f64,
    pub gross_rated_cop: f64,
    pub rated_shr: f64,
    pub rated_airflow_m3_s: f64,
    pub rated_indoor_wet_bulb_c: f64,
    pub rated_outdoor_dry_bulb_c: f64,
    pub capacity_temperature_curve: String,
    pub capacity_flow_curve: String,
    pub eir_temperature_curve: String,
    pub eir_flow_curve: String,
    pub part_load_fraction_curve: String,
    pub availability_schedule: String,
    pub cooling_setpoint_schedule: String,
    pub cooling_deadband_k: f64,
    pub supply_fan: SupplyFan,
    pub balanced_ventilation: BalancedVentilation,
    pub sizing_source: InputSource,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "basis", rename_all = "snake_case", deny_unknown_fields)]
pub enum InfiltrationRate {
    AirChangesPerHour { value: f64 },
    FlowM3PerSecond { value: f64 },
    FlowM3PerSecondPerFloorM2 { value: f64 },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ZoneGains {
    pub people_per_floor_m2: f64,
    pub occupancy_schedule: String,
    pub activity_schedule: String,
    pub people_sensible_fraction: f64,
    pub people_radiant_fraction_of_sensible: f64,
    pub lighting_w_per_floor_m2: f64,
    pub lighting_schedule: String,
    pub lighting_radiant_fraction: f64,
    pub lighting_visible_fraction: f64,
    pub lighting_return_air_fraction: f64,
    pub equipment_w_per_floor_m2: f64,
    pub equipment_schedule: String,
    pub equipment_radiant_fraction: f64,
    pub equipment_latent_fraction: f64,
    pub equipment_lost_fraction: f64,
    pub infiltration: InfiltrationRate,
    pub infiltration_schedule: String,
    pub source: InputSource,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThermalZone {
    pub id: String,
    pub name: String,
    pub storey: u8,
    pub origin_m: Vec3,
    pub size_m: Vec3,
    pub surfaces: Vec<ThermalSurface>,
    pub windows: Vec<ThermalWindow>,
    pub gains: ZoneGains,
    /// None is explicitly unconditioned. No hidden IdealLoads substitution.
    pub cooling_system: Option<DxCoolingSystem>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThermalSite {
    pub latitude_deg_north: f64,
    pub longitude_deg_east: f64,
    pub elevation_m: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThermalModel {
    pub schema_version: String,
    pub name: String,
    pub site: ThermalSite,
    /// Clockwise building-local rotation from true north; all zone boxes share it.
    pub north_rotation_deg: f64,
    pub zones: Vec<ThermalZone>,
    pub materials: Vec<ThermalMaterial>,
    pub constructions: Vec<OpaqueConstruction>,
    pub glazing: Vec<ThermalGlazing>,
    pub curves: Vec<PerformanceCurve>,
    pub schedules: Vec<Schedule>,
    pub detached_shading: Vec<ThermalBoxShade>,
    pub exterior_lighting_w: f64,
    pub exterior_lighting_schedule: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ValidatedThermalSurface {
    pub id: String,
    pub zone_id: String,
    pub face: ThermalFace,
    pub vertices_m: [Vec3; 4],
    pub outward_normal: Vec3,
    pub gross_area_m2: f64,
    pub opening_area_m2: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FacadeWwr {
    pub orientation: String,
    pub gross_wall_m2: f64,
    pub opening_m2: f64,
    pub ratio: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ValidatedThermalModel {
    pub model: ThermalModel,
    pub surfaces: Vec<ValidatedThermalSurface>,
    pub gross_floor_area_m2: f64,
    pub conditioned_floor_area_m2: f64,
    pub construction_u_values_w_m2k: BTreeMap<String, f64>,
    pub facade_wwr: Vec<FacadeWwr>,
    pub model_input_sha256: String,
}

fn register<'a>(seen: &mut BTreeSet<&'a str>, id: &'a str) -> Result<()> {
    identifier("id", id)?;
    if !seen.insert(id) {
        return Err(invalid("id", format!("Duplicate stable ID: {id}")));
    }
    Ok(())
}
fn bounds(origin: Vec3, size: Vec3) -> Result<([f64; 3], [f64; 3])> {
    let mut max = [0.; 3];
    for i in 0..3 {
        range("geometry.origin_m", origin[i], -100_000., 100_000.)?;
        positive("geometry.size_m", size[i])?;
        range("geometry.size_m", size[i], 0.01, 10_000.)?;
        max[i] = origin[i] + size[i];
    }
    Ok((origin, max))
}
fn intersection(a: ([f64; 3], [f64; 3]), b: ([f64; 3], [f64; 3])) -> [f64; 3] {
    std::array::from_fn(|i| a.1[i].min(b.1[i]) - a.0[i].max(b.0[i]))
}
fn axes(face: ThermalFace) -> [usize; 2] {
    match face.axis() {
        0 => [1, 2],
        1 => [0, 2],
        _ => [0, 1],
    }
}
fn rotate(p: Vec3, deg: f64) -> Vec3 {
    let (s, c) = deg.to_radians().sin_cos();
    [c * p[0] + s * p[1], -s * p[0] + c * p[1], p[2]]
}
fn face_geometry(zone: &ThermalZone, face: ThermalFace, rotation: f64) -> ([Vec3; 4], Vec3, f64) {
    let a = face.axis();
    let [u, v] = axes(face);
    let mut p = zone.origin_m;
    if face.high() {
        p[a] += zone.size_m[a];
    }
    let mut vertices = [p; 4];
    vertices[1][u] += zone.size_m[u];
    vertices[2][u] += zone.size_m[u];
    vertices[2][v] += zone.size_m[v];
    vertices[3][v] += zone.size_m[v];
    // Positive remaining-axis cross product is +X,-Y,+Z respectively.
    let cross_high = a != 1;
    if cross_high != face.high() {
        vertices.reverse();
    }
    (
        vertices.map(|p| rotate(p, rotation)),
        rotate(face.local_normal(), rotation),
        zone.size_m[u] * zone.size_m[v],
    )
}
pub fn construction_u_value(
    construction: &OpaqueConstruction,
    materials: &[ThermalMaterial],
) -> Result<f64> {
    range(
        "construction.inside_film",
        construction.derived_inside_film_m2k_w,
        0.,
        10.,
    )?;
    range(
        "construction.outside_film",
        construction.derived_outside_film_m2k_w,
        0.,
        10.,
    )?;
    if construction.layers.is_empty() || construction.layers.len() > 16 {
        return Err(invalid(
            "construction.layers",
            "Expected 1–16 explicit ordered layers",
        ));
    }
    let mut sum = Sum::default();
    sum.add(construction.derived_inside_film_m2k_w);
    sum.add(construction.derived_outside_film_m2k_w);
    for id in &construction.layers {
        let m = materials
            .iter()
            .find(|m| m.id == *id)
            .ok_or_else(|| invalid("construction.layers", format!("Unknown material {id}")))?;
        positive("material.thickness_m", m.thickness_m)?;
        positive("material.conductivity_w_mk", m.conductivity_w_mk)?;
        sum.add(m.thickness_m / m.conductivity_w_mk);
    }
    positive("construction.resistance", sum.value())?;
    let u = 1. / sum.value();
    positive("construction.u_value", u)?;
    Ok(u)
}
pub fn validate_thermal_model(
    model: &ThermalModel,
    calendar: &EnergyCalendar,
) -> Result<ValidatedThermalModel> {
    if model.schema_version != THERMAL_MODEL_SCHEMA {
        return Err(invalid(
            "thermal.schema_version",
            "Unsupported thermal model version",
        ));
    }
    text("thermal.name", &model.name)?;
    calendar.validate()?;
    range("site.latitude", model.site.latitude_deg_north, -90., 90.)?;
    range("site.longitude", model.site.longitude_deg_east, -180., 180.)?;
    range("site.elevation", model.site.elevation_m, -500., 9000.)?;
    range("north_rotation_deg", model.north_rotation_deg, 0., 360.)?;
    if model.zones.is_empty() || model.zones.len() > MAX_THERMAL_ZONES {
        return Err(invalid(
            "thermal.zones",
            "Expected 1–8 rectangular thermal zones",
        ));
    }
    for (field, count, limit) in [
        ("materials", model.materials.len(), 256),
        ("constructions", model.constructions.len(), 128),
        ("glazing", model.glazing.len(), 64),
        ("curves", model.curves.len(), 128),
        ("schedules", model.schedules.len(), 128),
        ("shading", model.detached_shading.len(), 64),
    ] {
        if count > limit {
            return Err(invalid(field, format!("At most {limit} entries")));
        }
    }
    let mut ids = BTreeSet::new();
    for m in &model.materials {
        register(&mut ids, &m.id)?;
        text("material.name", &m.name)?;
        m.source.validate()?;
        for (name, v) in [
            ("thickness", m.thickness_m),
            ("conductivity", m.conductivity_w_mk),
            ("density", m.density_kg_m3),
            ("specific_heat", m.specific_heat_j_kgk),
        ] {
            positive(name, v)?;
        }
        for v in [
            m.thermal_absorptance,
            m.solar_absorptance,
            m.visible_absorptance,
        ] {
            range("material.absorptance", v, 0., 1.)?;
        }
    }
    let mut u_values = BTreeMap::new();
    for c in &model.constructions {
        register(&mut ids, &c.id)?;
        text("construction.name", &c.name)?;
        u_values.insert(c.id.clone(), construction_u_value(c, &model.materials)?);
    }
    for g in &model.glazing {
        register(&mut ids, &g.id)?;
        text("glazing.name", &g.name)?;
        g.source.validate()?;
        positive("glazing.u_value", g.u_value_w_m2k)?;
        range("glazing.shgc", g.shgc, 0., 1.)?;
        range(
            "glazing.visible_transmittance",
            g.visible_transmittance,
            0.,
            1.,
        )?;
    }
    for curve in &model.curves {
        register(&mut ids, &curve.id)?;
        curve.validate()?;
    }
    for schedule in &model.schedules {
        register(&mut ids, &schedule.id)?;
        schedule.validate(calendar)?;
    }
    let schedule = |id: &str, kind: ScheduleKind| -> Result<()> {
        if !model.schedules.iter().any(|s| s.id == id && s.kind == kind) {
            return Err(invalid(
                "schedule.assignment",
                format!("Missing schedule {id} of required kind {kind:?}"),
            ));
        }
        Ok(())
    };
    range(
        "exterior_lighting_w",
        model.exterior_lighting_w,
        0.,
        f64::MAX,
    )?;
    schedule(&model.exterior_lighting_schedule, ScheduleKind::Fraction)?;
    let mut storeys: BTreeMap<u8, (f64, f64)> = BTreeMap::new();
    let mut surfaces = Vec::new();
    let mut gross = Sum::default();
    let mut conditioned = Sum::default();
    let mut facades = [(Sum::default(), Sum::default()); 4];
    for zone in &model.zones {
        register(&mut ids, &zone.id)?;
        text("zone.name", &zone.name)?;
        bounds(zone.origin_m, zone.size_m)?;
        if !(1..=2).contains(&zone.storey) {
            return Err(invalid("zone.storey", "Expected storey 1 or 2"));
        }
        if let Some((z, h)) = storeys.insert(zone.storey, (zone.origin_m[2], zone.size_m[2])) {
            if (z - zone.origin_m[2]).abs() > EPS || (h - zone.size_m[2]).abs() > EPS {
                return Err(invalid(
                    "zone.storey",
                    "Zones on a storey require one floor and ceiling elevation",
                ));
            }
        }
        gross.add(zone.size_m[0] * zone.size_m[1]);
        if zone.cooling_system.is_some() {
            conditioned.add(zone.size_m[0] * zone.size_m[1]);
        }
        if zone.surfaces.len() != 6
            || zone
                .surfaces
                .iter()
                .map(|s| s.face)
                .collect::<BTreeSet<_>>()
                .len()
                != 6
        {
            return Err(invalid(
                "zone.surfaces",
                "Exactly one of each six rectangular faces is required",
            ));
        }
        if zone.windows.len() > 64 {
            return Err(invalid(
                "zone.windows",
                "At most 64 rectangular openings per zone",
            ));
        }
        for s in &zone.surfaces {
            register(&mut ids, &s.id)?;
            if !u_values.contains_key(&s.construction_id) {
                return Err(invalid(
                    "surface.construction",
                    format!("Unknown construction {}", s.construction_id),
                ));
            }
            match &s.boundary {
                ThermalBoundary::InsulatedGround { reason } => {
                    text("boundary.reason", reason)?;
                    if s.face != ThermalFace::Floor || zone.storey != 1 {
                        return Err(invalid("boundary.insulated_ground","Only first-storey floors support this explicit adiabatic approximation"));
                    }
                }
                ThermalBoundary::Adiabatic { reason } => text("boundary.reason", reason)?,
                _ => (),
            }
            let (vertices_m, outward_normal, area) =
                face_geometry(zone, s.face, model.north_rotation_deg);
            let [u, v] = axes(s.face);
            let mut opening = Sum::default();
            let windows: Vec<_> = zone
                .windows
                .iter()
                .filter(|w| w.surface_id == s.id)
                .collect();
            for (i, w) in windows.iter().enumerate() {
                register(&mut ids, &w.id)?;
                if !model.glazing.iter().any(|g| g.id == w.glazing_id) {
                    return Err(invalid("window.glazing", "Unknown glazing"));
                }
                if !matches!(s.boundary, ThermalBoundary::Outdoors) || s.face == ThermalFace::Floor
                {
                    return Err(invalid(
                        "window.parent",
                        "Windows require an exterior wall or roof",
                    ));
                }
                positive("window.width", w.width_m)?;
                positive("window.height", w.height_m)?;
                range("window.offset_u", w.offset_u_m, 0., zone.size_m[u])?;
                range("window.offset_v", w.offset_v_m, 0., zone.size_m[v])?;
                if w.offset_u_m + w.width_m > zone.size_m[u]
                    || w.offset_v_m + w.height_m > zone.size_m[v]
                {
                    return Err(invalid(
                        "window.rectangle",
                        "Opening must be contained in its parent face",
                    ));
                }
                for other in &windows[..i] {
                    if w.offset_u_m < other.offset_u_m + other.width_m
                        && other.offset_u_m < w.offset_u_m + w.width_m
                        && w.offset_v_m < other.offset_v_m + other.height_m
                        && other.offset_v_m < w.offset_v_m + w.height_m
                    {
                        return Err(invalid("windows", "Overlapping openings"));
                    }
                }
                if let Some(o) = &w.overhang {
                    if s.face.axis() == 2 {
                        return Err(invalid("overhang", "Wall windows only"));
                    }
                    positive("overhang.projection", o.projection_m)?;
                    range("overhang.side_extension", o.side_extension_m, 0., 1000.)?;
                    range("overhang.gap", o.gap_above_m, 0., 1000.)?;
                }
                opening.add(w.width_m * w.height_m);
            }
            if s.face.axis() != 2 && matches!(s.boundary, ThermalBoundary::Outdoors) {
                let azimuth = outward_normal[0]
                    .atan2(outward_normal[1])
                    .to_degrees()
                    .rem_euclid(360.);
                let bin = ((azimuth + 45.).rem_euclid(360.) / 90.).floor() as usize;
                facades[bin].0.add(area);
                facades[bin].1.add(opening.value());
            }
            surfaces.push(ValidatedThermalSurface {
                id: s.id.clone(),
                zone_id: zone.id.clone(),
                face: s.face,
                vertices_m,
                outward_normal,
                gross_area_m2: area,
                opening_area_m2: opening.value(),
            });
        }
        if zone
            .windows
            .iter()
            .any(|w| !zone.surfaces.iter().any(|s| s.id == w.surface_id))
        {
            return Err(invalid("window.surface_id", "Unknown parent surface"));
        }
        let g = &zone.gains;
        g.source.validate()?;
        for v in [
            g.people_per_floor_m2,
            g.lighting_w_per_floor_m2,
            g.equipment_w_per_floor_m2,
        ] {
            range("gains.density", v, 0., f64::MAX)?;
        }
        for v in [
            g.people_sensible_fraction,
            g.people_radiant_fraction_of_sensible,
            g.lighting_radiant_fraction,
            g.lighting_visible_fraction,
            g.lighting_return_air_fraction,
            g.equipment_radiant_fraction,
            g.equipment_latent_fraction,
            g.equipment_lost_fraction,
        ] {
            range("gains.fraction", v, 0., 1.)?;
        }
        if g.lighting_radiant_fraction
            + g.lighting_visible_fraction
            + g.lighting_return_air_fraction
            > 1.
            || g.equipment_radiant_fraction
                + g.equipment_latent_fraction
                + g.equipment_lost_fraction
                > 1.
        {
            return Err(invalid(
                "gains.fractions",
                "Explicit fractions must sum to at most one; remainder is convective",
            ));
        }
        for id in [
            &g.occupancy_schedule,
            &g.lighting_schedule,
            &g.equipment_schedule,
            &g.infiltration_schedule,
        ] {
            schedule(id, ScheduleKind::Fraction)?;
        }
        schedule(&g.activity_schedule, ScheduleKind::ActivityWattsPerPerson)?;
        let infiltration = match g.infiltration {
            InfiltrationRate::AirChangesPerHour { value }
            | InfiltrationRate::FlowM3PerSecond { value }
            | InfiltrationRate::FlowM3PerSecondPerFloorM2 { value } => value,
        };
        range("infiltration.rate", infiltration, 0., f64::MAX)?;
        if let Some(h) = &zone.cooling_system {
            register(&mut ids, &h.id)?;
            h.sizing_source.validate()?;
            for v in [
                h.gross_rated_capacity_w,
                h.gross_rated_cop,
                h.rated_airflow_m3_s,
            ] {
                positive("cooling.rated", v)?;
            }
            range("cooling.shr", h.rated_shr, 0., 1.)?;
            range(
                "cooling.indoor_wet_bulb",
                h.rated_indoor_wet_bulb_c,
                -50.,
                100.,
            )?;
            range(
                "cooling.outdoor_dry_bulb",
                h.rated_outdoor_dry_bulb_c,
                -50.,
                100.,
            )?;
            range("cooling.deadband", h.cooling_deadband_k, 0., 20.)?;
            schedule(&h.availability_schedule, ScheduleKind::Fraction)?;
            schedule(
                &h.cooling_setpoint_schedule,
                ScheduleKind::TemperatureCelsius,
            )?;
            for (id, two_dimensional) in [
                (&h.capacity_temperature_curve, true),
                (&h.capacity_flow_curve, false),
                (&h.eir_temperature_curve, true),
                (&h.eir_flow_curve, false),
                (&h.part_load_fraction_curve, false),
            ] {
                let curve = model
                    .curves
                    .iter()
                    .find(|c| c.id == *id)
                    .ok_or_else(|| invalid("cooling.curve", format!("Unknown curve {id}")))?;
                if matches!(curve.form, CurveForm::Biquadratic { .. }) != two_dimensional {
                    return Err(invalid(
                        "cooling.curve",
                        "Curve dimensionality does not match declared role",
                    ));
                }
            }
            let fan = &h.supply_fan;
            range("fan.pressure_rise", fan.pressure_rise_pa, 0., f64::MAX)?;
            positive("fan.total_efficiency", fan.total_efficiency)?;
            range("fan.total_efficiency", fan.total_efficiency, 0., 1.)?;
            positive("fan.motor_efficiency", fan.motor_efficiency)?;
            range("fan.motor_efficiency", fan.motor_efficiency, 0., 1.)?;
            range(
                "fan.motor_in_air_fraction",
                fan.motor_in_air_fraction,
                0.,
                1.,
            )?;
            let air = &h.balanced_ventilation;
            range("ventilation.flow", air.design_flow_m3_s, 0., f64::MAX)?;
            range("ventilation.pressure", air.fan_pressure_pa, 0., f64::MAX)?;
            positive("ventilation.efficiency", air.total_efficiency)?;
            range("ventilation.efficiency", air.total_efficiency, 0., 1.)?;
            schedule(&air.fraction_schedule, ScheduleKind::Fraction)?;
        }
    }
    if !storeys.contains_key(&1) {
        return Err(invalid("storeys", "The first storey is required"));
    }
    if let Some(second) = storeys.get(&2) {
        let first = storeys[&1];
        if (second.0 - (first.0 + first.1)).abs() > EPS {
            return Err(invalid(
                "storeys",
                "Two storeys require a shared floor/ceiling elevation",
            ));
        }
    }
    for (i, a) in model.zones.iter().enumerate() {
        for b in &model.zones[i + 1..] {
            let ba = bounds(a.origin_m, a.size_m)?;
            let bb = bounds(b.origin_m, b.size_m)?;
            let overlap = intersection(ba, bb);
            if overlap.iter().all(|&x| x > EPS) {
                return Err(invalid(
                    "zones",
                    format!("Zones {} and {} overlap", a.id, b.id),
                ));
            }
            for axis in 0..3 {
                let remaining: Vec<_> = (0..3).filter(|&x| x != axis).collect();
                if overlap[remaining[0]] <= EPS || overlap[remaining[1]] <= EPS {
                    continue;
                }
                let high = if (ba.1[axis] - bb.0[axis]).abs() <= EPS {
                    Some(true)
                } else if (bb.1[axis] - ba.0[axis]).abs() <= EPS {
                    Some(false)
                } else {
                    None
                };
                if let Some(high) = high {
                    if remaining.iter().any(|&k| {
                        (ba.0[k] - bb.0[k]).abs() > EPS || (ba.1[k] - bb.1[k]).abs() > EPS
                    }) {
                        return Err(invalid(
                            "zones.adjacency",
                            "Partial-face adjacency is unsupported; align whole matching faces",
                        ));
                    }
                    let face = ThermalFace::ALL
                        .into_iter()
                        .find(|f| f.axis() == axis && f.high() == high)
                        .expect("face");
                    let sa = a
                        .surfaces
                        .iter()
                        .find(|s| s.face == face)
                        .expect("six faces");
                    let sb = b
                        .surfaces
                        .iter()
                        .find(|s| s.face == face.opposite())
                        .expect("six faces");
                    if !matches!(&sa.boundary,ThermalBoundary::Interzone{zone_id,surface_id} if zone_id==&b.id && surface_id==&sb.id)
                        || !matches!(&sb.boundary,ThermalBoundary::Interzone{zone_id,surface_id} if zone_id==&a.id && surface_id==&sa.id)
                    {
                        return Err(invalid(
                            "surface.boundary",
                            "Touching full faces require explicit reciprocal interzone pairing",
                        ));
                    }
                }
            }
        }
    }
    for zone in &model.zones {
        for surface in &zone.surfaces {
            if let ThermalBoundary::Interzone {
                zone_id,
                surface_id,
            } = &surface.boundary
            {
                let partner = model
                    .zones
                    .iter()
                    .find(|z| z.id == *zone_id && z.id != zone.id)
                    .ok_or_else(|| {
                        invalid("interzone.zone", "Unknown or self-referenced paired zone")
                    })?;
                let other = partner
                    .surfaces
                    .iter()
                    .find(|s| s.id == *surface_id)
                    .ok_or_else(|| invalid("interzone.surface", "Unknown paired face"))?;
                if surface.face.opposite() != other.face
                    || !matches!(&other.boundary,ThermalBoundary::Interzone{zone_id,surface_id} if zone_id==&zone.id && surface_id==&surface.id)
                {
                    return Err(invalid(
                        "interzone.pair",
                        "Expected reciprocal opposite-normal faces",
                    ));
                }
                let (av, _, aa) = face_geometry(zone, surface.face, 0.);
                let (bv, _, ba) = face_geometry(partner, other.face, 0.);
                if (aa - ba).abs() > EPS
                    || av.iter().any(|v| {
                        !bv.iter()
                            .any(|w| (0..3).all(|k| (v[k] - w[k]).abs() <= EPS))
                    })
                {
                    return Err(invalid(
                        "interzone.geometry",
                        "Paired faces must have matching vertices and area",
                    ));
                }
                let ca = model
                    .constructions
                    .iter()
                    .find(|c| c.id == surface.construction_id)
                    .expect("validated construction");
                let cb = model
                    .constructions
                    .iter()
                    .find(|c| c.id == other.construction_id)
                    .expect("validated construction");
                if !ca.layers.iter().eq(cb.layers.iter().rev()) {
                    return Err(invalid(
                        "interzone.construction",
                        "Paired layer stacks must be exact reversals",
                    ));
                }
            }
        }
    }
    for shade in &model.detached_shading {
        register(&mut ids, &shade.id)?;
        let sb = bounds(shade.origin_m, shade.size_m)?;
        for zone in &model.zones {
            if intersection(sb, bounds(zone.origin_m, zone.size_m)?)
                .iter()
                .all(|&v| v > EPS)
            {
                return Err(invalid("shading", "Detached box overlaps a thermal zone"));
            }
        }
    }
    let wwr = facades
        .into_iter()
        .zip(["north", "east", "south", "west"])
        .map(|((wall, opening), orientation)| FacadeWwr {
            orientation: orientation.into(),
            gross_wall_m2: wall.value(),
            opening_m2: opening.value(),
            ratio: if wall.value() > 0. {
                Some(opening.value() / wall.value())
            } else {
                None
            },
        })
        .collect();
    let mut identity = model.clone();
    identity.name.clear();
    for z in &mut identity.zones {
        z.name.clear();
    }
    for m in &mut identity.materials {
        m.name.clear();
    }
    for c in &mut identity.constructions {
        c.name.clear();
    }
    for g in &mut identity.glazing {
        g.name.clear();
    }
    Ok(ValidatedThermalModel {
        model: model.clone(),
        surfaces,
        gross_floor_area_m2: gross.value(),
        conditioned_floor_area_m2: conditioned.value(),
        construction_u_values_w_m2k: u_values,
        facade_wwr: wwr,
        model_input_sha256: canonical_hash(&(THERMAL_MODEL_SCHEMA, &identity))?,
    })
}
