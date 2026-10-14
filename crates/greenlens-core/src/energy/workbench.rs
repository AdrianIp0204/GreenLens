//! Bounded, atomic authoring operations. A preview is a candidate, never an
//! accepted model. Stable IDs and complete reciprocal boundaries are built here.
use super::*;
use crate::{canonical_hash, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const THERMAL_TRANSACTION_SCHEMA: &str = "greenlens.thermal-transaction.v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZoneBoxInput {
    pub id: String,
    pub name: String,
    pub storey: u8,
    pub origin_m: [f64; 3],
    pub size_m: [f64; 3],
}

/// Every newly paired or newly exposed face uses these explicit assignments.
/// Existing unchanged reciprocal pairs retain their original construction.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopologyAssignments {
    pub exterior_construction_id: String,
    pub ground_construction_id: String,
    pub interzone_construction_id: String,
    pub interzone_reverse_construction_id: String,
    pub insulated_ground_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum ThermalOperation {
    SetZoneBox {
        zone: ZoneBoxInput,
    },
    /// Copies gains, assignments and HVAC from an explicit source. Openings are
    /// not duplicated: a new exterior window is a separate visible operation.
    AddZone {
        source_zone_id: String,
        zone: ZoneBoxInput,
    },
    AddAdjacent {
        source_zone_id: String,
        id: String,
        name: String,
        face: ThermalFace,
        depth_m: f64,
    },
    RemoveZone {
        zone_id: String,
    },
    PutWindow {
        zone_id: String,
        window: ThermalWindow,
    },
    RemoveWindow {
        zone_id: String,
        window_id: String,
    },
    PutShade {
        shade: ThermalBoxShade,
    },
    RemoveShade {
        shade_id: String,
    },
    AssignSurface {
        zone_id: String,
        surface_id: String,
        construction_id: String,
        boundary: ThermalBoundary,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThermalTransaction {
    pub schema_version: String,
    pub expected_model_sha256: String,
    pub operation: ThermalOperation,
    pub topology: TopologyAssignments,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalTransactionPreview {
    pub schema_version: String,
    pub base_model_sha256: String,
    pub candidate_model_sha256: Option<String>,
    pub candidate: ThermalModel,
    pub affected_object_ids: Vec<String>,
    pub valid: bool,
    pub problems: Vec<String>,
    pub assumptions: Vec<String>,
}

fn zone_mut<'a>(m: &'a mut ThermalModel, id: &str) -> Result<&'a mut ThermalZone> {
    m.zones
        .iter_mut()
        .find(|z| z.id == id)
        .ok_or_else(|| invalid("zone.id", format!("Unknown zone {id}")))
}
fn axis(face: ThermalFace) -> usize {
    match face {
        ThermalFace::West | ThermalFace::East => 0,
        ThermalFace::South | ThermalFace::North => 1,
        _ => 2,
    }
}
fn is_high(face: ThermalFace) -> bool {
    matches!(
        face,
        ThermalFace::East | ThermalFace::North | ThermalFace::Roof
    )
}
fn face_id(id: &str, face: ThermalFace) -> String {
    format!("{id}_{face:?}")
}
fn exposed(face: ThermalFace, storey: u8, a: &TopologyAssignments) -> (String, ThermalBoundary) {
    if face == ThermalFace::Floor && storey == 1 {
        (
            a.ground_construction_id.clone(),
            ThermalBoundary::InsulatedGround {
                reason: a.insulated_ground_reason.clone(),
            },
        )
    } else {
        (
            a.exterior_construction_id.clone(),
            ThermalBoundary::Outdoors,
        )
    }
}
fn clone_zone(
    m: &ThermalModel,
    source: &str,
    spec: &ZoneBoxInput,
    a: &TopologyAssignments,
) -> Result<ThermalZone> {
    identifier("zone.id", &spec.id)?;
    if m.zones.iter().any(|z| z.id == spec.id) {
        return Err(invalid("zone.id", "Choose a new stable zone ID"));
    }
    let mut z = m
        .zones
        .iter()
        .find(|z| z.id == source)
        .cloned()
        .ok_or_else(|| {
            invalid(
                "zone.source",
                "Select an existing source zone for explicit input copying",
            )
        })?;
    z.id = spec.id.clone();
    z.name = spec.name.clone();
    z.storey = spec.storey;
    z.origin_m = spec.origin_m;
    z.size_m = spec.size_m;
    z.windows.clear();
    if let Some(h) = &mut z.cooling_system {
        h.id = format!("{}_cooling", z.id);
    }
    z.surfaces = ThermalFace::ALL
        .into_iter()
        .map(|face| {
            let (construction_id, boundary) = exposed(face, z.storey, a);
            ThermalSurface {
                id: face_id(&z.id, face),
                face,
                construction_id,
                boundary,
            }
        })
        .collect();
    Ok(z)
}

/// Construct a one-zone starting document with clearly synthetic input presets.
/// Geometry is explicit, not a cloned four-zone demonstration. Both designs are
/// initially identical; no proposed improvement or qualification is invented.
pub fn new_thermal_model(zone: &ZoneBoxInput, calendar: &EnergyCalendar) -> Result<ThermalModel> {
    let mut m = synthetic_four_zone_model(false);
    let assignments = synthetic_topology_assignments();
    // Use the preset as an input library; no old geometry or windows survive.
    let mut z = m.zones[0].clone();
    z.id = "preset_source".into();
    m.zones = vec![z];
    let z = clone_zone(&m, "preset_source", zone, &assignments)?;
    m.zones = vec![z];
    m.name = "New energy building — synthetic input presets".into();
    m.detached_shading.clear();
    for s in &mut m.schedules {
        s.holidays = calendar
            .holidays
            .iter()
            .map(|date| HolidayOverride {
                date: *date,
                profile: s.sunday.clone(),
            })
            .collect();
    }
    validate_thermal_model(&m, calendar)?;
    Ok(m)
}
pub fn synthetic_topology_assignments() -> TopologyAssignments {
    TopologyAssignments {
        exterior_construction_id: "external".into(), ground_construction_id: "external".into(),
        interzone_construction_id: "internal".into(), interzone_reverse_construction_id: "internal".into(),
        insulated_ground_reason: "Explicit synthetic insulated-ground adiabatic approximation; no ground heat-transfer model.".into(),
    }
}

fn full_face_contact(a: &ThermalZone, b: &ThermalZone) -> Option<ThermalFace> {
    const EPS: f64 = 1e-8;
    ThermalFace::ALL.into_iter().find(|&face| {
        let k = axis(face);
        let plane_a = a.origin_m[k] + if is_high(face) { a.size_m[k] } else { 0. };
        let plane_b = b.origin_m[k] + if is_high(face) { 0. } else { b.size_m[k] };
        (plane_a - plane_b).abs() <= EPS
            && (0..3).filter(|&j| j != k).all(|j| {
                (a.origin_m[j] - b.origin_m[j]).abs() <= EPS
                    && (a.size_m[j] - b.size_m[j]).abs() <= EPS
            })
    })
}
fn reconcile_topology(
    m: &mut ThermalModel,
    original: &ThermalModel,
    a: &TopologyAssignments,
) -> Result<()> {
    let mut pairs = BTreeMap::<String, (String, String, bool)>::new();
    for (i, za) in m.zones.iter().enumerate() {
        for zb in &m.zones[i + 1..] {
            if let Some(face) = full_face_contact(za, zb) {
                let sa = za
                    .surfaces
                    .iter()
                    .find(|s| s.face == face)
                    .ok_or_else(|| invalid("surfaces", "Missing face"))?;
                let sb = zb
                    .surfaces
                    .iter()
                    .find(|s| s.face == face.opposite())
                    .ok_or_else(|| invalid("surfaces", "Missing face"))?;
                pairs.insert(sa.id.clone(), (zb.id.clone(), sb.id.clone(), false));
                pairs.insert(sb.id.clone(), (za.id.clone(), sa.id.clone(), true));
            }
        }
    }
    for z in &mut m.zones {
        for s in &mut z.surfaces {
            let prior = original
                .zones
                .iter()
                .flat_map(|z| &z.surfaces)
                .find(|p| p.id == s.id);
            if let Some((partner_zone, partner_surface, reverse)) = pairs.get(&s.id) {
                let boundary = ThermalBoundary::Interzone {
                    zone_id: partner_zone.clone(),
                    surface_id: partner_surface.clone(),
                };
                if prior.is_none_or(|p| p.boundary != boundary) {
                    s.construction_id = if *reverse {
                        &a.interzone_reverse_construction_id
                    } else {
                        &a.interzone_construction_id
                    }
                    .clone();
                }
                s.boundary = boundary;
                // A window becoming internal must not silently disappear.
                if z.windows.iter().any(|w| w.surface_id == s.id) {
                    return Err(invalid("window.parent", format!("Face {} becomes interzone. Remove its exterior windows explicitly before connecting zones.", s.id)));
                }
            } else if matches!(s.boundary, ThermalBoundary::Interzone { .. }) {
                (s.construction_id, s.boundary) = exposed(s.face, z.storey, a);
            }
        }
    }
    Ok(())
}

fn candidate(m: &ThermalModel, tx: &ThermalTransaction) -> Result<ThermalModel> {
    let mut next = m.clone();
    let mut topology = false;
    match &tx.operation {
        ThermalOperation::SetZoneBox { zone } => {
            let z = zone_mut(&mut next, &zone.id)?;
            z.name = zone.name.clone();
            z.storey = zone.storey;
            z.origin_m = zone.origin_m;
            z.size_m = zone.size_m;
            topology = true;
        }
        ThermalOperation::AddZone {
            source_zone_id,
            zone,
        } => {
            next.zones
                .push(clone_zone(m, source_zone_id, zone, &tx.topology)?);
            topology = true;
        }
        ThermalOperation::AddAdjacent {
            source_zone_id,
            id,
            name,
            face,
            depth_m,
        } => {
            positive("zone.depth_m", *depth_m)?;
            let source = m
                .zones
                .iter()
                .find(|z| z.id == *source_zone_id)
                .ok_or_else(|| invalid("zone.id", "Unknown source zone"))?;
            let mut spec = ZoneBoxInput {
                id: id.clone(),
                name: name.clone(),
                storey: source.storey,
                origin_m: source.origin_m,
                size_m: source.size_m,
            };
            let k = axis(*face);
            spec.size_m[k] = *depth_m;
            spec.origin_m[k] += if is_high(*face) {
                source.size_m[k]
            } else {
                -*depth_m
            };
            if k == 2 {
                spec.storey = if is_high(*face) {
                    source.storey + 1
                } else {
                    source.storey.saturating_sub(1)
                };
            }
            next.zones
                .push(clone_zone(m, source_zone_id, &spec, &tx.topology)?);
            topology = true;
        }
        ThermalOperation::RemoveZone { zone_id } => {
            zone_mut(&mut next, zone_id)?;
            next.zones.retain(|z| z.id != *zone_id);
            topology = true;
        }
        ThermalOperation::PutWindow { zone_id, window } => {
            let z = zone_mut(&mut next, zone_id)?;
            if let Some(w) = z.windows.iter_mut().find(|w| w.id == window.id) {
                *w = window.clone();
            } else {
                z.windows.push(window.clone());
            }
        }
        ThermalOperation::RemoveWindow { zone_id, window_id } => {
            let z = zone_mut(&mut next, zone_id)?;
            if !z.windows.iter().any(|w| w.id == *window_id) {
                return Err(invalid("window.id", "Unknown window"));
            }
            z.windows.retain(|w| w.id != *window_id);
        }
        ThermalOperation::PutShade { shade } => {
            if let Some(s) = next.detached_shading.iter_mut().find(|s| s.id == shade.id) {
                *s = shade.clone();
            } else {
                next.detached_shading.push(shade.clone());
            }
        }
        ThermalOperation::RemoveShade { shade_id } => {
            if !next.detached_shading.iter().any(|s| s.id == *shade_id) {
                return Err(invalid("shade.id", "Unknown detached shading"));
            }
            next.detached_shading.retain(|s| s.id != *shade_id);
        }
        ThermalOperation::AssignSurface {
            zone_id,
            surface_id,
            construction_id,
            boundary,
        } => {
            let s = zone_mut(&mut next, zone_id)?
                .surfaces
                .iter_mut()
                .find(|s| s.id == *surface_id)
                .ok_or_else(|| invalid("surface.id", "Unknown surface"))?;
            s.construction_id = construction_id.clone();
            s.boundary = boundary.clone();
            // The normal validator rejects any unilateral interzone change.
        }
    }
    if topology {
        reconcile_topology(&mut next, m, &tx.topology)?;
    }
    Ok(next)
}

/// Semantic snapshots include generated/dependent geometry and assignments.
/// A window edit consequently affects the wall aperture and its overhang; a
/// building rotation affects every geometric object. Visibility is absent.
pub fn thermal_object_snapshots(m: &ThermalModel) -> Result<BTreeMap<String, Value>> {
    let mut objects = BTreeMap::new();
    // ':' cannot occur in a scientific object ID, so a user zone/material
    // cannot replace this whole-building lock snapshot.
    objects.insert("model:building".into(), serde_json::to_value(m)?);
    for mat in &m.materials {
        objects.insert(mat.id.clone(), serde_json::to_value(mat)?);
    }
    for c in &m.constructions {
        objects.insert(
            c.id.clone(),
            json!([
                c,
                m.materials
                    .iter()
                    .filter(|x| c.layers.contains(&x.id))
                    .collect::<Vec<_>>()
            ]),
        );
    }
    for g in &m.glazing {
        objects.insert(g.id.clone(), serde_json::to_value(g)?);
    }
    for c in &m.curves {
        objects.insert(c.id.clone(), serde_json::to_value(c)?);
    }
    for s in &m.schedules {
        objects.insert(s.id.clone(), serde_json::to_value(s)?);
    }
    for z in &m.zones {
        let transform = json!([z.origin_m, z.size_m, m.north_rotation_deg]);
        objects.insert(z.id.clone(), json!([z, transform]));
        for s in &z.surfaces {
            let construction = objects.get(&s.construction_id).cloned();
            objects.insert(
                s.id.clone(),
                json!([
                    s,
                    transform,
                    construction,
                    z.windows
                        .iter()
                        .filter(|w| w.surface_id == s.id)
                        .collect::<Vec<_>>()
                ]),
            );
        }
        for w in &z.windows {
            let glazing = objects.get(&w.glazing_id).cloned();
            let parent_face = z
                .surfaces
                .iter()
                .find(|s| s.id == w.surface_id)
                .map(|s| s.face);
            objects.insert(w.id.clone(), json!([w, parent_face, transform, glazing]));
            if w.overhang.is_some() {
                objects.insert(
                    format!("overhang:{}", w.id),
                    json!([
                        w.surface_id,
                        w.offset_u_m,
                        w.offset_v_m,
                        w.width_m,
                        w.height_m,
                        w.overhang,
                        parent_face,
                        transform
                    ]),
                );
            }
        }
        if let Some(h) = &z.cooling_system {
            let dependencies = [
                &h.capacity_temperature_curve,
                &h.capacity_flow_curve,
                &h.eir_temperature_curve,
                &h.eir_flow_curve,
                &h.part_load_fraction_curve,
                &h.availability_schedule,
                &h.cooling_setpoint_schedule,
                &h.balanced_ventilation.fraction_schedule,
            ];
            objects.insert(
                h.id.clone(),
                json!([
                    h,
                    dependencies
                        .iter()
                        .map(|id| objects.get(*id))
                        .collect::<Vec<_>>()
                ]),
            );
        }
    }
    for s in &m.detached_shading {
        objects.insert(s.id.clone(), json!([s, m.north_rotation_deg]));
    }
    Ok(objects)
}
pub fn changed_thermal_objects(before: &ThermalModel, after: &ThermalModel) -> Result<Vec<String>> {
    let a = thermal_object_snapshots(before)?;
    let b = thermal_object_snapshots(after)?;
    Ok(a.keys()
        .chain(b.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|id| a.get(*id) != b.get(*id))
        .cloned()
        .collect())
}
pub fn enforce_thermal_locks(
    before: &ThermalModel,
    after: &ThermalModel,
    locked_ids: &[String],
) -> Result<()> {
    let objects = thermal_object_snapshots(before)?;
    if locked_ids.len() > objects.len()
        || locked_ids.iter().collect::<BTreeSet<_>>().len() != locked_ids.len()
    {
        return Err(invalid(
            "locks",
            "Lock IDs must be unique existing semantic objects",
        ));
    }
    for id in locked_ids {
        if !objects.contains_key(id) {
            return Err(invalid("locks", format!("Unknown locked object {id}")));
        }
    }
    let changed = changed_thermal_objects(before, after)?;
    let affected = locked_ids
        .iter()
        .filter(|id| changed.contains(id))
        .cloned()
        .collect::<Vec<_>>();
    if !affected.is_empty() {
        return Err(invalid("locks", format!("This transaction affects locked objects: {}. Unlock them explicitly or cancel the edit.", affected.join(", "))));
    }
    Ok(())
}

pub fn preview_thermal_transaction(
    model: &ThermalModel,
    calendar: &EnergyCalendar,
    tx: &ThermalTransaction,
    locked_ids: &[String],
) -> Result<ThermalTransactionPreview> {
    let base = validate_thermal_model(model, calendar)?.model_input_sha256;
    if tx.schema_version != THERMAL_TRANSACTION_SCHEMA {
        return Err(invalid(
            "transaction.schema",
            "Unsupported thermal transaction",
        ));
    }
    if base != tx.expected_model_sha256 {
        return Err(invalid(
            "transaction.base",
            "Accepted inputs changed after this preview. Refresh and review a new preview.",
        ));
    }
    let next = candidate(model, tx)?;
    let mut problems = Vec::new();
    let candidate_model_sha256 = match validate_thermal_model(&next, calendar) {
        Ok(v) => Some(v.model_input_sha256),
        Err(e) => {
            problems.push(e.to_string());
            None
        }
    };
    if let Err(e) = enforce_thermal_locks(model, &next, locked_ids) {
        problems.push(e.to_string());
    }
    Ok(ThermalTransactionPreview {
        schema_version: THERMAL_TRANSACTION_SCHEMA.into(), base_model_sha256: base,
        candidate_model_sha256, affected_object_ids: changed_thermal_objects(model, &next)?,
        candidate: next, valid: problems.is_empty(), problems,
        assumptions: vec!["Only the explicitly targeted design is changed. Review shared-face construction and boundary changes before applying.".into(), "New zones copy the selected source zone's gains and cooling system, with new stable IDs; no openings are silently copied or removed.".into()],
    })
}
pub fn commit_thermal_transaction(
    model: &ThermalModel,
    calendar: &EnergyCalendar,
    tx: &ThermalTransaction,
    locked_ids: &[String],
) -> Result<ThermalModel> {
    let preview = preview_thermal_transaction(model, calendar, tx, locked_ids)?;
    if !preview.valid {
        return Err(invalid("transaction", preview.problems.join("\n")));
    }
    Ok(preview.candidate)
}

/// Exact piecewise-constant schedule blocks. Gaps, overlaps and off-grid times
/// are errors; existing ten-minute values are never interpolated or averaged.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleBlock {
    pub start_minute: u16,
    pub end_minute: u16,
    pub value: f64,
}
pub fn profile_from_blocks(blocks: &[ScheduleBlock], kind: ScheduleKind) -> Result<DayProfile> {
    if blocks.is_empty() || blocks.len() > 144 {
        return Err(invalid(
            "schedule.blocks",
            "Expected 1–144 complete time blocks",
        ));
    }
    let mut values = Vec::with_capacity(144);
    let mut end = 0;
    for b in blocks {
        if b.start_minute != end
            || b.end_minute <= b.start_minute
            || b.end_minute > 1440
            || b.start_minute % 10 != 0
            || b.end_minute % 10 != 0
        {
            return Err(invalid("schedule.blocks", "Cover 00:00–24:00 exactly, in order, with ten-minute boundaries and no gaps or overlaps"));
        }
        values.extend(std::iter::repeat_n(
            b.value,
            usize::from((b.end_minute - b.start_minute) / 10),
        ));
        end = b.end_minute;
    }
    if end != 1440 {
        return Err(invalid("schedule.blocks", "Final block must end at 24:00"));
    }
    let profile = DayProfile { values };
    let calendar = EnergyCalendar {
        start_weekday: Weekday::Monday,
        holidays: vec![],
        utc_offset_minutes: 0,
    };
    Schedule {
        id: "profile_check".into(),
        kind,
        weekday: profile.clone(),
        saturday: profile.clone(),
        sunday: profile.clone(),
        holidays: vec![],
    }
    .validate(&calendar)?;
    Ok(profile)
}
/// Explicit calendar edit: retain matching holiday profiles, remove deleted
/// dates and seed newly declared holidays from that schedule's Sunday profile.
/// The caller must disclose/choose this policy before staging the draft.
pub fn model_with_calendar(
    model: &ThermalModel,
    calendar: &EnergyCalendar,
) -> Result<ThermalModel> {
    calendar.validate()?;
    let mut candidate = model.clone();
    for schedule in &mut candidate.schedules {
        let old = schedule.holidays.clone();
        schedule.holidays = calendar
            .holidays
            .iter()
            .map(|date| HolidayOverride {
                date: *date,
                profile: old
                    .iter()
                    .find(|h| h.date == *date)
                    .map(|h| h.profile.clone())
                    .unwrap_or_else(|| schedule.sunday.clone()),
            })
            .collect();
    }
    validate_thermal_model(&candidate, calendar)?;
    Ok(candidate)
}

