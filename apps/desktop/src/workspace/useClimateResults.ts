import { useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ClimateComparison, ClimateProjectDocument, ClimateRunRecord, ClimateSensorSeries, ClimateValues, CurrentClimateResult, ResultField, Study, WeatherIntervalView } from '../climate-types';
import type { ValidatedModel } from '../types';
import { climateOverlayMatches, climateRequestKey, ClimateRequestGate } from './climate-presentation.mjs';
import { climateRelationKey, selectedRecordProject, selectedPairProject, selectedPairRelation, snapshotDisplayContext, snapshotDisplayScale } from './climate-pair-context.mjs';

interface Options {
  documentId:number; path:string; project:ClimateProjectDocument|null; alternative:string;
  study:Study|null; current:CurrentClimateResult|null; records:ClimateRunRecord[];
  validated:ValidatedModel|null; selection:string[];
}
function useWeatherView(path:string,project:ClimateProjectDocument|null,run:ClimateRunRecord|null,interval:number) {
  const weatherId=project?.climate?.weather.find(w=>w.binding.original_sha256===run?.weather_asset.binding.original_sha256&&w.binding.normalized_sha256===run?.weather_asset.binding.normalized_sha256&&w.binding.time_axis_sha256===run?.weather_asset.binding.time_axis_sha256)?.id??null;
  const key=JSON.stringify([path,weatherId,interval]);
  const [state,setState]=useState<{key:string;value:WeatherIntervalView|null;error:string}|null>(null);
  useEffect(()=>{let live=true;if(!path||!weatherId)return;const timer=window.setTimeout(()=>{
    invoke<WeatherIntervalView>('weather_interval',{path,weatherId,intervalIndex:interval}).then(value=>live&&setState({key,value,error:''})).catch(e=>live&&setState({key,value:null,error:String(e)}));
  },70);return()=>{live=false;window.clearTimeout(timer);};},[key]);
  return state?.key===key?state:null;
}
export function useClimateResults(options:Options) {
  const {documentId,path,project,alternative,study,current,records,validated,selection}=options;
  const context=`${documentId}:${alternative}`;
  const [display,setDisplay]=useState({context,runId:'current',field:'illuminance' as ResultField,interval:1908});
  const view=display.context===context?display:{context,runId:'current',field:'illuminance' as ResultField,interval:1908};
  const update=(patch:Partial<typeof display>)=>setDisplay({...view,...patch,context});
  const run=view.runId==='current'?current?.run??null:records.find(r=>r.id===view.runId&&r.alternative_id===alternative)??null;
  const summary=project?.climate?.runs.find(r=>r.id===run?.id)??null;
  const {annual,field,interval}=snapshotDisplayContext(run,view);
  const settings=view.runId==='current'&&study?study.metrics:run?.study.metrics??null;
  const key=climateRequestKey(path,summary,field,interval,settings);
  const [response,setResponse]=useState<{key:string;value:ClimateValues|null;error:string}|null>(null);
  const gate=useRef(new ClimateRequestGate());
  useEffect(()=>{
    const request=gate.current;request.cancel();
    if(!path||!summary)return;
    // While scrubbing, coalesce disk reads without changing the calculation.
    const timer=window.setTimeout(()=>{void request.read(()=>invoke<ClimateValues>('climate_result_values',{path,summary,field,intervalIndex:interval,settings}),
      result=>result.run_id===summary.id&&result.field===field&&(field!=='illuminance'||result.interval_index===interval))
      .then(result=>{if(result)setResponse({key,...result});});},70);
    return()=>{window.clearTimeout(timer);request.cancel();};
  },[key]);
  const result=response?.key===key?response:null;
  const values=result?.value??null;
  // A compatible older snapshot may overlay; the newest run's identity cannot
  // establish that relationship. Rust checks this one immutable record against
  // the current study, including the complete profile and engine method.
  const selectedProject=selectedRecordProject(project,summary);
  const relationKey=JSON.stringify([documentId,view.runId,alternative,study,climateRelationKey(path,selectedProject)]);
  const [relation,setRelation]=useState<{key:string;value:CurrentClimateResult|null;error:string}|null>(null);
  useEffect(()=>{let live=true;if(view.runId==='current'||!path||!summary||!selectedProject?.climate||!study)return;
    invoke<CurrentClimateResult>('current_climate_result',{path,project:selectedProject,alternativeId:alternative,study})
      .then(value=>live&&setRelation({key:relationKey,value,error:''})).catch(e=>live&&setRelation({key:relationKey,value:null,error:String(e)}));
    return()=>{live=false;};},[relationKey]);
  const selectedResponse=relation?.key===relationKey?relation:null;
  const selectedCondition=view.runId==='current'?current:selectedResponse?.value??null;
  const selectedRelation=selectedPairRelation(summary,selectedCondition??undefined,selectedResponse?.error);
  const canOverlay=climateOverlayMatches(summary,selectedCondition,values,validated);
  const weatherView=useWeatherView(path,project,run,interval);
  const sensorId=selection.find(id=>id.startsWith('sensor:'))?.slice(7)??null;
  const seriesKey=JSON.stringify([path,summary?.id,summary?.record_sha256,sensorId]);
  const [series,setSeries]=useState<{key:string;value:ClimateSensorSeries|null;error:string}|null>(null);
  useEffect(()=>{let live=true;if(!path||!summary||!sensorId||!run?.series.sensor_ids.includes(sensorId))return;
    invoke<ClimateSensorSeries>('climate_sensor_series',{path,summary,sensorId}).then(value=>{if(live)setSeries({key:seriesKey,value,error:''});}).catch(e=>live&&setSeries({key:seriesKey,value:null,error:String(e)}));
    return()=>{live=false;};},[seriesKey]);
  const sensorSeries=series?.key===seriesKey?series:null;

  const [pair,setPair]=useState<{documentId:number;aId:string|null;bId:string|null}>({documentId,aId:null,bId:null});
  const available=useMemo(()=>project?.climate?.runs??[],[project?.climate?.runs]);
  const selected=pair.documentId===documentId?pair:{documentId,aId:null,bId:null};
  const a=available.find(r=>r.id===selected.aId)??(!selected.aId?[...available].reverse().find(r=>r.alternative_id==='A'):null)??null;
  const b=available.find(r=>r.id===selected.bId)??(!selected.bId?[...available].reverse().find(r=>r.alternative_id==='B'):null)??null;
  useEffect(()=>{setPair({documentId,aId:a?.id??selected.aId,bId:b?.id??selected.bId});},[documentId,a?.id,b?.id]);
  const pairA=records.find(r=>r.id===a?.id)??null;
  const [pairDisplay,setPairDisplay]=useState({documentId,field:'illuminance' as ResultField,interval:1908});
  const pairView=pairDisplay.documentId===documentId?pairDisplay:{documentId,field:'illuminance' as ResultField,interval:1908};
  const {annual:pairAnnual,field:pairField,interval:pairInterval}=snapshotDisplayContext(pairA,pairView);
  const pairWeather=useWeatherView(path,project,pairA,pairInterval);
  const pairKey=JSON.stringify([path,a?.id,a?.record_sha256,b?.id,b?.record_sha256,pairField,pairInterval]);
  const [comparisonState,setComparison]=useState<{key:string;value:ClimateComparison|null;error:string}|null>(null);
  const pairGate=useRef(new ClimateRequestGate());
  useEffect(()=>{const requests=pairGate.current;requests.cancel();if(!path||!a||!b)return;
    const timer=window.setTimeout(()=>{void requests.read(()=>invoke<ClimateComparison>('climate_compare',{path,a,b,field:pairField,intervalIndex:pairInterval}),
      value=>value.a.run_id===a.id&&value.b.run_id===b.id&&value.a.field===pairField&&value.b.field===pairField&&
        (pairField!=='illuminance'||value.a.interval_index===pairInterval&&value.b.interval_index===pairInterval))
      .then(value=>value&&setComparison({key:pairKey,...value}));},90);
    return()=>{window.clearTimeout(timer);requests.cancel();};},[pairKey]);
  const pairResponse=comparisonState?.key===pairKey?comparisonState:null;
  const selectPair=(side:'A'|'B',id:string)=>setPair({documentId,aId:side==='A'?id:a?.id??null,bId:side==='B'?id:b?.id??null});
  // This projection is used only for a read: the shared Rust lookup must bind
  // metric identity to the selected raw series, not the newest compatible run.
  const pairProject=selectedPairProject(project,a,b);
  const conditionsKey=climateRelationKey(path,pairProject);
  const [conditions,setConditions]=useState<{key:string;values:Record<string,CurrentClimateResult>;error:string}|null>(null);
  useEffect(()=>{let live=true;if(!path||!pairProject?.climate)return;
    Promise.all((['A','B'] as const).map(async id=>[id,await invoke<CurrentClimateResult>('current_climate_result',{path,project:pairProject,alternativeId:id,study:pairProject.climate!.studies[id]})] as const))
      .then(entries=>live&&setConditions({key:conditionsKey,values:Object.fromEntries(entries),error:''})).catch(e=>live&&setConditions({key:conditionsKey,values:{},error:String(e)}));
    return()=>{live=false;};},[conditionsKey]);
  const activeConditions=conditions?.key===conditionsKey?conditions:null;
  const pairRelation=(side:'A'|'B')=>selectedPairRelation(side==='A'?a:b,activeConditions?.values[side],activeConditions?.error);
  return {path,run,summary,selectedRunId:view.runId,selectRun:(runId:string)=>update({runId}),field,setField:(field:ResultField)=>update({field}),
    interval,setInterval:(interval:number)=>update({interval}),annual,values,canOverlay,selectedRelation,loading:Boolean(key&&!result),error:result?.error??'',
    scale:snapshotDisplayScale(field,values),
    weather:weatherView?.value??null,weatherError:weatherView?.error??'',sensorSeries:sensorSeries?.value??null,seriesError:sensorSeries?.error??'',sensorId,
    pairTime:{path,annual:pairAnnual,field:pairField,interval:pairInterval,setInterval:(interval:number)=>setPairDisplay({...pairView,documentId,interval}),weather:pairWeather?.value??null,weatherError:pairWeather?.error??''},
    setPairField:(field:ResultField)=>setPairDisplay({...pairView,documentId,field}),pairRelation,
    comparison:pairResponse?.value??null,comparisonError:pairResponse?.error??'',comparing:Boolean(a&&b&&!pairResponse),a,b,selectPair};
}
export type ClimateResultController=ReturnType<typeof useClimateResults>;
