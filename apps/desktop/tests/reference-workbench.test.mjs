// Real React forms with explicit synthetic service responses. These tests do not
// establish native dialogs, import integrity, geometry validity or engine acceptance.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {JSDOM} from 'jsdom';
import {transformWithOxc} from 'vite';
const dom=new JSDOM('<!doctype html><div id="root"></div>',{url:'https://greenlens.local/'});
for(const name of ['window','document','HTMLElement','HTMLInputElement','HTMLTextAreaElement','HTMLSelectElement','Event','MouseEvent'])globalThis[name]=dom.window[name];
globalThis.IS_REACT_ACT_ENVIRONMENT=true;
const React=await import('react'),{act}=React,{createRoot}=await import('react-dom/client');
const modules=new Map();
async function load(filename){
  if(modules.has(filename))return modules.get(filename);
  const task=(async()=>{const {code}=await transformWithOxc(fs.readFileSync(filename,'utf8'),filename,{jsx:{runtime:'automatic'}});let linked=code.replace(/import\s+['"][^'"]+\.css['"];?/g,'');
    for(const m of [...code.matchAll(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g)]){let target;
      if(m[3].startsWith('.')){let resolved=path.resolve(path.dirname(filename),m[3]);if(!path.extname(resolved))resolved=['.ts','.tsx','.mjs'].map(e=>resolved+e).find(fs.existsSync);target=await load(resolved);}
      else target=import.meta.resolve(m[3]);linked=linked.replace(m[0],`import ${m[1]} from ${JSON.stringify(target)};`);
    }return`data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`;})();modules.set(filename,task);return task;
}
const source=name=>fileURLToPath(new URL(`../src/reference/${name}`,import.meta.url));
const {default:Panel}=await import(await load(source('ReferenceWorkbench.tsx')));
const {newReferenceDraft,newScopeDraft,parseReferenceScopeDraft,stageReferenceReportView,isReferenceWorkbenchDraft}=await import(await load(source('reference-drafts.ts')));
const hash=c=>c.repeat(64),point=[0,0,0];
function fixture(){
  const reference={schema_version:'greenlens.reference-model.v1',importer_id:'greenlens.glb-static-triangles/1',source_sha256:hash('1'),revision_sha256:hash('2'),geometry_sha256:hash('3'),
    options:{scene_index:null,metres_per_source_unit:1,clockwise_rotation_deg:0,translation_m:point,declared_north_clockwise_deg:null,conversion:null},source_units:'metres (glTF 2.0)',coordinate_convention:'Right-handed X/Y/Z-up; north only when declared',selected_scene:0,source_node_count:2,source_mesh_count:1,source_material_count:0,source_texture_count:1,
    occurrences:[0,1].map(i=>({id:`original-occurrence-${i}`,node_index:i,parent_id:null,label:'Repeated original fixture',node_path:[i],source_mesh_index:0,world_from_mesh:[1,0,0,0,0,1,0,0,0,0,1,0,i,0,0,1],mirrored:i===1,bounds:{min_m:point,max_m:[1,1,1]},triangle_count:1,triangle_area_m2:0.5})),materials:[],chunks:[{path:'geometry/00000.bin',sha256:hash('4'),byte_length:176,triangle_count:2}],bounds:{min_m:point,max_m:[2,1,1]},triangle_count:2,triangle_area_m2:1,warnings:['Original fixture warning: display materials are not constructions.'],exclusions:['Embedded textures excluded from normalized display.'],storage_policy:'normalized-geometry-only; original architectural source and GLB excluded',resources:{elapsed_ms:8,source_bytes:800,json_bytes:608,normalized_bytes:176,decoded_allocation_estimate_bytes:2200}};
  const context={reference_options_sha256:hash('5'),designs:{A:{model_input_sha256:hash('a'),targets:[{object_id:'zone-1',kind:'zone',label:'Zone one'},{object_id:'window-1',kind:'window',label:'Window one'},{object_id:'shade-1',kind:'detached_shading',label:'Context shade'}]},B:{model_input_sha256:hash('b'),targets:[{object_id:'zone-1',kind:'zone',label:'Zone one proposed'}]}}};
  return{reference,context};
}
const assessment=()=>({schema_version:'greenlens.architectural-scope-assessment.v1',scope_sha256:hash('6'),report_binding_sha256:hash('7'),relationship:'current',stale_reasons:[],coverage_percent:null,reconciliations:[],unresolved_mapping_count:2,missing_assumption_count:0,disclosure:'Synthetic service response; no physical validation or whole-building equivalence.'});
let root,props,calls,reply,saveOkay,importOkay;
async function draw(){await act(async()=>root.render(React.createElement(Panel,props)));}
async function setup({accepted=false}={}){
  if(root)await act(async()=>root.unmount());document.body.innerHTML='<div id="root"></div>';root=createRoot(document.getElementById('root'));
  calls=[];reply=assessment();saveOkay=true;importOkay=true;const {reference,context}=fixture();
  const scope=newScopeDraft(reference,context).value;Object.assign(scope,{title:'Accepted original fixture',purpose:'Test form transactions only',included_scope:['Original selected source portion'],excluded_scope:['Whole-building equivalence not claimed'],boundary_consequences:'Declared analytical boundaries remain authoritative'});
  props={documentId:'original-document',reference,context,scope:accepted?scope:null,assessment:accepted?assessment():null,draft:null,selectedOccurrenceId:null,disabled:false,
    onDraftChange:value=>{props={...props,draft:value};root.render(React.createElement(Panel,props));},
    onConsumeDraft:(kind,consumed,owner)=>{if(props.documentId!==owner||props.draft?.document_id!==owner||JSON.stringify(props.draft[kind])!==consumed)return false;props.onDraftChange({...props.draft,[kind]:null});return true;},
    onSelectOccurrence:id=>{calls.push(['select',id]);props={...props,selectedOccurrenceId:id};root.render(React.createElement(Panel,props));},
    onImport:async value=>{calls.push(['import',structuredClone(value)]);return importOkay;},
    onAssess:async value=>{calls.push(['assess',structuredClone(value)]);if(reply instanceof Error)throw reply;return reply;},
    onSaveScope:async value=>{calls.push(['save',structuredClone(value)]);return saveOkay;},
    onClearScope:async()=>{calls.push(['clear']);return true;},
  };await draw();
}
const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent===text);
async function click(text){const el=button(text);assert.ok(el,`Button ${text}`);assert.equal(el.disabled,false,`${text} enabled`);await act(async()=>el.dispatchEvent(new MouseEvent('click',{bubbles:true})));}
function field(text){const label=[...document.querySelectorAll('label')].find(el=>[...el.childNodes].filter(n=>n.nodeType===3).map(n=>n.textContent).join('').trim()===text);assert.ok(label,`Label ${text}`);return label.querySelector('input,select,textarea');}
async function type(text,value){const el=field(text),type=el.tagName==='SELECT'?HTMLSelectElement:el.tagName==='TEXTAREA'?HTMLTextAreaElement:HTMLInputElement;await act(async()=>{Object.getOwnPropertyDescriptor(type.prototype,'value').set.call(el,value);el.dispatchEvent(new Event(el.tagName==='SELECT'?'change':'input',{bubbles:true}));});}
async function check(text){const el=field(text);assert.equal(el.type,'checkbox');await act(async()=>el.dispatchEvent(new MouseEvent('click',{bubbles:true})));}
async function startScope(){await click('Start scope for current revision');await type('Scope title','Draft representative portion');await type('Study purpose','Explicit workflow test');await type('Included scope (one item per line)','Selected zone\nExternal window');await type('Excluded scope (one item per line)','Other source spaces');await type('Boundary-condition consequences','Declared adiabatic approximation at the omitted adjacent space.');}
const count=name=>calls.filter(c=>c[0]===name).length;
function deferred(){let resolve;const promise=new Promise(r=>{resolve=r;});return{promise,resolve};}

