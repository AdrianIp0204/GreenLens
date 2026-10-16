import {issueFixture,reviewFixture} from './report-issue-fixture.mjs';
import {inspectionCore} from './inspection-ipc.mjs';
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
      if(spec.endsWith('?url'))target='data:text/javascript,'+encodeURIComponent(`export default ${JSON.stringify(import.meta.resolve(spec.slice(0,-4)))};`);
      else if(spec==='@tauri-apps/api/core')target=inspectionCore('__draftIPC','export const isTauri=()=>true;');
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
const {default:ReviewGuard}=await import(await load(path.join(src,'reference/ScopeReviewDraftGuard.tsx')));
const {default:ReferenceGuard}=await import(await load(path.join(src,'reference/ReferenceDraftGuard.tsx')));
const {useDocument}=await import(await load(path.join(src,'workspace/useDocument.ts')));
const {default:Workspace}=await import(await load(path.join(src,'energy/EnergyWorkspace.tsx')));
const {EnergyDraftStore,fieldEdit,structuredEdit,cleanDraft,draftCandidate}=await import(await load(path.join(src,'energy/drafts.ts')));
const {ThermalPresentationStore,defaultThermalPresentation,validThermalPresentation}=await import(await load(path.join(src,'energy/thermal-presentation.ts')));
const inputs=JSON.parse(fs.readFileSync(new URL('../../../examples/energy/revision1-inputs.json',import.meta.url)));
const daylight=JSON.parse(fs.readFileSync(new URL('./viewport-fixture.json',import.meta.url)));
let doc,root,calls,rejectEdit=false,closeHandler,disk,fullWorkspace=false,appendEvidenceOnSave=false,metadataGate=null,sceneObjects=false,preflightIssues=[],referenceImportGate=null,referenceScopeGate=null,reportRuns=false,reportGate=null,reportError=null,saveDestination='test-project';
let issueEvidence=new Map(),issueGate=null,issueInspectGate=null,issueProductGate=null,issueError=null,issuePending=false,pendingFixture=null;
let referenceManifestGate=null,referenceRevision='2'.repeat(64),revisionOccurrenceId='synthetic-occurrence',withRevisions=false,revisionPrepareGate=null,revisionAcceptGate=null,revisionPlanMode='cop',revisionReadinessCase=false;
dom.window.HTMLDialogElement.prototype.showModal=function(){this.open=true;};
// Real ThermalView follows its WebGL-unavailable branch; no viewport acceptance claim.
dom.window.HTMLCanvasElement.prototype.getContext=()=>{throw Error('No WebGL context in DOM test');};
function fixture(){return {...(withRevisions?{architecture:revisionArchitecture()}:{}),schema_version:'0.4.0',id:'draft-fixture',name:'Draft fixture',alternatives:['A','B'].map(id=>({id,model:structuredClone(daylight.model)})),runs:[],energy:{designs:Object.fromEntries(['A','B'].map(id=>[id,{model:structuredClone(inputs.cases[id].model),study:structuredClone(inputs.cases[id].study)}])),weather:[],runs:reportRuns?[{id:'report-a',alternative_id:'A',record_sha256:'a'.repeat(64)},{id:'report-b',alternative_id:'B',record_sha256:'b'.repeat(64)},{id:'historical-a',alternative_id:'A',record_sha256:'c'.repeat(64)},{id:'historical-b',alternative_id:'B',record_sha256:'d'.repeat(64)}]:[],carbon_factor:null,lighting_service_evidence:null}};}
function view(p){return{project:structuredClone(p),revision:'test-revision',results:[],climate_results:[],energy_results:[]};}
const referenceHash=value=>value.repeat(64);
const referenceAsset={id:'synthetic-reference',path:'references/synthetic/manifest.json',revision_sha256:referenceHash('2'),manifest_sha256:referenceHash('4'),geometry_sha256:referenceHash('3')};
function referenceManifest(){return{schema_version:'greenlens.reference-model.v1',importer_id:'greenlens.glb-static-triangles/1',source_sha256:referenceHash('1'),revision_sha256:referenceHash('2'),geometry_sha256:referenceHash('3'),options:{scene_index:null,metres_per_source_unit:1,clockwise_rotation_deg:0,translation_m:[0,0,0],declared_north_clockwise_deg:null,conversion:null},source_units:'metres',coordinate_convention:'Right-handed X/Y/Z-up',selected_scene:0,source_node_count:1,source_mesh_count:1,source_material_count:0,source_texture_count:0,occurrences:[{id:revisionOccurrenceId,node_index:0,parent_id:null,label:'Original fixture',node_path:[0],source_mesh_index:0,world_from_mesh:[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1],mirrored:false,bounds:{min_m:[0,0,0],max_m:[1,1,0]},triangle_count:1,triangle_area_m2:.5}],materials:[],chunks:[{path:'geometry/00000.bin',sha256:referenceHash('5'),byte_length:88,triangle_count:1}],bounds:{min_m:[0,0,0],max_m:[1,1,0]},triangle_count:1,triangle_area_m2:.5,warnings:[],exclusions:[],storage_policy:'Original synthetic IPC fixture only',resources:{elapsed_ms:1,source_bytes:100,normalized_bytes:88,json_bytes:100,decoded_allocation_estimate_bytes:200}};}
function referenceContext(project){return{reference_options_sha256:referenceHash('6'),designs:Object.fromEntries(Object.entries(project.energy.designs).map(([id,design])=>[id,{model_input_sha256:referenceHash(id==='A'?'a':'b'),targets:design.model.zones.map(z=>({object_id:z.id,kind:'zone',label:z.name}))}]))};}
function referenceAssessment(){return{schema_version:'greenlens.architectural-scope-assessment.v1',scope_sha256:referenceHash('7'),report_binding_sha256:referenceHash('8'),relationship:'current',stale_reasons:[],coverage_percent:null,reconciliations:[],unresolved_mapping_count:1,missing_assumption_count:0,disclosure:'Synthetic IPC fixture only'};}
// Original synthetic revision-contract replies. These exercise actual form,
// document and history integration, not native Rust or source equivalence.
function revisionArchitecture(){
  const scope={schema_version:'greenlens.architectural-scope.v1',title:'Retained scope S1',purpose:'Original revision interaction fixture',scope_kind:'partial_building',reference_revision_sha256:referenceHash('2'),reference_geometry_sha256:referenceHash('3'),reference_options_sha256:referenceHash('6'),design_model_sha256:{A:referenceHash('a'),B:referenceHash('b')},mappings:[{occurrence_id:revisionOccurrenceId,status:'mapped',targets:[{design_id:'A',object_id:inputs.cases.A.model.zones[0].id,kind:'zone',model_input_sha256:referenceHash('a')}],rationale:'S1 historical mapping rationale'}],included_scope:['Original wing'],excluded_scope:['Unmodeled annex'],boundary_consequences:'Declared adiabatic interface',assumptions:[{id:'cop-source',link:{kind:'thermal_object',design_id:'B',object_id:inputs.cases.B.model.zones[0].id},field:'/system/rated_gross_cop',status:'assumed',value:'3.2',units:'W/W',source:'Original synthetic concept assumption',rationale:'Declared source value, not the applied value',affected_scope:'Cooling system'}],reconciliations:[],coverage:{status:'unknown',reason:'No whole-building denominator'},whole_building_review:null,report_views:[]};
  const provenance={schema_version:'greenlens.input-provenance-checklist.v1',rows:Array.from({length:105},(_,i)=>({id:`field-${i}`,design_id:'B',object:{kind:'zone',id:inputs.cases.B.model.zones[0].id},field_path:`/synthetic-fixture-field/${i}`,applied_value:i?i:3.5,units:i?'m':'W/W',declared_source:null,status:i?'unreviewed':'assumed',source_declarations:i?[]:[scope.assumptions[0]]})),unreviewed_count:104,missing_count:0,conflicting_count:0,documentation_complete:false,disclosure:'Original synthetic checklist; incomplete provenance does not imply accepted physics.'};
  const first={schema_version:'greenlens.scope-revision.v1',revision_sha256:referenceHash('c'),parent_revision_sha256:null,reference:{asset_id:referenceAsset.id,manifest_sha256:referenceAsset.manifest_sha256,manifest:referenceManifest()},designs:Object.fromEntries(['A','B'].map(id=>[id,{model:inputs.cases[id].model,calendar:inputs.cases[id].study.calendar,study:inputs.cases[id].study}])),scope,carry_forward_plan_sha256:null,review_decisions:[],provenance};
  const second={...structuredClone(first),revision_sha256:referenceHash('d'),parent_revision_sha256:first.revision_sha256,scope:{...structuredClone(scope),title:'Retained scope S2',mappings:[{...structuredClone(scope.mappings[0]),rationale:'S2 live mapping rationale'}]}};
  if(revisionReadinessCase){
    // Original IPC shape fixture: complete declared input provenance does not
    // make late Input/Reference carry-forward decisions resolved.
    second.schema_version='greenlens.scope-revision.v3';second.scope.schema_version='greenlens.architectural-scope.v3';second.carry_forward_plan_sha256=referenceHash('8');
    second.provenance={...second.provenance,schema_version:'greenlens.input-provenance-checklist.v3',rows:[second.provenance.rows[0]],unreviewed_count:0,documentation_complete:true};
    second.review_decisions=Array.from({length:82},(_,index)=>({row_id:index.toString(16).padStart(64,'0'),resolution:index<80?'reviewed':'unresolved',rationale:index===80?'Late input applicability remains unresolved.':index===81?'Late reference correspondence remains unresolved.':'Explicit original synthetic review fixture.'}));
    second.reviewed_rows=second.review_decisions.map((row,index)=>({row_id:row.row_id,kind:index===80?'input':index===81?'reference':'mapping',key:index===80?second.provenance.rows[0].id:index===81?'Exact retained reference correspondence':`Synthetic mapped target ${index}`}));
  }
  return{schema_version:'greenlens.architectural-project.v1',assets:[referenceAsset],active_asset_id:referenceAsset.id,scope:second.scope,scope_history:{schema_version:'greenlens.scope-history.v1',revisions:[first,second],active_revision_sha256:second.revision_sha256}};
}
function carryPlan(project,parent,candidate){
  const revision=project.architecture.scope_history.revisions.find(r=>r.revision_sha256===parent);
  const rows=[{id:'mapping-unchanged',kind:'mapping',key:'synthetic-occurrence',change:'unchanged',requires_review:false,before:revision.scope.mappings[0],after:revision.scope.mappings[0],reason:'Exact geometry unchanged'},{id:'cop-changed',kind:'input',key:'zone cooling COP',change:'changed',requires_review:true,before:3.2,after:3.5,reason:'Efficiency input changed; service unchanged'},{id:'assumption-changed',kind:'assumption',key:'cop-source',change:'changed',requires_review:true,before:revision.scope.assumptions[0],after:revision.scope.assumptions[0],reason:'Review source declaration against the changed applied COP'}];
  if(revisionPlanMode==='geometry')rows.push({id:'deleted-target',kind:'mapping',key:'removed-zone',change:'deleted',requires_review:true,before:'removed-zone',after:null,reason:'No equivalent target inferred'},{id:'new-source',kind:'reference',key:'new-source-revision',change:'unresolved',requires_review:true,before:referenceHash('2'),after:referenceHash('9'),reason:'Reimport requires object reconciliation'});
  if(revisionPlanMode==='v2'||revisionPlanMode==='v3'){
    const proposed_scope={...structuredClone(revision.scope),schema_version:`greenlens.architectural-scope.${revisionPlanMode}`};
    proposed_scope.assumptions[0].status='missing';proposed_scope.assumptions[0].value=null;
    if(candidate?.title==='Reject this candidate')throw Error('Synthetic invalid candidate: title fixture rejected; no revision accepted');
    const candidate_scope=structuredClone(candidate??proposed_scope);
    rows[2].after=structuredClone(candidate_scope.assumptions[0]??null);
    for(const figure of candidate_scope.report_views??[])if(!proposed_scope.report_views.some(previous=>previous.id===figure.id))rows.push({id:`figure-added-${figure.id}`,kind:'report_view',key:figure.id,change:'added',requires_review:true,before:null,after:structuredClone(figure),reason:'Review the exact captured frame for this candidate'});
    let candidate_provenance;
    if(revisionPlanMode==='v3'){
      // Explicit IPC fixture only: Rust's separate tests establish the actual
      // checklist algorithm; this tests the real editor/hook/render composition.
      const cop=candidate_scope.assumptions.filter(a=>a.link.kind==='thermal_object');
      const holiday=candidate_scope.assumptions.filter(a=>a.link.kind==='calendar_no_holidays');
      const none=project.energy.designs.A.study.calendar.holidays.length===0;
      if(holiday.some(a=>a.status!=='missing'&&a.value!==String(none)))throw Error('No-holidays declaration contradicts retained calendar; draft retained');
      const status=items=>!items.length?'unreviewed':items.some(a=>a.status!==items[0].status||a.value!==items[0].value)?'conflicting':items[0].status;
      const checklistRows=[{id:referenceHash('9'),design_id:'B',object:{kind:'cooling_system',id:'fixture-system'},field_path:'/gross_rated_cop',applied_value:3.8,units:'W/W',declared_source:null,status:status(cop),source_declarations:structuredClone(cop)},{id:referenceHash('0'),design_id:'A',object:{kind:'calendar',id:'calendar'},field_path:'/holidays/none',applied_value:none,units:'boolean',declared_source:null,status:status(holiday),source_declarations:structuredClone(holiday)}];
      if(project.energy.designs.B.study===null){const missingStudy={id:referenceHash('1'),design_id:'B',object:{kind:'study',id:'study'},field_path:'',applied_value:null,units:null,declared_source:null,status:'unreviewed',source_declarations:[]};checklistRows.push(missingStudy);rows.push({id:'unassigned-study',kind:'input',key:missingStudy.id,change:'unresolved',requires_review:true,before:{assigned:true},after:structuredClone(missingStudy),reason:'Unassigned study remains missing independently of source status'});}
      candidate_provenance={schema_version:'greenlens.input-provenance-checklist.v3',rows:checklistRows,unreviewed_count:checklistRows.filter(r=>r.status==='unreviewed').length,missing_count:checklistRows.filter(r=>r.status==='missing'||r.applied_value===null).length,conflicting_count:checklistRows.filter(r=>r.status==='conflicting').length,documentation_complete:checklistRows.every(r=>!['unreviewed','missing','conflicting'].includes(r.status)&&r.applied_value!==null),disclosure:'Synthetic IPC fixture; no physics claim'};
      rows[1].after=structuredClone(checklistRows[0]);
      rows.push({id:'calendar-input',kind:'input',key:'calendar no-holidays policy',change:'changed',requires_review:true,before:null,after:structuredClone(checklistRows[1]),reason:'Review exact candidate calendar declaration'});
    }
    return{schema_version:`greenlens.scope-carry-forward.${revisionPlanMode}`,parent_revision_sha256:parent,target_context_sha256:referenceHash('f'),plan_sha256:referenceHash(candidate?'8':'7'),proposed_scope,candidate_scope,...(candidate_provenance?{candidate_provenance}:{}),rows};
  }
  return{schema_version:'greenlens.scope-carry-forward.v1',parent_revision_sha256:parent,target_context_sha256:referenceHash('f'),plan_sha256:referenceHash('7'),proposed_scope:structuredClone(revision.scope),rows};
}
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
    !doc.referenceDrafts.guard&&!doc.energyDrafts.guard&&React.createElement(ReviewGuard,{drafts:doc.scopeReviewDrafts}),React.createElement(Guard,{drafts:doc.energyDrafts}),React.createElement(ReferenceGuard,{drafts:doc.referenceDrafts}));
}
async function setup({retainCache=false,workspace=false,objects=false,reports=false,revisions=false,revisionOccurrence='synthetic-occurrence',readiness=false}={}){
  fullWorkspace=workspace;withRevisions=revisions;revisionReadinessCase=readiness;revisionOccurrenceId=revisionOccurrence;issueGate=null;issueInspectGate=null;issueProductGate=null;issueError=null;issuePending=false;if(!retainCache){issueEvidence=new Map();pendingFixture=null;}revisionPrepareGate=null;revisionAcceptGate=null;revisionPlanMode='cop';
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
    if(command==='energy_run_catalog'){
      const {offset,limit,alternative}=args.request,runs=args.project.energy.runs.filter(row=>!alternative||row.alternative_id===alternative),rows=runs.slice().reverse().slice(offset,offset+limit).map(row=>({...row,name:row.id,finished_utc:'2026-10-16T00:00:00Z',input_revision_sha256:referenceHash('1'),model_input_sha256:referenceHash('2'),method:{method_id:'explicit-ipc-fixture',engine_version:'fixture',recipe_sha256:referenceHash('3'),numerical_settings_sha256:referenceHash('4')},interval_minutes:10,current_method:true,assessment:{execution:'completed',evidence_integrity:'metadata_checked',current_physical_inputs:row.id.startsWith('historical')?'changed':'current',scientific:{status:'unassessed',reason:'IPC metadata fixture, no engine execution',tested_scope:'None'}}}));return{schema_version:'greenlens.energy-run-catalog.v1',project_sha256:referenceHash('5'),offset,limit,alternative,total:runs.length,next_offset:offset+rows.length<runs.length?offset+rows.length:null,ordering:'reverse_retention_order',integrity_scope:'Explicit synthetic IPC only',rows};
    }
    if(command==='energy_result')return null;
    if(command==='energy_report_preflight')return{schema_version:'greenlens.energy-attachment-preflight.v1',ready:true,items:[]};
    if(command==='energy_sensitivity_export_selection')return{included_session_id:null,included_run_ids:['report-a','report-b'],excluded_session_ids:[],policy:'Original synthetic retained-run selection'};
    if(command==='dialog_energy_report')return 'synthetic-report-output';
    if(command==='energy_report'){if(reportGate)await reportGate;if(reportError)throw reportError;return{artifacts:[{path:'executive.html'}],renderer_status:'Synthetic IPC completion only',dataset_sha256:'d'.repeat(64)};}
    if(command==='energy_open_report')return;
    if(command==='report_issue_action'){
      if(issueGate)await issueGate;if(issueError)throw issueError;
      if(args.action.kind==='create'||args.action.kind==='recover'){
        const issue=args.action.kind==='recover'?structuredClone(pendingFixture):issueFixture(args.project,args.action.scope_sha256,{id:referenceHash(issueEvidence.size?'f':'1'),title:`Original issue R${issueEvidence.size+1}`,revision:`R${issueEvidence.size+1}`,a:args.action.a,b:args.action.b,pdf:!args.action.html_only});
        if(issuePending&&args.action.kind==='create'){pendingFixture=issue;return{status:'pending',recovery_token:'synthetic-recovery-token',message:'External save conflict; products retained for same-document recovery.'};}
        issueEvidence.set(issue.summary.id,structuredClone(issue));const next=structuredClone(args.project);next.report_issues??={schema_version:'greenlens.report-issue-index.v1',issues:[]};next.report_issues.issues.push(issue.summary);disk=structuredClone(next);return{status:'issued',view:view(next),issue};
      }
      return{destination:args.action.destination,issue_id:args.action.id,verification:'Synthetic IPC only'};
    }
    if(command==='report_issue_inspect'){if(issueInspectGate)await issueInspectGate;return structuredClone(issueEvidence.get(args.id));}
    if(command==='report_issue_product'){if(issueProductGate)await issueProductGate;const issue=issueEvidence.get(args.id),product=issue.products.products.find(row=>row.path===args.product),base={issue_id:args.id,path:args.product,sha256:product.sha256};return ['open','save_copy','reveal'].includes(args.action)?{...base,status:'prepared',integrity:'verified',copy:'verified',prepared:{path:'owned-output-copy',sha256:product.sha256,bytes:product.bytes,reused:false},handoff:args.action==='save_copy'?'not_requested':'requested',viewer_display:'not_observed'}:{...base,media_type:args.product.endsWith('.svg')?'image/svg+xml':'image/png',svg:'<svg xmlns="http://www.w3.org/2000/svg"><text>Original retained analytical figure</text></svg>',bytes:[137,80,78,71]};}

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
    if(command==='reference_revision_prepare'){if(revisionPrepareGate)await revisionPrepareGate;return carryPlan(args.project,args.parentRevisionSha256,args.candidateScope);}
    if(command==='reference_revision_accept'){if(revisionAcceptGate)await revisionAcceptGate;if(rejectEdit)throw Error('Synthetic revision context rejection');const plan=carryPlan(args.project,args.parentRevisionSha256,['v2','v3'].includes(revisionPlanMode)?args.scope:undefined);assert.equal(args.expectedPlanSha256,plan.plan_sha256);assert.deepEqual(args.scope,plan.candidate_scope??plan.proposed_scope);assert.ok(plan.rows.filter(r=>r.requires_review).every(r=>args.decisions.some(d=>d.row_id===r.id&&d.rationale.trim())));const next=structuredClone(args.project),old=next.architecture.scope_history.revisions.find(r=>r.revision_sha256===args.parentRevisionSha256),revision={...structuredClone(old),revision_sha256:referenceHash('e'),parent_revision_sha256:old.revision_sha256,scope:structuredClone(args.scope),carry_forward_plan_sha256:plan.plan_sha256,review_decisions:structuredClone(args.decisions),...(revisionPlanMode==='v3'?{schema_version:'greenlens.scope-revision.v3',provenance:structuredClone(plan.candidate_provenance),reviewed_rows:args.decisions.map(d=>({row_id:d.row_id,kind:plan.rows.find(r=>r.id===d.row_id).kind,key:plan.rows.find(r=>r.id===d.row_id).key}))}:{})};next.architecture.scope_history.revisions.push(revision);next.architecture.scope_history.active_revision_sha256=revision.revision_sha256;next.architecture.scope=revision.scope;return next;}
    if(command==='reference_revision_activate'){const next=structuredClone(args.project);next.architecture.scope_history.active_revision_sha256=args.revisionSha256;next.architecture.scope=next.architecture.scope_history.revisions.find(r=>r.revision_sha256===args.revisionSha256)?.scope??null;return next;}
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

test('actual report contents selection persists per document and design without changing outputs or accepted inputs',async()=>{
  await setup({workspace:true,reports:true});const accepted=structuredClone(doc.project),past=doc.history.past.length;
  const selected=profile=>button(`${profile} profile`).getAttribute('aria-pressed')==='true';
  await click('reports');assert.ok(selected('Executive'));await click('Technical profile');assert.ok(selected('Technical'));
  await click('results');await click('reports');assert.ok(selected('Technical'));
  await click('Daylight');await click('Energy');assert.ok(selected('Technical'));
  await click('Proposed B');await click('reports');assert.ok(selected('Executive'));
  await click('Technical profile');await click('Executive profile');await click('Baseline A');assert.ok(selected('Technical'));
  assert.deepEqual(doc.project,accepted);assert.equal(doc.history.past.length,past);assert.equal(count('energy_report'),0);assert.equal(count('energy_edit'),0);assert.equal(count('energy_start'),0);
  await click('Save');saveDestination='copied-profile-project';await click('Save As');assert.ok(selected('Technical'));
  await setup({retainCache:true,workspace:true,reports:true});assert.ok(selected('Technical'));await click('Proposed B');assert.ok(selected('Executive'));
  await click('Save');await click('Open');await click('reports');assert.ok(selected('Executive'));assert.equal(doc.project.id,'other-project-id');
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

test('actual workspace inspects a historical mapping independently of a live reference selection',async()=>{
  await setup({workspace:true,objects:true});await prepareReferenceImport();await click('Choose local GLB and import');await click('Start scope for current revision');
  const historicalId='old-revision-occurrence';
  await act(()=>doc.referenceDrafts.update({...doc.referenceDrafts.draft,scope:{...doc.referenceDrafts.draft.scope,value:{...doc.referenceDrafts.draft.scope.value,reference_revision_sha256:referenceHash('9'),mappings:[{occurrence_id:historicalId,status:'excluded',targets:[],rationale:'Only this historical mapping may be inspected'}]}}}));
  await click('Reference');await act(()=>[...document.querySelectorAll('[role="treeitem"]')].find(el=>el.title.startsWith('synthetic-occurrence ')).click());
  const live=structuredClone(doc.thermalPresentation.value.workspace.reference_selection),before=JSON.stringify(doc.referenceDrafts.draft);
  await act(()=>document.querySelector(`.reference-occurrence-button[title="${historicalId}"]`).click());
  assert.equal(input('Mapping rationale').value,'Only this historical mapping may be inspected');
  assert.deepEqual(doc.thermalPresentation.value.workspace.reference_selection,live);assert.equal(JSON.stringify(doc.referenceDrafts.draft),before);
  assert.deepEqual(doc.thermalPresentation.value.workspace.reference_inspection,{revision_sha256:referenceHash('9'),occurrence_id:historicalId});
  await type(input('Mapping rationale'),'A reviewed historical rationale');
  assert.deepEqual(doc.referenceDrafts.draft.scope.value.mappings.map(row=>[row.occurrence_id,row.rationale]),[[historicalId,'A reviewed historical rationale']]);
  const drafts=JSON.stringify(doc.referenceDrafts.draft),past=doc.history.past.length;
  await act(()=>input('Mapping rationale').dispatchEvent(new dom.window.KeyboardEvent('keydown',{key:'Escape',bubbles:true})));
  assert.equal(doc.thermalPresentation.value.workspace.reference_inspection,null);assert.deepEqual(doc.thermalPresentation.value.workspace.reference_selection,live);
  assert.equal(document.querySelector('.reference-mapping'),null);assert.equal(JSON.stringify(doc.referenceDrafts.draft),drafts);assert.equal(doc.history.past.length,past);
  await click('results');await click('reference');assert.equal(document.querySelector('.reference-mapping'),null);
  await click('Daylight');await click('Energy');assert.equal(document.querySelector('.reference-mapping'),null);
  // Explicitly picking the active occurrence is a distinct inspection. The old
  // draft cannot receive that live row merely because it has been selected.
  await click('Reference');await act(()=>[...document.querySelectorAll('[role="treeitem"]')].find(el=>el.title.startsWith('synthetic-occurrence ')).click());
  assert.match(document.querySelector('.reference-mapping').textContent,/another reference revision/);assert.equal(input('Mapping rationale').closest('fieldset').disabled,true);
  assert.equal(button('Add measurement for selected occurrence').disabled,true);assert.equal(JSON.stringify(doc.referenceDrafts.draft),drafts);
});

test('historical inspection survives reimport and delayed loading but is isolated by document and design',async()=>{
  await setup({workspace:true,objects:true});await prepareReferenceImport();await click('Choose local GLB and import');await click('Start scope for current revision');
  await type(input('Scope title'),'Original accepted scope');await type(input('Study purpose'),'Test historical inspection identity');await type(input('Included scope (one item per line)'),'Original synthetic occurrence');await type(input('Boundary-condition consequences'),'No physical interpretation in this fixture');
  const occurrence=document.querySelector('button[data-occurrence-id="synthetic-occurrence"]');assert.ok(occurrence,'Actual active hierarchy exposes the exact admitted occurrence');assert.equal(occurrence.disabled,false);await act(()=>occurrence.click());await type(input('Mapping state'),'excluded');await type(input('Mapping rationale'),'Revision two exact row');await click('Review scope in Rust');await click('Save accepted scope');
  const accepted=JSON.stringify(doc.project.architecture.scope),oldInspection=structuredClone(doc.thermalPresentation.value.workspace.reference_inspection);
  referenceRevision=referenceHash('9');await act(async()=>doc.importReference(referenceManifest().options));
  assert.equal(doc.thermalPresentation.value.workspace.reference_selection,null);assert.deepEqual(doc.thermalPresentation.value.workspace.reference_inspection,oldInspection);
  await click('Edit accepted scope');assert.equal(input('Mapping rationale').value,'Revision two exact row');assert.match(document.querySelector('.reference-mapping').textContent,/absent from active revision/);
  await click('Reference');await act(()=>[...document.querySelectorAll('[role="treeitem"]')].find(el=>el.title.startsWith('synthetic-occurrence ')).click());
  assert.equal(input('Mapping rationale').closest('fieldset').disabled,true);
  await act(()=>document.querySelector('.reference-occurrence-button[title="synthetic-occurrence"]').click());
  assert.equal(input('Mapping rationale').value,'Revision two exact row');assert.equal(doc.thermalPresentation.value.workspace.reference_selection.revision_sha256,referenceHash('9'));assert.deepEqual(doc.thermalPresentation.value.workspace.reference_inspection,oldInspection);
  await click('Daylight');let resume;referenceManifestGate=new Promise(resolve=>resume=resolve);await click('Energy');
  assert.equal(input('Mapping rationale').value,'Revision two exact row');assert.equal(document.querySelector('[role="treeitem"][aria-selected="true"]'),null);
  await act(async()=>{referenceManifestGate=null;resume();});assert.equal(input('Mapping rationale').value,'Revision two exact row');
  await click('Proposed B');assert.equal(doc.thermalPresentation.value.workspace?.reference_inspection??null,null);await click('reference');assert.equal(document.querySelector('.reference-mapping'),null);
  await click('Baseline A');assert.equal(input('Mapping rationale').value,'Revision two exact row');assert.equal(JSON.stringify(doc.project.architecture.scope),accepted);
  await click('Clear mapping inspection');assert.equal(document.querySelector('.reference-mapping'),null);
  // Remove the inspection-only draft before replacement; no accepted scope or
  // original evidence has changed during this entire sequence.
  await click('Discard scope draft…');await click('Discard scope draft');await click('Open');
  assert.equal(doc.project.id,'other-project-id');assert.equal(doc.thermalPresentation.value.workspace?.reference_inspection??null,null);
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
  const distinct={...workspace,reference_inspection:{revision_sha256:referenceHash('9'),occurrence_id:'old-revision-occurrence'}};
  store.set(key,{...old,workspace:distinct});assert.deepEqual(new ThermalPresentationStore({getItem:()=>text,setItem(){}}).get(key).workspace,distinct);
  for(const inspection of [{revision_sha256:'invalid',occurrence_id:'old'},{revision_sha256:referenceHash('9'),occurrence_id:''},{revision_sha256:referenceHash('9'),occurrence_id:'x'.repeat(257)},[]])assert.equal(validThermalPresentation({...old,workspace:{...workspace,reference_inspection:inspection}}),false);
  for(const profile of ['executive','technical']){store.set(key,{...old,workspace:{...workspace,report_contents_profile:profile}});assert.equal(new ThermalPresentationStore({getItem:()=>text,setItem(){}}).get(key).workspace.report_contents_profile,profile);}
  for(const profile of ['certified','',null,{},['executive']])assert.equal(validThermalPresentation({...old,workspace:{...workspace,report_contents_profile:profile}}),false);
  for(const hash of [null,referenceHash('c')])assert.equal(validThermalPresentation({...old,workspace:{...workspace,scope_revision_sha256:hash}}),true);
  for(const hash of ['',123,'not-a-hash',{}])assert.equal(validThermalPresentation({...old,workspace:{...workspace,scope_revision_sha256:hash}}),false);
});

const scopeHistoryPanel=()=>document.querySelector('[aria-label="Study scope revisions"]');
async function scopeReviewSetup(){await setup({workspace:true,objects:true,revisions:true});await click('Save');await click('reference');}
async function reviewDecision(id,resolution,rationale){await type(document.querySelector(`[aria-label="Review decision ${id}"]`),resolution);await type(document.querySelector(`[aria-label="Review rationale ${id}"]`),rationale);}
async function finishReview(){await reviewDecision('cop-changed','reviewed','The efficiency change is intentional; airflow and schedule remain unchanged.');await reviewDecision('assumption-changed','unresolved','Original source COP is retained as a source declaration; product evidence remains missing.');}

test('actual workspace history inspection is independent of live selection, exact revision and current scope drafts',async()=>{
  await scopeReviewSetup();const before=structuredClone(doc.project),past=doc.history.past.length;
  await click('Reference');await act(()=>[...document.querySelectorAll('[role="treeitem"]')].find(el=>el.title.startsWith('synthetic-occurrence ')).click());const live=structuredClone(doc.thermalPresentation.value.workspace.reference_selection);
  await type(input('Inspect scope revision'),referenceHash('c'));await act(()=>scopeHistoryPanel().querySelector('[aria-label="Inspect historical mapping synthetic-occurrence"]').click());
  assert.match(document.querySelector('[aria-label="Historical mapping inspection"]').textContent,/S1 historical mapping rationale/);assert.deepEqual(doc.thermalPresentation.value.workspace.reference_selection,live);assert.equal(doc.referenceDrafts.draft.scope,null);
  await act(()=>scopeHistoryPanel().dispatchEvent(new dom.window.KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true})));assert.equal(document.querySelector('[aria-label="Historical mapping inspection"]'),null);assert.deepEqual(doc.thermalPresentation.value.workspace.reference_selection,live);
  await click('results');await click('reference');assert.equal(input('Inspect scope revision').value,referenceHash('c'));assert.equal(document.querySelector('[aria-label="Historical mapping inspection"]'),null);
  await click('Proposed B');await click('reference');assert.equal(input('Inspect scope revision').value,referenceHash('d'));await click('Baseline A');assert.equal(input('Inspect scope revision').value,referenceHash('c'));
  await click('Daylight');await click('Energy');assert.equal(input('Inspect scope revision').value,referenceHash('c'));assert.deepEqual(doc.project,before);assert.equal(doc.history.past.length,past);
});

test('actual retained-scope mapping table keeps bounded columns and a compact exact-identity inspection action in narrow panels',async()=>{
  const occurrence=`occurrence-${referenceHash('9')}`;
  await setup({workspace:true,objects:true,revisions:true,revisionOccurrence:occurrence});await click('Save');await click('reference');await type(input('Inspect scope revision'),referenceHash('c'));
  const before=structuredClone(doc.project),past=doc.history.past.length,style=document.createElement('style');
  // Exercise the actual workspace/component and the production cascade. jsdom
  // checks explicit layout bounds and behavior, not installed pixel geometry.
  assert.match(fs.readFileSync(path.join(src,'reference/ScopeRevisions.tsx'),'utf8'),/import '\.\/reference-workbench\.css';\s*import '\.\/scope-revisions\.css';/);
  style.textContent=['styles.css','energy/energy.css','reference/reference-workbench.css','reference/scope-revisions.css'].map(file=>fs.readFileSync(path.join(src,file),'utf8')).join('\n');document.head.append(style);
  try {
    const panel=scopeHistoryPanel(),region=panel.querySelector('[aria-label="Retained mappings table"]'),table=region.querySelector('table'),row=table.querySelector('tbody tr'),inspect=row.querySelector('button');
    region.closest('details').open=true;
    for(const width of [260,320]){
      panel.closest('.energy-inspector').style.width=`${width}px`;
      assert.equal(window.getComputedStyle(region).overflow,'auto');assert.equal(window.getComputedStyle(region).maxWidth,'100%');
      assert.equal(window.getComputedStyle(table).tableLayout,'fixed');assert.equal(window.getComputedStyle(table).minWidth,'32rem');
      assert.deepEqual([...table.querySelectorAll('col')].map(col=>window.getComputedStyle(col).width),['13rem','13rem','6rem']);
      assert.equal(window.getComputedStyle(table.querySelector('thead th')).overflowWrap,'normal');assert.equal(window.getComputedStyle(table.querySelector('thead th')).position,'static');
      assert.equal(window.getComputedStyle(row.querySelector('code')).overflowWrap,'anywhere');
      assert.equal(inspect.textContent,'Inspect');assert.equal(inspect.getAttribute('aria-label'),`Inspect historical mapping ${occurrence}`);assert.equal(inspect.title,`Inspect historical mapping ${occurrence}`);assert.equal(window.getComputedStyle(inspect).maxWidth,'100%');
    }
    assert.equal(region.tabIndex,0);region.focus();assert.equal(document.activeElement,region);
    assert.equal(row.querySelector('th').scope,'row');assert.equal(row.querySelector('code').textContent,occurrence);assert.equal(table.querySelectorAll('thead th[scope="col"]').length,3);
    inspect.focus();assert.equal(document.activeElement,inspect);await act(()=>inspect.click());
    assert.equal(inspect.getAttribute('aria-pressed'),'true');assert.match(panel.querySelector('[aria-label="Historical mapping inspection"]').textContent,/S1 historical mapping rationale/);
    await act(()=>inspect.dispatchEvent(new dom.window.KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true})));
    assert.equal(panel.querySelector('[aria-label="Historical mapping inspection"]'),null);assert.equal(inspect.getAttribute('aria-pressed'),'false');
    assert.deepEqual(doc.project,before);assert.equal(doc.history.past.length,past);assert.equal(count('reference_revision_accept'),0);
  }finally{style.remove();}
});

