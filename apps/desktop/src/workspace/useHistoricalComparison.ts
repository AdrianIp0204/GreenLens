import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Comparison, RunRecord } from '../types';
import { chooseHistoricalRun, comparisonKey, ComparisonRequestGate, historicalSelection, selectedHistoricalRuns, type HistoricalSelection } from './comparison.mjs';

const compare=(a:RunRecord,b:RunRecord)=>invoke<Comparison>('compare_runs',{a,b});
export function useHistoricalComparison(documentId:number,records:RunRecord[]){
  const [stored,setStored]=useState<HistoricalSelection|null>(null);
  const selection=historicalSelection(documentId,records,stored);
  const {a,b}=selectedHistoricalRuns(records,selection);
  const key=comparisonKey(selection);
  const [response,setResponse]=useState<{key:string;comparison:Comparison|null;error:string}|null>(null);
  const gate=useRef(new ComparisonRequestGate());
  useEffect(()=>{setStored(previous=>previous?.documentId===selection.documentId&&previous.aId===selection.aId&&previous.bId===selection.bId?previous:selection);},[selection.documentId,selection.aId,selection.bId]);
  useEffect(()=>{
    const requests=gate.current;requests.cancel();setResponse(null);
    if(a&&b)void requests.compare(a,b,compare).then(value=>{if(value)setResponse({key,...value});});
    return()=>requests.cancel();
  },[key,a,b]);
  const unavailable=(selection.aId&&!a)||(selection.bId&&!b);
  const current=response?.key===key?response:null;
  return {a,b,selection,comparison:current?.comparison??null,
    comparisonError:unavailable?'A selected snapshot is unavailable. Choose another retained snapshot explicitly.':current?.error??'',
    comparing:Boolean(a&&b&&!current),
    selectRun:(alternative:'A'|'B',id:string)=>setStored(chooseHistoricalRun(records,selection,alternative,id)),
  };
}