test('intake is explicit, preserves incomplete numeric choices and sends only parsed typed options',async()=>{
  await setup();await click('Prepare another reference revision');await click('Choose local GLB and import');assert.equal(count('import'),0);assert.match(document.querySelector('[role="alert"]').textContent,/Review the explicit scale/);
  await check('I reviewed the explicit scale; source dimension checks must be recorded before relying on a mapping.');await check('I reviewed the axis, origin and orientation choices; unknown true north stays unknown.');
  await type('Metres per source unit','-');await click('Choose local GLB and import');assert.equal(count('import'),0);assert.equal(field('Metres per source unit').value,'-');
  await act(async()=>root.render(React.createElement('p',null,'Another panel')));await draw();assert.equal(field('Metres per source unit').value,'-');
  await type('Metres per source unit','0.001');await type('Model rotation (° clockwise)','90');await type('Translation X (m)','2.5');await type('Local conversion application','Original local fixture exporter');
  await click('Choose local GLB and import');assert.equal(count('import'),1);const options=calls.find(c=>c[0]==='import')[1];assert.deepEqual(options.translation_m,[2.5,0,0]);assert.equal(options.metres_per_source_unit,.001);assert.equal(options.clockwise_rotation_deg,90);assert.equal(options.declared_north_clockwise_deg,null);assert.equal(options.conversion.application,'Original local fixture exporter');assert.equal(props.draft.import,null);assert.equal(props.scope,null);
});

test('successful import consumes its exact draft after the accepted reference/context changes without reimporting it',async()=>{
  await setup();await startScope();const scopeBefore=structuredClone(props.draft.scope);await click('Prepare another reference revision');
  await check('I reviewed the explicit scale; source dimension checks must be recorded before relying on a mapping.');await check('I reviewed the axis, origin and orientation choices; unknown true north stays unknown.');
  const pending=deferred();props={...props,onImport:async options=>{calls.push(['import',options]);props={...props,reference:{...props.reference,revision_sha256:hash('8')},context:{...props.context,reference_options_sha256:hash('9')}};root.render(React.createElement(Panel,props));return pending.promise;}};await draw();
  await click('Choose local GLB and import');assert.equal(props.reference.revision_sha256,hash('8'));assert.ok(props.draft.import);await act(async()=>pending.resolve(true));
  assert.equal(props.draft.import,null);assert.deepEqual(props.draft.scope,scopeBefore);assert.equal(count('import'),1);assert.equal(button('Choose local GLB and import'),undefined);
});