test('retained actual-field provenance is bounded to 100 rows with source and applied values explicitly separate',async()=>{
  await scopeReviewSetup();const panel=scopeHistoryPanel(),table=[...panel.querySelectorAll('table')].find(t=>t.caption?.textContent.includes('Actual supported fields'));
  assert.equal(table.querySelectorAll('tbody tr').length,100);assert.match(table.textContent,/3.5/);assert.match(table.textContent,/Source value: 3.2 W\/W/);assert.match(panel.textContent,/Documentation: Incomplete/);assert.match(panel.textContent,/No reviewed source declaration/);
  await click('Next provenance',panel);assert.equal(table.querySelectorAll('tbody tr').length,5);assert.match(table.textContent,/fixture-field\/104/);await click('Previous provenance',panel);assert.equal(table.querySelectorAll('tbody tr').length,100);
});

test('review decisions survive panel, discipline, design and local recovery without becoming accepted inputs',async()=>{
  await scopeReviewSetup();await type(input('Inspect scope revision'),referenceHash('c'));const original=structuredClone(doc.project);await click('Review changes and carry forward',scopeHistoryPanel());
  assert.match(scopeHistoryPanel().textContent,/unchanged: 1 · changed: 2 · deleted: 0 · added: 0 · unresolved: 0/);assert.equal(button('Accept reviewed scope revision').disabled,true);await reviewDecision('cop-changed','reviewed','Retained exact review rationale');
  await click('reports');await click('reference');assert.equal(document.querySelector('[aria-label="Review rationale cop-changed"]').value,'Retained exact review rationale');await click('Daylight');await click('Energy');assert.ok(document.querySelector('[aria-label="Pending scope carry-forward review"]'));
  await click('Proposed B');await click('reference');assert.equal(document.querySelector('[aria-label="Pending scope carry-forward review"]'),null);await click('Baseline A');assert.ok(doc.scopeReviewDrafts.draft);assert.deepEqual(doc.project,original);assert.equal(count('reference_revision_accept'),0);
  await setup({retainCache:true,workspace:true,objects:true,revisions:true});assert.equal(doc.scopeReviewDrafts.draft.decisions['cop-changed'].rationale,'Retained exact review rationale');assert.match(scopeHistoryPanel().textContent,/review is stale/);assert.equal(button('Accept reviewed scope revision').disabled,true);
  await click('Discard review');assert.equal(doc.scopeReviewDrafts.draft,null);assert.equal(doc.history.past.length,0);
});

