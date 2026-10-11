import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { DocumentHistory, documentSignature, protectReplacement, isTextTarget } from '../src/workspace/document.mjs';
import { parseCommand, parseQuantity, modelCommand, setParameter, repeatable, suggestions } from '../src/workspace/commands.mjs';
import { editLockReason, validatedEditLockReason } from '../src/workspace/locks.mjs';
import { visibleResult, newestCompatibleResult } from '../src/presentation.mjs';

// Injected validator checks transaction orchestration only. Rust geometry and real
// Radiance acceptance are exercised independently by the required native suites.
const fixture=JSON.parse(await readFile(new URL('./viewport-fixture.json',import.meta.url),'utf8'));
const model=fixture.model;
const project=()=>({id:'test-document',name:'Synthetic',schema_version:'0.1.0',alternatives:[{id:'A',model:structuredClone(model)},{id:'B',model:structuredClone(model)}],runs:[]});
const validate=async candidate=>{
  if(candidate.window.width_m>candidate.room.width_m)throw new Error('Window exceeds facade');
  return {...fixture,model:structuredClone(candidate),scientific_input_hash:JSON.stringify(candidate)};
};
test('one provisional drag commits one validated snapshot; undo/redo restore exact inputs and result relation',async()=>{
  const h=new DocumentHistory(),original=project(),before=await validate(model);
  const run={id:'original',alternative_id:'A',validated:before,lux:[1]};
  let provisional=structuredClone(model);for(let i=0;i<20;i++)provisional.window.width_m=1.8+i*.01;
  const edited=await h.edit(original,'A',provisional,'Window drag',validate);
  assert.equal(h.past.length,1);assert.equal(h.future.length,0);assert.equal(original.alternatives[0].model.window.width_m,1.8);
  assert.equal(visibleResult(edited.validated,run),null);
  const undone=await h.traverse(edited.project,'undo',validate);
  assert.deepEqual(undone.project,original);assert.equal(visibleResult(undone.validated,run),run);
  const editedRun={id:'edited',alternative_id:'A',validated:edited.validated,lux:[2]},records=[run,editedRun];
  assert.equal(newestCompatibleResult(undone.validated,records,'A'),run);
  const redone=await h.traverse(undone.project,'redo',validate);
  assert.deepEqual(redone.project,edited.project);assert.equal(visibleResult(redone.validated,run),null);
  assert.equal(newestCompatibleResult(redone.validated,records,'A'),editedRun);
  assert.deepEqual(records.map(record=>record.id),['original','edited']);
});
test('invalid edits and failed undo validation preserve accepted document and both stacks',async()=>{
  const h=new DocumentHistory(),p=project();
  await assert.rejects(h.edit(p,'A',setParameter(model,'window.width_m',8),'Invalid',validate),/exceeds/);
  assert.equal(h.past.length,0);assert.equal(h.future.length,0);assert.equal(h.busy,false);
  const first=await h.edit(p,'A',setParameter(model,'window.width_m',1.6),'Width',validate);
  await assert.rejects(h.traverse(first.project,'undo',async()=>{throw new Error('service unavailable');}),/unavailable/);
  assert.equal(h.past.length,1);assert.equal(h.future.length,0);assert.equal(h.busy,false);
});
test('undo and redo honor current locks across alternatives without consuming history',async()=>{
  const h=new DocumentHistory(),p=project();
  const display={layers:{window:{locked:false}},objects:{}};
  const check=(before,after)=>{const reason=editLockReason(before,after,display);if(reason)throw new Error(reason);};
  const edited=await h.edit(p,'B',setParameter(model,'window.width_m',1.6),'B width',validate);
  display.layers.window.locked=true;
  await assert.rejects(h.traverse(edited.project,'undo',validate,check),/locked/);
  assert.equal(h.past.length,1);assert.equal(h.future.length,0);assert.equal(h.busy,false);
  display.layers.window.locked=false;
  const undone=await h.traverse(edited.project,'undo',validate,check);
  display.objects.window={locked:true};
  await assert.rejects(h.traverse(undone.project,'redo',validate,check),/locked/);
  assert.equal(h.past.length,0);assert.equal(h.future.length,1);
  display.objects.window.locked=false;
  const redone=await h.traverse(undone.project,'redo',validate,check);
  assert.deepEqual(redone.project,edited.project);
});
test('cancelled preview and no-op validation add no accepted history entry',async()=>{
  const h=new DocumentHistory(),p=project(),signature=documentSignature(p);
  const preview=setParameter(model,'window.center_x_m',2.3);assert.notDeepEqual(preview,model);
  // Escape discards a provisional candidate without invoking edit.
  assert.equal(documentSignature(p),signature);assert.equal(h.past.length,0);
  const unchanged=await h.edit(p,'A',model,'No change',validate);assert.equal(unchanged.changed,false);assert.equal(h.past.length,0);
});
test('optional object creation/removal survives undo/redo and a new accepted edit clears redo',async()=>{
  const h=new DocumentHistory(),p=project();
  const created=await h.edit(p,'A',modelCommand(model,[],'Overhang',[]).model,'Create shade',validate);
  const removed=await h.edit(created.project,'A',modelCommand(created.validated.model,['overhang'],'Remove',[]).model,'Remove shade',validate);
  assert.equal(removed.validated.model.overhang,null);
  const restored=await h.traverse(removed.project,'undo',validate);assert.deepEqual(restored.validated.model.overhang,created.validated.model.overhang);
  await h.edit(restored.project,'A',setParameter(restored.validated.model,'overhang.projection_m',.8),'Projection',validate);
  assert.equal(h.future.length,0);assert.equal(h.past.length,2);
});
test('history spans alternatives deliberately, save preserves history, opening resets it',async()=>{
  const h=new DocumentHistory(),p=project();
  const edited=await h.edit(p,'B',setParameter(model,'window.width_m',1.7),'B width',validate);
  const saved={...edited.project,runs:[]};
  assert.equal(h.past.length,1);const restored=await h.traverse(saved,'undo',validate);
  assert.equal(restored.alternativeId,'B');assert.deepEqual(restored.project.alternatives,p.alternatives);
  h.reset();assert.equal(h.past.length,0);assert.equal(h.future.length,0);
});
test('concurrent accepted edits are refused while Rust validation is pending',async()=>{
  const h=new DocumentHistory();let resolve;
  const slow=h.edit(project(),'A',setParameter(model,'window.width_m',1.7),'Slow',candidate=>new Promise(r=>{resolve=()=>r({...fixture,model:candidate});}));
  await assert.rejects(h.edit(project(),'A',model,'Racing',validate),/Wait/);resolve();await slow;assert.equal(h.past.length,1);
});
for(const action of ['New','Open','native close']){
  for(const decision of ['save','discard','cancel'])test(`${action}: ${decision} protection has explicit ordered effects`,async()=>{
    const events=[];const done=await protectReplacement({needsGuard:true,decide:async()=>{events.push('prompt');return decision;},save:async()=>{events.push('cancel-and-join','save-with-pending-evidence');return true;},discard:async()=>{events.push('explicit-discard');},proceed:async()=>{events.push(action);}});
    assert.equal(done,decision!=='cancel');assert.deepEqual(events,decision==='save'?['prompt','cancel-and-join','save-with-pending-evidence',action]:decision==='discard'?['prompt','explicit-discard',action]:['prompt']);
  });
}
test('failed or picker-cancelled save never proceeds to destructive replacement',async()=>{
  for(const mode of ['cancel','failure']){let discarded=false,proceeded=false;
    const call=()=>protectReplacement({needsGuard:true,decide:async()=>'save',save:async()=>{if(mode==='failure')throw new Error('external revision changed');return false;},discard:async()=>{discarded=true;},proceed:async()=>{proceeded=true;}});
    if(mode==='failure')await assert.rejects(call(),/revision/);else assert.equal(await call(),false);
    assert.equal(discarded,false);assert.equal(proceeded,false);
  }
});
test('clean replacement skips prompt; unsupported protection decision cannot proceed',async()=>{
  let count=0;await protectReplacement({needsGuard:false,decide:()=>assert.fail('no prompt'),save:()=>assert.fail(),discard:()=>assert.fail(),proceed:async()=>{count++;}});assert.equal(count,1);
  await assert.rejects(protectReplacement({needsGuard:true,decide:async()=>'invalid',save:()=>assert.fail(),discard:()=>assert.fail(),proceed:()=>assert.fail()}),/Unknown/);
});
test('display state does not alter scientific dirty signature or edit stacks',()=>{
  const p=project(),before=documentSignature(p);p.display_state={mode:'wireframe',cameras:{perspective:{zoom:2}},layers:{room:{visible:false,locked:true}}};
  assert.equal(documentSignature(p),before);p.alternatives[0].model.window.width_m=1.7;assert.notEqual(documentSignature(p),before);
});
test('bounded commands are case-insensitive, prompted, autocompleted and never shell-evaluated',()=>{
  assert.deepEqual(parseCommand('  ReSiZe WINDOW 1800mm 150cm '),{name:'Resize',args:['WINDOW','1800mm','150cm']});
  assert.deepEqual(parseCommand('Zoom Extents'),{name:'ZoomExtents',args:[]});assert.deepEqual(parseCommand('Save As'),{name:'SaveAs',args:[]});
  for(const mode of ['Replace','Add','Remove'])assert.deepEqual(parseCommand('SELECT '+mode.toLowerCase()),{name:'Select'+mode,args:[]});
  assert.equal(parseCommand('   '),null);assert.throws(()=>parseCommand('powershell rm *'),/Unknown command/);
  assert.equal(suggestions('zoom').length,2);assert.equal(repeatable('Run'),false);assert.equal(repeatable('Remove'),false);assert.equal(repeatable('ZoomExtents'),true);
  assert.match(modelCommand(model,['window'],'Resize',[]).prompt,/Resize window/);
});
test('numeric editing honors units and rejects incompatible, nonfinite or arbitrary input',()=>{
  assert.equal(parseQuantity('1800mm'),1.8);assert.equal(parseQuantity('150 cm'),1.5);assert.equal(parseQuantity('1ft'),.3048);assert.equal(parseQuantity('2in'),.0508);
  assert.equal(parseQuantity('90deg','angle'),90);assert.equal(parseQuantity('10000lx','lux'),10000);
  for(const value of ['NaN','Infinity','1e999','2;alert(1)','x=3','2lx'])assert.throws(()=>parseQuantity(value));
  assert.throws(()=>parseQuantity('2m','number'));assert.throws(()=>parseQuantity('2mm','angle'));
});
test('numeric commands change only supported parameters, reject ambiguous selections and preserve original',()=>{
  const moved=modelCommand(model,['window'],'Move',['100mm','-10cm']);assert.equal(moved.model.window.center_x_m,2.1);assert.equal(moved.model.window.sill_m,.8);assert.equal(model.window.center_x_m,2);
  assert.deepEqual(moved.model.room,model.room);assert.equal(modelCommand(model,[],'Rotate',['room','90deg']).model.room.rotation_deg,90);
  assert.throws(()=>modelCommand(model,['room','window'],'Move',['1','1']),/one/);assert.throws(()=>modelCommand(model,['room'],'Remove',[]),/required/);
  assert.throws(()=>modelCommand(model,['window'],'Rotate',['90']),/Only the room/);assert.throws(()=>setParameter(model,'overhang.projection_m',.5),/unavailable/);
});
test('layer/object locks reject edits across command/property/gizmo paths without affecting model',()=>{
  const display={layers:{window:{locked:true},room:{locked:false},sensors:{locked:false}},objects:{}};
  assert.match(editLockReason(model,setParameter(model,'window.width_m',1.7),display),/locked/i);
  assert.match(editLockReason(model,setParameter(model,'materials.visible_transmittance',.5),display),/locked/i);
  display.layers.window.locked=false;display.objects.window={locked:true};assert.match(editLockReason(model,setParameter(model,'window.sill_m',1),display),/locked/i);
  assert.equal(editLockReason(model,setParameter(model,'sky.ground_reflectance',.1),display),null);
});

