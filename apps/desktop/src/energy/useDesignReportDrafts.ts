import {useEffect,useRef,useState} from 'react';

export type DesignReportDeclaration={scope:string;north:string;selected:string};
type Row=DesignReportDeclaration&{document_id:string};
type Choice='keep'|'discard'|'cancel';
const KEY='greenlens.design-report-declarations.v1';
const VERSION='greenlens.local-design-report-declarations.v1';
const MAX_DOCUMENTS=32,MAX_BYTES=512*1024,MAX_TEXT_BYTES=16*1024;
const bytes=(text:string)=>new TextEncoder().encode(text).length;
const empty=():DesignReportDeclaration=>({scope:'',north:'',selected:''});
const validId=(id:unknown):id is string=>typeof id==='string'&&id.length>0&&bytes(id)<=512;
function validRow(value:unknown):value is Row{
  if(!value||typeof value!=='object'||Array.isArray(value))return false;
  const row=value as Row;
  return Object.keys(row).sort().join(',')==='document_id,north,scope,selected'&&validId(row.document_id)
    &&typeof row.scope==='string'&&bytes(row.scope)<=MAX_TEXT_BYTES
    &&typeof row.north==='string'&&bytes(row.north)<=MAX_TEXT_BYTES
    &&typeof row.selected==='string'&&(row.selected===''||/^[a-f0-9]{64}$/.test(row.selected));
}
function encode(rows:Row[]):string{
  if(rows.length>MAX_DOCUMENTS||!rows.every(validRow)||new Set(rows.map(row=>row.document_id)).size!==rows.length)throw Error('Local report declarations exceed their bounded document or field limits. Existing drafts were retained; discard an unneeded local declaration explicitly before adding another.');
  const text=JSON.stringify({schema_version:VERSION,drafts:rows});
  if(bytes(text)>MAX_BYTES)throw Error('Local report declarations exceed 512 KiB. Existing drafts were retained; no document was evicted.');
  return text;
}
function read():{rows:Row[];problem:string}{
  try{
    const text=window.localStorage.getItem(KEY);if(text===null)return{rows:[],problem:''};
    if(bytes(text)>MAX_BYTES)throw Error();
    const data=JSON.parse(text);
    if(!data||Object.keys(data).sort().join(',')!=='drafts,schema_version'||data.schema_version!==VERSION||!Array.isArray(data.drafts))throw Error();
    encode(data.drafts);return{rows:data.drafts,problem:''};
  }catch{return{rows:[],problem:'Stored local report declarations could not be read safely and have not been overwritten. Resolve the local draft storage before entering another declaration.'};}
}

/** Machine-local presentation drafts. Never part of project save or scientific identities. */
export function useDesignReportDrafts(projectId:string|undefined){
  const [initial]=useState(read),[rows,setRows]=useState(initial.rows),[warning,setWarning]=useState(initial.problem);
  const current=useRef({projectId,rows});current.current={projectId,rows};
  const [guard,setGuard]=useState<{reason:string;error:string}|null>(null);
  const waiting=useRef<{documentId:string;resolve:(ok:boolean)=>void}|null>(null);
  useEffect(()=>{const pending=waiting.current;if(pending&&pending.documentId!==projectId){waiting.current=null;setGuard(null);pending.resolve(false);}},[projectId]);
  useEffect(()=>()=>{waiting.current?.resolve(false);waiting.current=null;},[]);
  const persist=(next:Row[])=>{
    if(initial.problem){setWarning(initial.problem);return false;}
    try{window.localStorage.setItem(KEY,encode(next));setWarning('');return true;}
    catch(e){setWarning(`Local report draft could not be saved. It remains in this window; keep the window open or resolve storage before leaving. ${String(e)}`);return false;}
  };
  const change=(patch:Partial<DesignReportDeclaration>,discard=false)=>{
    const id=current.current.projectId;if(!validId(id)||initial.problem)return false;
    const previous=current.current.rows,old=previous.find(row=>row.document_id===id);
    const row:Row={...empty(),...old,document_id:id,...patch};
    const next=previous.filter(value=>value.document_id!==id);
    if(row.scope||row.north||row.selected)next.push(row);
    try{encode(next);}catch(e){setWarning(String(e));return false;}
    const stored=persist(next);
    // Failed ordinary writes retain entered text in memory. A discard must not
    // claim success while old text could reappear from persistent storage.
    if(discard&&!stored)return false;
    current.current={...current.current,rows:next};setRows(next);return true;
  };
  const discard=()=>change({scope:'',north:''},true);
  const beforeReplacement=(reason:string):Promise<boolean>=>{
    const id=current.current.projectId,row=current.current.rows.find(value=>value.document_id===id);
    if(waiting.current)return Promise.resolve(false);
    if(!id||!row||(!row.scope&&!row.north))return Promise.resolve(true);
    return new Promise(resolve=>{waiting.current={documentId:id,resolve};setGuard({reason,error:''});});
  };
  const resolve=(choice:Choice)=>{
    const pending=waiting.current;if(!pending)return;
    const finish=(ok:boolean)=>{waiting.current=null;setGuard(null);pending.resolve(ok);};
    if(choice==='cancel'||current.current.projectId!==pending.documentId){finish(false);return;}
    if(choice==='keep'?!persist(current.current.rows):!discard()){
      setGuard(value=>value&&({...value,error:'Local draft storage failed. The declaration is retained; cancel this action or resolve storage before continuing.'}));return;
    }
    finish(true);
  };
  const row=rows.find(value=>value.document_id===projectId);
  const locked=!!initial.problem||!validId(projectId)||(!row&&rows.length>=MAX_DOCUMENTS);
  const value:DesignReportDeclaration=row?{scope:row.scope,north:row.north,selected:row.selected}:empty();
  return{value,update:(patch:Partial<DesignReportDeclaration>)=>change(patch),discard,warning:warning||(!row&&rows.length>=MAX_DOCUMENTS?'The 32-document local draft limit is reached. No stored draft was removed. Explicitly clear an unneeded local draft before creating another.':''),locked,beforeReplacement,guard,resolve};
}
export type DesignReportDrafts=ReturnType<typeof useDesignReportDrafts>;
