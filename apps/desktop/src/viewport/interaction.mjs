// Display and interaction math only. Candidate edits are never accepted here;
// every committed model goes through the shared Rust document transaction.
export const worldToRender = ([x, y, z]) => [x, z, -y];
export const renderToWorld = ([x, y, z]) => [x, -z, y];
export function localToWorld([x, y, z], degrees) {
  const a = degrees * Math.PI / 180, c = Math.cos(a), s = Math.sin(a);
  return [c * x + s * y, -s * x + c * y, z];
}
export function worldToLocal(point, degrees) { return localToWorld(point, -degrees); }
export function mergeSelection(current, picked, shift = false, control = false) {
  if (control) return current.filter(id => !picked.includes(id));
  return shift ? [...new Set([...current, ...picked])] : [...new Set(picked)];
}
export function objectVisibility(id, layer, display) {
  const inherited = id.startsWith('surface:') ? display.objects.room : id.startsWith('sensor:') ? display.objects['sensor-plane'] : undefined;
  const own = display.objects[id];
  return {
    visible: display.layers[layer].visible && inherited?.visible !== false && own?.visible !== false,
    locked: display.layers[layer].locked || inherited?.locked === true || own?.locked === true,
    color: own?.color ?? inherited?.color ?? display.layers[layer].color,
  };
}
export function selectableIds(ids, objects, display) {
  return [...new Set(ids)].filter(id => {
    const object = objects.find(o => o.id === id);
    if (!object) return false;
    const state = objectVisibility(id, object.layer, display);
    return state.visible && !state.locked;
  });
}
const between = (v, a, b) => v >= Math.min(a, b) && v <= Math.max(a, b);
const orient = (a, b, c) => (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
function segmentsCross(a, b, c, d) {
  const p = orient(a, b, c), q = orient(a, b, d), r = orient(c, d, a), s = orient(c, d, b);
  if (p * q < 0 && r * s < 0) return true;
  return [[p, c, a, b], [q, d, a, b], [r, a, c, d], [s, b, c, d]].some(([v, x, y, z]) => Math.abs(v) < 1e-8 && between(x[0], y[0], z[0]) && between(x[1], y[1], z[1]));
}
function inPolygon(point, polygon) {
  let inside = false;
  for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i++) {
    const a = polygon[i], b = polygon[j];
    if (((a[1] > point[1]) !== (b[1] > point[1])) && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]) inside = !inside;
  }
  return inside;
}
export function rectangleSelect(polygon, start, end) {
  if (!polygon.length || polygon.some(p => p.some(v => !Number.isFinite(v)))) return false;
  const min = [Math.min(start[0], end[0]), Math.min(start[1], end[1])];
  const max = [Math.max(start[0], end[0]), Math.max(start[1], end[1])];
  const inside = p => between(p[0], min[0], max[0]) && between(p[1], min[1], max[1]);
  if (end[0] >= start[0]) return polygon.every(inside); // Window: complete containment.
  if (polygon.some(inside)) return true;
  const corners = [min, [max[0], min[1]], max, [min[0], max[1]]];
  if (polygon.length > 2 && corners.some(p => inPolygon(p, polygon))) return true;
  return polygon.some((a, i) => corners.some((c, j) => segmentsCross(a, polygon[(i + 1) % polygon.length], c, corners[(j + 1) % 4])));
}
export function constrainedDelta(delta, ortho) {
  if (!ortho) return [...delta];
  const axis = delta.reduce((winner, value, index) => Math.abs(value) > Math.abs(delta[winner]) ? index : winner, 0);
  return delta.map((value, index) => index === axis ? value : 0);
}
export function snapScalar(value, interval, enabled) { return enabled && interval > 0 ? Math.round(value / interval) * interval : value; }
export function snapCandidates(objects) {
  const found = new Map();
  for (const object of objects) {
    const v = object.vertices;
    if (!v.length) continue;
    const center = [0, 1, 2].map(axis => v.reduce((sum, p) => sum + p[axis], 0) / v.length);
    const rows = v.map(position => ({ position, kind: 'End', objectId: object.id }));
    if (v.length > 1) v.forEach((p, i) => rows.push({ position: p.map((x, axis) => (x + v[(i + 1) % v.length][axis]) / 2), kind: 'Mid', objectId: object.id }));
    rows.push({ position: center, kind: 'Cen', objectId: object.id });
    for (const row of rows) { const key = `${row.objectId}:${row.kind}:${row.position.map(x => x.toFixed(8)).join(',')}`; if (!found.has(key)) found.set(key, row); }
  }
  return [...found.values()];
}
export function nearestSnap(candidates, project, pointer, tolerance = 9) {
  let best = null, distance = tolerance;
  const priority = { End: 0, Mid: 1, Cen: 2 };
  for (const candidate of candidates) {
    const screen = project(candidate.position);
    if (!screen) continue;
    const d = Math.hypot(screen[0] - pointer[0], screen[1] - pointer[1]);
    if (d < distance - 0.001 || (best && Math.abs(d - distance) <= 0.001 && priority[candidate.kind] < priority[best.kind])) { best = candidate; distance = d; }
  }
  return best;
}
export function handleDefinitions(model, objectId, tool = 'select') {
  const all = [];
  const add = (axis, kind, label, path, multiplier = 1) => all.push({ id: `${kind}-${axis}`, axis, kind, label, path, multiplier });
  if (objectId === 'window') {
    add(0, 'move', 'Window center X', ['window', 'center_x_m']); add(2, 'move', 'Window sill', ['window', 'sill_m']);
    add(0, 'resize', 'Window width', ['window', 'width_m'], 2); add(2, 'resize', 'Window height', ['window', 'height_m']);
  } else if (objectId === 'overhang' && model.overhang) {
    add(1, 'move', 'Overhang projection', ['overhang', 'projection_m'], -1); add(2, 'move', 'Gap above window', ['overhang', 'gap_above_window_m']);
    add(0, 'resize', 'Overhang side extension', ['overhang', 'side_extension_m']);
  } else if (objectId === 'obstruction' && model.obstruction) {
    add(0, 'move', 'Obstruction center X', ['obstruction', 'center_x_m']); add(1, 'move', 'Facade clearance', ['obstruction', 'distance_m'], -1);
    add(0, 'resize', 'Obstruction width', ['obstruction', 'width_m'], 2); add(1, 'resize', 'Obstruction depth', ['obstruction', 'depth_m'], -1); add(2, 'resize', 'Obstruction height', ['obstruction', 'height_m']);
  } else if (objectId === 'sensor-plane') add(2, 'move', 'Workplane height', ['sensors', 'workplane_height_m']);
  else if (objectId === 'room') {
    add(0, 'resize', 'Room width', ['room', 'width_m']); add(1, 'resize', 'Room depth', ['room', 'depth_m']); add(2, 'resize', 'Room height', ['room', 'height_m']);
    add(2, 'rotate', 'Room rotation clockwise', ['room', 'rotation_deg']);
  }
  return all.filter(h => tool === 'select' || tool === h.kind);
}
export function candidateForHandle(model, handle, delta) {
  if (!Number.isFinite(delta)) return null;
  const candidate = structuredClone(model), [group, field] = handle.path;
  if (!candidate[group] || typeof candidate[group][field] !== 'number') return null;
  candidate[group][field] += delta * handle.multiplier;
  if (handle.kind === 'rotate') candidate[group][field] = ((candidate[group][field] % 360) + 360) % 360;
  return candidate; // Invalid bounds remain visible in preview; Rust decides acceptance.
}
export function candidateForPlanarMove(model, objectId, delta, ortho = false, gridSnap = false) {
  const constrained = constrainedDelta(delta, ortho), candidate = structuredClone(model);
  if (objectId === 'window') {
    candidate.window.center_x_m = snapScalar(model.window.center_x_m + constrained[0], 0.1, gridSnap);
    candidate.window.sill_m = snapScalar(model.window.sill_m + constrained[2], 0.1, gridSnap);
  } else if (objectId === 'obstruction' && candidate.obstruction) {
    candidate.obstruction.center_x_m = snapScalar(model.obstruction.center_x_m + constrained[0], 0.1, gridSnap);
    candidate.obstruction.distance_m = snapScalar(model.obstruction.distance_m - constrained[1], 0.1, gridSnap);
  } else return null;
  return candidate;
}
// Provisional wireframe only. Never used as saved geometry or engine input.
// Explicit perimeter/box edges preserve anchors and model rotation without
// creating world-AABB corners or diagonals across separate context faces.
export function previewWireSegments(model, objectId) {
  const { room: r, window: w, sensors: s } = model;
  let corners = [], edges = [[0, 1], [1, 2], [2, 3], [3, 0]];
  const box = (x0, x1, y0, y1, z1) => {
    corners = [[x0,y0,0],[x1,y0,0],[x1,y1,0],[x0,y1,0],[x0,y0,z1],[x1,y0,z1],[x1,y1,z1],[x0,y1,z1]];
    edges = [...edges, [4,5],[5,6],[6,7],[7,4],[0,4],[1,5],[2,6],[3,7]];
  };
  if (objectId === 'window') {
    const x0 = w.center_x_m - w.width_m / 2, x1 = w.center_x_m + w.width_m / 2, z1 = w.sill_m + w.height_m;
    corners = [[x0,0,w.sill_m],[x1,0,w.sill_m],[x1,0,z1],[x0,0,z1]];
  } else if (objectId === 'overhang' && model.overhang) {
    const o = model.overhang, x0 = w.center_x_m - w.width_m / 2 - o.side_extension_m, x1 = w.center_x_m + w.width_m / 2 + o.side_extension_m, z = w.sill_m + w.height_m + o.gap_above_window_m;
    corners = [[x0,0,z],[x1,0,z],[x1,-o.projection_m,z],[x0,-o.projection_m,z]];
  } else if (objectId === 'obstruction' && model.obstruction) {
    const o = model.obstruction; box(o.center_x_m - o.width_m / 2, o.center_x_m + o.width_m / 2, -o.distance_m - o.depth_m, -o.distance_m, o.height_m);
  } else if (objectId === 'room') box(0, r.width_m, 0, r.depth_m, r.height_m);
  else if (objectId === 'sensor-plane') corners = [[s.wall_inset_m,s.wall_inset_m,s.workplane_height_m],[r.width_m-s.wall_inset_m,s.wall_inset_m,s.workplane_height_m],[r.width_m-s.wall_inset_m,r.depth_m-s.wall_inset_m,s.workplane_height_m],[s.wall_inset_m,r.depth_m-s.wall_inset_m,s.workplane_height_m]];
  else return [];
  return edges.map(edge => edge.map(i => localToWorld(corners[i], r.rotation_deg)));
}
export function convexHull(points) {
  const sorted = [...new Map(points.map(p => [`${p[0]},${p[1]}`, p])).values()].sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  if (sorted.length < 3) return sorted;
  const half = rows => { const result = []; for (const point of rows) { while (result.length >= 2 && orient(result.at(-2), result.at(-1), point) <= 0) result.pop(); result.push(point); } return result; };
  const lower = half(sorted), upper = half([...sorted].reverse()); lower.pop(); upper.pop(); return [...lower, ...upper];
}
