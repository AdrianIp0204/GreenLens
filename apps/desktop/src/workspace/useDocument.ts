import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Channel, invoke, isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { Model, RunRecord, ValidatedModel } from '../types';
import type { ClimateJobStatus as JobStatus, ClimateProjectDocument as Project, ClimateProjectView as ProjectView, ClimateRunRecord, CurrentClimateResult, Study, StudyHistorySnapshot, WeatherSource, WeatherSourceProfile } from '../climate-types';
import { defaultDisplay, type ViewportDisplayState } from '../viewport/types';
import { DocumentHistory, completedJobResult, documentSignature, hasPendingEvidence, protectReplacement, type EnergyDraftBatch } from './document.mjs';
import { activeJob } from '../presentation.mjs';
import { validatedEditLockReason } from './locks.mjs';
import { useEnergyDrafts } from '../energy/useEnergyDrafts';
import { useThermalPresentation } from '../energy/thermal-presentation';
import type { EnergyDesign, EnergyRunRecord, EnergyCalendar, EnergyMetadataChange } from '../energy/types';
import type { OpenOperation, OpenProgress, ProjectAdmission, ProjectVerificationReceipt } from './project-intake';
import type { Session } from '../energy/EnergySensitivity';
import type {ArchitecturalScope,ReferenceImportOptions,ReferenceProgress} from '../reference/types';
import {useReferenceDrafts} from '../reference/useReferenceDrafts';
import {useScopeReviewDrafts} from '../reference/useScopeReviewDrafts';
import {scopeReviewContextKey,reviewDecisions,reviewCandidate} from '../reference/scope-review-drafts';
import {parseReferenceImportDraft,parseReferenceScopeDraft} from '../reference/reference-drafts';
import {reportRequestBinding,type ReportOutput} from '../energy/report-request';
import {usePendingReportIssues} from '../energy/usePendingReportIssues';
import type {ReportIssueAction,ReportIssueActionResult} from '../energy/report-issues';
import {assertIssueInspection} from '../energy/report-issue-response.mjs';

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
const restoreEnergyBatch=async(project:Project,batch:EnergyDraftBatch)=>{
  let next=project;
  if(Object.keys(batch.designs).length)next=await restoreEnergyGroup(next,batch.designs);
  if(batch.reportSettings!==undefined)next=await configureEnergyMetadata(next,{kind:'report_settings',...batch.reportSettings});
  if(batch.reportIssue!==undefined)next=await configureEnergyMetadata(next,{kind:'report_issue',metadata:batch.reportIssue});
  return next;
};
type GuardChoice = 'save'|'discard'|'cancel';
export function useDocument() {
  const [project,setProject] = useState<Project|null>(null);
  const pendingReportIssues=usePendingReportIssues(project?.id);
  const [alternative,setAlternative] = useState('A');
  const [validated,setValidated] = useState<ValidatedModel|null>(null);
  const [results,setResults] = useState<RunRecord[]>([]);
  const [climateResults,setClimateResults] = useState<ClimateRunRecord[]>([]);
  const [energyResults,setEnergyResults] = useState<EnergyRunRecord[]>([]);
  const [sensitivitySnapshots,setSensitivitySnapshots]=useState<Record<string,Session>>({});
  const [projectVerification,setProjectVerification]=useState<ProjectVerificationReceipt|null>(null);
  const [referenceProgress,setReferenceProgress]=useState<ReferenceProgress|null>(null);
  const referenceTicket=useRef<number|null>(null);
  const [opening,setOpening]=useState<OpenOperation|null>(null);
  const openingRef=useRef<OpenOperation|null>(null);
  const mounted=useRef(true);
  const [studyDefaults,setStudyDefaults] = useState<Study|null>(null);
  const [currentClimateState,setCurrentClimateState] = useState<{key:string;result:CurrentClimateResult|null}|null>(null);
  const [display,setDisplay] = useState<ViewportDisplayState>(defaultDisplay);
  const [path,setPath] = useState('');
  const [revision,setRevision] = useState<string|null>(null);
  const [savedSignature,setSavedSignature] = useState('');
  const [documentId,setDocumentId] = useState(0);
  // Session-scoped UI receipt. It survives panel/discipline unmounts and Save As,
  // but never follows a different or newly opened document into its workspace.
  const [reportCompletion,setReportCompletion]=useState<{documentId:number;projectId:string;output:ReportOutput}|null>(null);
  const currentDocumentId=useRef(documentId);currentDocumentId.current=documentId;
  const latestReport=reportCompletion?.documentId===documentId&&reportCompletion.projectId===project?.id?reportCompletion.output:null;
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
  const acceptedSignature=useMemo(()=>documentSignature(project),[project]);
  const dirty=Boolean(project && acceptedSignature!==savedSignature);
  const running=activeJob(job);
  const pending=hasPendingEvidence(job,results,climateResults,energyResults);
  const acceptProject = useCallback((value:ProjectView,directory:string,reset=false)=>{
    const signature=documentSignature(value.project);
    setProject(value.project);setRevision(value.revision);setResults(value.results);setClimateResults(value.climate_results??[]);setPath(directory);setSavedSignature(signature);
    setEnergyResults(value.energy_results??[]);
    setSensitivitySnapshots(value.energy_sensitivity_sessions??{});setProjectVerification(value.project_verification??null);
    state.current={...state.current,project:value.project,revision:value.revision,results:value.results,climateResults:value.climate_results??[],energyResults:value.energy_results??[],path:directory,savedSignature:signature};
    if(reset){setDisplay((value.project.display_state as ViewportDisplayState|null)??defaultDisplay());history.current.reset();setHistoryVersion(v=>v+1);setAlternative('A');setDocumentId(v=>v+1);setValidated(null);}
  },[]);
  const newDocument=useCallback(async()=>{
    const value=await invoke<Project>('example');
    setEnergyResults([]);
    setSensitivitySnapshots({});setProjectVerification(null);
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
  const runAction=async(body:()=>Promise<void>,reason='this document action',skipDrafts=false,skipAuthoring=false,skipReference=false)=>{
    if(authoringOpen.current&&!skipAuthoring){
      if(authoringRequestPending.current)return;
      if(!authoringLifecycle.current){setError('Finish, Discard or Cancel the geometry operation before another document action.');return;}
      authoringRequestPending.current=true;try{if(!await authoringLifecycle.current(reason))return;}finally{authoringRequestPending.current=false;}
    }
    if(!skipDrafts&&!await scopeReviewDrafts.beforeAction(reason))return;
    if(!skipReference&&!skipDrafts&&!await referenceDrafts.beforeAction(reason))return;
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
    const entry=(direction==='undo'?history.current.past:history.current.future).at(-1);if(energyLocked&&(entry?.kind==='energy'||entry?.kind==='energy_group'||entry?.kind==='energy_batch'&&Object.keys(entry.before.designs).length))throw new Error('Unlock thermal inputs before restoring a thermal edit.');
    const next=await history.current.traverse(s.project,direction,validateInStudy,checkEditLocks,restoreStudy,configureEnergy,configureEnergyMetadata,restoreEnergyGroup,restoreEnergyBatch,configureScope);setProject(next.project);setAlternative(next.alternativeId);setValidated(next.validated);state.current.project=next.project;state.current.validated=next.validated;setHistoryVersion(v=>v+1);setMessage(`${direction==='undo'?'Undid':'Redid'} ${next.label}.`);
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
  const commitEnergyBatch=async(batch:EnergyDraftBatch,canCommit:()=>boolean):Promise<boolean>=>{
    lastEnergyError.current='';
    const s=state.current;
    if(energyLocked&&Object.keys(batch.designs).length){lastEnergyError.current='Unlock thermal inputs before editing.';setError(lastEnergyError.current);return false;}
    if(!s.project||actionBusy.current||collecting.current||activeJob(s.job)){lastEnergyError.current='Finish the active operation before applying thermal drafts.';setError(lastEnergyError.current);return false;}
    actionBusy.current=true;setBusy(true);setError('');
    try{
      const next=await history.current.editEnergyBatch(s.project,s.alternative,batch,'Thermal and report drafts',restoreEnergyBatch,()=>state.current.project===s.project&&canCommit());
      setProject(next.project);state.current.project=next.project;setHistoryVersion(v=>v+1);setMessage(next.changed?'All drafts accepted by Rust together. One undo entry.':'No accepted input change.');return true;
    }catch(e){lastEnergyError.current=`Draft batch rejected; document and history unchanged. ${errorText(e)}`;setError(lastEnergyError.current);return false;}
    finally{actionBusy.current=false;setBusy(false);}
  };
  const energyDrafts=useEnergyDrafts(project,commitEnergy,()=>lastEnergyError.current,value=>commitEnergyMetadata({kind:'report_settings',...value},'Report settings draft',true),metadata=>commitEnergyMetadata({kind:'report_issue',metadata},'Report issue draft',true),commitEnergyBatch);
  const configureScope=(project:Project,scope:ArchitecturalScope|null,restoring:boolean,revisionSha256?:string|null)=>revisionSha256!==undefined?invoke<Project>('reference_revision_activate',{project,revisionSha256}):invoke<Project>('reference_scope_apply',{path:state.current.path,project,scope,restoring});
  const commitReferenceScope=async(scope:ArchitecturalScope|null)=>{
    const s=state.current;if(!s.project||actionBusy.current||collecting.current||activeJob(s.job)){setError('Finish the active operation before editing architectural scope.');return false;}
    actionBusy.current=true;setBusy(true);setError('');
    try{const next=await history.current.editScope(s.project,s.alternative,scope,scope?'Architectural scope':'Clear architectural scope',configureScope);setProject(next.project);state.current.project=next.project;setHistoryVersion(v=>v+1);setMessage('Scope accepted by Rust as one derived-document transaction. Physical simulation inputs are unchanged.');return true;}
    catch(e){setError(`Scope rejected; draft and history retained. ${errorText(e)}`);return false;}
    finally{actionBusy.current=false;setBusy(false);}
  };
  const acceptScopeRevision=async(plan:import('../reference/types').ScopeCarryForwardPlan,scope:ArchitecturalScope,decisions:import('../reference/types').ScopeReviewDecision[],reviewContext?:{key:string;design:string})=>{
    // Resolve ordinary drafts first; a changed accepted context is rejected by
    // Rust's exact plan binding, never silently carried forward by the UI.
    if(!await referenceDrafts.beforeAction('accepting a scope revision')||!await energyDrafts.beforeAction('accepting a scope revision'))return false;
    const s=state.current;if(!s.project||!s.path||actionBusy.current||collecting.current||activeJob(s.job)){setError('Finish the active operation before accepting a scope revision.');return false;}
    if(reviewContext&&scopeReviewContextKey(s.path,s.project,reviewContext.design)!==reviewContext.key){setError('Accepted inputs changed while resolving drafts. The scope review remains intact but is stale; prepare a new review explicitly.');return false;}
    actionBusy.current=true;setBusy(true);setError('');
    try{const next=await history.current.acceptScopeRevision(s.project,s.alternative,'Reviewed scope revision',p=>invoke<Project>('reference_revision_accept',{path:s.path,project:p,parentRevisionSha256:plan.parent_revision_sha256,expectedPlanSha256:plan.plan_sha256,scope,decisions}));setProject(next.project);state.current.project=next.project;setHistoryVersion(v=>v+1);setMessage('Reviewed scope revision accepted. Earlier revisions and issued evidence are retained.');return true;}
    catch(e){setError(`Revision rejected; review and accepted history retained. ${errorText(e)}`);return false;}finally{actionBusy.current=false;setBusy(false);}
  };
  const importReference=async(options:ReferenceImportOptions)=>{
    let imported=false;
    await runAction(async()=>{
      if(activeJob(state.current.job)||hasPendingEvidence(state.current.job,state.current.results,state.current.climateResults,state.current.energyResults))throw Error('Finish or save the current calculation before importing architecture.');
      const file=await invoke<string|null>('dialog_open_reference_glb',{initialPath:null});if(!file)return;
      if(!(await save()))return;
      const s=state.current,ticket=await invoke<number>('prepare_project_verification');
      if(!mounted.current){await invoke('cancel_project_verification',{ticket});throw Error('Document closed before reference import started.');}
      referenceTicket.current=ticket;
      const channel=new Channel<ReferenceProgress>();channel.onmessage=p=>{if(referenceTicket.current===ticket)setReferenceProgress(p);};
      setReferenceProgress({stage:'reading local GLB',completed:0,total:1,processed_bytes:0});
      try{const view=await invoke<ProjectView>('reference_import',{path:s.path,project:s.project,expectedRevision:s.revision,file,options,ticket,onProgress:channel});
        if(!mounted.current)return;acceptProject(view,s.path);setMessage('Admitted reference assets saved. Prior scope remains revision-bound; review mapping and measure dimensions before analytical use.');imported=true;
      }finally{referenceTicket.current=null;setReferenceProgress(null);}
    },'reference import',false,false,true);
    return imported;
  };
  const referenceDrafts=useReferenceDrafts(project?.id,async draft=>{
    if(draft.import&&draft.scope)throw Error('Apply the import or scope separately in the Reference panel; importing a new revision cannot silently rebind the scope draft.');
    if(draft.import)return importReference(parseReferenceImportDraft(draft.import));
    if(draft.scope)return commitReferenceScope(parseReferenceScopeDraft(draft.scope));
    return true;
  });
  const scopeReviewDrafts=useScopeReviewDrafts(project?.id,alternative,draft=>acceptScopeRevision(draft.plan,reviewCandidate(draft),reviewDecisions(draft),{key:draft.context_key,design:draft.design_id}));
  const cancelReferenceImport=async()=>{if(referenceTicket.current!==null)await invoke('cancel_project_verification',{ticket:referenceTicket.current});};
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
  const cancelOpening=async()=>{
    const active=openingRef.current;if(!active)return;
    active.cancelled=true;setOpening({...active});
    await invoke('cancel_project_verification',{ticket:active.ticket});
  };
  const verifyCandidate=async(directory:string,admission:ProjectAdmission)=>{
    const ticket=await invoke<number>('prepare_project_verification');
    if(!mounted.current){await invoke('cancel_project_verification',{ticket});throw new Error('Document closed before project verification started.');}
    const operation:OpenOperation={ticket,name:admission.project_name,cancelled:false,progress:null};
    openingRef.current=operation;setOpening({...operation});
    const channel=new Channel<OpenProgress>();
    channel.onmessage=progress=>{if(openingRef.current===operation){operation.progress=progress;setOpening({...operation});}};
    try{
      const candidate=await invoke<ProjectView>('verify_project',{path:directory,admission,ticket,onProgress:channel});
      if(operation.cancelled||openingRef.current!==operation)throw new Error('Project opening cancelled; the previous document and evidence are retained.');
      return candidate;
    }finally{if(openingRef.current===operation){openingRef.current=null;setOpening(null);}}
  };
  useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;if(referenceTicket.current!==null)void invoke('cancel_project_verification',{ticket:referenceTicket.current});const operation=openingRef.current;if(operation){operation.cancelled=true;openingRef.current=null;void invoke('cancel_project_verification',{ticket:operation.ticket});}};},[]);
  const replace=async(kind:'New'|'New Energy'|'Energy example'|'Open'|'Close')=>{
    let selected:string|null=null,admission:ProjectAdmission|null=null;
    if(kind==='Open'){
      if(actionBusy.current||collecting.current){setError('Wait for the current document operation to finish.');return;}
      try{selected=await invoke<string|null>('dialog_open_project',{initialPath:state.current.path||null});if(!selected)return;admission=await invoke<ProjectAdmission>('admit_project',{path:selected});}
      catch(e){setError(errorText(e));return;}
    }
    return runAction(async()=>{
    if(kind==='New Energy'){setEnergyNewOpen(true);return;}
    let discardPending=false;
    const s=state.current;
    await protectReplacement({needsGuard:documentSignature(s.project)!==s.savedSignature||activeJob(s.job)||Boolean(hasPendingEvidence(s.job,s.results,s.climateResults,s.energyResults)),
      decide:()=>decide(kind),save:async()=>{await cancelAndWait();return save();},
      discard:async()=>{discardPending=true;},
      proceed:async()=>{
        if(kind==='Energy example'){const destination=await invoke<string|null>('dialog_save_project',{initialPath:null});if(!destination)return;await cancelAndWait();const value=await invoke<ProjectView>('energy_create_example',{path:destination});await invoke('discard_job');setJob(null);state.current.job=null;acceptProject(value,destination,true);setMessage('Original synthetic thermal example saved; no engine has run.');}
        else if(kind==='New'){await invoke<Project>('example');await invoke('discard_job');await newDocument();}
        else if(kind==='Open'&&selected&&admission){const value=await verifyCandidate(selected,admission);await cancelAndWait();await invoke('discard_job');setJob(null);state.current.job=null;setCollectionError(false);acceptProject(value,selected,true);setMessage('Project opened; retained evidence integrity and arithmetic checked once per run. Scientific qualification is separate.');}
        else {await invoke('prepare_close',{discardPending});closing.current=true;try{await getCurrentWindow().destroy();}catch(e){closing.current=false;throw e;}}
      }});
  },kind);
  };
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
      if(result.session){const session=result.session as Session;setSensitivitySnapshots(previous=>({...previous,[session.reference.id]:session}));}
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
  const energyReport=async(a:string,b:string,typstDir:string,htmlOnly:boolean,portableZip:boolean)=>{let output:ReportOutput|undefined;await runAction(async()=>{
    if(activeJob(state.current.job))throw new Error('Finish or cancel the calculation before rendering reports.');
    if(!(await save()))return;
    const destination=await invoke<string|null>('dialog_energy_report',{portableZip});if(!destination)return;
    const request=state.current,requestDocumentId=currentDocumentId.current;if(!request.project)throw Error('No open document.');
    const binding=reportRequestBinding(request.path,request.project,a,b,typstDir,htmlOnly,portableZip);
    const artifacts=await invoke<{artifacts?:{path:string}[];renderer_status?:string;dataset_sha256?:string;report_dataset_sha256?:string}>('energy_report',{path:request.path,project:request.project,a,b,destination,typstDir:typstDir.trim()||null,htmlOnly,portableZip});
    output={destination,files:portableZip?[]:(artifacts.artifacts??[]).map(a=>a.path),renderer_status:artifacts.renderer_status??'Portable package completed',message:portableZip?'Portable A/B evidence ZIP completed at the selected path.':'All listed products were finalized and integrity checked.',request_binding:binding,dataset_sha256:artifacts.dataset_sha256??artifacts.report_dataset_sha256};
    if(mounted.current)setReportCompletion({documentId:requestDocumentId,projectId:request.project.id,output});
    setMessage(portableZip?'Portable A/B energy evidence and reports exported.':htmlOnly?'HTML, CSV, JSON and SVG reports written; PDF/PNG explicitly not requested.':'Offline HTML, PDF, PNG, CSV, JSON and SVG reports written.');
  });return output;};
  const reportIssue=async(action:ReportIssueAction,typstDir:string):Promise<ReportIssueActionResult|undefined>=>{
    let result:ReportIssueActionResult|undefined;
    await runAction(async()=>{
      const initial=state.current;
      if(!initial.project?.energy||activeJob(initial.job)||hasPendingEvidence(initial.job,initial.results,initial.climateResults,initial.energyResults))throw Error('Finish, cancel or collect the current calculation before report issuance.');
      if(action.kind==='create'&&!pendingReportIssues.capacityAvailable)throw Error(pendingReportIssues.warning||'Recover existing pending report products before creating another issue; the bounded local recovery inventory is full.');
      if(action.kind!=='recover'&&!await save())return;
      const current=state.current;
      if(!current.path||!current.project)throw Error('Save or reopen the complete project before handling an issued report.');
      let submitted:Record<string,unknown>=action;
      if(action.kind==='regenerate'||action.kind==='export'){
        const destination=await invoke<string|null>('dialog_energy_report',{portableZip:action.kind==='export'});if(!destination)return;
        submitted={...action,destination};
      }
      const requestDocumentId=currentDocumentId.current;
      const reply=await invoke<ReportIssueActionResult>('report_issue_action',{path:current.path,project:current.project,expectedRevision:current.revision,action:submitted,typstDir:typstDir.trim()||null});
      if(reply.status==='pending'){
        if(typeof reply.recovery_token!=='string'||typeof reply.message!=='string')throw Error('Report publication returned an invalid recovery receipt.');
        pendingReportIssues.add(current.project.id,reply.recovery_token,reply.message);
        if(state.current.project?.id!==current.project.id||state.current.path!==current.path||currentDocumentId.current!==requestDocumentId)return;
        result=reply;setMessage('Report products are retained outside the project. Publication did not complete; recover this exact issue after resolving the save conflict.');return;
      }
      if(state.current.project?.id!==current.project.id||state.current.path!==current.path||currentDocumentId.current!==requestDocumentId)return;
      if(reply.view){
        if(reply.view.project.id!==current.project.id||!reply.issue)throw Error('Report issuance returned another document or omitted its immutable issue.');
        assertIssueInspection({project:reply.view.project,id:reply.issue.summary.id},reply.issue);
        acceptProject(reply.view as ProjectView,current.path);
        if(action.kind==='recover')pendingReportIssues.remove(action.token);
      }
      result=action.kind==='export'?{...reply,destination:submitted.destination as string}:reply;
      setMessage(action.kind==='export'?'Complete issued project exported with all retained history and required assets.':action.kind==='regenerate'?'Issued report products regenerated from their exact retained inputs; no simulation was requested.':'Immutable report issue retained. Earlier issues and document edit history remain available.');
    },action.kind==='create'?'issuing this exact report':action.kind==='recover'?'recovering completed report products':'handling retained report evidence');
    return result;
  };
  return {reportIssue,pendingReportIssues,scopeReviewDrafts,acceptScopeRevision,latestReport,referenceDrafts,referenceProgress,importReference,commitReferenceScope,cancelReferenceImport,opening,cancelOpening,projectVerification,sensitivitySnapshots,thermalPresentation,copyEnergyDesign,setEnergyObjectLocks,energyDrafts,registerEnergyAuthoringLifecycle,energyAuthoringOpen,setEnergyAuthoringOpen,energyNewOpen,setEnergyNewOpen,commitEnergyTransaction,newEnergy,project,alternative,setAlternative,model,validated,results,climateResults,energyResults,energyLocked,setEnergyLocked,study,currentClimate,currentClimateLoading,display,setDisplay,path,revision,documentId,job,busy,validating,dirty,running,pending,error,setError,message,setMessage,guard,resolveGuard,collectionError,historyVersion,history:history.current,
    commit,commitStudy,commitEnergy,commitEnergyMetadata,attachEnergyEvidence,attachWeather,importWeather,importEnergyWeather,startEnergy,energySensitivity,energyReport,traverse,replace,save:()=>runAction(async()=>{await save();},'Save'),saveAs:()=>runAction(async()=>{await save(true);},'Save As'),recover,start,cancel:()=>runAction(async()=>{await cancelAndWait();setMessage('Calculation cancelled; process tree joined.');},'Cancel calculation',true),exportEvidence,runAction};
}
