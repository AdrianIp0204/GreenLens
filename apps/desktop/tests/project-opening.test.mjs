// Actual useDocument, draft store and thermal editor. IPC/channel doubles test
// transaction ordering; they do not establish native dialogs or file integrity.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {JSDOM} from 'jsdom';
import {transformWithOxc} from 'vite';

const dom=new JSDOM('<!doctype html><div id="root"></div>',{url:'https://greenlens.local/'});
for(const name of ['window','document','HTMLElement','HTMLInputElement','HTMLTextAreaElement','HTMLSelectElement','Event','MouseEvent'])globalThis[name]=dom.window[name];
globalThis.IS_REACT_ACT_ENVIRONMENT=true;
const React=await import('react');
const {act}=React,{createRoot}=await import('react-dom/client');
const modules=new Map();
async function load(filename){
  if(modules.has(filename))return modules.get(filename);
  const promise=(async()=>{
    const {code}=await transformWithOxc(fs.readFileSync(filename,'utf8'),filename,{jsx:{runtime:'automatic'}});
    let linked=code.replace(/import\s+['"][^'"]+\.css['"];?/g,'');
    for(const match of [...code.matchAll(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g)]){
      const spec=match[3];let target;
      if(spec==='@tauri-apps/api/core')target='data:text/javascript,'+encodeURIComponent('export const invoke=(...a)=>globalThis.__openIPC(...a);export const isTauri=()=>true;export class Channel { onmessage=()=>{}; }');
      else if(spec==='@tauri-apps/api/window')target='data:text/javascript,'+encodeURIComponent('export const getCurrentWindow=()=>globalThis.__openWindow;');
      else if(spec.startsWith('.')){
        let resolved=path.resolve(path.dirname(filename),spec);
        if(!path.extname(resolved))resolved=['.ts','.tsx','.mjs'].map(ext=>resolved+ext).find(f=>fs.existsSync(f));
        target=resolved.endsWith('.mjs')?pathToFileURL(resolved).href:await load(resolved);
      }else target=import.meta.resolve(spec);
      linked=linked.replace(match[0],`import ${match[1]} from ${JSON.stringify(target)};`);
    }
    return `data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`;
  })();modules.set(filename,promise);return promise;
}
const src=fileURLToPath(new URL('../src/',import.meta.url));
const {useDocument}=await import(await load(path.join(src,'workspace/useDocument.ts')));
const {default:Editor}=await import(await load(path.join(src,'energy/ThermalEditor.tsx')));
const {default:DraftGuard}=await import(await load(path.join(src,'energy/EnergyDraftGuard.tsx')));
const {default:ReferenceDraftGuard}=await import(await load(path.join(src,'reference/ReferenceDraftGuard.tsx')));
const {newReferenceDraft,newImportDraft,newScopeDraft}=await import(await load(path.join(src,'reference/reference-drafts.ts')));
const inputs=JSON.parse(fs.readFileSync(new URL('../../../examples/energy/revision1-inputs.json',import.meta.url)));
const daylight=JSON.parse(fs.readFileSync(new URL('./viewport-fixture.json',import.meta.url)));
const deferred=()=>{let resolve,reject;const promise=new Promise((yes,no)=>{resolve=yes;reject=no;});return{promise,resolve,reject};};
function fixture(id='original-document'){
  return{schema_version:'0.4.0',id,name:`Synthetic ${id}`,alternatives:['A','B'].map(id=>({id,model:structuredClone(daylight.model)})),runs:[],energy:{designs:Object.fromEntries(['A','B'].map(id=>[id,{model:structuredClone(inputs.cases[id].model),study:structuredClone(inputs.cases[id].study)}])),weather:[],runs:[],carbon_factor:null,lighting_service_evidence:null}};
}
const admission={schema_version:'greenlens.project-admission.v1',project_id:'candidate-document',project_name:'Candidate document',manifest_sha256:'a'.repeat(64),static_runs:0,climate_runs:0,energy_runs:4,evidence_checked:false};
const progress={schema_version:'greenlens.project-open-progress.v1',stage:'energy_evidence',run_id:'checked-run',runs_completed:1,runs_total:4,bytes_read:1048576,files_verified:9,numeric_passes:{'checked-run':1},elapsed_ms:125};
const receipt={schema_version:'greenlens.project-verification.v1',started_utc:'2026-01-01T00:00:00Z',finished_utc:'2026-01-01T00:00:01Z',manifest_sha256:admission.manifest_sha256,content_snapshot_sha256:'b'.repeat(64),progress:{...progress,stage:'completed',runs_completed:4},scope:'Synthetic IPC fixture only; no native verification claim'};
function view(project,verified=false){return{project:structuredClone(project),revision:verified?admission.manifest_sha256:'saved-revision',results:[],climate_results:[],energy_results:[],project_verification:verified?receipt:null,energy_sensitivity_sessions:{}};}
let root,doc,calls,gate,prepareGate,admissionError,saveError,verificationError,pendingBackend,verificationArgs,selectedPath,savePath;
function Harness(){
  doc=useDocument();const draft=doc.energyDrafts.get(doc.alternative);
  return React.createElement(React.Fragment,null,
    ...['Open','Save','Run'].map(label=>React.createElement('button',{key:label,onClick:()=>void ({Open:()=>doc.replace('Open'),Save:doc.save,Run:()=>doc.startEnergy('')})[label]()},label)),
    doc.opening&&React.createElement('div',{'aria-label':'Verification progress'},doc.opening.progress?.stage??'Starting',React.createElement('button',{onClick:()=>void doc.cancelOpening()},'Cancel opening')),
    doc.guard&&React.createElement('section',{'aria-label':'Document replacement',role:'dialog'},...['save','discard','cancel'].map(choice=>React.createElement('button',{key:choice,onClick:()=>doc.resolveGuard(choice)},choice))),
    draft&&React.createElement(Editor,{entry:draft,zoneId:draft.structured.model.zones[0].id,disabled:doc.busy||Boolean(doc.energyDrafts.guard),onChange:change=>doc.energyDrafts.update(doc.alternative,change),onApply:()=>doc.energyDrafts.apply(doc.alternative),onDiscard:()=>doc.energyDrafts.discard(doc.alternative)}),
    React.createElement(DraftGuard,{drafts:doc.energyDrafts}),
    React.createElement(ReferenceDraftGuard,{drafts:doc.referenceDrafts}));
}
async function setup({saved=false,pending=false}={}){
  if(root){await act(()=>root.unmount());root=null;}
  window.localStorage.clear();calls=[];gate=deferred();prepareGate=null;admissionError=null;saveError=null;verificationError=null;pendingBackend=null;verificationArgs=null;selectedPath='candidate-project';savePath='original-project';
  globalThis.__openWindow={onCloseRequested:async()=>()=>{},destroy:async()=>calls.push(['destroy'])};
  globalThis.__openIPC=async(command,args)=>{
    calls.push([command,args]);
    if(command==='example')return fixture();
    if(command==='study_defaults')return{calculation:{mode:'static_overcast'}};
    if(command==='validate')return{...daylight,model:args.model};
    if(command==='energy_validate')return{model:args.model,surfaces:[],gross_floor_area_m2:1};
    if(command==='energy_edit'){const next=structuredClone(args.project);next.energy.designs[args.alternativeId]={model:args.model,study:args.study};return next;}
    if(command==='energy_restore_designs'){const next=structuredClone(args.project);Object.assign(next.energy.designs,args.designs);return next;}
    if(command==='dialog_open_project')return selectedPath;
    if(command==='admit_project'){if(admissionError)throw admissionError;return admission;}
    if(command==='prepare_project_verification')return prepareGate?prepareGate.promise:41;
    if(command==='verify_project'){verificationArgs=args;args.onProgress.onmessage(progress);if(verificationError)throw verificationError;return gate.promise;}
    if(command==='cancel_project_verification')return;
    if(command==='dialog_save_project')return savePath;
    if(command==='save_project'||command==='save_project_as'){if(saveError)throw saveError;return view(args.project);}
    if(command==='energy_start'){pendingBackend={id:'pending-run',state:'completed',pending_evidence:true,energy_result:{id:'pending-run',alternative_id:'A'},log:[]};return pendingBackend;}
    if(command==='collect_run')throw Error('Synthetic external save conflict; completed evidence retained');
    if(command==='cancel_and_wait')return pendingBackend;
    if(command==='discard_job'){pendingBackend=null;return;}
    throw Error(`Unexpected IPC ${command}`);
  };
  root=createRoot(document.getElementById('root'));await act(async()=>root.render(React.createElement(Harness)));
  if(saved||pending)await act(async()=>doc.save());
  if(pending)await act(async()=>doc.startEnergy(''));
  calls=[];
}
const count=command=>calls.filter(c=>c[0]===command).length;
const replacement=()=>document.querySelector('[aria-label="Document replacement"]');
const draftDialog=()=>document.querySelector('[aria-labelledby="energy-draft-title"]');
async function click(text,scope=document){const button=[...scope.querySelectorAll('button')].find(b=>b.textContent===text);assert.ok(button,`Button ${text}`);await act(async()=>button.dispatchEvent(new MouseEvent('click',{bubbles:true})));}
function input(label){const field=[...document.querySelectorAll('label')].find(l=>l.firstChild?.textContent===label);assert.ok(field,`Field ${label}`);return field.querySelector('input,textarea,select');}
async function type(element,value){const prototype=element.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:HTMLInputElement.prototype;await act(async()=>{Object.getOwnPropertyDescriptor(prototype,'value').set.call(element,value);element.dispatchEvent(new Event('input',{bubbles:true}));});}
async function finish(){await act(async()=>gate.resolve(view(fixture('candidate-document'),true)));}

