import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { WeatherIntervalView } from '../climate-types';

/** Calendar lookup uses the retained Rust time axis, never the browser's timezone. */
export function WeatherCalendarPicker({path,weatherId,current,disabled=false,onSelect}:{
  path:string;weatherId:string|null;current:WeatherIntervalView|null;disabled?:boolean;onSelect:(index:number)=>void;
}) {
  const [month,setMonth]=useState('6'),[day,setDay]=useState('21'),[hour,setHour]=useState('12');
  const [error,setError]=useState(''),[busy,setBusy]=useState(false);
  const context=JSON.stringify([path,weatherId,current?.interval.index,disabled]);
  const latest=useRef(context),generation=useRef(0);
  latest.current=context;
  useEffect(()=>{++generation.current;setBusy(false);setError('');if(current&&current.weather_id===weatherId){setMonth(String(current.hour.month));setDay(String(current.hour.day));setHour(String(current.hour.hour_ending));}},[context]);
  const find=async()=>{
    if(!path||!weatherId||disabled)return;
    const requested=context,token=++generation.current;
    const numbers=[month,day,hour].map(value=>value.trim()?Number(value):NaN);
    if(numbers.some(value=>!Number.isInteger(value))){setError('Enter a whole month, day and hour ending.');return;}
    setBusy(true);setError('');
    try{
      const value=await invoke<WeatherIntervalView>('weather_find_interval',{path,weatherId,month:numbers[0],day:numbers[1],hourEnding:numbers[2]});
      if(latest.current===requested&&generation.current===token&&value.weather_id===weatherId)onSelect(value.interval.index);
    }catch(reason){if(latest.current===requested&&generation.current===token)setError(String(reason));}
    finally{if(latest.current===requested&&generation.current===token)setBusy(false);}
  };
  return <div className="weather-calendar"><div className="weather-calendar-fields"><label>Month<input aria-label="Weather month" inputMode="numeric" value={month} disabled={disabled||busy} onChange={e=>setMonth(e.target.value)}/></label><label>Day<input aria-label="Weather day" inputMode="numeric" value={day} disabled={disabled||busy} onChange={e=>setDay(e.target.value)}/></label><label>Hour ending<input aria-label="Weather hour ending" inputMode="numeric" value={hour} disabled={disabled||busy} onChange={e=>setHour(e.target.value)}/></label><button type="button" disabled={disabled||busy||!weatherId} onClick={()=>void find()}>{busy?'Finding…':'Go'}</button></div><small>Fixed local standard time. Hour 24 ends at the next midnight.</small>{error&&<p role="alert" className="climate-error">{error}</p>}</div>;
}
