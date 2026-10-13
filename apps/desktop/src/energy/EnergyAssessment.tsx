import type { EnergyAssessment as Assessment, ReportCell } from './types';
export const readable=(s:string)=>s.replaceAll('_',' ');
export function Cell({cell}:{cell:ReportCell}){return <span title={cell.state==='value'?`${cell.value} ${readable(cell.unit)}`:cell.reason}>{cell.state==='value'?`${cell.display} ${readable(cell.unit)}`:`${readable(cell.state)} — ${cell.reason}`}</span>;}
export default function EnergyAssessment({value,label}:{value?:Assessment;label:string}){
  return <section className="energy-assessment" aria-label={`${label} energy assessment`} data-status={value?.scientific.status??'unassessed'}>
    <strong>{label} · {value?readable(value.scientific.status):'Assessment pending — unassessed'}</strong>
    {value?<><p>{value.scientific.reason}</p><dl><dt>Execution</dt><dd>{readable(value.execution)}</dd><dt>Evidence check</dt><dd>{readable(value.evidence_integrity)}</dd><dt>Physical inputs</dt><dd>{readable(value.current_physical_inputs)}</dd><dt>Processed results</dt><dd>{readable(value.current_processed_results)}</dd><dt>Carbon / derived results</dt><dd>{readable(value.current_derived_results)}</dd></dl><details><summary>Method and tested scope</summary><p>{value.scientific.method.method_id} · EnergyPlus {value.scientific.method.engine_version}</p><p>{value.scientific.tested_scope}</p><p>Evidence: {value.scientific.evidence_ids.join(', ')||'None establishing general qualification'}</p></details></>:<p>Completion, retained evidence, input currency and scientific qualification are separate. No scientific acceptance is inferred while loading.</p>}
  </section>;
}