test('Save review Apply creates one immutable revision and Undo switches active history without deleting revisions',async()=>{
  await scopeReviewSetup();const old=structuredClone(doc.project.architecture.scope_history);await type(input('Inspect scope revision'),referenceHash('c'));await click('Review changes and carry forward');await finishReview();const proposal=structuredClone(doc.scopeReviewDrafts.draft.plan.proposed_scope);
  await click('Save');assert.match(dialog().textContent,/Resolve scope review drafts/);await click('Cancel',dialog());assert.equal(count('reference_revision_accept'),0);assert.ok(doc.scopeReviewDrafts.draft);
  await click('Save');await click('Apply',dialog());assert.equal(dialog(),null);assert.equal(count('reference_revision_accept'),1);assert.equal(doc.scopeReviewDrafts.draft,null);assert.equal(doc.history.past.length,1);assert.deepEqual(doc.project.architecture.scope_history.revisions.slice(0,2),old.revisions);assert.deepEqual(doc.project.architecture.scope,proposal);assert.equal(doc.project.architecture.scope_history.revisions[2].review_decisions[1].resolution,'unresolved');
  await click('Undo');assert.equal(doc.project.architecture.scope_history.active_revision_sha256,old.active_revision_sha256);assert.deepEqual(doc.project.architecture.scope_history.revisions.slice(0,2),old.revisions);assert.equal(doc.project.architecture.scope_history.revisions.length,3);assert.deepEqual(doc.project.architecture.scope,old.revisions[1].scope);
});

