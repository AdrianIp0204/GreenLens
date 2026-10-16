import test from 'node:test';
import assert from 'node:assert/strict';
import {DocumentHistory,documentSignature,hasPendingEvidence,completedJobResult,protectReplacement} from '../src/workspace/document.mjs';
const project=()=>({schema_version:'0.4.0',id:'test',name:'Test',alternatives:[{id:'A',model:{daylight:'unchanged'}}],runs:[{id:'old-static'}],climate:{runs:[{id:'old-climate'}]},energy:{designs:{A:{model:{value:1},study:{weather:'retained'}}},runs:[{id:'energy-before'}],weather:[{id:'weather-before'}]},display_state:{camera:'unchanged'}});
const validate=async model=>({model});
test('revision undo restores exact active identity without pruning later issues or equal-scope revisions',async()=>{
 const history=new DocumentHistory(),p=project(),scope={title:'Same description'};
 p.architecture={scope,scope_history:{revisions:[{revision_sha256:'s1',scope}],active_revision_sha256:'s1'}};
 const accept=async p=>{p.architecture.scope_history.revisions.push({revision_sha256:'s2',scope});p.architecture.scope_history.active_revision_sha256='s2';return p;};
 const edited=await history.acceptScopeRevision(p,'A','Reviewed change',accept);assert.equal(history.past.length,1);
 const live=structuredClone(edited.project);live.report_issues={issues:[{id:'r1'},{id:'r2'}]};
 const restore=async(p,_scope,_restoring,id)=>{p.architecture.scope_history.active_revision_sha256=id;p.architecture.scope=p.architecture.scope_history.revisions.find(r=>r.revision_sha256===id).scope;return p;};
 const undone=await history.traverse(live,'undo',validate,undefined,undefined,undefined,undefined,undefined,undefined,restore);
 assert.equal(undone.project.architecture.scope_history.active_revision_sha256,'s1');assert.equal(undone.project.architecture.scope_history.revisions.length,2);assert.deepEqual(undone.project.report_issues,live.report_issues);
 const redone=await history.traverse(undone.project,'redo',validate,undefined,undefined,undefined,undefined,undefined,undefined,restore);assert.deepEqual(redone.project,live);
 const before=structuredClone(history.past);await assert.rejects(history.acceptScopeRevision(live,'A','Cancelled',accept,()=>false),/No revision was accepted/);assert.deepEqual(history.past,before);assert.equal(live.architecture.scope_history.revisions.length,2);
});
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

test('architectural scope undo restores historical binding while preserving later reference revisions and simulation evidence',async()=>{
  const history=new DocumentHistory(),p=project();
  p.architecture={schema_version:'greenlens.architectural-project.v1',assets:[{id:'source-first',revision_sha256:'revision-first'}],active_asset_id:'source-first',scope:null};
  const calls=[],applyScope=async(p,scope,restoring)=>{calls.push({scope:structuredClone(scope),restoring});p.architecture.scope=scope;return p;};
  const candidate={title:'Original declared portion',reference_revision_sha256:'revision-first',design_model_sha256:{A:'accepted-model-first'},mappings:[{occurrence_id:'first-occurrence',status:'mapped'}]};
  const edited=await history.editScope(p,'A',candidate,'Declared scope',applyScope);
  assert.equal(history.past.length,1);assert.equal(history.past[0].kind,'architectural_scope');assert.notEqual(documentSignature(edited.project),documentSignature(p));assert.equal(p.architecture.scope,null);
  const later=structuredClone(edited.project);later.architecture.assets.push({id:'source-later',revision_sha256:'revision-later'});later.architecture.active_asset_id='source-later';later.energy.runs.push({id:'energy-later'});later.energy.weather.push({id:'weather-later'});later.energy.attachments=[{id:'attachment-later'}];later.energy.workbench={sensitivity:[{id:'sensitivity-later'}]};later.climate.runs.push({id:'climate-later'});later.runs.push({id:'static-later'});
  const undo=await history.traverse(later,'undo',validate,undefined,undefined,undefined,undefined,undefined,undefined,applyScope);
  assert.equal(undo.project.architecture.scope,null);assert.deepEqual(undo.project.architecture.assets,later.architecture.assets);assert.equal(undo.project.architecture.active_asset_id,'source-later');assert.deepEqual(undo.project.energy,later.energy);assert.deepEqual(undo.project.runs,later.runs);assert.deepEqual(undo.project.climate,later.climate);
  const redo=await history.traverse(undo.project,'redo',validate,undefined,undefined,undefined,undefined,undefined,undefined,applyScope);
  assert.deepEqual(redo.project,later);assert.equal(redo.project.architecture.scope.reference_revision_sha256,'revision-first','redo does not silently rebind to the newer source');assert.deepEqual(calls.map(c=>c.restoring),[false,true,true]);
});

test('scope rejection and rejected historical restore leave source and both history stacks unchanged',async()=>{
  const history=new DocumentHistory(),p=project();p.architecture={assets:[{id:'reference'}],active_asset_id:'reference',scope:null};const before=structuredClone(p);
  await assert.rejects(history.editScope(p,'A',{title:'Invalid'},'Invalid scope',async(p,scope)=>{p.architecture.scope=scope;throw Error('Rust scope validation rejected');}),/Rust scope validation rejected/);assert.deepEqual(p,before);assert.equal(history.past.length,0);assert.equal(history.future.length,0);
  const apply=async(p,scope)=>{p.architecture.scope=scope;return p;};const edited=await history.editScope(p,'A',{title:'First'},'First scope',apply);
  await assert.rejects(history.traverse(edited.project,'undo',validate,undefined,undefined,undefined,undefined,undefined,undefined,async()=>{throw Error('Retained source integrity failed');}),/Retained source integrity failed/);assert.equal(history.past.length,1);assert.equal(history.future.length,0);assert.deepEqual(edited.project.architecture.scope,{title:'First'});
  const unchanged=await history.editScope(edited.project,'A',{title:'First'},'Identical',apply);assert.equal(unchanged.changed,false);assert.equal(history.past.length,1);
  const undo=await history.traverse(edited.project,'undo',validate,undefined,undefined,undefined,undefined,undefined,undefined,apply);assert.equal(history.future.length,1);await history.editScope(undo.project,'A',{title:'New scope'},'New scope',apply);assert.equal(history.future.length,0);assert.equal(history.past.length,1);
});