test('import completion never consumes a newer intake edit or a different document draft',async()=>{
  for(const switchDocument of [false,true]){
    await setup();await click('Prepare another reference revision');await check('I reviewed the explicit scale; source dimension checks must be recorded before relying on a mapping.');await check('I reviewed the axis, origin and orientation choices; unknown true north stays unknown.');
    const pending=deferred();props={...props,onImport:async options=>{calls.push(['import',options]);return pending.promise;}};await draw();await click('Choose local GLB and import');
    const owner=switchDocument?'different-document':props.documentId;props={...props,documentId:owner,reference:{...props.reference,revision_sha256:hash('8')},draft:{...props.draft,document_id:owner,import:{...props.draft.import,note:'A later draft must survive'}}};await draw();await act(async()=>pending.resolve(true));
    assert.equal(props.draft.import.note,'A later draft must survive');assert.equal(props.draft.document_id,owner);assert.equal(count('import'),1);
  }
});

test('real mapping controls retain occurrence identity and bind explicit A/B targets without changing accepted scope',async()=>{
  await setup();await startScope();await type('Source occurrence','original-occurrence-1');await type('Mapping state','mapped');await type('Mapping rationale','Selected source occurrence corresponds to this explicit zone and window.');
  await check('A · Zone one · zone · zone-1');await check('A · Window one · window · window-1');await check('B · Zone one proposed · zone · zone-1');
  const map=props.draft.scope.value.mappings[0];assert.equal(map.occurrence_id,'original-occurrence-1');assert.equal(map.targets.length,3);assert.deepEqual(map.targets.map(t=>t.model_input_sha256),[hash('a'),hash('a'),hash('b')]);assert.equal(props.scope,null);assert.equal(count('save'),0);
  await type('Mapping state','context_only');assert.equal(props.draft.scope.value.mappings[0].targets.length,0);await check('A · Context shade · detached shading · shade-1');assert.equal(props.draft.scope.value.mappings[0].targets[0].object_id,'shade-1');
  await type('Mapping state','excluded');assert.deepEqual(props.draft.scope.value.mappings[0].targets,[]);assert.match(document.body.textContent,/mirrored occurrence/);
});

test('retained mapping controls page and search every historical ID without rebinding or editing the scope',async()=>{
  await setup({accepted:true});
  const historical=Array.from({length:205},(_,i)=>({occurrence_id:`historical-source-occurrence-${String(i).padStart(4,'0')}`,status:'excluded',targets:[],rationale:`Retained rationale ${i}`}));
  historical[204]={...historical[204],status:'mapped',targets:[{design_id:'A',object_id:'retained-old-zone',kind:'zone',model_input_sha256:hash('8')}]};
  props.scope.mappings=[...historical,{occurrence_id:'original-occurrence-0',status:'unresolved',targets:[],rationale:'Current source occurrence remains unresolved'}];
  props.reference.occurrences[0].label='Original repeated label with a deliberately long readable source description';
  await draw();const acceptedBefore=JSON.stringify(props.scope);await click('Edit accepted scope');const draftBefore=JSON.stringify(props.draft.scope);
  const rows=()=>[...document.querySelectorAll('caption')].find(c=>c.textContent.startsWith('Explicit source classifications')).closest('table').querySelectorAll('tbody tr');
  const shownIds=()=>[...rows()].map(row=>row.querySelector('button').title);
  const visited=[];
  assert.equal(button('Previous mappings').disabled,true);assert.equal(rows().length,100);visited.push(...shownIds());assert.match(document.body.textContent,/Showing 1–100 of 206 matching retained mappings/);
  await click('Next mappings');assert.equal(rows().length,100);visited.push(...shownIds());assert.match(document.body.textContent,/Showing 101–200 of 206 matching retained mappings/);
  await click('Next mappings');assert.equal(rows().length,6);visited.push(...shownIds());assert.equal(button('Next mappings').disabled,true);
  assert.deepEqual(visited,props.scope.mappings.map(m=>m.occurrence_id));assert.equal(new Set(visited).size,206);
  const lastHistorical=historical[204].occurrence_id;const selectButton=[...rows()].find(row=>row.querySelector('button').title===lastHistorical).querySelector('button');
  await act(async()=>selectButton.dispatchEvent(new MouseEvent('click',{bubbles:true})));
  assert.equal(props.selectedOccurrenceId,lastHistorical);assert.equal(field('Mapping rationale').value,'Retained rationale 204');assert.equal(field('Mapping state').value,'mapped');assert.match(document.querySelector('.reference-mapping').textContent,/absent from active revision/);assert.match(document.body.textContent,/A: retained-old-zone/);
  await type('Search retained mappings',lastHistorical);assert.equal(rows().length,1);assert.deepEqual(shownIds(),[lastHistorical]);assert.equal(rows()[0].querySelector('code').textContent,lastHistorical);assert.equal(button('Previous mappings').disabled,true);assert.equal(button('Next mappings').disabled,true);
  await type('Search retained mappings','retained-old-zone');assert.deepEqual(shownIds(),[lastHistorical]);
  await type('Search retained mappings','deliberately long readable');assert.deepEqual(shownIds(),['original-occurrence-0']);assert.match(rows()[0].textContent,/Original repeated label/);
  await type('Search retained mappings','no matching retained record');assert.equal(rows().length,0);assert.match(document.body.textContent,/Showing 0–0 of 0 matching retained mappings/);assert.equal(button('Previous mappings').disabled,true);assert.equal(button('Next mappings').disabled,true);
  await type('Search retained mappings','');assert.equal(rows().length,100);assert.equal(button('Previous mappings').disabled,true);await click('Next mappings');await click('Previous mappings');assert.equal(shownIds()[0],historical[0].occurrence_id);
  assert.equal(JSON.stringify(props.draft.scope),draftBefore);assert.equal(JSON.stringify(props.scope),acceptedBefore);assert.equal(count('assess'),0);assert.equal(count('save'),0);
});

