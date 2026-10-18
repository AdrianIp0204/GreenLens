// Actual DesignProposal React form and lifecycle. Explicit IPC/commit doubles
// test composition only; no Rust, native GUI, engine or scientific acceptance.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {JSDOM} from 'jsdom';
import {transformWithOxc} from 'vite';

const dom=new JSDOM('<!doctype html><div id="root"></div>',{url:'https://greenlens.local/'});
for(const name of ['window','document','HTMLElement','HTMLInputElement','HTMLTextAreaElement','HTMLSelectElement','HTMLDialogElement','Event','MouseEvent'])globalThis[name]=dom.window[name];
globalThis.IS_REACT_ACT_ENVIRONMENT=true;
dom.window.HTMLDialogElement.prototype.showModal=function(){this.open=true;};
const React=await import('react'),{act}=React,{createRoot}=await import('react-dom/client');
const cache=new Map();
async function load(filename){
  if(cache.has(filename))return cache.get(filename);
  const result=(async()=>{
    const {code}=await transformWithOxc(fs.readFileSync(filename,'utf8'),filename,{jsx:{runtime:'automatic'}});
    let linked=code.replace(/import\s+['"][^'"]+\.css['"];?/g,'');
    for(const match of [...code.matchAll(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g)]){
      let target;
      if(match[3]==='@tauri-apps/api/core')target='data:text/javascript,'+encodeURIComponent('export const invoke=(command,args)=>globalThis.__proposalIPC(command,args);');
      else if(match[3].startsWith('.')){
        let resolved=path.resolve(path.dirname(filename),match[3]);
        if(!path.extname(resolved))resolved=['.ts','.tsx','.mjs'].map(ext=>resolved+ext).find(fs.existsSync);
        target=await load(resolved);
      }else target=import.meta.resolve(match[3]);
      linked=linked.replace(match[0],`import ${match[1]} from ${JSON.stringify(target)};`);
    }
    return `data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`;
  })();cache.set(filename,result);return result;
}
const {default:DesignProposal}=await import(await load(fileURLToPath(new URL('../src/energy/DesignProposal.tsx',import.meta.url))));
const inputs=JSON.parse(fs.readFileSync(new URL('../../../examples/energy/revision1-inputs.json',import.meta.url)));
function fixture(){
  const project={id:'original-synthetic-proposal',energy:{designs:{A:structuredClone(inputs.cases.A),B:structuredClone(inputs.cases.B)},runs:[]}};
  project.energy.designs.A.model.zones[0].id='baseline-only-zone';
  project.energy.designs.A.model.zones[0].name='Baseline-only original synthetic zone';
  return project;
}
const deferred=()=>{let resolve,reject;const promise=new Promise((r,j)=>{resolve=r;reject=j;});return{promise,resolve,reject};};
let root,project,selectedZone,previewCalls,commitCalls,activities,closed,lifecycle,lifecycleCalls,previewImpl,commitImpl,visible;
function reply(valid=true){return{schema_version:'greenlens.energy-proposal-review.v1',target:'B',context_sha256:'c'.repeat(64),proposal:{candidate:structuredClone(project.energy.designs.B.model),valid,problems:valid?[]:['Window exceeds the selected facade. Accepted inputs unchanged.'],affected_object_ids:['window-from-B'],tradeoffs:['External shading can reduce solar gains and useful daylight; test actual results.'],base_model_sha256:'a'.repeat(64),candidate_model_sha256:valid?'b'.repeat(64):null,preview_sha256:'d'.repeat(64)}};}
function Harness(){return visible?React.createElement(DesignProposal,{project,selectedZone,onCommit:async(draft,sha)=>{commitCalls.push({draft:structuredClone(draft),sha});return commitImpl(draft,sha);},onClose:()=>{closed++;visible=false;void draw();},onActivityChange:value=>activities.push(value),registerLifecycle:handler=>{lifecycle=handler;return()=>{if(lifecycle===handler)lifecycle=null;};}}):React.createElement('p',null,'Proposal closed');}
async function draw(){await act(async()=>root.render(React.createElement(Harness)));}
async function setup(configure=()=>{}){
  if(root)await act(async()=>root.unmount());
  document.body.innerHTML='<div id="root"></div>';root=createRoot(document.getElementById('root'));
  project=fixture();configure(project);selectedZone=project.energy.designs.B.model.zones[0].id;previewCalls=[];commitCalls=[];activities=[];closed=0;lifecycle=null;lifecycleCalls=[];visible=true;
  previewImpl=async()=>reply();commitImpl=async()=>true;
  globalThis.__proposalIPC=async(command,args)=>{previewCalls.push({command,args:structuredClone(args)});return previewImpl(command,args);};
  await draw();
}
function button(text,scope=document){const result=[...scope.querySelectorAll('button')].find(e=>e.textContent===text);assert.ok(result,`button ${text}`);return result;}
async function click(text,scope=document){const e=button(text,scope);await act(async()=>e.dispatchEvent(new MouseEvent('click',{bubbles:true})));}
function field(label){const el=[...document.querySelectorAll('label')].find(e=>[...e.childNodes].filter(n=>n.nodeType===3).map(n=>n.textContent).join('').trim()===label);assert.ok(el,`field ${label}`);return el.querySelector('input,select,textarea');}
async function set(label,value){const el=field(label);const type=el.tagName==='SELECT'?HTMLSelectElement:el.tagName==='TEXTAREA'?HTMLTextAreaElement:HTMLInputElement;await act(async()=>{Object.getOwnPropertyDescriptor(type.prototype,'value').set.call(el,value);el.dispatchEvent(new Event(el.tagName==='SELECT'?'change':'input',{bubbles:true}));});}
async function documented(){await set('Source / assumption','Original synthetic concept assumption');await set('Rationale and applicability','One explicitly reviewed change; no guaranteed performance improvement.');}
async function request(reason){let settled=false,value;await act(async()=>{const p=lifecycle(reason);p.then(result=>{settled=true;value=result;lifecycleCalls.push({reason,result});});});return{get settled(){return settled;},get value(){return value;}};}
const guard=()=>document.querySelector('[role="alertdialog"][aria-label="Resolve proposed alternative"]');

