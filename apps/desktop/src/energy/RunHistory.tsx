import EvidenceInspection from './EvidenceInspection';
import {useEffect,useState} from 'react';
import type {Project} from '../types';
import type {EnergyAssessment,EnergyMethodIdentity} from './types';
import {useEnergyRead} from './useEnergyRead';
import './run-history.css';

export interface EnergyCatalogRow {
  id:string;alternative_id:'A'|'B';name:string;finished_utc:string;
  input_revision_sha256:string;model_input_sha256:string;physical_input_sha256:string;record_sha256:string;
  method:EnergyMethodIdentity;interval_minutes:5|10;current_method:boolean;assessment:EnergyAssessment;
}
export interface EnergyCatalogPage {
  schema_version:'greenlens.energy-run-catalog.v1';project_sha256:string;offset:number;limit:number;
  alternative:'A'|'B'|null;total:number;next_offset:number|null;ordering:'reverse_retention_order';
  integrity_scope:string;rows:EnergyCatalogRow[];
}
type Props={path:string;project:Project;onChoose:(alternative:'A'|'B',id:string)=>void;selected?:Partial<Record<'A'|'B',string>>;disabled?:boolean};
const PAGE_SIZE=25,hash=(v:unknown)=>typeof v==='string'&&/^[0-9a-f]{64}$/.test(v);
const label=(v:string)=>v.replaceAll('_',' ');
/** IPC identity/shape only; Rust owns currentness and scientific assessment. */
function checked(value:EnergyCatalogPage,project:Project,offset:number,alternative:'A'|'B'|null):boolean {
  const expected=project.energy?.runs.filter(r=>!alternative||r.alternative_id===alternative)??[];
  const expectedIds=expected.slice().reverse().slice(offset,offset+PAGE_SIZE).map(r=>r.id);
  if(value?.schema_version!=='greenlens.energy-run-catalog.v1'||!hash(value.project_sha256)||value.offset!==offset||value.limit!==PAGE_SIZE||value.alternative!==alternative||value.total!==expected.length||value.ordering!=='reverse_retention_order'||typeof value.integrity_scope!=='string'||!Array.isArray(value.rows)||value.rows.length!==expectedIds.length||value.rows.length>100)return false;
  if(value.next_offset!==(offset+value.rows.length<value.total?offset+value.rows.length:null))return false;
  return value.rows.every((r,i)=>{
    const summary=expected.find(v=>v.id===r?.id),a=r?.assessment,m=r?.method;
    return r&&r.id===expectedIds[i]&&summary?.alternative_id===r.alternative_id&&['A','B'].includes(r.alternative_id)&&r.record_sha256===summary.record_sha256&&r.physical_input_sha256===summary.physical_input_hash&&hash(r.input_revision_sha256)&&hash(r.model_input_sha256)&&typeof r.name==='string'&&typeof r.finished_utc==='string'&&/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?(?:Z|\+00:00)$/.test(r.finished_utc)&&[5,10].includes(r.interval_minutes)&&typeof r.current_method==='boolean'&&m&&typeof m.method_id==='string'&&typeof m.engine_version==='string'&&hash(m.recipe_sha256)&&hash(m.numerical_settings_sha256)&&a?.execution==='completed'&&a.evidence_integrity==='metadata_checked'&&['current','changed','not_compared'].includes(a.current_physical_inputs)&&['unassessed','failed','limited_qualified'].includes(a.scientific?.status);
  });
}
export default function RunHistory({path,project,onChoose,selected={},disabled=false}:Props){
  const [offset,setOffset]=useState(0),[alternative,setAlternative]=useState<'A'|'B'|null>(null);
  useEffect(()=>{setOffset(0);setAlternative(null);},[path,project.id]);
  const request={offset,limit:PAGE_SIZE,alternative};
  const response=useEnergyRead<EnergyCatalogPage>('energy_run_catalog',path&&project.energy?{path,project,request}:null);
  const valid=response.value&&checked(response.value,project,offset,alternative),page=valid?response.value:null;
  const error=response.error||(response.value&&!valid?'Catalog response does not match this document, page or immutable run identities.':'');
  return <section className="energy-run-history" aria-label="Retained energy run history"><h2>Retained run history</h2>
    <p>Choose evidence explicitly for baseline A or proposed B. Input revisions, method compatibility and scientific status are separate. Selection runs no engine and changes no model.</p>
    <label>Retained design<select value={alternative??''} onChange={e=>{setAlternative(e.target.value===''?null:e.target.value as 'A'|'B');setOffset(0);}}><option value="">Both A and B</option><option value="A">Baseline A</option><option value="B">Proposed B</option></select></label>
    {!path&&<p>Save the project before inspecting its retained run catalog.</p>}
    <EvidenceInspection label="Checking this page of retained metadata; previous rows are hidden" read={response}/>
    {error&&<p role="alert">{error}</p>}
    {page&&<><p>{page.total} retained snapshots. Most recently retained first; completion times are shown exactly in UTC. This order is not an assertion of newest completion.</p>
      <div className="erh-scroll"><table><caption>Snapshots {page.total?offset+1:0}–{offset+page.rows.length} of {page.total}</caption><thead><tr><th>Design / retained name</th><th>Finished UTC</th><th>Input revision</th><th>Method / timestep</th><th>Status</th><th>Selection</th></tr></thead><tbody>{page.rows.map(row=><tr key={row.id}><td><strong>{row.alternative_id==='A'?'Baseline A':'Proposed B'}</strong><br/>{row.name}<small>{row.id}</small></td><td><time dateTime={row.finished_utc}>{row.finished_utc}</time></td><td><details><summary>{row.input_revision_sha256.slice(0,12)}…</summary><dl><dt>Model and study revision SHA-256</dt><dd>{row.input_revision_sha256}</dd><dt>Typed model SHA-256</dt><dd>{row.model_input_sha256}</dd><dt>Physical inputs SHA-256</dt><dd>{row.physical_input_sha256}</dd><dt>Immutable record SHA-256</dt><dd>{row.record_sha256}</dd></dl></details></td><td>{row.method.method_id}<br/>EnergyPlus {row.method.engine_version}<br/>{row.interval_minutes}-minute intervals<details><summary>Recipe and numerical identity</summary><p>{row.method.recipe_sha256}</p><p>{row.method.numerical_settings_sha256}</p></details></td><td><strong>{row.assessment.current_physical_inputs==='current'?'Current inputs':row.assessment.current_physical_inputs==='changed'?'Historical inputs':'Inputs not compared'}</strong><br/>{row.current_method?'Current compiled method':'Historical method'}<br/>Scientific: {label(row.assessment.scientific.status)}<br/>Metadata checked<details><summary>Assessment scope</summary><p>{row.assessment.scientific.reason}</p><p>{row.assessment.scientific.tested_scope}</p></details></td><td><button disabled={disabled} aria-pressed={selected[row.alternative_id]===row.id} onClick={()=>onChoose(row.alternative_id,row.id)}>Use for {row.alternative_id==='A'?'baseline A':'proposed B'}</button></td></tr>)}</tbody></table></div>
      {!page.rows.length&&<p>No retained snapshots match this design filter.</p>}<p className="erh-scope">{page.integrity_scope}</p>
      <nav aria-label="Run history pages"><button disabled={disabled||offset===0} onClick={()=>setOffset(Math.max(0,offset-PAGE_SIZE))}>Previous page</button><button disabled={disabled||page.next_offset===null} onClick={()=>setOffset(page.next_offset!)}>Next page</button></nav>
    </>}
  </section>;
}
