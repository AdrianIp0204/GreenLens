import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { Model, RunRecord, ValidatedModel } from '../types';
import type { ClimateJobStatus as JobStatus, ClimateProjectDocument as Project, ClimateProjectView as ProjectView, ClimateRunRecord, CurrentClimateResult, Study, StudyHistorySnapshot, WeatherSource, WeatherSourceProfile } from '../climate-types';
import { defaultDisplay, type ViewportDisplayState } from '../viewport/types';
import { DocumentHistory, completedJobResult, documentSignature, hasPendingEvidence, protectReplacement } from './document.mjs';
import { activeJob } from '../presentation.mjs';
import { validatedEditLockReason } from './locks.mjs';
import { useEnergyDrafts } from '../energy/useEnergyDrafts';
import { useThermalPresentation } from '../energy/thermal-presentation';
import type { EnergyDesign, EnergyRunRecord, EnergyCalendar, EnergyMetadataChange } from '../energy/types';

export const errorText = (e: unknown) => typeof e === 'string' ? e : e instanceof Error ? e.message : JSON.stringify(e);
const validate = (model:Model) => invoke<ValidatedModel>('validate', { model });
const validateInStudy = async(model:Model,alternativeId:string,project:Project) => {
  const study=project.climate?.studies[alternativeId];
  if(study)await invoke('study_validate',{model,study});
  return validate(model);
};
const configureStudy=(project:Project,alternativeId:string,study:Study)=>invoke<Project>('study_configure',{project,alternativeId,study});
const restoreStudy=(project:Project,alternativeId:string,snapshot:StudyHistorySnapshot)=>invoke<Project>('study_restore',{project,alternativeId,snapshot});
const configureEnergy=(project:Project,alternativeId:string,design:EnergyDesign)=>invoke<Project>('energy_edit',{project,alternativeId,model:design.model,study:design.study});
const restoreEnergyGroup=(project:Project,designs:Record<string,EnergyDesign>)=>invoke<Project>('energy_restore_designs',{project,designs});
const configureEnergyMetadata=(project:Project,change:EnergyMetadataChange)=>invoke<Project>('energy_metadata',{project,change});
type GuardChoice = 'save'|'discard'|'cancel';
export function useDocument() {
  const [project,setProject] = useState<Project|null>(null);
  const [alternative,setAlternative] = useState('A');
  const [validated,setValidated] = useState<ValidatedModel|null>(null);
  const [results,setResults] = useState<RunRecord[]>([]);
  const [climateResults,setClimateResults] = useState<ClimateRunRecord[]>([]);
  const [energyResults,setEnergyResults] = useState<EnergyRunRecord[]>([]);
  const [studyDefaults,setStudyDefaults] = useState<Study|null>(null);
  const [currentClimateState,setCurrentClimateState] = useState<{key:string;result:CurrentClimateResult|null}|null>(null);
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
  const [energyLocked,setEnergyLocked]=useState(false);
  const [energyAuthoringOpen,setEnergyAuthoringState]=useState(false);
  const [energyNewOpen,setEnergyNewOpen]=useState(false);
  const authoringOpen=useRef(false);
  const authoringLifecycle=useRef<((reason:string)=>Promise<boolean>)|null>(null);
  const authoringRequestPending=useRef(false);
  const registerEnergyAuthoringLifecycle=(request:(reason:string)=>Promise<boolean>)=>{authoringLifecycle.current=request;return()=>{if(authoringLifecycle.current===request)authoringLifecycle.current=null;};};
  const setEnergyAuthoringOpen=(open:boolean)=>{authoringOpen.current=open;setEnergyAuthoringState(open);};
  useEffect(()=>{setEnergyLocked(false);},[documentId]);
  const history = useRef(new DocumentHistory());
  const state = useRef({project,alternative,validated,results,climateResults,energyResults,display,path,revision,job,busy,savedSignature});
  state.current={project,alternative,validated,results,climateResults,energyResults,display,path,revision,job,busy,savedSignature};
  const guardResolver=useRef<((choice:GuardChoice)=>void)|null>(null);
  const attemptedCollection=useRef<string|null>(null);
  const collecting=useRef(false);
  const actionBusy=useRef(false);
  const closing=useRef(false);
  const model=project?.alternatives.find(a=>a.id===alternative)?.model??null;
  const study=project?.climate?.studies[alternative]??studyDefaults;
  // Revision/display-only saves do not change scientific lookup inputs. Keep the
  // last verified projection during that refresh; edits, copies, new documents
  // and replaced evidence still hide it immediately, before any effect runs.
  const currentClimateKey=JSON.stringify({documentId,projectId:project?.id,path,alternative,model,study,runs:project?.climate?.runs??[],weather:project?.climate?.weather??[]});
  const currentClimate=currentClimateState?.key===currentClimateKey?currentClimateState.result:null;
  const currentClimateLoading=Boolean(project&&path&&study&&study.calculation.mode!=='static_overcast'&&currentClimateState?.key!==currentClimateKey);
  const validationKey=JSON.stringify([documentId,alternative,model,study]);
  const dirty=Boolean(project && documentSignature(project)!==savedSignature);
  const running=activeJob(job);
  const pending=hasPendingEvidence(job,results,climateResults,energyResults);
  const acceptProject = useCallback((value:ProjectView,directory:string,reset=false)=>{
    setProject(value.project);setRevision(value.revision);setResults(value.results);setClimateResults(value.climate_results??[]);setPath(directory);setSavedSignature(documentSignature(value.project));
    setEnergyResults(value.energy_results??[]);
    state.current={...state.current,project:value.project,revision:value.revision,results:value.results,climateResults:value.climate_results??[],energyResults:value.energy_results??[],path:directory,savedSignature:documentSignature(value.project)};
    if(reset){setDisplay((value.project.display_state as ViewportDisplayState|null)??defaultDisplay());history.current.reset();setHistoryVersion(v=>v+1);setAlternative('A');setDocumentId(v=>v+1);setValidated(null);}
  },[]);
  const newDocument=useCallback(async()=>{
    const value=await invoke<Project>('example');
    setEnergyResults([]);
    setProject(value);setResults([]);setClimateResults([]);setCurrentClimateState(null);setRevision(null);setSavedSignature('');setPath('');setJob(null);setAlternative('A');setDisplay(defaultDisplay());setDocumentId(v=>v+1);setValidated(null);setCollectionError(false);
    history.current.reset();setHistoryVersion(v=>v+1);attemptedCollection.current=null;
    state.current={...state.current,project:value,path:'',results:[],climateResults:[],energyResults:[],revision:null,job:null,savedSignature:''};
    setMessage('Synthetic A/B document. Select an entity, or enter a command.');
  },[]);
  useEffect(()=>{let alive=true;if(!isTauri()){setError('Use the native GreenLens application for the Rust document and Radiance backend.');return;}
    Promise.all([invoke<Project>('example'),invoke<Study>('study_defaults')]).then(([value,defaults])=>{if(!alive)return;setProject(value);setStudyDefaults(defaults);setDocumentId(v=>v+1);setMessage('Synthetic A/B document. Select an entity, or enter a command.');}).catch(e=>alive&&setError(errorText(e)));
    return()=>{alive=false;};
  },[]);
  useEffect(()=>{
    if(!model||!project)return;
    let live=true;setValidating(true);
    validateInStudy(model,alternative,project).then(value=>{if(live){setValidated(value);state.current.validated=value;}}).catch(e=>{if(live){setValidated(null);state.current.validated=null;setError(errorText(e));}}).finally(()=>live&&setValidating(false));
    return()=>{live=false;};
  },[validationKey]);
  useEffect(()=>{
    if(!project||!path||!study||study.calculation.mode==='static_overcast')return;
    let live=true;
    invoke<CurrentClimateResult>('current_climate_result',{path,project,alternativeId:alternative,study})
      .then(value=>{if(live)setCurrentClimateState({key:currentClimateKey,result:value});}).catch(e=>{if(live){setCurrentClimateState({key:currentClimateKey,result:null});setError(errorText(e));}});
    return()=>{live=false;};
  },[project,path,study,alternative,currentClimateKey]);
  const collect=useCallback(async(status:JobStatus)=>{
    const s=state.current;const completed=completedJobResult(status);if(!s.project||!s.path||!completed||collecting.current)return;
    collecting.current=true;attemptedCollection.current=status.id;
    try { const value=await invoke<ProjectView>('collect_run',{path:s.path,project:{...s.project,display_state:s.display},expectedRevision:s.revision});
      acceptProject(value,s.path);setCollectionError(false);setMessage(`Design ${completed.alternative_id} completed; immutable evidence saved.`);
    }catch(e){setCollectionError(true);setError(`Completed evidence is retained in memory. ${errorText(e)} Use Recover evidence or Save As a complete copy.`);}
    finally{collecting.current=false;}
  },[acceptProject]);
  useEffect(()=>{
    if(!running)return;
    let live=true;const timer=window.setInterval(()=>{invoke<JobStatus>('job_status').then(status=>{if(live){state.current.job=status;setJob(status);}}).catch(e=>live&&setError(errorText(e)));},350);
    return()=>{live=false;window.clearInterval(timer);};
  },[running]);
  useEffect(()=>{if(job?.state==='completed'&&attemptedCollection.current!==job.id&&!actionBusy.current)void collect(job);},[job,collect,busy]);
  const lastEnergyError=useRef('');
  const runAction=async(body:()=>Promise<void>,reason='this document action',skipDrafts=false,skipAuthoring=false)=>{
    if(authoringOpen.current&&!skipAuthoring){
      if(authoringRequestPending.current)return;
      if(!authoringLifecycle.current){setError('Finish, Discard or Cancel the geometry operation before another document action.');return;}
      authoringRequestPending.current=true;try{if(!await authoringLifecycle.current(reason))return;}finally{authoringRequestPending.current=false;}
    }
    if(!skipDrafts&&!await energyDrafts.beforeAction(reason))return;
    if(actionBusy.current||collecting.current){setError('Wait for the current document operation to finish.');return;}
    actionBusy.current=true;setBusy(true);setError('');
    try{await body();}catch(e){lastEnergyError.current=errorText(e);setError(lastEnergyError.current);}finally{actionBusy.current=false;setBusy(false);}
  };
  const checkEditLocks=async(before:Model,_after:Model,afterValidated:ValidatedModel)=>{
    const cached=state.current.validated;
    const beforeValidated=cached&&JSON.stringify(cached.model)===JSON.stringify(before)?cached:await validate(before);
    const reason=validatedEditLockReason(beforeValidated,afterValidated,state.current.display);
    if(reason)throw new Error(reason);
  };
  const commit=async(candidate:Model,label:string)=>{
    const s=state.current;if(!s.project||actionBusy.current||collecting.current||activeJob(s.job)){setError('Finish the active document operation or calculation before editing.');return false;}
    actionBusy.current=true;setBusy(true);setError('');
    try{
      const next=await history.current.edit(s.project,s.alternative,candidate,label,validateInStudy,checkEditLocks);
      setProject(next.project);setValidated(next.validated);state.current.project=next.project;state.current.validated=next.validated;setHistoryVersion(v=>v+1);
      setMessage(next.changed?`${label} accepted by Rust. One undo entry.`:'No parameter change.');return true;
    }catch(e){setError(`Edit rejected; document and history unchanged. ${errorText(e)}`);return false;}
    finally{actionBusy.current=false;setBusy(false);}
  };
  const traverse=async(direction:'undo'|'redo')=>runAction(async()=>{
    const s=state.current;if(!s.project||activeJob(s.job))throw new Error('Finish or cancel the calculation before undo/redo.');
    const entry=(direction==='undo'?history.current.past:history.current.future).at(-1);if(energyLocked&&(entry?.kind==='energy'||entry?.kind==='energy_group'))throw new Error('Unlock thermal inputs before restoring a thermal edit.');
    const next=await history.current.traverse(s.project,direction,validateInStudy,checkEditLocks,restoreStudy,configureEnergy,configureEnergyMetadata,restoreEnergyGroup);setProject(next.project);setAlternative(next.alternativeId);setValidated(next.validated);state.current.project=next.project;state.current.validated=next.validated;setHistoryVersion(v=>v+1);setMessage(`${direction==='undo'?'Undid':'Redid'} ${next.label}.`);
  });
  const commitStudy=async(candidate:Study,label='Study settings')=>{
    const s=state.current;if(!s.project||actionBusy.current||collecting.current||activeJob(s.job)){setError('Finish the active document operation or calculation before editing.');return false;}
    actionBusy.current=true;setBusy(true);setError('');
    try{
      const next=await history.current.editStudy(s.project,s.alternative,candidate,label,configureStudy);
      setProject(next.project);state.current.project=next.project;setCurrentClimateState(null);setHistoryVersion(v=>v+1);
      setMessage(next.changed?`${label} accepted by Rust. One undo entry.`:'No study change.');return true;
    }catch(e){setError(`Study rejected; document and history unchanged. ${errorText(e)}`);return false;}
    finally{actionBusy.current=false;setBusy(false);}
  };
  const cancelAndWait=async()=>{const status=await invoke<JobStatus|null>('cancel_and_wait');setJob(status);state.current.job=status;return status;};
  const commitEnergy=async(candidate:EnergyDesign,label='Thermal inputs',designId=state.current.alternative)=>{
    lastEnergyError.current='';
    if(energyLocked){lastEnergyError.current='Unlock thermal inputs before editing.';setError(lastEnergyError.current);return false;}
    const s=state.current;if(!s.project||actionBusy.current||collecting.current||activeJob(s.job)){lastEnergyError.current='Finish the active operation before editing thermal inputs.';setError(lastEnergyError.current);return false;}
    actionBusy.current=true;setBusy(true);setError('');
    try {const next=await history.current.editEnergy(s.project,designId,candidate,label,configureEnergy);setProject(next.project);state.current.project=next.project;setHistoryVersion(v=>v+1);setMessage(next.changed?`${label} accepted by Rust. One undo entry.`:'No thermal change.');return true;}
    catch(e){lastEnergyError.current=`Thermal edit rejected; document and history unchanged. ${errorText(e)}`;setError(lastEnergyError.current);return false;}
    finally{actionBusy.current=false;setBusy(false);}
  };
  const thermalPresentation=useThermalPresentation(project?.id,alternative);
  const copyEnergyDesign=async(source:'A'|'B',target:'A'|'B',expectedPreviewSha256:string):Promise<boolean>=>{
    if(!await energyDrafts.beforeAction('copying accepted design inputs'))return false;
    const s=state.current;if(energyLocked||!s.project||actionBusy.current||collecting.current||activeJob(s.job)){setError('Finish the active operation and unlock thermal inputs before copying.');return false;}
    actionBusy.current=true;setBusy(true);setError('');
    try{const next=await history.current.editEnergyGroup(s.project,target,[target],`Copy accepted ${source} inputs to ${target}`,p=>invoke<Project>('energy_copy_design_commit',{project:p,source,target,expectedPreviewSha256}));setProject(next.project);state.current.project=next.project;setHistoryVersion(v=>v+1);setMessage(next.changed?`Design ${target} inputs replaced after Rust validation. One undo entry; historical evidence preserved.`:'No accepted design change.');return true;}
    catch(e){setError(`Design copy rejected; document and history unchanged. ${errorText(e)}`);return false;}
    finally{actionBusy.current=false;setBusy(false);}
  };
  const energyDrafts=useEnergyDrafts(project,commitEnergy,()=>lastEnergyError.current,value=>commitEnergyMetadata({kind:'report_settings',...value},'Report settings draft',true),metadata=>commitEnergyMetadata({kind:'report_issue',metadata},'Report issue draft',true));
  const setEnergyObjectLocks=async(ids:string[])=>runAction(async()=>{const s=state.current;if(!s.project||activeJob(s.job))throw Error('Finish the calculation before changing edit locks.');const next=await invoke<Project>('energy_set_locks',{project:s.project,alternativeId:s.alternative,ids});setProject(next);state.current.project=next;setMessage('Thermal edit locks updated. Simulation inclusion is unchanged.');},'changing thermal edit locks');
  const commitEnergyTransaction=async(requests:Record<string,unknown>,expectedPreviewSha256:string,label:string):Promise<boolean>=>{
    if(!await energyDrafts.beforeAction('applying geometry'))return false;
    const s=state.current;if(energyLocked||!s.project||actionBusy.current||collecting.current||activeJob(s.job)){setError('Finish the active operation and unlock thermal inputs before applying geometry.');return false;}
    actionBusy.current=true;setBusy(true);setError('');
    try{const next=await history.current.editEnergyGroup(s.project,s.alternative,Object.keys(requests),label,p=>invoke<Project>('energy_transaction_commit',{project:p,requests,expectedPreviewSha256}));setProject(next.project);state.current.project=next.project;setHistoryVersion(v=>v+1);setMessage(`${label} accepted as one Rust-validated transaction.`);return true;}
    catch(e){setError(`Geometry rejected; accepted inputs and history unchanged. ${errorText(e)}`);return false;}finally{actionBusy.current=false;setBusy(false);}
  };
  const newEnergy=async(zone:unknown,calendar:EnergyCalendar)=>{let created=false;await runAction(async()=>{
    const s=state.current;
    await protectReplacement({needsGuard:documentSignature(s.project)!==s.savedSignature||activeJob(s.job)||hasPendingEvidence(s.job,s.results,s.climateResults,s.energyResults),decide:()=>decide('New Energy'),save:async()=>{await cancelAndWait();return save();},discard:async()=>{},proceed:async()=>{
      const value=await invoke<Project>('energy_new',{zone,calendar});await cancelAndWait();await invoke('discard_job');acceptProject({project:value,revision:'',results:[],climate_results:[],climate_assessments:{},energy_results:[]},'',true);setRevision(null);setSavedSignature('');setJob(null);state.current={...state.current,revision:null,savedSignature:'',job:null};setMessage('New unsaved thermal project; explicit synthetic assumptions, no engine execution.');created=true;
    }});
  },'New Energy Project',false,true);return created;};
  const save=async(asCopy=false):Promise<boolean>=>{
    const s=state.current;if(!s.project)throw new Error('No open document.');
    let destination=s.path;
    if(asCopy||!destination){const selected=await invoke<string|null>('dialog_save_project',{initialPath:destination||null});if(!selected)return false;destination=selected;}
    const snapshot={...s.project,display_state:s.display};
    let value:ProjectView;
    if(asCopy||!s.path)value=await invoke<ProjectView>('save_project_as',{sourcePath:s.path||null,destination,project:snapshot,expectedSourceRevision:s.revision,includePending:true});
    else if(hasPendingEvidence(s.job,s.results,s.climateResults,s.energyResults))value=await invoke<ProjectView>('collect_run',{path:destination,project:snapshot,expectedRevision:s.revision});
    else value=await invoke<ProjectView>('save_project',{path:destination,project:snapshot,expectedRevision:s.revision});
    acceptProject(value,destination);setCollectionError(false);setMessage(asCopy?'Complete project copy saved, including referenced run evidence.':'Project saved. Undo history retained.');return true;
  };
  const decide=(reason:string)=>new Promise<GuardChoice>(resolve=>{guardResolver.current=resolve;setGuard(reason);});
  const resolveGuard=(choice:GuardChoice)=>{const resolve=guardResolver.current;guardResolver.current=null;setGuard(null);resolve?.(choice);};
  const replace=async(kind:'New'|'New Energy'|'Energy example'|'Open'|'Close')=>runAction(async()=>{
    if(kind==='New Energy'){setEnergyNewOpen(true);return;}
    const selected=kind==='Open'?await invoke<string|null>('dialog_open_project',{initialPath:state.current.path||null}):null;
    if(kind==='Open'&&!selected)return;
    if(selected)await invoke<ProjectView>('open_project',{path:selected});
    let discardPending=false;
    const s=state.current;
    await protectReplacement({needsGuard:documentSignature(s.project)!==s.savedSignature||activeJob(s.job)||Boolean(hasPendingEvidence(s.job,s.results,s.climateResults,s.energyResults)),
      decide:()=>decide(kind),save:async()=>{await cancelAndWait();return save();},
      discard:async()=>{discardPending=true;},
      proceed:async()=>{
        if(kind==='Energy example'){const destination=await invoke<string|null>('dialog_save_project',{initialPath:null});if(!destination)return;await cancelAndWait();const value=await invoke<ProjectView>('energy_create_example',{path:destination});await invoke('discard_job');setJob(null);state.current.job=null;acceptProject(value,destination,true);setMessage('Original synthetic thermal example saved; no engine has run.');}
        else if(kind==='New'){await invoke<Project>('example');await invoke('discard_job');await newDocument();}
        else if(kind==='Open'&&selected){const value=await invoke<ProjectView>('open_project',{path:selected});await invoke('discard_job');setJob(null);state.current.job=null;setCollectionError(false);acceptProject(value,selected,true);setMessage('Project opened; run metadata integrity checked.');}
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
    if(hasPendingEvidence(s.job,s.results,s.climateResults,s.energyResults))throw new Error('Recover or save the completed evidence first.');
    if(!(await save()))return;
    attemptedCollection.current=null;
    const saved=state.current;
    const configured=saved.project?.climate?.studies[s.alternative];
    const status=configured
      ?await invoke<JobStatus>('start_study',{path:saved.path,project:saved.project,alternativeId:s.alternative,study:configured,engineDir:engineDir.trim()||null})
      :await invoke<JobStatus>('start_run',{model:s.project.alternatives.find(a=>a.id===s.alternative)!.model,alternativeId:s.alternative,engineDir:engineDir.trim()||null,projectId:s.project.id});
    setJob(status);state.current.job=status;setMessage(`Design ${s.alternative} queued for real Radiance execution.`);
  });
  const startEnergy=async(engineDir:string)=>runAction(async()=>{
    const s=state.current;if(!s.project?.energy)throw new Error('Create or open an energy document.');
    if(activeJob(s.job)||hasPendingEvidence(s.job,s.results,s.climateResults,s.energyResults))throw new Error('Finish, cancel or save the current calculation first.');
    if(!(await save()))return;attemptedCollection.current=null;const saved=state.current;
    const status=await invoke<JobStatus>('energy_start',{path:saved.path,project:saved.project,alternativeId:saved.alternative,engineDir:engineDir.trim()||null});setJob(status);state.current.job=status;setMessage(`Design ${saved.alternative} queued for EnergyPlus execution; qualification remains separate.`);
  });
  const importEnergyWeather=async(file:string,source:WeatherSource,calendar:EnergyCalendar,applyLocation:boolean,profile:'explicit_fields_v1'|'dry_envelope_v1'='explicit_fields_v1')=>runAction(async()=>{
    if(energyLocked)throw new Error('Unlock thermal inputs before importing weather.');
    if(activeJob(state.current.job))throw new Error('Finish or cancel the calculation before importing weather.');if(!(await save()))return;const s=state.current;
    const value=await invoke<ProjectView>('energy_import_weather',{path:s.path,project:s.project,expectedRevision:s.revision,file,source,calendar,applyLocation,profile});acceptProject(value,s.path);setMessage('Thermal weather admitted and saved with original bytes and explicit location policy.');
  });
  const energySensitivity=async(action:Record<string,unknown>,engineDir=''):Promise<Record<string,unknown>|undefined>=>{
    let result:Record<string,unknown>|undefined;
    await runAction(async()=>{
      const s=state.current;
      if(!s.project?.energy)throw new Error('Create or open an energy document.');
      if(activeJob(s.job)||hasPendingEvidence(s.job,s.results,s.climateResults,s.energyResults))throw new Error('Finish, cancel or collect the current job first.');
      if(!(await save()))return;
      const current=state.current;
      result=await invoke<Record<string,unknown>>('energy_sensitivity_action',{path:current.path,project:current.project,expectedRevision:current.revision,action,engineDir:engineDir.trim()||null});
      if(result.view)acceptProject(result.view as ProjectView,current.path);
      if(result.job){const status=result.job as JobStatus;attemptedCollection.current=null;setJob(status);state.current.job=status;}
      setMessage('Numerical sensitivity step recorded. Execution, integrity and qualification remain separate.');
    },'checking numerical sensitivity');
    return result;
  };
  const commitEnergyMetadata=async(change:EnergyMetadataChange,label:string,skipDrafts=false)=>{let applied=false;await runAction(async()=>{
    const s=state.current;if(!s.project||activeJob(s.job))throw new Error('Finish or cancel the calculation before editing report metadata.');
    const next=await history.current.editEnergyMetadata(s.project,s.alternative,change,label,configureEnergyMetadata);setProject(next.project);state.current.project=next.project;setHistoryVersion(v=>v+1);setMessage(`${label} accepted; physical evidence unchanged.`);applied=true;
  },'changing report settings',skipDrafts);return applied;};
  const attachEnergyEvidence=async(file:string,caption:string,rights:string)=>runAction(async()=>{
    if(activeJob(state.current.job))throw new Error('Finish or cancel the calculation before importing evidence.');
    if(!(await save()))return;const s=state.current;const value=await invoke<ProjectView>('energy_attach',{path:s.path,project:s.project,expectedRevision:s.revision,alternativeId:s.alternative,file,caption,rights});acceptProject(value,s.path);setMessage('Inert evidence retained with model identity; excluded from exports until explicitly selected.');
  });
  const attachWeather=async(assetPath:string)=>runAction(async()=>{
    if(activeJob(state.current.job))throw new Error('Finish or cancel the calculation before importing weather.');
    if(!(await save()))return;
    const s=state.current;
    const value=await invoke<ProjectView>('weather_attach',{path:s.path,project:s.project,expectedRevision:s.revision,assetPath});
    acceptProject(value,s.path);setMessage('Portable weather attached and saved with citation, interpretation and original bytes.');
  });
  const importWeather=async(file:string,profile:WeatherSourceProfile,source:WeatherSource)=>{let imported=false;await runAction(async()=>{
    if(activeJob(state.current.job))throw new Error('Finish or cancel the calculation before importing weather.');
    if(!(await save()))return;
    const s=state.current;
    const value=await invoke<ProjectView>('weather_import_project',{path:s.path,project:s.project,expectedRevision:s.revision,file,profile,source});
    acceptProject(value,s.path);setMessage('Weather imported and saved with original bytes, citation and time interpretation.');
    imported=true;
  });return imported;};
  const exportEvidence=async(selectedRuns?:string[])=>runAction(async()=>{
    if(!(await save()))return;
    const destination=await invoke<string|null>('dialog_export_evidence',{initialPath:state.current.path||null});if(!destination)return;
    await invoke('export_project',{path:state.current.path,destination,selectedRuns:selectedRuns??null});setMessage('Portable evidence ZIP and offline report exported.');
  });
  const energyReport=async(a:string,b:string,typstDir:string,htmlOnly:boolean,portableZip:boolean)=>{let output:import('../energy/ReportWorkbench').ReportOutput|undefined;await runAction(async()=>{
    if(activeJob(state.current.job))throw new Error('Finish or cancel the calculation before rendering reports.');
    if(!(await save()))return;
    const destination=await invoke<string|null>('dialog_energy_report',{portableZip});if(!destination)return;
    const artifacts=await invoke<{artifacts?:{path:string}[];renderer_status?:string}>('energy_report',{path:state.current.path,project:state.current.project,a,b,destination,typstDir:typstDir.trim()||null,htmlOnly,portableZip});
    output={destination,files:portableZip?[]:(artifacts.artifacts??[]).map(a=>a.path),renderer_status:artifacts.renderer_status??'Portable package completed',message:portableZip?'Portable A/B evidence ZIP completed at the selected path.':'All listed products were finalized and integrity checked.'};
    setMessage(portableZip?'Portable A/B energy evidence and reports exported.':htmlOnly?'HTML, CSV, JSON and SVG reports written; PDF/PNG explicitly not requested.':'Offline HTML, PDF, PNG, CSV, JSON and SVG reports written.');
  });return output;};
  return {thermalPresentation,copyEnergyDesign,setEnergyObjectLocks,energyDrafts,registerEnergyAuthoringLifecycle,energyAuthoringOpen,setEnergyAuthoringOpen,energyNewOpen,setEnergyNewOpen,commitEnergyTransaction,newEnergy,project,alternative,setAlternative,model,validated,results,climateResults,energyResults,energyLocked,setEnergyLocked,study,currentClimate,currentClimateLoading,display,setDisplay,path,revision,documentId,job,busy,validating,dirty,running,pending,error,setError,message,setMessage,guard,resolveGuard,collectionError,historyVersion,history:history.current,
    commit,commitStudy,commitEnergy,commitEnergyMetadata,attachEnergyEvidence,attachWeather,importWeather,importEnergyWeather,startEnergy,energySensitivity,energyReport,traverse,replace,save:()=>runAction(async()=>{await save();},'Save'),saveAs:()=>runAction(async()=>{await save(true);},'Save As'),recover,start,cancel:()=>runAction(async()=>{await cancelAndWait();setMessage('Calculation cancelled; process tree joined.');},'Cancel calculation',true),exportEvidence,runAction};
}