test('narrow mapping controls wrap full IDs, retain real selection and show only exactly bound target labels',async()=>{
  await setup({accepted:true});
  const occurrenceId=`ref-${hash('c')}`,historicalId=`ref-${hash('d')}`,zoneId='original-zone-with-a-deliberately-long-stable-identifier';
  props.reference.occurrences[0]={...props.reference.occurrences[0],id:occurrenceId,label:'Original source occurrence with a long readable architectural description'};
  props.context.designs.A.targets[0]={...props.context.designs.A.targets[0],object_id:zoneId,label:'Original lower floor perimeter zone'};
  props.scope.mappings=[
    {occurrence_id:occurrenceId,status:'mapped',targets:[{design_id:'A',object_id:zoneId,kind:'zone',model_input_sha256:hash('a')}],rationale:'Reviewed original fixture mapping'},
    {occurrence_id:historicalId,status:'mapped',targets:[{design_id:'B',object_id:'zone-1',kind:'zone',model_input_sha256:hash('8')}],rationale:'Historical binding stays inspectable'},
  ];
  await draw();const acceptedBefore=JSON.stringify(props.scope);await click('Edit accepted scope');const draftBefore=JSON.stringify(props.draft.scope);
  const style=document.createElement('style');
  // Load the actual shared styles too: their nowrap buttons and sticky <th>
  // rules caused the installed-panel defect. jsdom verifies the cascade and
  // real control behavior; installed pixel/layout observation remains separate.
  style.textContent=fs.readFileSync(fileURLToPath(new URL('../src/styles.css',import.meta.url)),'utf8')+'\n'+fs.readFileSync(source('reference-workbench.css'),'utf8');document.head.append(style);
  try {
    const wrapper=document.querySelector('.reference-mapping-table');wrapper.style.width='260px';
    const table=wrapper.querySelector('table'),rows=table.querySelectorAll('tbody tr'),selectButton=rows[0].querySelector('button');
    assert.equal(window.getComputedStyle(table).tableLayout,'fixed');assert.equal(window.getComputedStyle(selectButton).whiteSpace,'normal');assert.equal(window.getComputedStyle(selectButton).maxWidth,'100%');
    assert.equal(window.getComputedStyle(rows[0].querySelector('th')).position,'static');assert.equal(window.getComputedStyle(table.querySelector('thead')).position,'sticky');assert.equal(window.getComputedStyle(table.querySelector('thead th')).position,'static');
    assert.equal(window.getComputedStyle(table.querySelector('caption')).textAlign,'left');assert.equal(table.querySelectorAll('colgroup col').length,3);
    assert.equal(rows[0].querySelector('th').scope,'row');assert.equal(rows[0].querySelector('code').textContent,occurrenceId);assert.equal(selectButton.title,occurrenceId);
    assert.match(rows[0].querySelector('td:last-child').textContent,/A: Original lower floor perimeter zone/);assert.equal(rows[0].querySelector('td:last-child code').textContent,zoneId);
    assert.equal(rows[1].querySelector('td:last-child').textContent,'B: zone-1');assert.doesNotMatch(rows[1].textContent,/Zone one proposed/);
    selectButton.focus();assert.equal(document.activeElement,selectButton);await act(async()=>selectButton.dispatchEvent(new MouseEvent('click',{bubbles:true})));
    assert.equal(props.selectedOccurrenceId,occurrenceId);assert.equal(selectButton.getAttribute('aria-pressed'),'true');assert.equal(field('Mapping rationale').value,'Reviewed original fixture mapping');
    const historicalButton=rows[1].querySelector('button');await act(async()=>historicalButton.dispatchEvent(new MouseEvent('click',{bubbles:true})));
    assert.equal(props.selectedOccurrenceId,historicalId);assert.equal(field('Mapping rationale').value,'Historical binding stays inspectable');assert.match(document.querySelector('.reference-mapping').textContent,/absent from active revision/);
    assert.equal(JSON.stringify(props.draft.scope),draftBefore);assert.equal(JSON.stringify(props.scope),acceptedBefore);assert.equal(count('assess'),0);assert.equal(count('save'),0);
  } finally { style.remove(); }
});