test('actual proposal form exposes only B targets, marks preview unsimulated and never applies automatically',async()=>{
  await setup();assert.equal(document.querySelector('dialog').open,true);assert.deepEqual(activities,[true]);
  assert.match(document.body.textContent,/Design alternative · Proposed B/);assert.match(document.body.textContent,/Not simulated/);
  assert.doesNotMatch(field('Zone in B').textContent,/baseline-only-zone/);
  assert.equal(field('Zone in B').value,selectedZone);assert.equal(button('Apply to B').disabled,true);
  await documented();await click('Preview in Rust');
  assert.equal(previewCalls.length,1);assert.equal(previewCalls[0].command,'energy_proposal_preview');
  assert.equal(previewCalls[0].args.draft.action.zone_id,selectedZone);assert.equal(previewCalls[0].args.draft.action.kind,'set_overhang');
  assert.equal(previewCalls[0].args.draft.source.synthetic,true);assert.equal(previewCalls[0].args.draft.units,'m');
  assert.deepEqual(commitCalls,[]);assert.equal(closed,0);assert.match(document.querySelector('[aria-label="Proposal preview"]').textContent,/reduce solar gains and useful daylight/);
});

test('valid preview applies the exact reviewed draft/context once and leaves A untouched',async()=>{
  await setup();const baseline=JSON.stringify(project.energy.designs.A);await documented();await set('Projection (m)','0.75');await click('Preview in Rust');
  const reviewed=structuredClone(previewCalls[0].args.draft);await click('Apply to B');
  assert.equal(commitCalls.length,1);assert.deepEqual(commitCalls[0],{draft:reviewed,sha:'c'.repeat(64)});assert.equal(commitCalls[0].draft.action.overhang.projection_m,.75);
  assert.equal(JSON.stringify(project.energy.designs.A),baseline);assert.equal(closed,1);assert.deepEqual(activities,[true,false]);assert.equal(document.querySelector('dialog'),null);
});

