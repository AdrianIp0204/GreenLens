// Real React effects, shared hook and inspection controls. Explicit IPC doubles
// verify lifecycle wiring, never native evidence integrity or engine execution.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {JSDOM} from 'jsdom';
import {transformWithOxc} from 'vite';
const dom=new JSDOM('<div id="root"></div>');
for(const name of ['window','document','HTMLElement','Event','MouseEvent'])globalThis[name]=dom.window[name];
globalThis.IS_REACT_ACT_ENVIRONMENT=true;
const React=await import('react'),{act}=React,{createRoot}=await import('react-dom/client');
const cache=new Map();
async function load(file){
  if(cache.has(file))return cache.get(file);
  const pending=(async()=>{
    const{code}=await transformWithOxc(fs.readFileSync(file,'utf8'),file,{jsx:{runtime:'automatic'}});let linked=code;
    for(const m of [...code.matchAll(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g)]){
      let target;
      if(m[3]==='@tauri-apps/api/core')target='data:text/javascript,'+encodeURIComponent('export const invoke=(...args)=>globalThis.__inspectionIPC(...args); export class Channel {onmessage=()=>{};}');
      else if(m[3]==='../workspace/useDocument')target='data:text/javascript,export const errorText=e=>String(e);';
      else if(m[3].startsWith('.')){let f=path.resolve(path.dirname(file),m[3]);if(!path.extname(f))f=['.ts','.tsx','.mjs'].map(ext=>f+ext).find(fs.existsSync);target=f.endsWith('.mjs')?pathToFileURL(f).href:await load(f);}
      else target=import.meta.resolve(m[3]);
      linked=linked.replace(m[0],`import ${m[1]} from ${JSON.stringify(target)};`);
    }
    return'data:text/javascript;base64,'+Buffer.from(linked).toString('base64');
  })();cache.set(file,pending);return pending;
}
const src=fileURLToPath(new URL('../src/energy/',import.meta.url));
const{useEnergyRead}=await import(await load(path.join(src,'useEnergyRead.ts')));
const{default:Inspection}=await import(await load(path.join(src,'EvidenceInspection.tsx')));
const deferred=()=>{let resolve,reject;const promise=new Promise((yes,no)=>{resolve=yes;reject=no;});return{promise,resolve,reject};};
const progress=(patch={})=>({schema_version:'greenlens.project-open-progress.v1',stage:'energy_evidence',run_id:'run-a',runs_completed:0,runs_total:1,bytes_read:1048576,files_verified:2,numeric_passes:{},elapsed_ms:25,...patch});
let root,command,args,read,calls,nextTicket,prepare,inspect,direct,cancelReply;
function Harness(){read=useEnergyRead(command,args);return React.createElement(React.Fragment,null,React.createElement(Inspection,{label:'Reading selected evidence',read}),read.error&&React.createElement('p',{role:'alert'},read.error),read.value&&React.createElement('output',null,JSON.stringify(read.value)));}
async function draw(){await act(async()=>root.render(React.createElement(Harness)));}
async function setup(){
  if(root)await act(()=>root.unmount());root=createRoot(document.getElementById('root'));
  command='energy_report_preflight';args={path:'owned-project',project:{id:'original'},a:'a',b:'b'};calls=[];nextTicket=1;
  prepare=async()=>nextTicket++;inspect=()=>new Promise(()=>{});direct=async()=>({local:true});cancelReply=async()=>{};
  globalThis.__inspectionIPC=(c,a)=>{calls.push([c,a]);if(c==='prepare_evidence_inspection')return prepare();if(c==='inspect_energy_evidence')return inspect(a);if(c==='cancel_evidence_inspection')return cancelReply(a);return direct(c,a);};
}
const requests=()=>calls.filter(([c])=>c==='inspect_energy_evidence').map(([,a])=>a);
const cancellations=()=>calls.filter(([c])=>c==='cancel_evidence_inspection').map(([,a])=>a.ticket);
async function clickCancel(){const button=document.querySelector('button');assert.ok(button);await act(async()=>button.click());}

test('real hook reserves bounded inspection, displays actual stage/bytes and passes original arguments',async()=>{
  await setup();const answer=deferred();inspect=()=>answer.promise;await draw();assert.equal(read.loading,true);assert.match(document.body.textContent,/Starting evidence inspection/);
  const query=requests()[0];assert.deepEqual(query.query,{command,arguments:args});assert.equal(query.ticket,1);
  await act(()=>query.onProgress.onmessage(progress()));assert.match(document.body.textContent,/energy evidence · 1.0 MiB read · 0\/1 runs/);assert.doesNotMatch(document.body.textContent,/%|verified|complete/);
  await act(async()=>answer.resolve({ready:true}));assert.equal(read.loading,false);assert.deepEqual(read.value,{ready:true});assert.equal(document.querySelector('button'),null);assert.deepEqual(cancellations(),[]);
});

test('explicit cancellation sends the reserved ticket once and gates late progress and success',async()=>{
  await setup();const answer=deferred();inspect=()=>answer.promise;await draw();const query=requests()[0];await clickCancel();assert.deepEqual(cancellations(),[1]);assert.equal(read.loading,false);assert.equal(read.value,null);assert.match(read.error,/cancelled/);
  await act(()=>{query.onProgress.onmessage(progress({bytes_read:999999}));answer.resolve({unchecked:true});});assert.equal(read.progress,null);assert.equal(read.value,null);assert.match(read.error,/cancelled/);await act(()=>read.cancel());assert.deepEqual(cancellations(),[1]);
});

