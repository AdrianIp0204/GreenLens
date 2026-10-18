// Real report component, capacity panel, transport guard and read queue. IPC
// replies are explicitly synthetic integration fixtures, never native evidence.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {JSDOM} from 'jsdom';
import {transformWithOxc} from 'vite';
import {inspectionCore} from './inspection-ipc.mjs';
import {designReportFixture,designFixtureId,designFixtureA,designFixtureB} from './design-report-fixture.mjs';

const dom=new JSDOM('<!doctype html><div id="root"></div>',{url:'https://greenlens.local/'});
for(const key of ['window','document','HTMLElement','HTMLInputElement','HTMLTextAreaElement','HTMLSelectElement','Event','MouseEvent'])globalThis[key]=dom.window[key];
globalThis.IS_REACT_ACT_ENVIRONMENT=true;
const React=await import('react'),{act}=React,{createRoot}=await import('react-dom/client');
const cache=new Map();
async function load(file){if(cache.has(file))return cache.get(file);const job=(async()=>{
  const {code}=await transformWithOxc(fs.readFileSync(file,'utf8'),file,{jsx:{runtime:'automatic'}});let linked=code.replace(/import\s+['"][^'"]+\.css['"];?/g,'');
  for(const match of [...linked.matchAll(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g)]){const spec=match[3];let target;
    if(spec==='@tauri-apps/api/core')target=inspectionCore('__designReportsIPC');
    else if(spec.endsWith('workspace/useDocument'))target='data:text/javascript,export const errorText=e=>String(e);';
    else if(spec.endsWith('?url'))target='data:text/javascript,'+encodeURIComponent('export default '+JSON.stringify(import.meta.resolve(spec.slice(0,-4))));
    else if(spec.startsWith('.')){let resolved=path.resolve(path.dirname(file),spec);if(!path.extname(resolved))resolved=['.ts','.tsx','.mjs'].map(ext=>resolved+ext).find(fs.existsSync);target=resolved.endsWith('.mjs')?pathToFileURL(resolved).href:await load(resolved);}
    else target=import.meta.resolve(spec);
    linked=linked.replace(match[0],`import ${match[1]} from ${JSON.stringify(target)};`);
  }return'data:text/javascript;base64,'+Buffer.from(linked).toString('base64');})();cache.set(file,job);return job;}
const {default:Panel}=await import(await load(fileURLToPath(new URL('../src/energy/DesignReports.tsx',import.meta.url))));
const {useDesignReportDrafts}=await import(await load(fileURLToPath(new URL('../src/energy/useDesignReportDrafts.ts',import.meta.url))));
function Harness(props){const declaration=useDesignReportDrafts(props.project?.id);return React.createElement(Panel,{...props,declaration});}
const id=designFixtureId,productHash='2'.repeat(64);
const project=()=>designReportFixture().project;
const inspection=()=>designReportFixture().inspection;
const capacity=()=>({schema_version:'greenlens.project-capacity.v1',project_content_sha256:'6'.repeat(64),retained_bytes:192,retained_entries:2,available_bytes:268435264,available_entries:4998,limits:[],incremental_lower_bound_bytes:0,incremental_lower_bound_entries:4,estimated_incremental_bytes:null,estimate_basis:'Unknown final report size.',can_start:true,warnings:[],inventory_scope:'Actual file sizes only; not integrity.',remedy:'Preserve the complete study.'});
const deferred=()=>{let resolve,reject;const promise=new Promise((yes,no)=>{resolve=yes;reject=no;});return{promise,resolve,reject};};
let root,props,calls,actions,answer,actionAnswer,pairSelections;
async function render(){await act(async()=>root.render(React.createElement(Harness,props)));}
async function setup(){if(root)await act(()=>root.unmount());window.localStorage.clear();document.body.innerHTML='<div id="root"></div>';root=createRoot(document.getElementById('root'));calls=[];actions=[];pairSelections=0;answer=async command=>{if(command==='project_capacity')return capacity();if(command==='design_report_inspect')return inspection();throw Error(command);};actionAnswer=async()=>undefined;
  globalThis.__designReportsIPC=async(command,args)=>{calls.push([command,args]);return answer(command,args);};
  props={path:'owned-synthetic-project',project:project(),a:designFixtureA,b:designFixtureB,disabled:false,htmlOnly:true,pending:{items:[],warning:''},onSelectPair:()=>pairSelections++,onAction:async action=>{actions.push(action);return actionAnswer(action);}};await render();}