test('invalid Rust preview preserves every field and cannot be applied',async()=>{
  await setup();previewImpl=async()=>reply(false);await documented();await set('Projection (m)','99');await click('Preview in Rust');
  assert.match(document.body.textContent,/Invalid candidate — accepted inputs unchanged/);assert.match(document.body.textContent,/Window exceeds/);
  assert.equal(field('Projection (m)').value,'99');assert.equal(field('Source / assumption').value,'Original synthetic concept assumption');
  assert.equal(button('Apply to B').disabled,true);await click('Apply to B');assert.deepEqual(commitCalls,[]);assert.equal(closed,0);
});

test('rejected Apply retains candidate fields and accepted state without closing',async()=>{
  await setup();const original=JSON.stringify(project);commitImpl=async()=>false;await documented();await set('Projection (m)','0.88');await click('Preview in Rust');await click('Apply to B');
  assert.equal(commitCalls.length,1);assert.equal(JSON.stringify(project),original);assert.equal(closed,0);assert.equal(field('Projection (m)').value,'0.88');assert.match(document.body.textContent,/Apply was rejected. The draft is retained/);
});

test('pending Apply disables repeated submission and a rejected async commit keeps the draft',async()=>{
  await setup();await documented();await click('Preview in Rust');const pending=deferred();commitImpl=()=>pending.promise;
  await click('Apply to B');assert.equal(commitCalls.length,1);assert.equal(button('Apply to B').disabled,true);await click('Apply to B');assert.equal(commitCalls.length,1);
  await act(async()=>pending.resolve(false));assert.equal(closed,0);assert.equal(field('Source / assumption').value,'Original synthetic concept assumption');assert.match(document.body.textContent,/draft is retained/);
});

test('editing any input invalidates the accepted preview before Apply',async()=>{
  for(const [label,value] of [['Projection (m)','0.9'],['Source / assumption','Revised explicit source'],['Rationale and applicability','Revised applicability']]){
    await setup();await documented();await click('Preview in Rust');assert.equal(button('Apply to B').disabled,false);
    await set(label,value);assert.equal(document.querySelector('[aria-label="Proposal preview"]'),null);assert.equal(button('Apply to B').disabled,true);assert.deepEqual(commitCalls,[]);assert.equal(field(label).value,value);
  }
});

test('late preview for a changed document context is ignored rather than applied to current inputs',async()=>{
  await setup();await documented();const pending=deferred();previewImpl=()=>pending.promise;await click('Preview in Rust');assert.equal(button('Preview in Rust').disabled,true);
  project=structuredClone(project);project.energy.designs.B.model.zones[0].name='Updated proposed revision';await draw();
  await act(async()=>pending.resolve(reply()));assert.equal(document.querySelector('[aria-label="Proposal preview"]'),null);assert.equal(button('Apply to B').disabled,true);assert.deepEqual(commitCalls,[]);assert.match(document.body.textContent,/Inputs changed during preview/);
});

test('Save replacement and simulation lifecycle cancellation preserves the draft and blocks the transition',async()=>{
  for(const reason of ['Save','replace document','run simulation']){
    await setup();await documented();await set('Projection (m)','0.81');const waiting=await request(reason);assert.equal(waiting.settled,false);assert.match(guard().textContent,new RegExp(reason));
    assert.equal(button('Apply',guard()).disabled,true);await click('Cancel',guard());assert.equal(waiting.value,false);assert.equal(guard(),null);assert.equal(field('Projection (m)').value,'0.81');assert.equal(closed,0);assert.deepEqual(commitCalls,[]);
  }
});

test('lifecycle Discard closes without accepting an input or invoking preview',async()=>{
  await setup();await documented();await set('Projection (m)','0.77');const waiting=await request('replace document');await click('Discard',guard());
  assert.equal(waiting.value,true);assert.equal(closed,1);assert.deepEqual(commitCalls,[]);assert.deepEqual(previewCalls,[]);assert.deepEqual(activities,[true,false]);
});

test('unmount cancels a waiting lifecycle request rather than allowing document replacement',async()=>{
  await setup();await documented();const waiting=await request('replace document');assert.equal(waiting.settled,false);
  await act(async()=>root.unmount());root=null;assert.equal(waiting.value,false);assert.equal(lifecycle,null);assert.deepEqual(activities,[true,false]);assert.deepEqual(commitCalls,[]);
});

