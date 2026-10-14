// Actual ReactDOM events and the existing durable draft store. No engine or native acceptance claim.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {JSDOM} from 'jsdom';
import {transformWithOxc} from 'vite';
const dom=new JSDOM('<!doctype html><div id="root"></div>',{url:'https://greenlens.local/'});
for(const name of ['window','document','HTMLElement','HTMLInputElement','HTMLSelectElement','Event','MouseEvent'])globalThis[name]=dom.window[name];
globalThis.IS_REACT_ACT_ENVIRONMENT=true;
const React=await import('react'),{act}=React,{createRoot}=await import('react-dom/client');
const cache=new Map();
async function load(filename){if(cache.has(filename))return cache.get(filename);const task=(async()=>{const {code}=await transformWithOxc(fs.readFileSync(filename,'utf8'),filename,{jsx:{runtime:'automatic'}});let linked=code;for(const m of [...code.matchAll(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g)]){const spec=m[3];let target;if(spec.startsWith('.')){let resolved=path.resolve(path.dirname(filename),spec);if(!path.extname(resolved))resolved=['.ts','.tsx'].map(e=>resolved+e).find(fs.existsSync);target=await load(resolved);}else target=import.meta.resolve(spec);linked=linked.replace(m[0],`import ${m[1]} from ${JSON.stringify(target)};`);}return`data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`;})();cache.set(filename,task);return task;}
const src=fileURLToPath(new URL('../src/energy/',import.meta.url));
const {default:Envelope,HvacResourceEditors:Hvac,GainsResourceFields:Gains}=await import(await load(path.join(src,'ThermalResourceEditors.tsx')));
const {EnergyDraftStore,draftCandidate}=await import(await load(path.join(src,'drafts.ts')));
const historical=JSON.parse(fs.readFileSync(new URL('../../../examples/energy/revision1-inputs.json',import.meta.url)));
let root,accepted,store,alternative,show,mode;
const fixture=()=>Object.fromEntries(['A','B'].map(id=>[id,{model:structuredClone(historical.cases[id].model),study:structuredClone(historical.cases[id].study)}]));
function Harness(){const [design,setDesign]=React.useState('A'),[visible,setVisible]=React.useState(true),[,refresh]=React.useState(0);alternative=design;show=setVisible;const entry=store.get('test-resources',design,accepted[design]),onChange=change=>{store.set('test-resources',design,change(store.get('test-resources',design,accepted[design])));refresh(v=>v+1);};return React.createElement(React.Fragment,null,React.createElement('button',{onClick:()=>setDesign(d=>d==='A'?'B':'A')},'Switch test design'),visible&&React.createElement(mode==='envelope'?Envelope:mode==='hvac'?Hvac:Gains,{entry,onChange,zoneId:entry.structured.model.zones[0].id}));}
async function setup(kind='envelope',mutate=()=>{}){if(root)await act(()=>root.unmount());accepted=fixture();mutate(accepted);mode=kind;window.localStorage.clear();store=new EnergyDraftStore(window.localStorage);root=createRoot(document.getElementById('root'));await act(()=>root.render(React.createElement(Harness)));}
const draft=()=>store.get('test-resources',alternative,accepted[alternative]);
function input(label){const el=[...document.querySelectorAll('input,select')].find(v=>v.getAttribute('aria-label')===label||v.closest('label')?.firstChild?.textContent===label);assert.ok(el,`input ${label}`);return el;}
function button(label){const el=[...document.querySelectorAll('button')].find(v=>v.textContent===label||v.getAttribute('aria-label')===label);assert.ok(el,`button ${label}`);return el;}
async function click(label){await act(()=>button(label).dispatchEvent(new MouseEvent('click',{bubbles:true})));}
async function enter(label,text){const el=input(label),prototype=el.tagName==='SELECT'?HTMLSelectElement.prototype:HTMLInputElement.prototype;await act(()=>{Object.getOwnPropertyDescriptor(prototype,'value').set.call(el,text);el.dispatchEvent(new Event(el.tagName==='SELECT'?'change':'input',{bubbles:true}));});}
async function toggle(label){await act(()=>input(label).dispatchEvent(new MouseEvent('click',{bubbles:true})));}

