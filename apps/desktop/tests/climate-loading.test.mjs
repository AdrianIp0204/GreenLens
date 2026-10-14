// Actual document/result hooks and render branches; native replies are explicitly
// mocked. These exercise pending reads, not native UI or scientific correctness.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { transformWithOxc } from 'vite';

const fixture=JSON.parse(fs.readFileSync(new URL('./viewport-fixture.json',import.meta.url)));
const states=[],effects=[];let cursor=0,pending=[],reply,calls=[];
const host={
  useState(initial){const i=cursor++;if(!(i in states))states[i]=typeof initial==='function'?initial():initial;return [states[i],v=>{states[i]=typeof v==='function'?v(states[i]):v;}];},
  useRef(initial){const i=cursor++;return states[i]??(states[i]={current:initial});},
  useMemo(fn,deps){const i=cursor++,old=states[i];if(!old||deps.some((v,j)=>!Object.is(v,old.deps[j])))states[i]={deps,value:fn()};return states[i].value;},
  useCallback(fn,deps){return host.useMemo(()=>fn,deps);},
  useEffect(fn,deps){const i=cursor++,old=effects[i];if(!old||deps.some((v,j)=>!Object.is(v,old.deps[j]))){old?.cleanup?.();effects[i]={deps};pending.push(()=>{effects[i].cleanup=fn();});}},
  invoke(command,args){calls.push({command,args});return reply(command,args);},isTauri:()=>true,
  getCurrentWindow:()=>({onCloseRequested:async()=>()=>{}}),
  // Climate fixtures have no thermal drafts; their lifecycle is covered by real DOM tests.
  useEnergyDrafts:()=>({beforeAction:async()=>true}),
  useThermalPresentation:()=>({}),
  defaultDisplay:()=>({layers:{},objects:{},cameras:{}}),
  fmt:String,ClimateTimeControl:()=>null,ClimateLegend:()=>null,
};
globalThis.__greenlensLoadingHost=host;
globalThis.window={setTimeout,clearTimeout,setInterval,clearInterval};
async function load(relative){
  const url=new URL(relative,import.meta.url);
  const {code}=await transformWithOxc(fs.readFileSync(url,'utf8'),url.pathname,{jsx:{runtime:'automatic'}});
  const linked=code.replace(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g,(_,binding,_q,specifier)=>{
    if(['react','@tauri-apps/api/core','@tauri-apps/api/window','../viewport/types','../energy/useEnergyDrafts','../energy/thermal-presentation','./AnalysisContent'].includes(specifier))return `const ${binding.replace(/\bas\b/g,':')}=globalThis.__greenlensLoadingHost;`;
    const target=specifier.startsWith('.')?new URL(specifier,url).href:import.meta.resolve(specifier);
    if(specifier==='./QualificationStatus')return `const QualificationStatus=globalThis.__greenlensLoadingHost.QualificationStatus;`;
    if(specifier==='./WeatherCalendarPicker')return `const WeatherCalendarPicker=globalThis.__greenlensLoadingHost.WeatherCalendarPicker;`;
    return `import ${binding} from ${JSON.stringify(target)};`;
  }).replace(/import\s+['"][^'"]+\.css['"];?/g,'');
  return import(`data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`);
}
const {useDocument}=await load('../src/workspace/useDocument.ts');
const {useClimateResults,useClimateAssessment}=await load('../src/workspace/useClimateResults.ts');
host.QualificationStatus=(await load('../src/workspace/QualificationStatus.tsx')).QualificationStatus;
const {ClimateResultsContent}=await load('../src/workspace/ClimateResultsContent.tsx');
const {WeatherCalendarPicker}=await load('../src/workspace/WeatherCalendarPicker.tsx');
host.WeatherCalendarPicker=WeatherCalendarPicker;
const deferred=()=>{let resolve,reject;return {promise:new Promise((a,b)=>{resolve=a;reject=b;}),resolve:v=>resolve(v),reject:e=>reject(e)};};
function reset(){for(const e of effects)e?.cleanup?.();states.length=0;effects.length=0;pending=[];calls=[];}
function render(fn){cursor=0;const value=fn();const work=pending;pending=[];work.forEach(fn=>fn());return value;}
async function flush(fn){let value;for(let i=0;i<12;i++){await Promise.resolve();value=render(fn);}return value;}
const pause=()=>new Promise(resolve=>setTimeout(resolve,100));
const study={calculation:{mode:'annual',weather:{original_sha256:'weather'}},metrics:{da_threshold_lux:300}};
const summary={id:'run-A',alternative_id:'A',record_sha256:'sealed',physical_input_hash:'physics',path:'runs/run-A'};
const run={id:'run-A',alternative_id:'A',study,validated:fixture,series:{sensor_ids:fixture.sensor_grid.sensors.map(s=>s.id),interval_count:8760}};
const condition={run,summary,physical_input_hash:'physics',metrics_current:true};
const project={schema_version:'0.3.0',id:'document',name:'Mock document',alternatives:[{id:'A',model:fixture.model}],runs:[],climate:{weather:[{id:'weather'}],studies:{A:study},runs:[summary]}};
let revision=0;
const projectView=p=>({project:structuredClone(p),revision:String(++revision),results:[],climate_results:[run]});

