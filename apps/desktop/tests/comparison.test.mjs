import test from 'node:test';
import assert from 'node:assert/strict';
import { historicalSelection, selectedHistoricalRuns, chooseHistoricalRun, comparisonKey, snapshotRelation, ComparisonRequestGate } from '../src/workspace/comparison.mjs';
import { newestCompatibleResult } from '../src/presentation.mjs';

const run=(id,alternative_id,hash=id)=>({id,alternative_id,validated:{scientific_input_hash:hash},lux:[10]});
const a1=run('a-old','A','original'),a2=run('a-new','A','edited'),b1=run('b-old','B'),b2=run('b-new','B');
const records=[a1,b1,a2,b2];
const result=(a,b)=>({a_id:a.id,b_id:b.id,deltas_lux:[4],scale_max_lux:14,conditions:{sensor_order:'unchanged'}});
const deferred=()=>{let resolve,reject;const promise=new Promise((ok,no)=>{resolve=ok;reject=no;});return {promise,resolve,reject};};

test('historical pair defaults once per document and explicit IDs survive result arrival and undo',()=>{
  let selection=historicalSelection(1,records,null);
  assert.deepEqual(selectedHistoricalRuns(records,selection),{a:a2,b:b2});
  selection=chooseHistoricalRun(records,selection,'A',a1.id);
  const arriving=[...records,run('a-third','A','third'),run('b-third','B')];
  const retained=historicalSelection(1,arriving,selection);
  assert.deepEqual(retained,selection);assert.deepEqual(selectedHistoricalRuns(arriving,retained),{a:a1,b:b2});
  assert.equal(newestCompatibleResult({scientific_input_hash:'edited'},records,'A'),a2);
  assert.equal(newestCompatibleResult({scientific_input_hash:'original'},records,'A'),a1);
  assert.deepEqual(historicalSelection(1,records,selection),selection);
  const reopened=historicalSelection(2,arriving,selection);
  assert.equal(reopened.aId,'a-third');assert.equal(reopened.bId,'b-third');
  assert.notEqual(comparisonKey(reopened),comparisonKey(selection));
});
test('first completed run fills an empty side without replacing an existing selection',()=>{
  const empty=historicalSelection(1,[],null),first=historicalSelection(1,[a1],empty);
  assert.deepEqual(first,{documentId:1,aId:a1.id,bId:null});
  assert.deepEqual(historicalSelection(1,[a1,a2,b1],first),{documentId:1,aId:a1.id,bId:b1.id});
});
test('unknown or wrong-alternative choices cannot silently replace a selected historical pair',()=>{
  const selection=historicalSelection(1,records,null);
  assert.throws(()=>chooseHistoricalRun(records,selection,'A',b1.id),/indicated alternative/);
  assert.throws(()=>chooseHistoricalRun(records,selection,'A','missing'),/retained snapshot/);
  const missingRecords=records.filter(r=>r.id!==a2.id),retained=historicalSelection(1,missingRecords,selection);
  assert.equal(retained.aId,a2.id);assert.equal(selectedHistoricalRuns(missingRecords,retained).a,null);
});
test('source labels distinguish known mismatch from pending current-model validation',()=>{
  assert.match(snapshotRelation(a1,'original'),/^Matches/);
  assert.match(snapshotRelation(a1,'edited'),/^Current-design mismatch/);
  assert.match(snapshotRelation(a1,undefined),/pending or unavailable/);
});
test('pair response guard discards late success and preserves backend scale, order and differences',async()=>{
  const gate=new ComparisonRequestGate(),old=deferred(),fresh=deferred();
  const oldRequest=gate.compare(a1,b1,()=>old.promise),newRequest=gate.compare(a2,b2,()=>fresh.promise);
  const expected=result(a2,b2);fresh.resolve(expected);assert.deepEqual(await newRequest,{comparison:expected,error:''});
  old.resolve(result(a1,b1));assert.equal(await oldRequest,null);
});
test('pair response guard discards late failures and pending responses after document replacement',async()=>{
  const gate=new ComparisonRequestGate(),old=deferred();
  const pending=gate.compare(a1,b1,()=>old.promise);gate.cancel();old.reject(new Error('obsolete backend failure'));assert.equal(await pending,null);
  const next=deferred(),pendingNext=gate.compare(a2,b2,()=>next.promise);gate.cancel();next.resolve(result(a2,b2));assert.equal(await pendingNext,null);
});
test('incompatible selected pair fails visibly without requesting a different pair',async()=>{
  const gate=new ComparisonRequestGate(),calls=[];
  const response=await gate.compare(a1,b2,async(a,b)=>{calls.push([a.id,b.id]);throw 'Different sensor definitions; no resampling.';});
  assert.deepEqual(calls,[[a1.id,b2.id]]);assert.equal(response.comparison,null);assert.match(response.error,/Different sensor definitions/);
});
test('backend response identifying another pair is never displayed as the selected pair',async()=>{
  const response=await new ComparisonRequestGate().compare(a1,b1,async()=>result(a2,b2));
  assert.equal(response.comparison,null);assert.match(response.error,/does not match the selected snapshots/);
});
