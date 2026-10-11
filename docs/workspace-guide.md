# GreenLens 0.2 workspace

GreenLens uses original controls and a bounded Rhino-inspired interaction model. It edits one analytical room/window, optional horizontal overhang and detached box. It has no NURBS, arbitrary solids, booleans, copy/extrude, 3DM/DWG or BIM authoring. The [interaction matrix](rhino-interaction-matrix.md) links each behavior to official McNeel documentation and its intentional limits.

## Views and inspection

Click a viewport title to activate it. Double-click it, or use its square button, to maximize/restore. **4View** restores the four-view layout. Perspective uses right-drag orbit and Shift+right-drag pan; Top, Front and Right use right-drag pan with a fixed orthographic projection. Wheel zooms. With a viewport focused, arrow keys navigate; Shift+arrows pan, plus/minus zoom. **ZoomExtents** frames visible geometry and **ZoomSelected** frames the selection. Each camera survives selection, accepted model edits, display modes, panel changes and arriving calculation results.

Scientific coordinates are metres, +X east, +Y north and +Z up. The model-local y=0 window facade faces south at zero rotation; supported rotation is clockwise from true north. Renderer coordinates are `[x,z,-y]`. Top looks down Z with +X right/+Y up; Front looks north with +X right/+Z up; Right looks west with +Y right/+Z up. The construction grid and model handles are distinct from the equal-area scientific sensor grid.

Click geometry or a sensor to select. Shift adds; Ctrl removes. Left-to-right rectangular selection includes fully contained objects; right-to-left crossing includes intersecting objects. The overlap chooser disambiguates coincident surfaces, envelope and sensors. Escape cancels a provisional operation or clears selection. Layers and Properties use the same semantic IDs; generated faces are inspectable parts of the envelope, not independently editable CAD objects. Hidden objects cannot be picked. Locked objects cannot be picked for editing and have no manipulation handles; unlock them through Layers or **Unlock**.

## Supported editing

Choose **Move**, **Resize** or **Rotate**, then one entity. Drag a supported handle for a provisional preview. Release requests authoritative Rust validation and commits one undo entry; an invalid candidate is rejected without changing the document/history. Escape restores the accepted geometry. Click an axis/resize/rotation handle without dragging to enter an exact parameter. Properties offers the same parameters, constraints and units. A mixed selection never silently receives an ambiguous edit.

Window movement is local X/Z within its facade, with width/height resizing. Overhang controls projection, side extension and gap. The obstruction moves in local X/clearance and resizes width/depth/height. The room changes dimensions and clockwise vertical rotation around the fixed origin. Only overhang and obstruction can be created or removed. Unsupported axes, room/window removal and arbitrary transforms are rejected with explanations.

The visible construction grid has 1 m lines in world XY (Top/Perspective), XZ (Front) and YZ (Right). Grid Snap uses 0.1 m increments on supported model-local parameters. Ortho constrains planar movement to the dominant supported local axis. Endpoint, midpoint and center snaps derive from validated rectangular geometry and use a 9 CSS-pixel proximity tolerance; the marker and coordinate strip report the active snap. Geometric snaps position the selected object's Move grip; Resize/Rotate use grid/angular snapping and numeric values, without pretending to solve arbitrary boundary alignment. Object snaps and grid snapping can be toggled separately. Final candidates still require Rust validation. Hiding a layer never removes its geometry from Radiance; lock/visibility/camera/mode changes cannot stale a valid scientific result.

Property fields accept lengths such as `1800mm`, `1.8m`, `150 cm`, `1ft` or `2in`. Enter commits; Escape restores the accepted value. Native text selection, clipboard and text-field undo shortcuts are preserved. Outside text fields: Ctrl+N/O/S, Ctrl+Shift+S, Ctrl+Z/Y, Ctrl+Shift+Z, Ctrl+A, Delete, F8 Ortho and F9 Grid Snap operate on the document/workspace.

## Commands

The command line is case insensitive and uses a fixed registry. Tab completes names, Up recalls the last command, and the arrow expands history. Commands with missing arguments prompt for values. Numeric command tokens attach their units, e.g. `1800mm`; plain numbers are metres or degrees as appropriate. Escape clears a prompt. Empty Enter repeats only safe navigation/selection commands, never Run, removal, save or document replacement.

Examples:

```text
4View
Front
Zoom Extents
Window 1.8m 1.5m 0.9m 2m
Move window 100mm 0m
Resize window 1.6m 1.4m
Rotate room 15deg
Room 4m 6m 3m 0deg
Overhang 0.6m 0.1m 0m
Obstruction 6m 2m 4m 5m 2m
Remove obstruction
Undo
Redo
Run
Cancel
Export
```

Other commands include New, Open, Save, SaveAs, Close, Perspective/Top/Front/Right, ZoomSelected, SelectAll/SelectNone, Hide/Show, Lock/Unlock, Wireframe/Shaded/Analysis, Cutaway and Grid/Snap/Ortho/Osnap/Gumball. Cutaway makes the envelope translucent for interior inspection without removing any scientific surface. Menu items and command entry dispatch the same application actions. The tool strip selects a manipulation mode; it never runs a shell or arbitrary code.

## Documents and evidence

Open, Save As and Export use native dialogs. Select a project directory for Open/Save As and a new `.zip` filename for Export. Save As copies referenced compact evidence and pending completed evidence; it does not merely change a path. A saved A/B project reopens and exports without its original scratch. Octrees are disposable and regenerated for replay.

New, Open and native close protect unsaved scientific inputs and uncollected evidence with **Save / Discard / Cancel**. Save cancels/joins an active job first and must succeed before replacement; a failed or cancelled save leaves the document in place. Open chooses and validates its target before discarding anything, so a cancelled picker cannot lose pending evidence. Discard is explicit permission to release pending evidence. Camera/display changes alone do not trigger this scientific-document guard; use Save to persist their optional display state. Save retains scientific undo history.

If an external writer changes `project.json` during calculation, the completed run remains pending. **Retry save** still respects the known revision. **Save complete copy** retains current local models, old runs and pending evidence in a new folder. **Reload and reconcile** explicitly reloads externally saved models and resets local undo history while attaching the completed immutable snapshot. Its confirmation explains that unsaved local model edits will be replaced. No recovery silently overwrites an external revision.

The Analysis panel offers real Run/Cancel/Export, prescribed sky settings and engine diagnostics. Bottom tabs expose the sensor table, historical A/B comparison, job logs, provenance and method. A/B reports identify immutable run IDs and mismatches with current designs. Edited geometry never displays stale lux. Undo back to the exact scientific input restores validity. Percentage deltas are omitted; both alternatives share a lux scale.

Export keeps all retained runs and only the newest compatible A/B pair per comparison-condition group, rather than every A×B pairing. Writer and reader share limits of 5,000 entries, 64 MiB per file and 256 MiB total uncompressed data. Oversized exports fail before publishing a ZIP, with an actionable size message. See [replay](replay.md), [persistence](persistence.md) and [v0.2 verification](v0.2-native-verification.md).