/// A stable preview binding for callers that keep request/response separately.
pub fn transaction_identity(tx: &ThermalTransaction) -> Result<String> {
    canonical_hash(tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn first() -> (ThermalModel, EnergyCalendar) {
        let c = synthetic_energy_calendar();
        (
            new_thermal_model(
                &ZoneBoxInput {
                    id: "zone1".into(),
                    name: "First".into(),
                    storey: 1,
                    origin_m: [0., 0., 0.],
                    size_m: [8., 6., 3.],
                },
                &c,
            )
            .unwrap(),
            c,
        )
    }
    fn tx(m: &ThermalModel, c: &EnergyCalendar, op: ThermalOperation) -> ThermalTransaction {
        ThermalTransaction {
            schema_version: THERMAL_TRANSACTION_SCHEMA.into(),
            expected_model_sha256: validate_thermal_model(m, c).unwrap().model_input_sha256,
            operation: op,
            topology: synthetic_topology_assignments(),
        }
    }
    fn add(
        m: &ThermalModel,
        c: &EnergyCalendar,
        source: &str,
        id: &str,
        face: ThermalFace,
        depth: f64,
    ) -> ThermalModel {
        commit_thermal_transaction(
            m,
            c,
            &tx(
                m,
                c,
                ThermalOperation::AddAdjacent {
                    source_zone_id: source.into(),
                    id: id.into(),
                    name: id.into(),
                    face,
                    depth_m: depth,
                },
            ),
            &[],
        )
        .unwrap()
    }
    #[test]
    fn build_four_zones_and_remove_with_reciprocal_faces() {
        let (a, c) = first();
        let b = add(&a, &c, "zone1", "zone2", ThermalFace::East, 8.);
        let d = add(&b, &c, "zone1", "zone3", ThermalFace::Roof, 3.);
        let e = add(&d, &c, "zone2", "zone4", ThermalFace::Roof, 3.);
        assert_eq!(e.zones.len(), 4);
        assert_eq!(a.zones.len(), 1);
        assert_eq!(
            validate_thermal_model(&e, &c).unwrap().gross_floor_area_m2,
            192.
        );
        assert_eq!(
            e.zones
                .iter()
                .flat_map(|z| &z.surfaces)
                .filter(|s| matches!(s.boundary, ThermalBoundary::Interzone { .. }))
                .count(),
            8
        );
        let f = commit_thermal_transaction(
            &e,
            &c,
            &tx(
                &e,
                &c,
                ThermalOperation::RemoveZone {
                    zone_id: "zone4".into(),
                },
            ),
            &[],
        )
        .unwrap();
        assert!(!serde_json::to_string(&f.zones).unwrap().contains("zone4"));
        assert_eq!(
            validate_thermal_model(&f, &c).unwrap().gross_floor_area_m2,
            144.
        );
    }
    #[test]
    fn invalid_partial_adjacency_and_stale_preview_are_not_commits() {
        let (a, c) = first();
        let b = add(&a, &c, "zone1", "zone2", ThermalFace::East, 8.);
        let before = serde_json::to_vec(&b).unwrap();
        let request = tx(
            &b,
            &c,
            ThermalOperation::SetZoneBox {
                zone: ZoneBoxInput {
                    id: "zone2".into(),
                    name: "Second".into(),
                    storey: 1,
                    origin_m: [8., 1., 0.],
                    size_m: [8., 6., 3.],
                },
            },
        );
        assert!(
            !preview_thermal_transaction(&b, &c, &request, &[])
                .unwrap()
                .valid
        );
        assert!(commit_thermal_transaction(&b, &c, &request, &[]).is_err());
        assert_eq!(serde_json::to_vec(&b).unwrap(), before);
        assert!(preview_thermal_transaction(&a, &c, &request, &[]).is_err());
    }
    #[test]
    fn locks_include_generated_overhang_wall_and_shared_material() {
        let (mut a, c) = first();
        a.zones[0].windows.push(ThermalWindow {
            id: "window1".into(),
            surface_id: "zone1_South".into(),
            glazing_id: "window_glazing".into(),
            offset_u_m: 2.,
            offset_v_m: 1.,
            width_m: 3.,
            height_m: 1.5,
            overhang: Some(ThermalOverhang {
                projection_m: 0.5,
                side_extension_m: 0.,
                gap_above_m: 0.,
            }),
        });
        let mut b = a.clone();
        b.zones[0].windows[0].offset_u_m += 0.2;
        assert!(enforce_thermal_locks(&a, &b, &["overhang:window1".into()]).is_err());
        assert!(enforce_thermal_locks(&a, &b, &["zone1_South".into()]).is_err());
        let mut b = a.clone();
        b.materials[0].thickness_m += 0.1;
        assert!(enforce_thermal_locks(&a, &b, &["zone1_South".into()]).is_err());
        assert!(enforce_thermal_locks(&a, &a, &["window1".into()]).is_ok());
        let mut moved = a.clone();
        for s in &mut moved.zones[0].surfaces {
            s.face = match s.face {
                ThermalFace::South => ThermalFace::North,
                ThermalFace::North => ThermalFace::South,
                other => other,
            };
        }
        validate_thermal_model(&moved, &c).unwrap();
        assert!(enforce_thermal_locks(&a, &moved, &["window1".into()]).is_err());
        assert!(enforce_thermal_locks(&a, &moved, &["overhang:window1".into()]).is_err());
        let mut collision = a.clone();
        collision.materials[0].id = "building".into();
        let mut changed = collision.clone();
        changed.name = "Changed".into();
        assert!(enforce_thermal_locks(&collision, &changed, &["model:building".into()]).is_err());
        let request = tx(
            &a,
            &c,
            ThermalOperation::AddAdjacent {
                source_zone_id: "zone1".into(),
                id: "zone2".into(),
                name: "Second".into(),
                face: ThermalFace::South,
                depth_m: 6.,
            },
        );
        assert!(commit_thermal_transaction(&a, &c, &request, &[]).is_err());
        assert_eq!(a.zones[0].windows.len(), 1);
    }
    #[test]
    fn blocks_are_exact_and_never_interpolated() {
        let p = profile_from_blocks(
            &[
                ScheduleBlock {
                    start_minute: 0,
                    end_minute: 480,
                    value: 0.,
                },
                ScheduleBlock {
                    start_minute: 480,
                    end_minute: 1080,
                    value: 1.,
                },
                ScheduleBlock {
                    start_minute: 1080,
                    end_minute: 1440,
                    value: 0.,
                },
            ],
            ScheduleKind::Fraction,
        )
        .unwrap();
        assert_eq!(p.values.len(), 144);
        assert_eq!(p.values.iter().sum::<f64>(), 60.);
        assert_eq!(p.values[47], 0.);
        assert_eq!(p.values[48], 1.);
        assert_eq!(p.values[108], 0.);
        assert!(profile_from_blocks(
            &[ScheduleBlock {
                start_minute: 0,
                end_minute: 1440,
                value: 2.
            }],
            ScheduleKind::Fraction
        )
        .is_err());
        assert!(profile_from_blocks(
            &[ScheduleBlock {
                start_minute: 5,
                end_minute: 1440,
                value: 1.
            }],
            ScheduleKind::Fraction
        )
        .is_err());
    }
}
