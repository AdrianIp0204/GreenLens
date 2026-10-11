import test from 'node:test';
import assert from 'node:assert/strict';
import { visibleResult, newestCompatibleResult, luxColor, activeJob } from '../src/presentation.mjs';

test('historical illuminance cannot appear on modified or pending geometry', () => {
  const run = { validated: { scientific_input_hash: 'original' }, lux: [10, 20] };
  assert.equal(visibleResult({ scientific_input_hash: 'edited' }, run), null);
  assert.equal(visibleResult({ scientific_input_hash: 'original' }, run, true), null);
  assert.equal(visibleResult(null, run), null);
  assert.equal(visibleResult({ scientific_input_hash: 'original' }, run), run);
});

test('undo finds the newest matching run while later mismatched evidence remains explicitly inspectable', () => {
  const run=(id,alternative,hash)=>({id,alternative_id:alternative,validated:{scientific_input_hash:hash},lux:[10]});
  const original=run('original-first','A','original'),newerOriginal=run('original-second','A','original');
  const edited=run('edited','A','edited'),otherAlternative=run('other','B','original');
  const records=[original,newerOriginal,edited,otherAlternative],snapshot=structuredClone(records);
  assert.equal(newestCompatibleResult({scientific_input_hash:'original'},records,'A'),newerOriginal);
  assert.equal(newestCompatibleResult({scientific_input_hash:'edited'},records,'A'),edited);
  assert.equal(newestCompatibleResult({scientific_input_hash:'original'},records,'B'),otherAlternative);
  assert.equal(newestCompatibleResult({scientific_input_hash:'unrun'},records,'A'),null);
  assert.equal(newestCompatibleResult({scientific_input_hash:'original'},records,'A',true),null);
  assert.equal(newestCompatibleResult(null,records,'A'),null);
  // Explicit evidence inspection and the latest historical pair are independent
  // of the current-model lookup; no history record is removed or reordered.
  assert.equal(records.find(r=>r.id==='edited'),edited);
  assert.deepEqual(records,snapshot);
});

test('current-result lookup accepts caller-owned study compatibility without computing new identities', () => {
  const validated={scientific_input_hash:'same-model'};
  const records=['study-a','study-b','study-a','study-b'].map((study,id)=>({id,alternative_id:'A',validated,study}));
  assert.equal(newestCompatibleResult(validated,records,'A',false,run=>run.study==='study-a'),records[2]);
  assert.equal(newestCompatibleResult(validated,records,'A',false,()=>false),null);
  assert.equal(newestCompatibleResult(validated,records,'A'),records[3]);
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