test('review guards cover save, Save As, run, open and close commands; cancel preserves every draft',async()=>{
  for(const action of ['Save','Save As','Run','Open','Close']){
    await scopeReviewSetup();await click('Review changes and carry forward');await finishReview();const original=structuredClone(doc.project),draft=structuredClone(doc.scopeReviewDrafts.draft),history=doc.history.past.length;calls=[];
    await click(action);assert.match(dialog().textContent,/Resolve scope review drafts/);await click('Cancel',dialog());assert.deepEqual(doc.project,original);assert.deepEqual(doc.scopeReviewDrafts.draft,draft);assert.equal(doc.history.past.length,history);assert.equal(count('reference_revision_accept'),0);assert.equal(count('save_project'),0);assert.equal(count('save_project_as'),0);assert.equal(count('energy_start'),0);assert.equal(count('destroy'),0);
  }
});

test('native-close callback retains pending reviews and multiple design reviews never partially Apply',async()=>{
  await scopeReviewSetup();await click('Review changes and carry forward');await finishReview();let prevented=false;
  await act(async()=>closeHandler({preventDefault(){prevented=true;}}));assert.equal(prevented,true);assert.match(dialog().textContent,/scope review drafts/);await click('Cancel',dialog());assert.equal(count('destroy'),0);
  await click('Proposed B');await click('reference');await click('Review changes and carry forward');await finishReview();const accepted=structuredClone(doc.project);assert.deepEqual(doc.scopeReviewDrafts.pending,['A','B']);
  await click('Save');await click('Apply',dialog());assert.match(dialog().textContent,/Resolve each design review separately/);assert.equal(count('reference_revision_accept'),0);assert.deepEqual(doc.project,accepted);await click('Cancel',dialog());
  await click('Discard review');assert.deepEqual(doc.scopeReviewDrafts.pending,['A']);await click('Baseline A');assert.ok(doc.scopeReviewDrafts.draft);
});