test('lifecycle Apply commits one valid preview and releases the pending transition only on success',async()=>{
  await setup();await documented();await click('Preview in Rust');const waiting=await request('Save');commitImpl=async()=>false;await click('Apply',guard());
  assert.equal(waiting.settled,false);assert.ok(guard());assert.equal(commitCalls.length,1);assert.equal(closed,0);
  commitImpl=async()=>true;await click('Apply',guard());assert.equal(waiting.value,true);assert.equal(commitCalls.length,2);assert.equal(closed,1);
});

test('Escape requests Apply Discard Cancel and Cancel leaves the draft available',async()=>{
  await setup();await documented();const event=new Event('cancel',{bubbles:true,cancelable:true});await act(async()=>document.querySelector('dialog').dispatchEvent(event));assert.equal(event.defaultPrevented,true);assert.ok(guard());
  await click('Cancel',guard());assert.equal(guard(),null);assert.equal(field('Source / assumption').value,'Original synthetic concept assumption');assert.equal(closed,0);assert.deepEqual(commitCalls,[]);
});

test('source/rationale limits count UTF-8 bytes and invalid multibyte input survives failed Preview',async()=>{
  await setup();await documented();const exact='界'.repeat(682)+'ab';assert.equal(new TextEncoder().encode(exact).length,2048);
  await set('Rationale and applicability',exact);assert.match(field('Rationale and applicability').parentElement.textContent,/0 UTF-8 bytes remaining/);await click('Preview in Rust');assert.equal(previewCalls.length,1);
  const over=exact+'界';await set('Rationale and applicability',over);assert.match(field('Rationale and applicability').parentElement.textContent,/-3 UTF-8 bytes remaining/);await click('Preview in Rust');assert.equal(previewCalls.length,1);assert.equal(field('Rationale and applicability').value,over);assert.match(document.body.textContent,/2,048 UTF-8 bytes/);assert.deepEqual(commitCalls,[]);
  await set('Rationale and applicability','Declared assumption');await set('Source / assumption',over);await click('Preview in Rust');assert.equal(previewCalls.length,1);assert.equal(field('Source / assumption').value,over);assert.equal(button('Apply to B').disabled,true);
});

test('supported window and glazing forms send explicit typed values without arbitrary reference edits',async()=>{
  await setup();await documented();await set('Hypothesis','put_window');await set('Stable window ID (new or existing)','original_new_window');await set('Width (m)','1.6');await set('Height (m)','1.2');await click('Preview in Rust');
  let action=previewCalls.at(-1).args.draft.action;assert.equal(action.kind,'put_window');assert.equal(action.zone_id,selectedZone);assert.equal(action.window.id,'original_new_window');assert.equal(action.window.width_m,1.6);assert.equal(action.window.height_m,1.2);assert.equal(action.window.overhang,null);assert.deepEqual(commitCalls,[]);
  await set('Hypothesis','assign_window_glazing');await set('New glazing ID','original_explicit_glazing');await set('U-value (W/m²K)','2.4');await set('SHGC (0–1)','0.35');await set('Visible transmittance (0–1)','0.6');await click('Preview in Rust');
  action=previewCalls.at(-1).args.draft.action;assert.equal(action.kind,'assign_window_glazing');assert.equal(action.glazing.u_value_w_m2k,2.4);assert.equal(action.glazing.shgc,.35);assert.equal(action.glazing.visible_transmittance,.6);assert.equal(action.glazing.source.synthetic,true);assert.equal(action.glazing.source.citation,'Original synthetic concept assumption');assert.deepEqual(commitCalls,[]);
});

