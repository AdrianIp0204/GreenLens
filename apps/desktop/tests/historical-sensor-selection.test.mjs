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
    else target=specifier.startsWith('.')?new URL(specifier,url).href:import.meta.resolve(specifier);
    linked=linked.replace(match[0],`from ${JSON.stringify(target)}`);
  }
  const result=`data:text/javascript;base64,${Buffer.from(linked).toString('base64')}`;compiled.set(url.href,result);return result;
}
const {default:AnalysisContent}=await import(await component('../src/workspace/AnalysisContent.tsx'));
const {ClimateResultsContent}=await import(await component('../src/workspace/ClimateResultsContent.tsx'));
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
  assert.equal(nodes(ClimateResultsContent({tab:'series',records:[run],selection:[],onSelect(){},controller:{run,values,canOverlay:true}})).some(n=>n.props?.['aria-label']==='Historical time-series sensor'),false);
});
