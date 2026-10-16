import test, { afterEach } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { JSDOM } from 'jsdom';
import { transformWithOxc } from 'vite';
import { indexOccurrences, occurrenceAncestors, outlinerRows } from '../src/reference/reference-outliner.mjs';

const dom = new JSDOM('<div id="root"></div>', { url: 'https://greenlens.local/' });
for (const name of ['window', 'document', 'HTMLElement', 'HTMLInputElement', 'Event', 'MouseEvent', 'KeyboardEvent']) globalThis[name] = dom.window[name];
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
const React = await import('react'), { act } = React, { createRoot } = await import('react-dom/client');
const modules = new Map();
async function load(filename) {
  if (modules.has(filename)) return modules.get(filename);
  const task = (async () => {
    const { code } = await transformWithOxc(fs.readFileSync(filename, 'utf8'), filename, { jsx: { runtime: 'automatic' } });
    let linked = code.replace(/import\s+['"][^'"]+\.css['"];?/g, '');
    for (const m of [...code.matchAll(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g)]) {
      let target;
      if (m[3].startsWith('.')) { let resolved = path.resolve(path.dirname(filename), m[3]); if (!path.extname(resolved)) resolved = ['.ts', '.tsx', '.mjs'].map(ext => resolved + ext).find(fs.existsSync); target = await load(resolved); }
      else target = import.meta.resolve(m[3]);
      linked = linked.replace(m[0], `import ${m[1]} from ${JSON.stringify(target)};`);
    }
    return `data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`;
  })(); modules.set(filename, task); return task;
}
const { default: Outliner } = await import(await load(fileURLToPath(new URL('../src/reference/ReferenceOutliner.tsx', import.meta.url))));
const occurrence = (id, parent_id = null, label = 'Repeated source name', mesh = 7) => ({ id, parent_id, label, node_index: Number(id.replace(/\D/g, '')) || 0, node_path: [0, Number(id.replace(/\D/g, '')) || 0], source_mesh_index: mesh, mirrored: false, bounds: { min_m: [0, 0, 0], max_m: [1, 1, 1] }, triangle_count: 2, triangle_area_m2: 1, world_from_mesh: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1] });
function fixture(count = 205) { return { revision_sha256: 'a'.repeat(64), occurrences: [occurrence('root-0', null, 'Ground 空間', null), ...Array.from({ length: count }, (_, i) => occurrence(`child-${i + 1}`, 'root-0'))] }; }
let root, props, calls;
async function render() { await act(async () => root.render(React.createElement(Outliner, props))); }
async function setup(reference = fixture()) {
  calls = []; document.body.innerHTML = '<div id="root"></div>'; root = createRoot(document.getElementById('root'));
  props = { documentKey: 'study:A', reference, selectedOccurrenceId: null, onSelectOccurrence: id => { calls.push(['select', id]); props = { ...props, selectedOccurrenceId: id }; root.render(React.createElement(Outliner, props)); }, onFrameSelected: () => calls.push(['frame']) }; await render();
}
afterEach(async () => { if (root) await act(async () => root.unmount()); root = null; });
const button = text => [...document.querySelectorAll('button')].find(item => item.textContent === text);
const row = id => [...document.querySelectorAll('button[data-occurrence-id]')].find(item => item.dataset.occurrenceId === id);
async function click(element) { assert.ok(element); await act(async () => element.dispatchEvent(new MouseEvent('click', { bubbles: true }))); }
async function search(value) { const input = document.querySelector('input'); await act(async () => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, value); input.dispatchEvent(new Event('input', { bubbles: true })); }); }

test('display index uses exact parent IDs and mesh reuse, never repeated labels as identity', () => {
  const model = fixture(2); model.occurrences.push(occurrence('nested-3', 'child-2', '小窓')); const before = JSON.stringify(model);
  const index = indexOccurrences(model.occurrences);
  assert.deepEqual(occurrenceAncestors(index, 'nested-3').map(item => item.id), ['root-0', 'child-2', 'nested-3']);
  assert.deepEqual(outlinerRows(index, '小窓', new Set()).map(item => item.occurrence.id), ['nested-3']);
  assert.equal(outlinerRows(index, 'Ground', new Set(['root-0'])).length, 4);
  assert.equal(index.meshUses.get(7), 3); assert.equal(JSON.stringify(model), before);
});