test('qualification read survives display-only changes but invalidates edits and ignores late replies',async()=>{
  reset();const old=deferred(),fresh=deferred();let index=0;
  reply=(command)=>{assert.equal(command,'climate_assessment');return ++index===1?old.promise:fresh.promise;};
  let currentProject=structuredClone(project);
  const draw=()=>useClimateAssessment('saved-project',currentProject,summary);
  let result=render(draw);assert.equal(result.assessment,null);
  currentProject={...currentProject,display_state:{cameras:{top:'changed'},selected:'window'}};
  result=render(draw);assert.equal(index,1,'display state does not request another assessment');
  currentProject={...currentProject,alternatives:[{...currentProject.alternatives[0],model:{...fixture.model,name:'edited context'}}]};
  result=render(draw);assert.equal(index,2);assert.equal(result.assessment,null);
  const assessed={run_id:summary.id,record_sha256:summary.record_sha256,scientific_qualification:{status:'failed'}};
  fresh.resolve(assessed);result=await flush(draw);assert.equal(result.assessment,assessed);
  old.resolve({...assessed,scientific_qualification:{status:'limited_qualified'}});
  result=await flush(draw);assert.equal(result.assessment,assessed,'obsolete success cannot replace the fresh failure');
});
test('qualification never displays another immutable record or a failed assessment request',async()=>{
  for(const response of [{run_id:summary.id,record_sha256:'wrong-seal'},new Error('Retained metadata checksum mismatch')]){
    reset();reply=async()=>{if(response instanceof Error)throw response;return response;};
    const draw=()=>useClimateAssessment('saved-project',project,summary);
    render(draw);const result=await flush(draw);
    assert.equal(result.assessment,null);assert.match(result.error,/selected immutable snapshot|checksum mismatch/);
  }
});
const values=interval=>({run_id:run.id,field:'illuminance',interval_index:interval,sensor_ids:run.series.sensor_ids,values:run.series.sensor_ids.map(()=>interval),statistics:{min:interval,mean:interval,max:interval},units:'lx'});
const weather=interval=>({run_id:run.id,record_sha256:summary.record_sha256,interval:{index:interval},hour:{index:interval,month:3,day:21,hour_ending:interval===1919?24:13}});
function nodes(tree){return !tree||typeof tree!=='object'?[]:Array.isArray(tree)?tree.flatMap(nodes):[tree,...nodes(tree.props?.children)];}

async function openMock(){
  reset();reply=async(command,args)=>{
    if(command==='example')return structuredClone(project);
    if(command==='study_defaults')return study;
    if(command==='validate')return {...fixture,model:args.model};
    if(command==='study_validate')return {};
    if(command==='dialog_save_project')return 'saved-project';
    if(command==='save_project_as'||command==='save_project')return projectView(args.project);
    if(command==='current_climate_result')return condition;
    throw new Error(`Unexpected mocked command ${command}`);
  };
  let doc=await flush(useDocument);await doc.save();doc=await flush(useDocument);
  assert.equal(doc.currentClimate.run.id,run.id);return doc;
}

test('save/export refresh preserves same scientific result while native destination dialog is pending',async()=>{
  let doc=await openMock();const before=doc.currentClimate,lookup=deferred(),dialog=deferred();
  const previous=reply;reply=(command,args)=>command==='current_climate_result'?lookup.promise:command==='dialog_export_evidence'?dialog.promise:previous(command,args);
  const exported=doc.exportEvidence([run.id]);doc=await flush(useDocument);
  assert.equal(doc.currentClimate,before,'revision-only refresh must retain authoritative same-input result');
  assert.equal(doc.validating,false,'saving identical scientific inputs must not blank validated geometry');
  assert.ok(calls.some(c=>c.command==='dialog_export_evidence'));
  dialog.resolve(null);await exported;lookup.resolve(condition);await flush(useDocument);
});

test('accepted geometry changes and a failed refresh never retain the previous current projection',async()=>{
  let doc=await openMock();const next=deferred(),previous=reply;
  reply=(command,args)=>command==='current_climate_result'?next.promise:previous(command,args);
  const edited=structuredClone(doc.model);edited.room.width_m+=.1;
  await doc.commit(edited,'Width');doc=await flush(useDocument);
  assert.equal(doc.currentClimate,null);
  next.reject('Retained record failed verification');doc=await flush(useDocument);
  assert.equal(doc.currentClimate,null);assert.match(doc.error,/failed verification/);
});

test('same-context refresh error invalidates the retained current result',async()=>{
  let doc=await openMock();const next=deferred(),previous=reply;
  reply=(command,args)=>command==='current_climate_result'?next.promise:previous(command,args);
  await doc.save();doc=await flush(useDocument);next.reject('Checksum mismatch');doc=await flush(useDocument);
  assert.equal(doc.currentClimate,null);assert.match(doc.error,/Checksum mismatch/);
});

