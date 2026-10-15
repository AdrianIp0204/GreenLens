import {useEffect,useRef,useState} from 'react';
import type {ReferenceWorkbenchDraft} from './types';
import {isReferenceWorkbenchDraft} from './reference-drafts';

// Local UI drafts are explicitly separate from portable accepted scope records.
export function useReferenceDrafts(projectId:string|undefined,apply:(draft:ReferenceWorkbenchDraft)=>Promise<boolean>){
  const entries=useRef(new Map<string,ReferenceWorkbenchDraft>()),current=useRef(projectId);current.current=projectId;
  const mounted=useRef(true);
  const [,refresh]=useState(0),[warning,setWarning]=useState('');
  const [guard,setGuard]=useState<{reason:string;error:string;applying:boolean}|null>(null);
  const waiting=useRef<{id:string;resolve:(value:boolean)=>void}|null>(null);
  useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;const pending=waiting.current;waiting.current=null;pending?.resolve(false);};},[]);
  useEffect(()=>{if(waiting.current&&waiting.current.id!==projectId){const pending=waiting.current;waiting.current=null;setGuard(null);pending.resolve(false);}},[projectId]);
  const get=()=>{
    if(!current.current)return null;
    const id=current.current;
    if(!entries.current.has(id)){
      let value:ReferenceWorkbenchDraft={schema_version:'greenlens.reference-workbench-draft.v1',document_id:id,import:null,scope:null};
      try{const raw=window.localStorage.getItem(`greenlens.reference-draft.v1:${id}`);if(raw&&raw.length<=8388608){const parsed:unknown=JSON.parse(raw);if(isReferenceWorkbenchDraft(parsed,id))value=parsed;}}
      catch{/* An unavailable local store does not accept scientific inputs. */}
      entries.current.set(id,value);
    }
    return entries.current.get(id)!;
  };
  const update=(draft:ReferenceWorkbenchDraft)=>{
    if(draft.document_id!==current.current)return;
    entries.current.set(draft.document_id,draft);refresh(n=>n+1);
    try{const bytes=JSON.stringify(draft);if(bytes.length>8388608)throw Error('Draft exceeds local storage bound');window.localStorage.setItem(`greenlens.reference-draft.v1:${draft.document_id}`,bytes);setWarning('');}
    catch{setWarning('Reference draft remains in this window; local persistence is unavailable. Apply or keep the window open.');}
  };
  const discard=()=>{const d=get();if(d)update({...d,import:null,scope:null});};
  // The panel may unmount while a Rust action finishes. Consume only the exact
  // accepted category from the live document store, never an old panel snapshot.
  const consume=(kind:'import'|'scope',consumed:string,owner:string)=>{
    if(!mounted.current||current.current!==owner)return false;
    const live=get();
    if(!live||live[kind]===null||JSON.stringify(live[kind])!==consumed)return false;
    update({...live,[kind]:null});return true;
  };
  const beforeAction=(reason:string)=>{
    const d=get();if(!d||(!d.import&&!d.scope))return Promise.resolve(true);
    if(waiting.current)return Promise.resolve(false);
    return new Promise<boolean>(resolve=>{waiting.current={id:d.document_id,resolve};setGuard({reason,error:'',applying:false});});
  };
  const resolve=async(choice:'apply'|'discard'|'cancel')=>{
    const w=waiting.current;if(!w||guard?.applying)return;
    if(choice==='cancel'||current.current!==w.id){waiting.current=null;setGuard(null);w.resolve(false);return;}
    if(choice==='apply'){
      const d=get();if(!d)return;
      setGuard(g=>g&&({...g,applying:true,error:''}));
      try{
        if(!await apply(d))throw Error('Apply did not finish. Drafts and accepted inputs are retained.');
        if(current.current!==w.id||entries.current.get(w.id)!==d)throw Error('A newer draft was entered while Apply was pending. Review it before continuing.');
      }catch(e){setGuard(g=>g&&({...g,applying:false,error:String(e)}));return;}
    }
    discard();waiting.current=null;setGuard(null);w.resolve(true);
  };
  return {draft:get(),update,discard,consume,beforeAction,resolve,guard,warning};
}
