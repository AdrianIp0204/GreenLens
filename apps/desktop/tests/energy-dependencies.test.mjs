// Actual React component and bounded local store. IPC is mocked, so these tests
// verify settings/readiness presentation, not installed native dependencies.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {JSDOM} from 'jsdom';
import {transformWithOxc} from 'vite';
const dom=new JSDOM('<div id="root"></div>',{url:'https://greenlens.local/'});
for(const key of ['window','document','HTMLElement','HTMLInputElement','Event','MouseEvent'])globalThis[key]=dom.window[key];
globalThis.IS_REACT_ACT_ENVIRONMENT=true;
const React=await import('react'),{act}=React,{createRoot}=await import('react-dom/client');
const cache=new Map();async function load(file){if(cache.has(file))return cache.get(file);const pending=(async()=>{const {code}=await transformWithOxc(fs.readFileSync(file,'utf8'),file,{jsx:{runtime:'automatic'}});let linked=code;for(const match of [...code.matchAll(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g)]){let target;if(match[3]==='@tauri-apps/api/core')target='data:text/javascript,export const invoke=(...args)=>globalThis.__dependencyIPC(...args);';else if(match[3].startsWith('.')){let f=path.resolve(path.dirname(file),match[3]);if(!path.extname(f))f=['.ts','.tsx'].map(ext=>f+ext).find(fs.existsSync);target=await load(f);}else target=import.meta.resolve(match[3]);linked=linked.replace(match[0],`import ${match[1]} from ${JSON.stringify(target)};`);}return'data:text/javascript;base64,'+Buffer.from(linked).toString('base64');})();cache.set(file,pending);return pending;}
const src=fileURLToPath(new URL('../src/energy/',import.meta.url));
const {default:Settings}=await import(await load(path.join(src,'EnergyDependencySettings.tsx')));
const {useEnergyDependencyPaths,EnergyDependencyPathStore,validDependencyPaths}=await import(await load(path.join(src,'energy-dependency-settings.ts')));
let root,settings,calls,reply;
function Harness(){settings=useEnergyDependencyPaths();return React.createElement(Settings,{settings});}
const valid={schema_version:'greenlens.energy-dependencies.v1',energyplus_ready:true,typst_and_fonts_ready:true,issues:[]};
async function mount(clear=true){if(root)await act(()=>root.unmount());if(clear)window.localStorage.clear();calls=[];reply=async()=>structuredClone(valid);globalThis.__dependencyIPC=(name,args)=>{calls.push([name,structuredClone(args)]);return reply(name,args);};root=createRoot(document.getElementById('root'));await act(()=>root.render(React.createElement(Harness)));}
const field=label=>[...document.querySelectorAll('label')].find(n=>n.firstChild.textContent===label).querySelector('input');
async function type(label,value){await act(()=>{const input=field(label);Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,value);input.dispatchEvent(new Event('input',{bubbles:true}));});}
async function click(text){const b=[...document.querySelectorAll('button')].find(n=>n.textContent===text);assert.ok(b,text);await act(async()=>b.click());}
function deferred(){let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return{promise,resolve,reject};}