test('cheap admission precedes draft and replacement guards; Cancel performs no verification or document mutation',async()=>{
  await setup();await type(input('Name'),'Retained exact draft');const before=structuredClone(doc.project);
  await click('Open');assert.deepEqual(calls.map(c=>c[0]),['dialog_open_project','admit_project']);assert.ok(draftDialog());assert.equal(replacement(),null);
  await click('Cancel',draftDialog());assert.deepEqual(doc.project,before);assert.equal(input('Name').value,'Retained exact draft');assert.equal(count('verify_project'),0);assert.equal(count('discard_job'),0);
  await click('Open');await click('Discard',draftDialog());assert.ok(replacement());assert.equal(count('verify_project'),0);
  await click('cancel',replacement());assert.deepEqual(doc.project,before);assert.equal(count('prepare_project_verification'),0);assert.equal(count('discard_job'),0);
});

test('one guarded verification exposes progress then atomically accepts the candidate before resetting history',async()=>{
  await setup();await type(input('Name'),'Accepted edit');await click('Validate and apply');const before=structuredClone(doc.project);assert.equal(doc.history.past.length,1);
  await click('Open');assert.ok(replacement());assert.equal(count('verify_project'),0);await click('discard',replacement());
  assert.equal(count('admit_project'),1);assert.equal(count('prepare_project_verification'),1);assert.equal(count('verify_project'),1);assert.equal(count('open_project'),0);assert.equal(count('discard_job'),0);assert.deepEqual(doc.project,before);assert.equal(doc.history.past.length,1);assert.equal(doc.opening.progress.bytes_read,1048576);
  await finish();assert.equal(doc.project.id,'candidate-document');assert.equal(doc.path,'candidate-project');assert.equal(doc.history.past.length,0);assert.equal(doc.projectVerification.content_snapshot_sha256,receipt.content_snapshot_sha256);assert.equal(doc.opening,null);
  assert.ok(calls.findIndex(c=>c[0]==='verify_project')<calls.findIndex(c=>c[0]==='cancel_and_wait'));assert.equal(count('discard_job'),1);
});

