import type {Project} from '../types';
import {useEnergyRead} from './useEnergyRead';
import type {ThermalInspectorTarget} from './thermal-inspector-target';
export interface ThermalIssue extends ThermalInspectorTarget {code:string;kind:string;severity:'error'|'warning'|'information';message:string;remedy:string}
type Preflight={schema_version:string;can_prepare:boolean;issues:ThermalIssue[];coverage:string};
export default function EnergyPreflight({project,alternative,onObject}:{project:Project;alternative:string;onObject:(target:ThermalInspectorTarget)=>void}){
  const check=useEnergyRead<Preflight>('energy_preflight',{project,alternativeId:alternative});
  return <section aria-label="Calculation preflight"><h3>Calculation preflight</h3>{check.loading&&<p role="status">Checking accepted model and adapter restrictions…</p>}{check.error&&<p role="alert">{check.error}</p>}{check.value&&<><p>{check.value.can_prepare?'Accepted inputs can be prepared. Native dependencies are verified again at execution.':'Resolve the blocking issues before simulation.'}</p><p>{check.value.coverage}</p><ul>{check.value.issues.map((issue,i)=><li key={i} className={`preflight-${issue.severity}`}><strong>{issue.severity} · {issue.kind.replaceAll('_',' ')}</strong><br/><button onClick={()=>onObject({object:{...issue.object},field_path:issue.field_path})} title={issue.field_path}>{issue.object.id} · {issue.field_path}</button><p>{issue.message} {issue.remedy}</p></li>)}</ul></>}</section>;
}
