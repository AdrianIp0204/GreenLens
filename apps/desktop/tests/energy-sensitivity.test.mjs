import {inspectionCore} from './inspection-ipc.mjs';
// Real ReactDOM and component effects with explicitly synthetic IPC. No native claim.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {JSDOM} from 'jsdom';
import {transformWithOxc} from 'vite';
const dom=new JSDOM('<!doctype html><div id="root"></div>',{url:'https://greenlens.local/'});
for(const name of ['window','document','HTMLElement','HTMLInputElement','HTMLSelectElement','Event','MouseEvent'])globalThis[name]=dom.window[name];
globalThis.IS_REACT_ACT_ENVIRONMENT=true;
const React=await import('react'),{act}=React,{createRoot}=await import('react-dom/client');
const cache=new Map();
async function load(filename){if(cache.has(filename))return cache.get(filename);const task=(async()=>{const {code}=await transformWithOxc(fs.readFileSync(filename,'utf8'),filename,{jsx:{runtime:'automatic'}});let linked=code.replace(/import\s+['"][^'"]+\.css['"];?/g,'');for(const m of [...code.matchAll(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g)]){const spec=m[3];let target;if(spec==='@tauri-apps/api/core')target=inspectionCore('__sensitivityIPC');else if(spec.endsWith('workspace/useDocument'))target='data:text/javascript,'+encodeURIComponent('export const errorText=e=>String(e)');else if(spec.startsWith('.')){let resolved=path.resolve(path.dirname(filename),spec);if(!path.extname(resolved))resolved=['.ts','.tsx','.mjs'].map(e=>resolved+e).find(fs.existsSync);target=resolved.endsWith('.mjs')?pathToFileURL(resolved).href:await load(resolved);}else target=import.meta.resolve(spec);linked=linked.replace(m[0],`import ${m[1]} from ${JSON.stringify(target)};`);}return`data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`;})();cache.set(filename,task);return task;}