test('invalid revision acceptance and unresolved required rows preserve the review and accepted history',async()=>{
  await scopeReviewSetup();revisionPlanMode='geometry';await click('Review changes and carry forward');await finishReview();assert.match(scopeHistoryPanel().textContent,/deleted: 1 · added: 0 · unresolved: 1/);assert.equal(button('Accept reviewed scope revision').disabled,true);
  await reviewDecision('deleted-target','unresolved','Deleted analytical target cannot be assumed equivalent.');await reviewDecision('new-source','unresolved','Reimport correspondence is not established.');const original=structuredClone(doc.project),draft=structuredClone(doc.scopeReviewDrafts.draft);rejectEdit=true;
  await click('Accept reviewed scope revision');assert.equal(count('reference_revision_accept'),1);assert.deepEqual(doc.project,original);assert.deepEqual(doc.scopeReviewDrafts.draft,draft);assert.equal(doc.history.past.length,0);assert.match(doc.error,/Synthetic revision context rejection/);
  await click('Save');await click('Apply',dialog());assert.ok(dialog());assert.deepEqual(doc.scopeReviewDrafts.draft,draft);await click('Discard',dialog());assert.equal(doc.scopeReviewDrafts.draft,null);assert.deepEqual(doc.project,original);
});

test('accepted input changes mark review stale and applying a separate thermal draft cannot silently rebind it',async()=>{
  await scopeReviewSetup();await click('Review changes and carry forward');await finishReview();const retained=structuredClone(doc.scopeReviewDrafts.draft);await click('model');await editName('Accepted changed context');await click('Validate and apply');await click('reference');
  assert.match(scopeHistoryPanel().textContent,/review is stale/);assert.equal(button('Accept reviewed scope revision').disabled,true);assert.deepEqual(doc.scopeReviewDrafts.draft,retained);await click('Save');await click('Apply',dialog());assert.ok(dialog());assert.equal(count('reference_revision_accept'),0);assert.match(doc.error,/review remains intact but is stale/);await click('Cancel',dialog());
  await click('Discard review');await click('Review changes and carry forward');await finishReview();await click('model');await editName('Still a raw thermal draft');await click('reference');await click('Accept reviewed scope revision');assert.match(dialog().textContent,/thermal/);await click('Apply',dialog());assert.equal(count('reference_revision_accept'),0);assert.ok(doc.scopeReviewDrafts.draft);assert.match(doc.error,/review remains intact but is stale/);
});

test('late or cancelled preparation cannot stage a different revision or document review',async()=>{
  await scopeReviewSetup();let finish;revisionPrepareGate=new Promise(resolve=>finish=resolve);await click('Review changes and carry forward');await click('Cancel review request');await act(async()=>finish());assert.equal(doc.scopeReviewDrafts.draft,null);assert.equal(count('reference_revision_accept'),0);
  revisionPrepareGate=new Promise(resolve=>finish=resolve);await click('Review changes and carry forward');await type(input('Inspect scope revision'),referenceHash('c'));await act(async()=>finish());assert.equal(doc.scopeReviewDrafts.draft,null);
  revisionPrepareGate=new Promise(resolve=>finish=resolve);await click('Review changes and carry forward');await click('Daylight');await act(async()=>finish());await click('Energy');assert.equal(doc.scopeReviewDrafts.draft,null);
  revisionPrepareGate=new Promise(resolve=>finish=resolve);await click('Review changes and carry forward');await click('Open');await act(async()=>finish());assert.equal(doc.project.id,'other-project-id');assert.equal(doc.scopeReviewDrafts.draft,null);
});


async function issueSetup(){await setup({workspace:true,revisions:true,reports:true});await click('Save');await click('reports');await type(input('Issue scope revision'),referenceHash('c'));}
const issuePanel=()=>document.querySelector('[aria-label="Immutable report issues"]');

test('actual issue workspace discloses late unresolved Input and Reference decisions despite complete input provenance',async()=>{
  await setup({workspace:true,revisions:true,reports:true,readiness:true});await click('Save');await click('reports');await type(input('Issue scope revision'),referenceHash('d'));
  const original=JSON.stringify(doc.project.architecture.scope_history);
  assert.match(issuePanel().textContent,/Documentation: complete source declarations for supported fields/);
  assert.match(issuePanel().textContent,/Retained unresolved review decisions: 2/);
  await click('Issue exact selected report');
  const status=document.querySelector('[aria-label="Retained documentation and review status"]');assert.ok(status);
  assert.match(status.textContent,/Input documentation: complete source declarations for supported fields/);
  assert.match(status.textContent,/Unresolved review decisions: 2 of 82/);assert.match(status.textContent,/Reviewed decisions: 80/);
  assert.match(status.textContent,/input: B · zone .*\/synthetic-fixture-field\/0 \[W\/W\]/);assert.match(status.textContent,/Exact input identity: field-0/);assert.match(status.textContent,/reference: Exact retained reference correspondence/);
  assert.match(status.textContent,/Late input applicability remains unresolved/);assert.match(status.textContent,/Late reference correspondence remains unresolved/);
  assert.match(status.textContent,/Showing 2 of 2 unresolved decisions/);assert.equal(status.querySelectorAll('li').length,2);
  const retained=issueEvidence.get(referenceHash('1'));assert.deepEqual(retained.review_readiness,reviewFixture(retained.content.scope_revision));
  assert.equal(JSON.stringify(doc.project.architecture.scope_history),original);assert.equal(count('energy_start'),0);assert.equal(doc.history.past.length,0);
});

test('issued report composition freezes exact S1 pair and preserves R1 after active S2 and later metadata',async()=>{
  await issueSetup();assert.equal(button('Issue exact selected report').disabled,false);const before=JSON.stringify(doc.project.architecture.scope_history);await click('Issue exact selected report');
  const call=calls.find(([command])=>command==='report_issue_action')[1];assert.deepEqual(call.action,{kind:'create',a:'report-a',b:'report-b',scope_sha256:referenceHash('c'),html_only:false});assert.equal(call.expectedRevision,'test-revision');assert.equal(doc.project.report_issues.issues.length,1);assert.equal(doc.history.past.length,0);assert.equal(JSON.stringify(doc.project.architecture.scope_history),before);
  const r1=JSON.stringify(issueEvidence.get(referenceHash('1')));assert.match(issuePanel().textContent,/Retained scope: Retained scope S1/);assert.match(issuePanel().textContent,/Missing for these exact runs/);assert.match(issuePanel().textContent,/unassessed/);assert.match(issuePanel().textContent,/recorded at issuance/);assert.equal(document.querySelector('[aria-label="Portable ZIP assessment selection"]'),null);assert.equal(button('Export portable evidence ZIP…').disabled,true);
  await type(input('Issue scope revision'),referenceHash('d'));assert.match(issuePanel().textContent,/Historical issue/);assert.match(issuePanel().textContent,/Retained scope: Retained scope S1/);
  await act(()=>doc.commitEnergyMetadata({kind:'report_issue',metadata:{title:'Later active metadata',revision:'R9',issue_date:null,author:null,preparer:null,revision_history:[]}},'Later report metadata'));assert.match(document.querySelector('[aria-label="Inspected immutable issue"]').textContent,/Original issue R1 · R1/);assert.doesNotMatch(document.querySelector('[aria-label="Inspected immutable issue"]').textContent,/Later active metadata/);assert.equal(JSON.stringify(issueEvidence.get(referenceHash('1'))),r1);
  await click('Issue exact selected report');assert.equal(doc.project.report_issues.issues.length,2);assert.equal(doc.history.past.length,1);await type(input('Inspect issued report'),referenceHash('1'));assert.match(issuePanel().textContent,/Retained scope: Retained scope S1/);assert.equal(JSON.stringify(issueEvidence.get(referenceHash('1'))),r1);await click('Undo');assert.equal(doc.project.report_issues.issues.length,2);assert.equal(count('energy_start'),0);
});

test('issue creation respects real Apply Discard Cancel guard and failed Save without issuing old inputs',async()=>{
  await issueSetup();await click('model');await editName('Unapplied issue draft');await click('reports');await click('Issue exact selected report');assert.ok(dialog());assert.equal(count('report_issue_action'),0);await click('Cancel',dialog());assert.equal(count('report_issue_action'),0);assert.ok(doc.energyDrafts.get('A').dirty);
  await click('Issue exact selected report');await click('Discard',dialog());assert.equal(count('report_issue_action'),1);assert.equal(doc.energyDrafts.get('A').dirty,false);
  const original=globalThis.__draftIPC;globalThis.__draftIPC=async(command,args)=>{if(command==='save_project')throw Error('Synthetic save conflict');return original(command,args);};await click('Issue exact selected report');assert.equal(count('report_issue_action'),1);assert.match(doc.error,/Synthetic save conflict/);globalThis.__draftIPC=original;
});

