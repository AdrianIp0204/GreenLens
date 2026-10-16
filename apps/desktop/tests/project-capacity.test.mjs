// Real component + request lifecycle; IPC doubles are not native resource evidence.
import test, { afterEach } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { JSDOM } from 'jsdom';
import { transformWithOxc } from 'vite';
const dom = new JSDOM('<div id="root"></div>');
for (const name of ['window', 'document', 'HTMLElement', 'Event', 'MouseEvent']) globalThis[name] = dom.window[name];
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
const React = await import('react'), { act } = React, { createRoot } = await import('react-dom/client');
const cache = new Map();
async function load(file) {
  if (cache.has(file)) return cache.get(file);
  const promise = (async () => {
    const { code } = await transformWithOxc(fs.readFileSync(file, 'utf8'), file, { jsx: { runtime: 'automatic' } }); let linked = code;
    for (const m of [...code.matchAll(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g)]) {
      let target;
      if (m[3] === '@tauri-apps/api/core') target = 'data:text/javascript,' + encodeURIComponent('export const invoke=(...args)=>globalThis.__capacityIPC(...args); export class Channel {}');
      else if (m[3] === '../workspace/useDocument') target = 'data:text/javascript,export const errorText=e=>String(e);';
      else if (m[3].startsWith('.')) { let f = path.resolve(path.dirname(file), m[3]); if (!path.extname(f)) f = ['.ts', '.tsx', '.mjs'].map(ext => f + ext).find(fs.existsSync); target = f.endsWith('.mjs') ? pathToFileURL(f).href : await load(f); }
      else target = import.meta.resolve(m[3]);
      linked = linked.replace(m[0], `import ${m[1]} from ${JSON.stringify(target)};`);
    }
    return 'data:text/javascript;base64,' + Buffer.from(linked).toString('base64');
  })(); cache.set(file, promise); return promise;
}
const { default: Capacity } = await import(await load(fileURLToPath(new URL('../src/energy/ProjectCapacity.tsx', import.meta.url))));
function report(patch = {}) { return { schema_version: 'greenlens.project-capacity.v1', project_content_sha256: 'a'.repeat(64), retained_bytes: 230000000, retained_entries: 687, available_bytes: 38435456, available_entries: 4313, limits: [{ name: 'complete retained bytes', used: 230000000, limit: 268435456 }, { name: 'issue count', used: 2, limit: 16 }, { name: 'scope revision count', used: 2, limit: 16 }, { name: 'reference normalized bytes per asset', used: 123456, limit: 83886080 }], incremental_lower_bound_bytes: 0, incremental_lower_bound_entries: 4, estimated_incremental_bytes: null, estimate_basis: 'Final report products are unknown before rendering.', can_start: true, warnings: [], inventory_scope: 'Actual sizes of manifest-bound files. This is not content-integrity verification.', remedy: 'Preserve the complete study and start a separately scoped document; no history is pruned.', ...patch }; }
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
let root, props, calls, answer;
async function render() { await act(async () => root.render(React.createElement(Capacity, props))); }
async function setup() { document.body.innerHTML = '<div id="root"></div>'; root = createRoot(document.getElementById('root')); calls = []; answer = async () => report(); props = { path: 'owned-test-project', project: { id: 'original', energy: { runs: [] } }, operation: { kind: 'report_issue' } }; globalThis.__capacityIPC = async (command, args) => { calls.push([command, args]); return answer(args); }; }
afterEach(async () => { if (root) await act(async () => root.unmount()); root = null; });
const refresh = () => document.querySelector('button');

test('real capacity panel sends exact operation and displays backend numbers with uncertainty and no integrity claim', async () => {
  await setup(); await render(); assert.deepEqual(calls, [['project_capacity', { path: props.path, project: props.project, operation: props.operation }]]);
  assert.match(document.body.textContent, /230,000,000 B/); assert.match(document.body.textContent, /123,456 B/); assert.match(document.body.textContent, /4,313/); assert.match(document.body.textContent, /Unknown; no reliable total estimate/); assert.match(document.body.textContent, /not evidence-integrity verification/); assert.match(document.body.textContent, /Final publication and export still require exact admission/);
  assert.doesNotMatch(document.body.textContent, /Evidence verified|Guaranteed to fit|all checks passed/);
});

test('backend rejection and estimate warning remain distinct without a local capacity decision', async () => {
  await setup(); answer = async () => report({ can_start: false, incremental_lower_bound_bytes: 42000000, estimated_incremental_bytes: 50000000, warnings: ['Declared lower bound exceeds remaining capacity.'] }); await render();
  assert.match(document.body.textContent, /Cannot start: a known capacity restriction/); assert.match(document.body.textContent, /42,000,000 B/); assert.match(document.body.textContent, /estimate, not an admission guarantee/); assert.match(document.body.textContent, /no history is pruned/);
  answer = async () => report({ can_start: true, estimated_incremental_bytes: 50000000, warnings: ['Estimate exceeds allowance; exact publication will decide.'] }); await act(async () => refresh().click());
  assert.match(document.body.textContent, /Known capacity checks permit starting/); assert.match(document.body.textContent, /Estimate exceeds allowance/); assert.equal(calls.length, 2);
});

test('pending and failed inventory never leave an old capacity report visible; refresh is explicit', async () => {
  await setup(); const pending = deferred(); answer = () => pending.promise; await render(); assert.equal(refresh().disabled, true); assert.match(document.body.textContent, /Reading bounded retained-file inventory/); assert.equal(calls.length, 1);
  await act(async () => pending.reject(Error('Missing retained member'))); assert.match(document.body.textContent, /Capacity inspection failed.*Missing retained member/); assert.doesNotMatch(document.body.textContent, /Known capacity checks permit/); assert.equal(refresh().disabled, false); assert.equal(calls.length, 1);
  answer = async () => report(); await act(async () => refresh().click()); assert.equal(calls.length, 2); assert.match(document.body.textContent, /230,000,000 B/);
});

test('document and operation changes hide late inventory from the previous exact request', async () => {
  await setup(); const previous = deferred(), current = deferred(); answer = args => args.operation.kind === 'report_issue' ? previous.promise : current.promise; await render();
  props = { ...props, path: 'another-owned-project', project: { id: 'second' }, operation: { kind: 'energy_run', alternative: 'B' } }; await render();
  await act(async () => current.resolve(report({ retained_bytes: 123, project_content_sha256: 'b'.repeat(64), limits: [{ name: 'complete retained bytes', used: 123, limit: 268435456 }] }))); assert.match(document.body.textContent, /123 B/);
  await act(async () => previous.resolve(report())); assert.doesNotMatch(document.body.textContent, /230,000,000 B/); assert.match(document.body.textContent, /Energy run · B/);
  props = { ...props, project: null }; await render(); assert.match(document.body.textContent, /Select a project/); assert.doesNotMatch(document.body.textContent, /123 B/); assert.equal(calls.length, 2);
});

test('malformed bounds or unknown response versions cannot be presented as a valid inventory', async () => {
  await setup();
  for (const patch of [{ schema_version: 'unknown' }, { retained_bytes: Infinity }, { limits: [{ name: 'broken', used: -1, limit: 20 }] }, { warnings: 'not an array' }, { can_start: 'yes' }]) {
    answer = async () => report(patch); if (!calls.length) await render(); else await act(async () => refresh().click());
    assert.match(document.body.textContent, /response is malformed or unsupported/); assert.doesNotMatch(document.body.textContent, /Known capacity checks permit/);
  }
});
