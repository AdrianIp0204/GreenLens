import test from 'node:test';
import assert from 'node:assert/strict';
import { visibleResult, luxColor, activeJob } from '../src/presentation.mjs';

test('historical illuminance cannot appear on modified or pending geometry', () => {
  const run = { validated: { scientific_input_hash: 'original' }, lux: [10, 20] };
  assert.equal(visibleResult({ scientific_input_hash: 'edited' }, run), null);
  assert.equal(visibleResult({ scientific_input_hash: 'original' }, run, true), null);
  assert.equal(visibleResult(null, run), null);
  assert.equal(visibleResult({ scientific_input_hash: 'original' }, run), run);
});
test('the same lux value uses the same shared A/B scale and color', () => {
  assert.equal(luxColor(50, 100), 'rgb(79,171,139)');
  assert.equal(luxColor(100, 200), 'rgb(79,171,139)');
  assert.notEqual(luxColor(50, 100), luxColor(50, 200));
  assert.equal(luxColor(0, 0), 'rgb(28,57,83)');
});
test('terminal failure and cancellation release controls without implying completion', () => {
  assert.equal(activeJob({ state: 'preparing' }), true);
  assert.equal(activeJob({ state: 'running' }), true);
  assert.equal(activeJob({ state: 'failed' }), false);
  assert.equal(activeJob({ state: 'cancelled' }), false);
});