test('issue draft Apply remains validated and binds accepted metadata without erasing edit history',async()=>{
  await issueSetup();await click('evidence');await act(()=>doc.energyDrafts.updateIssue(d=>({...d,enabled:true,title:'Reviewed issue title',revision:'R3',issue_date:'2026-',dirty:true})));await click('reports');await click('Issue exact selected report');await click('Apply',dialog());assert.ok(dialog());assert.equal(count('report_issue_action'),0);assert.equal(doc.history.past.length,0);assert.equal(doc.energyDrafts.issue().issue_date,'2026-');await click('Cancel',dialog());
  await click('evidence');await type(input('Issue date'),'2026-10-16');await click('reports');await click('Issue exact selected report');await click('Apply',dialog());assert.equal(count('report_issue_action'),1);assert.equal(doc.history.past.length,1);assert.equal(doc.energyDrafts.issue().dirty,false);const submitted=calls.find(([command])=>command==='report_issue_action')[1];assert.equal(submitted.project.energy.workbench.report_metadata.title,'Reviewed issue title');assert.equal(submitted.project.energy.workbench.report_metadata.issue_date,'2026-10-16');assert.equal(doc.project.report_issues.issues.length,1);
});

test('issued product preview opens only enumerated exact bytes and late image response cannot replace another issue',async()=>{
  await issueSetup();await click('Issue exact selected report');await click('Preview figures/aligned-1.svg');let image=document.querySelector('[aria-label="Rendered issue product preview"] img');assert.ok(image.src.startsWith('data:image/svg+xml;'));assert.match(decodeURIComponent(image.src),/Original retained analytical figure/);assert.equal(document.querySelector('svg text'),null);assert.equal(document.querySelector('object,iframe'),null);
  await click('Open retained executive.pdf');let request=calls.filter(([command])=>command==='report_issue_product').at(-1)[1];assert.equal(request.action,'open');assert.equal(request.id,referenceHash('1'));assert.equal(request.product,'executive.pdf');assert.match(issuePanel().textContent,/Viewer display has not been confirmed/);assert.doesNotMatch(issuePanel().textContent,/Opened retained executive.pdf/);
  await click('Save Copy executive.pdf…');assert.equal(calls.filter(([command])=>command==='report_issue_product').at(-1)[1].action,'save_copy');assert.match(issuePanel().textContent,/No viewer was requested/);
  await click('Open copy folder executive.pdf');assert.equal(calls.filter(([command])=>command==='report_issue_product').at(-1)[1].action,'reveal');assert.match(issuePanel().textContent,/Windows handoff requested/);
  await type(input('Issue scope revision'),referenceHash('d'));await click('Issue exact selected report');let finish;issueProductGate=new Promise(resolve=>finish=resolve);await click('Preview figures/aligned-1.svg');await type(input('Inspect issued report'),referenceHash('1'));await act(async()=>finish());assert.equal(document.querySelector('[aria-label="Rendered issue product preview"]'),null);
  const original=globalThis.__draftIPC;globalThis.__draftIPC=async(command,args)=>command==='report_issue_product'?{issue_id:referenceHash('f'),path:args.product,sha256:referenceHash('a'),media_type:'image/svg+xml',svg:'<svg/>'}:original(command,args);await click('Preview figures/aligned-1.svg');assert.match(issuePanel().textContent,/Product response differs/);assert.equal(document.querySelector('[aria-label="Rendered issue product preview"]'),null);globalThis.__draftIPC=original;
});

test('issued history inspection is cancellable and rejects changed scope or inventory bindings',async()=>{
  await issueSetup();await click('Issue exact selected report');await type(input('Inspect issued report'),'');let finish,request;issueInspectGate=new Promise(resolve=>finish=resolve);globalThis.__inspectionProtocol=(command,args)=>{if(command==='inspect_energy_evidence'&&args.query.command==='report_issue_inspect')request=args;};
  await type(input('Inspect issued report'),referenceHash('1'));await act(()=>request.onProgress.onmessage({schema_version:'greenlens.project-open-progress.v1',stage:'issue_products',run_id:null,runs_completed:0,runs_total:2,bytes_read:1048576,files_verified:2,elapsed_ms:3,numeric_passes:{}}));assert.match(issuePanel().textContent,/issue products · 1.0 MiB read/);
  await act(()=>document.querySelector('[aria-label="Cancel inspection: Checking the selected immutable report issue and its retained evidence"]').click());await act(async()=>finish());assert.equal(document.querySelector('[aria-label="Inspected immutable issue"]'),null);assert.match(issuePanel().textContent,/inspection cancelled/);delete globalThis.__inspectionProtocol;
  issueInspectGate=null;issueEvidence.get(referenceHash('1')).content.scope_revision.revision_sha256=referenceHash('d');await type(input('Inspect issued report'),'');await type(input('Inspect issued report'),referenceHash('1'));assert.match(issuePanel().textContent,/content differs/);assert.equal(document.querySelector('[aria-label="Inspected immutable issue"]'),null);
});

test('complete issue export and regeneration use explicit native destinations without running an engine',async()=>{
  await issueSetup();await click('Issue exact selected report');const history=JSON.stringify(doc.project.report_issues),before=count('energy_start');await click('Regenerate exact issue into folder…');let action=calls.filter(([command])=>command==='report_issue_action').at(-1)[1].action;assert.deepEqual(action,{kind:'regenerate',id:referenceHash('1'),destination:'synthetic-report-output'});assert.match(issuePanel().textContent,/Exact issue products regenerated/);
  await click('Export complete issued project ZIP…');action=calls.filter(([command])=>command==='report_issue_action').at(-1)[1].action;assert.deepEqual(action,{kind:'export',destination:'synthetic-report-output'});assert.equal(calls.filter(([command])=>command==='dialog_energy_report').at(-1)[1].portableZip,true);assert.equal(JSON.stringify(doc.project.report_issues),history);assert.equal(count('energy_start'),before);
  await click('Save As');assert.equal(JSON.stringify(doc.project.report_issues),history);await click('Daylight');await click('Energy');assert.equal(input('Inspect issued report').value,referenceHash('1'));assert.match(issuePanel().textContent,/Retained scope: Retained scope S1/);
});

test('completed issue conflict preserves recovery receipt across navigation and reload without stale Save',async()=>{
  await issueSetup();issuePending=true;await click('Issue exact selected report');assert.equal(doc.project.report_issues,undefined);assert.equal(doc.pendingReportIssues.items.length,1);assert.match(issuePanel().textContent,/reopen the saved project/);await click('Daylight');await click('Energy');assert.ok(button('Recover completed report'));assert.match(window.localStorage.getItem('greenlens.pending-report-issues.v1'),/synthetic-recovery-token/);
  const previousIPC=globalThis.__draftIPC;globalThis.__draftIPC=async(command,args)=>{if(command==='admit_project')return{schema_version:'greenlens.project-admission.v1',project_id:disk.id,project_name:disk.name,manifest_sha256:'f'.repeat(64),static_runs:0,climate_runs:0,energy_runs:2,evidence_checked:false};if(command==='verify_project')return view(disk);return previousIPC(command,args);};await click('Open');assert.equal(doc.project.id,'draft-fixture');await click('reports');assert.ok(button('Recover completed report'));globalThis.__draftIPC=previousIPC;
  const saves=count('save_project');issuePending=false;await click('Recover completed report');assert.equal(count('save_project'),saves);assert.equal(doc.pendingReportIssues.items.length,0);assert.equal(doc.project.report_issues.issues.length,1);assert.equal(calls.filter(([command])=>command==='report_issue_action').at(-1)[1].action.kind,'recover');
});



test('explicit historical pair survives actual workspace discipline design and panel remount without changing accepted inputs',async()=>{
  await issueSetup();const inputsBefore=JSON.stringify(doc.project.energy.designs);await click('compare');await type(input('Baseline'),'historical-a');await type(input('Proposed'),'historical-b');await click('reports');await click('Issue exact selected report');
  assert.deepEqual(calls.filter(([c])=>c==='report_issue_action').at(-1)[1].action,{kind:'create',a:'historical-a',b:'historical-b',scope_sha256:referenceHash('c'),html_only:false});
  await click('Daylight');await click('Energy');assert.match(issuePanel().textContent,/Baseline snapshot: historical-a.*Proposed snapshot: historical-b/);assert.match(issuePanel().textContent,/selected pair and scope IDs match/);
  await click('Switch design');await click('compare');assert.equal(input('Baseline').value,'__automatic__');assert.equal(input('Proposed').value,'__automatic__');await type(input('Proposed'),'historical-b');await click('Switch design');await click('compare');assert.equal(input('Baseline').value,'historical-a');assert.equal(input('Proposed').value,'historical-b');
  await click('reports');await click('Save As');await click('Daylight');await click('Energy');assert.match(issuePanel().textContent,/Baseline snapshot: historical-a.*Proposed snapshot: historical-b/);assert.equal(JSON.stringify(doc.project.energy.designs),inputsBefore);assert.equal(doc.history.past.length,0);
  await click('Open');await click('compare');assert.equal(input('Baseline').value,'__automatic__');assert.equal(input('Proposed').value,'__automatic__');assert.equal(doc.project.id,'other-project-id');
});

test('explicit no-run and unavailable cached run choices never fall back to automatic current snapshots',async()=>{
  await issueSetup();await click('compare');await type(input('Baseline'),'');await click('reports');assert.match(issuePanel().textContent,/Baseline snapshot: Not selected/);assert.equal(button('Issue exact selected report').disabled,true);await click('Daylight');await click('Energy');assert.match(issuePanel().textContent,/Baseline snapshot: Not selected/);
  await act(()=>doc.thermalPresentation.update(p=>({...p,workspace:{...p.workspace,energy_run_selection:{A:'unavailable-old-run',B:'historical-b'}}})));await click('compare');assert.equal(input('Baseline').value,'unavailable-old-run');assert.match(document.body.textContent,/No current run is substituted/);await click('reports');assert.match(issuePanel().textContent,/Baseline snapshot: Not selected/);assert.equal(button('Issue exact selected report').disabled,true);
  await click('compare');await type(input('Baseline'),'__automatic__');await click('reports');assert.match(issuePanel().textContent,/Baseline snapshot: report-a/);assert.equal(button('Issue exact selected report').disabled,false);
});