test('a cancelled late reservation is released without starting evidence work',async()=>{
  await setup();const reservation=deferred();prepare=()=>reservation.promise;await draw();await clickCancel();assert.deepEqual(cancellations(),[]);await act(async()=>reservation.resolve(17));assert.deepEqual(cancellations(),[17]);assert.deepEqual(requests(),[]);assert.equal(read.value,null);
});

test('unmount releases a reservation arriving later and never starts inspection',async()=>{
  await setup();const reservation=deferred();prepare=()=>reservation.promise;await draw();await act(()=>root.unmount());root=null;await act(async()=>reservation.resolve(23));assert.deepEqual(cancellations(),[23]);assert.deepEqual(requests(),[]);
});

test('changing exact inputs cancels the old inspection and old responses cannot overwrite fresh state',async()=>{
  await setup();const old=deferred(),fresh=deferred();inspect=q=>q.ticket===1?old.promise:fresh.promise;await draw();const previous=requests()[0];args={...args,project:{...args.project,accepted_input_revision:'changed'}};await draw();assert.deepEqual(cancellations(),[1]);assert.equal(read.value,null);
  await act(async()=>fresh.resolve({current:'fresh'}));assert.deepEqual(read.value,{current:'fresh'});await act(()=>{previous.onProgress.onmessage(progress());old.resolve({current:'old'});});assert.deepEqual(read.value,{current:'fresh'});assert.equal(read.progress,null);
  args=null;await draw();assert.equal(read.value,null);assert.equal(read.loading,false);
});

test('changing the query while reservation is pending cancels the old eventual ticket only',async()=>{
  await setup();const old=deferred();let n=0;prepare=()=>++n===1?old.promise:Promise.resolve(2);await draw();args={...args,a:'new-a'};await draw();await act(async()=>old.resolve(1));assert.deepEqual(cancellations(),[1]);assert.equal(requests().length,1);assert.equal(requests()[0].ticket,2);assert.equal(requests()[0].query.arguments.a,'new-a');
});

test('clearing the query cancels active work; completion and failure after cleanup are hidden',async()=>{
  await setup();const answer=deferred();inspect=()=>answer.promise;await draw();args=null;await draw();assert.deepEqual(cancellations(),[1]);await act(async()=>answer.reject(Error('Old evidence failure')));assert.equal(read.value,null);assert.equal(read.error,'');assert.equal(read.loading,false);
});

test('original response seal validation remains mandatory through the inspection envelope',async()=>{
  await setup();command='energy_result';args={runId:'a',project:{energy:{runs:[{id:'a',record_sha256:'exact-seal'}]}}};inspect=async()=>({case:{run_id:'a',record_sha256:'wrong-seal'}});await draw();assert.equal(read.value,null);assert.match(read.error,/selected immutable run/);
  args={...args,refresh:1};inspect=async()=>({case:{run_id:'a',record_sha256:'exact-seal'}});await draw();assert.equal(read.value.case.record_sha256,'exact-seal');
});

test('invalid or nonfinite progress cancels inspection and cannot attest success',async()=>{
  for(const patch of [{schema_version:'unknown'},{bytes_read:Infinity},{runs_completed:2},{numeric_passes:[]},{stage:'x'.repeat(129)}]){
    await setup();const answer=deferred();inspect=()=>answer.promise;await draw();await act(()=>requests()[0].onProgress.onmessage(progress(patch)));assert.deepEqual(cancellations(),[1]);assert.equal(read.loading,false);assert.match(read.error,/progress is invalid/);await act(async()=>answer.resolve({ready:true}));assert.equal(read.value,null);
  }
});

test('bounded backend admission errors and inspection errors remain visible without retry',async()=>{
  await setup();prepare=async()=>{throw Error('At most 32 simultaneous evidence operations are allowed.');};await draw();assert.match(read.error,/32 simultaneous/);assert.equal(read.loading,false);assert.equal(requests().length,0);assert.equal(calls.length,1);
  args={...args,refresh:1};prepare=async()=>1;inspect=async()=>{throw Error('Changed content seal rejected');};await draw();assert.match(read.error,/content seal/);assert.equal(read.value,null);assert.equal(requests().length,1);
});

test('cancellation failure is explicit and does not permit a later successful response',async()=>{
  await setup();const answer=deferred();inspect=()=>answer.promise;cancelReply=async()=>{throw Error('Reservation transport failed');};await draw();await clickCancel();assert.match(read.error,/Cancellation could not be confirmed/);await act(async()=>answer.resolve({ready:true}));assert.equal(read.value,null);assert.match(read.error,/response remains hidden/);
});

test('non-evidence commands retain direct invocation and never reserve inspection tickets',async()=>{
  await setup();command='energy_dependencies';args={engineDir:null,typstDir:null};await draw();assert.deepEqual(read.value,{local:true});assert.deepEqual(calls,[['energy_dependencies',args]]);assert.equal(read.cancellable,false);
});

test.after(async()=>{if(root)await act(()=>root.unmount());dom.window.close();});