test('changing zones loads that opening and glazing rather than applying values from the previous target',async()=>{
  await setup(p=>{
    const m=p.energy.designs.B.model,[first,second]=m.zones;
    const firstGlass={...structuredClone(m.glazing[0]),id:'synthetic_first_glazing',u_value_w_m2k:2.4,shgc:.36,visible_transmittance:.61};
    const secondGlass={...structuredClone(m.glazing[0]),id:'synthetic_second_glazing',u_value_w_m2k:3.1,shgc:.52,visible_transmittance:.72};
    m.glazing.push(firstGlass,secondGlass);
    Object.assign(first.windows[0],{glazing_id:firstGlass.id,width_m:2.2,height_m:1.6,offset_u_m:1.1,offset_v_m:.8,overhang:{projection_m:.7,side_extension_m:.2,gap_above_m:.15}});
    Object.assign(second.windows[0],{glazing_id:secondGlass.id,width_m:1.4,height_m:1.1,offset_u_m:1.3,offset_v_m:.9,overhang:{projection_m:.4,side_extension_m:.12,gap_above_m:.08}});
  });
  const [first,second]=project.energy.designs.B.model.zones;
  await documented();await set('Hypothesis','put_window');
  assert.equal(field('Explicit glazing').value,first.windows[0].glazing_id);
  await click('Preview in Rust');assert.equal(button('Apply to B').disabled,false);
  await set('Zone in B',second.id);
  assert.equal(button('Apply to B').disabled,true);assert.equal(document.querySelector('[aria-label="Proposal preview"]'),null);
  assert.equal(field('Existing window').value,second.windows[0].id);assert.equal(field('Stable window ID (new or existing)').value,second.windows[0].id);
  assert.equal(field('Explicit glazing').value,second.windows[0].glazing_id);assert.equal(field('Surface').value,second.windows[0].surface_id);
  assert.equal(field('Width (m)').value,'1.4');assert.equal(field('Height (m)').value,'1.1');assert.equal(field('Horizontal offset on face (m)').value,'1.3');assert.equal(field('Vertical offset on face (m)').value,'0.9');
  await click('Preview in Rust');assert.deepEqual(previewCalls.at(-1).args.draft.action.window,second.windows[0]);assert.equal(previewCalls.at(-1).args.draft.action.zone_id,second.id);
  await set('Hypothesis','assign_window_glazing');
  assert.equal(field('U-value (W/m²K)').value,'3.1');assert.equal(field('SHGC (0–1)').value,'0.52');assert.equal(field('Visible transmittance (0–1)').value,'0.72');
  await click('Preview in Rust');assert.equal(previewCalls.at(-1).args.draft.action.window_id,second.windows[0].id);assert.equal(previewCalls.at(-1).args.draft.action.glazing.u_value_w_m2k,3.1);
  await set('Zone in B',first.id);assert.equal(field('U-value (W/m²K)').value,'2.4');assert.equal(field('SHGC (0–1)').value,'0.36');assert.equal(field('Visible transmittance (0–1)').value,'0.61');
  await set('Hypothesis','set_overhang');assert.equal(field('Projection (m)').value,'0.7');assert.equal(field('Side extension (m)').value,'0.2');assert.equal(field('Gap above window (m)').value,'0.15');
  assert.equal(field('Source / assumption').value,'Original synthetic concept assumption');assert.deepEqual(commitCalls,[]);
});

test('selecting a zone without windows does not retain another zone opening ID or glazing values',async()=>{
  await setup(p=>{p.energy.designs.B.model.zones[1].windows=[];});
  const model=project.energy.designs.B.model,empty=model.zones[1];await documented();await set('Hypothesis','put_window');await set('Width (m)','9.5');await set('Zone in B',empty.id);
  assert.equal(field('Existing window').value,'');assert.equal(field('Stable window ID (new or existing)').value,'proposed_window');assert.equal(field('Width (m)').value,'2');assert.equal(field('Height (m)').value,'1.5');assert.equal(field('Surface').value,empty.surfaces[0].id);assert.equal(field('Explicit glazing').value,model.glazing[0].id);
  await click('Preview in Rust');assert.equal(previewCalls.at(-1).args.draft.action.zone_id,empty.id);assert.equal(previewCalls.at(-1).args.draft.action.window.id,'proposed_window');assert.deepEqual(commitCalls,[]);
});

const originalShades=[{id:'synthetic_east_shade',origin_m:[11,-4,.5],size_m:[2,.45,3.2]},{id:'synthetic_west_shade',origin_m:[-6,-2,1.2],size_m:[1.7,.8,2.1]}];
function withShades(p){p.energy.designs.B.model.detached_shading=structuredClone(originalShades);}