test('cancel followed by a late successful response preserves the previous document and pending completed evidence',async()=>{
  await setup({pending:true});assert.equal(doc.pending,true);const before=structuredClone(doc.project),job=structuredClone(doc.job),revision=doc.revision;
  await click('Open');await click('discard',replacement());assert.ok(doc.opening);assert.equal(count('cancel_and_wait'),0);assert.equal(count('discard_job'),0);
  await click('Cancel opening');assert.equal(doc.opening.cancelled,true);assert.deepEqual(calls.find(c=>c[0]==='cancel_project_verification')[1],{ticket:41});
  await finish();assert.deepEqual(doc.project,before);assert.deepEqual(doc.job,job);assert.equal(doc.pending,true);assert.equal(doc.revision,revision);assert.equal(doc.path,'original-project');assert.equal(doc.opening,null);assert.match(doc.error,/cancelled/);assert.equal(count('discard_job'),0);assert.equal(count('cancel_and_wait'),0);assert.ok(pendingBackend);
});

test('verification failure or an external revision conflict retains accepted state, history and pending evidence',async()=>{
  for(const reason of ['Synthetic malformed checksum','Project changed since admission; choose it again']){
    await setup({pending:true});const before=structuredClone(doc.project),job=structuredClone(doc.job),history=structuredClone(doc.history.past);verificationError=Error(reason);
    await click('Open');await click('discard',replacement());assert.equal(doc.opening,null);assert.match(doc.error,new RegExp(reason.split(';')[0]));assert.deepEqual(doc.project,before);assert.deepEqual(doc.job,job);assert.deepEqual(doc.history.past,history);assert.equal(doc.pending,true);assert.equal(count('discard_job'),0);assert.equal(count('cancel_and_wait'),0);
  }
});

test('invalid input admission and picker cancellation do not consume drafts or request destructive guards',async()=>{
  await setup();await type(input('Name'),'Private local draft');const before=structuredClone(doc.project);admissionError=Error('Unsupported project schema');
  await click('Open');assert.match(doc.error,/Unsupported project/);assert.equal(draftDialog(),null);assert.equal(replacement(),null);assert.equal(input('Name').value,'Private local draft');assert.deepEqual(doc.project,before);assert.equal(count('verify_project'),0);
  admissionError=null;selectedPath=null;calls=[];await click('Open');assert.deepEqual(calls.map(c=>c[0]),['dialog_open_project']);assert.equal(input('Name').value,'Private local draft');assert.equal(doc.energyDrafts.pending.length,1);
});