const {default:Panel}=await import(await load(fileURLToPath(new URL('../src/energy/EnergySensitivity.tsx',import.meta.url))));
const {default:CommandBar}=await import(await load(fileURLToPath(new URL('../src/energy/EnergyCommandBar.tsx',import.meta.url))));
let root,doc,calls,session,plan,inputIdentity;
async function setup(){if(root)await act(async()=>root.unmount());document.body.innerHTML='<div id="root"></div>';root=createRoot(document.getElementById('root'));calls=[];
inputIdentity='inputs';
plan={created_utc:'2026-01-01T00:00:00Z',current_inputs_sha256:'inputs',declaration_sha256:'declared',slots:[{slot_id:'A10',reuse:{run_id:'old-a',finished_utc:'2026-01-01T00:00:00Z'}},{slot_id:'A5',reuse:null}],declaration:{annual_electricity_limit_percent:1,common_peak_limit_percent:3,occupied_cooling_unmet_limit_percent:1,cooling_tolerance_c:.2},reuse_policy:'Earliest exact evidence only'};
session={reference:{id:'session',plan_sha256:'plan'},plan,next_slot_id:'A5',active_slot_id:null,active_job_id:null,completed:{},interruptions:[],finalized:false,execution_policy:'One attempt, no retry',result:null};
doc={documentId:'document',path:'synthetic-not-executed',project:{energy:{workbench:{sensitivity:[]}}},energySensitivity:async(action,engine)=>{calls.push([action,engine]);if(action.kind==='plan')return{plan};if(action.kind==='retain'){doc.project={energy:{workbench:{sensitivity:[session.reference]}}};return{session};}return{session};}};
globalThis.__sensitivityIPC=async(command,args)=>{calls.push([command,args]);return command==='energy_sensitivity_input_identity'?inputIdentity:session;};await draw();}
async function draw(disabled=false){await act(async()=>root.render(React.createElement(Panel,{doc,engineDir:'synthetic-engine-no-execution',disabled})));}
const button=s=>[...document.querySelectorAll('button')].find(b=>b.textContent===s);
async function click(s){const b=button(s);assert.ok(b,s);await act(async()=>b.dispatchEvent(new MouseEvent('click',{bubbles:true})));}
test('review and retain do not launch; only explicit next dispatches one slot',async()=>{await setup();await click('Review finite run plan');assert.equal(calls[0][0].kind,'plan');assert.match(document.body.textContent,/Reuse old-a/);assert.match(document.body.textContent,/One full-year native run required/);await click('Retain this declaration');await draw();assert.equal(calls.filter(c=>c[0]?.kind==='next').length,0);await click('Run next declared case (A5)');assert.equal(calls.filter(c=>c[0]?.kind==='next').length,1);assert.match(document.body.textContent,/not physical validation/);});
test('failed attempt cannot launch next and outcome is visible without qualification promotion',async()=>{await setup();doc.project.energy.workbench.sensitivity=[session.reference];session={...session,next_slot_id:null,interruptions:[{kind:'cancelled',reason:'Synthetic cancellation receipt'}]};await draw();assert.equal(button('Run next declared case').disabled,true);assert.match(document.body.textContent,/Synthetic cancellation receipt/);assert.equal(button('Finalize numerical assessment').disabled,false);await draw(true);assert.equal(button('Finalize numerical assessment').disabled,true);});
test('final result is tied to declaration; stale service response is rejected',async()=>{await setup();doc.project.energy.workbench.sensitivity=[session.reference];session={...session,next_slot_id:null,finalized:true,result:{current_inputs_match:false,assessment:{numerical_sensitivity_passed:false,all_declared_runs_integrity_checked:true,criteria:[{id:'energy',alternative_id:'A',quantity:'Annual kWh',zone_id:null,ten_minute:102,five_minute_reference:100,measured_percent:2,limit_percent:1,passed:false,explanation:'Frozen ratio'}],missing_slots:[],limitations:['Not validation'],qualification_promoted:false}}};await draw();assert.match(document.body.textContent,/criteria not passed/);assert.match(document.body.textContent,/Historical/);assert.match(document.body.textContent,/qualification is unchanged/);assert.equal(button('Run next declared case'),undefined);session={...session,reference:{...session.reference,plan_sha256:'other'}};doc.project={...doc.project,name:'changed'};await draw();assert.match(document.querySelector('[role="alert"]').textContent,/selected immutable declaration/);assert.doesNotMatch(document.body.textContent,/Annual kWh/);});
test('command entry dispatches exact registry actions, rejects shell text and respects Escape',async()=>{await setup();let invoked=0;await act(async()=>root.render(React.createElement(CommandBar,{commands:[{name:'Run',description:'Real shared run action',run:()=>{invoked++;}},{name:'Unavailable',description:'bounded',disabled:'Import weather first',run:()=>{throw Error('must not run');}}]})));const input=document.querySelector('input');const type=async v=>act(async()=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,v);input.dispatchEvent(new Event('input',{bubbles:true}));});await type('rUn');await click('Execute');assert.equal(invoked,1);await type('Run; arbitrary-shell');await click('Execute');assert.equal(invoked,1);assert.match(document.body.textContent,/exact supported command/);await type('Unavailable');await click('Execute');assert.match(document.body.textContent,/Import weather first/);await act(async()=>input.dispatchEvent(new window.KeyboardEvent('keydown',{key:'Escape',bubbles:true})));assert.equal(input.value,'');assert.equal(document.querySelector('[role="alert"]'),null);});

// These tests exercise the real panel and useEnergyRead effects. The supplied
// snapshot represents an already checked backend response; no mocked response
// is evidence that an annual run passed integrity or numerical verification.
function installSnapshot(){
  session={...session,next_slot_id:null,finalized:true,result:{
    current_inputs_match:true,
    assessment:{numerical_sensitivity_passed:false,all_declared_runs_integrity_checked:true,
      criteria:[{id:'energy',alternative_id:'A',quantity:'Annual kWh',zone_id:null,
        ten_minute:102,five_minute_reference:100,measured_percent:2,limit_percent:1,
        passed:false,explanation:'Frozen ratio'}],missing_slots:[],
      limitations:['Synthetic response fixture, not engine evidence'],qualification_promoted:false}
  }};
  doc.project={...doc.project,energy:{...doc.project.energy,workbench:{sensitivity:[session.reference]}}};
  doc.sensitivitySnapshots={session};
}
function deferred(){let resolve;const promise=new Promise(r=>{resolve=r;});return{promise,resolve};}
const commandCalls=command=>calls.filter(c=>c[0]===command);

