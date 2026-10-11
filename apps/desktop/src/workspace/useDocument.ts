import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { JobStatus, Model, Project, ProjectView, RunRecord, ValidatedModel } from '../types';
import { defaultDisplay, type ViewportDisplayState } from '../viewport/types';
import { DocumentHistory, documentSignature, protectReplacement } from './document.mjs';
import { activeJob } from '../presentation.mjs';
import { editLockReason } from './locks.mjs';

export const errorText = (e: unknown) => typeof e === 'string' ? e : e instanceof Error ? e.message : JSON.stringify(e);
const validate = (model:Model) => invoke<ValidatedModel>('validate', { model });
type GuardChoice = 'save'|'discard'|'cancel';
export function useDocument() {
  const [project,setProject] = useState<Project|null>(null);
  const [alternative,setAlternative] = useState('A');
  const [validated,setValidated] = useState<ValidatedModel|null>(null);
  const [results,setResults] = useState<RunRecord[]>([]);
  const [display,setDisplay] = useState<ViewportDisplayState>(defaultDisplay);
  const [path,setPath] = useState('');
  const [revision,setRevision] = useState<string|null>(null);
  const [savedSignature,setSavedSignature] = useState('');
  const [documentId,setDocumentId] = useState(0);
  const [job,setJob] = useState<JobStatus|null>(null);
  const [busy,setBusy] = useState(false);
  const [validating,setValidating] = useState(false);
  const [error,setError] = useState('');
  const [message,setMessage] = useState('');
  const [guard,setGuard] = useState<string|null>(null);
  const [collectionError,setCollectionError] = useState(false);
  const [historyVersion,setHistoryVersion] = useState(0);
  const history = useRef(new DocumentHistory());
  const state = useRef({project,alternative,validated,results,display,path,revision,job,busy,savedSignature});
  state.current={project,alternative,validated,results,display,path,revision,job,busy,savedSignature};
  const guardResolver=useRef<((choice:GuardChoice)=>void)|null>(null);
  const attemptedCollection=useRef<string|null>(null);
  const collecting=useRef(false);
  const actionBusy=useRef(false);
  const closing=useRef(false);
  const model=project?.alternatives.find(a=>a.id===alternative)?.model??null;
  const dirty=Boolean(project && documentSignature(project)!==savedSignature);
  const running=activeJob(job);
  const pending=Boolean(job?.result && !results.some(r=>r.id===job.result?.id));
  const acceptProject = useCallback((value:ProjectView,directory:string,reset=false)=>{
    setProject(value.project);setRevision(value.revision);setResults(value.results);setPath(directory);setSavedSignature(documentSignature(value.project));
    state.current={...state.current,project:value.project,revision:value.revision,results:value.results,path:directory,savedSignature:documentSignature(value.project)};
    if(reset){setDisplay((value.project.display_state as ViewportDisplayState|null)??defaultDisplay());history.current.reset();setHistoryVersion(v=>v+1);setAlternative('A');setDocumentId(v=>v+1);setValidated(null);}
  },[]);
  const newDocument=useCallback(async()=>{
    const value=await invoke<Project>('example');
    setProject(value);setResults([]);setRevision(null);setSavedSignature('');setPath('');setJob(null);setAlternative('A');setDisplay(defaultDisplay());setDocumentId(v=>v+1);setValidated(null);setCollectionError(false);
    history.current.reset();setHistoryVersion(v=>v+1);attemptedCollection.current=null;
    state.current={...state.current,project:value,path:'',results:[],revision:null,job:null,savedSignature:''};
    setMessage('Synthetic A/B document. Select an entity, or enter a command.');
  },[]);
  useEffect(()=>{let alive=true;if(!isTauri()){setError('Use the native GreenLens application for the Rust document and Radiance backend.');return;}
    invoke<Project>('example').then(value=>{if(!alive)return;setProject(value);setDocumentId(v=>v+1);setMessage('Synthetic A/B document. Select an entity, or enter a command.');}).catch(e=>alive&&setError(errorText(e)));
    return()=>{alive=false;};
  },[]);
  useEffect(()=>{
    if(!model)return;
    if(state.current.validated && JSON.stringify(state.current.validated.model)===JSON.stringify(model)){setValidating(false);return;}
    let live=true;setValidating(true);
    validate(model).then(value=>{if(live){setValidated(value);state.current.validated=value;}}).catch(e=>live&&setError(errorText(e))).finally(()=>live&&setValidating(false));
    return()=>{live=false;};
  },[model]);
  const collect=useCallback(async(status:JobStatus)=>{
    const s=state.current;if(!s.project||!s.path||!status.result||collecting.current)return;
    collecting.current=true;attemptedCollection.current=status.id;
    try { const value=await invoke<ProjectView>('collect_run',{path:s.path,project:{...s.project,display_state:s.display},expectedRevision:s.revision});
      acceptProject(value,s.path);setCollectionError(false);setMessage(`Design ${status.result.alternative_id} completed; immutable evidence saved.`);
    }catch(e){setCollectionError(true);setError(`Completed evidence is retained in memory. ${errorText(e)} Use Recover evidence or Save As a complete copy.`);}
    finally{collecting.current=false;}
  },[acceptProject]);
  useEffect(()=>{
    if(!running)return;
    let live=true;const timer=window.setInterval(()=>{invoke<JobStatus>('job_status').then(status=>{if(live){state.current.job=status;setJob(status);}}).catch(e=>live&&setError(errorText(e)));},350);
    return()=>{live=false;window.clearInterval(timer);};
  },[running]);
  useEffect(()=>{if(job?.state==='completed'&&attemptedCollection.current!==job.id&&!actionBusy.current)void collect(job);},[job,collect,busy]);
  const runAction=async(body:()=>Promise<void>)=>{
    if(actionBusy.current||collecting.current){setError('Wait for the current document operation to finish.');return;}
    actionBusy.current=true;setBusy(true);setError('');
    try{await body();}catch(e){setError(errorText(e));}finally{actionBusy.current=false;setBusy(false);}
  };
  const commit=async(candidate:Model,label:string)=>{
    const s=state.current;if(!s.project||actionBusy.current||collecting.current||activeJob(s.job)){setError('Finish the active document operation or calculation before editing.');return false;}
    actionBusy.current=true;setBusy(true);setError('');
    try{
      const lockReason=editLockReason(s.project.alternatives.find(a=>a.id===s.alternative)!.model,candidate,s.display);if(lockReason)throw new Error(lockReason);
      const next=await history.current.edit(s.project,s.alternative,candidate,label,validate);
      setProject(next.project);setValidated(next.validated);state.current.project=next.project;state.current.validated=next.validated;setHistoryVersion(v=>v+1);
      setMessage(next.changed?`${label} accepted by Rust. One undo entry.`:'No parameter change.');return true;
    }catch(e){setError(`Edit rejected; document and history unchanged. ${errorText(e)}`);return false;}
    finally{actionBusy.current=false;setBusy(false);}
  };
  const traverse=async(direction:'undo'|'redo')=>runAction(async()=>{
    const s=state.current;if(!s.project||activeJob(s.job))throw new Error('Finish or cancel the calculation before undo/redo.');
    const next=await history.current.traverse(s.project,direction,validate);setProject(next.project);setAlternative(next.alternativeId);setValidated(next.validated);state.current.project=next.project;state.current.validated=next.validated;setHistoryVersion(v=>v+1);setMessage(`${direction==='undo'?'Undid':'Redid'} ${next.label}.`);
  });
  const cancelAndWait=async()=>{const status=await invoke<JobStatus|null>('cancel_and_wait');setJob(status);state.current.job=status;return status;};
  const save=async(asCopy=false):Promise<boolean>=>{
    const s=state.current;if(!s.project)throw new Error('No open document.');
    let destination=s.path;
    if(asCopy||!destination){const selected=await invoke<string|null>('dialog_save_project',{initialPath:destination||null});if(!selected)return false;destination=selected;}
    const snapshot={...s.project,display_state:s.display};
    let value:ProjectView;
    if(asCopy||!s.path)value=await invoke<ProjectView>('save_project_as',{sourcePath:s.path||null,destination,project:snapshot,expectedSourceRevision:s.revision,includePending:true});
    else if(s.job?.result&&!s.results.some(r=>r.id===s.job?.result?.id))value=await invoke<ProjectView>('collect_run',{path:destination,project:snapshot,expectedRevision:s.revision});
    else value=await invoke<ProjectView>('save_project',{path:destination,project:snapshot,expectedRevision:s.revision});
    acceptProject(value,destination);setCollectionError(false);setMessage(asCopy?'Complete project copy saved, including referenced run evidence.':'Project saved. Undo history retained.');return true;
  };
  const decide=(reason:string)=>new Promise<GuardChoice>(resolve=>{guardResolver.current=resolve;setGuard(reason);});
  const resolveGuard=(choice:GuardChoice)=>{const resolve=guardResolver.current;guardResolver.current=null;setGuard(null);resolve?.(choice);};
  const replace=async(kind:'New'|'Open'|'Close')=>runAction(async()=>{
    const selected=kind==='Open'?await invoke<string|null>('dialog_open_project',{initialPath:state.current.path||null}):null;
    if(kind==='Open'&&!selected)return;
    if(selected)await invoke<ProjectView>('open_project',{path:selected});
    let discardPending=false;
    const s=state.current;
    await protectReplacement({needsGuard:documentSignature(s.project)!==s.savedSignature||activeJob(s.job)||Boolean(s.job?.result&&!s.results.some(r=>r.id===s.job?.result?.id)),
      decide:()=>decide(kind),save:async()=>{await cancelAndWait();return save();},
      discard:async()=>{discardPending=true;},
      proceed:async()=>{
        if(kind==='New'){await invoke<Project>('example');await invoke('discard_job');await newDocument();}
        else if(kind==='Open'&&selected){const value=await invoke<ProjectView>('open_project',{path:selected});await invoke('discard_job');setJob(null);state.current.job=null;setCollectionError(false);acceptProject(value,selected,true);setMessage('Project opened; immutable run evidence verified.');}
        else {await invoke('prepare_close',{discardPending});closing.current=true;try{await getCurrentWindow().destroy();}catch(e){closing.current=false;throw e;}}
      }});
  });
  const closeRef=useRef(()=>{});closeRef.current=()=>{void replace('Close');};
  useEffect(()=>{if(!isTauri())return;let live=true;let unlisten:(()=>void)|undefined;
    getCurrentWindow().onCloseRequested(event=>{if(closing.current)return;event.preventDefault();closeRef.current();}).then(stop=>{if(live)unlisten=stop;else stop();}).catch(e=>setError(`Close protection could not initialize: ${errorText(e)}`));
    return()=>{live=false;unlisten?.();};
  },[]);
  const recover=async()=>runAction(async()=>{
    const s=state.current;if(!s.path)throw new Error('Use Save As to retain the completed evidence in a new project.');
    const disk=await invoke<ProjectView>('open_project',{path:s.path});
    const value=await invoke<ProjectView>('reconcile_completed_run',{path:s.path,expectedRevision:disk.revision});
    acceptProject(value,s.path,true);setCollectionError(false);setMessage('External project edits retained; completed immutable evidence reconciled. History reset to the reloaded document.');
  });
  const start=async(engineDir:string)=>runAction(async()=>{
    const s=state.current;if(!s.project||!s.validated)throw new Error('Wait for a valid document.');
    if(activeJob(s.job))throw new Error('One calculation is already active.');
    if(s.job?.result&&!s.results.some(r=>r.id===s.job?.result?.id))throw new Error('Recover or save the completed evidence first.');
    if(!(await save()))return;
    attemptedCollection.current=null;
    const status=await invoke<JobStatus>('start_run',{model:s.project.alternatives.find(a=>a.id===s.alternative)!.model,alternativeId:s.alternative,engineDir:engineDir.trim()||null,projectId:s.project.id});
    setJob(status);state.current.job=status;setMessage(`Design ${s.alternative} queued for real Radiance execution.`);
  });
  const exportEvidence=async()=>runAction(async()=>{
    if(!(await save()))return;
    const destination=await invoke<string|null>('dialog_export_evidence',{initialPath:state.current.path||null});if(!destination)return;
    await invoke('export_project',{path:state.current.path,destination});setMessage('Portable evidence ZIP and offline report exported.');
  });
  return {project,alternative,setAlternative,model,validated,results,display,setDisplay,path,revision,documentId,job,busy,validating,dirty,running,pending,error,setError,message,setMessage,guard,resolveGuard,collectionError,historyVersion,history:history.current,
    commit,traverse,replace,save:()=>runAction(async()=>{await save();}),saveAs:()=>runAction(async()=>{await save(true);}),recover,start,cancel:()=>runAction(async()=>{await cancelAndWait();setMessage('Calculation cancelled; process tree joined.');}),exportEvidence,runAction};
}
