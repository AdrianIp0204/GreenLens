// Actual ReactDOM, editor and hook effects; explicit IPC doubles are not native acceptance.
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
const cache=new Map();
async function load(filename){
  if(cache.has(filename))return cache.get(filename);
  const promise=(async()=>{
    const {code}=await transformWithOxc(fs.readFileSync(filename,'utf8'),filename,{jsx:{runtime:'automatic'}});
    let linked=code.replace(/import\s+['"][^'"]+\.css['"];?/g,'');
    for(const m of [...code.matchAll(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g)]){
      const spec=m[3];let target;
      if(spec==='@tauri-apps/api/core')target='data:text/javascript,'+encodeURIComponent('export const invoke=(...a)=>globalThis.__draftIPC(...a);export const isTauri=()=>true;export class Channel { onmessage=()=>{}; }');
      else if(spec==='@tauri-apps/api/window')target='data:text/javascript,'+encodeURIComponent('export const getCurrentWindow=()=>globalThis.__draftWindow;');
      else if(spec.startsWith('.')){
        let resolved=path.resolve(path.dirname(filename),spec);
        if(!path.extname(resolved))resolved=['.ts','.tsx','.mjs'].map(ext=>resolved+ext).find(f=>fs.existsSync(f));
        target=resolved.endsWith('.mjs')?pathToFileURL(resolved).href:await load(resolved);
      }else target=import.meta.resolve(spec);
      linked=linked.replace(m[0],`import ${m[1]} from ${JSON.stringify(target)};`);
    }
    return `data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`;
  })();cache.set(filename,promise);return promise;
}
const src=fileURLToPath(new URL('../src/',import.meta.url));
const {default:Editor}=await import(await load(path.join(src,'energy/ThermalEditor.tsx')));
const {default:Guard}=await import(await load(path.join(src,'energy/EnergyDraftGuard.tsx')));
const {default:ReferenceGuard}=await import(await load(path.join(src,'reference/ReferenceDraftGuard.tsx')));
const {useDocument}=await import(await load(path.join(src,'workspace/useDocument.ts')));
const {default:Workspace}=await import(await load(path.join(src,'energy/EnergyWorkspace.tsx')));
const {EnergyDraftStore,fieldEdit,structuredEdit,cleanDraft,draftCandidate}=await import(await load(path.join(src,'energy/drafts.ts')));
const {ThermalPresentationStore,defaultThermalPresentation,validThermalPresentation}=await import(await load(path.join(src,'energy/thermal-presentation.ts')));
const inputs=JSON.parse(fs.readFileSync(new URL('../../../examples/energy/revision1-inputs.json',import.meta.url)));
const daylight=JSON.parse(fs.readFileSync(new URL('./viewport-fixture.json',import.meta.url)));
let doc,root,calls,rejectEdit=false,closeHandler,disk,fullWorkspace=false,appendEvidenceOnSave=false,metadataGate=null,sceneObjects=false,preflightIssues=[],referenceImportGate=null,referenceScopeGate=null,reportRuns=false,reportGate=null,reportError=null,saveDestination='test-project';
let referenceManifestGate=null,referenceRevision='2'.repeat(64);
dom.window.HTMLDialogElement.prototype.showModal=function(){this.open=true;};
// Real ThermalView follows its WebGL-unavailable branch; no viewport acceptance claim.
dom.window.HTMLCanvasElement.prototype.getContext=()=>{throw Error('No WebGL context in DOM test');};
function fixture(){return {schema_version:'0.4.0',id:'draft-fixture',name:'Draft fixture',alternatives:['A','B'].map(id=>({id,model:structuredClone(daylight.model)})),runs:[],energy:{designs:Object.fromEntries(['A','B'].map(id=>[id,{model:structuredClone(inputs.cases[id].model),study:structuredClone(inputs.cases[id].study)}])),weather:[],runs:reportRuns?[{id:'report-a',alternative_id:'A'},{id:'report-b',alternative_id:'B'}]:[],carbon_factor:null,lighting_service_evidence:null}};}
function view(p){return{project:structuredClone(p),revision:'test-revision',results:[],climate_results:[],energy_results:[]};}
const referenceHash=value=>value.repeat(64);
const referenceAsset={id:'synthetic-reference',path:'references/synthetic/manifest.json',revision_sha256:referenceHash('2'),manifest_sha256:referenceHash('4'),geometry_sha256:referenceHash('3')};
function referenceManifest(){return{schema_version:'greenlens.reference-model.v1',importer_id:'greenlens.glb-static-triangles/1',source_sha256:referenceHash('1'),revision_sha256:referenceHash('2'),geometry_sha256:referenceHash('3'),options:{scene_index:null,metres_per_source_unit:1,clockwise_rotation_deg:0,translation_m:[0,0,0],declared_north_clockwise_deg:null,conversion:null},source_units:'metres',coordinate_convention:'Right-handed X/Y/Z-up',selected_scene:0,source_node_count:1,source_mesh_count:1,source_material_count:0,source_texture_count:0,occurrences:[{id:'synthetic-occurrence',node_index:0,parent_id:null,label:'Original fixture',node_path:[0],source_mesh_index:0,world_from_mesh:[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],mirrored:false,bounds:{min_m:[0,0,0],max_m:[1,1,0]},triangle_count:1,triangle_area_m2:.5}],materials:[],chunks:[{path:'geometry/00000.bin',sha256:referenceHash('5'),byte_length:88,triangle_count:1}],bounds:{min_m:[0,0,0],max_m:[1,1,0]},triangle_count:1,triangle_area_m2:.5,warnings:[],exclusions:[],storage_policy:'Original synthetic IPC fixture only',resources:{elapsed_ms:1,source_bytes:100,normalized_bytes:88,json_bytes:100,decoded_allocation_estimate_bytes:200}};}
function referenceContext(project){return{reference_options_sha256:referenceHash('6'),designs:Object.fromEntries(Object.entries(project.energy.designs).map(([id,design])=>[id,{model_input_sha256:referenceHash(id==='A'?'a':'b'),targets:design.model.zones.map(z=>({object_id:z.id,kind:'zone',label:z.name}))}]))};}
function referenceAssessment(){return{schema_version:'greenlens.architectural-scope-assessment.v1',scope_sha256:referenceHash('7'),report_binding_sha256:referenceHash('8'),relationship:'current',stale_reasons:[],coverage_percent:null,reconciliations:[],unresolved_mapping_count:1,missing_assumption_count:0,disclosure:'Synthetic IPC fixture only'};}
function Harness(){
  doc=useDocument();const [show,setShow]=React.useState(true),draft=doc.energyDrafts.get(doc.alternative);
  return React.createElement(React.Fragment,null,
    React.createElement('button',{onClick:()=>setShow(s=>!s)},'Toggle panel'),
    React.createElement('button',{onClick:()=>setShow(false)},'Daylight'),
    React.createElement('button',{onClick:()=>setShow(true)},'Energy'),
    React.createElement('button',{onClick:()=>doc.setAlternative(doc.alternative==='A'?'B':'A')},'Switch design'),
    React.createElement('button',{onClick:()=>{const requests=Object.fromEntries(['A','B'].map(id=>{const candidate=structuredClone(doc.project.energy.designs[id]);candidate.model.name=`Grouped ${id}`;return[id,{candidate}];}));void doc.commitEnergyTransaction(requests,'explicit-test-preview','Two-design geometry');}},'Group geometry'),
    ...['Save','Save As','Run','Open','Close','Undo'].map(label=>React.createElement('button',{key:label,onClick:()=>void ({Save:doc.save,'Save As':doc.saveAs,Run:()=>doc.startEnergy(''),Open:()=>doc.replace('Open'),Close:()=>doc.replace('Close'),Undo:()=>doc.traverse('undo')})[label]()},label)),
    show&&fullWorkspace&&React.createElement(Workspace,{doc}),
    show&&!fullWorkspace&&draft&&React.createElement(Editor,{entry:draft,zoneId:draft.structured.model.zones[0].id,disabled:doc.busy||Boolean(doc.energyDrafts.guard),onChange:change=>doc.energyDrafts.update(doc.alternative,change),onApply:()=>doc.energyDrafts.apply(doc.alternative),onDiscard:()=>doc.energyDrafts.discard(doc.alternative)}),
    React.createElement(Guard,{drafts:doc.energyDrafts}),React.createElement(ReferenceGuard,{drafts:doc.referenceDrafts}));
}
async function setup({retainCache=false,workspace=false,objects=false,reports=false}={}){
  fullWorkspace=workspace;
  if(root)await act(()=>root.unmount());if(!retainCache)window.localStorage.clear();
  calls=[];rejectEdit=false;appendEvidenceOnSave=false;metadataGate=null;reportRuns=reports;reportGate=null;reportError=null;saveDestination='test-project';disk=fixture();sceneObjects=objects;preflightIssues=[];referenceImportGate=null;referenceScopeGate=null;
  referenceManifestGate=null;referenceRevision=referenceHash('2');
  globalThis.__draftWindow={onCloseRequested:async cb=>{closeHandler=cb;return()=>{};},destroy:async()=>calls.push(['destroy'])};
  globalThis.__draftIPC=async(command,args)=>{
    calls.push([command,args]);
    if(command==='example')return fixture();
    if(command==='study_defaults')return{calculation:{mode:'static_overcast'}};
    if(command==='validate')return {...daylight,model:args.model};
    if(command==='energy_validate')return{model:args.model,surfaces:[],gross_floor_area_m2:1};
    if(command==='energy_default_runs')return reportRuns?{A:'report-a',B:'report-b'}:{};
    if(command==='energy_result')return null;
    if(command==='energy_report_preflight')return{schema_version:'greenlens.energy-attachment-preflight.v1',ready:true,items:[]};
    if(command==='energy_sensitivity_export_selection')return{included_session_id:null,included_run_ids:['report-a','report-b'],excluded_session_ids:[],policy:'Original synthetic retained-run selection'};
    if(command==='dialog_energy_report')return 'synthetic-report-output';
    if(command==='energy_report'){if(reportGate)await reportGate;if(reportError)throw reportError;return{artifacts:[{path:'executive.html'}],renderer_status:'Synthetic IPC completion only',dataset_sha256:'d'.repeat(64)};}
    if(command==='energy_open_report')return;
    if(command==='energy_scene')return{schema_version:'greenlens.thermal-scene.v1',model_sha256:'synthetic',coordinates:'world ENU',objects:sceneObjects?args.model.zones.flatMap(z=>[{id:z.id,label:z.name,kind:'zone',zone_id:z.id,storey:z.storey,polygons_m:[]},...z.surfaces.map(s=>({id:s.id,label:s.id,kind:'surface',zone_id:z.id,storey:z.storey,polygons_m:[]})),...z.windows.flatMap(w=>[{id:w.id,label:w.id,kind:'window',zone_id:z.id,storey:z.storey,polygons_m:[]},...(w.overhang?[{id:`overhang:${w.id}`,label:`Overhang ${w.id}`,kind:'overhang',zone_id:z.id,storey:z.storey,polygons_m:[]}]:[])])]):[]};
    if(command==='energy_preflight')return{schema_version:'greenlens.thermal-preflight.v1',can_prepare:false,coverage:'Synthetic issue-routing fixture; no native validation claim.',issues:preflightIssues};
    if(command==='energy_dependencies')return{schema_version:'greenlens.energy-dependencies.v1',energyplus_ready:true,typst_and_fonts_ready:true,issues:[]};
    if(command==='energy_metadata'){if(metadataGate)await metadataGate;if(rejectEdit)throw Error('Synthetic report settings rejection');const next=structuredClone(args.project);if(args.change.kind==='report_issue'){const m=args.change.metadata;if(m&&(!m.title||!m.revision||m.issue_date&&!/^\d{4}-\d{2}-\d{2}$/.test(m.issue_date)))throw Error('Synthetic explicit report issue validation rejection');next.energy.workbench={...(next.energy.workbench??{locks:{},calendars:{}}),report_metadata:m};}else Object.assign(next.energy,{carbon_factor:args.change.carbon_factor,lighting_service_evidence:args.change.lighting_service_evidence});return next;}
    if(command==='energy_new'){
      if(rejectEdit)throw Error('Synthetic new-project validation rejection');
      const next=fixture();next.id='new-thermal-project';for(const design of Object.values(next.energy.designs)){design.model.zones=[{...design.model.zones[0],...args.zone}];design.study=null;}return next;
    }
    if(command==='energy_operation_plan'){
      const requests=Object.fromEntries(Object.keys(args.operations).map(id=>{const candidate=structuredClone(args.project.energy.designs[id]);candidate.model.name=`Operation ${id}`;return[id,{candidate}];}));
      return{requests,preview:{preview_sha256:'explicit-test-preview',valid:true,untouched_designs:['A','B'].filter(id=>!requests[id]),designs:Object.fromEntries(Object.entries(requests).map(([id,r])=>[id,{candidate:r.candidate.model,valid:true,problems:[],affected_object_ids:['synthetic-zone']}]))}};
    }
    if(command==='energy_transaction_commit'){
      if(rejectEdit)throw Error('Synthetic atomic geometry rejection');assert.equal(args.expectedPreviewSha256,'explicit-test-preview');
      const next=structuredClone(args.project);for(const [id,request] of Object.entries(args.requests))next.energy.designs[id]=request.candidate;return next;
    }
    if(command==='energy_restore_designs'){
      if(rejectEdit)throw Error('Synthetic atomic restore rejection');const next=structuredClone(args.project);Object.assign(next.energy.designs,args.designs);return next;
    }
    if(command==='energy_edit'){
      if(rejectEdit)throw Error('Synthetic validator rejected the cooling input');
      const next=structuredClone(args.project);next.energy.designs[args.alternativeId]={model:args.model,study:args.study};return next;
    }
    if(command==='dialog_save_project')return saveDestination;
    if(command==='save_project_as'||command==='save_project'){disk=structuredClone(args.project);if(appendEvidenceOnSave){disk.energy.runs.push({id:'completed-after-edit'});disk.energy.weather.push({id:'weather-after-edit'});}return view(disk);}
    if(command==='dialog_open_project')return 'other-project';
    if(command==='admit_project')return{schema_version:'greenlens.project-admission.v1',project_id:'other-project-id',project_name:'Other project',manifest_sha256:'f'.repeat(64),static_runs:0,climate_runs:0,energy_runs:0,evidence_checked:false};
    if(command==='prepare_project_verification')return 1;
    if(command==='verify_project')return view({...fixture(),id:'other-project-id'});
    if(command==='cancel_project_verification')return;
    if(command==='dialog_open_reference_glb')return 'original-synthetic.glb';
    if(command==='reference_import'){if(referenceImportGate)await referenceImportGate;const next=structuredClone(args.project);next.architecture={schema_version:'greenlens.architectural-project.v1',assets:[{...referenceAsset,revision_sha256:referenceRevision}],active_asset_id:referenceAsset.id,scope:next.architecture?.scope??null};return view(next);}
    if(command==='reference_manifest'){if(referenceManifestGate)await referenceManifestGate;return{...referenceManifest(),revision_sha256:args.project.architecture.assets[0].revision_sha256};}
    if(command==='reference_chunk')return[{occurrence_index:0,material_index:null,vertices_m:[[0,0,0],[1,0,0],[0,1,0]]}];
    if(command==='reference_scope_context')return referenceContext(args.project);
    if(command==='reference_scope_assess')return referenceAssessment();
    if(command==='reference_scope_apply'){if(referenceScopeGate)await referenceScopeGate;const next=structuredClone(args.project);next.architecture.scope=args.scope;return next;}
    if(command==='open_project')return view({...fixture(),id:'other-project-id'});
    if(command==='energy_start')return{id:'running-test',state:'queued',log:[]};
    if(command==='cancel_and_wait')return null;
    if(command==='discard_job'||command==='prepare_close')return;
    throw Error(`Unexpected mocked IPC ${command}`);
  };
  root=createRoot(document.getElementById('root'));await act(async()=>{root.render(React.createElement(Harness));});
}
const button=(text,scope=document)=>[...scope.querySelectorAll('button')].find(b=>b.textContent===text);
async function click(text,scope=document){const b=button(text,scope);assert.ok(b,`button ${text}`);await act(async()=>b.dispatchEvent(new MouseEvent('click',{bubbles:true})));}
function input(label){const el=[...document.querySelectorAll('label')].find(l=>l.firstChild?.textContent===label);assert.ok(el,`field ${label}`);return el.querySelector('input,textarea,select');}
async function type(el,value){const proto=el.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:el.tagName==='SELECT'?HTMLSelectElement.prototype:HTMLInputElement.prototype;await act(async()=>{Object.getOwnPropertyDescriptor(proto,'value').set.call(el,value);el.dispatchEvent(new Event(el.tagName==='SELECT'?'change':'input',{bubbles:true}));});}
const dialog=()=>document.querySelector('[role="dialog"]');
const editName=async text=>type(input('Name'),text);
const count=command=>calls.filter(c=>c[0]===command).length;

