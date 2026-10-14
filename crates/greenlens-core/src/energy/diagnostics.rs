//! Structured, engine-free workbench diagnostics. Acceptance is still decided
//! by the unchanged authoritative v0.4 validator. Localization is conservative:
//! ambiguous/global failures stay at model level rather than guessing an object.
use super::*;
use crate::CoreError;
use serde::{Deserialize, Serialize};

pub const THERMAL_PREFLIGHT_SCHEMA: &str = "greenlens.thermal-preflight.v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThermalIssueSeverity {
    Error,
    Warning,
    Information,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThermalIssueKind {
    InvalidInput,
    UnsupportedAdapter,
    MissingDependency,
    MissingEvidence,
    UnassessedScience,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ThermalIssue {
    pub code: String,
    pub kind: ThermalIssueKind,
    pub severity: ThermalIssueSeverity,
    pub object: ThermalObjectRef,
    pub related_objects: Vec<ThermalObjectRef>,
    /// JSON Pointer relative to object. Empty denotes the whole object or a
    /// global/unlocalized issue; it never guesses an array index from a message.
    pub field_path: String,
    pub message: String,
    pub remedy: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThermalPreflight {
    pub schema_version: String,
    pub valid: bool,
    pub issues: Vec<ThermalIssue>,
    pub coverage: String,
}

pub fn preflight_thermal_inputs(
    model: &ThermalModel,
    calendar: &EnergyCalendar,
) -> ThermalPreflight {
    let issues = match validate_thermal_model(model, calendar) {
        Ok(_) => vec![],
        Err(error) => vec![validation_issue(model, calendar, error)],
    };
    ThermalPreflight { schema_version:THERMAL_PREFLIGHT_SCHEMA.into(),valid:issues.is_empty(),issues,
        coverage:"Authoritative core validation; reports its first failure only. A unique safely located cause links to its object; ambiguous/global failures remain model-level. No engine, weather-file, dependency or scientific-qualification check.".into() }
}

/// Shared adapter/runner fallback for an authoritative error that has no
/// stable object location. It does not parse a human error message into an ID.
pub fn model_level_issue(
    code: &str,
    kind: ThermalIssueKind,
    message: impl Into<String>,
    remedy: impl Into<String>,
) -> ThermalIssue {
    ThermalIssue {
        code: code.into(),
        kind,
        severity: ThermalIssueSeverity::Error,
        object: ThermalObjectRef::model(),
        related_objects: vec![],
        field_path: String::new(),
        message: message.into(),
        remedy: remedy.into(),
    }
}

fn validation_issue(
    model: &ThermalModel,
    calendar: &EnergyCalendar,
    error: CoreError,
) -> ThermalIssue {
    let (field, message) = match error {
        CoreError::Validation { field, message } => (field, message),
        other => ("model".into(), other.to_string()),
    };
    let candidates = locations(model, calendar, &field);
    let (object, path) = if candidates.len() == 1 {
        candidates[0].clone()
    } else {
        (ThermalObjectRef::model(), String::new())
    };
    let related = if candidates.len() > 1 {
        candidates.into_iter().map(|(o, _)| o).collect()
    } else {
        vec![]
    };
    ThermalIssue {code:format!("core.{field}"),kind:ThermalIssueKind::InvalidInput,severity:ThermalIssueSeverity::Error,object,related_objects:related,field_path:path,message,
        remedy:"Correct the indicated accepted-input draft and apply it again. Rejected validation must not change the document or undo history.".into()}
}
fn same_field(result: crate::Result<()>, field: &str) -> bool {
    matches!(result,Err(CoreError::Validation{field:f,..}) if f==field)
}

/// Reuses primitive/schedule validators only to locate the already-reported
/// failure. This function cannot accept an input or override strict validation.
fn locations(
    m: &ThermalModel,
    calendar: &EnergyCalendar,
    f: &str,
) -> Vec<(ThermalObjectRef, String)> {
    use ThermalObjectKind as K;
    let mut out = vec![];
    let mut add = |kind, id: &str, zone: Option<&str>, path: &str| {
        out.push((ThermalObjectRef::new(kind, id, zone), path.into()))
    };
    if f.starts_with("calendar.") {
        add(
            K::Calendar,
            "calendar",
            None,
            match f {
                "calendar.utc_offset_minutes" => "/utc_offset_minutes",
                "calendar.holidays" => "/holidays",
                _ => "",
            },
        );
        return out;
    }
    for s in &m.schedules {
        if same_field(s.validate(calendar), f) {
            add(
                K::Schedule,
                &s.id,
                None,
                if f == "schedule.holidays" {
                    "/holidays"
                } else {
                    ""
                },
            );
        }
    }
    for material in &m.materials {
        for (field, path, v) in [
            ("thickness", "/thickness_m", material.thickness_m),
            (
                "conductivity",
                "/conductivity_w_mk",
                material.conductivity_w_mk,
            ),
            ("density", "/density_kg_m3", material.density_kg_m3),
            (
                "specific_heat",
                "/specific_heat_j_kgk",
                material.specific_heat_j_kgk,
            ),
        ] {
            if f == field && positive(field, v).is_err() {
                add(K::Material, &material.id, None, path);
            }
        }
        for (path, v) in [
            ("/thermal_absorptance", material.thermal_absorptance),
            ("/solar_absorptance", material.solar_absorptance),
            ("/visible_absorptance", material.visible_absorptance),
        ] {
            if f == "material.absorptance" && range(f, v, 0., 1.).is_err() {
                add(K::Material, &material.id, None, path);
            }
        }
    }
    for g in &m.glazing {
        for (field, path, result) in [
            (
                "glazing.u_value",
                "/u_value_w_m2k",
                positive("glazing.u_value", g.u_value_w_m2k),
            ),
            (
                "glazing.shgc",
                "/shgc",
                range("glazing.shgc", g.shgc, 0., 1.),
            ),
            (
                "glazing.visible_transmittance",
                "/visible_transmittance",
                range(
                    "glazing.visible_transmittance",
                    g.visible_transmittance,
                    0.,
                    1.,
                ),
            ),
        ] {
            if f == field && result.is_err() {
                add(K::Glazing, &g.id, None, path);
            }
        }
    }
    for c in &m.constructions {
        if let Err(CoreError::Validation { field, .. }) = construction_u_value(c, &m.materials) {
            if field == f {
                add(
                    K::Construction,
                    &c.id,
                    None,
                    match f {
                        "construction.layers" => "/layers",
                        "construction.inside_film" => "/derived_inside_film_m2k_w",
                        "construction.outside_film" => "/derived_outside_film_m2k_w",
                        _ => "",
                    },
                );
            }
        }
    }
    for z in &m.zones {
        for i in 0..3 {
            if f == "geometry.origin_m" && range(f, z.origin_m[i], -100_000., 100_000.).is_err() {
                add(K::Zone, &z.id, Some(&z.id), &format!("/origin_m/{i}"));
            }
            if f == "geometry.size_m"
                && (positive(f, z.size_m[i]).is_err()
                    || range(f, z.size_m[i], 0.01, 10_000.).is_err())
            {
                add(K::Zone, &z.id, Some(&z.id), &format!("/size_m/{i}"));
            }
        }
        if f == "zone.storey" && !(1..=2).contains(&z.storey) {
            add(K::Zone, &z.id, Some(&z.id), "/storey");
        }
        if f == "zone.surfaces"
            && (z.surfaces.len() != 6
                || z.surfaces
                    .iter()
                    .map(|s| s.face)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != 6)
        {
            add(K::Zone, &z.id, Some(&z.id), "/surfaces");
        }
        for s in &z.surfaces {
            if f == "surface.construction"
                && !m.constructions.iter().any(|c| c.id == s.construction_id)
            {
                add(K::Surface, &s.id, Some(&z.id), "/construction_id");
            }
            if let ThermalBoundary::Interzone {
                zone_id,
                surface_id,
            } = &s.boundary
            {
                let partner = m.zones.iter().find(|p| p.id == *zone_id && p.id != z.id);
                if f == "interzone.zone" && partner.is_none() {
                    add(K::Surface, &s.id, Some(&z.id), "/boundary/zone_id");
                }
                if f == "interzone.surface"
                    && partner.is_some_and(|p| !p.surfaces.iter().any(|v| v.id == *surface_id))
                {
                    add(K::Surface, &s.id, Some(&z.id), "/boundary/surface_id");
                }
            }
        }
        for w in &z.windows {
            if f == "window.glazing" && !m.glazing.iter().any(|g| g.id == w.glazing_id) {
                add(K::Window, &w.id, Some(&z.id), "/glazing_id");
            }
            for (field, path, v) in [
                ("window.width", "/width_m", w.width_m),
                ("window.height", "/height_m", w.height_m),
            ] {
                if f == field && positive(field, v).is_err() {
                    add(K::Window, &w.id, Some(&z.id), path);
                }
            }
            if let Some(s) = z.surfaces.iter().find(|s| s.id == w.surface_id) {
                let [u, v] = match s.face {
                    ThermalFace::West | ThermalFace::East => [1, 2],
                    ThermalFace::South | ThermalFace::North => [0, 2],
                    _ => [0, 1],
                };
                if f == "window.rectangle"
                    && (w.offset_u_m + w.width_m > z.size_m[u]
                        || w.offset_v_m + w.height_m > z.size_m[v])
                {
                    add(K::Window, &w.id, Some(&z.id), "");
                }
                if f == "window.parent"
                    && (!matches!(s.boundary, ThermalBoundary::Outdoors)
                        || s.face == ThermalFace::Floor)
                {
                    add(K::Window, &w.id, Some(&z.id), "/surface_id");
                }
                for (field, path, x, max) in [
                    ("window.offset_u", "/offset_u_m", w.offset_u_m, z.size_m[u]),
                    ("window.offset_v", "/offset_v_m", w.offset_v_m, z.size_m[v]),
                ] {
                    if f == field && range(field, x, 0., max).is_err() {
                        add(K::Window, &w.id, Some(&z.id), path);
                    }
                }
            } else if f == "window.surface_id" {
                add(K::Window, &w.id, Some(&z.id), "/surface_id");
            }
        }
        let g = &z.gains;
        for (path, value) in [
            ("/people_per_floor_m2", g.people_per_floor_m2),
            ("/lighting_w_per_floor_m2", g.lighting_w_per_floor_m2),
            ("/equipment_w_per_floor_m2", g.equipment_w_per_floor_m2),
        ] {
            if f == "gains.density" && range(f, value, 0., f64::MAX).is_err() {
                add(K::Gains, &z.id, Some(&z.id), path);
            }
        }
        for (field, path, id, kind) in [
            (
                "schedule.assignment",
                "/occupancy_schedule",
                &g.occupancy_schedule,
                ScheduleKind::Fraction,
            ),
            (
                "schedule.assignment",
                "/activity_schedule",
                &g.activity_schedule,
                ScheduleKind::ActivityWattsPerPerson,
            ),
            (
                "schedule.assignment",
                "/lighting_schedule",
                &g.lighting_schedule,
                ScheduleKind::Fraction,
            ),
            (
                "schedule.assignment",
                "/equipment_schedule",
                &g.equipment_schedule,
                ScheduleKind::Fraction,
            ),
            (
                "schedule.assignment",
                "/infiltration_schedule",
                &g.infiltration_schedule,
                ScheduleKind::Fraction,
            ),
        ] {
            if f == field && !m.schedules.iter().any(|s| s.id == *id && s.kind == kind) {
                add(K::Gains, &z.id, Some(&z.id), path);
            }
        }
        if let Some(h) = &z.cooling_system {
            for (path, id, two) in [
                (
                    "/capacity_temperature_curve",
                    &h.capacity_temperature_curve,
                    true,
                ),
                ("/capacity_flow_curve", &h.capacity_flow_curve, false),
                ("/eir_temperature_curve", &h.eir_temperature_curve, true),
                ("/eir_flow_curve", &h.eir_flow_curve, false),
                (
                    "/part_load_fraction_curve",
                    &h.part_load_fraction_curve,
                    false,
                ),
            ] {
                if f == "cooling.curve"
                    && !m.curves.iter().any(|c| {
                        c.id == *id && matches!(c.form, CurveForm::Biquadratic { .. }) == two
                    })
                {
                    add(K::CoolingSystem, &h.id, Some(&z.id), path);
                }
            }
            for (path, value) in [
                ("/gross_rated_capacity_w", h.gross_rated_capacity_w),
                ("/gross_rated_cop", h.gross_rated_cop),
                ("/rated_airflow_m3_s", h.rated_airflow_m3_s),
            ] {
                if f == "cooling.rated" && positive(f, value).is_err() {
                    add(K::CoolingSystem, &h.id, Some(&z.id), path);
                }
            }
            for (field, path, value, lo, hi) in [
                ("cooling.shr", "/rated_shr", h.rated_shr, 0., 1.),
                (
                    "cooling.indoor_wet_bulb",
                    "/rated_indoor_wet_bulb_c",
                    h.rated_indoor_wet_bulb_c,
                    -50.,
                    100.,
                ),
                (
                    "cooling.outdoor_dry_bulb",
                    "/rated_outdoor_dry_bulb_c",
                    h.rated_outdoor_dry_bulb_c,
                    -50.,
                    100.,
                ),
                (
                    "cooling.deadband",
                    "/cooling_deadband_k",
                    h.cooling_deadband_k,
                    0.,
                    20.,
                ),
            ] {
                if f == field && range(field, value, lo, hi).is_err() {
                    add(K::CoolingSystem, &h.id, Some(&z.id), path);
                }
            }
            for (path, id, kind) in [
                (
                    "/availability_schedule",
                    &h.availability_schedule,
                    ScheduleKind::Fraction,
                ),
                (
                    "/cooling_setpoint_schedule",
                    &h.cooling_setpoint_schedule,
                    ScheduleKind::TemperatureCelsius,
                ),
            ] {
                if f == "schedule.assignment"
                    && !m.schedules.iter().any(|s| s.id == *id && s.kind == kind)
                {
                    add(K::CoolingSystem, &h.id, Some(&z.id), path);
                }
            }
            for (field, path, value, positive_required, hi) in [
                (
                    "fan.pressure_rise",
                    "/pressure_rise_pa",
                    h.supply_fan.pressure_rise_pa,
                    false,
                    f64::MAX,
                ),
                (
                    "fan.total_efficiency",
                    "/total_efficiency",
                    h.supply_fan.total_efficiency,
                    true,
                    1.,
                ),
                (
                    "fan.motor_efficiency",
                    "/motor_efficiency",
                    h.supply_fan.motor_efficiency,
                    true,
                    1.,
                ),
                (
                    "fan.motor_in_air_fraction",
                    "/motor_in_air_fraction",
                    h.supply_fan.motor_in_air_fraction,
                    false,
                    1.,
                ),
            ] {
                if f == field
                    && (range(field, value, 0., hi).is_err()
                        || (positive_required && positive(field, value).is_err()))
                {
                    add(K::SupplyFan, &h.id, Some(&z.id), path);
                }
            }
            let v = &h.balanced_ventilation;
            for (field, path, value, positive_required, hi) in [
                (
                    "ventilation.flow",
                    "/design_flow_m3_s",
                    v.design_flow_m3_s,
                    false,
                    f64::MAX,
                ),
                (
                    "ventilation.pressure",
                    "/fan_pressure_pa",
                    v.fan_pressure_pa,
                    false,
                    f64::MAX,
                ),
                (
                    "ventilation.efficiency",
                    "/total_efficiency",
                    v.total_efficiency,
                    true,
                    1.,
                ),
            ] {
                if f == field
                    && (range(field, value, 0., hi).is_err()
                        || (positive_required && positive(field, value).is_err()))
                {
                    add(K::BalancedVentilation, &h.id, Some(&z.id), path);
                }
            }
            if f == "schedule.assignment"
                && !m
                    .schedules
                    .iter()
                    .any(|s| s.id == v.fraction_schedule && s.kind == ScheduleKind::Fraction)
            {
                add(
                    K::BalancedVentilation,
                    &h.id,
                    Some(&z.id),
                    "/fraction_schedule",
                );
            }
        }
    }
    for s in &m.detached_shading {
        for i in 0..3 {
            if f == "geometry.origin_m" && range(f, s.origin_m[i], -100_000., 100_000.).is_err() {
                add(K::DetachedShade, &s.id, None, &format!("/origin_m/{i}"));
            }
            if f == "geometry.size_m"
                && (positive(f, s.size_m[i]).is_err()
                    || range(f, s.size_m[i], 0.01, 10_000.).is_err())
            {
                add(K::DetachedShade, &s.id, None, &format!("/size_m/{i}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn core_preflight_links_real_failures_and_never_accepts_a_rejected_model() {
        let mut m = synthetic_four_zone_model(false);
        let calendar = synthetic_energy_calendar();
        let before = crate::canonical_hash(&m).unwrap();
        assert!(preflight_thermal_inputs(&m, &calendar).valid);
        assert_eq!(before, crate::canonical_hash(&m).unwrap());
        let id = m.zones[2].cooling_system.as_ref().unwrap().id.clone();
        m.zones[2]
            .cooling_system
            .as_mut()
            .unwrap()
            .supply_fan
            .total_efficiency = 0.;
        assert!(validate_thermal_model(&m, &calendar).is_err());
        let p = preflight_thermal_inputs(&m, &calendar);
        assert!(!p.valid);
        assert_eq!(p.issues.len(), 1);
        assert_eq!(p.issues[0].object.id, id);
        assert_eq!(p.issues[0].object.kind, ThermalObjectKind::SupplyFan);
        assert_eq!(p.issues[0].field_path, "/total_efficiency");
        m.zones[1]
            .cooling_system
            .as_mut()
            .unwrap()
            .supply_fan
            .total_efficiency = 0.;
        let p = preflight_thermal_inputs(&m, &calendar);
        assert_eq!(p.issues[0].object, ThermalObjectRef::model());
        assert_eq!(p.issues[0].related_objects.len(), 2);
    }
    #[test]
    fn window_and_schedule_failures_link_without_guessing_from_message_text() {
        let mut m = synthetic_four_zone_model(false);
        let c = synthetic_energy_calendar();
        let window = m.zones[0].windows[0].id.clone();
        m.zones[0].windows[0].width_m = 100.;
        let p = preflight_thermal_inputs(&m, &c);
        assert!(!p.valid);
        assert_eq!(p.issues[0].object.id, window);
        let mut m = synthetic_four_zone_model(false);
        m.schedules[0].weekday.values.pop();
        let p = preflight_thermal_inputs(&m, &c);
        assert!(!p.valid);
        assert_eq!(p.issues[0].object.kind, ThermalObjectKind::Schedule);
        assert!(p.coverage.contains("first failure"));
    }
}
