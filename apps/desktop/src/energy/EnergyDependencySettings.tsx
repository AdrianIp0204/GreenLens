import {useEffect,useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import type {EnergyDependencyPaths,useEnergyDependencyPaths} from './energy-dependency-settings';

type Issue={severity:'error'|'warning'|'information';code:string;message:string;remedy:string};
type Readiness={schema_version:'greenlens.energy-dependencies.v1';energyplus_ready:boolean;typst_and_fonts_ready:boolean;issues:Issue[]};
function readiness(value:unknown):Readiness {
  const r=value as Readiness;
  if(!r||r.schema_version!=='greenlens.energy-dependencies.v1'||typeof r.energyplus_ready!=='boolean'||typeof r.typst_and_fonts_ready!=='boolean'||!Array.isArray(r.issues)||r.issues.length>100||!r.issues.every(i=>i&&['error','warning','information'].includes(i.severity)&&['code','message','remedy'].every(k=>typeof i[k as keyof Issue]==='string'&&i[k as keyof Issue].length<=16384)))throw Error('Dependency check returned an invalid response; readiness is unknown.');
  return r;
}
type Props={settings:ReturnType<typeof useEnergyDependencyPaths>;disabled?:boolean};
export default function EnergyDependencySettings({settings,disabled=false}:Props){
  const {paths}=settings,key=JSON.stringify(paths),live=useRef(key);live.current=key;
  const generation=useRef(0),[busy,setBusy]=useState(false),[error,setError]=useState(''),[result,setResult]=useState<{key:string;value:Readiness}|null>(null);
  useEffect(()=>{generation.current++;setBusy(false);setError('');setResult(null);return()=>{generation.current++;};},[key]);
  const check=async()=>{
    const token=++generation.current,captured=key;setBusy(true);setError('');setResult(null);
    try{
      const value=readiness(await invoke('energy_dependencies',{engineDir:paths.engineDir||null,typstDir:paths.typstDir||null}));
      if(token===generation.current&&live.current===captured)setResult({key:captured,value});
    }catch(e){if(token===generation.current&&live.current===captured)setError(String(e));}
    finally{if(token===generation.current&&live.current===captured)setBusy(false);}
  };
  const current=result?.key===key?result.value:null;
  const pathField=(field:keyof EnergyDependencyPaths,label:string)=><label>{label}<input value={paths[field]} disabled={disabled} spellCheck={false} autoComplete="off" placeholder="Pinned installation default" onChange={e=>settings.setPath(field,e.target.value)}/></label>;
  return <section className="energy-dependency-settings" aria-label="Local energy dependencies">
    <h3>Local installations</h3>
    <p className="energy-note">Directory settings stay on this machine, outside the project and exports. Empty fields use the separately installed pinned defaults.</p>
    <div className="energy-dependency-paths">{pathField('engineDir','EnergyPlus directory')}{pathField('typstDir','Typst directory')}</div>
    <div className="energy-inline"><button disabled={disabled||busy} onClick={()=>void check()}>{busy?'Checking installation hashes…':'Check local installations'}</button>
    <button disabled={disabled} onClick={settings.reset}>Use default locations</button></div>
    {settings.warning&&<p role="alert">{settings.warning}</p>}{error&&<p role="alert">{error}</p>}
    {!current&&!busy&&<p role="status">Installation readiness has not been checked for these settings.</p>}
    {current&&<><p role="status">EnergyPlus: {current.energyplus_ready?'pinned installation ready':'missing or altered'}. Typst and fonts: {current.typst_and_fonts_ready?'pinned installation ready':'missing or altered'}.</p>
      <ul>{current.issues.map((issue,i)=><li key={`${issue.code}:${i}`}><p>{issue.message}</p><p>{issue.remedy}</p></li>)}</ul></>}
    <p className="energy-note">This check does not run a simulation, install software or establish scientific qualification. Dependencies are verified again when used. Install the pinned packages using the documented EnergyPlus and Typst setup scripts; calculation and rendering then work offline.</p>
  </section>;
}