test('numeric/source forms preserve incomplete text through unrelated edits, design switches, unmount and recovery without accepting inputs',async()=>{
  await setup();const original=structuredClone(accepted),id=accepted.A.model.materials[0].id,label=`${id} · Density (kg/m³)`,key=`resource:material:${id}:density_kg_m3`;
  await enter(label,'1e');assert.equal(draft().fields[key],'1e');assert.equal(input(label).getAttribute('aria-invalid'),'true');assert.equal(draft().structured.model.materials[0].density_kg_m3,original.A.model.materials[0].density_kg_m3);
  await enter(`${id} source citation`,'Exact reviewed citation retained as a draft');await toggle(`${id} source synthetic`);assert.equal(draft().fields[key],'1e');assert.throws(()=>draftCandidate(draft(),accepted.A),/finite numeric/);
  await click('Switch test design');await click('Switch test design');assert.equal(input(label).value,'1e');await act(()=>show(false));await act(()=>show(true));assert.equal(input(label).value,'1e');
  store=new EnergyDraftStore(window.localStorage);await act(()=>root.render(React.createElement(Harness)));assert.equal(input(label).value,'1e');assert.match(draft().error,/Recovered unsaved/);
  await enter(label,'945.25');const candidate=draftCandidate(draft(),accepted.A);assert.equal(candidate.model.materials[0].density_kg_m3,945.25);assert.equal(candidate.model.materials[0].source.citation,'Exact reviewed citation retained as a draft');assert.equal(candidate.model.materials[0].source.synthetic,false);assert.deepEqual(accepted,original);assert.equal(document.querySelector('textarea'),null);
});
test('material and assembly creation/duplication keep stable unique IDs and explicit ordered shared layers',async()=>{
  await setup();const original=structuredClone(accepted),material=accepted.A.model.materials[0],construction=accepted.A.model.constructions[0];
  assert.match(document.body.textContent,/Shared-use impact/);assert.match(document.body.textContent,new RegExp(construction.id));
  await click(`Duplicate material ${material.id}`);const copy=draft().structured.model.materials.at(-1);assert.notEqual(copy.id,material.id);assert.deepEqual({...copy,id:material.id,name:material.name},material);
  await click(`Duplicate assembly ${construction.id}`);const assembly=draft().structured.model.constructions.at(-1),assemblyId=assembly.id;assert.notEqual(assemblyId,construction.id);
  await enter(`${assemblyId} layer 1`,copy.id);await click(`Append layer to ${assemblyId}`);const before=[...draft().structured.model.constructions.at(-1).layers];await click(`${assemblyId} layer 1 inward`);const expected=[...before];[expected[0],expected[1]]=[expected[1],expected[0]];assert.deepEqual(draft().structured.model.constructions.at(-1).layers,expected);await click(`${assemblyId} remove layer 1`);assert.deepEqual(draft().structured.model.constructions.at(-1).layers,expected.slice(1));
  await click('Add synthetic material');await click('Add assembly using first material');assert.equal(draft().structured.model.materials.at(-1).source.synthetic,true);const all=draft().structured.model;assert.equal(new Set([...all.materials,...all.constructions].map(v=>v.id)).size,all.materials.length+all.constructions.length);assert.deepEqual(all.constructions.find(v=>v.id===construction.id),construction);assert.deepEqual(accepted,original);
});
test('new resource IDs avoid IDs in other actual resource namespaces',async()=>{
  await setup('envelope',values=>{values.A.model.constructions.push({...structuredClone(values.A.model.constructions[0]),id:'material_1'});});await click('Add synthetic material');assert.equal(draft().structured.model.materials.at(-1).id,'material_2');
  await setup('hvac',values=>{values.A.model.schedules.push({...structuredClone(values.A.model.schedules[0]),id:'curve_1'});});await click('Add synthetic quadratic curve');assert.equal(draft().structured.model.curves.at(-1).id,'curve_2');
});
test('glazing basis, sources and creation are explicit; the actual 64-entry Rust bound is respected',async()=>{
  await setup();const original=structuredClone(accepted),glazing=accepted.A.model.glazing[0];await click(`Duplicate glazing ${glazing.id}`);const id=draft().structured.model.glazing.at(-1).id;await enter(`${id} basis`,'glass_without_frame');await enter(`${id} · U-value (W/m²K)`,'1.85');await enter(`${id} source citation`,'Original supplier basis, draft only');assert.equal(draft().structured.model.glazing.at(-1).basis,'glass_without_frame');assert.equal(draft().structured.model.glazing.at(-1).u_value_w_m2k,1.85);assert.deepEqual(accepted,original);await click('Add synthetic glazing');assert.equal(draft().structured.model.glazing.at(-1).source.synthetic,true);
  await setup('envelope',values=>{const first=values.A.model.glazing[0];values.A.model.glazing=Array.from({length:64},(_,i)=>({...structuredClone(first),id:`g${i}`}));});assert.equal(button('Add synthetic glazing').disabled,true);assert.equal(button('Duplicate glazing g0').disabled,true);
});
test('HVAC fields and complete polynomial domains remain explicit drafts with reviewed assignment and no reshaping of old curves',async()=>{
  await setup('hvac');const original=structuredClone(accepted),zone=accepted.A.model.zones[0],curve=accepted.A.model.curves.find(v=>v.form.kind==='biquadratic');
  await enter('Rated indoor wet-bulb (°C)','19.5');await enter('Rated outdoor dry-bulb (°C)','35.1');await enter('Thermostat cutout difference (K)','0');await enter(`${zone.id} HVAC sizing citation`,'Explicit draft sizing source');
  await enter(`${curve.id} coefficient c5`,'0.0125');await enter(`${curve.id} x minimum`,'12.22');await enter(`${curve.id} y maximum (°C)`,'51.67');await enter(`${curve.id} output minimum`,'0.7');await enter(`${curve.id} source citation`,'Domain source, applicability not asserted');
  const changed=draft().structured.model.curves.find(v=>v.id===curve.id);assert.equal(changed.form.coefficients[5],.0125);assert.equal(changed.form.x[0],12.22);assert.equal(changed.form.y[1],51.67);assert.equal(changed.output_limits[0],.7);
  await click(`Duplicate curve ${curve.id}`);const id=draft().structured.model.curves.at(-1).id;await enter(`${zone.id} capacity_temperature_curve`,id);assert.equal(draft().structured.model.zones[0].cooling_system.capacity_temperature_curve,id);assert.equal(draft().structured.model.zones[1].cooling_system.capacity_temperature_curve,original.A.model.zones[1].cooling_system.capacity_temperature_curve);
  await enter(`${id} coefficient c0`,'-');assert.throws(()=>draftCandidate(draft(),accepted.A),/finite numeric/);await click('Add synthetic cubic curve');assert.equal(input(`${id} coefficient c0`).value,'-');assert.equal(draft().structured.model.curves.at(-1).form.coefficients.length,4);assert.equal(draft().structured.model.curves.at(-1).source.synthetic,true);assert.deepEqual(accepted,original);
});
test('gain fractions and infiltration basis preserve declared numerical input and disclose unit changes',async()=>{
  await setup('gains');const original=structuredClone(accepted),zone=accepted.A.model.zones[0],fields=['people_sensible_fraction','people_radiant_fraction_of_sensible','lighting_radiant_fraction','lighting_visible_fraction','lighting_return_air_fraction','equipment_radiant_fraction','equipment_latent_fraction','equipment_lost_fraction'];
  for(let i=0;i<fields.length;i++)await enter(`${fields[i].replaceAll('_',' ')} (0–1)`,String((i+1)/20));await enter('Infiltration basis','flow_m3_per_second');await enter(`${zone.id} gains citation`,'Declared sensible and latent source');
  const gains=draft().structured.model.zones[0].gains;for(let i=0;i<fields.length;i++)assert.equal(gains[fields[i]],(i+1)/20);assert.equal(gains.infiltration.basis,'flow_m3_per_second');assert.equal(gains.infiltration.value,zone.gains.infiltration.value);assert.match(document.body.textContent,/No unit conversion is inferred/);assert.deepEqual(accepted,original);
  assert.match(document.body.textContent,/m³\/\(s·m²\)/);
});
test('explicit synthetic DX creation binds all roles, holidays and unique IDs without changing accepted inputs or another zone/design',async()=>{
  await setup('hvac',values=>{values.A.model.zones[0].cooling_system=null;values.A.model.materials.push({...structuredClone(values.A.model.materials[0]),id:'dx_system_1'});values.A.model.curves.push({...structuredClone(values.A.model.curves[0]),id:'dx_availability_1'});});
  const original=structuredClone(accepted),before=structuredClone(draft().structured.model);assert.match(document.body.textContent,/unqualified synthetic assumptions/);
  await click('Add explicit synthetic DX system');const model=draftCandidate(draft(),accepted.A).model,h=model.zones[0].cooling_system;
  assert.ok(h);assert.equal(h.id,'dx_system_2');assert.equal(h.availability_schedule,'dx_availability_2');assert.equal(h.gross_rated_capacity_w,5000);assert.equal(h.gross_rated_cop,3);assert.equal(h.rated_airflow_m3_s,.3);assert.equal(h.sizing_source.synthetic,true);assert.match(h.sizing_source.citation,/synthetic draft/);
  assert.equal(model.curves.length,before.curves.length+5);assert.equal(model.schedules.length,before.schedules.length+2);
  const roles=['capacity_temperature_curve','capacity_flow_curve','eir_temperature_curve','eir_flow_curve','part_load_fraction_curve'];assert.equal(new Set(roles.map(role=>h[role])).size,5);
  for(const role of roles){const curve=model.curves.find(c=>c.id===h[role]);assert.equal(curve.source.synthetic,true);assert.equal(curve.form.coefficients[0],1);assert.ok(curve.form.coefficients.slice(1).every(v=>v===0));assert.deepEqual(curve.output_limits,[.1,2]);}
  for(const [id,value,kind] of [[h.availability_schedule,1,'fraction'],[h.cooling_setpoint_schedule,24,'temperature_celsius']]){const schedule=model.schedules.find(s=>s.id===id);assert.equal(schedule.kind,kind);for(const day of ['weekday','saturday','sunday'])assert.deepEqual(schedule[day].values,Array(144).fill(value));assert.deepEqual(schedule.holidays.map(v=>v.date),accepted.A.study.calendar.holidays);for(const day of schedule.holidays)assert.deepEqual(day.profile.values,Array(144).fill(value));}
  assert.equal(h.balanced_ventilation.fraction_schedule,h.availability_schedule);assert.deepEqual(model.zones.slice(1),before.zones.slice(1));assert.deepEqual(model.materials,before.materials);assert.deepEqual(accepted,original);
  await click('Switch test design');await click('Switch test design');assert.equal(draft().structured.model.zones[0].cooling_system.id,h.id);await act(()=>show(false));await act(()=>show(true));assert.equal(draft().structured.model.zones[0].cooling_system.id,h.id);
});
test('removing DX explicitly discards only its own text while retaining shared resources and other invalid drafts',async()=>{
  await setup('hvac');const original=structuredClone(accepted),zone=accepted.A.model.zones[0],curve=accepted.A.model.curves[0];await enter('Rated indoor wet-bulb (°C)','1e');await enter(`${curve.id} coefficient c0`,'-');
  const curves=structuredClone(draft().structured.model.curves),schedules=structuredClone(draft().structured.model.schedules);
  await click('Remove DX and make zone unconditioned');assert.equal(draft().structured.model.zones[0].cooling_system,null);assert.equal(draft().fields[`resource:hvac:${zone.id}:rated_indoor_wet_bulb_c`],undefined);assert.equal(draft().fields[`resource:curve:${curve.id}:coefficient:0`],'-');assert.throws(()=>draftCandidate(draft(),accepted.A),/finite numeric/);assert.deepEqual(draft().structured.model.curves,curves);assert.deepEqual(draft().structured.model.schedules,schedules);assert.deepEqual(draft().structured.model.zones[0].gains,zone.gains);assert.deepEqual(accepted,original);
  await enter(`${curve.id} coefficient c0`,'1');assert.equal(draftCandidate(draft(),accepted.A).model.zones[0].cooling_system,null);
});
test('DX addition reserves complete curve and schedule capacity before making any draft change',async()=>{
  for(const [collection,length] of [['curves',124],['schedules',127]]){await setup('hvac',values=>{values.A.model.zones[0].cooling_system=null;const first=values.A.model[collection][0];values.A.model[collection]=Array.from({length},(_,i)=>({...structuredClone(first),id:`reserved_${i}`}));});const before=structuredClone(draft());assert.equal(button('Add explicit synthetic DX system').disabled,true);await click('Add explicit synthetic DX system');assert.deepEqual(draft(),before);assert.match(document.body.textContent,/five free curve entries and two free schedule/);}
});
test('exterior-lighting fields use one building value and fraction assignment with durable invalid drafts and exact routing markers',async()=>{
  await setup('gains');const original=structuredClone(accepted);await enter('Building exterior lighting (W)','1e');await enter('Exterior lighting fraction schedule','always');assert.throws(()=>draftCandidate(draft(),accepted.A),/finite numeric/);await click('Switch test design');await click('Switch test design');assert.equal(input('Building exterior lighting (W)').value,'1e');await enter('Building exterior lighting (W)','35.5');const candidate=draftCandidate(draft(),accepted.A);assert.equal(candidate.model.exterior_lighting_w,35.5);assert.equal(candidate.model.exterior_lighting_schedule,'always');assert.deepEqual(candidate.model.zones,accepted.A.model.zones);assert.deepEqual(accepted,original);assert.equal(input('Building exterior lighting (W)').dataset.thermalField,'/exterior_lighting_w');assert.equal(input('Exterior lighting fraction schedule').closest('[data-thermal-kind]').dataset.thermalKind,'model');assert.ok([...input('Exterior lighting fraction schedule').options].every(o=>candidate.model.schedules.find(s=>s.id===o.value).kind==='fraction'));
  await setup('envelope');const material=accepted.A.model.materials[0];const field=input(`${material.id} · Density (kg/m³)`);assert.equal(field.dataset.thermalField,'/density_kg_m3');assert.equal(field.closest('[data-thermal-id]').dataset.thermalId,material.id);
  await setup('hvac');const curve=accepted.A.model.curves[0];const coefficient=input(`${curve.id} coefficient c0`);assert.equal(coefficient.dataset.thermalField,'/form/coefficients/0');assert.equal(coefficient.closest('[data-thermal-kind]').dataset.thermalKind,'curve');
});

test.after(async()=>{if(root)await act(()=>root.unmount());dom.window.close();});