test('assumption status and source fields preserve missing versus declared values through section and panel navigation',async()=>{
  await setup();await startScope();await click('Add assumption');assert.equal(props.draft.scope.value.assumptions[0].value,null);
  await type('Input field','Location scenario');await type('Units','degrees');await type('Source / citation','Original synthetic concept-stage fixture');await type('Assumption rationale','No actual site has been established.');await type('Affected analysis scope','All explicitly modeled zones');await type('Input status','synthetic');await type('Declared value','Illustrative scenario only');
  await act(async()=>root.render(React.createElement('p',null,'Results')));await draw();assert.equal(field('Declared value').value,'Illustrative scenario only');assert.equal(props.draft.scope.value.assumptions[0].status,'synthetic');
  await type('Input status','not_applicable');assert.equal(props.draft.scope.value.assumptions[0].value,null);assert.equal([...document.querySelectorAll('input')].some(el=>el.value==='Illustrative scenario only'),false);assert.equal(count('save'),0);
});

test('Rust rejection keeps the complete draft; review is required and edits invalidate its save permission',async()=>{
  await setup();await startScope();const before=JSON.stringify(props.draft);reply=new Error('scope.included_scope: rejected synthetic service fixture');await click('Review scope in Rust');assert.equal(JSON.stringify(props.draft),before);assert.match(document.querySelector('[role="alert"]').textContent,/scope.included_scope/);assert.equal(button('Save accepted scope').disabled,true);assert.equal(count('save'),0);
  reply=assessment();await click('Review scope in Rust');assert.equal(button('Save accepted scope').disabled,false);await type('Scope title','Changed after review');assert.equal(button('Save accepted scope').disabled,true);
  await click('Review scope in Rust');saveOkay=false;await click('Save accepted scope');assert.equal(props.draft.scope.value.title,'Changed after review');assert.match(document.querySelector('[role="alert"]').textContent,/not accepted/);saveOkay=true;await click('Save accepted scope');assert.equal(props.draft.scope,null);
});

test('reimport preserves historical draft bindings and shows a stale assessment instead of silently rebinding',async()=>{
  await setup({accepted:true});await click('Edit accepted scope');const retained=JSON.stringify(props.draft.scope.value);
  props={...props,reference:{...props.reference,revision_sha256:hash('8')},assessment:{...assessment(),relationship:'stale',stale_reasons:[{field:'reference_revision',message:'Active source revision changed'}]}};await draw();assert.equal(JSON.stringify(props.draft.scope.value),retained);assert.match(document.body.textContent,/saving must not silently rebind/);assert.match(document.body.textContent,/Active source revision changed/);
  await click('Start scope for current revision');await click('Cancel — keep editing');assert.equal(JSON.stringify(props.draft.scope.value),retained);await click('Start scope for current revision');await click('Start new scope draft');assert.equal(props.draft.scope.value.reference_revision_sha256,hash('8'));assert.equal(props.scope.reference_revision_sha256,hash('2'));assert.equal(props.draft.scope.value.mappings.length,0);
});

test('source reconciliation retains exact numeric text and displays only Rust-returned analytical values',async()=>{
  await setup();await startScope();await type('Source occurrence','original-occurrence-0');await click('Add measurement for selected occurrence');await check('Zone one · zone-1');await check('A source measurement is available');await type('Source value (m²)','-');await click('Review scope in Rust');assert.equal(count('assess'),0);assert.equal(field('Source value (m²)').value,'-');
  await type('Source value (m²)','23.75');await type('Measurement source','Original synthetic measurement record');await type('Source measurement method','Declared test number');await type('Selected scope / abstraction method','Compare explicitly selected source and analytical floor areas.');
  reply={...assessment(),reconciliations:[{id:'measure-1',design_id:'A',unit:'square_metres',source_value:23.75,analytical_value:24,analytical_minus_source:.25,relative_difference_percent:1.0526315789473684}]};await click('Review scope in Rust');
  const sent=calls.find(c=>c[0]==='assess')[1];assert.equal(sent.reconciliations[0].source_measurement.value,23.75);assert.deepEqual(sent.reconciliations[0].analytical_object_ids,['zone-1']);assert.match(document.querySelector('[aria-label="Rust scope preview"]').textContent,/1\.0526315789473684/);assert.match(document.body.textContent,/Coverage: Unknown/);
});

test('changing reconciliation design preserves the building target for north rotation and clears zone-specific targets',async()=>{
  await setup();await startScope();await type('Source occurrence','original-occurrence-0');await click('Add measurement for selected occurrence');await type('Measurement','north_rotation');await type('Design','B');await type('Selected scope / abstraction method','Compare declared orientation for the selected design.');
  assert.deepEqual(props.draft.scope.value.reconciliations[0].analytical_object_ids,['model:building']);assert.equal(props.draft.scope.value.reconciliations[0].design_id,'B');
  await click('Review scope in Rust');let sent=calls.filter(c=>c[0]==='assess').at(-1)[1];assert.equal(sent.reconciliations[0].measure,'north_rotation');assert.equal(sent.reconciliations[0].design_id,'B');assert.deepEqual(sent.reconciliations[0].analytical_object_ids,['model:building']);assert.equal(props.scope,null);assert.equal(count('save'),0);
  await type('Measurement','floor_area');await check('Zone one proposed · zone-1');assert.deepEqual(props.draft.scope.value.reconciliations[0].analytical_object_ids,['zone-1']);await type('Design','A');assert.deepEqual(props.draft.scope.value.reconciliations[0].analytical_object_ids,[]);assert.equal(button('Save accepted scope').disabled,true);
  await check('Zone one · zone-1');await click('Review scope in Rust');sent=calls.filter(c=>c[0]==='assess').at(-1)[1];assert.equal(sent.reconciliations[0].design_id,'A');assert.deepEqual(sent.reconciliations[0].analytical_object_ids,['zone-1']);assert.equal(count('save'),0);
});

