//! Document presentation state is versioned independently of scientific inputs.
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const DISPLAY_SCHEMA_VERSION: &str = "greenlens.display.v0.2";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ViewId {
    Perspective,
    Top,
    Front,
    Right,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DisplayMode {
    Wireframe,
    Shaded,
    Analysis,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CameraState {
    /// Camera and target coordinates use the model's world ENU convention.
    pub position: [f64; 3],
    pub target: [f64; 3],
    pub up: [f64; 3],
    pub zoom: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LayerState {
    pub visible: bool,
    pub locked: bool,
    pub color: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ObjectDisplayState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DisplayState {
    pub schema_version: String,
    pub active_view: ViewId,
    pub maximized_view: Option<ViewId>,
    pub mode: DisplayMode,
    pub cameras: BTreeMap<ViewId, CameraState>,
    pub layers: BTreeMap<String, LayerState>,
    #[serde(default)]
    pub objects: BTreeMap<String, ObjectDisplayState>,
    pub grid_visible: bool,
    pub grid_snap: bool,
    pub object_snap: bool,
    pub ortho: bool,
    pub gumball: bool,
}
impl DisplayState {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == DISPLAY_SCHEMA_VERSION,
            "Unsupported display-state schema"
        );
        for camera in self.cameras.values() {
            ensure!(
                camera
                    .position
                    .iter()
                    .chain(&camera.target)
                    .all(|v| v.is_finite() && v.abs() <= 1e6),
                "Camera coordinates must be finite and within one million metres"
            );
            ensure!(
                camera.up.iter().all(|v| v.is_finite() && v.abs() <= 1.),
                "Camera up direction must be finite and normalized"
            );
            let length: f64 = camera.up.iter().map(|v| v * v).sum();
            ensure!(
                (length - 1.).abs() <= 1e-6,
                "Camera up direction must be normalized"
            );
            let direction: [f64; 3] =
                std::array::from_fn(|i| camera.position[i] - camera.target[i]);
            let separation: f64 = direction.iter().map(|v| v * v).sum();
            ensure!(separation > 1e-12, "Camera position and target must differ");
            let cross = [
                direction[1] * camera.up[2] - direction[2] * camera.up[1],
                direction[2] * camera.up[0] - direction[0] * camera.up[2],
                direction[0] * camera.up[1] - direction[1] * camera.up[0],
            ];
            ensure!(
                cross.iter().map(|v| v * v).sum::<f64>() > separation * 1e-12,
                "Camera up direction cannot be parallel to its viewing direction"
            );
            ensure!(
                camera.zoom.is_finite() && (0.0001..=10000.).contains(&camera.zoom),
                "Camera zoom must be between 0.0001 and 10000"
            );
        }
        let expected = [
            "room",
            "window",
            "overhang",
            "obstruction",
            "sensors",
            "analysis",
        ];
        ensure!(
            self.layers.len() == expected.len()
                && expected.iter().all(|id| self.layers.contains_key(*id)),
            "Display layers must match the six supported semantic layers"
        );
        for layer in self.layers.values() {
            validate_color(&layer.color)?;
        }
        ensure!(
            self.objects.len() <= 10032,
            "Display object overrides exceed the supported semantic object count"
        );
        for (id, object) in &self.objects {
            ensure!(
                valid_semantic_id(id),
                "Unsupported semantic object identifier: {id}"
            );
            if let Some(color) = &object.color {
                validate_color(color)?;
            }
        }
        Ok(())
    }
}
fn validate_color(color: &str) -> Result<()> {
    ensure!(
        color.len() == 7
            && color.starts_with('#')
            && color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit),
        "Display colors must use #RRGGBB; they do not change material reflectance"
    );
    Ok(())
}
pub fn valid_semantic_id(id: &str) -> bool {
    if ["room", "window", "overhang", "obstruction", "sensor-plane"].contains(&id) {
        return true;
    }
    if let Some(sensor) = id.strip_prefix("sensor:s") {
        return sensor.len() == 6
            && sensor.bytes().all(|c| c.is_ascii_digit())
            && sensor
                .parse::<usize>()
                .is_ok_and(|n| (1..=10000).contains(&n));
    }
    id.strip_prefix("surface:").is_some_and(|surface| {
        [
            "floor",
            "ceiling",
            "west_wall",
            "east_wall",
            "north_wall",
            "south_below",
            "south_above",
            "south_left",
            "south_right",
            "window",
            "overhang",
            "obstruction_bottom",
            "obstruction_top",
            "obstruction_west",
            "obstruction_east",
            "obstruction_south",
            "obstruction_north",
        ]
        .contains(&surface)
    })
}
