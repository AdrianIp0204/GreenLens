import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { DocumentHistory, documentSignature, studySnapshot, completedJobResult, hasPendingEvidence, protectReplacement } from '../src/workspace/document.mjs';

// Original mocked service replies test UI transaction ordering only. Rust tests
// independently validate weather, study restoration, metrics and engine evidence.
const fixture=JSON.parse(await readFile(new URL('./viewport-fixture.json',import.meta.url),'utf8'));
const model=fixture.model;
const staticStudy=()=>({schema_version:'greenlens.study.v0.3',calculation:{mode:'static_overcast'},metrics:{da_threshold_lux:300,udi_lower_lux:100,udi_upper_lux:3000,occupancy:{kind:'daily',start_minute:480,end_minute:1080}}});
const legacy=()=>({schema_version:'0.1.0',id:'mock-document',name:'Test',alternatives:['A','B'].map(id=>({id,model:structuredClone(model)})),runs:[]});
const withStudy=(project,study,alternative='A')=>({...structuredClone(project),schema_version:'0.3.0',climate:{schema_version:'greenlens.climate-project.v0.3',weather:[],runs:[],studies:{A:staticStudy(),B:staticStudy(),[alternative]:structuredClone(study)}}});
const validate=async candidate=>({...structuredClone(fixture),model:structuredClone(candidate)});

test('first study transaction restores absent legacy schema on undo and exact study on redo',async()=>{
  const original=legacy(),editedStudy=staticStudy();editedStudy.metrics.da_threshold_lux=500;
  const edited=withStudy(original,editedStudy),history=new DocumentHistory(),calls=[];
  const result=await history.editStudy(original,'A',editedStudy,'DA threshold',async(p,alt,study)=>{
    calls.push('configure');assert.deepEqual(p,original);assert.equal(alt,'A');assert.deepEqual(study,editedStudy);return edited;
  });
  assert.equal(history.past[0].kind,'study');assert.equal(history.past.length,1);
  assert.deepEqual(history.past[0].before,{project_schema_version:'0.1.0',study:null});
  assert.equal(original.climate,undefined);assert.notEqual(documentSignature(result.project),documentSignature(original));
  const restore=async(p,alt,snapshot)=>{
    assert.equal(alt,'A');calls.push(snapshot.study?'redo':'undo');
    assert.deepEqual(p,snapshot.study?original:edited);
    return structuredClone(snapshot.study?edited:original);
  };
  const undone=await history.traverse(result.project,'undo',validate,undefined,restore);
  assert.equal(documentSignature(undone.project),documentSignature(original));
  const redone=await history.traverse(undone.project,'redo',validate,undefined,restore);
  assert.deepEqual(redone.project,edited);assert.deepEqual(calls,['configure','undo','redo']);
});

test('study undo sends the latest document to Rust and never rolls back newly collected evidence or imported weather',async()=>{
  const original=legacy(),study=staticStudy();study.metrics.udi_upper_lux=2000;
  const edited=withStudy(original,study,'B'),history=new DocumentHistory();
  await history.editStudy(original,'B',study,'B metrics',async()=>edited);
  const later=structuredClone(edited);later.runs.push({id:'later-static'});later.climate.runs.push({id:'later-annual'});later.climate.weather.push({id:'later-weather'});
  later.display_state={cameras:{top:{position:[1,2,3]}}};
  const reply=structuredClone(later);reply.climate.studies.B=staticStudy();
  const restored=await history.traverse(later,'undo',async(candidate,alt,p)=>{
    assert.equal(alt,'B');assert.deepEqual(p,reply);return validate(candidate);
  },undefined,async(p,alt,snapshot)=>{
    assert.deepEqual(p,later);assert.equal(alt,'B');assert.equal(snapshot.study,null);return reply;
  });
  assert.deepEqual(restored.project,reply);assert.deepEqual(restored.project.climate.runs,later.climate.runs);
  assert.deepEqual(restored.project.display_state,later.display_state);
  assert.notEqual(documentSignature(restored.project),documentSignature(original));
});