test('real editor retains structured, incomplete numeric and invalid raw drafts through panel/design changes and local recovery',async()=>{
  await setup();const original=structuredClone(doc.project);
  await editName('Draft A');await type(input('Lighting W / m²'),'-');
  await click('Toggle panel');await click('Toggle panel');assert.equal(input('Name').value,'Draft A');assert.equal(input('Lighting W / m²').value,'-');
  await click('Switch design');await editName('Draft B');await click('Switch design');assert.equal(input('Name').value,'Draft A');
  await click('advanced');await type(document.querySelector('textarea'),'{ invalid exact text\n');
  await click('zone');await click('advanced');assert.equal(document.querySelector('textarea').value,'{ invalid exact text\n');
  await click('Toggle panel');await click('Toggle panel');assert.equal(document.querySelector('textarea').value,'{ invalid exact text\n');
  assert.deepEqual(doc.project,original);assert.equal(doc.history.past.length,0);
  await setup({retainCache:true});assert.match(document.body.textContent,/Recovered unsaved local draft/);assert.equal(document.querySelector('textarea').value,'{ invalid exact text\n');assert.deepEqual(doc.project,original);
});
test('Save, Save As, Run, Open and Close cannot ignore drafts; Cancel retains edits',async()=>{
  for(const action of ['Save','Save As','Run','Open','Close']){
    await setup();await editName('Unapplied');calls=[];
    await click(action);assert.ok(dialog());assert.deepEqual(calls.map(c=>c[0]),action==='Open'?['dialog_open_project','admit_project']:[]);
    await click('Cancel',dialog());assert.equal(dialog(),null);assert.equal(input('Name').value,'Unapplied');assert.deepEqual(calls.map(c=>c[0]),action==='Open'?['dialog_open_project','admit_project']:[]);assert.equal(count('verify_project'),0);
  }
});
test('Apply preserves rejected text/history, then validates A and B explicitly before Save',async()=>{
  await setup();await editName('Validated A');await click('Switch design');await editName('Validated B');await click('Switch design');
  rejectEdit=true;await click('Save');await click('Apply',dialog());assert.ok(dialog());assert.match(dialog().textContent,/Synthetic atomic restore rejection/);assert.equal(count('save_project_as'),0);assert.equal(doc.history.past.length,0);assert.equal(input('Name').value,'Validated A');
  rejectEdit=false;await click('Apply',dialog());assert.equal(dialog(),null);assert.equal(doc.history.past.length,1);assert.equal(doc.history.past[0].kind,'energy_batch');assert.equal(count('save_project_as'),1);assert.equal(disk.energy.designs.A.model.zones[0].name,'Validated A');assert.equal(disk.energy.designs.B.model.zones[0].name,'Validated B');assert.deepEqual(doc.energyDrafts.pending,[]);
});
test('guarded Apply parses every actual-editor draft before accepting a valid earlier design',async()=>{
  await setup();const accepted=structuredClone(doc.project);await editName('A must remain a draft');await click('Switch design');await type(input('Lighting W / m²'),'-');
  await click('Save');await click('Apply',dialog());assert.match(dialog().textContent,/finite numeric/);
  assert.deepEqual(doc.project,accepted);assert.equal(doc.history.past.length,0);assert.equal(doc.history.future.length,0);assert.equal(count('energy_restore_designs'),0);assert.equal(count('save_project_as'),0);
  await click('Cancel',dialog());assert.equal(input('Lighting W / m²').value,'-');await click('Switch design');assert.equal(input('Name').value,'A must remain a draft');assert.deepEqual(doc.energyDrafts.pending,['A','B']);
});
test('a later Rust issue rejection retains all four draft categories and both history stacks; Cancel does not partially accept',async()=>{
  await setup();await editName('Prior accepted edit');await click('Validate and apply');await click('Undo');
  const accepted=structuredClone(doc.project),past=structuredClone(doc.history.past),future=structuredClone(doc.history.future);
  await editName('Pending A');await click('advanced');const raw=JSON.stringify({...doc.energyDrafts.get('A').structured,model:{...doc.energyDrafts.get('A').structured.model,name:'Exact raw A'}} ,null,2)+'\n';await type(document.querySelector('textarea'),raw);
  await click('Switch design');await editName('Pending B');
  await act(()=>{doc.energyDrafts.updateReport(d=>({...d,service:'Pending service evidence',dirty:true}));doc.energyDrafts.updateIssue(d=>({...d,enabled:true,title:'Pending title',revision:'R2',issue_date:'2026-',dirty:true}));});
  const drafts=()=>({a:structuredClone(doc.energyDrafts.get('A')),b:structuredClone(doc.energyDrafts.get('B')),report:structuredClone(doc.energyDrafts.report()),issue:structuredClone(doc.energyDrafts.issue())}),retained=drafts();
  calls=[];await click('Save');await click('Apply',dialog());assert.match(dialog().textContent,/explicit report issue validation rejection/);assert.equal(count('energy_restore_designs'),1);assert.equal(count('energy_metadata'),2);
  assert.deepEqual(doc.project,accepted);assert.deepEqual(doc.history.past,past);assert.deepEqual(doc.history.future,future);assert.deepEqual(drafts(),retained);assert.equal(count('save_project_as'),0);
  await click('Cancel',dialog());assert.deepEqual(doc.project,accepted);assert.deepEqual(drafts(),retained);assert.deepEqual(doc.energyDrafts.pending,['A','B','report settings','report issue']);
});
test('all guarded draft categories commit as one undo/redo transaction and preserve later run evidence',async()=>{
  await setup();const original=structuredClone(doc.project.energy);await editName('Batch A');await click('Switch design');await editName('Batch B');
  await act(()=>{doc.energyDrafts.updateReport(d=>({...d,service:'Batch service text',dirty:true}));doc.energyDrafts.updateIssue(d=>({...d,enabled:true,title:'Batch report',revision:'R3',dirty:true}));});
  appendEvidenceOnSave=true;await click('Save');await click('Apply',dialog());assert.equal(dialog(),null);assert.equal(doc.history.past.length,1);assert.equal(doc.history.past[0].kind,'energy_batch');assert.deepEqual(doc.energyDrafts.pending,[]);
  const applied=structuredClone(doc.project.energy);assert.equal(applied.designs.A.model.zones[0].name,'Batch A');assert.equal(applied.designs.B.model.zones[0].name,'Batch B');assert.equal(applied.lighting_service_evidence,'Batch service text');assert.equal(applied.workbench.report_metadata.title,'Batch report');
  rejectEdit=true;await click('Undo');assert.deepEqual(doc.project.energy,applied);assert.equal(doc.history.past.length,1);assert.equal(doc.history.future.length,0);
  rejectEdit=false;await click('Undo');assert.deepEqual(doc.project.energy.designs,original.designs);assert.equal(doc.project.energy.lighting_service_evidence,original.lighting_service_evidence);assert.equal(doc.project.energy.workbench.report_metadata,null);assert.deepEqual(doc.project.energy.runs,applied.runs);assert.deepEqual(doc.project.energy.weather,applied.weather);
  await act(async()=>doc.traverse('redo'));assert.deepEqual(doc.project.energy,applied);assert.equal(doc.history.past.length,1);assert.equal(doc.history.future.length,0);
});
test('invalid numeric/raw blocks Apply; Discard saves accepted inputs, never the stale candidate',async()=>{
  await setup();const accepted=structuredClone(doc.project);await type(input('Lighting W / m²'),'1e');await click('Save');await click('Apply',dialog());assert.match(dialog().textContent,/finite numeric/);assert.equal(count('energy_edit'),0);assert.equal(doc.history.past.length,0);
  await click('Discard',dialog());assert.equal(count('save_project_as'),1);assert.deepEqual(disk.energy,accepted.energy);
  await click('advanced');await type(document.querySelector('textarea'),'{');await click('Run');await click('Apply',dialog());assert.ok(dialog());assert.equal(count('energy_start'),0);assert.equal(count('energy_edit'),0);await click('Cancel',dialog());assert.equal(document.querySelector('textarea').value,'{');
});
test('native close callback and Undo share the draft guard and cancelled undo preserves history',async()=>{
  await setup();await editName('First accepted');await click('Validate and apply');assert.equal(doc.history.past.length,1);
  await editName('Second draft');let prevented=false;await act(async()=>closeHandler({preventDefault(){prevented=true;}}));assert.equal(prevented,true);assert.ok(dialog());await click('Cancel',dialog());assert.equal(count('destroy'),0);
  await click('Undo');assert.ok(dialog());await click('Cancel',dialog());assert.equal(doc.history.past.length,1);assert.equal(input('Name').value,'Second draft');
  await click('Undo');await click('Discard',dialog());assert.equal(doc.history.past.length,0);assert.equal(input('Name').value,fixture().energy.designs.A.model.zones[0].name);
});
test('base conflicts, bad cache identities and cache quota failures preserve inputs without automatic acceptance',()=>{
  const design=fixture().energy.designs.A,draft=fieldEdit(cleanDraft(design),'lighting','-',()=>{});
  assert.throws(()=>draftCandidate(draft,design),/finite numeric/);
  const changed=structuredClone(design);changed.model.name+=' external';assert.throws(()=>draftCandidate(draft,changed),/Accepted inputs changed/);
  const store=new EnergyDraftStore({getItem:()=>null,setItem(){throw Error('quota');}});store.set('project','A',draft);assert.equal(store.get('project','A',design).fields.lighting,'-');assert.match(store.warning,/remain in this window/);
  const malformed=new EnergyDraftStore({getItem:()=>JSON.stringify({schema:'greenlens.thermal-drafts.v1',entries:[['not-json',draft]]}),setItem(){}});assert.equal(malformed.entries.size,0);assert.match(malformed.warning,/invalid/);
});
test('actual workspace tab/design and whole workspace unmount retain drafts; report settings guard Save',async()=>{
  await setup({workspace:true});await editName('Workspace A');
  await click('results');await click('model');assert.equal(input('Name').value,'Workspace A');
  await click('Proposed B');await editName('Workspace B');await click('Baseline A');assert.equal(input('Name').value,'Workspace A');
  await click('Toggle panel');await click('Toggle panel');assert.equal(input('Name').value,'Workspace A');
  await click('evidence');await click('Advanced carbon JSON');await type(input('Explicit carbon factor JSON, or null for missing'),'{ invalid');await type(input('Lighting-service equivalence evidence'),'Exact local service text');
  await click('model');await click('evidence');assert.equal(input('Explicit carbon factor JSON, or null for missing').value,'{ invalid');
  await click('Toggle panel');await click('Toggle panel');await click('evidence');assert.equal(input('Lighting-service equivalence evidence').value,'Exact local service text');
  await click('Save');await click('Apply',dialog());assert.ok(dialog());assert.equal(count('energy_metadata'),0);assert.equal(count('save_project_as'),0);await click('Cancel',dialog());
  await type(input('Explicit carbon factor JSON, or null for missing'),'null');rejectEdit=true;await click('Save');await click('Apply',dialog());assert.match(dialog().textContent,/Synthetic atomic restore rejection/);assert.equal(count('save_project_as'),0);
  rejectEdit=false;await click('Apply',dialog());assert.equal(dialog(),null);assert.equal(disk.energy.lighting_service_evidence,'Exact local service text');assert.deepEqual(doc.energyDrafts.pending,[]);
});

