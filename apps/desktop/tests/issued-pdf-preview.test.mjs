// Real React component and effects. Synthetic PDF worker replies test lifecycle only;
// these tests do not establish real PDF rendering or installed/native acceptance.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {fileURLToPath} from 'node:url';
import {JSDOM} from 'jsdom';
import {transformWithOxc} from 'vite';
const dom = new JSDOM('<!doctype html><button id="opener">Preview report</button><div id="root"></div>', {url:'https://greenlens.local/'});
for (const name of ['window','document','HTMLElement','HTMLInputElement','HTMLSelectElement','Event','MouseEvent','KeyboardEvent']) globalThis[name] = dom.window[name];
globalThis.IS_REACT_ACT_ENVIRONMENT = true;
const React = await import('react'), {act} = React, {createRoot} = await import('react-dom/client');
const filename = fileURLToPath(new URL('../src/energy/IssuedPdfPreview.tsx', import.meta.url));
const transformed = await transformWithOxc(fs.readFileSync(filename,'utf8'), filename, {jsx:{runtime:'automatic'}});
let code = transformed.code.replace(/import\((['"])pdfjs-dist\1\)/g, 'Promise.resolve(globalThis.__issuedPdf.api)');
for (const match of [...code.matchAll(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g)]) {
  const target = match[3].endsWith('?url') ? 'data:text/javascript,'+encodeURIComponent('export default "owned-worker.mjs"') : import.meta.resolve(match[3]);
  code = code.replace(match[0], `import ${match[1]} from ${JSON.stringify(target)};`);
}
const {IssuedPdfPreview:Preview, ISSUED_PDF_LIMITS:LIMITS} = await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));
let root, props, fixture, closed, timers;
const realSetTimeout = globalThis.setTimeout, realClearTimeout = globalThis.clearTimeout;
globalThis.setTimeout = (callback, ms, ...args) => {
  if (ms === LIMITS.loadMs || ms === LIMITS.pageMs) {const timer = {callback,ms,active:true}; timers.push(timer); return timer;}
  return realSetTimeout(callback,ms,...args);
};
globalThis.clearTimeout = id => {if (id?.callback) id.active=false; else realClearTimeout(id);};
function deferred() {let resolve,reject;const promise=new Promise((yes,no)=>{resolve=yes;reject=no;});return{promise,resolve,reject};}
function engine(options={}) {
  const state = {workers:[],pdfWorkers:[],tasks:[],pages:[],renders:[],documentDestroyed:0,loading:deferred(),getPageCalls:[],pendingPage:deferred()};
  const makePage = n => {
    const page={n,cleaned:0,cleanup(){page.cleaned++;},getViewport({scale}){return{width:(options.width??600)*scale,height:(options.height??800)*scale};},render(args){
      const wait=deferred();const task={args,promise:wait.promise,cancelled:0,cancel(){task.cancelled++;if(!options.ignoreCancel)wait.reject(Error('RenderingCancelledException'));},finish:wait.resolve};
      state.renders.push(task);if(!options.pendingRender)wait.resolve();return task;
    }};state.pages.push(page);return page;
  };
  state.document={numPages:options.numPages??3,async destroy(){state.documentDestroyed++;},getPage(n){state.getPageCalls.push(n);return options.pendingPage?state.pendingPage.promise:Promise.resolve(makePage(n));}};
  state.makePage=makePage;
  state.api={PDFWorker:{create({port}){const worker={port,destroyed:0,destroy(){worker.destroyed++;}};state.pdfWorkers.push(worker);return worker;}},getDocument(args){const task={args,promise:state.loading.promise,destroyed:0,async destroy(){task.destroyed++;}};state.tasks.push(task);if(!options.pendingLoad)state.loading.resolve(state.document);return task;}};
  globalThis.Worker=class {constructor(url,options){this.url=url;this.options=options;this.terminated=0;state.workers.push(this);}terminate(){this.terminated++;}};
  return state;
}
async function setup(options={}, overrides={}) {
  if(root)await act(()=>root.unmount());timers=[];closed=0;fixture=engine(options);globalThis.__issuedPdf=fixture;
  props={bytes:new TextEncoder().encode('%PDF-1.7\noriginal synthetic owned-byte fixture'),identity:'original-issued-product-identity',label:'Technical report — 原創',onClose(){closed++;},...overrides};
  document.getElementById('opener').focus();root=createRoot(document.getElementById('root'));await act(async()=>root.render(React.createElement(Preview,props)));
}
const text=()=>document.querySelector('[role="dialog"]').textContent;
const button=s=>{const b=[...document.querySelectorAll('button')].find(x=>x.textContent===s);assert.ok(b,s);return b;};
async function click(s){await act(async()=>button(s).click());}
async function timeout(ms){const timer=timers.find(x=>x.ms===ms&&x.active);assert.ok(timer,`active ${ms} budget`);timer.active=false;await act(async()=>timer.callback());}

test('owned bytes are copied, one worker and canvas are used, and all auxiliary fetching/actions are disabled', async()=>{
  await setup();assert.match(text(),/Page 1 of 3 rendered/);assert.equal(document.querySelectorAll('canvas').length,1);assert.equal(fixture.workers.length,1);assert.deepEqual(fixture.workers[0].options,{type:'module',name:'GreenLens issued PDF'});
  const o=fixture.tasks[0].args;assert.notEqual(o.data,props.bytes);assert.deepEqual(o.data,props.bytes);
  for(const k of ['useWasm','useSystemFonts','useWorkerFetch','enableXfa','isOffscreenCanvasSupported','isImageDecoderSupported','isEvalSupported'])assert.equal(o[k],false,k);
  for(const k of ['disableFontFace','stopAtErrors','disableRange','disableStream','disableAutoFetch'])assert.equal(o[k],true,k);
  await assert.rejects(new o.BinaryDataFactory().fetch({url:'https://example.invalid/font'}),/does not fetch/);
  assert.equal(fixture.renders[0].args.annotationMode,0);assert.equal(document.querySelector('a,iframe,object,form,script'),null);
  const before=props.bytes.slice();await click('Next page');assert.deepEqual(fixture.getPageCalls,[1,2]);assert.match(text(),/Page 2 of 3 rendered/);assert.deepEqual(props.bytes,before);
});

test('closing/context change terminates workers, cancels paint and suppresses stale loading completion',async()=>{
  await setup({pendingLoad:true});const old=fixture;fixture=engine();globalThis.__issuedPdf=fixture;
  props={...props,identity:'second-owned-identity',bytes:new TextEncoder().encode('%PDF-1.7\nsecond')};await act(async()=>root.render(React.createElement(Preview,props)));
  assert.equal(old.tasks[0].destroyed,1);assert.equal(old.workers[0].terminated,1);assert.match(text(),/Page 1 of 3 rendered/);
  await act(async()=>old.loading.resolve(old.document));assert.ok(old.tasks[0].destroyed>=2);assert.deepEqual(old.getPageCalls,[]);assert.equal(fixture.renders.length,1);
  await act(()=>root.unmount());root=undefined;assert.ok(fixture.tasks[0].destroyed>=1);assert.ok(fixture.workers[0].terminated>=1);assert.equal(document.activeElement.id,'opener');
});

test('page changes serialize cancellation before canvas reuse and a late old paint cannot claim current display',async()=>{
  await setup({pendingRender:true});const first=fixture.renders[0];assert.match(text(),/Rendering page 1/);
  await click('Next page');assert.equal(first.cancelled,1);assert.deepEqual(fixture.getPageCalls,[1,2]);assert.equal(fixture.renders.length,2);
  await act(async()=>first.finish());assert.doesNotMatch(text(),/Page 1 of 3 rendered/);
  await act(async()=>fixture.renders[1].finish());assert.match(text(),/Page 2 of 3 rendered/);
});

test('load and getPage deadlines terminate the worker and do not resurrect a late success',async()=>{
  await setup({pendingLoad:true});await timeout(LIMITS.loadMs);assert.match(text(),/30-second loading budget/);assert.equal(fixture.workers[0].terminated,1);
  await act(async()=>fixture.loading.resolve(fixture.document));assert.deepEqual(fixture.getPageCalls,[]);assert.match(text(),/Preview unavailable/);
  await setup({pendingPage:true});await timeout(LIMITS.pageMs);assert.match(text(),/15-second budget/);assert.ok(fixture.workers[0].terminated>=1);
  await act(async()=>fixture.pendingPage.resolve(fixture.makePage(1)));assert.equal(fixture.renders.length,0);assert.match(text(),/Preview unavailable/);
});

test('invalid size/signature, excessive pages and encrypted reports fail without claiming rendered output',async()=>{
  for(const bytes of [new Uint8Array(),new Uint8Array(LIMITS.bytes+1),new TextEncoder().encode('not a PDF')]){await setup({}, {bytes});assert.equal(fixture.workers.length,0);assert.ok(document.querySelector('[role="alert"]'));assert.doesNotMatch(text(),/Page 1 of 3 rendered/);}
  for(const n of [0,161,NaN]){await setup({numPages:n});assert.match(text(),/1–160 pages/);assert.equal(fixture.renders.length,0);assert.ok(fixture.workers[0].terminated>=1);}
  await setup({pendingLoad:true});await act(async()=>fixture.tasks[0].onPassword());assert.match(text(),/Encrypted PDFs/);assert.equal(fixture.workers[0].terminated,1);
});

test('large pages stay within the pixel bound and invalid dimensions fail',async()=>{
  await setup({width:1_000_000,height:2_000_000});const c=document.querySelector('canvas');assert.ok(c.width*c.height<=LIMITS.pixels);assert.match(text(),/pixel budget limit/);
  await setup({width:Infinity});assert.match(text(),/invalid dimensions/);assert.equal(fixture.renders.length,0);assert.ok(fixture.workers[0].terminated>=1);
});

test('modal keyboard focus wraps and Escape closes without invoking any PDF action',async()=>{
  await setup();const close=button('Close'),zoom=document.querySelector('select');assert.equal(document.activeElement,close);
  await act(async()=>close.dispatchEvent(new KeyboardEvent('keydown',{key:'Tab',shiftKey:true,bubbles:true,cancelable:true})));assert.equal(document.activeElement,zoom);
  await act(async()=>zoom.dispatchEvent(new KeyboardEvent('keydown',{key:'Tab',bubbles:true,cancelable:true})));assert.equal(document.activeElement,close);
  await act(async()=>close.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true})));assert.equal(closed,1);
  await click('Close');assert.equal(closed,2);
});

test.after(async()=>{if(root)await act(()=>root.unmount());globalThis.setTimeout=realSetTimeout;globalThis.clearTimeout=realClearTimeout;delete globalThis.__issuedPdf;});
