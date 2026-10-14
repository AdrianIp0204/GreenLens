// Real schedule/calendar components and controlled draft state. IPC is a test double;
// these tests establish editing safety, not native Rust or simulation acceptance.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {JSDOM} from 'jsdom';
import {transformWithOxc} from 'vite';
const dom=new JSDOM('<div id="root"></div>',{url:'https://greenlens.local/'});
for(const key of ['window','document','HTMLElement','HTMLInputElement','HTMLSelectElement','Event','MouseEvent'])globalThis[key]=dom.window[key];
globalThis.IS_REACT_ACT_ENVIRONMENT=true;
const React=await import('react'),{act}=React,{createRoot}=await import('react-dom/client');
const cache=new Map();async function load(file){if(cache.has(file))return cache.get(file);const promise=(async()=>{const{code}=await transformWithOxc(fs.readFileSync(file,'utf8'),file,{jsx:{runtime:'automatic'}});let out=code;for(const m of [...code.matchAll(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g)]){let target;if(m[3]==='@tauri-apps/api/core')target='data:text/javascript,export const invoke=(...args)=>globalThis.__scheduleIPC(...args);';else if(m[3].startsWith('.')){let f=path.resolve(path.dirname(file),m[3]);if(!path.extname(f))f=['.ts','.tsx'].map(ext=>f+ext).find(fs.existsSync);target=await load(f);}else target=import.meta.resolve(m[3]);out=out.replace(m[0],`import ${m[1]} from ${JSON.stringify(target)};`);}return'data:text/javascript;base64,'+Buffer.from(out).toString('base64');})();cache.set(file,promise);return promise;}
const src=fileURLToPath(new URL('../src/energy/',import.meta.url));
const {default:Schedule}=await import(await load(path.join(src,'ScheduleEditor.tsx'))),{default:Calendar}=await import(await load(path.join(src,'ThermalCalendarEditor.tsx')));
const {cleanDraft,draftCandidate,fieldEdit,EnergyDraftStore}=await import(await load(path.join(src,'drafts.ts')));
const fixture=JSON.parse(fs.readFileSync(new URL('../../../examples/energy/revision1-inputs.json',import.meta.url))).cases.A;
let root,entry,change,visible,show,setShow,calls,reply,initial;
function Harness(){[entry,change]=React.useState(()=>cleanDraft(initial));[show,setShow]=React.useState(true);return show?React.createElement(visible,{entry,onChange:change}):null;}
async function setup(Component=Schedule){if(root)await act(()=>root.unmount());calls=[];initial=structuredClone({model:fixture.model,study:fixture.study});initial.model.schedules[0].weekday.values=Array(144).fill(.2);initial.model.schedules[0].saturday.values=Array(144).fill(.4);initial.model.schedules[0].holidays=[{date:{month:10,day:1},profile:{values:Array(144).fill(.6)}},{date:{month:10,day:2},profile:{values:Array(144).fill(.8)}}];visible=Component;reply=async()=>{throw Error('Explicit reply required');};globalThis.__scheduleIPC=(cmd,args)=>{calls.push([cmd,structuredClone(args)]);return reply(cmd,args);};root=createRoot(document.getElementById('root'));await act(()=>root.render(React.createElement(Harness)));}
function field(text){return[...document.querySelectorAll('label')].find(n=>n.firstChild?.textContent===text)?.querySelector('input,select');}
const named=text=>document.querySelector(`[aria-label="${text}"]`);
const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent===text);
async function click(text){assert.ok(button(text),text);await act(async()=>button(text).click());}
async function type(el,value){assert.ok(el);await act(()=>{Object.getOwnPropertyDescriptor(el.tagName==='SELECT'?HTMLSelectElement.prototype:HTMLInputElement.prototype,'value').set.call(el,value);el.dispatchEvent(new Event(el.tagName==='SELECT'?'change':'input',{bubbles:true}));});}
const deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return{promise,resolve};};