test('grouped A/B geometry has one undo entry; failed restore preserves both stacks and successful restore retains later evidence',async()=>{
  await setup();const original=structuredClone(doc.project.energy.designs);
  rejectEdit=true;await click('Group geometry');assert.deepEqual(doc.project.energy.designs,original);assert.equal(doc.history.past.length,0);
  rejectEdit=false;await click('Group geometry');assert.equal(doc.history.past.length,1);assert.equal(doc.project.energy.designs.A.model.name,'Grouped A');assert.equal(doc.project.energy.designs.B.model.name,'Grouped B');
  appendEvidenceOnSave=true;await click('Save');rejectEdit=true;await click('Undo');assert.equal(doc.history.past.length,1);assert.equal(doc.history.future.length,0);assert.equal(doc.project.energy.designs.B.model.name,'Grouped B');
  rejectEdit=false;await click('Undo');assert.deepEqual(doc.project.energy.designs,original);assert.equal(doc.history.past.length,0);assert.equal(doc.history.future.length,1);assert.deepEqual(doc.project.energy.runs,[{id:'completed-after-edit'}]);assert.deepEqual(doc.project.energy.weather,[{id:'weather-after-edit'}]);
  await act(async()=>{await doc.traverse('redo');});assert.equal(doc.project.energy.designs.A.model.name,'Grouped A');assert.equal(doc.project.energy.designs.B.model.name,'Grouped B');assert.deepEqual(doc.project.energy.runs,[{id:'completed-after-edit'}]);
});

