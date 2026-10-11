// Production event-listener integration, with real Three.js ray picking and
// geometry. EventTarget/capture shims replace browser delivery only; these tests
// are supplementary and are not native desktop or scientific acceptance.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { stripTypeScriptTypes } from 'node:module';
import * as THREE from 'three';

const compiled = new Map();
function moduleUrl(url) {
  if (!url.endsWith('.ts')) return url;
  if (compiled.has(url)) return compiled.get(url);
  const source = stripTypeScriptTypes(fs.readFileSync(new URL(url), 'utf8'), { mode: 'transform', sourceUrl: url });
  const linked = source.replace(/from\s+(['"])([^'"]+)\1/g, (_, quote, specifier) => {
    let target = specifier.startsWith('.') ? new URL(specifier, url).href : import.meta.resolve(specifier);
    if (specifier.startsWith('.') && !/\.[mc]?[jt]s$/.test(target)) target += '.ts';
    return `from ${quote}${moduleUrl(target)}${quote}`;
  });
  const result = `data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`;
  compiled.set(url, result); return result;
}
const { ViewportEngine } = await import(moduleUrl(new URL('../src/viewport/ViewportEngine.ts', import.meta.url).href));
const { defaultDisplay } = await import(moduleUrl(new URL('../src/viewport/types.ts', import.meta.url).href));
const fixture = JSON.parse(fs.readFileSync(new URL('./viewport-fixture.json', import.meta.url)));

function event(type, values = {}) {
  const result = new Event(type, { bubbles: true, cancelable: true });
  for (const [key, value] of Object.entries(values)) Object.defineProperty(result, key, { value, configurable: true });
  return result;
}
class Pane extends EventTarget {
  clientWidth = 1200; clientHeight = 900; style = {}; tagName = 'DIV'; captures = new Set();
  closest() { return null; }
  getBoundingClientRect() { return { left: 0, top: 0, width: this.clientWidth, height: this.clientHeight }; }
  focus() { this.focused = true; }
  setPointerCapture(id) { this.captures.add(id); }
  hasPointerCapture(id) { return this.captures.has(id); }
  releasePointerCapture(id) {
    if (this.captures.delete(id)) this.dispatchEvent(event('lostpointercapture', { pointerId: id }));
  }
}
function harness(selectionMode = 'replace', viewId = 'front') {
  const pane = new Pane(), host = new EventTarget(), priorWindow = globalThis.window;
  globalThis.window = host;
  const camera = viewId === 'perspective' ? new THREE.PerspectiveCamera(40, 4 / 3, 0.01, 10000) : new THREE.OrthographicCamera(-6, 6, 4.5, -4.5, -5000, 5000);
  camera.position.set(2, 1.5, 20); camera.lookAt(2, 1.5, 0); camera.updateProjectionMatrix(); camera.updateMatrixWorld();
  const view = { id: viewId, camera, target: new THREE.Vector3(2, 1.5, 0), element: pane };
  const display = defaultDisplay(); display.active_view = viewId; display.maximized_view = viewId;
  display.object_snap = false; display.grid_snap = false;
  const state = { selection: ['window'], selectionMode, candidate: null, commits: [], numeric: [], overlay: null };
  // Skip only the WebGL renderer constructor. Attach the same listener methods
  // used by the live constructor and run actual picking/preview/commit paths.
  const engine = Object.assign(Object.create(ViewportEngine.prototype), {
    scene: new THREE.Scene(), surfaces: new Map(), objects: new Map(), views: new Map([[viewId, view]]),
    grid: new THREE.Group(), handles: new THREE.Group(), ghost: new THREE.Group(), sensors: null, cells: null,
    plane: new THREE.Mesh(new THREE.BufferGeometry(), new THREE.MeshBasicMaterial()), raycaster: new THREE.Raycaster(),
    disposers: [], drag: null, pending: false, signature: fixture.scientific_input_hash, documentId: 'fixture', awaitingFrame: false,
    overlay: { rectangle: null, chooser: null, hover: null, snap: null, preview: null }, handleKey: '', lastCommand: -1,
    onOverlay: overlay => { state.overlay = overlay; },
  });
  engine.plane.userData.entityId = 'sensor-plane';
  engine.scene.add(engine.plane, engine.handles, engine.ghost);
  engine.props = {
    documentId: 'fixture', validated: fixture, lux: null, scale: 1, selection: state.selection, selectionMode, display, tool: 'move',
    onSelectionChange: ids => { state.selection = ids; engine.update({ ...engine.props, selection: ids }); },
    onSelectionModeChange: mode => { state.selectionMode = mode; engine.update({ ...engine.props, selectionMode: mode }); },
    onDisplayChange: next => { engine.update({ ...engine.props, display: next }); },
    onPreviewCandidate: candidate => { state.candidate = candidate; },
    onCommitCandidate: async candidate => { state.commits.push(candidate); return true; },
    onNumericHandle: request => { state.numeric.push(request); },
  };
  engine.syncGeometry(); engine.syncAppearance(); engine.syncHandles(); engine.scene.updateMatrixWorld(true);
  engine.bindViewEvents(view); engine.bindGlobalEvents();
  const center = engine.project([fixture.model.window.center_x_m, 0, fixture.model.window.sill_m + fixture.model.window.height_m / 2], view);
  const pointer = (type, overrides = {}) => pane.dispatchEvent(event(type, { pointerId: 1, pointerType: 'mouse', button: 0, buttons: type === 'pointerup' ? 0 : 1, clientX: center[0], clientY: center[1], shiftKey: false, ctrlKey: false, metaKey: false, ...overrides }));
  const beginDrag = () => { pointer('pointerdown'); pointer('pointermove', { clientX: center[0] + 35, clientY: center[1] - 20 }); assert.ok(state.candidate, 'real handle picking must produce a preview before cancellation'); };
  const close = () => { engine.disposers.forEach(dispose => dispose()); globalThis.window = priorWindow; };
  return { engine, pane, host, state, center, pointer, beginDrag, close };
}

test('Ctrl-click on a selected Move grip removes the unique selected hit instead of manipulating', () => {
  const h = harness();
  try {
    h.pointer('pointerdown', { ctrlKey: true }); h.pointer('pointerup', { ctrlKey: true });
    assert.equal(h.state.overlay.chooser, null, 'unselected transparent surfaces must not obscure a unique removal');
    assert.deepEqual(h.state.selection, []); assert.equal(h.state.commits.length, 0); assert.equal(h.state.numeric.length, 0);
  } finally { h.close(); }
});
test('ambiguous removal keeps its original Ctrl intent after the modifier is released for the chooser', () => {
  const h = harness();
  try {
    h.state.selection = ['window', 'room']; h.engine.update({ ...h.engine.props, selection: h.state.selection });
    h.pointer('pointerdown', { ctrlKey: true }); h.pointer('pointerup', { ctrlKey: true });
    assert.deepEqual(h.state.overlay.chooser.ids.sort(), ['room', 'window']);
    h.engine.choose('window', false, false);
    assert.deepEqual(h.state.selection, ['room']); assert.equal(h.state.commits.length, 0);
  } finally { h.close(); }
});
test('Shift-click over a selected grip retains selection and adds the chosen overlapping entity', () => {
  const h = harness();
  try {
    h.pointer('pointerdown', { shiftKey: true }); h.pointer('pointerup', { shiftKey: true });
    assert.ok(h.state.overlay.chooser?.ids.includes('room'));
    h.engine.choose('room', false, false);
    assert.deepEqual(h.state.selection, ['window', 'room']); assert.equal(h.state.commits.length, 0);
  } finally { h.close(); }
});
test('explicit Add and Remove modes select without a modifier and bypass manipulation grips', () => {
  const add = harness('add');
  try {
    add.pointer('pointerdown'); add.pointer('pointerup');
    assert.equal(add.state.overlay.chooser?.shift, true);
    add.engine.choose('room', false, false);
    assert.deepEqual(add.state.selection, ['window', 'room']);
    assert.equal(add.state.commits.length, 0); assert.equal(add.state.numeric.length, 0);
  } finally { add.close(); }
  const remove = harness('remove');
  try {
    remove.pointer('pointerdown'); remove.pointer('pointerup');
    assert.deepEqual(remove.state.selection, []);
    assert.equal(remove.state.commits.length, 0); assert.equal(remove.state.numeric.length, 0);
  } finally { remove.close(); }
});
test('pointer modifiers override explicit selection modes with Ctrl taking priority', () => {
  for (const selectionMode of ['replace', 'add', 'remove']) {
    const h = harness(selectionMode);
    try {
      h.pointer('pointerdown', { shiftKey: true, ctrlKey: true }); h.pointer('pointerup');
      assert.deepEqual(h.state.selection, [], `${selectionMode}: Ctrl removes despite Shift or base mode`);
    } finally { h.close(); }
  }
  const h = harness('remove');
  try {
    h.pointer('pointerdown', { shiftKey: true }); h.pointer('pointerup');
    assert.equal(h.state.overlay.chooser?.shift, true);
    h.engine.choose('room', false, false);
    assert.deepEqual(h.state.selection, ['window', 'room']);
  } finally { h.close(); }
});
test('explicit mode is captured for the overlap chooser, with a later modifier able to override it', () => {
  const h = harness('remove');
  try {
    h.engine.update({ ...h.engine.props, selection: ['window', 'room'] });
    h.pointer('pointerdown'); h.pointer('pointerup');
    assert.equal(h.state.overlay.chooser?.ctrl, true);
    h.engine.choose('window', true, false);
    assert.deepEqual(h.state.selection, ['window', 'room'], 'Shift overrides stored Remove rather than combining intents');
    h.pointer('pointerdown'); h.pointer('pointerup');
    h.engine.choose('window', false, false);
    assert.deepEqual(h.state.selection, ['room'], 'unmodified choice retains the mode from the original pick');
  } finally { h.close(); }
});
test('changing the visible selection mode dismisses a stale chooser without selecting or editing', () => {
  const h = harness('add');
  try {
    h.pointer('pointerdown'); h.pointer('pointerup');
    assert.ok(h.state.overlay.chooser);
    h.engine.update({ ...h.engine.props, selectionMode: 'remove' });
    assert.equal(h.state.overlay.chooser, null); assert.deepEqual(h.state.selection, ['window']);
    h.engine.choose('room', false, false);
    assert.deepEqual(h.state.selection, ['window'], 'a late stale-menu click cannot apply the old intent');
    assert.equal(h.state.commits.length, 0);
  } finally { h.close(); }
});
test('explicit Add and Remove modes apply to crossing rectangles', () => {
  for (const mode of ['add', 'remove']) {
    const h = harness(mode);
    try {
      const initial = ['window', 'room']; h.engine.update({ ...h.engine.props, selection: initial });
      h.pointer('pointerdown', { clientX: h.center[0] + 15, clientY: h.center[1] + 15 });
      h.pointer('pointermove', { clientX: h.center[0] - 15, clientY: h.center[1] - 15 });
      h.pointer('pointerup', { clientX: h.center[0] - 15, clientY: h.center[1] - 15 });
      assert.equal(h.state.selection.includes('room'), true, 'room envelope is not a rectangle target and must survive');
      assert.equal(h.state.selection.includes('window'), mode === 'add');
      assert.equal(h.state.commits.length, 0);
    } finally { h.close(); }
  }
});
test('selection mode does not replace physical Shift when navigating with the right mouse button', () => {
  for (const mode of ['replace', 'add', 'remove']) for (const shift of [false, true]) {
    const h = harness(mode, 'perspective');
    try {
      const view = h.engine.views.get('perspective'), beforeTarget = view.target.clone(), beforePosition = view.camera.position.clone();
      h.pointer('pointerdown', { button: 2, buttons: 2, shiftKey: shift, ctrlKey: true });
      h.pointer('pointermove', { button: 2, buttons: 2, shiftKey: shift, ctrlKey: true, clientX: h.center[0] + 30, clientY: h.center[1] - 20 });
      h.pointer('pointerup', { button: 2, clientX: h.center[0] + 30, clientY: h.center[1] - 20 });
      assert.notDeepEqual(view.camera.position.toArray(), beforePosition.toArray());
      assert.equal(view.target.equals(beforeTarget), !shift, `${mode}: physical Shift pans; no Shift orbits`);
      assert.deepEqual(h.state.selection, ['window']); assert.equal(h.state.commits.length, 0);
    } finally { h.close(); }
  }
});
test('Escape resets selection mode except in idle text fields, including when a chooser is open', () => {
  const h = harness('add');
  try {
    h.pointer('pointerdown'); h.pointer('pointerup');
    const inputEscape = event('keydown', { key: 'Escape', target: { tagName: 'INPUT' } });
    h.host.dispatchEvent(inputEscape);
    assert.equal(inputEscape.defaultPrevented, false); assert.equal(h.state.selectionMode, 'add');
    assert.ok(h.state.overlay.chooser);
    h.host.dispatchEvent(event('keydown', { key: 'Escape', target: h.pane }));
    assert.equal(h.state.selectionMode, 'replace'); assert.equal(h.state.overlay.chooser, null);
    assert.deepEqual(h.state.selection, ['window']);
    h.engine.update({ ...h.engine.props, selectionMode: 'remove' }); h.state.selectionMode = 'remove';
    h.host.dispatchEvent(event('keydown', { key: 'Escape', target: h.pane }));
    assert.equal(h.state.selectionMode, 'replace'); assert.deepEqual(h.state.selection, []);
  } finally { h.close(); }
});
test('Escape during a held Move drag clears preview, releases capture and cannot commit on later release', () => {
  const h = harness();
  try {
    h.beginDrag(); h.host.dispatchEvent(event('keydown', { key: 'Escape', target: h.pane }));
    assert.equal(h.state.candidate, null); assert.equal(h.pane.hasPointerCapture(1), false);
    h.pointer('pointermove', { clientX: h.center[0] + 70 }); h.pointer('pointerup');
    assert.equal(h.state.commits.length, 0); assert.deepEqual(h.state.selection, ['window']);
  } finally { h.close(); }
});
test('active viewport drag owns Escape even when focus is on a text input; idle text Escape stays native', () => {
  const h = harness(), input = { tagName: 'INPUT' };
  try {
    const idle = event('keydown', { key: 'Escape', target: input }); h.host.dispatchEvent(idle);
    assert.equal(idle.defaultPrevented, false); assert.deepEqual(h.state.selection, ['window']);
    h.beginDrag(); const escape = event('keydown', { key: 'Escape', target: input }); h.host.dispatchEvent(escape);
    assert.equal(h.state.candidate, null); assert.equal(escape.defaultPrevented, true);
    h.pointer('pointerup'); assert.equal(h.state.commits.length, 0);
  } finally { h.close(); }
});
test('an open overlap chooser does not steal Escape from a focused Properties text field', () => {
  const h = harness();
  try {
    h.pointer('pointerdown', { shiftKey: true }); h.pointer('pointerup', { shiftKey: true });
    const chooser = h.state.overlay.chooser; assert.ok(chooser);
    const escape = event('keydown', { key: 'Escape', target: { tagName: 'INPUT' } });
    h.host.dispatchEvent(escape);
    assert.equal(escape.defaultPrevented, false, 'the field must receive Escape to restore its accepted value');
    assert.deepEqual(h.state.overlay.chooser, chooser); assert.deepEqual(h.state.selection, ['window']);
    assert.equal(h.state.commits.length, 0);
  } finally { h.close(); }
});
test('pointer cancel, lost capture and window blur cancel provisional edits without a later commit', () => {
  for (const reason of ['pointercancel', 'lostpointercapture', 'blur']) {
    const h = harness();
    try {
      h.beginDrag();
      if (reason === 'blur') h.host.dispatchEvent(event('blur'));
      else if (reason === 'pointercancel') h.pointer('pointercancel');
      else h.pane.releasePointerCapture(1);
      assert.equal(h.state.candidate, null, reason); h.pointer('pointerup'); assert.equal(h.state.commits.length, 0, reason);
    } finally { h.close(); }
  }
});
test('ordinary Move release commits once despite synchronous lost-capture delivery', async () => {
  const h = harness();
  try {
    h.beginDrag(); const preview = structuredClone(h.state.candidate);
    h.pointer('pointerup', { clientX: h.center[0] + 35, clientY: h.center[1] - 20 });
    await Promise.resolve();
    assert.deepEqual(h.state.commits, [preview]); assert.equal(h.state.candidate, null);
    assert.equal(h.pane.hasPointerCapture(1), false);
    h.pointer('pointerup'); assert.equal(h.state.commits.length, 1);
  } finally { h.close(); }
});
