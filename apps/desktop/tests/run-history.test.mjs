// Actual React effects and table interactions; synthetic IPC, no native claim.
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
async function load(filename){if(cache.has(filename))return cache.get(filename);const task=(async()=>{const {code}=await transformWithOxc(fs.readFileSync(filename,'utf8'),filename,{jsx:{runtime:'automatic'}});let linked=code.replace(/import\s+['"][^'"]+\.css['"];?/g,'');for(const m of [...code.matchAll(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g)]){const spec=m[3];let target;if(spec==='@tauri-apps/api/core')target='data:text/javascript,'+encodeURIComponent('export const invoke=(...a)=>globalThis.__catalogIPC(...a)');else if(spec.endsWith('workspace/useDocument'))target='data:text/javascript,'+encodeURIComponent('export const errorText=e=>String(e)');else if(spec.startsWith('.')){let resolved=path.resolve(path.dirname(filename),spec);if(!path.extname(resolved))resolved=['.ts','.tsx','.mjs'].map(e=>resolved+e).find(fs.existsSync);target=resolved.endsWith('.mjs')?pathToFileURL(resolved).href:await load(resolved);}else target=import.meta.resolve(spec);linked=linked.replace(m[0],`import ${m[1]} from ${JSON.stringify(target)};`);}return`data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`;})();cache.set(filename,task);return task;}
const {default:Panel}=await import(await load(fileURLToPath(new URL('../src/energy/RunHistory.tsx',import.meta.url))));
const sha='a'.repeat(64),method={method_id:'synthetic-method-only',adapter_version:'1',engine_version:'26.2.0',engine_package_sha256:sha,engine_executable_sha256:sha,engine_schema_sha256:sha,recipe_sha256:sha,numerical_settings_sha256:sha};
const project=()=>({id:'fixture-project',energy:{runs:Array.from({length:36},(_,i)=>({id:`run-${i}`,alternative_id:i%2?'B':'A',record_sha256:sha,physical_input_hash:sha,path:`runs/run-${i}`}))}});
function page(args){const {offset,limit,alternative}=args.request,all=args.project.energy.runs.filter(r=>!alternative||r.alternative_id===alternative),selected=all.slice().reverse().slice(offset,offset+limit);return{schema_version:'greenlens.energy-run-catalog.v1',project_sha256:sha,offset,limit,alternative,total:all.length,next_offset:offset+selected.length<all.length?offset+selected.length:null,ordering:'reverse_retention_order',integrity_scope:'Synthetic metadata only; no matrix or native acceptance.',rows:selected.map(s=>({id:s.id,alternative_id:s.alternative_id,name:`Retained <name> ${s.id}`,finished_utc:'2026-10-14T01:02:03.1234567Z',input_revision_sha256:sha,model_input_sha256:sha,physical_input_sha256:sha,record_sha256:sha,method,interval_minutes:10,current_method:s.alternative_id==='A',assessment:{execution:'completed',evidence_integrity:'metadata_checked',current_physical_inputs:s.alternative_id==='A'?'current':'changed',current_processed_results:'not_compared',current_derived_results:'not_compared',scientific:{status:'unassessed',reason:'Synthetic test record; no qualification.',tested_scope:'No native output.'}}}))};}
let root,props,calls,chosen,reply;
async function setup(extra={}){if(root)await act(()=>root.unmount());calls=[];chosen=[];reply=async args=>page(args);globalThis.__catalogIPC=async(command,args)=>{calls.push([command,args]);return reply(args);};props={path:'owned-project',project:project(),onChoose:(a,id)=>chosen.push([a,id]),...extra};root=createRoot(document.getElementById('root'));await act(async()=>root.render(React.createElement(Panel,props)));}
const button=text=>{const el=[...document.querySelectorAll('button')].find(b=>b.textContent===text);assert.ok(el,text);return el;};
async function click(text){await act(async()=>button(text).dispatchEvent(new MouseEvent('click',{bubbles:true})));}
async function filter(value){const el=document.querySelector('select');await act(async()=>{Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(el,value);el.dispatchEvent(new Event('change',{bubbles:true}));});}
async function rerender(extra){props={...props,...extra};await act(async()=>root.render(React.createElement(Panel,props)));}
test('bounded metadata pages show exact UTC, input and method scope; A/B selection is explicit',async()=>{
  await setup();assert.deepEqual(calls[0][1].request,{offset:0,limit:25,alternative:null});assert.equal(document.querySelectorAll('tbody tr').length,25);assert.match(document.body.textContent,/2026-10-14T01:02:03.1234567Z/);assert.match(document.body.textContent,/Historical inputs/);assert.match(document.body.textContent,/Historical method/);assert.match(document.body.textContent,/Current inputs/);assert.match(document.body.textContent,/Metadata checked/);assert.equal(document.querySelector('name'),null);
  await click('Use for proposed B');assert.deepEqual(chosen,[['B','run-35']]);await click('Use for baseline A');assert.deepEqual(chosen.at(-1),['A','run-34']);assert.equal(calls.length,1);
  await click('Next page');assert.equal(calls.at(-1)[1].request.offset,25);assert.equal(document.querySelectorAll('tbody tr').length,11);assert.equal(button('Next page').disabled,true);await filter('A');assert.deepEqual(calls.at(-1)[1].request,{offset:0,limit:25,alternative:'A'});assert.equal(document.querySelectorAll('tbody tr').length,18);assert.equal(button('Previous page').disabled,true);assert.ok(calls.every(([c])=>c==='energy_run_catalog'));
});
test('bad record hashes, oversized rows and inflated full-integrity claims are rejected',async()=>{
  for(const corrupt of [p=>{p.rows[0].record_sha256='b'.repeat(64);},p=>{p.rows.push(...p.rows,...p.rows,...p.rows,...p.rows);},p=>{p.rows[0].assessment.evidence_integrity='complete_retained_evidence_checked';},p=>{p.next_offset=26;},p=>{p.rows[0].finished_utc='2026-10-14T09:02:03+08:00';}]){
    await setup();reply=async args=>{const p=page(args);corrupt(p);return p;};await filter('A');assert.match(document.querySelector('[role="alert"]').textContent,/does not match/);assert.equal(document.querySelector('tbody'),null);
  }
});
test('switching documents hides old rows and stale replies cannot select old evidence',async()=>{
  await setup();let release;reply=args=>new Promise(resolve=>{release=()=>resolve(page(args));});await filter('B');assert.equal(document.querySelector('tbody'),null);assert.match(document.body.textContent,/previous rows are hidden/);const old=release;reply=async args=>page(args);const replacement=project();replacement.id='replacement';replacement.energy.runs=[];await rerender({path:'replacement-project',project:replacement});assert.match(document.body.textContent,/No retained snapshots/);await act(async()=>old());assert.match(document.body.textContent,/No retained snapshots/);assert.equal(document.querySelectorAll('tbody tr').length,0);assert.deepEqual(chosen,[]);
});
test('unsaved and disabled states never invoke an engine or silently select a run',async()=>{
  await setup({path:''});assert.equal(calls.length,0);assert.match(document.body.textContent,/Save the project/);await setup({disabled:true});assert.equal(button('Use for proposed B').disabled,true);assert.deepEqual(chosen,[]);assert.ok(calls.every(([c])=>c==='energy_run_catalog'));
});
test.after(async()=>{if(root)await act(()=>root.unmount());dom.window.close();});
