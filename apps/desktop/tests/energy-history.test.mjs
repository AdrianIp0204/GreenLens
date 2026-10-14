import test from 'node:test';
import assert from 'node:assert/strict';
import {DocumentHistory,documentSignature,hasPendingEvidence,completedJobResult,protectReplacement} from '../src/workspace/document.mjs';
const project=()=>({schema_version:'0.4.0',id:'test',name:'Test',alternatives:[{id:'A',model:{daylight:'unchanged'}}],runs:[{id:'old-static'}],climate:{runs:[{id:'old-climate'}]},energy:{designs:{A:{model:{value:1},study:{weather:'retained'}}},runs:[{id:'energy-before'}],weather:[{id:'weather-before'}]},display_state:{camera:'unchanged'}});
const validate=async model=>({model});
const configure=async(p,id,design)=>{if(design.model.value<0)throw Error('Rust validation rejected');return {...p,energy:{...p.energy,designs:{...p.energy.designs,[id]:design}}};};
test('energy edits are dirty, validated and one undo entry; undo keeps newly retained evidence',async()=>{
  const history=new DocumentHistory(),p=project(),signature=documentSignature(p);
  const edit=await history.editEnergy(p,'A',{model:{value:2},study:{weather:'retained'}},'Thermal material',configure);
  assert.notEqual(documentSignature(edit.project),signature);assert.equal(history.past.length,1);
  const live=structuredClone(edit.project);live.energy.runs.push({id:'energy-after'});live.energy.weather.push({id:'weather-after'});
  const undone=await history.traverse(live,'undo',validate,undefined,undefined,configure);
  assert.equal(undone.project.energy.designs.A.model.value,1);assert.equal(undone.project.energy.runs.length,2);assert.equal(undone.project.energy.weather.length,2);
  assert.deepEqual(undone.project.alternatives,p.alternatives);assert.deepEqual(undone.project.display_state,p.display_state);
  const redone=await history.traverse(undone.project,'redo',validate,undefined,undefined,configure);assert.equal(redone.project.energy.designs.A.model.value,2);assert.equal(redone.project.energy.runs.length,2);
});
test('rejected thermal edit or undo leaves both history and document unchanged',async()=>{
  const history=new DocumentHistory(),p=project(),before=JSON.stringify(p);
  await assert.rejects(history.editEnergy(p,'A',{model:{value:-1},study:null},'Invalid',configure));assert.equal(JSON.stringify(p),before);assert.equal(history.past.length,0);
  const edit=await history.editEnergy(p,'A',{model:{value:2},study:null},'Valid',configure);
  await assert.rejects(history.traverse(edit.project,'undo',validate,undefined,undefined,async()=>{throw Error('Changed external context');}));assert.equal(history.past.length,1);assert.equal(history.future.length,0);
});
test('energy pending result is protected independently of legacy results',()=>{
  const result={id:'energy',alternative_id:'A'},job={energy_result:result,pending_evidence:true};
  assert.equal(completedJobResult(job),result);assert.equal(hasPendingEvidence(job,[{id:'energy'}],[{id:'energy'}]),true);
  assert.equal(hasPendingEvidence(job,[],[],[result]),false);
});
test('cancelled document replacement never creates a thermal example',async()=>{
  let writes=0;assert.equal(await protectReplacement({needsGuard:true,decide:async()=>'cancel',save:async()=>false,discard:async()=>{},proceed:async()=>{writes++;}}),false);assert.equal(writes,0);
});

