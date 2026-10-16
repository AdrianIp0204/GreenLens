// Real DesignCopy + useEnergyRead with explicit IPC/callback doubles. These
// establish review/action boundaries, not Rust/native transaction acceptance.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {JSDOM} from 'jsdom';
import {transformWithOxc} from 'vite';
const dom=new JSDOM('<div id="root"></div>',{url:'https://greenlens.local/'});
for(const k of ['window','document','HTMLElement','HTMLSelectElement','Event','MouseEvent'])globalThis[k]=dom.window[k];
globalThis.IS_REACT_ACT_ENVIRONMENT=true;
const React=await import('react'),{act}=React,{createRoot}=await import('react-dom/client');
const cache=new Map();async function load(file){if(cache.has(file))return cache.get(file);const task=(async()=>{const{code}=await transformWithOxc(fs.readFileSync(file,'utf8'),file,{jsx:{runtime:'automatic'}});let linked=code;for(const m of [...code.matchAll(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g)]){let target;if(m[3]==='@tauri-apps/api/core')target='data:text/javascript,export const invoke=(...a)=>globalThis.__copyIPC(...a);export class Channel{onmessage=()=>{};}';else if(m[3]==='../workspace/useDocument')target='data:text/javascript,export const errorText=e=>String(e);';else if(m[3].startsWith('.')){let f=path.resolve(path.dirname(file),m[3]);if(!path.extname(f))f=['.ts','.tsx','.mjs'].map(ext=>f+ext).find(fs.existsSync);target=f.endsWith('.mjs')?pathToFileURL(f).href:await load(f);}else target=import.meta.resolve(m[3]);linked=linked.replace(m[0],`import ${m[1]} from ${JSON.stringify(target)};`);}return'data:text/javascript;base64,'+Buffer.from(linked).toString('base64');})();cache.set(file,task);return task;}
const {default:Panel}=await import(await load(fileURLToPath(new URL('../src/energy/DesignCopy.tsx',import.meta.url))));
let root,doc,calls,copies,reply,commit,disabled;
const preview=(source='A',target='B')=>({schema_version:'greenlens.energy-design-copy-preview.v1',source,target,preview_sha256:(source==='A'?'a':'b').repeat(64),before:{zones:2,windows:3},after:{zones:4,windows:5},affected_object_ids:['zone:destination'],calendar_changed:true,study_changed:false,valid:true,problems:[],effects:['Retained runs remain immutable.']});
const deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return{promise,resolve};};
async function draw(){await act(async()=>root.render(React.createElement(Panel,{doc,disabled})));}
async function setup(){if(root)await act(()=>root.unmount());calls=[];copies=[];disabled=false;reply=async(_,args)=>preview(args.source,args.target);commit=async()=>true;doc={project:{id:'synthetic',energy:{designs:{A:{name:'before A'},B:{name:'before B'}},runs:[{id:'immutable-old-run'}]}},energyLocked:false,copyEnergyDesign:async(...args)=>{copies.push(args);return commit(...args);}};globalThis.__copyIPC=(...args)=>{calls.push(structuredClone(args));return reply(...args);};root=createRoot(document.getElementById('root'));await draw();}
const apply=()=>[...document.querySelectorAll('button')].find(b=>b.textContent.startsWith('Replace only design'));
async function click(){assert.ok(apply());await act(async()=>apply().click());}
async function direction(value){await act(()=>{const input=document.querySelector('select');Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set.call(input,value);input.dispatchEvent(new Event('change',{bubbles:true}));});}

test('preview lists exact destination effects and copies nothing until explicit reviewed action in either direction',async()=>{
  await setup();const before=structuredClone(doc.project);assert.equal(copies.length,0);assert.match(document.body.textContent,/Accepted destination B/);assert.match(document.body.textContent,/zone:destination/);await click();assert.deepEqual(copies,[['A','B','a'.repeat(64)]]);assert.deepEqual(doc.project,before);await direction('B');assert.equal(copies.length,1);assert.match(apply().textContent,/design A.*B inputs/);await click();assert.deepEqual(copies[1],['B','A','b'.repeat(64)]);
});
test('project changes and late previews remove the old action and never relabel another source preview',async()=>{
  await setup();const older=deferred(),fresh=deferred();reply=(_,args)=>args.source==='A'?older.promise:fresh.promise;doc.project={...doc.project,name:'new accepted input context'};await draw();assert.equal(apply(),undefined);await direction('B');
  assert.equal(calls.length,2,'cancelled active A preview retains the lane until its IPC settles');assert.equal(apply(),undefined);
  await act(async()=>older.resolve(preview('A','B')));assert.equal(calls.length,3);assert.equal(calls.at(-1)[1].source,'B');assert.equal(apply(),undefined,'stale A response is never exposed while B is pending');
  await act(async()=>fresh.resolve(preview('B','A')));assert.match(apply().textContent,/design A/);await click();assert.deepEqual(copies,[['B','A','b'.repeat(64)]]);
});
test('locked, disabled and invalid previews block the actual Replace button',async()=>{
  await setup();doc.energyLocked=true;await draw();assert.equal(apply().disabled,true);await click();assert.equal(copies.length,0);doc.energyLocked=false;disabled=true;await draw();assert.equal(apply().disabled,true);await click();assert.equal(copies.length,0);disabled=false;reply=async()=>({...preview(),valid:false,problems:['Destination semantic object is locked.']});doc.project={...doc.project,name:'locks updated'};await draw();assert.equal(apply().disabled,true);assert.match(document.body.textContent,/semantic object is locked/);await click();assert.equal(copies.length,0);
});
test('wrong direction/schema/hash or untyped validity cannot present a reviewed copy action',async()=>{
  await setup();for(const bad of [{...preview(),source:'B',target:'A'},{...preview(),schema_version:'wrong'},{...preview(),preview_sha256:'not-a-sha'},{...preview(),valid:'true'}]){reply=async()=>bad;doc.project={...doc.project,name:JSON.stringify(bad)};await draw();assert.equal(apply(),undefined);assert.match(document.body.textContent,/does not bind/);}assert.equal(copies.length,0);
});
test('rejected or false Apply preserves the explicit direction without claiming acceptance and busy blocks double action',async()=>{
  await setup();const wait=deferred();commit=()=>wait.promise;await click();assert.equal(apply().disabled,true);assert.equal(document.querySelector('select').disabled,true);await click();assert.equal(copies.length,1);await act(async()=>wait.resolve(false));assert.doesNotMatch(document.body.textContent,/inputs copied/);commit=async()=>{throw Error('Synthetic persistence refusal');};await click();assert.match(document.querySelector('[role=alert]').textContent,/not accepted.*Synthetic persistence refusal/);assert.equal(document.querySelector('select').value,'A');assert.equal(apply().disabled,false);commit=async()=>true;await click();assert.match(document.body.textContent,/inputs copied to B/);assert.equal(document.querySelector('[role=alert]'),null);
});
test.after(async()=>{if(root)await act(()=>root.unmount());dom.window.close();});