test('existing shade initializes all accepted coordinates and a height-only edit preserves every other dimension',async()=>{
  await setup(withShades);const original=JSON.stringify(project);await documented();await set('Hypothesis','put_detached_shade');
  assert.equal(field('Detached shade target').value,originalShades[0].id);assert.equal(field('Stable shade ID (new or existing)').readOnly,true);
  for(const [label,value] of [['Origin x (m)','11'],['Origin y (m)','-4'],['Origin z (m)','0.5'],['Width X (m)','2'],['Depth Y (m)','0.45'],['Height Z (m)','3.2']])assert.equal(field(label).value,value);
  await set('Height Z (m)','3.6');await click('Preview in Rust');
  assert.deepEqual(previewCalls[0].args.draft.action,{kind:'put_detached_shade',shade:{...originalShades[0],size_m:[2,.45,3.6]}});
  assert.equal(JSON.stringify(project),original);assert.deepEqual(commitCalls,[]);
  await click('Apply to B');assert.equal(commitCalls.length,1);assert.deepEqual(commitCalls[0].draft,previewCalls[0].args.draft);assert.equal(JSON.stringify(project),original);
});

test('selecting another existing shade loads its geometry and invalidates the previous preview',async()=>{
  await setup(withShades);await documented();await set('Hypothesis','put_detached_shade');await set('Width X (m)','5');await click('Preview in Rust');
  assert.equal(button('Apply to B').disabled,false);await set('Detached shade target',originalShades[1].id);
  assert.equal(button('Apply to B').disabled,true);assert.equal(document.querySelector('[aria-label="Proposal preview"]'),null);
  assert.equal(field('Stable shade ID (new or existing)').value,originalShades[1].id);
  for(const [label,value] of [['Origin x (m)','-6'],['Origin y (m)','-2'],['Origin z (m)','1.2'],['Width X (m)','1.7'],['Depth Y (m)','0.8'],['Height Z (m)','2.1']])assert.equal(field(label).value,value);
  await set('Depth Y (m)','0.9');await click('Preview in Rust');assert.deepEqual(previewCalls.at(-1).args.draft.action,{kind:'put_detached_shade',shade:{...originalShades[1],size_m:[1.7,.9,2.1]}});
  await set('Hypothesis','remove_detached_shade');assert.equal(field('Detached shade').value,originalShades[1].id);await set('Detached shade',originalShades[0].id);await click('Preview in Rust');assert.deepEqual(previewCalls.at(-1).args.draft.action,{kind:'remove_detached_shade',shade_id:originalShades[0].id});
  await set('Hypothesis','put_detached_shade');assert.equal(field('Detached shade target').value,originalShades[0].id);assert.equal(field('Depth Y (m)').value,'0.45');assert.deepEqual(commitCalls,[]);
});

test('new shade requires an explicit unique ID and does not implicitly replace an existing shade',async()=>{
  await setup(withShades);const original=JSON.stringify(project);await documented();await set('Hypothesis','put_detached_shade');await set('Detached shade target','');
  assert.equal(field('Stable shade ID (new or existing)').readOnly,false);assert.match(document.body.textContent,/editable synthetic starting values/);
  await set('Stable shade ID (new or existing)',originalShades[1].id);await click('Preview in Rust');
  assert.equal(previewCalls.length,0);assert.equal(field('Stable shade ID (new or existing)').value,originalShades[1].id);assert.match(document.body.textContent,/shade ID already exists/);assert.equal(button('Apply to B').disabled,true);
  await set('Stable shade ID (new or existing)','synthetic_new_shade');await set('Origin x (m)','15');await set('Origin y (m)','-5');await set('Origin z (m)','0');await set('Width X (m)','2.2');await set('Depth Y (m)','0.4');await set('Height Z (m)','2.8');await click('Preview in Rust');
  assert.deepEqual(previewCalls[0].args.draft.action,{kind:'put_detached_shade',shade:{id:'synthetic_new_shade',origin_m:[15,-5,0],size_m:[2.2,.4,2.8]}});assert.equal(JSON.stringify(project),original);assert.deepEqual(commitCalls,[]);
  await set('Hypothesis','remove_detached_shade');await click('Preview in Rust');assert.equal(previewCalls.length,1);assert.match(document.body.textContent,/Select an existing detached shade to remove/);assert.equal(JSON.stringify(project),original);
});

test.after(async()=>{if(root)await act(async()=>root.unmount());delete globalThis.__proposalIPC;dom.window.close();});
