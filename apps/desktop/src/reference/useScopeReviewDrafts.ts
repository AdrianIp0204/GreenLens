import {useEffect,useRef,useState} from 'react';
import type {ScopeCarryForwardPlan,ScopeReviewDecision,ReferenceScopeDraft} from './types';
import {ScopeReviewDraftStore,reviewDecisions,sameReviewValue,type ScopeReviewDraft} from './scope-review-drafts';

export function useScopeReviewDrafts(projectId:string|undefined,designId:string,apply:(draft:ScopeReviewDraft)=>Promise<boolean>){
  const [store]=useState(()=>{try{return new ScopeReviewDraftStore(window.localStorage);}catch{return new ScopeReviewDraftStore();}}),[,refresh]=useState(0);
  const [guard,setGuard]=useState<{reason:string;error:string;applying:boolean;designs:string[]}|null>(null);
  const current=useRef({projectId,designId});current.current={projectId,designId};
  const applying=useRef(false),mounted=useRef(true),waiting=useRef<{id:string;resolve:(value:boolean)=>void}|null>(null);
  const changed=()=>{if(mounted.current)refresh(n=>n+1);};
  const pending=()=>[...store.entries.values()].filter(d=>d.document_id===current.current.projectId);
  useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;waiting.current?.resolve(false);waiting.current=null;};},[]);
  useEffect(()=>{if(waiting.current&&waiting.current.id!==projectId){const w=waiting.current;waiting.current=null;setGuard(null);w.resolve(false);}},[projectId]);
  const stage=(plan:ScopeCarryForwardPlan,contextKey:string)=>{const c=current.current;if(!c.projectId||!['A','B'].includes(c.designId))throw Error('Open a supported document and design before review.');if(store.get(c.projectId,c.designId))throw Error('A review draft already exists for this design. Accept or discard it explicitly first.');store.set({schema_version:plan.schema_version.replace('scope-carry-forward','scope-review-draft') as ScopeReviewDraft['schema_version'],document_id:c.projectId,design_id:c.designId as 'A'|'B',context_key:contextKey,plan,...(plan.candidate_scope?{candidate:{value:structuredClone(plan.candidate_scope),numbers:{}}}:{}),decisions:{}});changed();};
  const updateDecision=(rowId:string,decision:Omit<ScopeReviewDecision,'row_id'>|null)=>{const c=current.current,d=store.get(c.projectId,c.designId);if(!d||applying.current)return;const decisions={...d.decisions};if(decision)decisions[rowId]={...decision,row_id:rowId};else delete decisions[rowId];store.set({...d,decisions});changed();};
  const updateCandidate=(candidate:ReferenceScopeDraft)=>{const c=current.current,d=store.get(c.projectId,c.designId);if(!d||applying.current||d.schema_version==='greenlens.scope-review-draft.v1')return;store.set({...d,candidate});changed();};
  const refreshPlan=(plan:ScopeCarryForwardPlan,contextKey:string,candidateSignature:string)=>{
    const c=current.current,d=store.get(c.projectId,c.designId);
    if(!d||applying.current||d.context_key!==contextKey||d.plan.parent_revision_sha256!==plan.parent_revision_sha256||JSON.stringify(d.candidate)!==candidateSignature)throw Error('Candidate or document changed during review. The newer draft remains intact.');
    const decisions=Object.fromEntries(Object.entries(d.decisions).filter(([id])=>sameReviewValue(d.plan.rows.find(r=>r.id===id),plan.rows.find(r=>r.id===id))));
    store.set({...d,plan,decisions});changed();
  };
  const discard=()=>{const c=current.current;if(c.projectId&&!applying.current){store.delete(c.projectId,c.designId);changed();}};
  const applyOne=async(d:ScopeReviewDraft)=>{
    if(applying.current||current.current.projectId!==d.document_id)return false;
    reviewDecisions(d);applying.current=true;changed();
    try{if(!await apply(d))return false;if(current.current.projectId!==d.document_id||store.get(d.document_id,d.design_id)!==d)throw Error('A different review draft is now active; it was not discarded.');store.delete(d.document_id,d.design_id);changed();return true;}
    finally{applying.current=false;changed();}
  };
  const applyCurrent=async()=>{const c=current.current,d=store.get(c.projectId,c.designId);return d?applyOne(d):false;};
  const beforeAction=(reason:string)=>{const drafts=pending();if(!drafts.length)return Promise.resolve(true);if(waiting.current||applying.current)return Promise.resolve(false);return new Promise<boolean>(resolve=>{waiting.current={id:projectId!,resolve};setGuard({reason,error:'',applying:false,designs:drafts.map(d=>d.design_id)});});};
  const resolve=async(choice:'apply'|'discard'|'cancel')=>{
    const w=waiting.current;if(!w||guard?.applying)return;
    if(choice==='cancel'||current.current.projectId!==w.id){waiting.current=null;setGuard(null);w.resolve(false);return;}
    const drafts=pending();
    if(choice==='apply'){
      if(drafts.length>1){setGuard(g=>g&&({...g,error:'Resolve each design review separately in Reference before continuing. A review must not partially apply another design review.'}));return;}
      setGuard(g=>g&&({...g,applying:true,error:''}));
      try{if(drafts[0]&&!await applyOne(drafts[0]))throw Error('Review Apply did not finish. The draft and accepted history are retained.');}
      catch(e){setGuard(g=>g&&({...g,applying:false,error:String(e)}));return;}
    }else{for(const d of drafts)store.delete(d.document_id,d.design_id);changed();}
    waiting.current=null;setGuard(null);w.resolve(true);
  };
  return{draft:store.get(projectId,designId),pending:pending().map(d=>d.design_id),stage,updateDecision,updateCandidate,refreshPlan,discard,applyCurrent,beforeAction,resolve,guard,warning:store.warning,applying:applying.current};
}