test('real hierarchy pages every occurrence and selection distinguishes duplicate names', async () => {
  await setup(); const before = JSON.stringify(props.reference), seen = [];
  for (let i = 0; i < 3; i++) { seen.push(...[...document.querySelectorAll('button[data-occurrence-id]')].map(item => item.dataset.occurrenceId)); if (i < 2) await click(button('Next occurrences')); }
  assert.equal(seen.length, 206); assert.equal(new Set(seen).size, 206); assert.equal(button('Next occurrences').disabled, true);
  await click(row('child-205')); assert.equal(props.selectedOccurrenceId, 'child-205'); assert.equal(row('child-205').getAttribute('aria-pressed'), 'true');
  assert.match(document.querySelector('nav').textContent, /Ground 空間/); assert.match(document.body.textContent, /205 occurrence\(s\) use this mesh/);
  assert.equal(JSON.stringify(props.reference), before);
});

test('search includes ancestor paths; reveal expands and finds controlled selected row outside the current page', async () => {
  await setup(); props = { ...props, selectedOccurrenceId: 'child-205' }; await render();
  await click(document.querySelector('[aria-label="Collapse node 0"]')); assert.equal(document.querySelectorAll('button[data-occurrence-id]').length, 1);
  await search('not found'); assert.equal(document.querySelectorAll('button[data-occurrence-id]').length, 0);
  await click(button('Reveal selected')); assert.ok(row('child-205')); assert.equal(document.activeElement, row('child-205')); assert.equal(document.querySelector('input').value, '');
  await search('Ground 空間'); assert.equal(document.querySelectorAll('button[data-occurrence-id]').length, 100); assert.match(document.body.textContent, /206 matching occurrences/);
  assert.equal(calls.length, 0);
});

test('framing only uses the current admitted selection and an absent historical identity is never substituted', async () => {
  await setup(); props = { ...props, selectedOccurrenceId: 'historical-unknown' }; await render();
  assert.equal(button('Frame selected').disabled, true); assert.equal(button('Reveal selected').disabled, true); assert.match(document.body.textContent, /absent from this active reference/);
  props = { ...props, selectedOccurrenceId: 'child-2' }; await render(); await click(button('Frame selected')); assert.deepEqual(calls, [['frame']]);
  props = { ...props, reference: { ...fixture(2), revision_sha256: 'b'.repeat(64), occurrences: [occurrence('new-1')] } }; await render();
  assert.equal(button('Frame selected').disabled, true); assert.equal(document.querySelectorAll('[aria-pressed="true"]').length, 0);
});

test('Escape clears controlled active selection without resurrecting it or invoking framing', async () => {
  await setup(fixture(2)); await click(row('child-2')); row('child-2').focus();
  await act(async () => row('child-2').dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })));
  assert.equal(props.selectedOccurrenceId, null); assert.equal(document.querySelector('nav'), null); await search('Ground'); await render();
  assert.equal(document.querySelectorAll('[aria-pressed="true"]').length, 0); assert.deepEqual(calls, [['select', 'child-2'], ['select', null]]);
});

test('breadcrumb keyboard clearing targets active selection while text-field Escape retains native editing', async () => {
  await setup(fixture(2)); await click(row('child-2')); const input = document.querySelector('input'); input.focus();
  await act(async () => input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })));
  assert.equal(props.selectedOccurrenceId, 'child-2'); const crumb = document.querySelector('[aria-current="location"]'); crumb.focus();
  await act(async () => crumb.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })));
  assert.equal(props.selectedOccurrenceId, null); assert.deepEqual(calls, [['select', 'child-2'], ['select', null]]);
});

test('navigation never mutates accepted input and resets only for another document/design or revision', async () => {
  await setup(fixture(2)); const before = JSON.stringify(props.reference); await search('child-2');
  props = { ...props, disabled: true }; await render(); assert.equal(document.querySelector('input').value, 'child-2'); assert.equal(row('child-2').disabled, true);
  props = { ...props, disabled: false, documentKey: 'study:B' }; await render(); assert.equal(document.querySelector('input').value, '');
  await search('Ground'); props = { ...props, reference: { ...props.reference, revision_sha256: 'c'.repeat(64) } }; await render(); assert.equal(document.querySelector('input').value, '');
  assert.equal(JSON.stringify({ ...props.reference, revision_sha256: 'a'.repeat(64) }), before); assert.deepEqual(calls, []);
});
