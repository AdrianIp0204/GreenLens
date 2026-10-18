import {useRef,useState} from 'react';

type Pending={document_id:string;token:string;message:string};
const KEY='greenlens.pending-report-issues.v1';
const valid=(row:Pending)=>row&&typeof row.document_id==='string'&&row.document_id.length<=128&&typeof row.token==='string'&&/^[a-zA-Z0-9-]{1,128}$/.test(row.token)&&typeof row.message==='string'&&row.message.length<=16384;
/** Machine-local recovery receipts, never serialized into scientific evidence. */
export function usePendingReportIssues(projectId:string|undefined,storageKey=KEY){
  const [initial]=useState<{items:Pending[];problem:string}>(()=>{try{const text=window.localStorage.getItem(storageKey);if(!text)return{items:[],problem:''};if(text.length>512*1024)throw Error();const rows=JSON.parse(text);if(!Array.isArray(rows)||rows.length>32||!rows.every(valid))throw Error();return{items:rows,problem:''};}catch{return{items:[],problem:'Stored report recovery receipts could not be read safely. They have not been overwritten. Resolve the local recovery settings before creating another issue.'};}});
  const [items,setItems]=useState<Pending[]>(initial.items);
  const current=useRef(items);current.current=items;
  const [warning,setWarning]=useState(initial.problem);
  const change=(update:(previous:Pending[])=>Pending[])=>{if(initial.problem)throw Error(initial.problem);const next=update(current.current);current.current=next;setItems(next);try{window.localStorage.setItem(storageKey,JSON.stringify(next));setWarning('');}catch{setWarning('Completed issue recovery is retained in this window. Local recovery settings could not be saved; keep this window open until recovery completes.');}};
  return {items:items.filter(row=>row.document_id===projectId),warning,capacityAvailable:!initial.problem&&items.length<32,
    add:(document_id:string,token:string,message:string)=>{const row={document_id,token,message};if(!valid(row))throw Error('Invalid report recovery receipt.');change(previous=>{const next=[...previous.filter(p=>p.token!==token),row];if(next.length>32)throw Error('Local pending-issue limit reached. Recover retained issues before creating more; no recovery receipt was removed.');return next;});},
    remove:(token:string)=>change(previous=>previous.filter(row=>row.token!==token))};
}
