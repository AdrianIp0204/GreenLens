import {useState} from 'react';
import type {useDocument} from '../workspace/useDocument';
import type {EnergyRows,EnergyTimestamp} from './types';
import {useEnergyRead} from './useEnergyRead';
import {energyRowStart} from './actions.mjs';
const time=(t:EnergyTimestamp)=>`${String(t.month).padStart(2,'0')}/${String(t.day).padStart(2,'0')} ${Math.floor(t.end_minute/60).toString().padStart(2,'0')}:${String(t.end_minute%60).padStart(2,'0')}`;
export default function EnergySeries({doc,runId,intervalCount}:{doc:ReturnType<typeof useDocument>;runId:string;intervalCount:number}){
  const [start,setStart]=useState(0),[draft,setDraft]=useState('0'),[channel,setChannel]=useState('demand'),[error,setError]=useState('');
  const request=useEnergyRead<EnergyRows>('energy_rows',doc.project&&doc.path&&runId?{path:doc.path,project:doc.project,runId,start,count:Math.min(144,intervalCount-start)}:null);
  const result=request.value,index=Number(channel),demand=channel==='demand',selected=!demand?result?.series.channels[index]:null;
  const values=demand?result?.demand_10min.map(r=>r.facility_kw)??[]:result?.rows.map(r=>r[index])??[];
  const labels=demand?result?.demand_10min.map(r=>time(r.timestamp))??[]:values.map((_,i)=>`interval ${start+i}`);
  const unit=demand?'kW':selected?.unit??'',name=demand?'Coincident ten-minute facility demand':selected?.label??'';
  const lo=Math.min(...values),hi=Math.max(...values),span=hi-lo||1;
  const path=values.map((v,i)=>`${i?'L':'M'}${15+i*690/Math.max(1,values.length-1)},${145-(v-lo)*125/span}`).join(' ');
  return <><div className="energy-inline"><label>Start retained interval<input type="number" min="0" max={intervalCount-1} step="1" value={draft} onChange={e=>setDraft(e.target.value)}/></label><button onClick={()=>{try{setStart(energyRowStart(draft,intervalCount));setError('');}catch(e){setError(String(e));}}}>Read up to 144 intervals</button>{result&&<select aria-label="Time series channel" value={channel} onChange={e=>setChannel(e.target.value)}><option value="demand">Facility demand (ten-minute kW)</option>{result.series.channels.map((c,i)=><option value={i} key={c.id}>Raw: {c.label} ({c.unit})</option>)}</select>}</div>{error&&<p role="alert">{error}</p>}{request.loading&&<p role="status">Reading this interval range; previous values hidden.</p>}{request.error&&<p role="alert">{request.error}</p>}{result&&<><p>{name} · {unit} · fixed local standard time. {demand?'Demand is supplied by Rust over complete ten-minute intervals.':`Raw retained ${result.series.interval_minutes}-minute values.`}</p><p>{result.integrity_scope}</p>{values.length?<><svg className="energy-chart" viewBox="0 0 720 180" role="img" aria-label={`${name} (${unit})`}><path d={path} fill="none" stroke="#397c70" strokeWidth="2"/><text x="15" y="170">{labels[0]}</text><text x="510" y="170">{labels.at(-1)}</text></svg><details open><summary>Exact Rust values</summary><table><thead><tr><th>{demand?'Local interval end':'Retained interval'}</th><th>{unit}</th></tr></thead><tbody>{values.map((v,i)=><tr key={i}><td>{labels[i]}</td><td>{String(v)}</td></tr>)}</tbody></table></details></>:<p>No complete ten-minute demand interval in this range.</p>}</>}</>;
}
