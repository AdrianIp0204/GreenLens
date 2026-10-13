import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { errorText } from '../workspace/useDocument';
import { assertEnergyResponse } from './response.mjs';
/** A response can only be shown for the exact request that produced it. */
export function useEnergyRead<T>(command:string,args:Record<string,unknown>|null){
  const key=args?JSON.stringify([command,args]):'';
  const [state,setState]=useState<{key:string;value:T|null;error:string}>({key:'',value:null,error:''});
  useEffect(()=>{if(!key)return;let live=true;const [,request]=JSON.parse(key);invoke<T>(command,request).then(value=>assertEnergyResponse(command,request,value)).then(value=>{if(live)setState({key,value,error:''});},error=>{if(live)setState({key,value:null,error:errorText(error)});});return()=>{live=false;};},[key,command]);
  return {value:state.key===key?state.value:null,error:state.key===key?state.error:'',loading:Boolean(key&&state.key!==key)};
}