test('bounded display storage validates and restores exact explicit pair choices without a scientific identity',()=>{
  let text;const storage={getItem:()=>text,setItem:(_,value)=>text=value},key=JSON.stringify(['pair-display-fixture','A']),store=new ThermalPresentationStore(storage),original=store.get(key);
  const withPicks={...original,workspace:{panel:'reports',inspector:null,reference_selection:null,energy_run_selection:{A:'historical-a',B:null}}};store.set(key,withPicks);assert.deepEqual(new ThermalPresentationStore(storage).get(key).workspace.energy_run_selection,{A:'historical-a',B:null});
  for(const picks of [[],null,{C:'wrong-design'},{A:''},{A:'x'.repeat(257)},{A:123}])assert.equal(validThermalPresentation({...withPicks,workspace:{...withPicks.workspace,energy_run_selection:picks}}),false);
});

test('actual report product inspection shows ticketed progress cancels once and hides late success',async()=>{
  await issueSetup();await click('Issue exact selected report');let finish,request;const cancellations=[];issueProductGate=new Promise(resolve=>finish=resolve);globalThis.__inspectionProtocol=(command,args)=>{if(command==='report_issue_product')request=args;if(command==='cancel_evidence_inspection')cancellations.push(args.ticket);};
  await click('Preview figures/aligned-1.svg');assert.ok(Number.isSafeInteger(request.ticket));assert.equal(button('Open retained executive.pdf').disabled,true);assert.match(issuePanel().textContent,/Checking the requested retained report product/);await act(()=>request.onProgress.onmessage({schema_version:'greenlens.project-open-progress.v1',stage:'issue_product_bytes',run_id:null,runs_completed:0,runs_total:0,bytes_read:2097152,files_verified:1,elapsed_ms:12,numeric_passes:{}}));assert.match(issuePanel().textContent,/issue product bytes · 2.0 MiB read/);
  await act(()=>document.querySelector('[aria-label="Cancel inspection: Checking the requested retained report product"]').click());assert.deepEqual(cancellations,[request.ticket]);await act(async()=>finish());assert.equal(document.querySelector('[aria-label="Rendered issue product preview"]'),null);assert.match(issuePanel().textContent,/inspection cancelled/);delete globalThis.__inspectionProtocol;
});

test('report product verification is cancelled on issue switch and actual discipline unmount',async()=>{
  await issueSetup();await click('Issue exact selected report');await type(input('Issue scope revision'),referenceHash('d'));await click('Issue exact selected report');let finish,request;const cancelled=[];globalThis.__inspectionProtocol=(command,args)=>{if(command==='report_issue_product')request=args;if(command==='cancel_evidence_inspection')cancelled.push(args.ticket);};
  issueProductGate=new Promise(resolve=>finish=resolve);await click('Preview figures/aligned-1.svg');const old=request.ticket;await type(input('Inspect issued report'),referenceHash('1'));assert.ok(cancelled.includes(old));await act(async()=>finish());assert.equal(document.querySelector('[aria-label="Rendered issue product preview"]'),null);
  issueProductGate=new Promise(resolve=>finish=resolve);await click('Open retained executive.pdf');const closing=request.ticket;await click('Daylight');assert.ok(cancelled.includes(closing));await act(async()=>finish());await click('Energy');assert.equal(document.querySelector('[aria-label="Rendered issue product preview"]'),null);assert.doesNotMatch(issuePanel().textContent,/Opened retained executive.pdf/);delete globalThis.__inspectionProtocol;
});



test('choosing a historical other-design row opens that exact result and retains the target selection',async()=>{
  await issueSetup();await click('history');const row=[...document.querySelectorAll('tbody tr')].find(row=>row.textContent.includes('historical-b'));assert.ok(row);await click('Use for proposed B',row);assert.equal(doc.alternative,'B');assert.equal(doc.thermalPresentation.value.workspace.panel,'results');assert.equal(doc.thermalPresentation.value.workspace.energy_run_selection.B,'historical-b');await click('Daylight');await click('Energy');await click('compare');assert.equal(input('Proposed').value,'historical-b');
});

test.after(async()=>{if(root)await act(()=>root.unmount());dom.window.close();});


const editableCandidate=()=>document.querySelector('[aria-label="Editable carry-forward candidate"]');
function candidateInput(label){const container=editableCandidate(),field=[...container.querySelectorAll('label')].find(l=>l.firstChild?.textContent===label);assert.ok(field,`candidate field ${label}`);return field.querySelector('input,textarea,select');}
async function startV2Review(){await scopeReviewSetup();revisionPlanMode='v2';await click('Review changes and carry forward');assert.ok(editableCandidate());}
async function finishV2Review(){await reviewDecision('cop-changed','reviewed','The supported efficiency change keeps service unchanged.');await reviewDecision('assumption-changed','reviewed','Explicit corrected synthetic source declaration for this candidate.');}
async function captureTopReportView(title){
  // Exercise the real ThermalView capture handler and workspace callback. This
  // DOM fixture supplies a display camera/frame, not rendered/native evidence.
  await act(()=>doc.thermalPresentation.update(p=>({...p,active_view:'top',cameras:{...p.cameras,top:{position:[2,3,20],target:[2,3,0],up:[0,1,0],zoom:2}}})));
  const pane=document.querySelector('[aria-label="Top thermal view"]');assert.ok(pane);
  pane.getBoundingClientRect=()=>({x:0,y:0,left:0,top:0,right:400,bottom:300,width:400,height:300});
  await type(input('View name'),title);assert.equal(button('Add active view to report draft').disabled,false);await click('Add active view to report draft');
}

for(const version of ['v2','v3'])test(`${version} viewport capture updates only its exact candidate and requires refreshed Rust review before one accepted revision`,async()=>{
  await scopeReviewSetup();await click('Edit accepted scope');await type(input('Scope title'),'Preserved unrelated ordinary scope draft');
  const ordinary=structuredClone(doc.referenceDrafts.draft),accepted=structuredClone(doc.project),past=doc.history.past.length;
  revisionPlanMode=version;await click('Review changes and carry forward');const plan=structuredClone(doc.scopeReviewDrafts.draft.plan);
  await captureTopReportView('Candidate aligned plan');
  const captured=doc.scopeReviewDrafts.draft.candidate.value.report_views[0];
  assert.equal(captured.title,'Candidate aligned plan');assert.equal(captured.design_id,'A');assert.equal(captured.model_input_sha256,referenceHash('a'));
  assert.deepEqual(captured.eye_m,[2,3,20]);assert.equal(captured.vertical_span_m,8);assert.equal(captured.aspect_ratio,4/3);
  assert.deepEqual(doc.referenceDrafts.draft,ordinary);assert.deepEqual(doc.scopeReviewDrafts.draft.plan,plan);assert.deepEqual(doc.project,accepted);
  assert.equal(button('Accept reviewed scope revision').disabled,true);assert.match(scopeHistoryPanel().textContent,/Candidate edits have not been reviewed in Rust/);
  await click('reports');await click('reference');assert.deepEqual(doc.scopeReviewDrafts.draft.candidate.value.report_views,[captured]);
  await type(candidateInput('Scope title'),'Reject this candidate');await click('Review edited candidate in Rust');
  assert.match(scopeHistoryPanel().textContent,/Synthetic invalid candidate/);assert.deepEqual(doc.project,accepted);assert.equal(doc.history.past.length,past);assert.deepEqual(doc.scopeReviewDrafts.draft.candidate.value.report_views,[captured]);
  await type(candidateInput('Scope title'),'Candidate with aligned report frame');let finish;revisionPrepareGate=new Promise(resolve=>finish=resolve);
  await click('Review edited candidate in Rust');await click('Cancel review request');await act(async()=>finish());
  assert.deepEqual(doc.scopeReviewDrafts.draft.plan,plan);assert.deepEqual(doc.referenceDrafts.draft,ordinary);assert.equal(doc.history.past.length,past);
  revisionPrepareGate=null;await click('Review edited candidate in Rust');await finishV2Review();if(version==='v3')await reviewDecision('calendar-input','unresolved','Calendar source remains missing in this candidate; disclose it.');
  assert.equal(button('Accept reviewed scope revision').disabled,true,'the newly added figure needs an explicit review');
  await reviewDecision('figure-added-view-1','reviewed','Exact source-bound top frame retained for this synthetic candidate.');
  await click('Accept reviewed scope revision');assert.ok(dialog());await click('Cancel',dialog());
  assert.deepEqual(doc.referenceDrafts.draft,ordinary);assert.deepEqual(doc.project,accepted);assert.equal(doc.history.past.length,past);assert.equal(count('reference_revision_accept'),0);
  await click('Accept reviewed scope revision');await click('Discard',dialog());
  assert.equal(count('reference_revision_accept'),1);assert.equal(count('reference_scope_apply'),0);assert.equal(doc.history.past.length,past+1);
  assert.deepEqual(doc.project.architecture.scope.report_views,[captured]);assert.deepEqual(doc.project.architecture.scope_history.revisions.slice(0,2),accepted.architecture.scope_history.revisions);
  assert.deepEqual(doc.project.energy.designs,accepted.energy.designs);assert.equal(count('energy_start'),0);
});

for(const version of ['v2','v3'])test(`${version} viewport capture remains design-local and a stale current review cannot redirect it into ordinary drafts`,async()=>{
  await scopeReviewSetup();revisionPlanMode=version;await click('Review changes and carry forward');const pendingA=structuredClone(doc.scopeReviewDrafts.draft);
  await click('Proposed B');await click('reference');assert.equal(doc.scopeReviewDrafts.draft,null);
  await captureTopReportView('Ordinary B plan');const ordinaryB=structuredClone(doc.referenceDrafts.draft);
  assert.equal(ordinaryB.scope.value.report_views[0].design_id,'B');
  await click('Baseline A');await click('reference');assert.deepEqual(doc.scopeReviewDrafts.draft,pendingA);assert.deepEqual(doc.referenceDrafts.draft,ordinaryB);
  await click('model');await editName('Changed accepted model after scope preview');await click('Validate and apply');await click('reference');
  const accepted=structuredClone(doc.project),past=doc.history.past.length;
  await captureTopReportView('Must not enter either draft');
  assert.match(doc.error,/pending scope review no longer matches/);assert.deepEqual(doc.scopeReviewDrafts.draft,pendingA);assert.deepEqual(doc.referenceDrafts.draft,ordinaryB);
  assert.deepEqual(doc.project,accepted);assert.equal(doc.history.past.length,past);assert.equal(count('reference_revision_accept'),0);
});

