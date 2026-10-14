//! Bounded display geometry generated from authoritative, validated thermal inputs.
//! This supplementary DTO is not a simulation identity or an editable mesh model.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThermalSceneObject {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub zone_id: Option<String>,
    pub storey: Option<u8>,
    pub polygons_m: Vec<[[f64; 3]; 4]>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThermalScene {
    pub schema_version: String,
    pub model_sha256: String,
    pub coordinates: String,
    pub objects: Vec<ThermalSceneObject>,
}
fn axes(face: ThermalFace) -> (usize, [usize; 2], bool) {
    match face {
        ThermalFace::West => (0, [1, 2], false),
        ThermalFace::East => (0, [1, 2], true),
        ThermalFace::South => (1, [0, 2], false),
        ThermalFace::North => (1, [0, 2], true),
        ThermalFace::Floor => (2, [0, 1], false),
        ThermalFace::Roof => (2, [0, 1], true),
    }
}
fn rectangle(
    origin: [f64; 3],
    size: [f64; 3],
    face: ThermalFace,
    offset: [f64; 2],
    extent: [f64; 2],
) -> [[f64; 3]; 4] {
    let (a, [u, v], high) = axes(face);
    let mut p = origin;
    if high {
        p[a] += size[a]
    };
    p[u] += offset[0];
    p[v] += offset[1];
    let mut q = [p; 4];
    q[1][u] += extent[0];
    q[2][u] += extent[0];
    q[2][v] += extent[1];
    q[3][v] += extent[1];
    if (a != 1) != high {
        q.reverse()
    };
    q
}
pub fn thermal_scene(
    model: &ThermalModel,
    calendar: &EnergyCalendar,
) -> crate::Result<ThermalScene> {
    let valid = validate_thermal_model(model, calendar)?;
    let (sin, cos) = model.north_rotation_deg.to_radians().sin_cos();
    let rotate = |p: [f64; 3]| [cos * p[0] + sin * p[1], -sin * p[0] + cos * p[1], p[2]];
    let mut objects = vec![];
    for zone in &model.zones {
        objects.push(ThermalSceneObject {
            id: zone.id.clone(),
            label: zone.name.clone(),
            kind: "zone".into(),
            zone_id: Some(zone.id.clone()),
            storey: Some(zone.storey),
            polygons_m: vec![],
        });
        for surface in valid.surfaces.iter().filter(|s| s.zone_id == zone.id) {
            objects.push(ThermalSceneObject {
                id: surface.id.clone(),
                label: format!("{:?} · {}", surface.face, zone.name),
                kind: "surface".into(),
                zone_id: Some(zone.id.clone()),
                storey: Some(zone.storey),
                polygons_m: vec![surface.vertices_m],
            });
        }
        for window in &zone.windows {
            let face = zone
                .surfaces
                .iter()
                .find(|s| s.id == window.surface_id)
                .ok_or_else(|| invalid("scene.window", "Parent surface missing"))?
                .face;
            objects.push(ThermalSceneObject {
                id: window.id.clone(),
                label: format!("Window {}", window.id),
                kind: "window".into(),
                zone_id: Some(zone.id.clone()),
                storey: Some(zone.storey),
                polygons_m: vec![rectangle(
                    zone.origin_m,
                    zone.size_m,
                    face,
                    [window.offset_u_m, window.offset_v_m],
                    [window.width_m, window.height_m],
                )
                .map(rotate)],
            });
            if let Some(h) = &window.overhang {
                let (a, [u, v], high) = axes(face);
                let mut p = zone.origin_m;
                if high {
                    p[a] += zone.size_m[a]
                };
                p[u] += window.offset_u_m - h.side_extension_m;
                p[v] += window.offset_v_m + window.height_m + h.gap_above_m;
                let mut q = [p; 4];
                q[1][u] += window.width_m + 2. * h.side_extension_m;
                q[2] = q[1];
                q[2][a] += if high {
                    h.projection_m
                } else {
                    -h.projection_m
                };
                q[3][a] += if high {
                    h.projection_m
                } else {
                    -h.projection_m
                };
                objects.push(ThermalSceneObject {
                    id: format!("overhang:{}", window.id),
                    label: format!("Overhang · {}", window.id),
                    kind: "overhang".into(),
                    zone_id: Some(zone.id.clone()),
                    storey: Some(zone.storey),
                    polygons_m: vec![q.map(rotate)],
                });
            }
        }
    }
    for shade in &model.detached_shading {
        let polygons_m = [
            ThermalFace::West,
            ThermalFace::East,
            ThermalFace::South,
            ThermalFace::North,
            ThermalFace::Floor,
            ThermalFace::Roof,
        ]
        .into_iter()
        .map(|face| {
            let (_, [u, v], _) = axes(face);
            rectangle(
                shade.origin_m,
                shade.size_m,
                face,
                [0., 0.],
                [shade.size_m[u], shade.size_m[v]],
            )
            .map(rotate)
        })
        .collect();
        objects.push(ThermalSceneObject {
            id: shade.id.clone(),
            label: format!("Shading {}", shade.id),
            kind: "shading".into(),
            zone_id: None,
            storey: None,
            polygons_m,
        });
    }
    Ok(ThermalScene{schema_version:"greenlens.thermal-scene.v1".into(),model_sha256:valid.model_input_sha256,coordinates:"metres; world right-handed +X east, +Y north, +Z up; clockwise building rotation from north already applied".into(),objects})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scene_ids_match_locks_and_transform_preserves_accepted_identity() {
        let mut m = synthetic_four_zone_model(false);
        let cal = synthetic_energy_calendar();
        m.north_rotation_deg = 90.;
        let before = crate::canonical_hash(&m).unwrap();
        let scene = thermal_scene(&m, &cal).unwrap();
        let snapshots = thermal_object_snapshots(&m).unwrap();
        for object in &scene.objects {
            assert!(snapshots.contains_key(&object.id));
            for polygon in &object.polygons_m {
                assert!(polygon.iter().flatten().all(|v| v.is_finite()));
            }
        }
        assert_eq!(scene.objects.iter().filter(|o| o.kind == "zone").count(), 4);
        assert_eq!(before, crate::canonical_hash(&m).unwrap());
    }
}
