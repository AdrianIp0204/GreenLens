// Render production table handlers without a DOM. Service results are synthetic;
// this verifies selection routing, never native UI or scientific acceptance.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { transformWithOxc } from 'vite';

const compiled=new Map();
async function component(path){
  const url=new URL(path,import.meta.url);
  if(compiled.has(url.href))return compiled.get(url.href);
  const {code}=await transformWithOxc(fs.readFileSync(url,'utf8'),url.pathname,{jsx:{runtime:'automatic'}});
  let linked=code.replace(/import\s+['"][^'"]+\.css['"];?/g,'');
  for(const match of [...linked.matchAll(/from\s+(['"])([^'"]+)\1/g)]){
    const specifier=match[2];let target;
    if(specifier.endsWith('/WeatherCalendarPicker'))target='data:text/javascript,export function WeatherCalendarPicker(){return null}';
    else if(specifier.endsWith('/AnalysisContent'))target=await component('../src/workspace/AnalysisContent.tsx');
    else if(specifier.endsWith('/QualificationStatus'))target=await component('../src/workspace/QualificationStatus.tsx');
    else target=specifier.startsWith('.')?new URL(specifier,url).href:import.meta.resolve(specifier);
    linked=linked.replace(match[0],`from ${JSON.stringify(target)}`);
  }
  const result=`data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`;compiled.set(url.href,result);return result;
}
const {default:AnalysisContent}=await import(await component('../src/workspace/AnalysisContent.tsx'));
const {ClimateResultsContent,ClimateResultControls}=await import(await component('../src/workspace/ClimateResultsContent.tsx'));
const fixture=JSON.parse(fs.readFileSync(new URL('./viewport-fixture.json',import.meta.url)));
const id=fixture.sensor_grid.sensors[0].id;
const run={id:'historical',alternative_id:'A',validated:fixture,lux:fixture.sensor_grid.sensors.map(()=>100)};
const values={values:run.lux,units:'lx'};
function nodes(tree){return !tree||typeof tree!=='object'?[]:Array.isArray(tree)?tree.flatMap(nodes):[tree,...nodes(tree.props?.children)];}
function sensorButton(tree){return nodes(tree).find(n=>n.type==='button'&&n.props.children===id);}

test('static snapshot table disables viewport selection and selection highlighting when historical',()=>{
  const selected=[];
  const props={tab:'table',validated:fixture,current:run,selection:[`sensor:${id}`],onSelect:ids=>selected.push(ids)};
  const historical=AnalysisContent({...props,canSelectSensors:false});const disabled=sensorButton(historical);
  assert.equal(disabled.props.disabled,true);assert.match(disabled.props.title,/Historical sensor positions/);
  disabled.props.onClick();assert.deepEqual(selected,[]);
  assert.equal(nodes(historical).some(n=>n.type==='tr'&&n.props.className==='selected-row'),false);
  const current=AnalysisContent({...props,canSelectSensors:true});const enabled=sensorButton(current);
  assert.equal(enabled.props.disabled,false);enabled.props.onClick();assert.deepEqual(selected,[[`sensor:${id}`]]);
  assert.equal(nodes(current).some(n=>n.type==='tr'&&n.props.className==='selected-row'),true);
});

test('climate table does not translate a historical stable ID into a current viewport point',()=>{
  const selected=[];
  const props={tab:'table',records:[run],selection:[`sensor:${id}`],onSelect:ids=>selected.push(ids)};
  const controller={run,values,field:'illuminance',canOverlay:false};
  const historical=ClimateResultsContent({...props,controller});const disabled=sensorButton(historical);
  assert.equal(disabled.props.disabled,true);disabled.props.onClick();assert.deepEqual(selected,[]);
  assert.equal(nodes(historical).some(n=>n.type==='tr'&&n.props.className==='selected-row'),false);
  const current=ClimateResultsContent({...props,controller:{...controller,canOverlay:true}});
  sensorButton(current).props.onClick();assert.deepEqual(selected,[[`sensor:${id}`]]);
});

test('historical series uses a separate saved-coordinate choice without viewport mutation',()=>{
  const local=[],viewport=[];
  const tree=ClimateResultsContent({tab:'series',records:[run],selection:[`sensor:${id}`],onSelect:ids=>viewport.push(ids),controller:{run,values,canOverlay:false,sensorId:null,selectHistoricalSensor:value=>local.push(value)}});
  const choice=nodes(tree).find(n=>n.props?.['aria-label']==='Historical time-series sensor');assert.ok(choice);
  const option=nodes(choice).find(n=>n.type==='option'&&n.props.value===id);assert.ok(option);
  assert.match(option.props.children.join(''),/ m$/);choice.props.onChange({target:{value:id}});
  assert.deepEqual(local,[id]);assert.deepEqual(viewport,[]);
  assert.equal(nodes(ClimateResultsContent({tab:'series',records:[run],selection:[],onSelect(){},controller:{run,values,canOverlay:true,matchesCurrentGeometry:true}})).some(n=>n.props?.['aria-label']==='Historical time-series sensor'),false);
});

test('production result and table captions follow only the displayed metric context',()=>{
  const metricSettings={da_threshold_lux:500,udi_lower_lux:200,udi_upper_lux:2000,occupancy:{kind:'daily',start_minute:540,end_minute:1020}};
  const controller={run,values,field:'da',metricSettings,selectedRunId:'current',canOverlay:true,scale:100};
  const control=ClimateResultControls({controller,records:[],alternative:'A'});
  assert.match(nodes(control).find(n=>n.props?.['aria-label']==='Displayed metric conventions').props.children,/E ≥ 500 lx.*09:00–17:00/);
  const table=ClimateResultsContent({tab:'table',controller,records:[],selection:[],onSelect(){}});
  assert.match(nodes(table).find(n=>n.props?.['aria-label']==='Table metric conventions').props.children,/E ≥ 500 lx/);
  const loading=ClimateResultControls({controller:{...controller,values:null},records:[],alternative:'A'});
  assert.equal(nodes(loading).some(n=>n.props?.['aria-label']==='Displayed metric conventions'),false,'no caption accompanies a superseded response');
  const hourly=ClimateResultsContent({tab:'table',controller:{...controller,field:'illuminance'},records:[],selection:[],onSelect(){}});
  assert.equal(nodes(hourly).some(n=>n.props?.['aria-label']==='Table metric conventions'),false);
});

test('comparison captions use the compatible historical pair, not current study conventions',()=>{
  const saved={da_threshold_lux:300,udi_lower_lux:100,udi_upper_lux:3000,occupancy:{kind:'daily',start_minute:480,end_minute:1080}};
  const a={...run,series:{interval_count:8760},study:{metrics:saved}},b={...a,id:'historical-B',alternative_id:'B'};
  const comparison={a:values,b:values,scale_maximum:100,delta_units:'percentage points',delta_statistics:{min:0,mean:0,max:0},deltas:[]};
  const controller={a,b,comparison,pairTime:{field:'udi',annual:true},pairRelation:()=>'',metricSettings:{...saved,udi_upper_lux:1500}};
  const props={tab:'compare',controller,records:[a,b],selection:[],onSelect(){}};
  const tree=ClimateResultsContent(props);
  assert.match(nodes(tree).find(n=>n.props?.['aria-label']==='Comparison metric conventions').props.children,/100 lx ≤ E ≤ 3000 lx/);
  const pending=ClimateResultsContent({...props,controller:{...controller,comparison:null}});
  assert.equal(nodes(pending).some(n=>n.props?.['aria-label']==='Comparison metric conventions'),false,'unvalidated pairs have no accepted convention caption');
});
