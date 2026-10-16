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
async function load(filename){if(cache.has(filename))return cache.get(filename);const task=(async()=>{const {code}=await transformWithOxc(fs.readFileSync(filename,'utf8'),filename,{jsx:{runtime:'automatic'}});let linked=code.replace(/import\s+['"][^'"]+\.css['"];?/g,'');for(const m of [...code.matchAll(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g)]){const spec=m[3];let target;if(spec==='@tauri-apps/api/core')target=inspectionCore('__investigationIPC');else if(spec.endsWith('workspace/useDocument'))target='data:text/javascript,'+encodeURIComponent('export const errorText=e=>String(e)');else if(spec.startsWith('.')){let resolved=path.resolve(path.dirname(filename),spec);if(!path.extname(resolved))resolved=['.ts','.tsx','.mjs'].map(e=>resolved+e).find(fs.existsSync);target=resolved.endsWith('.mjs')?pathToFileURL(resolved).href:await load(resolved);}else target=import.meta.resolve(spec);linked=linked.replace(m[0],`import ${m[1]} from ${JSON.stringify(target)};`);}return`data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`;})();cache.set(filename,task);return task;}
const {default:Panel}=await import(await load(fileURLToPath(new URL('../src/energy/EnergyInvestigation.tsx',import.meta.url))));
let root,calls,reply,props;
const project={energy:{runs:[{id:'a',record_sha256:'sha-a'},{id:'b',record_sha256:'sha-b'},{id:'new-a',record_sha256:'sha-new-a'}]}};
function result(id='a',value=2){return{schema_version:'greenlens.energy-investigation.v1',run_id:id,record_sha256:`sha-${id}`,integrity_scope:'Synthetic bounded integrity contract',searched_interval_count:0,data:{window:{start_interval:0,count:2,focus_interval:null},source_interval_minutes:5,demand_10min:[0,1].map(i=>({timestamp:{interval_index:i,month:1,day:1,start_minute:i*10,end_minute:i*10+10},facility_kw:value+i,end_uses_kw:{cooling_system:value+i}})),comfort:[0,1,2,3].map(i=>({timestamp:{source_interval_index:i,common_interval_index:Math.floor(i/2),date:{month:1,day:1},start_minute:i*5,end_minute:i*5+5},samples:[{zone_id:'z',air_temperature_c:i===1?24.3:24,cooling_setpoint_c:24,occupied:true}],occupied_unmet_zone_ids:i===1?['z']:[]}))}};}
const pair=()=>({schema_version:'greenlens.energy-pair-investigation.v1',anchor:'baseline',baseline:result('a'),proposed:result('b',1),shared_demand_scale:{minimum_kw:0,maximum_kw:3},limitations:['Synthetic fixture, not a native result']});
async function setup(extra={}){if(root)await act(()=>root.unmount());calls=[];reply=async(c,a)=>c==='energy_navigate_pair'?{...pair(),anchor:a.anchor}:result(a.runId);globalThis.__investigationIPC=async(c,a)=>{calls.push([c,a]);return reply(c,a);};props={path:'owned-test-project',project,runId:'a',...extra};root=createRoot(document.getElementById('root'));await act(async()=>root.render(React.createElement(Panel,props)));}
async function rerender(extra){props={...props,...extra};await act(async()=>root.render(React.createElement(Panel,props)));}
function button(s){const el=[...document.querySelectorAll('button')].find(x=>x.textContent===s);assert.ok(el,`button ${s}`);return el;}
async function click(s){await act(async()=>button(s).dispatchEvent(new MouseEvent('click',{bubbles:true})));}
function field(s){const el=[...document.querySelectorAll('label')].find(x=>x.firstChild?.textContent===s);assert.ok(el,`field ${s}`);return el.querySelector('input,select');}
async function type(s,value){const el=field(s),proto=el.tagName==='SELECT'?HTMLSelectElement.prototype:HTMLInputElement.prototype;await act(async()=>{Object.getOwnPropertyDescriptor(proto,'value').set.call(el,value);el.dispatchEvent(new Event(el.tagName==='SELECT'?'change':'input',{bubbles:true}));});}
test('date/range/peak/unmet actions invoke only bounded Rust navigation; exact native comfort remains visible',async()=>{
  await setup();assert.equal(calls[0][0],'energy_navigate');assert.deepEqual(calls[0][1].request,{kind:'day',date:{month:1,day:1}});assert.match(document.body.textContent,/24.3/);assert.match(document.body.textContent,/heating\/lower-temperature service is not evaluated/);
  await type('Month and day','12-31');await type('Local time','23:50');await type('Range end day','12-31');await type('Range end time','24:00');await click('Read range (up to seven days)');assert.deepEqual(calls.at(-1)[1].request,{kind:'range',start:{date:{month:12,day:31},minute:1430},end:{date:{month:12,day:31},minute:1440}});
  await click('Facility peak day');assert.equal(calls.at(-1)[1].request.kind,'facility_peak');await type('Retained zone ID (optional)','z');await click('Find next occupied unmet interval');assert.equal(calls.at(-1)[1].request.zone_id,'z');assert.ok(calls.every(([c])=>c==='energy_navigate'));
  const count=calls.length;await type('Local time','00:05');await click('Read interval');assert.equal(calls.length,count);assert.match(document.querySelector('[role="alert"]').textContent,/ten-minute/);
});
test('pair navigation uses one anchor, one shared axis, aligned exact table and no separate own-peak request',async()=>{
  await setup({baselineRunId:'a',proposedRunId:'b'});assert.equal(calls[0][0],'energy_navigate_pair');assert.equal(calls[0][1].anchor,'baseline');assert.equal(document.querySelectorAll('.ei-line').length,2);assert.match(document.querySelector('output').textContent,/Baseline A: 2 kW.*Proposed B: 1 kW/);
  await type('Navigation anchor','proposed');assert.equal(calls.at(-1)[1].anchor,'proposed');await click('Facility peak day');assert.equal(calls.at(-1)[1].request.kind,'facility_peak');assert.equal(calls.at(-1)[1].a,'a');assert.equal(calls.at(-1)[1].b,'b');assert.ok(calls.every(([c])=>c==='energy_navigate_pair'));
});
test('new snapshot hides old values and a stale or mismatched response cannot replace current evidence',async()=>{
  await setup();let release;reply=()=>new Promise(resolve=>{release=resolve;});await rerender({runId:'new-a'});assert.match(document.body.textContent,/previous values are hidden/);assert.equal(document.querySelector('output'),null);await act(async()=>release(result('a')));assert.match(document.querySelector('[role="alert"]').textContent,/selected immutable snapshot/);assert.equal(document.querySelector('output'),null);
});
test('not-found, native query rejection and year boundaries remain explicit without an engine action',async()=>{
  await setup();reply=async()=>({...result('a'),data:null,searched_interval_count:52560});await click('Find next occupied unmet interval');assert.match(document.body.textContent,/No occupied cooling-unmet interval/);assert.equal(document.querySelector('.ei-plot'),null);
  reply=async()=>{throw Error('Rust rejected a range longer than seven days');};await click('Read day');assert.match(document.querySelector('[role="alert"]').textContent,/Rust rejected/);
  const count=calls.length;await click('Previous day');assert.equal(calls.length,count);assert.match(document.querySelector('[role="alert"]').textContent,/year boundary/);
});