test('an active geometry modal blocks document mutation and verification while allowing cheap Open admission',async()=>{
  await setup();await act(()=>doc.setEnergyAuthoringOpen(true));calls=[];
  for(const action of ['Save','Save As','Run','Open','Close','Undo'])await click(action);
  await act(async()=>closeHandler({preventDefault(){}}));assert.deepEqual(calls.map(c=>c[0]),['dialog_open_project','admit_project']);assert.equal(count('verify_project'),0);assert.match(doc.error,/geometry operation/);
  await act(()=>doc.setEnergyAuthoringOpen(false));await click('Save');assert.equal(count('save_project_as'),1);
});

test('real geometry dialog protects native close, retains rejected parameters and applies one reviewed explicit A/B transaction',async()=>{
  await setup({workspace:true});await click('Create / edit geometry…');assert.ok(document.querySelector('dialog[open]'));assert.equal(doc.energyAuthoringOpen,true);
  await type(input('New stable zone ID'),'retained_zone');await act(async()=>closeHandler({preventDefault(){}}));
  assert.match(document.querySelector('[role=alertdialog]').textContent,/Resolve this operation/);await click('Apply operation');assert.match(document.body.textContent,/Preview a valid operation/);assert.equal(count('energy_transaction_commit'),0);
  await click('Cancel — continue editing');assert.equal(input('New stable zone ID').value,'retained_zone');assert.equal(count('destroy'),0);
  await type(input('Explicit target'),'both');await click('Preview in Rust');assert.equal(count('energy_operation_plan'),1);
  rejectEdit=true;await click('Save');await click('Apply operation');assert.match(document.body.textContent,/transaction was not accepted/);assert.equal(doc.history.past.length,0);assert.equal(input('New stable zone ID').value,'retained_zone');
  rejectEdit=false;await click('Apply operation');assert.equal(document.querySelector('dialog'),null);assert.equal(doc.energyAuthoringOpen,false);assert.equal(doc.history.past.length,1);assert.equal(count('save_project_as'),1);assert.equal(disk.energy.designs.A.model.name,'Operation A');assert.equal(disk.energy.designs.B.model.name,'Operation B');
});
test('geometry dropdown retains parameters while unrelated hidden invalid fields do not enter an operation; Discard resumes Save',async()=>{
  await setup({workspace:true});await click('Create / edit geometry…');await type(input('Operation'),'set_zone_box');await type(input('Width X (m)'),'-');await type(input('Operation'),'remove_zone');await click('Preview in Rust');assert.equal(count('energy_operation_plan'),1);assert.equal(calls.find(c=>c[0]==='energy_operation_plan')[1].operations.A.operation,'remove_zone');
  await type(input('Operation'),'set_zone_box');assert.equal(input('Width X (m)').value,'-');assert.equal(button('Apply one transaction').disabled,true);
  await click('Save');await click('Discard operation');assert.equal(document.querySelector('dialog'),null);assert.equal(count('save_project_as'),1);assert.equal(count('energy_transaction_commit'),0);assert.equal(doc.history.past.length,0);
});
test('typed New form retains invalid and rejected parameters, guards close, and creates an unsaved project only on Rust acceptance',async()=>{
  await setup({workspace:true});await click('Save');await click('New');assert.match(document.body.textContent,/New thermal project/);assert.equal(doc.energyAuthoringOpen,true);
  await type(input('Width X (m)'),'-');await click('Create unsaved project');assert.equal(count('energy_new'),0);assert.match(document.body.textContent,/Width: enter a finite number/);
  await act(async()=>closeHandler({preventDefault(){}}));await click('Cancel — continue editing');assert.equal(input('Width X (m)').value,'-');assert.equal(count('destroy'),0);
  await type(input('Width X (m)'),'12.5');rejectEdit=true;await click('Create unsaved project');assert.equal(doc.project.id,'draft-fixture');assert.equal(input('Width X (m)').value,'12.5');assert.equal(doc.history.past.length,0);
  rejectEdit=false;await click('Create unsaved project');assert.equal(doc.project.id,'new-thermal-project');assert.equal(doc.path,'');assert.equal(doc.dirty,true);assert.equal(doc.energyNewOpen,false);assert.equal(doc.energyAuthoringOpen,false);assert.equal(doc.project.energy.designs.A.model.zones[0].size_m[0],12.5);assert.equal(count('energy_start'),0);assert.equal(count('energy_create_example'),0);
});

