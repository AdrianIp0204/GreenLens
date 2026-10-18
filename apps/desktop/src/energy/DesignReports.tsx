import {useEffect,useRef,useState} from 'react';
import type {Project} from '../types';
import type {DesignReportAction,DesignReportInspection,DesignReportReply} from './design-reports';
import type {IssueProduct,ProductAction,ReportIssueProduct} from './report-issues';
import {productPurpose} from './report-issues';
import {useEnergyRead} from './useEnergyRead';
import EvidenceInspection from './EvidenceInspection';
import ProjectCapacity from './ProjectCapacity';
import {IssuedPdfPreview} from './IssuedPdfPreview';
import Assessment from './EnergyAssessment';
import type {DesignReportDrafts} from './useDesignReportDrafts';

type Props={path:string|null;project:Project|null;a:string;b:string;disabled:boolean;htmlOnly:boolean;onAction:(action:DesignReportAction)=>Promise<DesignReportReply|undefined>;onSelectPair:()=>void;onEditScope?:()=>void;pending:{items:{token:string;message:string}[];warning:string};declaration:DesignReportDrafts};
/** Images and arithmetic are retained Rust products; optional drawings bind accepted history. */
export default function DesignReports({path,project,a,b,disabled,htmlOnly,onAction,onSelectPair,onEditScope,pending,declaration}:Props){
  const {selected,scope,north,drawing}=declaration.value;
  const [busy,setBusy]=useState(false),[error,setError]=useState(''),[message,setMessage]=useState('');
  const [preview,setPreview]=useState<{key:string;path:string;src:string}|null>(null),[pdf,setPdf]=useState<{key:string;path:string;bytes:Uint8Array}|null>(null);
  const reports=project?.design_reports?.reports??[],summary=reports.find(r=>r.id===selected);
  const revisions=project?.architecture?.scope_history?.revisions??[];
  const drawingRevision=revisions.find(row=>row.revision_sha256===drawing?.scope_revision_sha256);
  const acceptedViews=drawingRevision?.scope.report_views??[];
  const unavailableViewIds=drawing?.accepted_view_ids.filter(id=>!acceptedViews.some(view=>view.id===id))??[];
  const drawingUnavailable=!!drawing&&(!drawingRevision||unavailableViewIds.length>0);
  const owner=JSON.stringify([project?.id,path]),context=JSON.stringify([owner,selected]);
  const live=useRef({owner,context,mounted:true});live.current={owner,context,mounted:live.current.mounted};
  useEffect(()=>{live.current.mounted=true;return()=>{live.current.mounted=false;};},[]);
  useEffect(()=>{setError('');setMessage('');},[owner]);
  const read=useEnergyRead<DesignReportInspection>('design_report_inspect',project&&path&&summary?{project,path,id:summary.id}:null);
  const [request,setRequest]=useState<{context:string;sequence:number;product:IssueProduct;action:ProductAction}|null>(null);const sequence=useRef(0);
  const product=useEnergyRead<ReportIssueProduct>('report_issue_product',project&&path&&request?.context===context?{path,project,id:selected,product:request.product.path,action:request.action,designReport:true}:null,request?.sequence??0);
  useEffect(()=>{setPreview(null);setPdf(null);setRequest(null);},[context]);
  const perform=async(action:DesignReportAction)=>{
    if(disabled||busy||product.loading)return;const identity=owner;setBusy(true);setError('');setMessage('');
    try{const reply=await onAction(action);if(!live.current.mounted||live.current.owner!==identity)return;
      if(reply?.report){declaration.update({selected:reply.report.content.content_sha256});setMessage('Immutable analytical report accepted. Images bind the selected retained models; no new simulation was requested. The local declaration remains available for an explicitly selected future issue.');}
      else if(reply?.status==='pending')setMessage(reply.message??'Complete products are staged for explicit recovery.');
      else if(reply?.destination)setMessage(`Completed output: ${reply.destination}`);
      else setMessage('No new report accepted. Existing reports remain unchanged; check document messages and drafts.');
    }catch(e){if(live.current.mounted&&live.current.owner===identity)setError(String(e));}finally{if(live.current.mounted)setBusy(false);}
  };
  useEffect(()=>{
    if(!request||request.context!==context||!product.value)return;
    const value=product.value;
    try{
      if(value.issue_id!==selected||value.path!==request.product.path||value.sha256!==request.product.sha256)throw Error('Product differs from the exact retained report.');
      if(['open','save_copy','reveal'].includes(request.action))setMessage(value.status==='cancelled'?'Save Copy cancelled; no viewer requested.':`Issued bytes verified; ${value.prepared?.path??'copy unavailable'}. ${value.handoff==='requested'?'Windows handoff requested; display is not yet observed.':value.handoff==='failed'?`Handoff failed: ${value.message??'See verified copy'}`:'No viewer requested.'}`);
      else if(request.action==='preview_pdf'){
        if(value.media_type!=='application/pdf'||!Array.isArray(value.bytes)||value.bytes.length>8*1024*1024||value.bytes.some(b=>!Number.isInteger(b)||b<0||b>255))throw Error('Invalid bounded PDF response.');
        setPdf({key:context,path:value.path,bytes:new Uint8Array(value.bytes)});
      }else if(value.media_type==='image/svg+xml'&&typeof value.svg==='string'&&new TextEncoder().encode(value.svg).length<=8*1024*1024)setPreview({key:context,path:value.path,src:`data:image/svg+xml;charset=utf-8,${encodeURIComponent(value.svg)}`});
      else if(value.media_type==='image/png'&&Array.isArray(value.bytes)&&value.bytes.length<=8*1024*1024&&value.bytes.every(b=>Number.isInteger(b)&&b>=0&&b<=255)){
        let binary='';for(let i=0;i<value.bytes.length;i+=16384)binary+=String.fromCharCode(...value.bytes.slice(i,i+16384));setPreview({key:context,path:value.path,src:`data:image/png;base64,${btoa(binary)}`});
      }else throw Error('Unsupported preview product.');
    }catch(e){setError(String(e));}setRequest(null);
  },[product.value,request,context,selected]);
  const inspect=(p:IssueProduct,action:ProductAction)=>{setPreview(null);setPdf(null);setError('');setRequest({context,sequence:++sequence.current,product:p,action});};
  const groups=read.value?.products.products.reduce<Record<string,IssueProduct[]>>((all,p)=>{(all[productPurpose(p.path)]??=[]).push(p);return all;},{})??{};
  const retainedDrawing=read.value?.content.drawing,retainedScope=retainedDrawing?.scope_revision;
  const scopeBytes=new TextEncoder().encode(scope).length,northBytes=new TextEncoder().encode(north).length;
  return <section className="erw-issued" aria-label="Analytical design comparison reports"><h3>Matched design drawings and measured outcomes</h3>
    <p>The exact retained A/B geometry, axonometric and plan pairs, change overlay and complete parameter table are included by default. Imported reference scope is optional and must be selected explicitly below; the active model never substitutes for an accepted revision. These drawings are not spatial simulation fields.</p>
    <p>Selected A: <code>{a||'No run selected'}</code>. Selected B: <code>{b||'No run selected'}</code>. <button onClick={onSelectPair}>Select genuine A/B evidence</button></p>
    <ProjectCapacity path={path} project={project} operation={{kind:'design_report'}}/>
    <p>Local declaration draft · retained on this computer for this document across panels, designs and Save As. Project Save does not include it. It becomes immutable report content only after successful issuance.</p>
    <label>Analytical study scope and exclusions<textarea aria-label="Design report analytical scope" value={scope} onChange={e=>declaration.update({scope:e.target.value})} disabled={busy||disabled||declaration.locked}/></label><small>{scopeBytes}/2,048 UTF-8 bytes for issuance; local drafts retain up to 16 KiB without accepting invalid text.</small>
    <label>Orientation declaration, if established<input aria-label="Design report north declaration" value={north} onChange={e=>declaration.update({north:e.target.value})} disabled={busy||disabled||declaration.locked}/></label><small>{northBytes}/2,048 UTF-8 bytes. Leave blank when true north is unknown. A drawing axis is not evidence of site orientation.</small>
    <fieldset disabled={busy||disabled||declaration.locked}><legend>Optional accepted scope and report views</legend>
      <label>Accepted scope for design report<select aria-label="Accepted scope for design report" value={drawing?.scope_revision_sha256??''} onChange={e=>declaration.update({drawing:e.target.value?{schema_version:'greenlens.design-report-drawing-selection.v1',scope_revision_sha256:e.target.value,accepted_view_ids:[],include_reference_context:false}:null})}>
        <option value="">Analytical-only — no scope revision</option>
        {drawing&&!drawingRevision&&<option value={drawing.scope_revision_sha256}>Unavailable retained revision · {drawing.scope_revision_sha256.slice(0,12)}</option>}
        {revisions.map(row=><option key={row.revision_sha256} value={row.revision_sha256}>{row.scope.title} · {row.revision_sha256.slice(0,12)}{row.revision_sha256===project?.architecture?.scope_history?.active_revision_sha256?' · active':' · historical'}</option>)}
      </select></label>
      <p>Default matched A/B frames are always included. Select zero to two additional accepted views. Changing the scope clears the previous view and reference-context choices; it never rebinds them by name.</p>
      {drawing&&<><p>Exact scope: <code>{drawing.scope_revision_sha256}</code>. It must match the exact retained A/B snapshots selected above. Rust validates those bindings and the accepted frames before rendering; selection alone is not compatibility or qualification.</p>
        {drawingRevision&&<><p>Reference revision: <code>{drawingRevision.reference.manifest.revision_sha256}</code>.</p>{acceptedViews.length===0&&<p>This accepted scope has no report views. The default matched frames remain available.</p>}
          {acceptedViews.map(view=><label className="energy-check" key={view.id}><input type="checkbox" aria-label={`Include accepted view ${view.id}`} checked={drawing.accepted_view_ids.includes(view.id)} disabled={drawing.accepted_view_ids.length>=2&&!drawing.accepted_view_ids.includes(view.id)} onChange={e=>declaration.update({drawing:{...drawing,accepted_view_ids:e.target.checked?[...drawing.accepted_view_ids,view.id]:drawing.accepted_view_ids.filter(id=>id!==view.id)}})}/>{view.title} · design {view.design_id} · {view.section?`${view.section.axis.toUpperCase()} section at ${view.section.offset_m} m, retain ${view.section.keep}`:'orthographic frame'} · <code>{view.id}</code></label>)}
          <small>{drawing.accepted_view_ids.length}/2 additional accepted views selected.</small></>}
        {unavailableViewIds.map(id=><p role="alert" key={id}>Accepted view <code>{id}</code> is unavailable in the selected revision. <button onClick={()=>declaration.update({drawing:{...drawing,accepted_view_ids:drawing.accepted_view_ids.filter(value=>value!==id)}})}>Remove unavailable view {id}</button></p>)}
        <label className="energy-check"><input type="checkbox" aria-label="Include neutral reference context" checked={drawing.include_reference_context} disabled={!drawingRevision} onChange={e=>declaration.update({drawing:{...drawing,include_reference_context:e.target.checked}})}/>Include neutral reference context from this exact accepted revision</label>
        <p>Reference context is a drawing layer. It does not supply thermal properties, establish whole-building coverage or change analytical inclusion.</p></>}
      {drawing&&!drawingRevision&&<p role="alert">The selected scope revision is unavailable in this document. Choose an existing accepted revision or analytical-only mode; no active revision has been substituted.</p>}
      {onEditScope?<button onClick={onEditScope}>Inspect or edit accepted scope and views…</button>:<p>Use the Reference panel to inspect scope history or capture and accept another report view.</p>}
    </fieldset>
    <button disabled={busy||disabled||declaration.locked||(!scope&&!north&&!drawing)} onClick={()=>declaration.discard()}>Discard local declaration</button>{declaration.warning&&<p role="alert">{declaration.warning}</p>}
    <p>Report metadata and profiles come from the accepted report settings. Included attachments must be explicitly cleared for this paired-design output; checked evidence is never silently omitted. Exact-pair sensitivity is included only when retained and verified.</p>
    <button disabled={disabled||busy||declaration.locked||drawingUnavailable||!path||!a||!b||!scope.trim()||scopeBytes>2048||northBytes>2048} onClick={()=>void perform({kind:'create',a,b,scope:scope.trim(),north:north.trim()||null,html_only:htmlOnly,...(drawing?{drawing}:{})})}>Issue analytical design report</button>
    {pending.warning&&<p role="alert">{pending.warning}</p>}{pending.items.map(row=><p key={row.token}>{row.message} <button disabled={disabled||busy} onClick={()=>void perform({kind:'recover',token:row.token})}>Recover completed design report</button></p>)}
    <label>Retained analytical report<select aria-label="Retained analytical report" value={selected} onChange={e=>declaration.update({selected:e.target.value})} disabled={busy||declaration.locked}><option value="">Choose a retained report</option>{reports.map(r=><option key={r.id} value={r.id}>{r.title} · {r.report_revision} · {r.id.slice(0,12)}</option>)}</select></label>
    <EvidenceInspection label="Verifying immutable design products" read={read}/>{read.error&&<p role="alert">{read.error}</p>}
    {read.value&&summary&&<><p>Retained A: <code>{summary.baseline.run_id}</code>; B: <code>{summary.proposed.run_id}</code>. {summary.baseline.run_id===a&&summary.proposed.run_id===b?'Matches the selected pair.':'Historical pair; current inputs and selected runs are not substituted.'}</p><p>{read.value.content.analytical_scope}</p><p>{read.value.content.sensitivity?'Exact-pair numerical sensitivity evidence is retained; inspect its outcome in the report.':'Exact-pair numerical sensitivity is missing. No evidence is transferred from another pair.'}</p><Assessment value={read.value.content.workbench.base.baseline.assessment} label="Retained baseline"/><Assessment value={read.value.content.workbench.base.proposed.assessment} label="Retained proposal"/>
      {retainedDrawing&&retainedScope?<section aria-label="Retained accepted drawing scope"><h4>Retained accepted scope and drawing</h4><p>{retainedScope.scope.title} · exact scope revision <code>{retainedScope.revision_sha256}</code>. The current scope and local drawing draft are not substituted.</p><p>Additional accepted view IDs: {retainedDrawing.selection.accepted_view_ids.length?retainedDrawing.selection.accepted_view_ids.map((id,index)=><span key={id}>{index?', ':''}<code>{id}</code></span>):'none; default paired frames only'}. Neutral reference context: {retainedDrawing.selection.include_reference_context?'requested; depiction limits and omissions are retained in design-drawing.json':'not requested'}.</p><p>Documentation completeness: {retainedScope.provenance.documentation_complete?'complete':'incomplete'}. Unresolved retained review decisions: {retainedScope.review_decisions.filter(row=>row.resolution==='unresolved').length} of {retainedScope.review_decisions.length}. Documentation and review resolution are separate; neither establishes approval, physical qualification or whole-building coverage. Complete declarations and rationales are retained in design-scope-revision.json.</p></section>:<p>Analytical-only report: no accepted scope revision or reference context is attached.</p>}
      <button disabled={disabled||busy||product.loading} onClick={()=>void perform({kind:'regenerate',id:selected})}>Regenerate exact products…</button>
      {Object.entries(groups).map(([name,items])=><details key={name}><summary>{name} · {items.length}</summary><ul>{items.map(p=><li key={p.path}><span>{p.path} · {p.bytes.toLocaleString()} B</span>{/\.(pdf|png|svg)$/.test(p.path)&&<button disabled={busy||product.loading} onClick={()=>inspect(p,p.path.endsWith('.pdf')?'preview_pdf':'preview')}>Preview {p.path}</button>}{/\.(pdf|png)$/.test(p.path)&&<><button disabled={busy||product.loading} onClick={()=>inspect(p,'save_copy')}>Save Copy {p.path}…</button><button disabled={busy||product.loading} onClick={()=>inspect(p,'open')}>Open {p.path}</button><button disabled={busy||product.loading} onClick={()=>inspect(p,'reveal')}>Reveal {p.path}</button></>}</li>)}</ul></details>)}
      {pdf?.key===context&&<IssuedPdfPreview bytes={pdf.bytes} identity={`${context}:${pdf.path}`} label={pdf.path} onClose={()=>setPdf(null)}/>}{preview?.key===context&&<figure><img src={preview.src} alt={`Exact retained analytical product ${preview.path}`}/><figcaption>{preview.path} · report {selected}</figcaption></figure>}
    </>}
    <button disabled={disabled||busy||!path||!reports.length} onClick={()=>void perform({kind:'export'})}>Export complete design study ZIP…</button>
    {busy&&<p role="status">Working with exact retained evidence. Rendering requests no EnergyPlus execution.</p>}<EvidenceInspection label="Checking selected product bytes" read={product}/>{product.error&&<p role="alert">{product.error}</p>}{message&&<p role="status">{message}</p>}{error&&<p role="alert">{error}</p>}
  </section>;
}
