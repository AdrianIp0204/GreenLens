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

## Post-report event regression (2026-10-11)

The installed-app user reported modifier selection and held-drag Escape failures. Earlier browser evidence did not establish those native interactions; native retesting remains required after this repair.

Before behavioral repair, `node --test tests/viewport-events.test.mjs` reproduced five failures through production listeners and real Three.js picking: Ctrl/Shift over a selected Move grip never reached selection; Escape left pointer capture; input-targeted Escape left the active preview; lost capture did not cancel. Listener registration was extracted without changing behavior before this failing run. The event delivery/capture shim and commit callback are explicitly test substitutes.

After repair and review, eight listener tests pass, covering unique Ctrl removal, ambiguous removal with the modifier released before choice, Shift addition over a grip, held-pointer Escape, active-drag text focus versus idle text Escape, an open chooser not intercepting a focused Properties field's Escape, pointer cancel/lost capture/window blur, and exactly one ordinary release commit despite synchronous lost-capture delivery. The focused-field/chooser regression failed before the reviewed ordering correction: held drag takes priority, then native text editing, then chooser dismissal.

The actual React `SceneView` browser harness additionally verified:

- A real Ctrl-click at the selected window's visible center grip changed selection from `[window]` to `[]`, with zero commits and no numeric dialog.
- A real Shift-click at that grip opened **Add to selection**. Choosing Room Envelope after releasing Shift produced `[window, room]`.
- A real Ctrl-click with both entities selected opened **Remove from selection**. Choosing Window after releasing Ctrl left `[room]`.
- The explicitly synthetic held-drag/input-focus Escape fixture flushes a parent React preview update before cancellation. A non-null window candidate existed, capture changed from `true` to `false`, the candidate cleared, selection stayed `[window]`, and commit count stayed zero. An actual Escape in the idle text input preserved its text and selection.

An initial asynchronous fixture attempt waited for animation frames; the automation released its initiating pointer before Escape, so it had already committed and was **not** counted as cancellation evidence. The final fixture uses an explicit React flush while the initiating pointer remains held. These browser/synthetic results supplement the pending installed-native user retest; they do not establish that the exact reported native failure is resolved.

Browser investigation found and fixed an overlap-menu defect: native pane pointer capture ran before React's delegated overlay handler. The native listener now excludes menu/button descendants, and genuine click/Shift/Ctrl chooser actions passed afterward. Development hot-reload `createRoot` warnings came from editing the harness entrypoint; the final source was checked after a clean reload, with no new browser errors in that final check. They were not production runtime errors.

`npm run check` passed after the final viewport source changes. `npm test` passed all 35 frontend tests at this integration point, including nine viewport math tests (coordinates, selection, polygon crossing, snap boundaries/priority, immutable supported candidates, actual Ortho/Grid Snap math, rotation/context signs and exact provisional wires). The snap regression excludes invented midpoints between separate context faces and preserves coincident visible-owner points. Exact-wire regressions verify a 37° rotated resized window retains its sill/facade and a 31° rotated context has exactly twelve true box edges. Installed desktop, accepted Rust transactions, authoritative invalid-input rejection, undo/redo, native screen sizes/scaling, real result arrival and engine save/export/replay remain the separate native acceptance record.

## Visible selection modes (2026-10-11)

Subsequent user feedback clarified a discoverability problem: pressing the modifier keys alone gave no useful indication of what to do. The production `SelectionBar` was mounted with the actual `SceneView` in this isolated browser harness. No new native failure cause is inferred from the following supplementary checks.

- Clicking **Add**, then geometry and **Room envelope** in its overlap chooser, changed `[window]` to `[window, room]`; the visible summary changed to **2 selected** with both names. Add mode hid manipulation handles.
- Clicking **Remove**, then geometry and **Window**, retained `[room]`, with **1 selected**. It did not delete geometry or invoke a model commit.
- A genuine Shift-click overrode Remove and offered **Add to selection**. Choosing Window after releasing Shift added it. A genuine Ctrl-click overrode Add and offered **Remove from selection**; choosing Window after releasing Ctrl removed it.
- Changing the explicit mode dismissed an open chooser without changing the selection. Escape in the idle text fixture retained its text, selection and Remove mode. Escape with the modeling canvas focused restored Replace and cleared selection.
- Controls, count and selected names remained visible in the observed 1039-pixel-wide browser viewport. Camera states were unchanged throughout these selection-only checks; candidate and numeric dialog remained null and commit count remained zero.

Four newly added production-listener regressions failed before implementation of explicit modes. The resulting 15-listener suite covers explicit clicks and crossing rectangles, modifier precedence, retained chooser intent, stale-chooser dismissal, Escape mode reset with native text preservation, all previous cancellation/capture cases, and physical Shift/RMB behavior independent of selection mode. The complete application and installed-package checks are separate; this fixture does not call Rust or Radiance.