test('a later report-settings draft survives both successful and rejected in-flight Apply, and Save cannot skip it',async()=>{
  for(const fails of [false,true]){
    await setup();const accepted=structuredClone(doc.project);let finish;metadataGate=new Promise(resolve=>{finish=resolve;});
    await act(()=>doc.energyDrafts.updateReport(d=>({...d,service:'Submitted report text',dirty:true})));
    await click('Save');await click('Apply',dialog());assert.equal(count('energy_metadata'),1);
    await act(()=>doc.energyDrafts.updateReport(d=>({...d,service:'Newer exact report text',dirty:true})));
    rejectEdit=fails;await act(async()=>finish());assert.equal(doc.energyDrafts.report().service,'Newer exact report text');assert.equal(doc.energyDrafts.report().dirty,true);assert.ok(dialog());assert.equal(count('save_project_as'),0);
    assert.match(dialog().textContent,fails?/Synthetic report settings rejection/:/newer draft/);assert.deepEqual(doc.project,accepted);assert.equal(doc.history.past.length,0);await click('Cancel',dialog());
  }
});

test('live structured arrays can grow and shrink without recovery normalization erasing the edit',()=>{
  const storage={getItem:()=>null,setItem(){}};const store=new EnergyDraftStore(storage),design=fixture().energy.designs.A;
  let entry=store.get('arrays','A',design);entry=structuredEdit(entry,d=>{d.model.constructions[0].layers.push(structuredClone(d.model.constructions[0].layers[0]));});store.set('arrays','A',entry);
  assert.equal(store.get('arrays','A',design).rawDirty,false);assert.equal(store.get('arrays','A',design).structured.model.constructions[0].layers.length,design.model.constructions[0].layers.length+1);
  store.set('arrays','A',structuredEdit(store.get('arrays','A',design),d=>{d.model.constructions[0].layers.pop();d.model.constructions[0].layers.pop();}));
  assert.equal(store.get('arrays','A',design).rawDirty,false);assert.equal(store.get('arrays','A',design).structured.model.constructions[0].layers.length,design.model.constructions[0].layers.length-1);
});

test('real carbon fields retain incomplete numeric text, recover locally, and commit only the explicit sourced factor',async()=>{
  await setup({workspace:true});const physics=structuredClone(doc.project.energy.designs);await click('evidence');
  const toggle=[...document.querySelectorAll('label')].find(e=>e.textContent==='Use an explicit electricity carbon factor').querySelector('input');await act(()=>toggle.click());
  for(const [label,value] of [['Factor ID','declared-factor'],['Region','Example region'],['Factor year','2026'],['Factor kg / kWh','1e'],['Factor source','Synthetic test source'],['Factor version','test-v1']])await type(input(label),value);
  await click('Validate report settings');assert.equal(count('energy_metadata'),0);assert.match(document.body.textContent,/finite nonnegative/);assert.equal(input('Factor kg / kWh').value,'1e');
  await setup({retainCache:true,workspace:true});await click('evidence');assert.equal(input('Factor kg / kWh').value,'1e');assert.equal(input('Factor source').value,'Synthetic test source');await type(input('Factor kg / kWh'),'0.4');
  await act(()=>[...document.querySelectorAll('label')].find(e=>e.textContent.startsWith('Synthetic factor')).querySelector('input').click());
  await click('Validate report settings');assert.equal(doc.project.energy.carbon_factor.kg_per_kwh,.4);assert.equal(doc.project.energy.carbon_factor.synthetic,true);assert.equal(doc.project.energy.carbon_factor.fuel,'electricity');assert.deepEqual(doc.project.energy.designs,physics);assert.deepEqual(doc.energyDrafts.pending,[]);
});
test('real report issue fields preserve rejected dates/history, guard report/export and undo first metadata back to absence',async()=>{
  await setup({workspace:true});const physics=structuredClone(doc.project.energy.designs);await click('evidence');
  await act(()=>[...document.querySelectorAll('label')].find(e=>e.textContent==='Use explicit report issue information').querySelector('input').click());
  await type(input('Report title'),'Owned <report>');await type(input('Report revision'),'R1');await type(input('Issue date'),'2026-');await click('Add revision history');await type(document.querySelector('[aria-label="History 1 revision"]'),'R0');await type(document.querySelector('[aria-label="History 1 issue date"]'),'2026-10-01');await type(document.querySelector('[aria-label="History 1 description"]'),'Original retained issue');
  await click('Validate issue information');assert.equal(doc.history.past.length,0);assert.equal(input('Issue date').value,'2026-');assert.match(document.body.textContent,/issue validation rejection/);
  await click('model');await click('evidence');assert.equal(document.querySelector('[aria-label="History 1 description"]').value,'Original retained issue');
  let continued=false;await act(()=>{void doc.energyDrafts.beforeAction('exporting reports').then(ok=>{continued=ok;});});assert.ok(dialog());await click('Cancel',dialog());assert.equal(continued,false);
  await setup({retainCache:true,workspace:true});await click('evidence');assert.equal(input('Report title').value,'Owned <report>');assert.equal(input('Issue date').value,'2026-');await type(input('Issue date'),'');await click('Validate issue information');assert.equal(doc.history.past.length,1);assert.equal(doc.project.energy.workbench.report_metadata.author,null);assert.equal(doc.project.energy.workbench.report_metadata.issue_date,null);assert.equal(doc.project.energy.workbench.report_metadata.revision_history.length,1);
  await click('Undo');assert.equal(doc.project.energy.workbench.report_metadata,null);await act(async()=>{await doc.traverse('redo');});assert.equal(doc.project.energy.workbench.report_metadata.title,'Owned <report>');assert.deepEqual(doc.project.energy.designs,physics);assert.deepEqual(doc.project.energy.runs,[]);
});
test('later issue information survives in-flight Apply and cannot be silently cleared before Save',async()=>{
  for(const fails of [false,true]){await setup();let finish;metadataGate=new Promise(resolve=>{finish=resolve;});await act(()=>doc.energyDrafts.updateIssue(d=>({...d,enabled:true,title:'Submitted title',revision:'R1',dirty:true})));await click('Save');await click('Apply',dialog());await act(()=>doc.energyDrafts.updateIssue(d=>({...d,title:'Newer exact title',dirty:true})));rejectEdit=fails;await act(async()=>finish());assert.equal(doc.energyDrafts.issue().title,'Newer exact title');assert.ok(doc.energyDrafts.issue().dirty);assert.equal(count('save_project_as'),0);assert.ok(dialog());await click('Cancel',dialog());}
});
test('newer design, settings and issue drafts stop an in-flight batch before any accepted change',async()=>{
  await setup();const accepted=structuredClone(doc.project);await editName('Submitted A');await click('Switch design');await editName('Submitted B');
  await act(()=>{doc.energyDrafts.updateReport(d=>({...d,service:'Submitted service',dirty:true}));doc.energyDrafts.updateIssue(d=>({...d,enabled:true,title:'Submitted title',revision:'R1',dirty:true}));});
  let finish;metadataGate=new Promise(resolve=>{finish=resolve;});await click('Save');await click('Apply',dialog());assert.equal(count('energy_restore_designs'),1);assert.equal(count('energy_metadata'),1);
  await act(()=>{for(const id of ['A','B'])doc.energyDrafts.update(id,d=>structuredEdit(d,next=>{next.model.name=`Newer ${id}`;}));doc.energyDrafts.updateReport(d=>({...d,service:'Newer exact service',dirty:true}));doc.energyDrafts.updateIssue(d=>({...d,title:'Newer exact title',dirty:true}));});
  await act(async()=>finish());assert.match(dialog().textContent,/newer draft/);assert.deepEqual(doc.project,accepted);assert.equal(doc.history.past.length,0);assert.equal(count('save_project_as'),0);
  assert.equal(doc.energyDrafts.get('A').structured.model.name,'Newer A');assert.equal(doc.energyDrafts.get('B').structured.model.name,'Newer B');assert.equal(doc.energyDrafts.report().service,'Newer exact service');assert.equal(doc.energyDrafts.issue().title,'Newer exact title');assert.deepEqual(doc.energyDrafts.pending,['A','B','report settings','report issue']);await click('Cancel',dialog());
});

