import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ClimateRunSummary, RunWeatherIntervalView, WeatherIntervalView } from '../climate-types';

/** Calendar lookup uses the retained Rust time axis, never the browser's timezone. */
export function WeatherCalendarPicker({path,weatherId=null,summary=null,current,disabled=false,onSelect}:{
  path:string;weatherId?:string|null;summary?:ClimateRunSummary|null;current:WeatherIntervalView|RunWeatherIntervalView|null;disabled?:boolean;onSelect:(index:number)=>void;
}) {
  const [month,setMonth]=useState(''),[day,setDay]=useState(''),[hour,setHour]=useState('');
  const [error,setError]=useState(''),[busy,setBusy]=useState(false);
  const context=JSON.stringify([path,weatherId,summary,current?.interval.index,disabled]);
  const latest=useRef(context),generation=useRef(0);
  latest.current=context;
  const matches=(value:WeatherIntervalView|RunWeatherIntervalView)=>summary
    ?'run_id' in value&&value.run_id===summary.id&&value.record_sha256===summary.record_sha256
    :'weather_id' in value&&value.weather_id===weatherId;
  const sourceContext=JSON.stringify([path,weatherId,summary]);
  useEffect(()=>{setMonth('');setDay('');setHour('');},[sourceContext]);
  useEffect(()=>{++generation.current;setBusy(false);setError('');},[context]);
  // A missing response during an hour read must not invent a date or reset a
  // typed selection. Only a verified response for this source fills the fields.
  useEffect(()=>{if(current&&matches(current)){setMonth(String(current.hour.month));setDay(String(current.hour.day));setHour(String(current.hour.hour_ending));}},[sourceContext,current?.interval.index]);
  const find=async()=>{
    if(!path||(!summary&&!weatherId)||disabled)return;
    const requested=context,token=++generation.current;
    const numbers=[month,day,hour].map(value=>value.trim()?Number(value):NaN);
    if(numbers.some(value=>!Number.isInteger(value))){setError('Enter a whole month, day and hour ending.');return;}
    setBusy(true);setError('');
    try{
      const date={month:numbers[0],day:numbers[1],hourEnding:numbers[2]};
      const value=summary
        ?await invoke<RunWeatherIntervalView>('run_weather_find_interval',{path,summary,...date})
        :await invoke<WeatherIntervalView>('weather_find_interval',{path,weatherId,...date});
      if(latest.current===requested&&generation.current===token){
        if(!matches(value))throw new Error('Weather response does not match the requested source snapshot.');
        onSelect(value.interval.index);
      }
    }catch(reason){if(latest.current===requested&&generation.current===token)setError(String(reason));}
    finally{if(latest.current===requested&&generation.current===token)setBusy(false);}
  };
  return <div className="weather-calendar"><div className="weather-calendar-fields"><label>Month<input aria-label="Weather month" inputMode="numeric" value={month} disabled={disabled||busy} onChange={e=>setMonth(e.target.value)}/></label><label>Day<input aria-label="Weather day" inputMode="numeric" value={day} disabled={disabled||busy} onChange={e=>setDay(e.target.value)}/></label><label>Hour ending<input aria-label="Weather hour ending" inputMode="numeric" value={hour} disabled={disabled||busy} onChange={e=>setHour(e.target.value)}/></label><button type="button" disabled={disabled||busy||(!summary&&!weatherId)} onClick={()=>void find()}>{busy?'Finding…':'Go'}</button></div><small>Fixed local standard time. Hour 24 ends at the next midnight.</small>{error&&<p role="alert" className="climate-error">{error}</p>}</div>;
}