test('whole-building claims remain explicit and malformed coverage never reaches the accepted action',async()=>{
  await setup();await startScope();await type('Scope classification','whole_building');assert.match(document.body.textContent,/Rust rejects a whole-building claim/);await type('Coverage basis','declared_floor_area');await type('Included source floor area (m²)','100');await type('Whole-building source floor area (m²)','');await click('Review scope in Rust');assert.equal(count('assess'),0);assert.equal(props.draft.scope.numbers['coverage.whole'],'');assert.equal(props.scope,null);
  await type('Whole-building source floor area (m²)','100');await type('Coverage source','Original measured synthetic fixture');await type('Coverage measurement method','Explicit reviewed source scope');await type('Whole-building review evidence','Test only, no equivalence claim');reply=new Error('Whole-building review has no source mapping for zone');await click('Review scope in Rust');assert.equal(count('assess'),1);assert.equal(count('save'),0);assert.match(document.querySelector('[role="alert"]').textContent,/no source mapping/);
});

test('discard and clear require distinct confirmations and preserve the other draft category',async()=>{
  await setup({accepted:true});await click('Edit accepted scope');await type('Scope title','Unapplied edit');await click('Prepare another reference revision');await type('Metres per source unit','0.01');await click('Discard scope draft…');await click('Cancel — keep editing');assert.equal(props.draft.scope.value.title,'Unapplied edit');
  await click('Discard scope draft…');await click('Discard scope draft');assert.equal(props.draft.scope,null);assert.equal(props.draft.import.scale,'0.01');assert.equal(props.scope.title,'Accepted original fixture');
  await click('Clear accepted scope…');await click('Cancel — keep editing');assert.equal(count('clear'),0);await click('Clear accepted scope…');await click('Clear scope');assert.equal(count('clear'),1);assert.equal(props.draft.import.scale,'0.01');
});

test('scope confirmation reveals and focuses safe Cancel, preserves invalid drafts and returns focus only on cancellation',async()=>{
  await setup({accepted:true});await click('Edit accepted scope');await type('Scope title','Retained unapplied edit');
  await type('Coverage basis','declared_floor_area');await type('Included source floor area (m²)','-');
  await click('Prepare another reference revision');await type('Metres per source unit','0.01');
  const draftBefore=JSON.stringify(props.draft),acceptedBefore=JSON.stringify(props.scope),scrolls=[];
  const original=Object.getOwnPropertyDescriptor(HTMLElement.prototype,'scrollIntoView');
  // jsdom has no layout. Observe the real component's explicit scroll request;
  // an installed long/scrolled inspector is still required for pixel evidence.
  Object.defineProperty(HTMLElement.prototype,'scrollIntoView',{configurable:true,value:function(options){scrolls.push({element:this,options});}});
  let escapedOutside=0;const onEscape=e=>{if(e.key==='Escape')escapedOutside++;};document.body.addEventListener('keydown',onEscape);
  try {
    const trigger=button('Discard scope draft…');trigger.focus();await click('Discard scope draft…');
    const prompt=document.querySelector('[role="alertdialog"]'),cancel=button('Cancel — keep editing');
    assert.equal(document.activeElement,cancel);assert.equal(prompt.isConnected,true);
    assert.deepEqual(scrolls,[{element:prompt,options:{block:'nearest'}}]);
    assert.equal(document.querySelector('.reference-scope-editor').disabled,true);
    assert.equal(JSON.stringify(props.draft),draftBefore);assert.equal(JSON.stringify(props.scope),acceptedBefore);
    const escape=new window.KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true});
    await act(async()=>cancel.dispatchEvent(escape));
    assert.equal(escape.defaultPrevented,true);assert.equal(escapedOutside,0);assert.equal(document.querySelector('[role="alertdialog"]'),null);
    assert.equal(document.activeElement,trigger);assert.deepEqual(scrolls.at(-1),{element:trigger,options:{block:'nearest'}});
    assert.equal(document.querySelector('.reference-scope-editor').disabled,false);
    assert.equal(JSON.stringify(props.draft),draftBefore);assert.equal(JSON.stringify(props.scope),acceptedBefore);
    for(const text of ['Clear accepted scope…','Start scope for current revision']){
      scrolls.length=0;const invoker=button(text);invoker.focus();await click(text);
      const current=document.querySelector('[role="alertdialog"]');assert.equal(document.activeElement,button('Cancel — keep editing'));
      assert.deepEqual(scrolls,[{element:current,options:{block:'nearest'}}]);await click('Cancel — keep editing');
      assert.equal(document.activeElement,invoker);assert.equal(JSON.stringify(props.draft),draftBefore);assert.equal(JSON.stringify(props.scope),acceptedBefore);
    }
    assert.equal(count('assess'),0);assert.equal(count('save'),0);assert.equal(count('clear'),0);
    await click('Discard scope draft…');await click('Discard scope draft');
    assert.equal(props.draft.scope,null);assert.equal(props.draft.import.scale,'0.01');assert.equal(JSON.stringify(props.scope),acceptedBefore);
    assert.notEqual(document.activeElement,trigger);assert.equal(trigger.isConnected,false);
  } finally {
    document.body.removeEventListener('keydown',onEscape);
    if(original)Object.defineProperty(HTMLElement.prototype,'scrollIntoView',original);else delete HTMLElement.prototype.scrollIntoView;
  }
});