test('actual object tree and preflight links route exact zone/window/fan fields without accepting or clearing drafts',async()=>{
  await setup({workspace:true,objects:true});const accepted=structuredClone(doc.project),zone=doc.project.energy.designs.A.model.zones[2],window=zone.windows[0];
  const treeitem=[...document.querySelectorAll('[role="treeitem"]')].find(el=>el.title.startsWith(`${window.id} · window`));assert.ok(treeitem);await act(()=>treeitem.click());
  assert.equal(doc.thermalPresentation.value.selected,window.id);assert.equal(doc.energyDrafts.get('A').section,'envelope');assert.equal(document.activeElement.dataset.thermalId,window.id);assert.match(document.querySelector('[aria-label="Inspector target"]').textContent,new RegExp(window.id));
  const marker=document.querySelector(`[data-thermal-kind="window"][data-thermal-id="${window.id}"]`);await type(marker.querySelector('[data-thermal-field="/width_m"]'),'-');
  const issue=(kind,id,field_path,zone_id=null)=>({code:'test.routing',kind:'invalid_input',severity:'error',object:{kind,id,zone_id},field_path,message:'Synthetic field issue',remedy:'Inspect this field.'});
  preflightIssues=[issue('supply_fan',zone.cooling_system.id,'/total_efficiency',zone.id),issue('window',window.id,'/width_m',zone.id)];
  await click('preflight');await click(`${zone.cooling_system.id} · /total_efficiency`);assert.equal(doc.energyDrafts.get('A').section,'systems');assert.equal(document.activeElement.dataset.thermalField,'/total_efficiency');assert.equal(document.activeElement.closest('[data-thermal-kind]').dataset.thermalKind,'supply_fan');assert.equal(document.activeElement.closest('[data-thermal-id]').dataset.thermalId,zone.cooling_system.id);
  await click('preflight');await click(`${window.id} · /width_m`);assert.equal(document.activeElement.value,'-');assert.equal(document.activeElement.dataset.thermalField,'/width_m');assert.equal(document.activeElement.closest('[data-thermal-kind="window"]').dataset.thermalId,window.id);assert.match(document.querySelector('[aria-label="Inspector target"]').textContent,/Field: \/width_m/);
  assert.deepEqual(doc.project,accepted);assert.equal(doc.history.past.length,0);assert.equal(count('energy_edit'),0);assert.equal(count('energy_start'),0);
});

test('actual preflight resource and schedule routing opens the exact entry and never substitutes a mismatched zone',async()=>{
  await setup({workspace:true,objects:true});const model=doc.project.energy.designs.A.model,accepted=structuredClone(doc.project),material=model.materials[1],curve=model.curves[1],schedule=model.schedules[1],zone=model.zones[1];
  const issue=(kind,id,field_path,zone_id=null)=>({code:'test.routing',kind:'invalid_input',severity:'error',object:{kind,id,zone_id},field_path,message:'Synthetic field issue',remedy:'Inspect exact target.'});
  preflightIssues=[issue('material',material.id,'/conductivity_w_mk'),issue('curve',curve.id,'/form/coefficients/0'),issue('schedule',schedule.id,'/saturday/values/2'),issue('window',zone.windows[0].id,'/width_m',model.zones[0].id)];
  for(const item of preflightIssues.slice(0,3)){
    await click('preflight');await click(`${item.object.id} · ${item.field_path}`);assert.equal(document.activeElement.dataset.thermalField,item.field_path);const object=document.activeElement.closest('[data-thermal-kind]');assert.equal(object.dataset.thermalId,item.object.id);assert.equal(object.dataset.thermalKind,item.object.kind);if(object.tagName==='DETAILS')assert.equal(object.open,true);assert.match(document.querySelector('[aria-label="Inspector target"]').textContent,new RegExp(item.object.id));
  }
  await click('preflight');await click(`${zone.windows[0].id} · /width_m`);assert.match(document.querySelector('[aria-label="Inspector target"]').textContent,/exact target is absent/);assert.notEqual(document.activeElement.dataset.thermalField,'/width_m');assert.deepEqual(doc.project,accepted);assert.equal(doc.energyDrafts.get('A').dirty,false);assert.equal(doc.history.past.length,0);
});

test('scene overhang aliases preserve the original selection identity and DX removal keeps structured add controls reachable',async()=>{
  await setup({workspace:true,objects:true});await click('Proposed B');const accepted=structuredClone(doc.project),zone=doc.project.energy.designs.B.model.zones.find(z=>z.windows.some(w=>w.overhang)),window=zone.windows.find(w=>w.overhang);
  const id=`overhang:${window.id}`,item=[...document.querySelectorAll('[role="treeitem"]')].find(el=>el.title.startsWith(`${id} · overhang`));assert.ok(item);await act(()=>item.click());assert.equal(doc.thermalPresentation.value.selected,id);assert.match(document.querySelector('[aria-label="Inspector target"]').textContent,new RegExp(id));assert.equal(document.activeElement.dataset.thermalKind,'overhang');assert.equal(document.activeElement.dataset.thermalId,window.id);
  await click('Baseline A');await click('Proposed B');assert.match(document.querySelector('[aria-label="Inspector target"]').textContent,new RegExp(id));await click('systems');await click('Remove DX and make zone unconditioned');assert.ok(button('Add explicit synthetic DX system'));await click('Add explicit synthetic DX system');assert.ok(button('Remove DX and make zone unconditioned'));assert.equal(doc.energyDrafts.get('B').dirty,true);assert.equal(doc.history.past.length,0);assert.deepEqual(doc.project,accepted);assert.equal(count('energy_start'),0);
});

