import test from 'node:test';
import assert from 'node:assert/strict';
import { ClimateRequestGate, climateRequestKey, climateOverlayMatches, climateSeriesSensor, displayedMetricSettings, metricConvention, hourLabel } from '../src/workspace/climate-presentation.mjs';
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
test('hour scrubbing discards obsolete reads and failures',async()=>{
  const gate=new ClimateRequestGate(),old=deferred(),fresh=deferred();
  const stale=gate.read(()=>old.promise),current=gate.read(()=>fresh.promise);
  fresh.resolve({interval:2});assert.deepEqual(await current,{value:{interval:2},error:''});
  old.reject('old failure');assert.equal(await stale,null);
  const closing=deferred(),pending=gate.read(()=>closing.promise);gate.cancel();closing.resolve({interval:3});assert.equal(await pending,null);
});
test('snapshot or field mismatch never reaches display',async()=>{
  const result=await new ClimateRequestGate().read(async()=>({run_id:'wrong'}),value=>value.run_id==='expected');
  assert.equal(result.value,null);assert.match(result.error,/does not match/);
});
test('request identity changes for hours, immutable records, settings and projects',()=>{
  const summary={id:'a',record_sha256:'seal'};
  const key=climateRequestKey('project',summary,'da',0,{da_threshold_lux:300});
  for(const alternative of [climateRequestKey('copy',summary,'da',0,{da_threshold_lux:300}),climateRequestKey('project',{...summary,record_sha256:'other'},'da',0,{da_threshold_lux:300}),climateRequestKey('project',summary,'da',1,{da_threshold_lux:300}),climateRequestKey('project',summary,'da',0,{da_threshold_lux:500})])assert.notEqual(alternative,key);
  assert.equal(climateRequestKey('project',null,'da',0,null),'');
});
test('historical values cannot overlay changed geometry or reordered sensors',()=>{
  const summary={id:'a',record_sha256:'seal',physical_input_hash:'science'},current={physical_input_hash:'science',summary,run:{id:'a'}},values={run_id:'a',sensor_ids:['s1','s2']},validated={sensor_grid:{sensors:[{id:'s1'},{id:'s2'}]}};
  assert.equal(climateOverlayMatches(summary,current,values,validated),true);
  assert.equal(climateOverlayMatches(summary,{physical_input_hash:'edited'},values,validated),false);
  assert.equal(climateOverlayMatches(summary,current,{...values,sensor_ids:['s2','s1']},validated),false);
  assert.equal(climateOverlayMatches(summary,current,{...values,run_id:'other'},validated),false);
});
test('hour24 is displayed as interval end without OS date conversion',()=>{
  assert.equal(hourLabel({hour:{month:12,day:31,hour_ending:24}}),'12/31 · 23:00–24:00 · midpoint 23:30 local standard time');
});

test('metric captions bind to the selected snapshot or current derived settings',()=>{
  const saved={da_threshold_lux:300,udi_lower_lux:100,udi_upper_lux:3000,occupancy:{kind:'daily',start_minute:480,end_minute:1080}};
  const edited={...saved,da_threshold_lux:500,udi_upper_lux:2000,occupancy:{kind:'daily',start_minute:555,end_minute:1440}};
  const run={study:{metrics:saved}},study={metrics:edited};
  assert.equal(displayedMetricSettings('current',study,run),edited);
  assert.equal(displayedMetricSettings('old-A',study,run),saved);
  assert.equal(displayedMetricSettings('old-A',null,run),saved);
  assert.equal(displayedMetricSettings('current',null,null),null);
  assert.equal(metricConvention('da',edited),'DA: E ≥ 500 lx. Occupancy: every day 09:15–24:00 local standard time; occupied nighttime is included.');
  for(const [field,boundary] of [['cda','cDA: E / (300 lx), capped at 1'],['udi','UDI: 100 lx ≤ E ≤ 3000 lx'],['underlit','Underlit: E < 100 lx'],['overlit','Overlit: E > 3000 lx']])assert.equal(metricConvention(field,saved),`${boundary}. Occupancy: every day 08:00–18:00 local standard time; occupied nighttime is included.`);
  assert.match(metricConvention('da',{...saved,occupancy:{kind:'daily',start_minute:1335,end_minute:390}}),/22:15–06:30 local standard time \(across midnight\)/);
  assert.match(metricConvention('udi',{...saved,occupancy:{kind:'mask',weights:Array(8760).fill(.5)}}),/explicit 8,760 interval weights/);
  assert.equal(metricConvention('illuminance',saved),'');assert.equal(metricConvention('da',null),'');
});

test('selected older snapshot overlays only when Rust confirms that exact record',()=>{
  const summary={id:'older',record_sha256:'old-seal',physical_input_hash:'same-role'};
  const values={run_id:'older',sensor_ids:['s1']},validated={sensor_grid:{sensors:[{id:'s1'}]}};
  const rejected={physical_input_hash:'same-role',summary:null,run:null};
  assert.equal(climateOverlayMatches(summary,rejected,values,validated),false);
  const newest={...summary,id:'newest',record_sha256:'new-seal'};
  assert.equal(climateOverlayMatches(summary,{...rejected,summary:newest,run:{id:'newest'}},values,validated),false);
  assert.equal(climateOverlayMatches(summary,{...rejected,summary,run:{id:'older'}},values,validated),true);
  assert.equal(climateOverlayMatches(summary,{...rejected,summary:{...summary,record_sha256:'modified'},run:{id:'older'}},values,validated),false);
});

test('historical series never inherits a same-ID sensor at a different current geometry position',()=>{
  const ids=['s000001','s000002'];
  assert.equal(climateSeriesSensor(false,['sensor:s000001'],null,ids),null);
  assert.equal(climateSeriesSensor(false,['sensor:s000001'],'s000002',ids),'s000002');
  assert.equal(climateSeriesSensor(true,['sensor:s000001'],'s000002',ids),'s000001');
  assert.equal(climateSeriesSensor(false,['sensor:s000001'],'missing',ids),null);
  assert.equal(climateSeriesSensor(true,['sensor:missing'],'s000002',ids),null);
});
