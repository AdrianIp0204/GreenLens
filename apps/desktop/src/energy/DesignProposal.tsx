import {useEffect,useRef,useState} from 'react';
import {invoke} from '@tauri-apps/api/core';
import type {ClimateProjectDocument} from '../climate-types';
import type {ProposalAction,ProposalDraft,ProposalReview} from './design-proposals';
import type {ThermalWindow,ThermalZone} from './types';
import './authoring.css';

type Props={project:ClimateProjectDocument;selectedZone:string;onCommit:(draft:ProposalDraft,sha:string)=>Promise<boolean>;onClose:()=>void;onActivityChange:(open:boolean)=>void;registerLifecycle:(request:(reason:string)=>Promise<boolean>)=>()=>void};
const choices=[['set_overhang','External window shading'],['put_window','Add / resize rectangular window'],['remove_window','Remove window'],['put_detached_shade','Add / resize detached shading'],['remove_detached_shade','Remove detached shading'],['assign_window_glazing','Explicit glazing properties'],['assign_construction','Opaque construction assignment']] as const;
const bytes=(v:string)=>new TextEncoder().encode(v).length;
export default function DesignProposal({project,selectedZone,onCommit,onClose,onActivityChange,registerLifecycle}:Props){
  const model=project.energy!.designs.B.model,initial=model.zones.find(z=>z.id===selectedZone)??model.zones[0];
  const [kind,setKind]=useState<ProposalAction['kind']>('set_overhang'),[zoneId,setZoneId]=useState(initial.id),[windowId,setWindowId]=useState(initial.windows[0]?.id??''),[surfaceId,setSurfaceId]=useState(initial.windows[0]?.surface_id??initial.surfaces[0].id),[constructionId,setConstructionId]=useState(model.constructions[0]?.id??''),[glazingId,setGlazingId]=useState(initial.windows[0]?.glazing_id??model.glazing[0]?.id??'');
  const w=initial.windows[0],g=model.glazing.find(x=>x.id===w?.glazing_id)??model.glazing[0],shade=model.detached_shading[0];
  const [shadeTarget,setShadeTarget]=useState(shade?.id??'');
  const [fields,setFields]=useState<Record<string,string>>({windowId:w?.id??'proposed_window',width:String(w?.width_m??2),height:String(w?.height_m??1.5),u:String(w?.offset_u_m??1),v:String(w?.offset_v_m??1),projection:String(w?.overhang?.projection_m??0.6),extension:String(w?.overhang?.side_extension_m??0.1),gap:String(w?.overhang?.gap_above_m??0.1),shadeId:shade?.id??'proposed_shade',x:String(shade?.origin_m[0]??0),y:String(shade?.origin_m[1]??-3),z:String(shade?.origin_m[2]??0),sx:String(shade?.size_m[0]??4),sy:String(shade?.size_m[1]??0.3),sz:String(shade?.size_m[2]??3),newGlazingId:'proposed_glazing',uValue:String(g?.u_value_w_m2k??''),shgc:String(g?.shgc??''),vlt:String(g?.visible_transmittance??'')});
  const [removeOverhang,setRemoveOverhang]=useState(false),[citation,setCitation]=useState(''),[rationale,setRationale]=useState(''),[synthetic,setSynthetic]=useState(true);
  const [review,setReview]=useState<ProposalReview|null>(null),[error,setError]=useState(''),[busy,setBusy]=useState(false),[closing,setClosing]=useState(false),[reason,setReason]=useState('');
  const dialog=useRef<HTMLDialogElement>(null),resolver=useRef<((answer:boolean)=>void)|null>(null),activity=useRef(onActivityChange),lifecycle=useRef(registerLifecycle);
  activity.current=onActivityChange;lifecycle.current=registerLifecycle;
  useEffect(()=>{activity.current(true);dialog.current?.showModal();return()=>{activity.current(false);resolver.current?.(false);};},[]);
  useEffect(()=>lifecycle.current(message=>new Promise(resolve=>{resolver.current=resolve;setReason(message);setClosing(true);})),[]);
  const decide=(value:boolean)=>{resolver.current?.(value);resolver.current=null;setClosing(false);setReason('');};
  const signature=JSON.stringify([project,kind,zoneId,windowId,surfaceId,constructionId,glazingId,shadeTarget,fields,removeOverhang,citation,rationale,synthetic]);
  const latest=useRef(signature);latest.current=signature;const reviewed=useRef(''),draftRef=useRef<ProposalDraft|null>(null);
  useEffect(()=>{setReview(null);},[signature]);
  const zone=model.zones.find(z=>z.id===zoneId)!;
  const n=(key:string)=>{if(!fields[key].trim()||!Number.isFinite(Number(fields[key])))throw Error(`${key}: enter a finite value in the displayed units.`);return Number(fields[key]);};
  const build=():ProposalDraft=>{
    if(!citation.trim()||!rationale.trim()||bytes(citation)>2048||bytes(rationale)>2048)throw Error('Provide a source and rationale, each at most 2,048 UTF-8 bytes.');
    let action:ProposalAction;
    if(kind==='set_overhang')action={kind,zone_id:zoneId,window_id:windowId,overhang:removeOverhang?null:{projection_m:n('projection'),side_extension_m:n('extension'),gap_above_m:n('gap')}};
    else if(kind==='put_window'){const old=zone.windows.find(x=>x.id===fields.windowId);const window:ThermalWindow={id:fields.windowId,surface_id:surfaceId,glazing_id:glazingId,offset_u_m:n('u'),offset_v_m:n('v'),width_m:n('width'),height_m:n('height'),overhang:old?.overhang??null};action={kind,zone_id:zoneId,window};}
    else if(kind==='remove_window')action={kind,zone_id:zoneId,window_id:windowId};
    else if(kind==='put_detached_shade'){
      if(shadeTarget&&!model.detached_shading.some(s=>s.id===shadeTarget))throw Error('The selected shade is no longer present. Select the current target before previewing.');
      if(!shadeTarget&&model.detached_shading.some(s=>s.id===fields.shadeId))throw Error('That shade ID already exists. Select it as the existing shade to edit, or choose a new ID.');
      action={kind,shade:{id:shadeTarget||fields.shadeId,origin_m:[n('x'),n('y'),n('z')],size_m:[n('sx'),n('sy'),n('sz')]}};
    }
    else if(kind==='remove_detached_shade'){
      if(!shadeTarget||!model.detached_shading.some(s=>s.id===shadeTarget))throw Error('Select an existing detached shade to remove.');
      action={kind,shade_id:shadeTarget};
    }
    else if(kind==='assign_window_glazing')action={kind,zone_id:zoneId,window_id:windowId,glazing:{id:fields.newGlazingId,name:fields.newGlazingId,u_value_w_m2k:n('uValue'),shgc:n('shgc'),visible_transmittance:n('vlt'),basis:model.glazing.find(x=>x.id===zone.windows.find(w=>w.id===windowId)?.glazing_id)?.basis??'whole_window',source:{citation,synthetic}}};
    else action={kind,zone_id:zoneId,surface_id:surfaceId,construction_id:constructionId};
    return{action,source:{citation,synthetic},units:kind==='assign_window_glazing'?'U: W/(m²·K); SHGC and VLT: dimensionless':kind==='assign_construction'?'Declared layered construction ID':'m',rationale};
  };
  const preview=async()=>{setBusy(true);setError('');setReview(null);const started=signature;try{const draft=build();const value=await invoke<ProposalReview>('energy_proposal_preview',{project,draft});if(latest.current!==started)throw Error('Inputs changed during preview; review the current values again.');draftRef.current=draft;reviewed.current=started;setReview(value);}catch(e){setError(String(e));}finally{setBusy(false);}};
  const apply=async()=>{if(!review?.proposal.valid||reviewed.current!==signature||!draftRef.current){setError('Preview a valid candidate before applying. Your parameters are retained.');return;}setBusy(true);try{if(await onCommit(draftRef.current,review.context_sha256)){decide(true);onClose();}else setError('Apply was rejected. The draft is retained; refresh the preview after resolving the document error.');}catch(e){setError(String(e));}finally{setBusy(false);}};
  const input=(key:string,label:string)=><label>{label}<input value={fields[key]} onChange={e=>setFields(old=>({...old,[key]:e.target.value}))}/></label>;
  const selectWindow=(target:ThermalZone,id:string)=>{
    const value=target.windows.find(x=>x.id===id),glazing=model.glazing.find(x=>x.id===value?.glazing_id)??model.glazing[0];
    setZoneId(target.id);setWindowId(value?.id??'');setSurfaceId(value?.surface_id??target.surfaces[0].id);setGlazingId(value?.glazing_id??glazing?.id??'');setRemoveOverhang(false);
    setFields(old=>({...old,windowId:value?.id??'proposed_window',width:String(value?.width_m??2),height:String(value?.height_m??1.5),u:String(value?.offset_u_m??1),v:String(value?.offset_v_m??1),projection:String(value?.overhang?.projection_m??0.6),extension:String(value?.overhang?.side_extension_m??0.1),gap:String(value?.overhang?.gap_above_m??0.1),uValue:String(glazing?.u_value_w_m2k??''),shgc:String(glazing?.shgc??''),vlt:String(glazing?.visible_transmittance??'')}));
  };
  const selectShade=(id:string)=>{
    const value=model.detached_shading.find(s=>s.id===id);
    setShadeTarget(value?.id??'');
    setFields(old=>({...old,shadeId:value?.id??'proposed_shade',x:String(value?.origin_m[0]??0),y:String(value?.origin_m[1]??-3),z:String(value?.origin_m[2]??0),sx:String(value?.size_m[0]??4),sy:String(value?.size_m[1]??0.3),sz:String(value?.size_m[2]??3)}));
  };
  return <dialog ref={dialog} className="energy-authoring-dialog design-proposal-dialog" aria-labelledby="proposal-title" onCancel={e=>{e.preventDefault();setClosing(true);}}><header><h2 id="proposal-title">Design alternative · Proposed B</h2><button disabled={busy} onClick={()=>setClosing(true)} aria-label="Close proposal">×</button></header>
    <p>Test a declared hypothesis. Baseline A and imported reference geometry remain unchanged. A valid preview is <strong>Not simulated</strong>; savings and service equivalence require matching evidence.</p>
    {closing&&<div role="alertdialog" aria-label="Resolve proposed alternative"><p>Resolve this draft before {reason||'closing'}.</p><button disabled={busy||!review?.proposal.valid} onClick={()=>void apply()}>Apply</button><button disabled={busy} onClick={()=>{decide(true);onClose();}}>Discard</button><button disabled={busy} onClick={()=>decide(false)}>Cancel</button></div>}
    <fieldset disabled={busy||closing}><legend>Applicable supported change</legend><label>Hypothesis<select value={kind} onChange={e=>setKind(e.target.value as ProposalAction['kind'])}>{choices.map(([id,label])=><option key={id} value={id}>{label}</option>)}</select></label>
    <label>Zone in B<select value={zoneId} onChange={e=>{const z=model.zones.find(x=>x.id===e.target.value)!;selectWindow(z,z.windows[0]?.id??'');}}>{model.zones.map(z=><option key={z.id} value={z.id}>{z.name} · {z.id}</option>)}</select></label>
    {['set_overhang','put_window','remove_window','assign_window_glazing'].includes(kind)&&<label>Existing window<select value={windowId} onChange={e=>selectWindow(zone,e.target.value)}><option value="">Choose an existing window</option>{zone.windows.map(w=><option key={w.id} value={w.id}>{w.id}</option>)}</select></label>}
    {kind==='set_overhang'&&<><label><input type="checkbox" checked={removeOverhang} onChange={e=>setRemoveOverhang(e.target.checked)}/>Remove overhang</label>{!removeOverhang&&<>{input('projection','Projection (m)')}{input('extension','Side extension (m)')}{input('gap','Gap above window (m)')}</>}</>}
    {kind==='put_window'&&<>{input('windowId','Stable window ID (new or existing)')}{input('u','Horizontal offset on face (m)')}{input('v','Vertical offset on face (m)')}{input('width','Width (m)')}{input('height','Height (m)')}<label>Explicit glazing<select value={glazingId} onChange={e=>setGlazingId(e.target.value)}>{model.glazing.map(g=><option key={g.id} value={g.id}>{g.name} · {g.id}</option>)}</select></label><p>Editing an existing window retains its overhang. A new ID adds a window; it does not rename or remove another.</p></>}
    {['put_window','assign_construction'].includes(kind)&&<label>Surface<select value={surfaceId} onChange={e=>setSurfaceId(e.target.value)}>{zone.surfaces.map(s=><option key={s.id} value={s.id}>{s.face} · {s.id}</option>)}</select></label>}
    {kind==='assign_construction'&&<label>Existing construction<select value={constructionId} onChange={e=>setConstructionId(e.target.value)}>{model.constructions.map(c=><option key={c.id} value={c.id}>{c.name} · {c.id}</option>)}</select></label>}
    {kind==='assign_window_glazing'&&<>{input('newGlazingId','New glazing ID')}{input('uValue','U-value (W/m²K)')}{input('shgc','SHGC (0–1)')}{input('vlt','Visible transmittance (0–1)')}<p>Values start from the original selected glazing, not a recommended product. Supply the applicable source; this assigns only the selected window.</p></>}
    {kind==='remove_detached_shade'&&<label>Detached shade<select value={shadeTarget} onChange={e=>selectShade(e.target.value)}><option value="">Choose an existing shade</option>{model.detached_shading.map(s=><option key={s.id}>{s.id}</option>)}</select></label>}
    {kind==='put_detached_shade'&&<><label>Detached shade target<select value={shadeTarget} onChange={e=>selectShade(e.target.value)}><option value="">Create a new shade</option>{model.detached_shading.map(s=><option key={s.id} value={s.id}>Edit {s.id}</option>)}</select></label>{shadeTarget?<label>Stable shade ID (new or existing)<input value={shadeTarget} readOnly/></label>:input('shadeId','Stable shade ID (new or existing)')}{['x','y','z'].map(axis=><span key={axis}>{input(axis,`Origin ${axis} (m)`)}</span>)}{input('sx','Width X (m)')}{input('sy','Depth Y (m)')}{input('sz','Height Z (m)')}<p>{shadeTarget?'Coordinates and dimensions start from the selected accepted shade. Only the displayed candidate is proposed.':'New-shade coordinates are editable synthetic starting values. Choose a unique ID and review its position before applying.'}</p></>}
    <label>Source / assumption<input value={citation} onChange={e=>setCitation(e.target.value)}/><small>{2048-bytes(citation)} UTF-8 bytes remaining</small></label><label><input type="checkbox" checked={synthetic} onChange={e=>setSynthetic(e.target.checked)}/>Explicitly synthetic assumption</label><label>Rationale and applicability<textarea value={rationale} onChange={e=>setRationale(e.target.value)}/><small>{2048-bytes(rationale)} UTF-8 bytes remaining</small></label>
    </fieldset>
    {error&&<p role="alert">{error}</p>}{review&&<section aria-label="Proposal preview"><h3>{review.proposal.valid?'Valid candidate — review before applying':'Invalid candidate — accepted inputs unchanged'}</h3><p>Affected objects: {review.proposal.affected_object_ids.join(', ')||'none'}</p>{review.proposal.problems.map((s,i)=><p key={i} role="alert">{s}</p>)}<ul>{review.proposal.tradeoffs.map((s,i)=><li key={i}>{s}</li>)}</ul><div className="paired-proposal-figures">{review.figures?.map(f=><figure key={f.filename}><img alt={`${f.frame_id} · ${f.side} · provisional analytical geometry`} src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(f.svg)}`}/><figcaption>{f.frame_id} · {f.side} · Not simulated</figcaption></figure>)}</div><p>Before: <code>{review.proposal.base_model_sha256}</code><br/>Candidate: <code>{review.proposal.candidate_model_sha256??'invalid'}</code></p></section>}
    <footer><button disabled={busy||closing} onClick={()=>void preview()}>Preview in Rust</button><button disabled={busy||closing||!review?.proposal.valid} onClick={()=>void apply()}>Apply to B</button><button disabled={busy} onClick={()=>setClosing(true)}>Discard / close…</button><span role="status">{busy?'Validating proposal…':'Nothing is accepted until Apply.'}</span></footer>
  </dialog>;
}