test('historical noneditable review rejects viewport capture without overwriting ordinary or review drafts',async()=>{
  await scopeReviewSetup();await click('Review changes and carry forward');
  const review=structuredClone(doc.scopeReviewDrafts.draft),ordinary=structuredClone(doc.referenceDrafts.draft),accepted=structuredClone(doc.project),past=doc.history.past.length;
  await captureTopReportView('Historical review cannot receive this frame');
  assert.match(doc.error,/historical review has no editable candidate/);assert.deepEqual(doc.scopeReviewDrafts.draft,review);assert.deepEqual(doc.referenceDrafts.draft,ordinary);
  assert.deepEqual(doc.project,accepted);assert.equal(doc.history.past.length,past);assert.equal(count('reference_revision_accept'),0);
});

test('v2 actual workspace edits corrected source candidate, reviews atomically, and retains immutable parent',async()=>{
  await startV2Review();const before=structuredClone(doc.project),history=doc.history.past.length,proposal=structuredClone(doc.scopeReviewDrafts.draft.plan.proposed_scope);
  assert.equal(button('Save accepted scope',editableCandidate()),undefined,'candidate cannot create an intermediate accepted scope');
  await type(candidateInput('Input status'),'synthetic');await type(candidateInput('Declared value'),'3.8');
  await type(candidateInput('Source / citation'),'Original declared concept-stage equipment assumption');
  assert.equal(doc.scopeReviewDrafts.draft.candidate.value.assumptions[0].value,'3.8');assert.deepEqual(doc.scopeReviewDrafts.draft.plan.proposed_scope,proposal);
  assert.equal(button('Accept reviewed scope revision').disabled,true);assert.equal(count('reference_revision_accept'),0);
  await click('reports');await click('reference');assert.equal(candidateInput('Declared value').value,'3.8');
  await click('Proposed B');await click('reference');assert.equal(editableCandidate(),null);await click('Baseline A');assert.equal(candidateInput('Declared value').value,'3.8');
  await click('Review edited candidate in Rust');await finishV2Review();await click('Accept reviewed scope revision');
  assert.equal(count('reference_revision_accept'),1);assert.equal(doc.history.past.length,history+1);assert.equal(doc.scopeReviewDrafts.draft,null);
  assert.deepEqual(doc.project.architecture.scope_history.revisions.slice(0,2),before.architecture.scope_history.revisions);assert.equal(doc.project.architecture.scope.assumptions[0].value,'3.8');
  assert.deepEqual(doc.project.energy.designs,before.energy.designs);assert.equal(count('energy_start'),0);
});

test('v2 invalid candidate and cancelled late review keep edits and history without intermediate acceptance',async()=>{
  await startV2Review();const accepted=structuredClone(doc.project),past=doc.history.past.length;
  await type(candidateInput('Scope title'),'Reject this candidate');await click('Review edited candidate in Rust');assert.match(scopeHistoryPanel().textContent,/Synthetic invalid candidate/);
  assert.equal(candidateInput('Scope title').value,'Reject this candidate');assert.deepEqual(doc.project,accepted);assert.equal(doc.history.past.length,past);
  await type(candidateInput('Scope title'),'Reviewed replacement title');let finish;revisionPrepareGate=new Promise(r=>finish=r);
  await click('Review edited candidate in Rust');await click('Cancel review request');await act(async()=>finish());
  assert.equal(doc.scopeReviewDrafts.draft.plan.plan_sha256,referenceHash('7'));assert.equal(candidateInput('Scope title').value,'Reviewed replacement title');assert.equal(count('reference_revision_accept'),0);
  revisionPrepareGate=null;await click('Review edited candidate in Rust');await finishV2Review();
  await type(document.querySelector('[aria-label="Review rationale cop-changed"]'),'é'.repeat(1024)+'界');
  assert.equal(button('Accept reviewed scope revision').disabled,true);assert.match(scopeHistoryPanel().textContent,/-3 UTF-8 bytes remaining/);
  await click('Save');await click('Apply',dialog());assert.ok(dialog());assert.match(dialog().textContent,/2048 UTF-8 bytes/);assert.equal(doc.scopeReviewDrafts.draft.decisions['cop-changed'].rationale,'é'.repeat(1024)+'界');assert.deepEqual(doc.project,accepted);assert.equal(doc.history.past.length,past);await click('Cancel',dialog());
  await type(document.querySelector('[aria-label="Review rationale cop-changed"]'),'é'.repeat(1024));assert.equal(button('Accept reviewed scope revision').disabled,false);
});


test('v3 actual workspace shows exact candidate provenance, invalidates stale values, and refreshes source decisions',async()=>{
  await scopeReviewSetup();revisionPlanMode='v3';await click('Review changes and carry forward');
  const accepted=structuredClone(doc.project),past=doc.history.past.length;
  const preview=()=>document.querySelector('[aria-label="Reviewed candidate input provenance"]');
  assert.match(preview().textContent,/missing 1/);
  await type(candidateInput('Input status'),'assumed');await type(candidateInput('Declared value'),'3.8');await type(candidateInput('Source / citation'),'Candidate-only declared source');
  assert.match(preview().textContent,/changed since the last Rust review/);assert.equal(preview().querySelector('table'),null);
  assert.equal(button('Accept reviewed scope revision').disabled,true);
  await click('Review edited candidate in Rust');
  assert.match(preview().textContent,/Candidate-only declared source/);assert.match(preview().textContent,/missing 0/);
  const candidateRow=doc.scopeReviewDrafts.draft.plan.candidate_provenance.rows[0];
  assert.deepEqual(doc.scopeReviewDrafts.draft.plan.rows.find(r=>r.id==='cop-changed').after,candidateRow);
  await finishV2Review();await reviewDecision('calendar-input','unresolved','No calendar source declaration yet; retain disclosure.');
  await type(candidateInput('Source / citation'),'Revised candidate citation');await click('Review edited candidate in Rust');
  assert.equal(doc.scopeReviewDrafts.draft.decisions['cop-changed'],undefined);
  assert.equal(doc.scopeReviewDrafts.draft.decisions['assumption-changed'],undefined);
  assert.equal(doc.scopeReviewDrafts.draft.decisions['calendar-input'].resolution,'unresolved');
  await click('Remove cop-source');await click('Review edited candidate in Rust');
  assert.equal(doc.scopeReviewDrafts.draft.plan.candidate_provenance.rows[0].status,'unreviewed');
  assert.deepEqual(doc.scopeReviewDrafts.draft.plan.candidate_provenance.rows[0].source_declarations,[]);
  assert.match(preview().textContent,/No reviewed source declaration/);
  assert.deepEqual(doc.project,accepted);assert.equal(doc.history.past.length,past);assert.equal(count('reference_revision_accept'),0);
});

test('v3 explicit no-holidays form rejects contradicted calendar and accepts exact corrected declaration atomically',async()=>{
  await scopeReviewSetup();revisionPlanMode='v3';await click('Review changes and carry forward');
  const accepted=structuredClone(doc.project),past=doc.history.past.length;
  await type(candidateInput('Linked scope'),'calendar_no_holidays');await type(candidateInput('Input status'),'assumed');await type(candidateInput('Declared value'),'true');
  assert.equal(doc.scopeReviewDrafts.draft.candidate.value.assumptions[0].field,'/holidays/none');
  assert.match(editableCandidate().textContent,/not a generic array pointer/);
  await click('Review edited candidate in Rust');assert.match(scopeHistoryPanel().textContent,/contradicts retained calendar/);
  assert.equal(candidateInput('Declared value').value,'true');assert.deepEqual(doc.project,accepted);assert.equal(doc.history.past.length,past);
  await type(candidateInput('Declared value'),'false');await type(candidateInput('Source / citation'),'Original synthetic calendar lists one holiday');
  await click('Review edited candidate in Rust');
  const policy=doc.scopeReviewDrafts.draft.plan.candidate_provenance.rows[1];assert.equal(policy.applied_value,false);assert.equal(policy.status,'assumed');assert.equal(policy.source_declarations[0].value,'false');
  await finishV2Review();await reviewDecision('calendar-input','reviewed','The listed holiday is explicitly retained; no absence is claimed.');
  await click('Accept reviewed scope revision');assert.equal(doc.history.past.length,past+1);assert.equal(count('reference_revision_accept'),1);
  assert.deepEqual(doc.project.architecture.scope_history.revisions.slice(0,2),accepted.architecture.scope_history.revisions);
  assert.deepEqual(doc.project.energy.designs,accepted.energy.designs);assert.equal(doc.project.architecture.scope.schema_version,'greenlens.architectural-scope.v3');assert.equal(count('energy_start'),0);
});

test('v3 actual workspace admits a null unassigned study and keeps its missing provenance visible after candidate review',async()=>{
  await scopeReviewSetup();const candidate=structuredClone(doc.project.energy.designs.B);candidate.study=null;
  await act(()=>doc.commitEnergyTransaction({B:{candidate}},'explicit-test-preview','Unassign proposed study'));
  assert.equal(doc.project.energy.designs.B.study,null);const accepted=structuredClone(doc.project),past=doc.history.past.length;
  revisionPlanMode='v3';await click('Review changes and carry forward');
  const preview=()=>document.querySelector('[aria-label="Reviewed candidate input provenance"]');assert.ok(preview());assert.match(preview().textContent,/missing 2/);assert.match(preview().textContent,/B: study study/);
  const absent=doc.scopeReviewDrafts.draft.plan.candidate_provenance.rows.find(row=>row.object.kind==='study');assert.equal(absent.applied_value,null);assert.equal(absent.status,'unreviewed');
  await type(candidateInput('Input status'),'assumed');await type(candidateInput('Declared value'),'3.8');await type(candidateInput('Source / citation'),'Explicit synthetic COP source');await click('Review edited candidate in Rust');
  assert.match(preview().textContent,/missing 1/);assert.match(preview().textContent,/Documentation is incomplete/);assert.equal(doc.scopeReviewDrafts.draft.plan.candidate_provenance.documentation_complete,false);
  assert.equal(button('Accept reviewed scope revision').disabled,true,'missing review decisions still require explicit resolution');assert.deepEqual(doc.project,accepted);assert.equal(doc.history.past.length,past);assert.equal(count('reference_revision_accept'),0);assert.equal(count('energy_start'),0);
});