test('metric edits, replaced seals and Save As paths hide the old projection until Rust responds',async()=>{
  for(const kind of ['metrics','seal','path']){
    let doc=await openMock();const next=deferred(),previous=reply;
    reply=(command,args)=>{
      if(command==='current_climate_result')return next.promise;
      if(command==='study_configure')return Promise.resolve({...args.project,climate:{...args.project.climate,studies:{A:args.study}}});
      if(command==='dialog_save_project')return Promise.resolve('different-copy');
      if(command==='save_project'&&kind==='seal'){const changed=structuredClone(args.project);changed.climate.runs[0].record_sha256='new-seal';return Promise.resolve(projectView(changed));}
      return previous(command,args);
    };
    if(kind==='metrics')await doc.commitStudy({...study,metrics:{da_threshold_lux:600}});
    else if(kind==='path')await doc.saveAs();
    else await doc.save();
    doc=await flush(useDocument);assert.equal(doc.currentClimate,null,kind);assert.equal(doc.currentClimateLoading,true,kind);
    next.resolve({...condition,run:null,summary:null});doc=await flush(useDocument);
    assert.equal(doc.currentClimate.run,null);assert.equal(doc.currentClimateLoading,false);
  }
});

test('late pre-edit refresh cannot reinstate an older current result',async()=>{
  let doc=await openMock();const old=deferred(),fresh=deferred(),previous=reply;let lookups=0;
  reply=(command,args)=>command==='current_climate_result'?(++lookups===1?old.promise:fresh.promise):previous(command,args);
  await doc.save();doc=await flush(useDocument);
  const edited=structuredClone(doc.model);edited.room.width_m+=.1;
  await doc.commit(edited,'Width');doc=await flush(useDocument);assert.equal(doc.currentClimate,null);
  fresh.resolve({...condition,run:null,summary:null});doc=await flush(useDocument);
  old.resolve(condition);doc=await flush(useDocument);assert.equal(doc.currentClimate.run,null);
});

test('hour-only read preserves scientific relation and selected series but never relabels older values',async()=>{
  reset();let held=null;
  reply=async(command,args)=>{
    if(command==='climate_result_values')return held?held.promise:values(args.intervalIndex);
    if(command==='run_weather_interval')return weather(args.intervalIndex);
    if(command==='climate_sensor_series')return {sensor_id:args.sensorId,values:[1,2],first_interval_index:0};
    if(command==='current_climate_result')return condition;
    throw new Error(`Unexpected mocked command ${command}`);
  };
  const options={documentId:1,path:'saved-project',project,alternative:'A',study,current:condition,records:[run],validated:fixture,selection:[`sensor:${run.series.sensor_ids[0]}`]};
  const draw=()=>useClimateResults(options);render(draw);await pause();let c=await flush(draw);
  assert.equal(c.canOverlay,true);assert.ok(c.sensorSeries);
  held=deferred();c.setInterval(1919);c=await flush(draw);
  assert.equal(c.run.id,run.id);assert.equal(c.values,null);assert.equal(c.canOverlay,false);
  assert.equal(c.matchesCurrentGeometry,true);assert.equal(c.loading,true);
  assert.equal(c.sensorId,run.series.sensor_ids[0]);assert.ok(c.sensorSeries);
  // Production series branch must remain mounted while only hour values load.
  const tree=render(()=>ClimateResultsContent({tab:'series',controller:c,records:[run],selection:options.selection,onSelect(){}}));
  assert.ok(nodes(tree).some(n=>n.type?.name==='ClimateTimeControl'));
  assert.ok(nodes(tree).some(n=>n.props?.role==='status'));
  await pause();held.resolve(values(1919));c=await flush(draw);assert.equal(c.values.interval_index,1919);
});

test('calendar has no invented default date and keeps entered fields while retained hour is pending',()=>{
  reset();const props={path:'saved-project',summary,current:null,onSelect(){}};
  const inputs=tree=>nodes(tree).filter(n=>n.type==='input');
  let tree=render(()=>WeatherCalendarPicker(props));assert.deepEqual(inputs(tree).map(n=>n.props.value),['','','']);
  render(()=>WeatherCalendarPicker({...props,current:weather(1908)}));tree=render(()=>WeatherCalendarPicker({...props,current:weather(1908)}));
  assert.deepEqual(inputs(tree).map(n=>n.props.value),['3','21','13']);
  tree=render(()=>WeatherCalendarPicker(props));assert.deepEqual(inputs(tree).map(n=>n.props.value),['3','21','13']);
  const changed={...props,summary:{...summary,record_sha256:'new-seal'}};
  render(()=>WeatherCalendarPicker(changed));tree=render(()=>WeatherCalendarPicker(changed));assert.deepEqual(inputs(tree).map(n=>n.props.value),['','','']);
});