test('directory settings persist locally across remount without invoking native work or creating project data',async()=>{
  await mount();await type('EnergyPlus directory','C:\\local tools 空間\\EnergyPlus');await type('Typst directory','D:\\render tools\\Typst');assert.equal(calls.length,0);const stored=JSON.parse(window.localStorage.getItem('greenlens.energy-dependency-paths.v1'));assert.deepEqual(stored,{schema_version:'greenlens.energy-dependency-paths.v1',paths:{engineDir:'C:\\local tools 空間\\EnergyPlus',typstDir:'D:\\render tools\\Typst'}});
  await mount(false);assert.equal(field('EnergyPlus directory').value,stored.paths.engineDir);assert.equal(field('Typst directory').value,stored.paths.typstDir);assert.equal(calls.length,0);assert.match(document.body.textContent,/not been checked/);
});
test('explicit checks pass exact directory arguments, show separate readiness/remedies and defaults use null',async()=>{
  await mount();await type('EnergyPlus directory','C:\\EnergyPlus 空間');reply=async()=>({...valid,typst_and_fonts_ready:false,issues:[{severity:'error',code:'dependency.typst',message:'Fonts are missing.',remedy:'Install the pinned fonts.'}]});await click('Check local installations');assert.deepEqual(calls,[['energy_dependencies',{engineDir:'C:\\EnergyPlus 空間',typstDir:null}]]);assert.match(document.body.textContent,/EnergyPlus: pinned installation ready/);assert.match(document.body.textContent,/Typst and fonts: missing or altered/);assert.match(document.body.textContent,/Install the pinned fonts/);
  await click('Use default locations');assert.deepEqual(settings.paths,{engineDir:'',typstDir:''});assert.match(document.body.textContent,/not been checked/);await click('Check local installations');assert.deepEqual(calls[1][1],{engineDir:null,typstDir:null});
});
test('edited paths and unmount reject stale readiness success or failure, including returning to the same path',async()=>{
  await mount();const waiting=deferred();reply=()=>waiting.promise;await click('Check local installations');await type('EnergyPlus directory','C:\\changed');await type('EnergyPlus directory','');await act(async()=>waiting.resolve(valid));assert.doesNotMatch(document.body.textContent,/EnergyPlus: pinned installation ready/);assert.match(document.body.textContent,/not been checked/);
  const late=deferred();reply=()=>late.promise;await click('Check local installations');await act(()=>root.unmount());root=null;await act(async()=>late.reject(Error('late stale failure')));await mount(false);assert.doesNotMatch(document.body.textContent,/late stale failure/);
});
test('invalid typed readiness cannot imply ready and failed checks can be explicitly retried',async()=>{
  await mount();for(const bad of [{...valid,energyplus_ready:'true'},{...valid,schema_version:'other'},{...valid,issues:null}]){reply=async()=>bad;await click('Check local installations');assert.match(document.body.textContent,/invalid response/);assert.doesNotMatch(document.body.textContent,/EnergyPlus: pinned installation ready/);}
  reply=async()=>{throw Error('Read failed');};await click('Check local installations');assert.match(document.body.textContent,/Read failed/);reply=async()=>valid;await click('Check local installations');assert.match(document.body.textContent,/EnergyPlus: pinned installation ready/);assert.doesNotMatch(document.body.textContent,/Read failed/);
});
test('bounded cache rejects oversized, control-character and malformed settings without losing the last valid paths',()=>{
  assert.equal(validDependencyPaths({engineDir:'C:\\x',typstDir:''}),true);assert.equal(validDependencyPaths({engineDir:'x\n',typstDir:''}),false);assert.equal(validDependencyPaths({engineDir:'x'.repeat(4097),typstDir:''}),false);
  for(const raw of ['{','x'.repeat(32769),JSON.stringify({schema_version:'old',paths:{engineDir:'x',typstDir:''}}),JSON.stringify({schema_version:'greenlens.energy-dependency-paths.v1',paths:{engineDir:3,typstDir:''}})]){const store=new EnergyDependencyPathStore({getItem:()=>raw,setItem(){}});assert.deepEqual(store.paths,{engineDir:'',typstDir:''});assert.match(store.warning,/Default discovery/);}
  const store=new EnergyDependencyPathStore({getItem:()=>null,setItem(){}});assert.equal(store.set({engineDir:'C:\\kept',typstDir:''}),true);assert.equal(store.set({engineDir:'x\u0000',typstDir:''}),false);assert.equal(store.paths.engineDir,'C:\\kept');assert.match(store.warning,/previous settings remain/);
});
test('quota and unavailable storage preserve current-window settings with an explicit warning',()=>{
  for(const storage of [null,{getItem:()=>null,setItem(){throw Error('quota');}}]){const store=new EnergyDependencyPathStore(storage);assert.equal(store.set({engineDir:'C:\\temporary',typstDir:''}),true);assert.equal(store.paths.engineDir,'C:\\temporary');assert.match(store.warning,/only in this window/);}
});
test.after(async()=>{if(root)await act(()=>root.unmount());dom.window.close();});