test('actual workspace has one searchable object hierarchy with zone properties and a reachable thermal lock',async()=>{
  await setup({workspace:true,objects:true});const accepted=structuredClone(doc.project),zone=doc.project.energy.designs.A.model.zones[2];
  assert.equal(document.querySelectorAll('[role="tree"][aria-label="Thermal objects"]').length,1);assert.equal(document.querySelector('.energy-zones'),null);
  const item=[...document.querySelectorAll('[role="treeitem"]')].find(el=>el.title.startsWith(`${zone.id} · zone`));await act(()=>item.click());
  assert.equal(doc.thermalPresentation.value.selected,zone.id);assert.equal(input('Name').value,zone.name);assert.equal(doc.energyDrafts.get('A').section,'zone');
  await click('Lock thermal inputs');assert.equal(doc.energyLocked,true);assert.equal(input('Name').matches(':disabled'),true);assert.equal(button('Create / edit geometry…').disabled,true);
  await click('Unlock thermal inputs');assert.equal(input('Name').matches(':disabled'),false);assert.deepEqual(doc.project,accepted);assert.equal(doc.history.past.length,0);
});

test('actual workspace readiness and job panels remain compact, keyboard dismissible and do not mutate scientific inputs',async()=>{
  await setup({workspace:true,objects:true});const accepted=structuredClone(doc.project),workspace=document.querySelector('[aria-label="Building energy workspace"]');
  const panel=document.querySelector('.energy-utility-panel');assert.equal(panel.hidden,true);assert.equal(count('energy_dependencies'),0);
  await click('Readiness & settings');assert.equal(panel.hidden,false);assert.equal(document.querySelector('#energy-local-settings').hidden,false);assert.equal(document.querySelector('#energy-current-job').hidden,true);
  assert.equal(document.activeElement.getAttribute('aria-label'),'Close readiness or job panel');
  await type(input('EnergyPlus directory'),'C:\\local tooling\\EnergyPlus');await click('Check local installations');assert.equal(count('energy_dependencies'),1);
  assert.match(document.querySelector('#energy-local-settings').textContent,/EnergyPlus: pinned installation ready/);
  await act(()=>input('EnergyPlus directory').dispatchEvent(new dom.window.KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true})));assert.equal(panel.hidden,true);assert.equal(document.activeElement,button('Readiness & settings',workspace));
  await click('results',workspace);await click('Readiness & settings');assert.equal(input('EnergyPlus directory').value,'C:\\local tooling\\EnergyPlus');assert.match(document.querySelector('#energy-local-settings').textContent,/EnergyPlus: pinned installation ready/);assert.equal(count('energy_dependencies'),1);
  await click('Close panel');await click('Current job');assert.equal(document.querySelector('#energy-local-settings').hidden,true);assert.equal(document.querySelector('#energy-current-job').hidden,false);assert.match(document.querySelector('[aria-label="Current job log"]').textContent,/No job/);
  await click('Close panel');assert.equal(document.activeElement,button('Current job',workspace));assert.deepEqual(doc.project,accepted);assert.equal(doc.history.past.length,0);assert.equal(count('energy_start'),0);
});

async function prepareReferenceImport(){
  const workspace=document.querySelector('[aria-label="Building energy workspace"]');await click('reference',workspace);await click('Prepare local GLB import');
  for(const label of ['I reviewed the explicit scale; source dimension checks must be recorded before relying on a mapping.','I reviewed the axis, origin and orientation choices; unknown true north stays unknown.']){
    const control=[...document.querySelectorAll('label')].find(el=>el.textContent.trim()===label)?.querySelector('input');assert.ok(control,label);await act(async()=>control.dispatchEvent(new MouseEvent('click',{bubbles:true})));
  }
  return workspace;
}

test('actual workspace navigation consumes a successful import draft after its panel unmounts',async()=>{
  await setup({workspace:true});const workspace=await prepareReferenceImport();let finish;referenceImportGate=new Promise(resolve=>finish=resolve);
  await click('Choose local GLB and import');assert.equal(count('reference_import'),1);assert.equal(doc.busy,true);const acceptedBefore=structuredClone(doc.project.energy);
  await click('results',workspace);assert.equal(document.querySelector('[aria-label="Architectural reference and study scope"]'),null);
  await act(async()=>finish());assert.equal(doc.project.architecture.active_asset_id,referenceAsset.id);assert.equal(doc.referenceDrafts.draft.import,null);assert.deepEqual(doc.project.energy,acceptedBefore);
  assert.equal(JSON.parse(window.localStorage.getItem(`greenlens.reference-draft.v1:${doc.project.id}`)).import,null);
  await click('reference',workspace);assert.ok(button('Prepare another reference revision'));assert.equal(button('Choose local GLB and import'),undefined);
  const saved=count('save_project');await click('Save');assert.equal(doc.referenceDrafts.guard,null);assert.equal(count('save_project'),saved+1);assert.equal(count('reference_import'),1);
});

test('actual workspace scope-save completion consumes only its reviewed category after navigation',async()=>{
  await setup({workspace:true});const workspace=await prepareReferenceImport();await click('Choose local GLB and import');await click('Start scope for current revision');
  await type(input('Scope title'),'Original navigation fixture');await type(input('Study purpose'),'Verify draft ownership through actual panel navigation');
  await type(input('Included scope (one item per line)'),'Explicit synthetic portion');await type(input('Boundary-condition consequences'),'Only declared analytical boundaries');await click('Review scope in Rust');
  const accepted=structuredClone(doc.referenceDrafts.draft.scope.value);let finish;referenceScopeGate=new Promise(resolve=>finish=resolve);await click('Save accepted scope');
  assert.equal(count('reference_scope_apply'),1);await click('results',workspace);assert.equal(document.querySelector('[aria-label="Architectural reference and study scope"]'),null);
  await act(async()=>doc.referenceDrafts.update({...doc.referenceDrafts.draft,import:{scale:'-',rotation:'0',x:'0',y:'0',z:'0',scene:'',north:'',application:'',version:'',options:'',note:'Independent later intake draft',dimensions_checked:false,orientation_checked:false}}));
  await act(async()=>finish());assert.deepEqual(doc.project.architecture.scope,accepted);assert.equal(doc.referenceDrafts.draft.scope,null);assert.equal(doc.referenceDrafts.draft.import.note,'Independent later intake draft');assert.equal(doc.history.past.length,1);
  await click('reference',workspace);assert.ok(button('Edit accepted scope'));assert.equal(input('Metres per source unit').value,'-');assert.equal(doc.referenceDrafts.draft.scope,null);
});

test('actual report guard completion survives tab and discipline remounts with its post-Apply binding, but does not follow another document',async()=>{
  await setup({workspace:true,reports:true});await click('Save');const originalDocumentId=doc.documentId;
  await click('evidence');await act(()=>[...document.querySelectorAll('label')].find(e=>e.textContent==='Use explicit report issue information').querySelector('input').click());
  await type(input('Report title'),'Navigation fixture');await type(input('Report revision'),'R2');
  let applyFinish,renderFinish;metadataGate=new Promise(resolve=>applyFinish=resolve);reportGate=new Promise(resolve=>renderFinish=resolve);
  await click('reports');await click('Generate reports…');assert.ok(dialog());assert.equal(count('energy_report'),0);await click('Apply',dialog());
  await click('results');assert.equal(document.querySelector('[aria-label="Report workbench"]'),null);await act(async()=>applyFinish());
  assert.equal(count('energy_report'),1);const request=calls.find(([c])=>c==='energy_report')[1];assert.equal(request.project.energy.workbench.report_metadata.revision,'R2');assert.equal(doc.latestReport,null);
  await act(async()=>renderFinish());assert.equal(doc.latestReport.request_binding.report_revision,'R2');assert.equal(doc.latestReport.request_binding.baseline_run_id,'report-a');assert.equal(doc.latestReport.dataset_sha256,'d'.repeat(64));
  await click('reports');let completed=document.querySelector('[aria-label="Completed report output"]');assert.match(completed.textContent,/matches current request/);assert.match(completed.textContent,/report revision R2/);
  await click('Open executive.html');assert.deepEqual(calls.find(([c])=>c==='energy_open_report')[1],{destination:'synthetic-report-output',path:'executive.html'});
  await click('Toggle panel');await click('Toggle panel');await click('reports');assert.match(document.querySelector('[aria-label="Completed report output"]').textContent,/matches current request/);assert.equal(count('energy_report'),1);
  // A new destination keeps the completed link and its original source binding.
  saveDestination='copied-report-project';await click('Save As');assert.equal(doc.documentId,originalDocumentId);assert.equal(doc.path,saveDestination);
  completed=document.querySelector('[aria-label="Completed report output"]');assert.match(completed.textContent,/historical request/);assert.match(completed.textContent,/test-project/);assert.equal(doc.latestReport.request_binding.project_path,'test-project');
  // An unsuccessful attempt cannot overwrite the retained successful receipt.
  reportError=Error('Original synthetic renderer failure');const earlier=doc.latestReport;await click('Generate reports…');assert.equal(doc.latestReport,earlier);assert.match(doc.error,/synthetic renderer failure/);assert.match(document.querySelector('[aria-label="Completed report output"]').textContent,/historical request/);
  await click('Open');assert.notEqual(doc.documentId,originalDocumentId);assert.equal(doc.project.id,'other-project-id');assert.equal(doc.latestReport,null);assert.equal(document.querySelector('[aria-label="Completed report output"]'),null);
});