test('invalid actual-editor draft Apply retains text and both histories and never starts verification',async()=>{
  await setup();const before=structuredClone(doc.project);await type(input('Lighting W / m²'),'-');await click('Open');await click('Apply',draftDialog());
  assert.match(draftDialog().textContent,/finite numeric/);assert.equal(input('Lighting W / m²').value,'-');assert.deepEqual(doc.project,before);assert.equal(doc.history.past.length,0);assert.equal(doc.history.future.length,0);assert.equal(count('energy_restore_designs'),0);assert.equal(count('verify_project'),0);assert.equal(count('discard_job'),0);
  await click('Cancel',draftDialog());assert.equal(input('Lighting W / m²').value,'-');
});

test('failed or cancelled pre-replacement Save never starts verification or discards evidence',async()=>{
  for(const mode of ['failure','cancel']){
    await setup();const before=structuredClone(doc.project);if(mode==='failure')saveError=Error('Synthetic save conflict');else savePath=null;
    await click('Open');await click('save',replacement());assert.deepEqual(doc.project,before);assert.equal(count('verify_project'),0);assert.equal(count('discard_job'),0);if(mode==='failure')assert.match(doc.error,/save conflict/);
  }
});

test('another Open cannot begin while verification is active; unmount cancels the ticket and ignores late success',async()=>{
  await setup({saved:true});await click('Open');assert.ok(doc.opening);const before=structuredClone(doc.project);
  await click('Open');assert.equal(count('dialog_open_project'),1);assert.equal(count('verify_project'),1);assert.match(doc.error,/current document operation/);
  await act(()=>root.unmount());root=null;assert.equal(count('cancel_project_verification'),1);await finish();assert.deepEqual(doc.project,before);assert.equal(count('discard_job'),0);
});

test('unmount while a verification reservation is pending cancels the late ticket without starting or accepting a candidate',async()=>{
  await setup({saved:true});prepareGate=deferred();const before=structuredClone(doc.project);
  await click('Open');assert.equal(count('prepare_project_verification'),1);assert.equal(doc.opening,null);assert.equal(count('verify_project'),0);
  await act(()=>root.unmount());root=null;
  await act(async()=>prepareGate.resolve(41));
  const verifications=count('verify_project');
  await finish();
  assert.equal(verifications,0);assert.equal(count('cancel_project_verification'),1);assert.equal(count('discard_job'),0);assert.deepEqual(doc.project,before);
});

test('actual document Save and Open resolve reference drafts before saving, verification or pending-evidence discard',async()=>{
  await setup({pending:true});const before=structuredClone(doc.project),job=structuredClone(doc.job);
  const draft=newReferenceDraft(doc.project.id);draft.import={...newImportDraft(),scale:'-',dimensions_checked:true,orientation_checked:true};await act(async()=>doc.referenceDrafts.update(draft));
  const refDialog=()=>document.querySelector('[aria-labelledby="reference-draft-title"]');
  await click('Save');assert.ok(refDialog());assert.equal(count('save_project'),0);await click('Apply',refDialog());assert.match(refDialog().textContent,/finite number/);assert.deepEqual(doc.project,before);assert.deepEqual(doc.job,job);assert.equal(doc.pending,true);assert.equal(doc.history.past.length,0);await click('Cancel',refDialog());
  calls=[];await click('Open');assert.deepEqual(calls.map(c=>c[0]),['dialog_open_project','admit_project']);assert.ok(refDialog());assert.equal(replacement(),null);assert.equal(count('verify_project'),0);await click('Cancel',refDialog());assert.equal(doc.referenceDrafts.draft.import.scale,'-');assert.equal(doc.pending,true);assert.equal(count('discard_job'),0);
});

test('actual document guard rejects combined import and scope Apply without a partial accepted action',async()=>{
  await setup({saved:true});const before=structuredClone(doc.project),draft=newReferenceDraft(doc.project.id);
  draft.import={...newImportDraft(),dimensions_checked:true,orientation_checked:true};draft.scope=newScopeDraft({revision_sha256:'1'.repeat(64),geometry_sha256:'2'.repeat(64)},{reference_options_sha256:'3'.repeat(64),designs:{A:{model_input_sha256:'a'.repeat(64),targets:[]}}});
  await act(async()=>doc.referenceDrafts.update(draft));await click('Save');const dialog=document.querySelector('[aria-labelledby="reference-draft-title"]');await click('Apply',dialog);
  assert.match(dialog.textContent,/import or scope separately/);assert.deepEqual(doc.project,before);assert.deepEqual(doc.referenceDrafts.draft,draft);assert.equal(count('reference_import'),0);assert.equal(count('reference_scope_apply'),0);assert.equal(count('save_project'),0);assert.equal(doc.history.past.length,0);await click('Cancel',dialog);
});

test.after(async()=>{if(root)await act(()=>root.unmount());dom.window.close();});
