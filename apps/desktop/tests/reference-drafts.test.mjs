// Real draft hook, guard and reference forms; explicit fake Rust actions only.
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
async function load(filename){if(modules.has(filename))return modules.get(filename);const task=(async()=>{const {code}=await transformWithOxc(fs.readFileSync(filename,'utf8'),filename,{jsx:{runtime:'automatic'}});let linked=code.replace(/import\s+['"][^'"]+\.css['"];?/g,'');for(const m of [...code.matchAll(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g)]){let target;if(m[3].startsWith('.')){let resolved=path.resolve(path.dirname(filename),m[3]);if(!path.extname(resolved))resolved=['.ts','.tsx','.mjs'].map(e=>resolved+e).find(fs.existsSync);target=await load(resolved);}else target=import.meta.resolve(m[3]);linked=linked.replace(m[0],`import ${m[1]} from ${JSON.stringify(target)};`);}return`data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`;})();modules.set(filename,task);return task;}
const source=name=>fileURLToPath(new URL(`../src/reference/${name}`,import.meta.url));
const {useReferenceDrafts}=await import(await load(source('useReferenceDrafts.ts')));
const {default:Guard}=await import(await load(source('ReferenceDraftGuard.tsx')));
const {default:Panel}=await import(await load(source('ReferenceWorkbench.tsx')));
const {newReferenceDraft,newImportDraft,parseReferenceImportDraft}=await import(await load(source('reference-drafts.ts')));
let root,drafts,apply,projectId,showPanel,actions,applyCalls,guardOutcome;
const deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return{promise,resolve};};
function Harness(){
  drafts=useReferenceDrafts(projectId,async value=>{applyCalls.push(structuredClone(value));return apply(value);});
  return React.createElement(React.Fragment,null,
    ...['Save','Replace document','Run simulation'].map(reason=>React.createElement('button',{key:reason,onClick:()=>{guardOutcome=null;void drafts.beforeAction(reason).then(allowed=>{guardOutcome=allowed;if(allowed)actions.push(reason);});}},reason)),
    showPanel&&React.createElement(Panel,{documentId:projectId,reference:null,scope:null,assessment:null,context:null,draft:drafts.draft,onDraftChange:value=>value?drafts.update(value):drafts.discard(),onConsumeDraft:drafts.consume,onSelectOccurrence:()=>{},onImport:async()=>false,onAssess:async()=>{throw Error('unused');},onSaveScope:async()=>false,onClearScope:async()=>false}),
    drafts.warning&&React.createElement('p',{role:'status'},drafts.warning),React.createElement(Guard,{drafts}));
}
async function draw(){await act(async()=>root.render(React.createElement(Harness)));}
async function setup({raw=null}={}){if(root)await act(async()=>root.unmount());window.localStorage.clear();document.body.innerHTML='<div id="root"></div>';root=createRoot(document.getElementById('root'));projectId='original-document';showPanel=true;actions=[];applyCalls=[];guardOutcome=null;apply=async value=>{if(value.import&&value.scope)throw Error('Resolve import and scope separately');if(value.import)parseReferenceImportDraft(value.import);return true;};if(raw!==null)window.localStorage.setItem(`greenlens.reference-draft.v1:${projectId}`,raw);await draw();}
const button=text=>[...document.querySelectorAll('button')].find(el=>el.textContent===text);
async function click(text){const el=button(text);assert.ok(el,text);await act(async()=>el.dispatchEvent(new MouseEvent('click',{bubbles:true})));}
function field(text){const label=[...document.querySelectorAll('label')].find(el=>[...el.childNodes].filter(n=>n.nodeType===3).map(n=>n.textContent).join('').trim()===text);assert.ok(label,text);return label.querySelector('input');}
async function type(text,value){const el=field(text);await act(async()=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(el,value);el.dispatchEvent(new Event('input',{bubbles:true}));});}
async function pendingDraft(){const value=newReferenceDraft(projectId);value.import={...newImportDraft(),scale:'-',dimensions_checked:true,orientation_checked:true};await act(async()=>drafts.update(value));}

test('actual form draft survives panel remount, document switches and hook remount without acceptance',async()=>{
  await setup();await click('Prepare local GLB import');await type('Metres per source unit','0.001');await type('Conversion provenance / rights note','Original fixture only');
  showPanel=false;await draw();showPanel=true;await draw();assert.equal(field('Metres per source unit').value,'0.001');
  projectId='another-document';await draw();assert.equal(button('Prepare local GLB import')!==undefined,true);await click('Prepare local GLB import');await type('Metres per source unit','2');
  projectId='original-document';await draw();assert.equal(field('Metres per source unit').value,'0.001');assert.equal(field('Conversion provenance / rights note').value,'Original fixture only');assert.deepEqual(applyCalls,[]);
  await act(async()=>root.unmount());document.body.innerHTML='<div id="root"></div>';root=createRoot(document.getElementById('root'));await draw();assert.equal(field('Metres per source unit').value,'0.001');
});