test('opening snapshot survives panel remount without another annual evidence inspection',async()=>{
  await setup();installSnapshot();const retained=JSON.stringify(session);
  await draw();
  assert.equal(commandCalls('energy_sensitivity_inspect').length,0);
  assert.equal(commandCalls('energy_sensitivity_input_identity').length,1);
  assert.deepEqual(commandCalls('energy_sensitivity_input_identity')[0][1].alternatives,['A']);
  assert.match(document.body.textContent,/Current-input relationship: Current\./);
  assert.match(document.body.textContent,/Evidence integrity snapshot retained from opening/);
  assert.match(document.body.textContent,/criteria not passed/);
  await act(async()=>root.render(React.createElement('p',null,'Another panel')));
  await draw();
  assert.equal(commandCalls('energy_sensitivity_inspect').length,0);
  assert.equal(commandCalls('energy_sensitivity_input_identity').length,2);
  assert.match(document.body.textContent,/Current-input relationship: Current\./);
  assert.equal(JSON.stringify(session),retained,'display never rewrites the retained assessment');
});

test('accepted-input changes invalidate current status while retaining the historical snapshot without redecoding runs',async()=>{
  await setup();installSnapshot();await draw();const retained=JSON.stringify(session);
  const identity=deferred();inputIdentity=identity.promise;
  doc.project={...doc.project,energy:{...doc.project.energy,model_revision:'different accepted model'}};
  await draw();
  assert.match(document.body.textContent,/Current-input relationship: Checking current input identity\./);
  assert.doesNotMatch(document.body.textContent,/Current-input relationship: Current\./);
  assert.equal(commandCalls('energy_sensitivity_inspect').length,0);
  await act(async()=>identity.resolve('different-input-identity'));
  assert.match(document.body.textContent,/Current-input relationship: Historical — accepted inputs changed\./);
  assert.match(document.body.textContent,/Annual kWh/);
  assert.equal(JSON.stringify(session),retained,'old current_inputs_match is immutable historical evidence');
  assert.equal(commandCalls('energy_sensitivity_inspect').length,0);
});

test('late identity replies cannot mark a changed accepted model current',async()=>{
  await setup();installSnapshot();const oldIdentity=deferred();inputIdentity=oldIdentity.promise;
  await draw();
  inputIdentity='changed-inputs';
  doc.project={...doc.project,energy:{...doc.project.energy,model_revision:'new revision'}};
  await draw();
  assert.match(document.body.textContent,/Current-input relationship: Historical — accepted inputs changed\./);
  await act(async()=>oldIdentity.resolve('inputs'));
  assert.match(document.body.textContent,/Current-input relationship: Historical — accepted inputs changed\./);
  assert.equal(commandCalls('energy_sensitivity_input_identity').length,2);
  assert.equal(commandCalls('energy_sensitivity_inspect').length,0);
});

test('cached sensitivity data with a different immutable declaration is rejected without a fallback traversal',async()=>{
  await setup();installSnapshot();
  doc.sensitivitySnapshots={session:{...session,reference:{...session.reference,plan_sha256:'wrong-plan'}}};
  await draw();
  assert.match(document.querySelector('[role="alert"]').textContent,/selected immutable declaration/);
  assert.doesNotMatch(document.body.textContent,/Annual kWh/);
  assert.doesNotMatch(document.body.textContent,/Current-input relationship/);
  assert.equal(commandCalls('energy_sensitivity_inspect').length,0);
  assert.equal(commandCalls('energy_sensitivity_input_identity').length,0);
});

test.after(async()=>{if(root)await act(async()=>root.unmount());});