test('report metadata undo preserves later attachment imports, weather and immutable runs',async()=>{const history=new DocumentHistory(),p=project();p.energy.attachments=[{id:'first',include_in_export:false}];p.energy.carbon_factor=null;p.energy.lighting_service_evidence=null;const configureMetadata=async(p,c)=>({...p,energy:{...p.energy,...(c.kind==='attachment'?{attachments:p.energy.attachments.map(a=>a.id===c.id?{...a,include_in_export:c.included}:a)}:{carbon_factor:c.carbon_factor,lighting_service_evidence:c.lighting_service_evidence})}});const edit=await history.editEnergyMetadata(p,'A',{kind:'attachment',id:'first',included:true},'Include',configureMetadata);const later=structuredClone(edit.project);later.energy.attachments.push({id:'second',include_in_export:false});later.energy.runs.push({id:'later'});const undo=await history.traverse(later,'undo',validate,undefined,undefined,configure,configureMetadata);assert.equal(undo.project.energy.attachments[0].include_in_export,false);assert.equal(undo.project.energy.attachments[1].id,'second');assert.equal(undo.project.energy.runs.at(-1).id,'later');assert.deepEqual(undo.project.energy.designs,p.energy.designs);});

test('batch history restores only targeted inputs and report fields, retaining independently added evidence and report state',async()=>{
  const history=new DocumentHistory(),p=project();p.energy.designs.B={model:{value:10},study:null};p.energy.carbon_factor=null;p.energy.lighting_service_evidence=null;p.energy.workbench={report_metadata:null,locks:{A:['locked-object']}};p.energy.attachments=[];
  const restore=async(p,b)=>{Object.assign(p.energy.designs,b.designs);if(b.reportSettings)Object.assign(p.energy,b.reportSettings);if('reportIssue'in b)p.energy.workbench.report_metadata=b.reportIssue;return p;};
  const batch={designs:{A:{model:{value:2},study:null}},reportIssue:{title:'Report',revision:'R1'}};
  const edit=await history.editEnergyBatch(p,'A',batch,'Guarded drafts',restore);assert.equal(history.past.length,1);assert.equal(history.past[0].kind,'energy_batch');
  const later=structuredClone(edit.project);later.energy.runs.push({id:'later-run'});later.energy.weather.push({id:'later-weather'});later.energy.attachments.push({id:'later-attachment'});later.energy.workbench.sensitivity=[{id:'later-session'}];later.energy.lighting_service_evidence='Later independent settings';later.energy.designs.B.model.value=20;
  const undo=await history.traverse(later,'undo',validate,undefined,undefined,undefined,undefined,undefined,restore);
  assert.equal(undo.project.energy.designs.A.model.value,1);assert.equal(undo.project.energy.designs.B.model.value,20);assert.equal(undo.project.energy.workbench.report_metadata,null);assert.equal(undo.project.energy.lighting_service_evidence,'Later independent settings');
  for(const key of ['runs','weather','attachments'])assert.deepEqual(undo.project.energy[key],later.energy[key]);assert.deepEqual(undo.project.energy.workbench.sensitivity,later.energy.workbench.sensitivity);assert.deepEqual(undo.project.energy.workbench.locks,later.energy.workbench.locks);
  const redo=await history.traverse(undo.project,'redo',validate,undefined,undefined,undefined,undefined,undefined,restore);assert.deepEqual(redo.project,later);
});
test('batch failure and changed draft checks do not mutate source or history, and a new accepted batch clears redo',async()=>{
  const history=new DocumentHistory(),p=project(),before=structuredClone(p),batch={designs:{A:{model:{value:2},study:null}}};
  const configure=async(p,b)=>{Object.assign(p.energy.designs,b.designs);return p;};
  await assert.rejects(history.editEnergyBatch(p,'A',batch,'Failed',async(p,b)=>{Object.assign(p.energy.designs,b.designs);throw Error('Later metadata rejected');}),/Later metadata rejected/);assert.deepEqual(p,before);assert.deepEqual(history.past,[]);
  await assert.rejects(history.editEnergyBatch(p,'A',batch,'Newer',configure,()=>false),/newer draft/);assert.deepEqual(p,before);assert.deepEqual(history.past,[]);
  const edit=await history.editEnergyBatch(p,'A',batch,'Accepted',configure),undo=await history.traverse(edit.project,'undo',validate,undefined,undefined,undefined,undefined,undefined,configure);assert.equal(history.future.length,1);
  await history.editEnergyBatch(undo.project,'A',{designs:{A:{model:{value:3},study:null}}},'Another accepted',configure);assert.equal(history.future.length,0);assert.equal(history.past.length,1);
});
