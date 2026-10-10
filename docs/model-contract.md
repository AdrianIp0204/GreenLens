# Model and result conventions

The public structural contract is `schemas/model-v0.1.schema.json`. Rust's `validate_model` additionally checks dependent geometry bounds, clearance, polygon winding, and sensor count. Unknown model fields and unsupported schema versions are rejected. JSON uses SI metres and illuminance in lux. Every numeric input must be finite.

The calculation label is **Static daylight illuminance under a prescribed CIE standard overcast sky.** No date, weather file, occupancy or annual sequence is part of this contract. Location and IANA timezone strings are display-only metadata and do not change the sky or results. This prescribed overcast distribution is azimuth-symmetric. Run timestamps, handled by the application service, are UTC.

## Coordinate frame and surfaces

Local geometry is a rectangular room occupying `0 <= x <= width`, `0 <= y <= depth`, and `0 <= z <= height`. At zero rotation, +X is east, +Y north, +Z up. The opening is on `y=0`, facing south (azimuth 180 degrees). The local origin is the southwest floor corner. World position is the local position rotated clockwise about the Z axis through the origin:

```
x_world = cos(theta) * x_local + sin(theta) * y_local
y_world = -sin(theta) * x_local + cos(theta) * y_local
z_world = z_local
```

Facade azimuth is `(180 + rotation_deg) mod 360`, clockwise from true north. All returned surfaces and sensors already use world coordinates. The renderer uses the explicit right-handed rotation `(x_renderer, y_renderer, z_renderer) = (x_world, z_world, -y_world)` because Three.js uses Y up. Display rotation must not be fed back as scientific geometry.

Opaque room normals point inward. Four disjoint opaque rectangles surround the opening: full-width strips above and below, plus side strips between the sill and head. Their combined area is facade area minus opening area. There is exactly one zero-thickness glass rectangle, facing outward south. No opaque polygon spans the aperture. The optional overhang is one horizontal, zero-thickness opaque rectangle, extending south from the facade at window-head height plus its gap. Its upward normal is a winding convention; the diffuse material is two-sided in the adapter. These are geometrical abstraction surfaces, not construction assemblies.

The obstruction is one closed rectangular opaque box outside the south facade. Its local x extent is `center_x_m +/- width_m/2`, its y extent is `[-distance_m-depth_m, -distance_m]`, and its z extent is `[0,height_m]`. It rotates with the room. The distance must be at least 0.05 m. If its x and z extents intersect the overhang, including boundary-only contact, the obstruction must clear the overhang projection by at least 0.01 m. Positive lateral or vertical separation is also allowed. There is no arbitrary mesh or site-object import.

All generated polygons are checked for finite vertices, nonzero edges and area, planarity, convexity, and expected winding. The construction and input bounds prevent opaque facade overlap, box/room intersection and obstruction/overhang intersection. Adjacent room faces and the glass perimeter intentionally share boundary edges.

## Bounds

| Input | Accepted range |
|---|---|
| Room width/depth | 0.5–100 m |
| Room height | 0.5–20 m |
| Clockwise room rotation | 0–359.999999 degrees |
| Window width/height | At least 0.05 m; must fit the facade |
| Window perimeter | At least 0.01 m of opaque wall on every edge |
| Neutral visible normal-incidence transmittance | 0.01–0.90 |
| Diffuse reflectances, including ground | 0–0.95 |
| Enabled overhang projection | 0.01–10 m |
| Overhang side extension/gap | 0–10 m; elevation cannot exceed room roof |
| Obstruction width/depth/height | 0.05–100 m |
| Obstruction nearest distance | 0.05–100 m, also subject to overhang clearance |
| Obstruction x center | -100–200 m |
| Workplane | At least 0.05 m above floor and below ceiling |
| Requested maximum sensor spacing | 0.05–10 m |
| Wall inset | At least 0.01 m; twice the inset must be less than room width/depth |
| Exterior horizontal illuminance | 1–200,000 lx |
| Sensor count | 1–10,000 |

Dimensions of the current model are always checked together. For example, increasing room width does not silently move or recenter its existing window. The user edits `window.center_x_m` explicitly. Setting an optional object to `null` disables it.

## Sensor grid

The inset analysis rectangle is tiled by equal-area cells. Column count is `ceil((width-2*inset)/requested_spacing)` and row count is `ceil((depth-2*inset)/requested_spacing)`. Actual spacings are inset width/column count and inset depth/row count. Both are no larger than the requested maximum. Sensors are cell centers at the workplane height, all with direction `[0,0,1]`.

Order is ascending local y rows, with ascending local x within each row. Stable IDs start at `s000001`. Rotation changes positions while retaining this order. The default grid has 7 columns and 11 rows: 77 sensors, 0.50 m actual spacing in each direction and 0.25 m² cell area. The first position is `[0.5,0.5,0.8]` and the last is `[3.5,5.5,0.8]` before rotation. Mean illuminance is the arithmetic mean of these equal-area cell values, not a whole-room area integral including the excluded boundary strip.

## Glazing and results

The adapter uses identical RGB transmissivity on a single Radiance glass surface. Rust converts neutral normal-incidence visible transmittance T using the release formula for refractive index 1.52. Converted values must be finite and within [0,1]; inputs are never silently clamped. T=0.60 gives transmissivity 0.654047488895434. This equivalent single-pane abstraction omits thickness, frames, spectral behavior, complex scattering, multilayer effects and measured angular properties of real products.

Rust converts RGB irradiance to illuminance with `179*(0.265074126*R + 0.670114631*G + 0.064811243*B)`. Every channel and output must be finite and nonnegative. Exact expected sensor count is required. This convention applies to the pinned Radiance RGB calculation and is not a conversion of broadband solar irradiance to lux.

Statistics have `min`, `mean`, `max` and `count`. Signed statistics are permitted only for comparison deltas. Calculation results require nonnegative lux. Core functions do not populate placeholder results.

## Fingerprints, staleness and comparisons

Canonical JSON sorts object keys recursively and normalizes negative zero; arrays retain order. SHA-256 digests are lowercase hexadecimal. The normalized input hash covers all model fields, while the scientific input hash excludes display name and optional metadata. The sensor hash covers ordered sensor IDs, world positions and directions. A historical result becomes stale when the scientific input hash differs from current validated inputs. Renaming a model does not make results scientifically stale.

The comparison-condition hash covers room dimensions/rotation, complete sensor grid, sky, obstruction and interior reflectances. It excludes the window, glazing and overhang so those three design categories can differ. The application service must also match engine/library identity, adapter version and numerical settings. Input hashes do not substitute for those run-provenance checks. Incompatible comparisons are rejected without interpolation or resampling. The comparison response records the allowed design differences. Absolute lux deltas may be positive or negative: reflective overhangs are not assumed to decrease every sensor. Both alternatives must share one heatmap scale.

## Synthetic examples and ownership

`examples/synthetic-a.json` and `examples/synthetic-b.json` are original synthetic examples licensed Apache-2.0 with this repository. Both have a 4 × 6 × 3 m room, centered 1.8 × 1.5 m window with 0.9 m sill, T=0.60, ceiling/wall/floor reflectances 0.80/0.50/0.20, 10,000 lx exterior horizontal illuminance, ground reflectance 0.20, no obstruction, zero model rotation and no location metadata. B adds a 0.60 m overhang exactly matching window width, at the window head, with neutral diffuse reflectance 0.50. A has no overhang.

These are simulation inputs, not measured building datasets. Limited analytical checks do not establish arbitrary-building accuracy. A metric is not a criterion evaluation or formal certification.
