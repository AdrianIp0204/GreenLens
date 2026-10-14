import type { useEnergyDrafts } from './useEnergyDrafts';
import type { ReportIssueDraft } from './drafts';

export default function ReportIssueFields({drafts}:{drafts:ReturnType<typeof useEnergyDrafts>}){
  const value=drafts.issue();if(!value)return null;
  const change=(patch:Partial<ReportIssueDraft>)=>drafts.updateIssue(old=>({...old,...patch,dirty:true,error:''}));
  const row=(index:number,key:'revision'|'issue_date'|'description',text:string)=>change({revision_history:value.revision_history.map((r,i)=>i===index?{...r,[key]:text}:r)});
  return <section aria-label="Report issue information"><h3>Report issue information</h3><p role="status">{value.dirty?'Unsaved local issue information · not applied':'Accepted issue information'}</p><p className="energy-note">Presentation only. No person or date is inferred. Blank optional fields remain absent; an issue date is entered explicitly as YYYY-MM-DD.</p>
    <label className="energy-check"><input type="checkbox" checked={value.enabled} onChange={e=>change({enabled:e.target.checked})}/>Use explicit report issue information</label>
    <fieldset disabled={!value.enabled}><label>Report title<input value={value.title} onChange={e=>change({title:e.target.value})}/></label><label>Report revision<input value={value.revision} onChange={e=>change({revision:e.target.value})}/></label><label>Issue date<input placeholder="YYYY-MM-DD or blank" value={value.issue_date} onChange={e=>change({issue_date:e.target.value})}/></label><label>Author<input value={value.author} onChange={e=>change({author:e.target.value})}/></label><label>Preparer<input value={value.preparer} onChange={e=>change({preparer:e.target.value})}/></label>
      <h4>Revision history</h4>{!value.revision_history.length&&<p>No revision history supplied.</p>}
      {value.revision_history.map((item,i)=><fieldset key={i}><legend>History entry {i+1}</legend><label>Revision<input aria-label={`History ${i+1} revision`} value={item.revision} onChange={e=>row(i,'revision',e.target.value)}/></label><label>Issue date<input aria-label={`History ${i+1} issue date`} placeholder="YYYY-MM-DD" value={item.issue_date} onChange={e=>row(i,'issue_date',e.target.value)}/></label><label>Description<textarea aria-label={`History ${i+1} description`} rows={2} value={item.description} onChange={e=>row(i,'description',e.target.value)}/></label><button onClick={()=>change({revision_history:value.revision_history.filter((_,index)=>index!==i)})}>Remove history entry {i+1}</button></fieldset>)}
      <button disabled={value.revision_history.length>=32} onClick={()=>change({revision_history:[...value.revision_history,{revision:'',issue_date:'',description:''}]})}>Add revision history</button>
    </fieldset>
    <button disabled={!value.dirty} onClick={()=>void drafts.applyIssue()}>Validate issue information</button><button disabled={!value.dirty} onClick={drafts.discardIssue}>Discard issue draft</button>{value.error&&<p role="alert">{value.error}</p>}
  </section>;
}