const button=label=>{const found=[...document.querySelectorAll('button')].find(node=>node.textContent===label);assert.ok(found,`Button ${label}`);return found;};
async function click(label){await act(async()=>button(label).click());}
async function value(label,text){const element=document.querySelector(`[aria-label="${label}"]`);assert.ok(element,label);const proto=element.tagName==='SELECT'?HTMLSelectElement.prototype:element.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:HTMLInputElement.prototype;await act(async()=>{Object.getOwnPropertyDescriptor(proto,'value').set.call(element,text);element.dispatchEvent(new Event(element.tagName==='SELECT'?'change':'input',{bubbles:true}));});}
async function select(){await value('Retained analytical report',id);}

test('real form requires explicit scope and exact selected pair; HTML-only is an explicit action',async()=>{
  await setup();assert.equal(button('Issue analytical design report').disabled,true);assert.match(document.body.textContent,new RegExp(`Selected A: ${designFixtureA}`));assert.match(document.body.textContent,/no architectural reference|Imported reference scope/);
  await value('Design report analytical scope','Four original rectangular zones; plant excluded.');await value('Design report north declaration','');assert.equal(button('Issue analytical design report').disabled,false);
  await click('Issue analytical design report');assert.deepEqual(actions,[{kind:'create',a:designFixtureA,b:designFixtureB,scope:'Four original rectangular zones; plant excluded.',north:null,html_only:true}]);assert.match(document.body.textContent,/No new report accepted/);
  await value('Design report analytical scope','界'.repeat(683));assert.match(document.body.textContent,/2049\/2,048 UTF-8 bytes/);assert.equal(button('Issue analytical design report').disabled,true);
  await click('Select genuine A/B evidence');assert.equal(pairSelections,1);assert.ok(calls.some(([command,args])=>command==='project_capacity'&&args.operation.kind==='design_report'));
});

test('retained exact pair is distinct from current selection and missing sensitivity stays visible',async()=>{
  await setup();await select();assert.match(document.body.textContent,/Matches the selected pair/);assert.match(document.body.textContent,/Exact-pair numerical sensitivity is missing/);assert.match(document.body.textContent,/Retained baseline · unassessed/);
  const request=calls.find(([command])=>command==='design_report_inspect')[1];assert.equal(request.id,id);assert.equal(request.project,props.project);
  props={...props,a:'different-current-a'};await render();assert.match(document.body.textContent,/Historical pair; current inputs and selected runs are not substituted/);assert.match(document.body.textContent,new RegExp(`Retained A: ${designFixtureA}; B: ${designFixtureB}`));assert.match(document.body.textContent,/Four original synthetic zones/);
  await click('Regenerate exact products…');assert.deepEqual(actions,[{kind:'regenerate',id}]);assert.ok(calls.every(([command])=>!command.includes('start')));
});

test('mismatched inspection is rejected and a late old-document response remains hidden',async()=>{
  await setup();answer=async command=>command==='project_capacity'?capacity():{...inspection(),manifest_file_sha256:'f'.repeat(64)};await select();assert.match(document.body.textContent,/differs from the selected immutable evidence/);assert.doesNotMatch(document.body.textContent,/Retained A:/);
  await setup();const old=deferred();answer=async command=>command==='project_capacity'?capacity():old.promise;await select();assert.match(document.body.textContent,/Verifying immutable design products/);
  props={...props,path:'different-owned-project',project:{...project(),id:'second-document',design_reports:{schema_version:'greenlens.design-report-index.v1',reports:[]}}};await render();await act(async()=>old.resolve(inspection()));
  assert.doesNotMatch(document.body.textContent,/Retained A:|Four original synthetic zones/);assert.equal(document.querySelector('[aria-label="Retained analytical report"]').value,'');assert.equal(document.querySelector('figure'),null);
});