// These intentionally synthetic validator outputs test lock/transaction
// orchestration. They are not substitutes for Rust geometry or engine checks.
const lockDisplay=()=>({layers:{},objects:{}});
const sampleSurface=(id,material)=>({id,material,reflectance:material==='glass'?null:.5,vertices:[[0,0,0],[1,0,0],[1,1,0],[0,1,0]]});
const withDependents=()=>{
  const value=structuredClone(fixture);
  value.model.overhang={projection_m:.6,side_extension_m:0,gap_above_window_m:0,reflectance:.5};
  value.model.obstruction={center_x_m:2,width_m:1,depth_m:1,height_m:1,distance_m:3,reflectance:.2};
  value.surfaces.push(sampleSurface('overhang','overhang'),sampleSurface('obstruction_top','obstruction'));
  return value;
};
test('window edits honor locked generated shading and facade surfaces, but unaffected locks allow edits',()=>{
  const before=withDependents(),after=structuredClone(before),display=lockDisplay();
  after.model.window.center_x_m+=.1;
  for(const surface of after.surfaces.filter(s=>['overhang','south_right'].includes(s.id)))surface.vertices[0][0]+=.1;
  display.layers.overhang={locked:true};assert.match(validatedEditLockReason(before,after,display),/overhang is locked/);
  display.layers.overhang.locked=false;display.objects.overhang={locked:true};assert.match(validatedEditLockReason(before,after,display),/overhang is locked/);
  display.objects={room:{locked:true}};assert.match(validatedEditLockReason(before,after,display),/surface:south_right is locked/);
  display.objects={'surface:south_right':{locked:true}};assert.match(validatedEditLockReason(before,after,display),/surface:south_right is locked/);
  display.objects={'surface:floor':{locked:true}};assert.equal(validatedEditLockReason(before,after,display),null);
});
test('room rotation honors each affected dependent layer and individual generated sensor',()=>{
  const before=withDependents(),after=structuredClone(before);
  after.model.room.rotation_deg=90;
  for(const surface of after.surfaces)surface.vertices[0][0]+=.1;
  after.sensor_grid.sensors[0].position[0]+=.1;
  for(const layer of ['window','overhang','obstruction','sensors']){
    const display=lockDisplay();display.layers[layer]={locked:true};assert.match(validatedEditLockReason(before,after,display),/locked/);
  }
  for(const id of ['window','overhang','obstruction','sensor-plane','sensor:s000001']){
    const display=lockDisplay();display.objects[id]={locked:true};assert.match(validatedEditLockReason(before,after,display),/locked/);
  }
});
test('locks inspect added/removed geometry and material changes without broad dependency guessing',()=>{
  const before=withDependents(),removed=structuredClone(before),display=lockDisplay();
  removed.model.sensors.requested_spacing_m=1;
  removed.sensor_grid.sensors.shift();display.objects['sensor:s000001']={locked:true};
  assert.match(validatedEditLockReason(before,removed,display),/sensor:s000001 is locked/);
  const material=structuredClone(before);material.model.materials.ceiling_reflectance=.7;
  material.surfaces.find(s=>s.id==='ceiling').reflectance=.7;
  display.objects={'surface:ceiling':{locked:true}};assert.match(validatedEditLockReason(before,material,display),/surface:ceiling is locked/);
  display.objects={'surface:floor':{locked:true}};assert.equal(validatedEditLockReason(before,material,display),null);
  const glazing=structuredClone(before);glazing.model.materials.visible_transmittance=.5;
  display.objects={overhang:{locked:true}};assert.equal(validatedEditLockReason(before,glazing,display),null);
  const metadata=structuredClone(before);metadata.model.name='Display name only';
  display.layers={room:{locked:true},window:{locked:true},overhang:{locked:true},obstruction:{locked:true},sensors:{locked:true}};
  assert.equal(validatedEditLockReason(before,metadata,display),null);
});
test('validated lock rejection preserves document and both history stacks for edits, undo and redo',async()=>{
  const before=withDependents(),after=structuredClone(before),display=lockDisplay();
  after.model.window.center_x_m+=.1;after.surfaces.find(s=>s.id==='overhang').vertices[0][0]+=.1;
  const snapshots=new Map([before,after].map(v=>[JSON.stringify(v.model),v]));
  const backend=async candidate=>structuredClone(snapshots.get(JSON.stringify(candidate)));
  const check=async(b,_a,validated)=>{const reason=validatedEditLockReason(await backend(b),validated,display);if(reason)throw new Error(reason);};
  const p=project();p.alternatives[0].model=structuredClone(before.model);const original=structuredClone(p),h=new DocumentHistory();
  display.layers.overhang={locked:true};
  await assert.rejects(h.edit(p,'A',after.model,'Move window',backend,check),/overhang is locked/);
  assert.deepEqual(p,original);assert.equal(h.past.length,0);assert.equal(h.future.length,0);assert.equal(h.busy,false);
  display.layers.overhang.locked=false;const edited=await h.edit(p,'A',after.model,'Move window',backend,check);
  display.layers.overhang.locked=true;await assert.rejects(h.traverse(edited.project,'undo',backend,check),/overhang is locked/);
  assert.equal(h.past.length,1);assert.equal(h.future.length,0);assert.deepEqual(edited.project.alternatives[0].model,after.model);
  display.layers.overhang.locked=false;const undone=await h.traverse(edited.project,'undo',backend,check);
  display.layers.overhang.locked=true;await assert.rejects(h.traverse(undone.project,'redo',backend,check),/overhang is locked/);
  assert.equal(h.past.length,0);assert.equal(h.future.length,1);assert.deepEqual(undone.project,original);assert.equal(h.busy,false);
});
test('global shortcuts preserve text controls and contenteditable native editing',()=>{
  for(const tagName of ['INPUT','TEXTAREA','SELECT'])assert.equal(isTextTarget({tagName}),true);
  assert.equal(isTextTarget({isContentEditable:true}),true);assert.equal(isTextTarget({tagName:'BUTTON'}),false);assert.equal(isTextTarget(null),false);
});
