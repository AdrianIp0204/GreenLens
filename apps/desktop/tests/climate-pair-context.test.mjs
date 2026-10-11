import test from 'node:test';
import assert from 'node:assert/strict';
import { selectedPairProject, selectedPairRelation, snapshotDisplayContext, snapshotDisplayScale } from '../src/workspace/climate-pair-context.mjs';

const oldA={id:'old-a',alternative_id:'A',record_sha256:'old-seal',physical_input_hash:'same-input'};
const newA={...oldA,id:'new-a',record_sha256:'different-series-seal'};
const b={...oldA,id:'b',alternative_id:'B'};
const project={id:'project',alternatives:[{id:'A',model:{width:4}},{id:'B',model:{width:4}}],climate:{weather:[{id:'retained-weather'}],studies:{A:{threshold:300},B:{threshold:300}},runs:[oldA,b,newA]}};

test('historical relationship lookup excludes newer raw series without removing retained evidence',()=>{
  const before=structuredClone(project);
  const selected=selectedPairProject(project,oldA,b);
  assert.deepEqual(selected.climate.runs,[oldA,b]);
  assert.deepEqual(selected.alternatives,project.alternatives);
  assert.deepEqual(selected.climate.studies,project.climate.studies);
  assert.deepEqual(selected.climate.weather,project.climate.weather);
  assert.deepEqual(project,before);
  assert.notEqual(selected,project);assert.notEqual(selected.climate,project.climate);
  const changed=selectedPairProject(project,newA,b);
  assert.notEqual(JSON.stringify(changed.climate.runs),JSON.stringify(selected.climate.runs));
  assert.notEqual(JSON.stringify(selectedPairProject(project,{...oldA,record_sha256:'resealed'},b).climate.runs),JSON.stringify(selected.climate.runs));
});

test('empty comparison side never falls back to a newer run for relation lookup',()=>{
  assert.deepEqual(selectedPairProject(project,null,b).climate.runs,[b]);
  assert.deepEqual(selectedPairProject(project,null,null).climate.runs,[]);
  assert.equal(selectedPairProject(null,oldA,b),null);
  assert.equal(selectedPairProject({id:'legacy'},null,null),null);
});

test('relationship uses Rust selected-record metric currentness, not another result checksum',()=>{
  // Mock Rust responses test presentation only. Physical/metric identity
  // correctness has a separate native Rust fixture; no physics is computed here.
  const selected={physical_input_hash:'same-input',stored_metric_identity:'old-series+300',requested_metric_identity:'old-series+300',metrics_current:true};
  assert.doesNotMatch(selectedPairRelation(oldA,selected),/SETTINGS DIFFER/);
  assert.match(selectedPairRelation(oldA,{...selected,requested_metric_identity:'old-series+500',metrics_current:false}),/METRIC SETTINGS DIFFER/);
  assert.match(selectedPairRelation(oldA,{...selected,physical_input_hash:'edited'}),/CURRENT-STUDY MISMATCH/);
  assert.match(selectedPairRelation(oldA,undefined),/Checking/);
  assert.equal(selectedPairRelation(oldA,undefined,'Verified record unavailable'),'Verified record unavailable');
  assert.equal(selectedPairRelation(null,selected),'No selected snapshot.');
});

test('annual comparison navigation stays independent of current selected-hour or absent result',()=>{
  const main={study:{calculation:{mode:'weather_hour',interval_index:7,weather:{original_sha256:'weather-Y'}}}};
  const pair={study:{calculation:{mode:'annual',weather:{original_sha256:'weather-X'}}}};
  const mainRequest={field:'da',interval:30},pairRequest={field:'illuminance',interval:1908};
  assert.deepEqual(snapshotDisplayContext(main,mainRequest),{annual:false,field:'illuminance',interval:7});
  assert.deepEqual(snapshotDisplayContext(pair,pairRequest),{annual:true,field:'illuminance',interval:1908});
  assert.deepEqual(snapshotDisplayContext(pair,{...pairRequest,field:'cda',interval:8759}),{annual:true,field:'cda',interval:8759});
  assert.deepEqual(snapshotDisplayContext(main,mainRequest),{annual:false,field:'illuminance',interval:7});
  assert.equal(snapshotDisplayContext(null,mainRequest).annual,false);
  assert.equal(snapshotDisplayContext(pair,pairRequest).annual,true);
  assert.equal(main.study.calculation.weather.original_sha256,'weather-Y');
  assert.equal(pair.study.calculation.weather.original_sha256,'weather-X');
});

test('main legend/viewport scale uses only that snapshot and metric units remain percent',()=>{
  const main={statistics:{max:50}},pair={statistics:{max:500}};
  assert.equal(snapshotDisplayScale('illuminance',main),50);
  assert.equal(snapshotDisplayScale('illuminance',pair),500);
  assert.equal(snapshotDisplayScale('illuminance',main),50);
  assert.equal(snapshotDisplayScale('da',main),100);
  assert.equal(snapshotDisplayScale('illuminance',{statistics:{max:0}}),1);
});