test('Save, replacement and calculation require Apply Discard or Cancel and invalid Apply preserves the full draft',async()=>{
  for(const action of ['Save','Replace document','Run simulation']){
    await setup();await pendingDraft();const before=JSON.stringify(drafts.draft);await click(action);assert.match(document.querySelector('[role="dialog"]').textContent,new RegExp(action));assert.deepEqual(actions,[]);
    await click('Apply');assert.match(document.querySelector('[role="alert"]').textContent,/finite number/);assert.equal(JSON.stringify(drafts.draft),before);assert.deepEqual(actions,[]);assert.equal(guardOutcome,null);
    await click('Cancel');assert.equal(guardOutcome,false);assert.equal(JSON.stringify(drafts.draft),before);assert.equal(document.querySelector('[role="dialog"]'),null);
    await click(action);await click('Discard');assert.deepEqual(actions,[action]);assert.equal(drafts.draft.import,null);assert.equal(guardOutcome,true);
  }
});

test('newer edits during an in-flight Apply survive and the waiting destructive action does not continue',async()=>{
  await setup();await pendingDraft();const value={...drafts.draft,import:{...drafts.draft.import,scale:'1'}};await act(async()=>drafts.update(value));const pending=deferred();apply=()=>pending.promise;
  await click('Save');await click('Apply');assert.equal(button('Apply').disabled,true);const newer={...drafts.draft,import:{...drafts.draft.import,note:'New edit while validation pending'}};await act(async()=>drafts.update(newer));await act(async()=>pending.resolve(true));
  assert.equal(drafts.draft.import.note,'New edit while validation pending');assert.match(document.querySelector('[role="alert"]').textContent,/newer draft/);assert.deepEqual(actions,[]);await click('Cancel');assert.equal(guardOutcome,false);
});

test('a guard for one document cannot discard a different document draft',async()=>{
  await setup();await pendingDraft();await click('Replace document');const oldResolve=drafts.resolve;projectId='second-document';await draw();await pendingDraft();assert.equal(document.querySelector('[role="dialog"]'),null);await act(async()=>oldResolve('discard'));assert.equal(guardOutcome,false);assert.equal(drafts.draft.import.scale,'-');assert.deepEqual(actions,[]);
  projectId='original-document';await draw();assert.equal(drafts.draft.import.scale,'-');
});

test('unmount resolves a pending replacement guard as cancelled',async()=>{
  await setup();await pendingDraft();await click('Replace document');assert.equal(guardOutcome,null);await act(async()=>root.unmount());root=null;assert.equal(guardOutcome,false);assert.deepEqual(actions,[]);
});

test('durable consumption survives panel unmount and preserves newer categories and different documents',async()=>{
  await setup();await pendingDraft();const consumed=JSON.stringify(drafts.draft.import),consume=drafts.consume;
  showPanel=false;await draw();const otherCategory={value:{title:'An independent scope draft'},numbers:{incomplete:'-'}};
  await act(async()=>drafts.update({...drafts.draft,scope:otherCategory}));
  await act(async()=>assert.equal(consume('import',consumed,'original-document'),true));
  assert.equal(drafts.draft.import,null);assert.deepEqual(drafts.draft.scope,otherCategory);
  await pendingDraft();const old=JSON.stringify(drafts.draft.import);await act(async()=>drafts.update({...drafts.draft,import:{...drafts.draft.import,note:'Newer intake'}}));
  await act(async()=>assert.equal(consume('import',old,'original-document'),false));assert.equal(drafts.draft.import.note,'Newer intake');
  projectId='second-document';await draw();await pendingDraft();await act(async()=>assert.equal(consume('import',JSON.stringify(drafts.draft.import),'original-document'),false));assert.equal(drafts.draft.import.scale,'-');
  const current=drafts.consume,second=JSON.stringify(drafts.draft.import);await act(async()=>root.unmount());root=null;assert.equal(current('import',second,'second-document'),false);
});

test('malformed recovered draft shape is rejected before rendering actual forms',async()=>{
  await setup({raw:JSON.stringify({schema_version:'greenlens.reference-workbench-draft.v1',document_id:'original-document',import:{scale:1},scope:{}})});
  assert.equal(drafts.draft.import,null);assert.equal(drafts.draft.scope,null);assert.equal(button('Prepare local GLB import')!==undefined,true);assert.deepEqual(applyCalls,[]);
});

test('local storage failures retain the live draft and show an actionable warning',async()=>{
  await setup();const proto=Object.getPrototypeOf(window.localStorage),original=proto.setItem;proto.setItem=()=>{throw Error('Synthetic quota failure');};
  try{await pendingDraft();assert.equal(drafts.draft.import.scale,'-');assert.match(document.body.textContent,/local persistence is unavailable/);await click('Save');await click('Cancel');assert.equal(drafts.draft.import.scale,'-');}
  finally{proto.setItem=original;}
});
test.after(async()=>{if(root)await act(async()=>root.unmount());});
