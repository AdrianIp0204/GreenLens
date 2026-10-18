// Actual React draft lifecycle. Local storage is explicitly machine-local;
// these tests are not native report, renderer or simulation acceptance.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {JSDOM} from 'jsdom';
import {transformWithOxc} from 'vite';

const dom=new JSDOM('<!doctype html><div id="root"></div>',{url:'https://greenlens.local/'});
for(const name of ['window','document','HTMLElement','Event'])globalThis[name]=dom.window[name];
globalThis.IS_REACT_ACT_ENVIRONMENT=true;
const React=await import('react'),{act}=React,{createRoot}=await import('react-dom/client');
const filename=new URL('../src/energy/useDesignReportDrafts.ts',import.meta.url);
const {code}=await transformWithOxc(fs.readFileSync(filename,'utf8'),filename.pathname);
const linked=code.replace(/from\s+(['"])react\1/g,`from ${JSON.stringify(import.meta.resolve('react'))}`);
const {useDesignReportDrafts}=await import(`data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`);
const key='greenlens.design-report-declarations.v1',version='greenlens.local-design-report-declarations.v1';
const setItem=dom.window.Storage.prototype.setItem;
let root,controller,projectId,path,design,visible;
function Harness(){controller=useDesignReportDrafts(projectId);return visible?React.createElement('section',{'data-path':path,'data-design':design},React.createElement('p',null,controller.value.scope),React.createElement('p',null,controller.value.north),React.createElement('p',null,controller.value.selected)):null;}
async function draw(){await act(async()=>root.render(React.createElement(Harness)));}
async function setup(stored=null){
  if(root)await act(async()=>root.unmount());
  dom.window.Storage.prototype.setItem=setItem;window.localStorage.clear();
  if(stored!==null)window.localStorage.setItem(key,stored);
  document.body.innerHTML='<div id="root"></div>';root=createRoot(document.getElementById('root'));
  projectId='synthetic-document-A';path='project/original';design='A';visible=true;await draw();
}
async function update(value){let result;await act(async()=>{result=controller.update(value);});return result;}
async function choose(choice){await act(async()=>controller.resolve(choice));}
async function request(reason='Replace document'){
  let settled=false,result;
  await act(async()=>{controller.beforeReplacement(reason).then(value=>{settled=true;result=value;});});
  return{get settled(){return settled;},get result(){return result;}};
}
const row=(id,scope='')=>({document_id:id,scope,north:'',selected:''});
const encoded=rows=>JSON.stringify({schema_version:version,drafts:rows});

test('local scope, orientation and selection survive panel/design navigation, Save As and remount',async()=>{
  await setup();const value={scope:'Original supported partial scope; context excluded.',north:'True north remains unknown.',selected:'a'.repeat(64)};
  await update(value);const saved=window.localStorage.getItem(key);
  visible=false;await draw();design='B';path='portable/copy';await draw();visible=true;await draw();
  assert.deepEqual(controller.value,value);assert.equal(window.localStorage.getItem(key),saved);
  await act(async()=>root.unmount());root=createRoot(document.getElementById('root'));await draw();
  assert.deepEqual(controller.value,value);assert.equal(controller.warning,'');
  assert.equal(saved.includes('portable/copy'),false);assert.equal(saved.includes('project/original'),false);
});

test('genuine document changes isolate drafts without overwriting earlier declarations',async()=>{
  await setup();await update({scope:'First explicit scope',north:'Unknown',selected:'a'.repeat(64)});
  projectId='synthetic-document-B';await draw();assert.deepEqual(controller.value,{scope:'',north:'',selected:''});
  await update({scope:'Second separate scope',selected:'b'.repeat(64)});
  projectId='synthetic-document-A';await draw();assert.deepEqual(controller.value,{scope:'First explicit scope',north:'Unknown',selected:'a'.repeat(64)});
  const rows=JSON.parse(window.localStorage.getItem(key)).drafts;assert.equal(rows.length,2);
});

test('invalid issuance-sized multibyte draft stays intact; local bound fails visibly without truncation',async()=>{
  await setup();const invalidForIssuance='界'.repeat(683);assert.equal(new TextEncoder().encode(invalidForIssuance).length,2049);
  await update({scope:invalidForIssuance});assert.equal(controller.value.scope,invalidForIssuance);
  visible=false;await draw();visible=true;await draw();assert.equal(controller.value.scope,invalidForIssuance);
  const bounded='界'.repeat(5461);assert.equal(await update({scope:bounded}),true);
  const previous=window.localStorage.getItem(key);assert.equal(await update({scope:'界'.repeat(5462)}),false);
  assert.equal(controller.value.scope,bounded);assert.equal(window.localStorage.getItem(key),previous);assert.match(controller.warning,/limits/);
});

test('replacement Cancel preserves draft; Keep persists it; Discard explicitly clears only declaration',async()=>{
  await setup();await update({scope:'Explicit candidate',north:'Unknown',selected:'a'.repeat(64)});
  const cancelled=await request('Open another study');assert.equal(cancelled.settled,false);assert.equal(controller.guard.reason,'Open another study');
  await choose('cancel');assert.equal(cancelled.result,false);assert.equal(controller.value.scope,'Explicit candidate');
  const kept=await request('Close native window');await choose('keep');assert.equal(kept.result,true);assert.equal(controller.guard,null);assert.equal(controller.value.scope,'Explicit candidate');
  const discarded=await request();await choose('discard');assert.equal(discarded.result,true);assert.deepEqual(controller.value,{scope:'',north:'',selected:'a'.repeat(64)});
  assert.equal((await request()).result,true);assert.equal(controller.guard,null);
});

test('failed local write retains text and blocks Keep/Discard until storage is repaired',async()=>{
  await setup();await update({scope:'Previously stored text'});const previous=window.localStorage.getItem(key);
  dom.window.Storage.prototype.setItem=function(){throw Error('Synthetic quota failure');};
  await update({scope:'New draft retained in memory'});assert.equal(controller.value.scope,'New draft retained in memory');assert.equal(window.localStorage.getItem(key),previous);assert.match(controller.warning,/remains in this window/);
  const pending=await request();await choose('keep');assert.equal(pending.settled,false);assert.match(controller.guard.error,/retained/);
  await choose('discard');assert.equal(pending.settled,false);assert.equal(controller.value.scope,'New draft retained in memory');
  dom.window.Storage.prototype.setItem=setItem;await choose('keep');assert.equal(pending.result,true);assert.equal(controller.guard,null);assert.equal(controller.warning,'');
  assert.equal(JSON.parse(window.localStorage.getItem(key)).drafts[0].scope,'New draft retained in memory');
});

test('malformed, future or duplicate stored contracts are preserved and editing fails closed',async()=>{
  for(const stored of ['{broken',JSON.stringify({schema_version:'future',drafts:[]}),encoded([row('same'),row('same')])]){
    await setup(stored);assert.equal(controller.locked,true);assert.match(controller.warning,/not been overwritten/);
    assert.equal(await update({scope:'Must not overwrite'}),false);assert.equal(window.localStorage.getItem(key),stored);
  }
});

test('32-document limit never evicts another local declaration',async()=>{
  const stored=encoded(Array.from({length:32},(_,i)=>row(`retained-${i}`,'Keep this explicit local draft')));
  await setup(stored);assert.equal(controller.locked,true);assert.match(controller.warning,/32-document/);
  assert.equal(await update({scope:'A thirty-third declaration'}),false);assert.equal(window.localStorage.getItem(key),stored);
  projectId='retained-0';await draw();assert.equal(controller.locked,false);await act(async()=>controller.discard());
  projectId='synthetic-document-A';await draw();assert.equal(controller.locked,false);assert.equal(await update({scope:'Now explicitly available'}),true);
  assert.equal(JSON.parse(window.localStorage.getItem(key)).drafts.length,32);
});

test('aggregate expanded storage limit rejects the new text without deleting retained drafts',async()=>{
  const rows=Array.from({length:31},(_,i)=>row(`retained-${i}`,'x'.repeat(16384)));
  const stored=encoded(rows);assert.ok(new TextEncoder().encode(stored).length<512*1024);
  await setup(stored);assert.equal(controller.locked,false);
  assert.equal(await update({scope:'y'.repeat(16384)}),false);assert.equal(window.localStorage.getItem(key),stored);assert.equal(controller.value.scope,'');assert.match(controller.warning,/512 KiB/);
});

test('document change or hook unmount cancels an obsolete replacement request without clearing stored draft',async()=>{
  await setup();await update({scope:'Retain despite obsolete guard'});const changed=await request();projectId='different-document';await draw();
  assert.equal(changed.result,false);assert.equal(controller.guard,null);assert.equal(controller.value.scope,'');
  projectId='synthetic-document-A';await draw();assert.equal(controller.value.scope,'Retain despite obsolete guard');
  const unmounted=await request();await act(async()=>root.unmount());root=null;assert.equal(unmounted.result,false);
  assert.equal(JSON.parse(window.localStorage.getItem(key)).drafts[0].scope,'Retain despite obsolete guard');
});

test('duplicate replacement requests fail without replacing the first pending decision',async()=>{
  await setup();await update({scope:'Await one explicit decision'});const first=await request('First action'),second=await request('Second action');
  assert.equal(second.result,false);assert.equal(first.settled,false);assert.equal(controller.guard.reason,'First action');await choose('cancel');assert.equal(first.result,false);
});

test.after(async()=>{dom.window.Storage.prototype.setItem=setItem;if(root)await act(async()=>root.unmount());dom.window.close();});
