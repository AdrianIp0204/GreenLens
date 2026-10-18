import { useCallback, useEffect, useRef, useState } from 'react';
import { Channel, invoke } from '@tauri-apps/api/core';
import { errorText } from '../workspace/useDocument';
import type { OpenProgress } from '../workspace/project-intake';
import { assertEnergyResponse } from './response.mjs';
import {largeReadQueue,type BoundedRead} from './bounded-read';

const inspectedCommands=new Set(['energy_default_runs','energy_run_catalog','energy_result','energy_rows','energy_navigate','energy_navigate_pair','energy_compare','energy_report_preflight','energy_sensitivity_inspect','energy_sensitivity_export_selection','report_issue_inspect','design_report_inspect','report_issue_product']);
const count=(v:unknown):v is number=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=0;
function validProgress(p:OpenProgress):boolean {
  return Boolean(p&&p.schema_version==='greenlens.project-open-progress.v1'&&typeof p.stage==='string'&&p.stage.length>0&&p.stage.length<=128&&(p.run_id===null||typeof p.run_id==='string'&&p.run_id.length<=256)&&[p.runs_completed,p.runs_total,p.bytes_read,p.files_verified,p.elapsed_ms].every(count)&&p.runs_completed<=p.runs_total&&p.numeric_passes&&typeof p.numeric_passes==='object'&&!Array.isArray(p.numeric_passes)&&Object.keys(p.numeric_passes).length<=5000&&Object.entries(p.numeric_passes).every(([id,n])=>id.length<=256&&count(n)));
}
type ReadState<T>={key:number;value:T|null;error:string;progress:OpenProgress|null;pending:boolean};
type ReadRequest={key:number;command:string;args:Record<string,unknown>|null;sequence:number};
function sameJsonValue(before:unknown,after:unknown):boolean {
  if(Object.is(before,after))return true;
  if(before===null||after===null||typeof before!=='object'||typeof after!=='object')return false;
  if(Array.isArray(before)!==Array.isArray(after))return false;
  if(Array.isArray(before)&&Array.isArray(after)&&before.length!==after.length)return false;
  const left=before as Record<string,unknown>,right=after as Record<string,unknown>,keys=Object.keys(left);
  return keys.length===Object.keys(right).length&&keys.every(key=>Object.hasOwn(right,key)&&sameJsonValue(left[key],right[key]));
}
function sameArguments(before:Record<string,unknown>|null,after:Record<string,unknown>|null):boolean {
  if(before===null||after===null)return before===after;
  const keys=Object.keys(before);
  return keys.length===Object.keys(after).length&&keys.every(key=>Object.hasOwn(after,key)&&(key==='project'?Object.is(before[key],after[key]):sameJsonValue(before[key],after[key])));
}
/** A response can only be shown for the exact request that produced it. */
export function useEnergyRead<T>(command:string,args:Record<string,unknown>|null,requestSequence=0){
  // Inputs are immutable application objects. Keep one shallow argument
  // envelope, not a full-project JSON key plus a parsed duplicate per query.
  // A different project reference conservatively starts another request; small
  // JSON options may be freshly constructed but must still match exactly.
  // this token is a local lifecycle identity, never a scientific/content hash.
  const requestRef=useRef<ReadRequest|null>(null);
  const previous=requestRef.current;
  if(!previous||previous.command!==command||!Object.is(previous.sequence,requestSequence)||!sameArguments(previous.args,args)){
    requestRef.current={key:(previous?.key??0)+1,command,args:args?{...args}:null,sequence:requestSequence};
  }
  const snapshot=requestRef.current!,key=snapshot.key;
  const [state,setState]=useState<ReadState<T>>({key:0,value:null,error:'',progress:null,pending:false});
  const operation=useRef<{key:number;cancel:()=>void}|null>(null);
  const cancel=useCallback(()=>{if(operation.current?.key===key)operation.current.cancel();},[key]);
  useEffect(()=>{
    const {args:request,command}=snapshot;
    if(!request)return;
    let live=true,finished=false,cancelled=false,cancelSent=false,ticket:number|null=null;
    let scheduled:BoundedRead<void>|null=null;
    const inspected=inspectedCommands.has(command);
    setState({key,value:null,error:'',progress:null,pending:true});
    const cancelReserved=()=>{
      if(ticket===null||cancelSent||finished)return;
      cancelSent=true;
      void invoke('cancel_evidence_inspection',{ticket}).catch(error=>{
        if(live)setState(s=>({...s,error:`Cancellation could not be confirmed: ${errorText(error)}. The response remains hidden.`,value:null,pending:false}));
      });
    };
    const current={key,cancel:()=>{
      if(cancelled||finished)return;
      cancelled=true;scheduled?.cancel();cancelReserved();
      if(live)setState(s=>({...s,key,value:null,error:'Evidence inspection cancelled. No unchecked results are shown.',pending:false}));
    }};
    operation.current=current;
    const execute=async()=>{
        let value:T;
        if(inspected){
          ticket=await invoke<number>('prepare_evidence_inspection');
          if(!Number.isSafeInteger(ticket)||ticket<0)throw Error('Evidence inspection reservation is invalid.');
          if(!live||cancelled){cancelReserved();return;}
          const channel=new Channel<OpenProgress>();
          channel.onmessage=progress=>{
            if(!live||cancelled||finished)return;
            if(!validProgress(progress)){
              current.cancel();
              setState(s=>({...s,error:'Evidence inspection progress is invalid; the inspection was cancelled.'}));
              return;
            }
            setState(s=>({...s,key,progress}));
          };
          value=command==='report_issue_product'
            ?await invoke<T>(command,{...request,ticket,onProgress:channel})
            :await invoke<T>('inspect_energy_evidence',{ticket,onProgress:channel,query:{command,arguments:request}});
        }else value=await invoke<T>(command,request);
        if(live&&!cancelled){const checked=assertEnergyResponse(command,request,value);setState(s=>({...s,key,value:checked,error:'',pending:false}));}
    };
    // Reserve a native inspection only after obtaining the shared transport
    // lane. Otherwise queued requests could expire or deserialize concurrently.
    const promise=Object.hasOwn(request,'project')?(scheduled=largeReadQueue.enqueue(execute)).promise:execute();
    void promise.catch(error=>{if(live&&!cancelled)setState(s=>({...s,key,value:null,error:errorText(error),pending:false}));})
      .finally(()=>{finished=true;if(operation.current===current)operation.current=null;});
    return()=>{live=false;cancelled=true;scheduled?.cancel();cancelReserved();if(operation.current===current)operation.current=null;};
  },[snapshot]);
  const current=Boolean(snapshot.args)&&state.key===key,loading=Boolean(snapshot.args&&(!current||state.pending));
  return {value:current?state.value:null,error:current?state.error:'',loading,progress:current?state.progress:null,cancel,cancellable:loading&&inspectedCommands.has(command)};
}
