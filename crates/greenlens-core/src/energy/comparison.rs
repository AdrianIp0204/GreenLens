//! Additive workbench interpretation. The v0.4 comparison and report codecs
//! deliberately retain their original semantics and bytes.
use super::*;
use crate::{canonical_hash, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const THERMAL_DIFFERENCES_SCHEMA: &str = "greenlens.thermal-input-differences.v1";
pub const THERMAL_COMPARISON_POLICY: &str = "greenlens.thermal-service-comparison/1";
/// Presentation bound only. Every difference still contributes to the count
/// and service/compatibility flags, even when its detailed row is omitted.
pub const MAX_THERMAL_DIFFERENCE_ROWS: usize = 4096;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ThermalObjectKind {
    Model,
    Site,
    Study,
    Calendar,
    Zone,
    Surface,
    Window,
    Overhang,
    DetachedShade,
    Material,
    Construction,
    Glazing,
    Schedule,
    Curve,
    Gains,
    CoolingSystem,
    SupplyFan,
    BalancedVentilation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct ThermalObjectRef {
    pub kind: ThermalObjectKind,
    /// A nested fan/ventilation object uses its owning system ID; an overhang
    /// uses its window ID. Kind + ID + zone ID form the semantic reference.
    pub id: String,
    pub zone_id: Option<String>,
}
impl ThermalObjectRef {
    pub fn model() -> Self {
        Self {
            kind: ThermalObjectKind::Model,
            id: "model".into(),
            zone_id: None,
        }
    }
    pub fn new(kind: ThermalObjectKind, id: &str, zone_id: Option<&str>) -> Self {
        Self {
            kind,
            id: id.into(),
            zone_id: zone_id.map(str::to_owned),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ThermalInputValue {
    Missing,
    Null,
    Boolean(bool),
    Number(f64),
    Text(String),
    ObjectPresent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThermalDifferenceCategory {
    Geometry,
    WeatherCalendar,
    Envelope,
    EquipmentPerformance,
    Gains,
    Schedule,
    Assignment,
    Service,
    Provenance,
    Metadata,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThermalServiceEffect {
    None,
    Changed,
    IncompatibleComparison,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThermalInputDifference {
    pub object: ThermalObjectRef,
    /// JSON Pointer relative to the referenced object; empty means presence.
    /// The numeric array indices are exact ten-minute slots/curve coefficients.
    pub field_path: String,
    pub category: ThermalDifferenceCategory,
    pub before: ThermalInputValue,
    pub after: ThermalInputValue,
    pub unit: Option<String>,
    pub service_effect: ThermalServiceEffect,
    pub message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ThermalInputDifferences {
    pub schema_version: String,
    pub policy_id: String,
    pub baseline_model_input_sha256: String,
    pub proposed_model_input_sha256: String,
    pub baseline_study_sha256: String,
    pub proposed_study_sha256: String,
    pub comparable_geometry_weather_calendar: bool,
    pub unchanged_service_inputs: bool,
    pub total_changes: usize,
    pub truncated: bool,
    pub differences: Vec<ThermalInputDifference>,
    pub limitations: Vec<String>,
}

struct Collector<'a> {
    a: &'a ThermalModel,
    b: &'a ThermalModel,
    rows: Vec<ThermalInputDifference>,
    count: usize,
    compatible: bool,
    same_service: bool,
}

/// This is a new derived policy, never a replacement for compare_energy_inputs.
/// Physical/model/study/method/recipe hashes are not changed by this function.
pub fn compare_thermal_inputs_v1(
    a: &ThermalModel,
    sa: &EnergyStudy,
    b: &ThermalModel,
    sb: &EnergyStudy,
) -> Result<ThermalInputDifferences> {
    sa.validate(a)?;
    sb.validate(b)?;
    let mut c = Collector {
        a,
        b,
        rows: Vec::new(),
        count: 0,
        compatible: true,
        same_service: true,
    };
    let root = ThermalObjectRef::model();
    c.object_fields(
        &root,
        &serde_json::to_value(a)?,
        &serde_json::to_value(b)?,
        &[
            "site",
            "zones",
            "materials",
            "constructions",
            "glazing",
            "curves",
            "schedules",
            "detached_shading",
        ],
    );
    c.walk(
        &ThermalObjectRef::new(ThermalObjectKind::Site, "site", None),
        "",
        Some(&serde_json::to_value(&a.site)?),
        Some(&serde_json::to_value(&b.site)?),
    );
    for (name, kind) in [
        ("materials", ThermalObjectKind::Material),
        ("constructions", ThermalObjectKind::Construction),
        ("glazing", ThermalObjectKind::Glazing),
        ("curves", ThermalObjectKind::Curve),
        ("schedules", ThermalObjectKind::Schedule),
        ("detached_shading", ThermalObjectKind::DetachedShade),
    ] {
        let av = serde_json::to_value(a)?;
        let bv = serde_json::to_value(b)?;
        c.collection(kind, None, &av[name], &bv[name]);
    }
    let az = by_id(&serde_json::to_value(&a.zones)?);
    let bz = by_id(&serde_json::to_value(&b.zones)?);
    for id in keys(&az, &bz) {
        let object = ThermalObjectRef::new(ThermalObjectKind::Zone, &id, Some(&id));
        match (az.get(&id), bz.get(&id)) {
            (Some(x), Some(y)) => {
                c.object_fields(
                    &object,
                    x,
                    y,
                    &["surfaces", "windows", "gains", "cooling_system"],
                );
                c.collection(
                    ThermalObjectKind::Surface,
                    Some(&id),
                    &x["surfaces"],
                    &y["surfaces"],
                );
                let aw = by_id(&x["windows"]);
                let bw = by_id(&y["windows"]);
                for wid in keys(&aw, &bw) {
                    let w = ThermalObjectRef::new(ThermalObjectKind::Window, &wid, Some(&id));
                    match (aw.get(&wid), bw.get(&wid)) {
                        (Some(v), Some(z)) => {
                            c.object_fields(&w, v, z, &["overhang"]);
                            c.walk(
                                &ThermalObjectRef::new(
                                    ThermalObjectKind::Overhang,
                                    &wid,
                                    Some(&id),
                                ),
                                "",
                                v.get("overhang"),
                                z.get("overhang"),
                            );
                        }
                        (v, z) => c.walk(&w, "", v, z),
                    }
                }
                c.walk(
                    &ThermalObjectRef::new(ThermalObjectKind::Gains, &id, Some(&id)),
                    "",
                    x.get("gains"),
                    y.get("gains"),
                );
                let ah = x.get("cooling_system").filter(|v| !v.is_null());
                let bh = y.get("cooling_system").filter(|v| !v.is_null());
                let sid = ah.or(bh).and_then(|v| v["id"].as_str()).unwrap_or(&id);
                let h = ThermalObjectRef::new(ThermalObjectKind::CoolingSystem, sid, Some(&id));
                match (ah, bh) {
                    (Some(v), Some(z)) => {
                        c.object_fields(&h, v, z, &["supply_fan", "balanced_ventilation"]);
                        c.walk(
                            &ThermalObjectRef::new(ThermalObjectKind::SupplyFan, sid, Some(&id)),
                            "",
                            v.get("supply_fan"),
                            z.get("supply_fan"),
                        );
                        c.walk(
                            &ThermalObjectRef::new(
                                ThermalObjectKind::BalancedVentilation,
                                sid,
                                Some(&id),
                            ),
                            "",
                            v.get("balanced_ventilation"),
                            z.get("balanced_ventilation"),
                        );
                    }
                    (v, z) => c.walk(&h, "", v, z),
                }
            }
            (x, y) => c.walk(&object, "", x, y),
        }
    }
    c.walk(
        &ThermalObjectRef::new(ThermalObjectKind::Study, "study", None),
        "",
        Some(&serde_json::to_value(sa)?),
        Some(&serde_json::to_value(sb)?),
    );
    Ok(ThermalInputDifferences {
        schema_version: THERMAL_DIFFERENCES_SCHEMA.into(), policy_id: THERMAL_COMPARISON_POLICY.into(),
        baseline_model_input_sha256: validate_thermal_model(a,&sa.calendar)?.model_input_sha256,
        proposed_model_input_sha256: validate_thermal_model(b,&sb.calendar)?.model_input_sha256,
        baseline_study_sha256: canonical_hash(sa)?, proposed_study_sha256: canonical_hash(sb)?,
        comparable_geometry_weather_calendar: c.compatible, unchanged_service_inputs: c.same_service,
        total_changes:c.count, truncated:c.count>c.rows.len(), differences:c.rows,
        limitations:vec!["Unchanged service inputs do not establish adequate conditioning or equal lighting service; retained comfort and explicit lighting evidence remain necessary.".into(),"Typed changes describe inputs, not causal attribution of energy savings or scientific qualification. Collection members are matched by stable ID; their retained input hashes preserve original ordering.".into()],
    })
}

fn by_id(v: &Value) -> BTreeMap<String, Value> {
    v.as_array()
        .into_iter()
        .flatten()
        .map(|x| {
            (
                x["id"].as_str().expect("validated stable ID").to_owned(),
                x.clone(),
            )
        })
        .collect()
}
fn keys(a: &BTreeMap<String, Value>, b: &BTreeMap<String, Value>) -> BTreeSet<String> {
    a.keys().chain(b.keys()).cloned().collect()
}
fn pointer(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}
fn scalar(v: Option<&Value>) -> ThermalInputValue {
    match v {
        None => ThermalInputValue::Missing,
        Some(Value::Null) => ThermalInputValue::Null,
        Some(Value::Bool(v)) => ThermalInputValue::Boolean(*v),
        Some(Value::Number(v)) => {
            ThermalInputValue::Number(v.as_f64().expect("validated finite input"))
        }
        Some(Value::String(v)) => ThermalInputValue::Text(v.clone()),
        _ => ThermalInputValue::ObjectPresent,
    }
}
impl Collector<'_> {
    fn collection(&mut self, kind: ThermalObjectKind, zone: Option<&str>, a: &Value, b: &Value) {
        let a = by_id(a);
        let b = by_id(b);
        for id in keys(&a, &b) {
            self.walk(
                &ThermalObjectRef::new(kind, &id, zone),
                "",
                a.get(&id),
                b.get(&id),
            );
        }
    }
    fn object_fields(&mut self, object: &ThermalObjectRef, a: &Value, b: &Value, skip: &[&str]) {
        let keys: BTreeSet<_> = a
            .as_object()
            .into_iter()
            .flat_map(|m| m.keys())
            .chain(b.as_object().into_iter().flat_map(|m| m.keys()))
            .collect();
        for key in keys {
            if !skip.contains(&key.as_str()) {
                self.walk(
                    object,
                    &format!("/{}", pointer(key)),
                    a.get(key),
                    b.get(key),
                );
            }
        }
    }
    fn walk(
        &mut self,
        object: &ThermalObjectRef,
        path: &str,
        a: Option<&Value>,
        b: Option<&Value>,
    ) {
        if a == b {
            return;
        }
        match (a, b) {
            (Some(Value::Object(x)), Some(Value::Object(y))) => {
                let keys: BTreeSet<_> = x.keys().chain(y.keys()).collect();
                for key in keys {
                    self.walk(
                        object,
                        &format!("{path}/{}", pointer(key)),
                        x.get(key),
                        y.get(key),
                    );
                }
            }
            (Some(Value::Array(x)), Some(Value::Array(y))) => {
                for i in 0..x.len().max(y.len()) {
                    self.walk(object, &format!("{path}/{i}"), x.get(i), y.get(i));
                }
            }
            (Some(Value::Object(x)), None | Some(Value::Null)) => {
                self.emit(object, path, a, b);
                for (key, value) in x {
                    self.walk(
                        object,
                        &format!("{path}/{}", pointer(key)),
                        Some(value),
                        None,
                    );
                }
            }
            (None | Some(Value::Null), Some(Value::Object(y))) => {
                self.emit(object, path, a, b);
                for (key, value) in y {
                    self.walk(
                        object,
                        &format!("{path}/{}", pointer(key)),
                        None,
                        Some(value),
                    );
                }
            }
            (Some(Value::Array(x)), None | Some(Value::Null)) => {
                for (i, value) in x.iter().enumerate() {
                    self.walk(object, &format!("{path}/{i}"), Some(value), None);
                }
            }
            (None | Some(Value::Null), Some(Value::Array(y))) => {
                for (i, value) in y.iter().enumerate() {
                    self.walk(object, &format!("{path}/{i}"), None, Some(value));
                }
            }
            _ => self.emit(object, path, a, b),
        }
    }
    fn emit(
        &mut self,
        object: &ThermalObjectRef,
        path: &str,
        a: Option<&Value>,
        b: Option<&Value>,
    ) {
        let (category, effect) = classify(object, path, self.a, self.b);
        self.count += 1;
        self.compatible &= effect != ThermalServiceEffect::IncompatibleComparison;
        self.same_service &= effect == ThermalServiceEffect::None;
        if self.rows.len() < MAX_THERMAL_DIFFERENCE_ROWS {
            self.rows.push(ThermalInputDifference {object:object.clone(),field_path:path.into(),category,before:scalar(a),after:scalar(b),unit:unit(object,path,self.a,self.b),service_effect:effect,message:match effect {
                        ThermalServiceEffect::Changed=>"Service input differs; do not attribute savings solely to efficiency without reviewing service equivalence.",
                        ThermalServiceEffect::IncompatibleComparison=>"Comparison geometry, weather or time basis differs; no like-for-like savings claim.",
                        ThermalServiceEffect::None=>"Declared input differs; this alone does not establish the cause of an output change.",
                    }.into()});
        }
    }
}
fn schedule_service(model: &ThermalModel, id: &str) -> bool {
    model.exterior_lighting_schedule == id
        || model.zones.iter().any(|z| {
            [
                &z.gains.occupancy_schedule,
                &z.gains.activity_schedule,
                &z.gains.lighting_schedule,
                &z.gains.equipment_schedule,
            ]
            .into_iter()
            .any(|s| s == id)
                || z.cooling_system.as_ref().is_some_and(|h| {
                    [
                        &h.availability_schedule,
                        &h.cooling_setpoint_schedule,
                        &h.balanced_ventilation.fraction_schedule,
                    ]
                    .into_iter()
                    .any(|s| s == id)
                })
        })
}
fn classify(
    o: &ThermalObjectRef,
    p: &str,
    a: &ThermalModel,
    b: &ThermalModel,
) -> (ThermalDifferenceCategory, ThermalServiceEffect) {
    use ThermalDifferenceCategory as C;
    use ThermalObjectKind as O;
    use ThermalServiceEffect as E;
    if p.contains("/source/") || p.contains("/sizing_source/") {
        return (C::Provenance, E::None);
    }
    if matches!(p, "/name" | "/id" | "/schema_version") {
        return (C::Metadata, E::None);
    }
    match o.kind {
        O::Site | O::Study | O::Calendar => (C::WeatherCalendar, E::IncompatibleComparison),
        O::Zone => (C::Geometry, E::IncompatibleComparison),
        O::SupplyFan => (C::EquipmentPerformance, E::None),
        O::BalancedVentilation => {
            if matches!(p, "" | "/design_flow_m3_s" | "/fraction_schedule") {
                (C::Service, E::Changed)
            } else {
                (C::EquipmentPerformance, E::None)
            }
        }
        O::CoolingSystem => {
            if matches!(
                p,
                "" | "/rated_airflow_m3_s"
                    | "/availability_schedule"
                    | "/cooling_setpoint_schedule"
                    | "/cooling_deadband_k"
            ) {
                (C::Service, E::Changed)
            } else {
                (C::EquipmentPerformance, E::None)
            }
        }
        O::Gains => {
            if matches!(
                p,
                "" | "/people_per_floor_m2"
                    | "/occupancy_schedule"
                    | "/activity_schedule"
                    | "/lighting_schedule"
                    | "/equipment_schedule"
            ) {
                (C::Service, E::Changed)
            } else {
                (C::Gains, E::None)
            }
        }
        O::Schedule => (
            C::Schedule,
            if schedule_service(a, &o.id) || schedule_service(b, &o.id) {
                E::Changed
            } else {
                E::None
            },
        ),
        O::Curve => (C::EquipmentPerformance, E::None),
        O::Surface => {
            if p == "/construction_id" {
                (C::Assignment, E::None)
            } else {
                (C::Envelope, E::None)
            }
        }
        O::Window => {
            if matches!(p, "/surface_id" | "/glazing_id") {
                (C::Assignment, E::None)
            } else {
                (C::Envelope, E::None)
            }
        }
        O::Overhang | O::DetachedShade | O::Material | O::Construction | O::Glazing => {
            (C::Envelope, E::None)
        }
        O::Model => {
            if p == "/exterior_lighting_schedule" {
                (C::Service, E::Changed)
            } else if p == "/north_rotation_deg" {
                (C::Envelope, E::None)
            } else {
                (C::Gains, E::None)
            }
        }
    }
}
fn unit(o: &ThermalObjectRef, p: &str, a: &ThermalModel, b: &ThermalModel) -> Option<String> {
    let field = p.rsplit('/').next().unwrap_or("");
    let value = if p.contains("/origin_m/") || p.contains("/size_m/") || field.ends_with("_m") {
        "m"
    } else if field.ends_with("_m3_s") {
        "m3/s"
    } else if field.ends_with("_pa") {
        "Pa"
    } else if field.ends_with("_w_m2k") {
        "W/(m2 K)"
    } else if field.ends_with("_m2k_w") {
        "m2 K/W"
    } else if field.ends_with("_w_mk") {
        "W/(m K)"
    } else if field.ends_with("_kg_m3") {
        "kg/m3"
    } else if field.ends_with("_j_kgk") {
        "J/(kg K)"
    } else if field.ends_with("_w_per_floor_m2") {
        "W/m2"
    } else if field.ends_with("_per_floor_m2") {
        "people/m2"
    } else if field.ends_with("_w") {
        "W"
    } else if field.ends_with("_c") {
        "C"
    } else if field.ends_with("_k") {
        "K"
    } else if field.contains("deg") {
        "degree"
    } else if field == "utc_offset_minutes" {
        "minute"
    } else if field == "zone_timesteps_per_hour" {
        "1/h"
    } else if o.kind == ThermalObjectKind::Gains && p == "/infiltration/value" {
        let basis = |m: &ThermalModel| {
            m.zones
                .iter()
                .find(|z| Some(&z.id) == o.zone_id.as_ref())
                .map(|z| match z.gains.infiltration {
                    InfiltrationRate::AirChangesPerHour { .. } => "1/h",
                    InfiltrationRate::FlowM3PerSecond { .. } => "m3/s",
                    InfiltrationRate::FlowM3PerSecondPerFloorM2 { .. } => "m3/(s m2)",
                })
        };
        match (basis(a), basis(b)) {
            (Some(x), Some(y)) if x == y => x,
            (Some(x), None) | (None, Some(x)) => x,
            _ => return None,
        }
    } else if o.kind == ThermalObjectKind::Curve {
        let two = a
            .curves
            .iter()
            .chain(&b.curves)
            .find(|c| c.id == o.id)
            .is_some_and(|c| matches!(c.form, CurveForm::Biquadratic { .. }));
        if two && (p.starts_with("/form/x/") || p.starts_with("/form/y/")) {
            "C"
        } else if two && p.starts_with("/form/coefficients/") {
            match field {
                "1" | "3" => "1/C",
                "2" | "4" | "5" => "1/C2",
                _ => "1",
            }
        } else if p.contains("/output_limits/") || p.contains("/coefficients/") || p.contains("/x/")
        {
            "1"
        } else {
            return None;
        }
    } else if field.contains("efficiency")
        || field.contains("fraction")
        || matches!(
            field,
            "shgc"
                | "visible_transmittance"
                | "rated_shr"
                | "gross_rated_cop"
                | "thermal_absorptance"
                | "solar_absorptance"
                | "visible_absorptance"
        )
    {
        "1"
    } else if o.kind == ThermalObjectKind::Schedule && p.contains("/values/") {
        match a
            .schedules
            .iter()
            .chain(&b.schedules)
            .find(|s| s.id == o.id)
            .map(|s| s.kind)
        {
            Some(ScheduleKind::TemperatureCelsius) => "C",
            Some(ScheduleKind::ActivityWattsPerPerson) => "W/person",
            _ => "1",
        }
    } else {
        return None;
    };
    Some(value.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (ThermalModel, EnergyStudy) {
        let r = synthetic_energy_report().unwrap();
        (r.baseline.model, r.baseline.study)
    }
    #[test]
    fn supply_and_ventilation_efficiency_pressure_are_design_not_service() {
        let (a, s) = fixture();
        let mut b = a.clone();
        let system = b.zones[0].cooling_system.as_mut().unwrap();
        system.supply_fan.total_efficiency = 0.6;
        system.supply_fan.pressure_rise_pa = 90.;
        // The fixture already uses 0.6 for ventilation; change it explicitly.
        system.balanced_ventilation.total_efficiency = 0.7;
        system.balanced_ventilation.fan_pressure_pa = 90.;
        let old = compare_energy_inputs(&a, &s, &b, &s).unwrap();
        assert!(
            !old.unchanged_service_inputs,
            "frozen v0.4 classifies whole ventilation object"
        );
        let new = compare_thermal_inputs_v1(&a, &s, &b, &s).unwrap();
        assert_eq!(new.total_changes, 4);
        assert!(new.unchanged_service_inputs);
        assert!(new.differences.iter().all(|d| d.category
            == ThermalDifferenceCategory::EquipmentPerformance
            && d.service_effect == ThermalServiceEffect::None));
        let row = new
            .differences
            .iter()
            .find(|d| {
                d.object.kind == ThermalObjectKind::SupplyFan && d.field_path == "/total_efficiency"
            })
            .unwrap();
        assert_eq!(row.before, ThermalInputValue::Number(0.5));
        assert_eq!(row.after, ThermalInputValue::Number(0.6));
        assert_eq!(row.unit.as_deref(), Some("1"));
        assert_eq!(row.object.zone_id.as_deref(), Some(a.zones[0].id.as_str()));
        assert_eq!(old, compare_energy_inputs(&a, &s, &b, &s).unwrap());
    }
    #[test]
    fn flow_availability_occupancy_and_setpoint_are_service_inputs() {
        let (a, s) = fixture();
        for change in 0..5 {
            let mut b = a.clone();
            let h = b.zones[0].cooling_system.as_mut().unwrap();
            match change {
                0 => h.rated_airflow_m3_s *= 0.9,
                1 => h.balanced_ventilation.design_flow_m3_s *= 0.9,
                2 => {
                    let id = h.availability_schedule.clone();
                    b.schedules
                        .iter_mut()
                        .find(|x| x.id == id)
                        .unwrap()
                        .weekday
                        .values[0] = 0.25;
                }
                3 => {
                    b.zones[0].gains.people_per_floor_m2 *= 0.9;
                }
                _ => {
                    let id = h.cooling_setpoint_schedule.clone();
                    b.schedules
                        .iter_mut()
                        .find(|x| x.id == id)
                        .unwrap()
                        .weekday
                        .values[0] += 1.;
                }
            }
            let d = compare_thermal_inputs_v1(&a, &s, &b, &s).unwrap();
            assert!(!d.unchanged_service_inputs, "change {change}");
            assert!(d.comparable_geometry_weather_calendar);
            assert!(d
                .differences
                .iter()
                .any(|x| x.service_effect == ThermalServiceEffect::Changed));
        }
    }
    #[test]
    fn differences_cover_objects_fields_units_and_nonservice_categories() {
        let (a, s) = fixture();
        let mut b = a.clone();
        b.name = "Presentation name only".into();
        b.materials[0].thickness_m += 0.01;
        b.materials[0].source.citation = "New explicit citation".into();
        b.zones[0].gains.lighting_w_per_floor_m2 += 1.;
        b.zones[0].windows[0].width_m -= 0.1;
        let mut glazing = b.glazing[0].clone();
        glazing.id = "second_glazing".into();
        b.glazing.push(glazing);
        b.zones[0].windows[0].glazing_id = "second_glazing".into();
        match &mut b.curves[0].form {
            CurveForm::Biquadratic { coefficients, .. } => coefficients[0] += 0.1,
            _ => panic!("fixture temperature curve"),
        };
        let d = compare_thermal_inputs_v1(&a, &s, &b, &s).unwrap();
        assert!(d.unchanged_service_inputs);
        for category in [
            ThermalDifferenceCategory::Metadata,
            ThermalDifferenceCategory::Provenance,
            ThermalDifferenceCategory::Envelope,
            ThermalDifferenceCategory::Gains,
            ThermalDifferenceCategory::EquipmentPerformance,
            ThermalDifferenceCategory::Assignment,
        ] {
            assert!(
                d.differences.iter().any(|x| x.category == category),
                "{category:?}"
            );
        }
        assert!(d
            .differences
            .iter()
            .any(|x| x.object.kind == ThermalObjectKind::Window
                && x.field_path == "/width_m"
                && x.unit.as_deref() == Some("m")));
        assert!(d
            .differences
            .iter()
            .any(|x| x.object.kind == ThermalObjectKind::Gains
                && x.field_path == "/lighting_w_per_floor_m2"
                && x.unit.as_deref() == Some("W/m2")));
    }
    #[test]
    fn geometry_weather_calendar_scope_and_presence_are_explicit() {
        let (a, s) = fixture();
        let mut b = a.clone();
        for z in &mut b.zones {
            z.origin_m[0] += 1.;
        }
        for z in &mut b.detached_shading {
            z.origin_m[0] += 1.;
        }
        let d = compare_thermal_inputs_v1(&a, &s, &b, &s).unwrap();
        assert!(!d.comparable_geometry_weather_calendar);
        assert!(!d.unchanged_service_inputs);
        let mut sb = s.clone();
        sb.calendar.start_weekday = Weekday::Tuesday;
        let d = compare_thermal_inputs_v1(&a, &s, &a, &sb).unwrap();
        assert!(!d.comparable_geometry_weather_calendar);
        assert_eq!(d.differences[0].field_path, "/calendar/start_weekday");
        let mut b = a.clone();
        b.zones[0].cooling_system = None;
        let d = compare_thermal_inputs_v1(&a, &s, &b, &s).unwrap();
        assert!(!d.unchanged_service_inputs);
        assert!(d
            .differences
            .iter()
            .any(|x| x.object.kind == ThermalObjectKind::CoolingSystem
                && x.field_path.is_empty()
                && x.after == ThermalInputValue::Missing));
    }
    #[test]
    fn row_bound_does_not_hide_service_failure_and_order_is_deterministic() {
        let (a, s) = fixture();
        let mut b = a.clone();
        for i in 0..12 {
            let mut schedule = b.schedules[0].clone();
            schedule.id = format!("extra_{i}");
            b.schedules.push(schedule);
        }
        b.zones[0]
            .cooling_system
            .as_mut()
            .unwrap()
            .rated_airflow_m3_s *= 0.9;
        let d = compare_thermal_inputs_v1(&a, &s, &b, &s).unwrap();
        assert!(d.total_changes > MAX_THERMAL_DIFFERENCE_ROWS);
        assert!(d.truncated);
        assert_eq!(d.differences.len(), MAX_THERMAL_DIFFERENCE_ROWS);
        assert!(!d.unchanged_service_inputs);
        assert_eq!(d, compare_thermal_inputs_v1(&a, &s, &b, &s).unwrap());
    }
    #[test]
    fn old_report_and_physical_identities_are_unchanged_by_new_interpretation() {
        let report = synthetic_energy_report().unwrap();
        let bytes = serde_json::to_vec(&report).unwrap();
        let hash = report.dataset_sha256().unwrap();
        let physical = report
            .baseline
            .study
            .physical_input_hash(&report.baseline.model)
            .unwrap();
        compare_thermal_inputs_v1(
            &report.baseline.model,
            &report.baseline.study,
            &report.proposed.model,
            &report.proposed.study,
        )
        .unwrap();
        report.validate().unwrap();
        assert_eq!(bytes, serde_json::to_vec(&report).unwrap());
        assert_eq!(hash, report.dataset_sha256().unwrap());
        assert_eq!(
            physical,
            report
                .baseline
                .study
                .physical_input_hash(&report.baseline.model)
                .unwrap()
        );
    }
}