test('block drafts prevent exact-table collisions and stale expansion never overwrites newer edits',async()=>{
 await setup();await type(named('Block 1 value'),'.25');assert.throws(()=>draftCandidate(entry,initial),/Stage the edited/);const exact=named(`${entry.structured.model.schedules[0].id} weekday 00:00`);assert.ok(exact.matches(':disabled'));
 const pending=deferred();reply=()=>pending.promise;await click('Validate and stage blocks');assert.equal(calls[0][1].blocks[0].value,.25);await type(named('Block 1 value'),'.75');await act(async()=>pending.resolve({values:Array(144).fill(.25)}));assert.equal(entry.structured.model.schedules[0].weekday.values[0],.2);assert.equal(named('Block 1 value').value,'.75');assert.match(document.body.textContent,/not staged/);
 reply=async()=>({values:Array(144).fill(.75)});await click('Validate and stage blocks');assert.equal(Object.keys(entry.invalid).length,0);assert.equal(named(`${entry.structured.model.schedules[0].id} weekday 00:00`).value,'0.75');assert.deepEqual(entry.structured.model.schedules[0].weekday.values,Array(144).fill(.75));assert.equal(initial.model.schedules[0].weekday.values[0],.2);
});
test('exact-table invalid text is retained and profile replacement removes stale valid text',async()=>{
 await setup();const id=entry.structured.model.schedules[0].id;await type(named(`${id} weekday 00:00`),'-');assert.ok(named('Block 1 value').matches(':disabled'));assert.match(document.body.textContent,/Finish the incomplete/);await type(named(`${id} weekday 00:00`),'.3');await type(field('Day profile'),'saturday');await type(named(`${id} saturday 00:00`),'.91');await click('Copy weekday into this profile');assert.equal(named(`${id} saturday 00:00`).value,'0.3');assert.equal(entry.fields[`schedule:${id}:saturday:0`],undefined);
 await type(named('Block 1 value'),'.7');await click('Discard block edits');assert.equal(Object.keys(entry.invalid).length,0);assert.equal(entry.structured.model.schedules[0].saturday.values[0],.3);
});
test('removed selected holiday falls back to weekday and never edits the shifted remaining holiday',async()=>{
 await setup();await type(field('Day profile'),'holiday:10/1');const id=entry.structured.model.schedules[0].id;await act(()=>change(old=>{const next=structuredClone(old);next.structured.model.schedules[0].holidays.shift();next.version++;return next;}));assert.equal(field('Day profile').value,'weekday');await type(named(`${id} weekday 00:00`),'.1');assert.equal(entry.structured.model.schedules[0].weekday.values[0],.1);assert.equal(entry.structured.model.schedules[0].holidays[0].profile.values[0],.8);
});
test('selection changes and unmount reject late block responses; malformed returned profiles are not staged',async()=>{
 await setup();const delayed=deferred();reply=()=>delayed.promise;await click('Validate and stage blocks');await type(field('Day profile'),'saturday');await act(async()=>delayed.resolve({values:Array(144).fill(.9)}));assert.equal(entry.structured.model.schedules[0].weekday.values[0],.2);assert.match(document.body.textContent,/not staged/);
 reply=async()=>({values:Array(143).fill(.9)});await click('Validate and stage blocks');assert.match(document.body.textContent,/incomplete daily/);const late=deferred();reply=()=>late.promise;await click('Validate and stage blocks');await act(()=>setShow(false));const before=JSON.stringify(entry);await act(async()=>late.resolve({values:Array(144).fill(.9)}));assert.equal(JSON.stringify(entry),before);
});
test('calendar preserves invalid text and pending schedules, and late full-design replies cannot erase a newer site edit',async()=>{
 await setup(Calendar);await type(field('Fixed UTC offset (minutes east of UTC)'),'1e');await click('Stage calendar with this holiday policy');assert.equal(calls.length,0);assert.match(document.body.textContent,/whole-number/);await type(field('Fixed UTC offset (minutes east of UTC)'),'60');
 await act(()=>change(old=>({...old,fields:{...old.fields,'schedule-blocks:pending:weekday':'[]'},version:old.version+1})));await click('Stage calendar with this holiday policy');assert.equal(calls.length,0);assert.match(document.body.textContent,/schedule edits/);
 await act(()=>change(old=>{const fields={...old.fields};delete fields['schedule-blocks:pending:weekday'];return{...old,fields,version:old.version+1};}));const pending=deferred();reply=()=>pending.promise;await click('Stage calendar with this holiday policy');assert.equal(calls[0][1].calendar.utc_offset_minutes,60);await type(field('Elevation (m)'),'22');await act(async()=>pending.resolve(structuredClone(initial)));assert.equal(entry.structured.model.site.elevation_m,22);assert.match(document.body.textContent,/not staged/);assert.ok(entry.fields['calendar:pending']);
});
test('calendar staging sends exact declared dates and policy result stays a draft; invalid cache text requires explicit discard',async()=>{
 await setup(Calendar);await click('Add holiday');await type(named('Holiday 1 month'),'12');await type(named('Holiday 1 day'),'25');reply=async(cmd,args)=>{const result=structuredClone(initial);result.study.calendar=args.calendar;return result;};await click('Stage calendar with this holiday policy');assert.deepEqual(calls[0][1].calendar.holidays,[{month:12,day:25},{month:1,day:1}]);assert.equal(entry.structured.study.calendar.holidays[0].day,25);assert.equal(entry.dirty,true);assert.equal(entry.fields['calendar:pending'],undefined);assert.deepEqual(initial.study.calendar.holidays,fixture.study.calendar.holidays);
 await act(()=>change(old=>({...old,fields:{...old.fields,'calendar:pending':'{bad'},version:old.version+1})));assert.match(document.body.textContent,/Recovered calendar text/);await click('Discard calendar edits');assert.equal(entry.fields['calendar:pending'],undefined);assert.equal(entry.structured.study.calendar.holidays[0].day,25);
});
test('site draft recovery is accepted by the bounded cache without replacing incomplete text',()=>{
 const design={model:fixture.model,study:fixture.study};let saved;const store=new EnergyDraftStore({getItem:()=>null,setItem(k,v){saved=v;}});const draft=fieldEdit({...cleanDraft(design),section:'site'},'site:latitude_deg_north','-',()=>{});store.set('p','A',draft);const recovered=new EnergyDraftStore({getItem:()=>saved,setItem(){}});assert.equal(recovered.warning,'');assert.equal(recovered.get('p','A',design).section,'site');assert.equal(recovered.get('p','A',design).fields['site:latitude_deg_north'],'-');
});
test('same-version replacement drafts reject both pending schedule and calendar responses',async()=>{
 for(const Component of [Schedule,Calendar]){await setup(Component);const waiting=deferred();reply=()=>waiting.promise;await click(Component===Schedule?'Validate and stage blocks':'Stage calendar with this holiday policy');const replacement=structuredClone(entry);replacement.structured.model.site.elevation_m=44;assert.equal(replacement.version,entry.version);await act(()=>change(replacement));await act(async()=>waiting.resolve(Component===Schedule?{values:Array(144).fill(.9)}:structuredClone(initial)));assert.equal(entry.structured.model.site.elevation_m,44);assert.equal(entry.structured.model.schedules[0].weekday.values[0],.2);assert.match(document.body.textContent,/not staged/);}
});
test('schedule duplication reserves the complete object namespace and preserves bounded stable IDs',async()=>{
 await setup();const source=entry.structured.model.schedules[0].id;
 await act(()=>change(old=>{const next=structuredClone(old),m=next.structured.model;const categories=[m.materials[0],m.constructions[0],m.glazing[0],m.curves[0],m.zones[0],m.zones[0].surfaces[0],m.zones[0].windows[0],m.zones[0].cooling_system];categories.forEach((item,i)=>{assert.ok(item);item.id=`${source}_copy${i?i+1:''}`;});m.detached_shading.push({id:`${source}_copy9`});return next;}));
 const before=structuredClone(entry.structured.model);await click('Duplicate schedule');const copy=entry.structured.model.schedules.at(-1);assert.equal(copy.id,`${source}_copy10`);assert.equal(field('Schedule').value,copy.id);assert.deepEqual({...copy,id:source},before.schedules[0]);assert.deepEqual(entry.structured.model.materials,before.materials);assert.equal(calls.length,0);
 const long='s'.repeat(80);await act(()=>change(old=>{const next=structuredClone(old);next.structured.model.schedules=[{...next.structured.model.schedules[0],id:long}];return next;}));await click('Duplicate schedule');assert.equal(entry.structured.model.schedules.at(-1).id,'s'.repeat(75)+'_copy');assert.equal(entry.structured.model.schedules.at(-1).id.length,80);
 await act(()=>change(old=>{const next=structuredClone(old);next.structured.model.schedules=Array.from({length:128},(_,i)=>({...structuredClone(next.structured.model.schedules[0]),id:`schedule_${i}`}));return next;}));assert.ok(button('Duplicate schedule').disabled);await click('Duplicate schedule');assert.equal(entry.structured.model.schedules.length,128);
});
test.after(async()=>{if(root)await act(()=>root.unmount());dom.window.close();});
