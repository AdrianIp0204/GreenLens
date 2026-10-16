import EvidenceInspection from './EvidenceInspection';
import {useEffect,useState} from 'react';
import type {useDocument} from '../workspace/useDocument';
import {useEnergyRead} from './useEnergyRead';
import {assertEnergyResponse} from './response.mjs';

type Plan={created_utc:string;current_inputs_sha256:string;declaration_sha256:string;slots:{slot_id:string;reuse:{run_id:string;finished_utc:string}|null}[];declaration:{annual_electricity_limit_percent:number;common_peak_limit_percent:number;occupied_cooling_unmet_limit_percent:number;cooling_tolerance_c:number};reuse_policy:string};
type Criterion={id:string;alternative_id:string;quantity:string;zone_id:string|null;ten_minute:number|null;five_minute_reference:number|null;measured_percent:number|null;limit_percent:number;passed:boolean;explanation:string};
export type Session={reference:{id:string;plan_sha256:string};plan:Plan;next_slot_id:string|null;active_slot_id:string|null;active_job_id:string|null;completed:Record<string,string>;interruptions:{reason:string;kind:string}[];finalized:boolean;execution_policy:string;result:{current_inputs_match:boolean;assessment:{numerical_sensitivity_passed:boolean;all_declared_runs_integrity_checked:boolean;criteria:Criterion[];missing_slots:string[];limitations:string[];qualification_promoted:false}}|null};
type Doc=ReturnType<typeof useDocument>;
const number=(n:number|null)=>n===null?'Not available':String(n);
export default function EnergySensitivity({doc,engineDir,disabled}:{doc:Doc;engineDir:string;disabled:boolean}){
  const [targets,setTargets]=useState('A,B'),[plan,setPlan]=useState<Plan|null>(null),[chosen,setChosen]=useState('');
  const [busy,setBusy]=useState(false),[error,setError]=useState('');
  const references=doc.project?.energy?.workbench?.sensitivity??[];
  const id=references.some(r=>r.id===chosen)?chosen:references.at(-1)?.id??'';
  const cached=doc.sensitivitySnapshots?.[id];
  let snapshot:Session|null=null,snapshotError='';
  if(cached){try{snapshot=assertEnergyResponse('energy_sensitivity_inspect',{project:doc.project,id},cached);}catch(e){snapshotError=String(e);}}
  const current=useEnergyRead<Session>('energy_sensitivity_inspect',!cached&&id&&doc.project&&doc.path?{path:doc.path,project:doc.project,id}:null);
  const session=snapshot??current.value;
  const inputIdentity=useEnergyRead<string>('energy_sensitivity_input_identity',snapshot&&doc.project?{project:doc.project,alternatives:[...new Set(snapshot.plan.slots.map(s=>s.slot_id.replace(/(?:10|5)$/,'')))]}:null);
  const inputRelation=snapshot?(inputIdentity.loading?'Checking current input identity':inputIdentity.value===snapshot.plan.current_inputs_sha256?'Current':'Historical — accepted inputs changed'):session?.result?.current_inputs_match?'Current':'Historical — accepted inputs changed';
  useEffect(()=>{setPlan(null);setChosen('');setError('');},[doc.documentId]);
  const act=async(action:Record<string,unknown>)=>{
    setBusy(true);setError('');
    try{const response=await doc.energySensitivity(action,engineDir);if(!response)return;
      if(response.plan)setPlan(response.plan as Plan);
      if(response.session){setChosen((response.session as Session).reference.id);setPlan(null);}
    }catch(e){setError(String(e));}finally{setBusy(false);}
  };
  const locked=disabled||busy;
  return <section aria-label="Numerical sensitivity"><h2>Check numerical sensitivity</h2>
    <p>Compare ten-minute and five-minute simulations on common ten-minute demand intervals. This is numerical sensitivity evidence, not physical validation or automatic scientific qualification.</p>
    <p>Frozen criteria: annual electricity difference ≤1%, facility peak difference ≤3%, and occupied cooling-unmet ≤1% in every conditioned zone at 0.2 °C tolerance. Each missing case gets one attempt; failure or cancellation stops that declaration.</p>
    <label>Study scope<select value={targets} disabled={locked} onChange={e=>{setTargets(e.target.value);setPlan(null);}}><option value="A,B">Baseline A and proposed B</option><option value="A">Baseline A only</option><option value="B">Proposed B only</option></select></label>
    <button disabled={locked} onClick={()=>void act({kind:'plan',alternatives:targets.split(',')})}>Review finite run plan</button>
    {plan&&<section aria-label="Proposed sensitivity plan"><p>Exact accepted inputs: <code>{plan.current_inputs_sha256}</code>. Created {plan.created_utc}.</p><table><caption>Fixed execution order and verified reuse</caption><thead><tr><th>Slot</th><th>Declared action</th></tr></thead><tbody>{plan.slots.map(s=><tr key={s.slot_id}><th>{s.slot_id}</th><td>{s.reuse?`Reuse ${s.reuse.run_id} (${s.reuse.finished_utc})`:'One full-year native run required'}</td></tr>)}</tbody></table><p>{plan.reuse_policy}</p><p>Per case: 30-minute deadline, 2 GiB sampled memory, 1 GiB scratch, 128 MiB file bound. No tuning or automatic retries.</p><button disabled={locked} onClick={()=>void act({kind:'retain',plan})}>Retain this declaration</button><button disabled={locked} onClick={()=>setPlan(null)}>Cancel declaration</button></section>}
    <label>Retained assessment<select value={id} disabled={busy} onChange={e=>setChosen(e.target.value)}><option value="">No retained assessment</option>{references.map(r=><option key={r.id} value={r.id}>{r.id}</option>)}</select></label>
    {(error||snapshotError||current.error||inputIdentity.error)&&<p role="alert">{error||snapshotError||current.error||inputIdentity.error}</p>}<EvidenceInspection label="Checking all bound evidence" read={current}/>
    {snapshot&&<p>Evidence integrity snapshot retained from opening or the last explicit assessment action. Files are checked again for subsequent evidence actions.</p>}{session&&<><p>Declared {session.plan.created_utc} · {session.finalized?'Finalized assessment':session.active_slot_id?`Attempt ${session.active_slot_id} awaiting collection/outcome`:'Declaration retained'}.</p><p>{session.execution_policy}</p><table><caption>Every declared case</caption><thead><tr><th>Case</th><th>Evidence</th></tr></thead><tbody>{session.plan.slots.map(s=><tr key={s.slot_id}><th>{s.slot_id}</th><td>{s.reuse?`Reused ${s.reuse.run_id}`:session.completed[s.slot_id]?`Collected ${session.completed[s.slot_id]}`:session.active_slot_id===s.slot_id?`Attempt reserved · ${session.active_job_id??'no persisted job ID'}`:'Missing'}</td></tr>)}</tbody></table>
      {!session.finalized&&<div className="energy-inline">
        <button disabled={locked||!session.next_slot_id||Boolean(session.active_slot_id)||Boolean(session.interruptions.length)} onClick={()=>void act({kind:'next',id})}>Run next declared case{session.next_slot_id?` (${session.next_slot_id})`:''}</button>
        <button disabled={locked||!session.active_slot_id} onClick={()=>void act({kind:'observe',id})}>Record collected / failed outcome</button>
        <button disabled={locked||Boolean(session.active_slot_id)||Boolean(session.next_slot_id)} onClick={()=>void act({kind:'finalize',id})}>Finalize numerical assessment</button>
      </div>}
      {session.interruptions.map((r,i)=><p role="alert" key={i}>{r.kind}: {r.reason}</p>)}
      {session.result&&<><h3>{session.result.assessment.numerical_sensitivity_passed?'Declared numerical criteria passed':'Declared numerical criteria not passed'}</h3><p>Current-input relationship: {inputRelation}. Full declared evidence: {session.result.assessment.all_declared_runs_integrity_checked?'integrity checked':'incomplete'}. Scientific qualification is unchanged.</p><table><caption>Full-precision assessment; every zone and criterion retained</caption><thead><tr><th>Case / quantity</th><th>10-minute</th><th>5-minute reference</th><th>Measured %</th><th>Limit %</th><th>Outcome</th></tr></thead><tbody>{session.result.assessment.criteria.map(c=><tr key={c.id}><th>{c.alternative_id} · {c.zone_id??''} {c.quantity}<details><summary>Definition</summary>{c.explanation}</details></th><td>{number(c.ten_minute)}</td><td>{number(c.five_minute_reference)}</td><td>{number(c.measured_percent)}</td><td>{c.limit_percent}</td><td>{c.passed?'Pass':'Not passed'}</td></tr>)}</tbody></table>{session.result.assessment.limitations.map(x=><p key={x}>{x}</p>)}</>}
      {!session.finalized&&session.active_slot_id&&<details><summary>Interrupted-session recovery</summary><p>First recover any pending completed evidence through the document controls. These explicit actions never rerun an attempt.</p><button disabled={locked} onClick={()=>void act({kind:'recover_metadata',id})}>Reconcile interrupted receipt commit</button><button disabled={locked} onClick={()=>void act({kind:'mark_interrupted',id})}>Mark unresolved attempt interrupted</button></details>}
    </>}
  </section>;
}