test('actual Energy workspace restores panel and exact inspector across discipline remounts and isolates designs without accepting drafts',async()=>{
  await setup({workspace:true,objects:true});const accepted=structuredClone(doc.project),zone=accepted.energy.designs.A.model.zones[2],window=zone.windows[0];
  await act(()=>[...document.querySelectorAll('[role="treeitem"]')].find(el=>el.title.startsWith(`${window.id} · window`)).click());
  const width=()=>document.querySelector(`[data-thermal-kind="window"][data-thermal-id="${window.id}"] [data-thermal-field="/width_m"]`);
  await type(width(),'-');await click('reports');await click('Daylight');assert.equal(document.querySelector('[aria-label="Building energy workspace"]'),null);
  await click('Energy');assert.equal(button('reports').getAttribute('aria-pressed'),'true');assert.ok(document.querySelector('[aria-label="Report workbench"]'));
  await click('model');assert.match(document.querySelector('[aria-label="Inspector target"]').textContent,new RegExp(window.id));assert.equal(width().value,'-');assert.equal(doc.thermalPresentation.value.selected,window.id);
  await click('Proposed B');assert.equal(button('model').getAttribute('aria-pressed'),'true');assert.equal(doc.thermalPresentation.value.workspace?.inspector??null,null);await click('weather');
  await click('Baseline A');assert.equal(button('model').getAttribute('aria-pressed'),'true');assert.match(document.querySelector('[aria-label="Inspector target"]').textContent,new RegExp(window.id));assert.equal(width().value,'-');
  await click('Proposed B');assert.equal(button('weather').getAttribute('aria-pressed'),'true');await click('Daylight');await click('Energy');assert.equal(button('weather').getAttribute('aria-pressed'),'true');
  assert.deepEqual(doc.project,accepted);assert.equal(doc.history.past.length,0);assert.equal(count('energy_edit'),0);assert.equal(count('energy_start'),0);
});

test('actual reference selection waits for admitted geometry after discipline remount and resets for missing IDs, changed revision and another document',async()=>{
  await setup({workspace:true,objects:true});await prepareReferenceImport();await click('Choose local GLB and import');const energy=structuredClone(doc.project.energy);
  const selected=()=>[...document.querySelectorAll('[role="treeitem"][aria-selected="true"]')].find(el=>el.title.startsWith('synthetic-occurrence '));
  const selectReference=async()=>{await click('Reference');await act(()=>[...document.querySelectorAll('[role="treeitem"]')].find(el=>el.title.startsWith('synthetic-occurrence ')).click());};
  await selectReference();assert.ok(selected());const retained=structuredClone(doc.thermalPresentation.value.workspace.reference_selection);assert.equal(doc.thermalPresentation.value.selected,null);
  await click('Daylight');let resume;referenceManifestGate=new Promise(resolve=>resume=resolve);await click('Energy');
  assert.equal(button('reference').getAttribute('aria-pressed'),'true');assert.equal(selected(),undefined);assert.deepEqual(doc.thermalPresentation.value.workspace.reference_selection,retained);
  await act(async()=>{referenceManifestGate=null;resume();});assert.ok(selected());assert.deepEqual(doc.thermalPresentation.value.workspace.reference_selection,retained);
  await click('Proposed B');assert.equal(button('model').getAttribute('aria-pressed'),'true');assert.equal(doc.thermalPresentation.value.workspace?.reference_selection??null,null);
  await click('Baseline A');assert.equal(button('reference').getAttribute('aria-pressed'),'true');assert.ok(selected());
  await act(()=>doc.thermalPresentation.update(p=>({...p,workspace:{...p.workspace,reference_selection:{...retained,occurrence_id:'no-such-retained-occurrence'}}})));assert.equal(doc.thermalPresentation.value.workspace.reference_selection,null);assert.equal(selected(),undefined);
  await selectReference();referenceRevision=referenceHash('9');await act(async()=>doc.importReference(referenceManifest().options));
  assert.equal(doc.project.architecture.assets[0].revision_sha256,referenceRevision);assert.equal(doc.thermalPresentation.value.workspace.reference_selection,null);assert.equal(selected(),undefined);assert.deepEqual(doc.project.energy,energy);
  await selectReference();await click('Open');assert.equal(doc.project.id,'other-project-id');assert.equal(button('model').getAttribute('aria-pressed'),'true');assert.equal(doc.thermalPresentation.value.workspace?.reference_selection??null,null);assert.equal(doc.thermalPresentation.value.workspace?.inspector??null,null);
  assert.equal(count('energy_edit'),0);assert.equal(count('energy_start'),0);assert.equal(doc.history.past.length,0);
});

test('retained absent inspector is cleared on discipline entry after its explicit field-link warning',async()=>{
  await setup({workspace:true,objects:true});const accepted=structuredClone(doc.project),zone=accepted.energy.designs.A.model.zones[0];
  preflightIssues=[{code:'test.absent-retained-target',kind:'invalid_input',severity:'error',object:{kind:'window',id:'removed-window',zone_id:zone.id},field_path:'/width_m',message:'Synthetic absent target',remedy:'Inspect the exact object.'}];
  await click('preflight');await click('removed-window · /width_m');assert.match(document.querySelector('[aria-label="Inspector target"]').textContent,/exact target is absent/);
  await click('Daylight');await click('Energy');assert.equal(doc.thermalPresentation.value.workspace.inspector,null);assert.equal(doc.thermalPresentation.value.selected,null);assert.equal(document.querySelector('[aria-label="Inspector target"]'),null);assert.deepEqual(doc.project,accepted);assert.equal(doc.history.past.length,0);
});

test('additive bounded workspace presentation preserves legacy entries and rejects malformed persisted navigation',()=>{
  const old=defaultThermalPresentation();assert.equal(validThermalPresentation(old),true);let text;
  const store=new ThermalPresentationStore({getItem:()=>null,setItem:(key,value)=>{text=value;}}),key='["display-fixture","A"]';
  const workspace={panel:'reference',inspector:{object:{kind:'window',id:'window-3',zone_id:'zone-3'},field_path:'/width_m'},reference_selection:{revision_sha256:referenceHash('2'),occurrence_id:'synthetic-occurrence'}};
  store.set(key,{...old,workspace});const restored=new ThermalPresentationStore({getItem:()=>text,setItem(){}});assert.deepEqual(restored.get(key).workspace,workspace);assert.equal(restored.get('["display-fixture","B"]').workspace,undefined);assert.equal(restored.get('["other","A"]').workspace,undefined);
  const legacy=new ThermalPresentationStore({getItem:()=>JSON.stringify([[key,old]]),setItem(){}});assert.equal(legacy.warning,'');assert.deepEqual(legacy.get(key),old);
  for(const invalid of [{...workspace,panel:'execute-shell'},{...workspace,inspector:{...workspace.inspector,field_path:'x'.repeat(1025)}},{...workspace,inspector:{object:{kind:'window',id:[],zone_id:null},field_path:''}},{...workspace,reference_selection:{...workspace.reference_selection,revision_sha256:'not-a-hash'}},{...workspace,reference_selection:{...workspace.reference_selection,occurrence_id:'x'.repeat(257)}}])assert.equal(validThermalPresentation({...old,workspace:invalid}),false);
});

test.after(async()=>{if(root)await act(()=>root.unmount());dom.window.close();});
