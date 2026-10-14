import { useEffect, useRef, useState } from 'react';
import type { useDocument } from '../workspace/useDocument';
import type { EnergyCalendar } from './types';
import type { ZoneBoxInput } from './authoring-types';
import './authoring.css';

/** Provisional creation form. Rust alone constructs the thermal project. */
export default function NewEnergyProject({doc}:{doc:ReturnType<typeof useDocument>}){
  const [fields,setFields]=useState({id:'zone_1',name:'First zone',width:'8',depth:'6',height:'3',offset:'0'});
  const [weekday,setWeekday]=useState<EnergyCalendar['start_weekday']>('monday');
  const [busy,setBusy]=useState(false),[error,setError]=useState(''),[closing,setClosing]=useState(false),[reason,setReason]=useState('');
  const current=useRef(doc);current.current=doc;
  const resolver=useRef<((proceed:boolean)=>void)|null>(null);
  useEffect(()=>{
    current.current.setEnergyAuthoringOpen(true);
    const unregister=current.current.registerEnergyAuthoringLifecycle(requested=>new Promise(resolve=>{resolver.current=resolve;setReason(requested);setClosing(true);}));
    return()=>{unregister();current.current.setEnergyAuthoringOpen(false);resolver.current?.(false);resolver.current=null;};
  },[]);
  const decision=(proceed:boolean)=>{const resolve=resolver.current;resolver.current=null;setReason('');setClosing(false);resolve?.(proceed);};
  const discard=()=>{decision(true);doc.setEnergyNewOpen(false);};
  const create=async()=>{
    setError('');setBusy(true);
    try{
      const number=(value:string,label:string)=>{if(!value.trim()||!Number.isFinite(Number(value)))throw Error(`${label}: enter a finite number.`);return Number(value);};
      const zone:ZoneBoxInput={id:fields.id,name:fields.name,storey:1,origin_m:[0,0,0],size_m:[number(fields.width,'Width'),number(fields.depth,'Depth'),number(fields.height,'Height')]};
      const calendar:EnergyCalendar={start_weekday:weekday,holidays:[],utc_offset_minutes:number(fields.offset,'UTC offset')};
      if(await doc.newEnergy(zone,calendar)){decision(true);doc.setEnergyNewOpen(false);}
      else setError('Creation was cancelled or rejected. Your parameters are retained. Review the document error or continue editing.');
    }catch(e){setError(String(e));}finally{setBusy(false);}
  };
  return <div className="modal-backdrop"><section className="energy-authoring-dialog" role="dialog" aria-modal="true" aria-labelledby="new-energy-title" onKeyDown={e=>{if(e.key==='Escape'){e.stopPropagation();setClosing(true);}}}>
    <header><h2 id="new-energy-title">New thermal project</h2><button disabled={busy} onClick={()=>setClosing(true)} aria-label="Close new thermal project">×</button></header>
    <p>Create one first-storey zone at the origin, with identical baseline A and proposed B. Rust supplies explicitly synthetic, uncalibrated envelope, gains and cooling-system assumptions. No windows or weather are inferred. Review these inputs before calculation.</p>
    {closing&&<div role="alertdialog" aria-label="Resolve new thermal project"><p>{reason?`Resolve the new-project parameters before ${reason}.`:'Discard the new-project parameters?'}</p>{reason&&<button disabled={busy} onClick={()=>void create()}>Apply — create project</button>}<button disabled={busy} onClick={discard}>Discard new project</button><button autoFocus disabled={busy} onClick={()=>decision(false)}>Cancel — continue editing</button></div>}
    <fieldset disabled={busy||closing}><div className="energy-authoring-grid">{(['id','name','width','depth','height','offset'] as const).map(key=><label key={key}>{({id:'Stable zone ID',name:'Zone name',width:'Width X (m)',depth:'Depth Y (m)',height:'Height Z (m)',offset:'UTC offset (minutes)'})[key]}<input value={fields[key]} inputMode={key==='id'||key==='name'?'text':'decimal'} onChange={e=>setFields(f=>({...f,[key]:e.target.value}))}/></label>)}<label>January 1 weekday<select value={weekday} onChange={e=>setWeekday(e.target.value as typeof weekday)}>{(['monday','tuesday','wednesday','thursday','friday','saturday','sunday'] as const).map(day=><option key={day}>{day}</option>)}</select></label></div></fieldset>
    <p>The project starts unsaved, with no holidays. You can edit the explicit calendar and import licensed weather before running.</p>
    {error&&<p role="alert">{error}</p>}
    <footer><button disabled={busy||closing} onClick={()=>void create()}>Create unsaved project</button><button disabled={busy} onClick={()=>setClosing(true)}>Discard / close…</button></footer>
  </section></div>;
}
