# Viewport interaction contract

`SceneView` owns one `ViewportEngine` for its mounted lifetime. Four camera objects and one WebGL2 renderer remain alive while validated geometry, results, selection and display state change. The parent must keep `SceneView` mounted when panels/tabs change. `documentId` changes only for New/Open, not Save As. Explicit view commands use a monotonically increasing `sequence`.

`types.ts` defines the parent boundary. Accepted geometry comes exclusively from Rust `ValidatedModel`; `interaction.mjs` creates provisional parameter candidates. The final drag calls `onCommitCandidate` once and awaits its boolean response. Escape sends no commit. A rejected/throwing callback restores the accepted scene and clears the provisional outline. The parent owns Rust validation, actionable errors and one-entry document history. Numeric handle clicks route the supported parameter path to the same parent transaction.

## Coordinates and navigation

Persisted coordinates/cameras use metres and world east/north/up. Three.js uses `[x,z,-y]`, a right-handed rotation. Model rotation is clockwise about world Z. Top looks downward with X right/Y up. Front looks north from south with X right/Z up. Right looks west from east with Y right/Z up. Parallel views cannot become perspective. Their construction grids lie in the corresponding world XY/XZ/YZ plane; Perspective uses world XY.

- Perspective right-drag orbits; Shift+right-drag pans. Parallel right-drag pans. Middle-drag also pans.
- Wheel zooms about the current target. Focused canvas arrow keys navigate, Shift+arrows pan and `+`/`-` zoom. Native text fields retain normal editing.
- Title double-click maximizes/restores, preserving every camera. Zoom Extents/Selected is explicit; ordinary edits/results never frame automatically.
- Cutaway makes room walls/ceiling translucent; switching it off makes those opaque. It changes materials only and preserves cameras and simulation inputs.
- Click replaces selection, Shift adds, Ctrl removes. Left-to-right rectangle requires complete containment; right-to-left selects crossing projected silhouettes. An overlap menu resolves coincident objects. Hidden/locked objects cannot be newly picked; an existing locked selection has no edit handles.

Official McNeel documentation supplied behavioral references only: [navigation](https://docs.mcneel.com/rhino/8/help/en-us/seealso/sak_navigate.htm), [mouse selection](https://docs.mcneel.com/rhino/8/help/en-us/options/mouse.htm), [selection commands](https://docs.mcneel.com/rhino/8/help/en-us/commands/selection_commands.htm). The implementation and assets are original GreenLens code.

## Supported edits and aids

The center grip moves a window in its facade X/Z plane or context in model X/Y. Colored arrow handles move supported scalar parameters; square handles resize supported dimensions. The room has a clockwise Z rotation ring, used from Top/Perspective. Clicking an individual handle requests numeric entry. A mixed/inspect-only selection offers no ambiguous transform.

The visible construction grid is 1 m. Grid Snap quantizes edited scalar coordinates to 0.1 m; it is independent of the scientific sensor grid. Ortho restricts a planar drag to its dominant local axis (and room rotation to 90° steps). Object snaps apply to Move operations: the movable center reference aligns to the chosen point in its constrained plane/axis. Resize uses Grid Snap or numeric dimensions; Rotate uses angular steps or numeric entry. These operations suppress object-snap markers and do not imply snapping a resized boundary to a point.

Object snaps use authoritative polygon endpoints, real edge midpoints and polygon vertex-average centers, including the sensor-plane rectangle. Compound context faces remain separate so no connector edges are invented. Candidates within 9 CSS pixels are eligible; the nearest wins, with End before Mid before Cen on a distance tie. Hidden geometry is excluded; locking does not remove a geometric snap reference. Coincident points retain their semantic owners.

The blue provisional outline uses exact supported parameter wire corners and model rotation, preserving fixed anchors such as the window sill. It is not simulation geometry. The displayed numeric candidate describes the proposed parameter edit; Rust validates the accepted geometry after release. Invalid candidates are never silently clamped into valid input. Display layers, camera positions, selection and aids are separate from scientific inputs/hashes.

## Supplemental verification

`npm test` includes the pure interaction regressions. `tests/viewport-harness.html` is a development-only browser harness, excluded from the production entrypoint. Its JSON geometry fixtures are native CLI validation output from original synthetic A/B models. Its colored values and rejecting callbacks are explicitly fixtures, never simulation evidence. See `tests/viewport-browser-verification.md` for observed browser checks and the native-evidence boundary.
