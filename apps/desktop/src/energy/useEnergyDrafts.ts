import { useEffect, useRef, useState } from 'react';
import type { EnergyDesign, EnergyReportMetadata } from './types';
import { EnergyDraftStore, draftCandidate, cleanReportDraft, reportCandidate, cleanIssueDraft, issueCandidate, type EnergyDraft, type ReportSettings, type ReportSettingsDraft, type ReportIssueDraft } from './drafts';

type DraftProject={id:string;energy?:{designs:Record<'A'|'B',EnergyDesign>;workbench?:{report_metadata?:EnergyReportMetadata|null}}&ReportSettings|null};
type Choice='apply'|'discard'|'cancel';
export function useEnergyDrafts(project:DraftProject|null, commit:(design:EnergyDesign,label:string,id:string)=>Promise<boolean>, acceptedError:()=>string,commitReport:(value:ReportSettings)=>Promise<boolean>,commitIssue:(value:EnergyReportMetadata|null)=>Promise<boolean>){
  const [store]=useState(()=>{try{return new EnergyDraftStore(window.localStorage);}catch{return new EnergyDraftStore();}});
  const [,refresh]=useState(0);
  const current=useRef(project);current.current=project;
  const [guard,setGuard]=useState<{reason:string;alternatives:string[];error:string;applying:boolean}|null>(null);
  const waiting=useRef<{projectId:string;resolve:(ok:boolean)=>void}|null>(null);
  useEffect(()=>store.subscribe(()=>refresh(n=>n+1)),[store]);
  const get=(id:string)=>{const p=current.current,d=p?.energy?.designs[id as 'A'|'B'];return p&&d?store.get(p.id,id,d):null;};
  const report=()=>{const p=current.current;return p?.energy?store.report(p.id,p.energy):null;};
  const updateReport=(change:(old:ReportSettingsDraft)=>ReportSettingsDraft)=>{const p=current.current,d=report();if(p&&d)store.setReport(p.id,change(d));};
  const discardReport=()=>{const p=current.current;if(p?.energy)store.setReport(p.id,cleanReportDraft(p.energy));};
  const applyReport=async()=>{const p=current.current,d=report();if(!p?.energy||!d)return false;try{const value=reportCandidate(d,p.energy);if(!await commitReport(value))throw new Error(acceptedError()||'Report settings were rejected.');if(current.current?.id===p.id&&store.reports.get(p.id)===d)store.setReport(p.id,cleanReportDraft(value));return true;}catch(e){if(current.current?.id===p.id)store.setReport(p.id,{...(store.reports.get(p.id)??d),error:e instanceof Error?e.message:String(e)});return false;}};
  const issue=()=>{const p=current.current;return p?.energy?store.issue(p.id,p.energy.workbench?.report_metadata??null):null;};
  const updateIssue=(change:(old:ReportIssueDraft)=>ReportIssueDraft)=>{const p=current.current,d=issue();if(p&&d)store.setIssue(p.id,change(d));};
  const discardIssue=()=>{const p=current.current;if(p?.energy)store.setIssue(p.id,cleanIssueDraft(p.energy.workbench?.report_metadata??null));};
  const applyIssue=async()=>{const p=current.current,d=issue();if(!p?.energy||!d)return false;try{const value=issueCandidate(d,p.energy.workbench?.report_metadata??null);if(!await commitIssue(value))throw new Error(acceptedError()||'Report issue information was rejected.');if(current.current?.id===p.id&&store.issues.get(p.id)===d)store.setIssue(p.id,cleanIssueDraft(value));return true;}catch(e){if(current.current?.id===p.id)store.setIssue(p.id,{...(store.issues.get(p.id)??d),error:e instanceof Error?e.message:String(e)});return false;}};
  const update=(id:string,change:(old:EnergyDraft)=>EnergyDraft)=>{
    const p=current.current,d=get(id);if(!p||!d)return;
    try{store.set(p.id,id,change(d));}catch(e){store.set(p.id,id,{...d,error:String(e)});}
  };
  const discard=(id:string)=>{const p=current.current,d=p?.energy?.designs[id as 'A'|'B'];if(p&&d)store.discard(p.id,id,d);};
  const apply=async(id:string)=>{
    const p=current.current,draft=get(id),accepted=p?.energy?.designs[id as 'A'|'B'];if(!p||!draft||!accepted)return false;
    try{
      const candidate=draftCandidate(draft,accepted);
      if(!await commit(candidate,`Thermal ${id} draft`,id))throw new Error(acceptedError()||'Rust rejected the draft. Accepted inputs and history are unchanged.');
      const next=current.current;
      // A successful commit updates the accepted state synchronously in useDocument.
      // Its React render may follow this continuation; use the accepted candidate here.
      if(next?.id===p.id&&get(id)?.version===draft.version)store.discard(p.id,id,candidate);
      return true;
    }catch(e){if(current.current?.id===p.id)store.set(p.id,id,{...(get(id)??draft),error:e instanceof Error?e.message:String(e)});return false;}
  };
  const beforeAction=(reason:string):Promise<boolean>=>{
    const p=current.current,alternatives=p?store.pending(p.id):[];
    if(waiting.current)return Promise.resolve(false);
    if(!p||!alternatives.length)return Promise.resolve(true);
    return new Promise(resolve=>{waiting.current={projectId:p.id,resolve};setGuard({reason,alternatives,error:'',applying:false});});
  };
  const resolve=async(choice:Choice)=>{
    const pending=waiting.current;if(!pending||guard?.applying)return;
    if(choice==='cancel'){waiting.current=null;setGuard(null);pending.resolve(false);return;}
    if(current.current?.id!==pending.projectId){waiting.current=null;setGuard(null);pending.resolve(false);return;}
    const ids=store.pending(pending.projectId);
    if(choice==='apply'){
      setGuard(g=>g&&({...g,applying:true,error:''}));
      for(const id of ids)if(!await(id==='report settings'?applyReport():id==='report issue'?applyIssue():apply(id))){setGuard(g=>g&&({...g,applying:false,error:(id==='report settings'?report()?.error:id==='report issue'?issue()?.error:get(id)?.error)??'Apply failed; draft retained.'}));return;}
    }else ids.forEach(id=>id==='report settings'?discardReport():id==='report issue'?discardIssue():discard(id));
    if(choice==='apply'&&store.pending(pending.projectId).length){setGuard(g=>g&&({...g,applying:false,error:'A newer draft was entered while Apply was pending. It is retained; review it before continuing.'}));return;}
    waiting.current=null;setGuard(null);pending.resolve(true);
  };
  return {get,update,discard,apply,report,updateReport,discardReport,applyReport,issue,updateIssue,discardIssue,applyIssue,beforeAction,resolve,guard,warning:store.warning,pending:project?store.pending(project.id):[]};
}