test('geometry edits and cross-alternative undo validate against the current study before changing either stack',async()=>{
  const p=withStudy(legacy(),staticStudy(),'B'),history=new DocumentHistory();
  const candidate=structuredClone(model);candidate.window.width_m=1.7;
  const seen=[];
  const checked=async(value,alt,project)=>{seen.push({alt,study:project.climate.studies[alt]});return validate(value);};
  const edited=await history.edit(p,'B',candidate,'B width',checked);
  const changed=structuredClone(edited.project);changed.climate.studies.B.metrics.da_threshold_lux=500;
  await assert.rejects(history.traverse(changed,'undo',async(value,alt,current)=>{
    assert.equal(alt,'B');assert.equal(current.climate.studies.B.metrics.da_threshold_lux,500);
    throw new Error('Current study rejects this sensor definition');
  }),/Current study/);
  assert.equal(history.past.length,1);assert.equal(history.future.length,0);
  const undone=await history.traverse(changed,'undo',checked);
  assert.equal(seen[1].study.metrics.da_threshold_lux,500);assert.equal(undone.alternativeId,'B');
  assert.deepEqual(undone.project.alternatives,p.alternatives);
  assert.equal(undone.project.climate.studies.B.metrics.da_threshold_lux,500);
});

test('failed study configure/restore and concurrent geometry edits do not consume accepted history',async()=>{
  const p=legacy(),history=new DocumentHistory(),study=staticStudy();study.metrics.da_threshold_lux=500;
  await assert.rejects(history.editStudy(p,'A',study,'Invalid',async()=>{throw new Error('invalid weather');}),/weather/);
  assert.equal(history.past.length,0);assert.equal(history.busy,false);
  let finish;
  const pending=history.editStudy(p,'A',study,'Queued',()=>new Promise(resolve=>{finish=()=>resolve(withStudy(p,study));}));
  await assert.rejects(history.edit(p,'A',model,'Concurrent',validate),/Wait/);
  finish();const edited=await pending;
  await assert.rejects(history.traverse(edited.project,'undo',validate,undefined,async()=>{throw new Error('Service unavailable');}),/unavailable/);
  assert.equal(history.past.length,1);assert.equal(history.future.length,0);assert.equal(history.busy,false);
  await assert.rejects(history.traverse(edited.project,'undo',validate),/restoration service/);
  assert.equal(history.past.length,1);
});

test('study no-op keeps redo history and changes to occupancy, weather or mode affect the dirty signature',async()=>{
  const p=withStudy(legacy(),staticStudy()),history=new DocumentHistory();
  const candidate=structuredClone(model);candidate.window.width_m=1.7;
  const edited=await history.edit(p,'A',candidate,'Width',validate);
  const undone=await history.traverse(edited.project,'undo',validate);
  const unchanged=await history.editStudy(undone.project,'A',staticStudy(),'Same',async document=>document);
  assert.equal(unchanged.changed,false);assert.equal(history.future.length,1);assert.equal(history.past.length,0);
  for(const change of [document=>{document.climate.studies.A.metrics.occupancy.end_minute=1200;},document=>{document.climate.weather.push({id:'weather'});},document=>{document.climate.studies.A.calculation={mode:'annual'};}]){
    const changed=structuredClone(p);change(changed);assert.notEqual(documentSignature(changed),documentSignature(p));
  }
  const snapshot=studySnapshot(p,'A');snapshot.study.metrics.da_threshold_lux=999;
  assert.equal(p.climate.studies.A.metrics.da_threshold_lux,300);
});

test('completed climate evidence is pending until its own immutable record is accepted, even when static result is null',()=>{
  const climate={id:'climate-result',alternative_id:'B'},job={state:'completed',result:null,climate_result:climate,pending_evidence:true};
  assert.equal(completedJobResult(job),climate);
  assert.equal(hasPendingEvidence(job,[],[]),true);
  assert.equal(hasPendingEvidence(job,[{id:climate.id}],[]),true);
  assert.equal(hasPendingEvidence(job,[],[climate]),false);
  assert.equal(hasPendingEvidence({state:'failed',result:null,climate_result:null,pending_evidence:false}),false);
  assert.equal(hasPendingEvidence({pending_evidence:true}),true);
  const staticRun={id:'static-result'};assert.equal(hasPendingEvidence({result:staticRun},[staticRun],[]),false);
});

for(const decision of ['cancel','save','discard'])test(`uncollected climate result participates in ${decision} close protection`,async()=>{
  const calls=[],job={state:'completed',result:null,climate_result:{id:'annual'},pending_evidence:true};
  const result=await protectReplacement({needsGuard:hasPendingEvidence(job),decide:async()=>decision,
    save:async()=>{calls.push('save-climate-evidence');return true;},discard:async()=>{calls.push('explicit-discard');},proceed:async()=>{calls.push('close');}});
  assert.equal(result,decision!=='cancel');
  assert.deepEqual(calls,decision==='cancel'?[]:decision==='save'?['save-climate-evidence','close']:['explicit-discard','close']);
});
