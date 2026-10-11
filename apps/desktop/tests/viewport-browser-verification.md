# Supplemental viewport verification — 2026-10-11

This record covers the real Three.js/WebGL2 renderer in Chrome at a 1038 × 705 CSS-pixel browser viewport. It is supplementary to the installed Windows Tauri/Radiance verification. The harness uses native-CLI-validated synthetic A/B geometry but **mock color values and callbacks that reject every candidate**. It establishes no scientific accuracy, native acceptance, persistence or successful Rust edit acceptance.

Reproduce from `apps/desktop` with `npm run dev -- --port 1421`, then open `http://127.0.0.1:1421/tests/viewport-harness.html`. Native `greenlens validate examples/synthetic-a.json` and `synthetic-b.json` produced the compact geometry fixture JSON. No engine output, private model, machine configuration or runtime test bridge is embedded in production.

Observed genuine UI actions:

| Check | Evidence/result |
| --- | --- |
| Keyboard Perspective orbit/zoom | ArrowRight changed camera position, retaining target and the three parallel cameras. Focused `+` moved the Perspective camera toward the unchanged target. |
| Persistent render resources | Read-only DOM `data-camera-identity` UUIDs and `data-camera-state` position/target/zoom strings were identical before/after selection, fixture result arrival, display-mode changes and native-validated A→B geometry update. |
| Four views/maximize | Title double-click and maximize/restore preserved all four camera UUIDs/states and parallel projection labels. |
| Cutaway | Final-source clean reload: after ArrowRight and `+`, Cutaway off visibly made walls/ceiling opaque. Off/on/off preserved all four settled camera UUID/position/target/zoom records exactly. The first immediate comparison read a previous animation frame; a settled comparison isolated the Cutaway action. |
| Numeric gumball | Clicking Window X arrow requested `window.center_x_m`, operation `move`, value `2`, unit `m`. |
| Actual left-drag | An X-arrow drag produced exactly one rejecting callback, followed by `candidate:null`; accepted fixture remained unchanged. |
| Grid Snap + Ortho | Actual center-grip diagonal drag produced X `2.4000000000000004 m` (floating representation of 2.4 m) and unchanged sill `0.9 m`, then rejected/cleared. This is a provisional candidate, not Rust acceptance evidence. |
| Contained selection | A rectangle fully containing the window and aligned overhang selected those two. A small left-to-right rectangle entirely inside the window selected no object. |
| Crossing selection | Reversing that same small rectangle selected Window and North Wall, whose projected polygons crossed it. |
| Overlap disambiguation | Click within Window offered Window, North Wall and Room envelope. Choosing Window replaced selection. A real Shift-click followed by North Wall added it; a real Ctrl-click followed by Window removed only Window. |
| Locks and visibility | Locked Window was absent from overlap picker and crossing selection. Unlocked then hidden Window remained absent from overlap picker. These operations changed display state only. |

Explicitly **synthetic DOM pointer sequences**, initiated through named harness buttons, exercised the production viewport event listeners because available browser/native control APIs cannot hold the right button during a drag:

- RMB Perspective changed position with target unchanged.
- Shift+RMB Perspective moved camera and target by the same vector: approximately `[-0.846725,-0.657027,-0.556744] m` in the recorded fixture view.
- Top RMB preserved direction `[0,0,20]` and `orthographic` projection while translating position/target.
- A provisional Window center drag reached X `2.255922102042076 m`, sill `1.0437869822485206 m`; Escape then produced `candidate:null`, zero commit callbacks and unchanged selection/model. This verifies event cancellation, not native device input.

The harness exposes state in `#test-state`; panes expose read-only `data-viewport`, `data-camera-identity`, `data-camera-projection`, `data-camera-state`, and the canvas exposes `data-renderer-identity`. There is no production mutation/testing API. Tests capture camera states after a rendered frame, not a potentially stale immediate screenshot. Escape was repeated after the final exact-wire-preview change: a candidate was present, then cleared, with zero commit callbacks and no interaction error.

Browser investigation found and fixed an overlap-menu defect: native pane pointer capture ran before React's delegated overlay handler. The native listener now excludes menu/button descendants, and genuine click/Shift/Ctrl chooser actions passed afterward. Development hot-reload `createRoot` warnings came from editing the harness entrypoint; the final source was checked after a clean reload, with no new browser errors in that final check. They were not production runtime errors.

`npm run check` passed after the final viewport source changes. `npm test` passed all 35 frontend tests at this integration point, including nine viewport math tests (coordinates, selection, polygon crossing, snap boundaries/priority, immutable supported candidates, actual Ortho/Grid Snap math, rotation/context signs and exact provisional wires). The snap regression excludes invented midpoints between separate context faces and preserves coincident visible-owner points. Exact-wire regressions verify a 37° rotated resized window retains its sill/facade and a 31° rotated context has exactly twelve true box edges. Installed desktop, accepted Rust transactions, authoritative invalid-input rejection, undo/redo, native screen sizes/scaling, real result arrival and engine save/export/replay remain the separate native acceptance record.