test('scope confirmation never restores an old document or disconnected trigger and does not steal focus on unmount',async()=>{
  await setup({accepted:true});await click('Edit accepted scope');
  const scrolls=[],original=Object.getOwnPropertyDescriptor(HTMLElement.prototype,'scrollIntoView');
  Object.defineProperty(HTMLElement.prototype,'scrollIntoView',{configurable:true,value:function(options){scrolls.push({element:this,options});}});
  const outside=document.createElement('button');outside.textContent='Outside current panel';document.body.append(outside);
  try {
    const oldTrigger=button('Discard scope draft…');oldTrigger.focus();await click('Discard scope draft…');
    outside.focus();scrolls.length=0;
    const replacement=newReferenceDraft('replacement-document');replacement.scope=newScopeDraft(props.reference,props.context);
    props={...props,documentId:'replacement-document',draft:replacement};await draw();
    assert.equal(document.querySelector('[role="alertdialog"]'),null);assert.equal(document.activeElement,outside);assert.deepEqual(scrolls,[]);
    await click('Discard scope draft…');const disconnected=button('Discard scope draft…');
    await act(async()=>props.onDraftChange({...props.draft,scope:null}));assert.equal(disconnected.isConnected,false);
    const refocuses=[];disconnected.focus=()=>refocuses.push('stale trigger');scrolls.length=0;
    await click('Cancel — keep editing');assert.deepEqual(refocuses,[]);assert.deepEqual(scrolls,[]);
    await click('Clear accepted scope…');outside.focus();scrolls.length=0;
    await act(async()=>root.render(React.createElement('p',null,'Another panel')));
    assert.equal(document.activeElement,outside);assert.deepEqual(scrolls,[]);assert.equal(count('clear'),0);assert.equal(count('save'),0);
  } finally {
    outside.remove();if(original)Object.defineProperty(HTMLElement.prototype,'scrollIntoView',original);else delete HTMLElement.prototype.scrollIntoView;
  }
});

test('reference confirmation and validation messages retain readable foreground/background contrast in the actual style cascade',async()=>{
  await setup({accepted:true});await click('Clear accepted scope…');
  const style=document.createElement('style');style.textContent=
    fs.readFileSync(fileURLToPath(new URL('../src/styles.css',import.meta.url)),'utf8')+'\n'+
    fs.readFileSync(fileURLToPath(new URL('../src/energy/energy.css',import.meta.url)),'utf8')+'\n'+
    fs.readFileSync(source('reference-workbench.css'),'utf8');document.head.append(style);
  document.getElementById('root').className='energy-workspace';
  const luminance=color=>{const rgb=color.match(/[\d.]+/g).slice(0,3).map(Number).map(v=>v/255).map(v=>v<=.04045?v/12.92:((v+.055)/1.055)**2.4);return .2126*rgb[0]+.7152*rgb[1]+.0722*rgb[2];};
  const assertContrast=element=>{const css=window.getComputedStyle(element),a=luminance(css.color),b=luminance(css.backgroundColor);assert.ok((Math.max(a,b)+.05)/(Math.min(a,b)+.05)>=4.5,'Reference message text must remain readable against its own background');};
  try {
    assertContrast(document.querySelector('[role="alertdialog"]'));
    await click('Cancel — keep editing');await click('Edit accepted scope');reply=new Error('Original fixture validation error');await click('Review scope in Rust');
    assertContrast(document.querySelector('[role="alert"]'));assert.equal(count('save'),0);
  } finally {style.remove();document.getElementById('root').className='';}
});

test('late review cannot enable acceptance for a different document or overwritten draft',async()=>{
  await setup();await startScope();const pending=deferred();reply=pending.promise;await click('Review scope in Rust');assert.equal(button('Review scope in Rust').closest('fieldset').disabled,true);
  const replacement=newReferenceDraft('other-document');replacement.scope=newScopeDraft(props.reference,props.context);replacement.scope.value.title='Other document draft';props={...props,documentId:'other-document',draft:replacement};await draw();await act(async()=>pending.resolve(assessment()));assert.equal(button('Save accepted scope').disabled,true);assert.equal(document.querySelector('[aria-label="Rust scope preview"]'),null);assert.equal(props.draft.scope.value.title,'Other document draft');assert.equal(count('save'),0);
});

