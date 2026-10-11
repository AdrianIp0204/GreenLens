import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { selectableIds } from '../src/viewport/interaction.mjs';
import { worldToRender, renderToWorld, localToWorld, worldToLocal, mergeSelection, rectangleSelect, convexHull, snapCandidates, nearestSnap, handleDefinitions, candidateForHandle, candidateForPlanarMove, previewWireSegments } from '../src/viewport/interaction.mjs';

const model = JSON.parse(fs.readFileSync(new URL('../../../examples/synthetic-b.json', import.meta.url)));
const close = (a, b) => a.forEach((value, i) => assert.ok(Math.abs(value - b[i]) < 1e-10));
test('right-handed scientific ENU and renderer coordinates roundtrip without changing rotation convention', () => {
  assert.deepEqual(worldToRender([2, 3, 4]), [2, 4, -3]);
  close(renderToWorld(worldToRender([-2.7, 6.1, 0.8])), [-2.7, 6.1, 0.8]);
  close(localToWorld([0, -1, 0], 90), [-1, 0, 0]);
  close(worldToLocal(localToWorld([1.3, -5.2, 2.4], 31), 31), [1.3, -5.2, 2.4]);
});
test('click replaces, Shift adds once, Ctrl removes only picked IDs', () => {
  assert.deepEqual(mergeSelection(['room'], ['window']), ['window']);
  assert.deepEqual(mergeSelection(['window'], ['window', 'overhang'], true), ['window', 'overhang']);
  assert.deepEqual(mergeSelection(['window', 'overhang'], ['window'], true, true), ['overhang']);
  assert.deepEqual(mergeSelection(['window'], []), []);
});
test('inspector and table selection honor inherited layer and object visibility/locks',()=>{
  const objects=[{id:'room',layer:'room'},{id:'surface:south',layer:'room'},{id:'window',layer:'window'},{id:'sensor:s000001',layer:'sensors'}];
  const display={layers:Object.fromEntries(['room','window','sensors'].map(id=>[id,{visible:true,locked:false}])),objects:{}};
  display.layers.room.visible=false;display.objects['sensor-plane']={locked:true};
  assert.deepEqual(selectableIds(objects.map(o=>o.id).concat(['unknown','window']),objects,display),['window']);
  display.layers.room.visible=true;display.objects.room={locked:true};display.objects['sensor-plane']={visible:false};
  assert.deepEqual(selectableIds(objects.map(o=>o.id),objects,display),['window']);
  display.objects={};assert.deepEqual(selectableIds(objects.map(o=>o.id),objects,display),objects.map(o=>o.id));
});
test('window requires complete containment while crossing tests actual projected polygon', () => {
  const polygon = [[2, 2], [8, 2], [8, 8], [2, 8]];
  assert.equal(rectangleSelect(polygon, [0, 0], [9, 9]), true);
  assert.equal(rectangleSelect(polygon, [0, 0], [5, 5]), false);
  assert.equal(rectangleSelect(polygon, [5, 0], [0, 5]), true);
  assert.equal(rectangleSelect(polygon, [5, 4], [4, 5]), true); // rectangle lies inside polygon
  assert.equal(rectangleSelect([[0, 0], [10, 10], [11, 10], [1, 0]], [3, 7], [2, 8]), false); // bounding boxes overlap, shapes do not
  assert.equal(rectangleSelect([[3, 3]], [2, 2], [4, 4]), true);
  assert.equal(rectangleSelect([[NaN, 2]], [0, 0], [4, 4]), false);
  assert.deepEqual(convexHull([[0, 0], [1, 0], [.5, .5], [1, 1], [0, 1], [0, 0]]), [[0, 0], [1, 0], [1, 1], [0, 1]]);
});
test('snaps derive endpoints/midpoints/centers from validated geometry, with bounded screen tolerance and explicit priority', () => {
  const candidates = snapCandidates([{ id: 'window', vertices: [[0, 0, 1], [2, 0, 1], [2, 0, 3], [0, 0, 3]] }]);
  assert.ok(candidates.some(p => p.kind === 'Mid' && p.position[0] === 1 && p.position[2] === 1));
  assert.ok(candidates.some(p => p.kind === 'Cen' && p.position[0] === 1 && p.position[2] === 2));
  assert.equal(nearestSnap(candidates, p => [p[0] * 100, p[2] * 100], [100, 202]).kind, 'Cen');
  assert.equal(nearestSnap(candidates, p => [p[0] * 100, p[2] * 100], [130, 202]), null);
  assert.equal(nearestSnap([{ kind: 'Cen', position: [0, 0] }, { kind: 'End', position: [0, 0] }], p => p, [0, 0]).kind, 'End');
  const coincident = snapCandidates([{ id: 'wall', vertices: [[0, 0, 0]] }, { id: 'window', vertices: [[0, 0, 0]] }]);
  assert.ok(coincident.filter(p => p.objectId === 'window').some(p => p.kind === 'End')); // hiding wall cannot erase window snap
  const faces = snapCandidates([{ id: 'context', vertices: [[0, 0, 0], [2, 0, 0], [2, 0, 2], [0, 0, 2]] }, { id: 'context', vertices: [[0, 1, 0], [2, 1, 0], [2, 1, 2], [0, 1, 2]] }]);
  assert.ok(faces.every(p => p.position[1] === 0 || p.position[1] === 1)); // never invent a connector midpoint between faces
});
test('gumball candidate preserves original and makes only the supported parameter change; invalid bounds are not clamped', () => {
  const original = structuredClone(model), move = handleDefinitions(model, 'window', 'move').find(h => h.axis === 0);
  const result = candidateForHandle(model, move, 0.2);
  assert.equal(result.window.center_x_m, model.window.center_x_m + .2);
  assert.deepEqual(model, original);
  assert.deepEqual(result.materials, model.materials);
  const width = handleDefinitions(model, 'window', 'resize').find(h => h.axis === 0);
  assert.equal(candidateForHandle(model, width, -10).window.width_m, model.window.width_m - 20);
  assert.equal(handleDefinitions(model, 'surface:floor').length, 0);
  assert.equal(handleDefinitions(model, 'sensor:s000001').length, 0);
  assert.equal(handleDefinitions(model, 'room', 'move').length, 0);
  assert.equal(candidateForHandle(model, move, NaN), null);
});
test('planar window movement uses facade X/Z; Ortho and Grid Snap demonstrably change provisional candidate coordinates', () => {
  const planar = candidateForPlanarMove(model, 'window', [.13, 0, .07], false, false);
  assert.equal(planar.window.center_x_m, 2.13); assert.equal(planar.window.sill_m, .97);
  const ortho = candidateForPlanarMove(model, 'window', [.13, 0, .07], true, true);
  assert.ok(Math.abs(ortho.window.center_x_m - 2.1) < 1e-12); assert.equal(ortho.window.sill_m, .9);
  const vertical = candidateForPlanarMove(model, 'window', [.04, 0, .23], true, true);
  assert.equal(vertical.window.center_x_m, 2); assert.ok(Math.abs(vertical.window.sill_m - 1.1) < 1e-12);
});
test('overhang projection is a dimension edit, so only its translated gap accepts Move object snaps', () => {
  const handles = handleDefinitions(model, 'overhang');
  const projection = handles.find(h => h.path[1] === 'projection_m');
  assert.equal(projection.kind, 'resize');
  assert.equal(handles.find(h => h.path[1] === 'side_extension_m').kind, 'resize');
  assert.equal(handles.find(h => h.path[1] === 'gap_above_window_m').kind, 'move');
  assert.deepEqual(handleDefinitions(model, 'overhang', 'move').map(h => h.path[1]), ['gap_above_window_m']);
  assert.deepEqual(handleDefinitions(model, 'overhang', 'resize').map(h => h.path[1]), ['projection_m', 'side_extension_m']);
  const candidate = candidateForHandle(model, projection, -.2);
  assert.equal(candidate.overhang.projection_m, model.overhang.projection_m + .2);
  assert.equal(candidate.overhang.gap_above_window_m, model.overhang.gap_above_window_m);
});
test('context facade-distance sign and room clockwise angle remain bounded-schema operations', () => {
  const context = { ...model, obstruction: { center_x_m: 2, distance_m: 5, width_m: 2, depth_m: 1, height_m: 4, reflectance: .2 } };
  assert.equal(candidateForPlanarMove(context, 'obstruction', [0, .3, 0]).obstruction.distance_m, 4.7);
  const rotation = handleDefinitions(model, 'room', 'rotate')[0];
  assert.equal(candidateForHandle(model, rotation, -10).room.rotation_deg, 350);
});
test('rotated window resize preview preserves facade and fixed sill exactly', () => {
  const rotated = structuredClone(model); rotated.room.rotation_deg = 37;
  const height = handleDefinitions(rotated, 'window', 'resize').find(h => h.axis === 2);
  const candidate = candidateForHandle(rotated, height, .35), segments = previewWireSegments(candidate, 'window');
  assert.equal(segments.length, 4);
  const corners = segments.flat().map(p => worldToLocal(p, 37));
  assert.ok(corners.every(p => Math.abs(p[1]) < 1e-12));
  assert.equal(Math.min(...corners.map(p => p[2])), rotated.window.sill_m);
  assert.equal(Math.max(...corners.map(p => p[2])), rotated.window.sill_m + candidate.window.height_m);
  assert.ok(Math.abs(Math.min(...corners.map(p => p[0])) - (rotated.window.center_x_m - rotated.window.width_m/2)) < 1e-12);
  assert.equal(model.window.height_m, 1.5);
});
test('context preview uses only twelve true box edges, including under model rotation', () => {
  const context = { ...model, room: { ...model.room, rotation_deg: 31 }, obstruction: { center_x_m: 2, distance_m: 5, width_m: 2, depth_m: 1, height_m: 4, reflectance: .2 } };
  const segments = previewWireSegments(context, 'obstruction').map(edge => edge.map(p => worldToLocal(p, 31)));
  assert.equal(segments.length, 12);
  for (const [a,b] of segments) assert.equal(a.filter((v,i) => Math.abs(v-b[i]) > 1e-10).length, 1);
  const vertices = segments.flat();
  assert.ok(vertices.every(p => Math.abs(p[0]-1) < 1e-10 || Math.abs(p[0]-3) < 1e-10));
  assert.ok(vertices.every(p => Math.abs(p[1]+5) < 1e-10 || Math.abs(p[1]+6) < 1e-10));
  assert.deepEqual(previewWireSegments(context, 'surface:north_wall'), []);
});
