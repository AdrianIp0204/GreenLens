import { useEffect, useRef, useState } from 'react';
import type { Model, RunRecord, ValidatedModel } from '../types';
import { parseQuantity, setParameter } from './commands.mjs';

export function ParameterField({label,value,unit='m',onCommit,disabled=false}:{label:string;value:number;unit?:string;onCommit:(value:number)=>Promise<boolean>;disabled?:boolean}) {
  const [text,setText]=useState(String(value));const [error,setError]=useState('');const [pending,setPending]=useState(false);
  const accepting=useRef(false),escaping=useRef(false);
  useEffect(()=>{setText(String(value));setError('');},[value]);
  const accept=async()=>{
    if(escaping.current){escaping.current=false;return;}
    if(accepting.current||text===String(value))return;
    accepting.current=true;
    try{const next=parseQuantity(text,unit==='°'?'angle':unit==='lx'?'lux':unit==='m'?'length':'number');setPending(true);if(await onCommit(next)){setText(String(next));setError('');}else{setText(String(value));setError('Rejected. The accepted value was restored.');}}
    catch(e){setError(e instanceof Error?e.message:String(e));setText(String(value));}finally{accepting.current=false;setPending(false);}
  };
  return <label className="parameter"><span>{label}</span><span className="parameter-input"><input aria-label={`${label} (${unit})`} value={text} disabled={disabled||pending} inputMode="decimal" onChange={e=>setText(e.target.value)} onBlur={()=>void accept()} onKeyDown={e=>{if(e.key==='Enter'){e.preventDefault();void accept();}if(e.key==='Escape'){e.stopPropagation();escaping.current=true;setText(String(value));setError('');e.currentTarget.blur();}}}/><small>{unit}</small></span>{error&&<span className="field-error">{error}</span>}</label>;
}
type Field=[string,string,number,string?];
export default function PropertiesPanel({model,validated,selection,result,locked,disabled,commit,select}:{model:Model;validated:ValidatedModel|null;selection:string[];result:RunRecord|null;locked:boolean;disabled:boolean;commit:(candidate:Model,label:string)=>Promise<boolean>;select:(ids:string[])=>void}){
  if(!selection.length)return <div className="panel-empty"><h3>Object properties</h3><p>Select geometry or a sensor in a viewport or the Layers list.</p><button onClick={()=>select(['room'])}>Inspect room</button><button onClick={()=>select(['window'])}>Inspect window</button><p>Use Add or Remove above the viewports, then click an object. Or hold Shift and click to add; hold Ctrl and click to remove. Drag left to right for containment; right to left for crossing.</p></div>;
  if(selection.length>1)return <div className="panel-empty"><h3>{selection.length} selected</h3><p>Mixed selections are inspectable together. Select one parametric entity for an unambiguous edit.</p><ul>{selection.slice(0,20).map(id=><li key={id}>{id}</li>)}</ul></div>;
  const id=selection[0];
  if(id.startsWith('sensor:')){
    const sensor=validated?.sensor_grid.sensors.find(s=>s.id===id.slice(7));const index=validated?.sensor_grid.sensors.findIndex(s=>s.id===id.slice(7))??-1;
    return <div className="panel-empty"><h3>{id.slice(7)}</h3><p>Scientific sensor · read only</p><dl className="key-values">{sensor?.position.map((v,i)=><div key={i}><dt>{['East X','North Y','Up Z'][i]}</dt><dd>{v.toFixed(4)} m</dd></div>)}<div><dt>Direction</dt><dd>+Z</dd></div><div><dt>Illuminance</dt><dd>{result&&index>=0?`${result.lux[index].toFixed(3)} lx`:'No current result'}</dd></div></dl><button onClick={()=>select(['sensor-plane'])}>Edit sensor plane</button></div>;
  }
  if(id.startsWith('surface:')){
    const surface=validated?.surfaces.find(s=>s.id===id.slice(8));
    return <div className="panel-empty"><h3>{id.slice(8)}</h3><p>Generated {surface?.material} surface · inspect only</p><p>{surface?.vertices.length} vertices. This surface belongs to the parametric envelope; it is not an independent CAD face.</p><pre>{surface?.vertices.map(p=>p.map(v=>v.toFixed(3)).join(', ')).join('\n')}</pre><button onClick={()=>select(['room'])}>Edit room parameters</button></div>;
  }
  let fields:Field[]=[];let note='';
  if(id==='room'){fields=[['Room width','room.width_m',model.room.width_m],['Room depth','room.depth_m',model.room.depth_m],['Room height','room.height_m',model.room.height_m],['Clockwise rotation','room.rotation_deg',model.room.rotation_deg,'°'],['Ceiling reflectance','materials.ceiling_reflectance',model.materials.ceiling_reflectance,'0–1'],['Wall reflectance','materials.wall_reflectance',model.materials.wall_reflectance,'0–1'],['Floor reflectance','materials.floor_reflectance',model.materials.floor_reflectance,'0–1']];note='Fixed origin. Width/depth 0.5–100 m, height 0.5–20 m; rotation around vertical Z. Room deletion and translation are unsupported.';}
  if(id==='window'){fields=[['Window center X','window.center_x_m',model.window.center_x_m],['Window sill','window.sill_m',model.window.sill_m],['Window width','window.width_m',model.window.width_m],['Window height','window.height_m',model.window.height_m],['Visible transmittance','materials.visible_transmittance',model.materials.visible_transmittance,'0–1']];note='Movement is local X/Z on the y=0 facade. Keep 0.01 m opaque perimeter; T 0.01–0.90. No facade-normal translation, free rotation or deletion.';}
  if(id==='overhang'&&model.overhang){fields=[['Overhang projection','overhang.projection_m',model.overhang.projection_m],['Side extension','overhang.side_extension_m',model.overhang.side_extension_m],['Gap above window','overhang.gap_above_window_m',model.overhang.gap_above_window_m],['Overhang reflectance','overhang.reflectance',model.overhang.reflectance,'0–1']];note='Horizontal shading follows the window head. Projection 0.01–10 m; gap cannot lift shading above the room roof.';}
  if(id==='obstruction'&&model.obstruction){fields=[['Obstruction center X','obstruction.center_x_m',model.obstruction.center_x_m],['Facade clearance','obstruction.distance_m',model.obstruction.distance_m],['Obstruction width','obstruction.width_m',model.obstruction.width_m],['Obstruction depth','obstruction.depth_m',model.obstruction.depth_m],['Obstruction height','obstruction.height_m',model.obstruction.height_m],['Obstruction reflectance','obstruction.reflectance',model.obstruction.reflectance,'0–1']];note='One detached box on the south side. Clearance ≥0.05 m and no shading overlap; no independent rotation or vertical translation.';}
  if(id==='sensor-plane'){fields=[['Workplane height','sensors.workplane_height_m',model.sensors.workplane_height_m],['Maximum sensor spacing','sensors.requested_spacing_m',model.sensors.requested_spacing_m],['Wall inset','sensors.wall_inset_m',model.sensors.wall_inset_m]];note='Equal-area cell centers, upward normals, at most 10,000 sensors. This grid is separate from construction snapping.';}
  return <div className="properties-content"><div className="panel-section-heading"><h3>{id.replace('sensor-plane','Sensor plane')}</h3><span>{locked?'Locked':'Parametric'}</span></div>{locked&&<p className="warning-line">Unlock this entity or its layer before editing.</p>}
    <fieldset disabled={disabled||locked}>{fields.map(([label,path,value,unit])=><ParameterField key={path} label={label} value={value} unit={unit} onCommit={n=>commit(setParameter(model,path,n),`${label} = ${n}${unit??' m'}`)}/>)}</fieldset><p className="panel-note">{note}</p><p className="panel-note">Enter commits one validated edit. Escape restores the field. Lengths accept m, cm, mm, ft and in.</p></div>;
}