test('import receipt exposes exclusions, unknown orientation and resource scope without claiming equivalence',async()=>{
  await setup();assert.match(document.body.textContent,/Embedded textures excluded/);assert.match(document.body.textContent,/estimated allocation bytes \(not measured process memory\)/);assert.match(document.body.textContent,/not a complete architectural-source archive/);assert.match(document.body.textContent,/Bounds are not a measured floor area/);assert.equal(props.reference.options.declared_north_clockwise_deg,null);
  let cancelled=0;props={...props,importProgress:{stage:'Admitting triangles',completed:4,total:10,processed_bytes:256},onCancelImport:()=>cancelled++};await draw();await click('Cancel reference import');assert.equal(cancelled,1);assert.equal(button('Prepare another reference revision').disabled,true);
});

test('pure draft parsing copies accepted snapshots and leaves their hashes and numeric draft text unchanged',()=>{
  const {reference,context}=fixture(),draft=newScopeDraft(reference,context);draft.value.coverage={status:'declared_floor_area',included_m2:0,whole_building_m2:0,source:'Original fixture',method:'Declared'};draft.numbers={'coverage.included':'24.125','coverage.whole':'100.5'};
  const before=JSON.stringify(draft),candidate=parseReferenceScopeDraft(draft);assert.equal(candidate.coverage.included_m2,24.125);assert.equal(candidate.coverage.whole_building_m2,100.5);assert.equal(JSON.stringify(draft),before);assert.equal(candidate.reference_revision_sha256,reference.revision_sha256);assert.deepEqual(candidate.design_model_sha256,{A:hash('a'),B:hash('b')});
});


const reportCapture=()=>({title:'Original synthetic portable section',eye_m:[0,-10,2],target_m:[0,0,2],up:[0,0,1],vertical_span_m:8,aspect_ratio:4/3,section:{axis:'z',offset_m:2.5,keep:'below'}});
test('portable report staging preserves existing draft text and requires explicit Rust review/save',async()=>{
  await setup({accepted:true});const acceptedBytes=JSON.stringify(props.scope);await click('Edit accepted scope');await type('Scope title','Existing unapplied title');props.draft.scope.numbers['preserved.invalid.numeric']='-';
  await act(async()=>props.onDraftChange(stageReferenceReportView(props.documentId,props.draft,props.scope,props.reference,props.context,'A',reportCapture())));
  assert.equal(field('Scope title').value,'Existing unapplied title');assert.equal(props.draft.scope.numbers['preserved.invalid.numeric'],'-');assert.equal(JSON.stringify(props.scope),acceptedBytes);assert.equal(count('save'),0);assert.equal(button('Save accepted scope').disabled,true);assert.match(document.body.textContent,/Portable report figure draft \(1\/8\)/);assert.equal(isReferenceWorkbenchDraft(props.draft,props.documentId),true);
  await type('Report figure title','Revised retained section');await click('Review scope in Rust');await click('Save accepted scope');const saved=calls.find(c=>c[0]==='save')[1];assert.equal(saved.report_views[0].title,'Revised retained section');assert.equal(saved.report_views[0].model_input_sha256,hash('a'));assert.equal(saved.report_views[0].reference_revision_sha256,props.reference.revision_sha256);assert.equal(saved.title,'Existing unapplied title');assert.equal(JSON.stringify(props.scope),acceptedBytes);
});
test('report-view draft deletion, pending import, stale bindings and finite bounds preserve accepted data',async()=>{
  await setup({accepted:true});const before=JSON.stringify(props.scope);let staged=stageReferenceReportView(props.documentId,null,props.scope,props.reference,props.context,'A',reportCapture());assert.equal(props.draft,null);assert.equal(JSON.stringify(props.scope),before);
  await act(async()=>props.onDraftChange(staged));await click('Remove report view view-1');assert.deepEqual(props.draft.scope.value.report_views,[]);assert.equal(JSON.stringify(props.scope),before);
  const importDraft={...staged,import:{pending:true}};assert.throws(()=>stageReferenceReportView(props.documentId,importDraft,props.scope,props.reference,props.context,'A',reportCapture()),/pending reference import/);
  const wrong=structuredClone(staged);wrong.scope.value.reference_revision_sha256=hash('9');const wrongBytes=JSON.stringify(wrong);assert.throws(()=>stageReferenceReportView(props.documentId,wrong,props.scope,props.reference,props.context,'A',reportCapture()),/another reference/);assert.equal(JSON.stringify(wrong),wrongBytes);
  staged.scope.value.report_views=Array.from({length:8},(_,i)=>({...staged.scope.value.report_views[0],id:`view-${i+1}`}));assert.throws(()=>stageReferenceReportView(props.documentId,staged,props.scope,props.reference,props.context,'A',reportCapture()),/eight portable/);
  staged.scope.value.report_views[0].eye_m[0]=Infinity;assert.equal(isReferenceWorkbenchDraft(staged,props.documentId),false);
});

test.after(async()=>{if(root)await act(async()=>root.unmount());});
