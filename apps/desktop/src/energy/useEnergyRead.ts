import { useCallback, useEffect, useRef, useState } from 'react';
import { Channel, invoke } from '@tauri-apps/api/core';
import { errorText } from '../workspace/useDocument';
import type { OpenProgress } from '../workspace/project-intake';
import { assertEnergyResponse } from './response.mjs';

const inspectedCommands=new Set(['energy_default_runs','energy_run_catalog','energy_result','energy_rows','energy_navigate','energy_navigate_pair','energy_compare','energy_report_preflight','energy_sensitivity_inspect','energy_sensitivity_export_selection','report_issue_inspect']);
const count=(v:unknown):v is number=>typeof v==='number'&&Number.isSafeInteger(v)&&v>=0;
function validProgress(p:OpenProgress):boolean {
  return Boolean(p&&p.schema_version==='greenlens.project-open-progress.v1'&&typeof p.stage==='string'&&p.stage.length>0&&p.stage.length<=128&&(p.run_id===null||typeof p.run_id==='string'&&p.run_id.length<=256)&&[p.runs_completed,p.runs_total,p.bytes_read,p.files_verified,p.elapsed_ms].every(count)&&p.runs_completed<=p.runs_total&&p.numeric_passes&&typeof p.numeric_passes==='object'&&!Array.isArray(p.numeric_passes)&&Object.keys(p.numeric_passes).length<=5000&&Object.entries(p.numeric_passes).every(([id,n])=>id.length<=256&&count(n)));
}
type ReadState<T>={key:string;value:T|null;error:string;progress:OpenProgress|null;pending:boolean};
/** A response can only be shown for the exact request that produced it. */
export function useEnergyRead<T>(command:string,args:Record<string,unknown>|null){
  const key=args?JSON.stringify([command,args]):'';
  const [state,setState]=useState<ReadState<T>>({key:'',value:null,error:'',progress:null,pending:false});
  const operation=useRef<{key:string;cancel:()=>void}|null>(null);
  const cancel=useCallback(()=>{if(operation.current?.key===key)operation.current.cancel();},[key]);
  useEffect(()=>{
    if(!key)return;
    let live=true,finished=false,cancelled=false,cancelSent=false,ticket:number|null=null;
    const [,request]=JSON.parse(key),inspected=inspectedCommands.has(command);
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
      cancelled=true;cancelReserved();
      if(live)setState(s=>({...s,key,value:null,error:'Evidence inspection cancelled. No unchecked results are shown.',pending:false}));
    }};
    operation.current=current;
    void (async()=>{
      try{
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
          value=await invoke<T>('inspect_energy_evidence',{ticket,onProgress:channel,query:{command,arguments:request}});
        }else value=await invoke<T>(command,request);
        if(live&&!cancelled){const checked=assertEnergyResponse(command,request,value);setState(s=>({...s,key,value:checked,error:'',pending:false}));}
      }catch(error){if(live&&!cancelled)setState(s=>({...s,key,value:null,error:errorText(error),pending:false}));}
      finally{finished=true;if(operation.current===current)operation.current=null;}
    })();
    return()=>{live=false;cancelled=true;cancelReserved();if(operation.current===current)operation.current=null;};
  },[key,command]);
  const current=state.key===key,loading=Boolean(key&&(!current||state.pending));
  return {value:current?state.value:null,error:current?state.error:'',loading,progress:current?state.progress:null,cancel,cancellable:loading&&inspectedCommands.has(command)};
}