test('preview checks exact issued product hash and real hook never treats handoff as observed display',async()=>{
  await setup();await select();let wrong=true;answer=async(command,args)=>{
    if(command==='project_capacity')return capacity();if(command==='design_report_inspect')return inspection();
    if(command==='report_issue_product'&&args.action==='preview')return{issue_id:id,path:args.product,sha256:wrong?'9'.repeat(64):productHash,media_type:'image/svg+xml',svg:'<svg xmlns="http://www.w3.org/2000/svg"><text>Original synthetic figure</text></svg>'};
    if(command==='report_issue_product')return{issue_id:id,path:args.product,sha256:'5'.repeat(64),status:'prepared',integrity:'verified',copy:'verified',prepared:{path:'owned copy/Report 空間.pdf',sha256:'5'.repeat(64),bytes:128,reused:true},handoff:'requested',viewer_display:'not_observed'};throw Error(command);};
  await click('Preview design-axonometric-a.svg');assert.match(document.body.textContent,/Product differs from the exact retained report/);assert.equal(document.querySelector('figure'),null);
  wrong=false;await click('Preview design-axonometric-a.svg');assert.match(document.querySelector('figure').textContent,new RegExp(id));assert.match(document.querySelector('figure img').getAttribute('src'),/^data:image\/svg\+xml/);
  await click('Open design-executive.pdf');assert.match(document.body.textContent,/Windows handoff requested; display is not yet observed/);const request=calls.filter(([command])=>command==='report_issue_product').at(-1)[1];assert.equal(request.designReport,true);assert.equal(request.id,id);
});

test('complete export and pending publication recovery are explicit; cancellation preserves accepted index',async()=>{
  await setup();const before=JSON.stringify(props.project);props={...props,pending:{items:[{token:'synthetic-owned-token',message:'Complete products retained after a conflict.'}],warning:'Reload and reconcile before recovery.'}};await render();
  await click('Recover completed design report');assert.deepEqual(actions,[{kind:'recover',token:'synthetic-owned-token'}]);assert.match(document.body.textContent,/No new report accepted/);assert.equal(JSON.stringify(props.project),before);
  actionAnswer=async()=>({destination:'owned complete study.zip'});await click('Export complete design study ZIP…');assert.deepEqual(actions.at(-1),{kind:'export'});assert.match(document.body.textContent,/Completed output: owned complete study.zip/);assert.equal(JSON.stringify(props.project),before);
  await select();answer=async(command,args)=>command==='project_capacity'?capacity():command==='design_report_inspect'?inspection():{issue_id:id,path:args.product,sha256:'5'.repeat(64),status:'cancelled',integrity:'verified',copy:'not_prepared',handoff:'not_requested',viewer_display:'not_observed'};
  await click('Save Copy design-executive.pdf…');assert.match(document.body.textContent,/Save Copy cancelled; no viewer requested/);assert.equal(JSON.stringify(props.project),before);
});

test('document replacement hides late issuance acknowledgement without inventing an accepted issue',async()=>{
  await setup();await value('Design report analytical scope','Explicit synthetic scope.');const old=deferred();actionAnswer=()=>old.promise;await click('Issue analytical design report');assert.match(document.body.textContent,/Working with exact retained evidence/);
  props={...props,project:{...project(),id:'replacement-document'},path:'new-owned-path'};await render();await act(async()=>old.resolve({status:'issued',report:inspection()}));
  assert.doesNotMatch(document.body.textContent,/Immutable analytical report accepted/);assert.equal(document.querySelector('[aria-label="Retained analytical report"]').value,'');assert.equal(document.querySelector('[aria-label="Design report analytical scope"]').value,'');
});

test.after(async()=>{if(root)await act(()=>root.unmount());});
