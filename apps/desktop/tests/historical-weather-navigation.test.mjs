// Production hooks/calendar handlers with explicit mocked native replies.
// These verify routing and async identity only, not weather parsing or native UI.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { transformWithOxc } from 'vite';

const states=[],effects=[];let cursor=0,pending=[],calls=[],reply;
const host={
  useState(initial){const i=cursor++;if(!(i in states))states[i]=typeof initial==='function'?initial():initial;return [states[i],v=>{states[i]=typeof v==='function'?v(states[i]):v;}];},
  useRef(initial){const i=cursor++;return states[i]??(states[i]={current:initial});},
  useMemo:fn=>fn(),
  useEffect(fn,deps){const i=cursor++,old=effects[i];if(!old||deps.some((v,j)=>!Object.is(v,old.deps[j]))){old?.cleanup?.();effects[i]={deps};pending.push(()=>{effects[i].cleanup=fn();});}},
  invoke(command,args){calls.push({command,args});return reply(command,args);},
};
globalThis.__greenlensHistoricalWeatherHost=host;
globalThis.window={setTimeout,clearTimeout};
async function load(relative){
  const url=new URL(relative,import.meta.url);
  const {code}=await transformWithOxc(fs.readFileSync(url,'utf8'),url.pathname,{jsx:{runtime:'automatic'}});
  const linked=code.replace(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g,(_,binding,_q,specifier)=>{
    if(['react','@tauri-apps/api/core'].includes(specifier))return `const ${binding.replace(/\bas\b/g,':')}=globalThis.__greenlensHistoricalWeatherHost;`;
    const target=specifier.startsWith('.')?new URL(specifier,url).href:import.meta.resolve(specifier);
    return `import ${binding} from ${JSON.stringify(target)};`;
  });
  return import(`data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`);
}
const {useRunWeatherView}=await load('../src/workspace/useClimateResults.ts');
const {WeatherCalendarPicker}=await load('../src/workspace/WeatherCalendarPicker.tsx');
const summary=(id='old-A')=>({id,alternative_id:'A',path:`runs/${id}`,record_sha256:`seal-${id}`,physical_input_hash:'physics',comparison_fingerprint:'conditions'});
const view=(s,index=8759)=>({run_id:s.id,record_sha256:s.record_sha256,binding:{time_axis_sha256:'axis'},hour:{index,month:12,day:31,hour_ending:24},interval:{index}});
const deferred=()=>{let resolve;return {promise:new Promise(r=>{resolve=r;}),resolve:v=>resolve(v)};};
const pause=()=>new Promise(resolve=>setTimeout(resolve,90));
const settle=async()=>{await Promise.resolve();await Promise.resolve();};
function reset(){for(const e of effects)e?.cleanup?.();states.length=0;effects.length=0;pending=[];calls=[];}
function render(fn){cursor=0;const value=fn();const work=pending;pending=[];work.forEach(fn=>fn());return value;}
function nodes(tree){return !tree||typeof tree!=='object'?[]:Array.isArray(tree)?tree.flatMap(nodes):[tree,...nodes(tree.props?.children)];}
const button=tree=>nodes(tree).find(n=>n.type==='button');

test('historical weather reads bind the immutable run without a current project weather reference',async()=>{
  reset();const s=summary();reply=async()=>view(s);
  assert.equal(render(()=>useRunWeatherView('portable-copy',s,8759)),null);
  await pause();const result=render(()=>useRunWeatherView('portable-copy',s,8759));
  assert.deepEqual(calls,[{command:'run_weather_interval',args:{path:'portable-copy',summary:s,intervalIndex:8759}}]);
  assert.equal(result.value.hour.hour_ending,24);assert.equal(result.value.run_id,s.id);assert.equal(result.error,'');
});

test('changing historical run discards late weather replies even at the same interval',async()=>{
  reset();const a=summary(),b=summary('old-B'),first=deferred(),second=deferred();
  reply=(_cmd,args)=>args.summary.id===a.id?first.promise:second.promise;
  render(()=>useRunWeatherView('copy',a,8759));await pause();
  assert.equal(render(()=>useRunWeatherView('copy',b,8759)),null);await pause();
  second.resolve(view(b));await settle();assert.equal(render(()=>useRunWeatherView('copy',b,8759)).value.run_id,b.id);
  first.resolve(view(a));await settle();assert.equal(render(()=>useRunWeatherView('copy',b,8759)).value.run_id,b.id);
});

test('weather response with a different record pin or interval never becomes a label',async()=>{
  for(const changed of [v=>({...v,record_sha256:'wrong'}),v=>({...v,interval:{index:0}})]){
    reset();const s=summary();reply=async()=>changed(view(s));
    render(()=>useRunWeatherView('copy',s,8759));await pause();
    const result=render(()=>useRunWeatherView('copy',s,8759));assert.equal(result.value,null);assert.match(result.error,/immutable run and interval/);
  }
});

test('historical calendar uses summary-bound lookup and retains hour24 labels',async()=>{
  reset();const s=summary(),selected=[];reply=async()=>view(s);
  const props={path:'copy',summary:s,current:view(s),onSelect:index=>selected.push(index)};
  render(()=>WeatherCalendarPicker(props));const tree=render(()=>WeatherCalendarPicker(props));
  assert.equal(button(tree).props.disabled,false);button(tree).props.onClick();await settle();
  assert.deepEqual(calls,[{command:'run_weather_find_interval',args:{path:'copy',summary:s,month:12,day:31,hourEnding:24}}]);
  assert.deepEqual(selected,[8759]);
});

test('study calendar preserves top-level weather editing service',async()=>{
  reset();const selected=[],current={...view(summary()),weather_id:'weather-asset'};delete current.run_id;delete current.record_sha256;
  reply=async()=>current;const props={path:'copy',weatherId:'weather-asset',current,onSelect:index=>selected.push(index)};
  render(()=>WeatherCalendarPicker(props));button(render(()=>WeatherCalendarPicker(props))).props.onClick();await settle();
  assert.deepEqual(calls,[{command:'weather_find_interval',args:{path:'copy',weatherId:'weather-asset',month:12,day:31,hourEnding:24}}]);
  assert.deepEqual(selected,[8759]);
});

test('calendar ignores late historical selection and rejects a mismatched record reply',async()=>{
  reset();const a=summary(),b=summary('old-B'),pending=deferred(),selected=[];reply=()=>pending.promise;
  const props=s=>({path:'copy',summary:s,current:view(s),onSelect:index=>selected.push(index)});
  render(()=>WeatherCalendarPicker(props(a)));button(render(()=>WeatherCalendarPicker(props(a)))).props.onClick();
  render(()=>WeatherCalendarPicker(props(b)));pending.resolve(view(a));await settle();assert.deepEqual(selected,[]);
  reply=async()=>view(a);button(render(()=>WeatherCalendarPicker(props(b)))).props.onClick();await settle();
  const tree=render(()=>WeatherCalendarPicker(props(b)));assert.deepEqual(selected,[]);
  assert.ok(nodes(tree).some(n=>n.props?.role==='alert'&&String(n.props.children).includes('source snapshot')));
});
