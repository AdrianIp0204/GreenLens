// Real component, Three.js cameras, meshes, OrbitControls and DOM events.
// The GPU draw port alone is replaced; this is not native/WebGL acceptance.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {JSDOM} from 'jsdom';
import {transformWithOxc} from 'vite';
const dom=new JSDOM('<div id="root"></div>',{url:'https://greenlens.local/'});
for(const name of ['window','document','HTMLElement','HTMLInputElement','Event','MouseEvent','KeyboardEvent','WheelEvent'])globalThis[name]=dom.window[name];
globalThis.IS_REACT_ACT_ENVIRONMENT=true;globalThis.devicePixelRatio=1;
// jsdom lacks pointer capture; real OrbitControls still handles every pointer event.
HTMLElement.prototype.setPointerCapture=function(){};HTMLElement.prototype.releasePointerCapture=function(){};
globalThis.ResizeObserver=class{constructor(callback){this.callback=callback;}observe(){this.callback();}disconnect(){}};
HTMLElement.prototype.getBoundingClientRect=function(){let left=0,top=0,width=800,height=600;if(this.classList.contains('thermal-viewport')){if(this.style.display==='none')width=height=0;else if(!this.parentElement.classList.contains('is-maximized')){const i=[...this.parentElement.querySelectorAll('.thermal-viewport')].indexOf(this);width=400;height=300;left=i%2*400;top=Math.floor(i/2)*300;}}return{left,top,width,height,right:left+width,bottom:top+height};};
Object.defineProperties(HTMLElement.prototype,{clientWidth:{get(){return this.getBoundingClientRect().width;}},clientHeight:{get(){return this.getBoundingClientRect().height;}}});
const React=await import('react'),{act}=React,{createRoot}=await import('react-dom/client');
const cache=new Map();async function moduleUrl(file){if(cache.has(file))return cache.get(file);const pending=(async()=>{const{code}=await transformWithOxc(fs.readFileSync(file,'utf8'),file,{jsx:{runtime:'automatic'}});let linked=code.replace(/import\s+['"][^'"]+\.css['"];?/g,'');for(const m of [...code.matchAll(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g)]){let target;if(m[3].startsWith('.')){let f=path.resolve(path.dirname(file),m[3]);if(!path.extname(f))f=['.ts','.tsx'].map(ext=>f+ext).find(fs.existsSync);target=await moduleUrl(f);}else target=import.meta.resolve(m[3]);linked=linked.replace(m[0],`import ${m[1]} from ${JSON.stringify(target)};`);}return 'data:text/javascript;base64,'+Buffer.from(linked).toString('base64');})();cache.set(file,pending);return pending;}
const src=fileURLToPath(new URL('../src/energy/',import.meta.url));
const {default:View}=await import(await moduleUrl(path.join(src,'ThermalView.tsx')));
const {default:InspectorResizer}=await import(await moduleUrl(path.join(src,'ThermalInspectorResizer.tsx')));
const {useThermalPresentation,ThermalPresentationStore,defaultThermalPresentation,validThermalPresentation}=await import(await moduleUrl(path.join(src,'thermal-presentation.ts')));
const {objectVisible}=await import(await moduleUrl(path.join(src,'ThermalViewport.ts')));
const scene={schema_version:'greenlens.thermal-scene.v1',model_sha256:'synthetic-scene',coordinates:'world ENU metres',objects:[{id:'zone:z',label:'Office',kind:'zone',zone_id:'z',storey:1,polygons_m:[]},{id:'surface:s',label:'South',kind:'surface',zone_id:'z',storey:1,polygons_m:[[[0,0,0],[8,0,0],[8,0,3],[0,0,3]]]},{id:'window:w',label:'Glazing',kind:'window',zone_id:'z',storey:1,polygons_m:[[[1,-.01,1],[3,-.01,1],[3,-.01,2],[1,-.01,2]]]}]};
const original=JSON.stringify(scene);let root,controller,design='A',show=true,selected=null,lockCalls=[],rendererCount=0,disposeCount=0,commands=null,registrations=[],shownScene=scene;
const registerCommands=value=>{commands=value;registrations.push(value);};
function renderer(){rendererCount++;return{domElement:document.createElement('canvas'),setPixelRatio(){},setScissorTest(){},setSize(){},setViewport(){},setScissor(){},render(){},setAnimationLoop(){},dispose(){disposeCount++;}};}
function Harness(){controller=useThermalPresentation('camera-project',design);return show?React.createElement(React.Fragment,null,React.createElement(View,{scene:shownScene,presentation:controller,locks:[],disabled:false,onSelect:o=>{selected=o;},onLocks:async ids=>lockCalls.push(ids),rendererFactory:renderer,onCommandsReady:registerCommands}),React.createElement(InspectorResizer,{presentation:controller})):null;}
async function draw(){await act(()=>root.render(React.createElement(Harness)));}
async function setup({clear=true}={}){if(root)await act(()=>root.unmount());if(clear)window.localStorage.clear();design='A';show=true;selected=null;lockCalls=[];shownScene=scene;root=createRoot(document.getElementById('root'));await draw();}
const button=text=>[...document.querySelectorAll('button')].find(b=>b.textContent===text);
async function click(target){assert.ok(target);await act(()=>target.dispatchEvent(new MouseEvent('click',{bubbles:true})));}
const pane=id=>document.querySelector(`[aria-label="${id} thermal view"]`);
const snapshot=()=>structuredClone(controller.value.cameras);
function near(a,b){assert.equal(a.length,b.length);a.forEach((v,i)=>assert.ok(Math.abs(v-b[i])<1e-9,`${v} vs ${b[i]}`));}
function sameCameras(a,b){for(const id of ['top','front','right','perspective']){near(a[id].position,b[id].position);near(a[id].target,b[id].target);near(a[id].up,b[id].up);assert.ok(Math.abs(a[id].zoom-b[id].zoom)<1e-9);}}
test('real four-view component retains independent cameras across unmount, design return and cold local recovery',async()=>{
  await setup();assert.equal(document.querySelectorAll('.thermal-viewport').length,4);const before=snapshot();
  await act(()=>pane('Top').dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true,cancelable:true})));const moved=snapshot();assert.notDeepEqual(moved.top.target,before.top.target);assert.deepEqual(moved.perspective,before.perspective);
  await act(()=>pane('Front').dispatchEvent(new WheelEvent('wheel',{deltaY:-100,bubbles:true,cancelable:true})));const after=snapshot();assert.notEqual(after.front.zoom,moved.front.zoom);assert.deepEqual(after.top,moved.top);
  show=false;await draw();show=true;await draw();sameCameras(snapshot(),after);
  design='B';await draw();assert.notDeepEqual(snapshot().top.target,after.top.target);design='A';await draw();sameCameras(snapshot(),after);
  await setup({clear:false});sameCameras(snapshot(),after);assert.equal(JSON.stringify(scene),original);
});
test('maximize/restore, tree selection, fit selected, visibility, layers, locks and resizing use presentation only',async()=>{
  await setup();const before=snapshot();await click(document.querySelector('[aria-label="Maximize Front view"]'));assert.equal(controller.value.maximized_view,'front');assert.equal([...document.querySelectorAll('.thermal-viewport')].filter(p=>p.style.display!=='none').length,1);await click(button('Four views'));sameCameras(snapshot(),before);
  await click([...document.querySelectorAll('[role=treeitem]')].find(b=>b.title.startsWith('window:w')));assert.equal(selected.id,'window:w');await click(button('Fit selected'));near(controller.value.cameras.front.target,[2,-.01,1.5]);
  await click(button('Hide'));assert.equal(objectVisible(scene.objects[2],controller.value,scene.objects),false);await click(button('Show'));await click(button('Isolate'));assert.deepEqual(controller.value.isolated,['window:w']);assert.equal(objectVisible(scene.objects[1],controller.value,scene.objects),false);await click(button('Show all'));
  await click(button('Lock'));assert.deepEqual(lockCalls,[['window:w']]);assert.equal(JSON.stringify(scene),original);
  const separator=document.querySelector('[role=separator]');await act(()=>separator.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true})));assert.equal(controller.value.tree_width,226);await click(button('Reset layout'));assert.equal(controller.value.tree_width,210);assert.equal(controller.value.selected,'window:w');
});
test('keyboard focus preserves native text and search survives view updates without changing model or camera',async()=>{
  await setup();const before=snapshot(),search=document.querySelector('input[type=search]');await act(()=>{Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(search,'glazing');search.dispatchEvent(new Event('input',{bubbles:true}));});assert.equal(document.querySelectorAll('[role=treeitem]').length,1);await act(()=>search.dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true})));sameCameras(snapshot(),before);assert.equal(JSON.stringify(scene),original);
});
test('actual OrbitControls preserve right-orbit/Shift-right-pan, orthographic and middle pan, and left selection navigation boundaries',async()=>{
  await setup();const drag=async(name,button,shiftKey=false)=>{const el=pane(name),bounds=el.getBoundingClientRect();const event=(type,x,y)=>{const e=new MouseEvent(type,{clientX:x,clientY:y,button,buttons:type==='pointerup'?0:button===2?2:button===1?4:1,shiftKey,bubbles:true,cancelable:true});Object.defineProperties(e,{pointerId:{value:17},pointerType:{value:'mouse'}});return e;};await act(()=>el.dispatchEvent(event('pointerdown',bounds.left+120,bounds.top+100)));await act(()=>document.dispatchEvent(event('pointermove',bounds.left+165,bounds.top+125)));await act(()=>document.dispatchEvent(event('pointerup',bounds.left+165,bounds.top+125)));};
  let before=snapshot();await drag('Perspective',0);sameCameras(snapshot(),before);
  await drag('Perspective',2);let after=snapshot();assert.notDeepEqual(after.perspective.position,before.perspective.position);near(after.perspective.target,before.perspective.target);near(after.top.position,before.top.position);
  const offset=c=>c.position.map((p,i)=>p-c.target[i]);before=after;await drag('Perspective',2,true);after=snapshot();assert.notDeepEqual(after.perspective.target,before.perspective.target);near(offset(after.perspective),offset(before.perspective));
  for(const shift of [false,true]){before=snapshot();await drag('Top',2,shift);after=snapshot();assert.notDeepEqual(after.top.target,before.top.target);near(offset(after.top),offset(before.top));before=after;await drag('Perspective',1,shift);after=snapshot();assert.notDeepEqual(after.perspective.target,before.perspective.target);near(offset(after.perspective),offset(before.perspective));}
  assert.equal(JSON.stringify(scene),original);
});
test('mounted fit commands use current view and selection, clear on unmount and never replay on return',async()=>{
  await setup();const start=registrations.length;assert.equal(typeof commands.fitModel,'function');await click([...document.querySelectorAll('[role=treeitem]')].find(b=>b.title.startsWith('window:w')));await act(()=>pane('Front').focus());await act(()=>commands.fitSelected());near(controller.value.cameras.front.target,[2,-.01,1.5]);assert.equal(registrations.length,start);
  await act(()=>pane('Front').dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true,cancelable:true})));const moved=snapshot();show=false;await draw();assert.equal(commands,null);show=true;await draw();sameCameras(snapshot(),moved);assert.equal(typeof commands.fitModel,'function');await act(()=>commands.fitModel());near(controller.value.cameras.front.target,[4,-.005,1.5]);
});
test('explicit fit while scene is loading waits once for that mount and is discarded on unmount',async()=>{
  await setup();await act(()=>pane('Top').focus());await act(()=>pane('Top').dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true,cancelable:true})));const moved=snapshot();shownScene=null;await draw();await act(()=>commands.fitModel());sameCameras(snapshot(),moved);shownScene=scene;await draw();assert.notDeepEqual(snapshot().top.target,moved.top.target);
  await act(()=>pane('Top').dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true,cancelable:true})));const again=snapshot();shownScene=null;await draw();await act(()=>commands.fitModel());show=false;await draw();show=true;shownScene=scene;await draw();sameCameras(snapshot(),again);
});
test('bounded presentation cache rejects malformed cameras and isolates document/design keys',()=>{
  const p=defaultThermalPresentation();assert.equal(validThermalPresentation(p),true);p.cameras.top={position:[0,0,0],target:[0,0,0],up:[0,0,1],zoom:1};assert.equal(validThermalPresentation(p),false);
  const invalid=new ThermalPresentationStore({getItem:()=>JSON.stringify([['["p","A"]',p]]),setItem(){}});assert.equal(invalid.entries.size,0);assert.match(invalid.warning,/invalid/);
  let saved;const store=new ThermalPresentationStore({getItem:()=>null,setItem(k,v){saved=v;}});const key='["p","A"]';store.set(key,{...defaultThermalPresentation(),selected:'window:w'});assert.equal(store.get('["p","B"]').selected,null);const recovered=new ThermalPresentationStore({getItem:()=>saved,setItem(){}});assert.equal(recovered.get(key).selected,'window:w');
  const quota=new ThermalPresentationStore({getItem:()=>null,setItem(){throw Error('quota');}});quota.set(key,defaultThermalPresentation());assert.match(quota.warning,/this window/);
});
test('Properties resize keyboard/pointer bounds persist separately and unmount detaches active drag',async()=>{
  await setup();const separator=()=>document.querySelector('[aria-label="Properties panel width"]');await act(()=>separator().dispatchEvent(new KeyboardEvent('keydown',{key:'ArrowLeft',bubbles:true})));assert.equal(controller.value.inspector_width,340);
  const pointer=(type,x)=>{const e=new MouseEvent(type,{clientX:x,button:0,bubbles:true});Object.defineProperty(e,'pointerId',{value:4});return e;};await act(()=>separator().dispatchEvent(pointer('pointerdown',600)));await act(()=>window.dispatchEvent(pointer('pointermove',-500)));assert.equal(controller.value.inspector_width,520);await act(()=>window.dispatchEvent(pointer('pointerup',-500)));
  show=false;await draw();show=true;await draw();assert.equal(controller.value.inspector_width,520);design='B';await draw();assert.equal(controller.value.inspector_width,320);design='A';await draw();assert.equal(controller.value.inspector_width,520);
  await act(()=>separator().dispatchEvent(pointer('pointerdown',500)));show=false;await draw();await act(()=>window.dispatchEvent(pointer('pointermove',1000)));assert.equal(controller.value.inspector_width,520);show=true;await draw();await click(button('Reset layout'));assert.equal(controller.value.inspector_width,320);assert.equal(JSON.stringify(scene),original);
});
test.after(async()=>{if(root)await act(()=>root.unmount());assert.equal(rendererCount,disposeCount);dom.window.close();});
