import {useEffect,useRef,useState} from 'react';
import type {Project} from '../types';
import type {EnergyAssessment} from './types';
import {useEnergyRead} from './useEnergyRead';
import EvidenceInspection from './EvidenceInspection';
import {productPurpose,type ReportIssueInspection,type ReportIssueAction,type ReportIssueActionResult,type ReportIssueProduct,type IssueProduct} from './report-issues';

type Props={documentInstance:number;path:string|null;project:Project|null;a:string;b:string;scopeId?:string|null;onScopeChange:(id:string|null)=>void;selectedId?:string|null;onSelect:(id:string|null)=>void;disabled:boolean;htmlOnly:boolean;nativeReady:boolean;createReady:boolean;onAction:(action:ReportIssueAction)=>Promise<ReportIssueActionResult|undefined>;onNavigate:(panel:'reference'|'evidence'|'compare'|'sensitivity')=>void;pending:{items:{token:string;message:string}[];warning:string}};
function Assessment({value,label}:{value:EnergyAssessment;label:string}){return <div><h5>{label} · recorded at issuance</h5><p>Execution: {value.execution}. Evidence: {value.evidence_integrity.replaceAll('_',' ')}. Input relationship then: {value.current_physical_inputs}. Scientific assessment: <strong>{value.scientific.status.replaceAll('_',' ')}</strong>.</p><p>{value.scientific.reason}</p><p>These frozen assessments are not a new comparison with today’s editor inputs.</p></div>;}
export default function ReportIssues({documentInstance,path,project,a,b,scopeId,onScopeChange,selectedId,onSelect,disabled,htmlOnly,nativeReady,createReady,onAction,onNavigate,pending}:Props){
  const scopes=project?.architecture?.scope_history?.revisions??[],scope=scopes.find(row=>row.revision_sha256===scopeId),issues=project?.report_issues?.issues??[];
  const summary=issues.find(row=>row.id===selectedId);
  const read=useEnergyRead<ReportIssueInspection>('report_issue_inspect',path&&project&&summary?{path,project,id:summary.id}:null);
  const [busy,setBusy]=useState(false),[message,setMessage]=useState(''),[error,setError]=useState(''),[preview,setPreview]=useState<{key:string;path:string;src:string}|null>(null);
  const mounted=useRef(true);useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;};},[]);
  const context=JSON.stringify([documentInstance,project?.id,path,selectedId]);
  const [productRequest,setProductRequest]=useState<{context:string;sequence:number;product:IssueProduct;args:{path:string;project:Project;id:string;product:string;open:boolean}}|null>(null);
  const nextProduct=useRef(0);
  const productRead=useEnergyRead<ReportIssueProduct>('report_issue_product',productRequest?.context===context?productRequest.args:null,productRequest?.sequence??0);
  const documentKey=JSON.stringify([documentInstance,project?.id,path]),latestDocument=useRef(documentKey);latestDocument.current=documentKey;
  useEffect(()=>{setError('');setMessage('');setPreview(null);},[documentKey]);
  useEffect(()=>{setPreview(null);setError('');setProductRequest(null);},[context]);
  const perform=async(action:ReportIssueAction)=>{
    if(busy||disabled||productRead.loading)return;const owner=documentKey;setBusy(true);setError('');setMessage('');
    try{const result=await onAction(action);if(!mounted.current||latestDocument.current!==owner)return;
      if(result?.issue){onSelect(result.issue.summary.id);setMessage('Exact report issue retained; previous issue bytes are unchanged.');}
      else if(result?.status==='pending')setMessage('Completed products await publication. The recovery receipt is retained locally.');
      else if(result?.destination)setMessage(`${action.kind==='export'?'Complete issued project exported':'Exact issue products regenerated'}: ${result.destination}`);
      else if(result)setMessage('Shared report operation completed.');
      else setMessage('No new issue or export was accepted. Any existing issues remain available; check document messages or unresolved drafts.');
    }catch(e){if(mounted.current&&latestDocument.current===owner)setError(String(e));}finally{if(mounted.current)setBusy(false);}
  };
  const inspectProduct=(product:IssueProduct,open:boolean)=>{
    if(!read.value||!path||!project||productRead.loading||busy)return;
    setError('');setPreview(null);
    setProductRequest({context,sequence:++nextProduct.current,product,args:{path,project,id:read.value.summary.id,product:product.path,open}});
  };
  useEffect(()=>{
    const result=productRead.value,request=productRequest;
    if(!result||!request||request.context!==context)return;
    const {product,args}=request;
    try{
      if(result.issue_id!==args.id||result.path!==product.path||result.sha256!==product.sha256)throw Error('Product response differs from the selected immutable issue.');
      if(args.open){if(result.opened!==true)throw Error('The native viewer did not confirm opening this retained product.');setMessage(`Opened retained ${product.path} in its registered local application.`);}
      else{
        let src:string;
        if(result.media_type==='image/svg+xml'&&typeof result.svg==='string'&&new TextEncoder().encode(result.svg).length<=8*1024*1024)src=`data:image/svg+xml;charset=utf-8,${encodeURIComponent(result.svg)}`;
        else if(result.media_type==='image/png'&&Array.isArray(result.bytes)&&result.bytes.length<=8*1024*1024&&result.bytes.every(byte=>Number.isInteger(byte)&&byte>=0&&byte<=255)){
          let binary='';for(let i=0;i<result.bytes.length;i+=16384)binary+=String.fromCharCode(...result.bytes.slice(i,i+16384));src=`data:image/png;base64,${btoa(binary)}`;
        }else throw Error('This product has no supported bounded image preview. Open a retained PDF or PNG instead.');
        setPreview({key:context,path:product.path,src});
      }
    }catch(e){setError(String(e));}
    setProductRequest(null);
  },[productRead.value,productRequest,context]);
  const inspected=read.value;
  const groups=inspected?.products.products.reduce<Record<string,IssueProduct[]>>((all,product)=>{(all[productPurpose(product.path)]??=[]).push(product);return all;},{})??{};
  const match=Boolean(inspected&&inspected.summary.baseline.run_id===a&&inspected.summary.proposed.run_id===b&&inspected.summary.scope_revision_sha256===scopeId);
  return <section className="erw-issued" aria-label="Immutable report issues"><h3>Issue a retained report</h3><p>Issuance freezes the exact pair, accepted scope revision, report metadata, renderer and generated products. It does not grant independent approval or scientific qualification.</p>
    <p>Baseline snapshot: <strong>{a||'Not selected'}</strong> · Proposed snapshot: <strong>{b||'Not selected'}</strong>. <button onClick={()=>onNavigate('compare')}>Select A/B evidence</button></p>
    <label>Issue scope revision<select aria-label="Issue scope revision" disabled={disabled||busy} value={scopeId??''} onChange={e=>onScopeChange(e.target.value||null)}><option value="">Choose an accepted revision</option>{scopes.map(row=><option key={row.revision_sha256} value={row.revision_sha256}>{row.scope.title} · {row.revision_sha256.slice(0,12)}</option>)}</select></label>
    {scopeId&&!scope&&<p role="alert">The selected revision is unavailable. No other scope is substituted.</p>}
    {scope?<><p>Exact scope SHA-256: <code>{scope.revision_sha256}</code>. {scope.revision_sha256===project?.architecture?.scope_history?.active_revision_sha256?'This is the active accepted revision.':'This is an earlier accepted revision; it remains bound to its own analytical snapshots.'} Rust checks compatibility with the exact selected A/B inputs before issuance.</p><p>Documentation: {scope.provenance.documentation_complete?'all supported fields have reviewed provenance':'incomplete provenance remains disclosed'}. Unreviewed: {scope.provenance.unreviewed_count}; missing: {scope.provenance.missing_count}; conflicting: {scope.provenance.conflicting_count}.</p></>:<p>Accept a scope revision before issuing a durable report. Legacy standalone reports remain available below.</p>}
    <div className="erw-actions"><button onClick={()=>onNavigate('reference')}>Review scope and provenance</button><button onClick={()=>onNavigate('evidence')}>Edit issue metadata and attachments</button><button onClick={()=>onNavigate('sensitivity')}>Inspect exact-pair sensitivity</button></div>
    <p>{htmlOnly?'This issue explicitly omits PDF/PNG; HTML, SVG and machine-readable evidence remain retained.':'Both report PDF families and performance images require the pinned local renderer and fonts.'} Unrelated sensitivity assessments are never transferred; missing exact-pair sensitivity stays visible in the issued report.</p>
    {!createReady&&<p>Resolve the selected-run attachment preflight below before creating a new issue. Historical issue inspection remains available.</p>}<button disabled={disabled||busy||productRead.loading||!createReady||!path||!a||!b||!scope||(!htmlOnly&&!nativeReady)} onClick={()=>void perform({kind:'create',a,b,scope_sha256:scope!.revision_sha256,html_only:htmlOnly})}>Issue exact selected report</button>
    {pending.warning&&<p role="alert">{pending.warning}</p>}{pending.items.map(row=><section key={row.token} aria-label="Pending completed report"><h4>Completed report products await publication</h4><p>{row.message}</p><p>After an external save conflict, reopen the saved project through the document guard, then recover. The same-case recovery receipt survives panel navigation and reopening. No external edits are overwritten silently.</p><button disabled={busy||disabled||productRead.loading} onClick={()=>void perform({kind:'recover',token:row.token})}>Recover completed report</button></section>)}
    <h3>Issued report history</h3><label>Inspect issued report<select aria-label="Inspect issued report" value={selectedId??''} onChange={e=>onSelect(e.target.value||null)}><option value="">Choose an issued report</option>{issues.map(row=><option key={row.id} value={row.id}>{row.report_revision} · {row.title} · {row.id.slice(0,12)}</option>)}</select></label>
    {!issues.length&&<p>No immutable report issues are retained in this document.</p>}{selectedId&&!summary&&<p role="alert">This issue is unavailable in the current document. No newer issue is substituted.</p>}
    <EvidenceInspection label="Checking the selected immutable report issue and its retained evidence" read={read}/>{read.error&&<p role="alert">{read.error}</p>}
    {inspected&&<section aria-label="Inspected immutable issue"><h4>{inspected.summary.title} · {inspected.summary.report_revision}</h4><p>Actual retained product bytes and required evidence were checked by Rust. {match?'The selected pair and scope IDs match this issue.':'Historical issue: it differs from the report pair or scope selected above; its contents have not been rebound.'}</p><p>Issued UTC: {inspected.receipt.issued_utc}. Renderer observation: {inspected.receipt.renderer_status}.</p>
      <dl><dt>Baseline run / record</dt><dd>{inspected.summary.baseline.run_id} / {inspected.summary.baseline.record_sha256}</dd><dt>Proposed run / record</dt><dd>{inspected.summary.proposed.run_id} / {inspected.summary.proposed.record_sha256}</dd><dt>Scope revision</dt><dd>{inspected.summary.scope_revision_sha256}</dd><dt>Content identity</dt><dd>{inspected.summary.content_sha256}</dd><dt>Product set identity</dt><dd>{inspected.summary.products_sha256}</dd></dl>
      <p>Retained scope: {inspected.content.scope_revision.scope.title}. Scope relationship at issuance: {inspected.content.scope_assessment.relationship}. {inspected.content.scope_assessment.disclosure}</p><p>Documentation: {inspected.content.scope_revision.provenance.documentation_complete?'reviewed supported-field provenance':'incomplete; inspect the retained appendix and provenance inventory'}.</p>
      <Assessment label="Baseline" value={inspected.content.workbench.base.baseline.assessment}/><Assessment label="Proposed" value={inspected.content.workbench.base.proposed.assessment}/>
      <p>Exact-pair sensitivity: {inspected.summary.sensitivity_session_id??'Missing for these exact runs. No other assessment is substituted.'} Sensitivity is numerical evidence, not physical validation.</p>
      {inspected.content.sensitivity&&<details><summary>Retained numerical sensitivity outcome: {inspected.content.sensitivity.result.assessment.numerical_sensitivity_passed?'passed':'failed'}</summary><p>Frozen limits: ≤1% annual electricity, ≤3% aligned ten-minute facility peak, and the declared cooling-comfort criterion. Missing cases: {inspected.content.sensitivity.result.assessment.missing_slots.join(', ')||'None'}.</p><ul>{inspected.content.sensitivity.result.assessment.criteria.map(row=><li key={row.id}>{row.id}: {row.passed?'passed':'failed'} · {row.explanation}</li>)}</ul></details>}
      <details><summary>Frozen renderer and report metadata</summary><p>{inspected.content.renderer.method} · {inspected.content.renderer.figure_method}</p><p>Template: {inspected.content.renderer.template_sha256}. Typst: {inspected.content.renderer.typst_version} / {inspected.content.renderer.typst_executable_sha256}.</p><p>Author: {inspected.content.workbench.metadata.author??'Not supplied'}; preparer: {inspected.content.workbench.metadata.preparer??'Not supplied'}. No approval is inferred.</p></details>
      <p>{inspected.regeneration_message}</p><button disabled={disabled||busy||productRead.loading||!inspected.regeneration_available||(inspected.summary.include_pdf&&!nativeReady)} onClick={()=>void perform({kind:'regenerate',id:inspected.summary.id})}>Regenerate exact issue into folder…</button>
      <EvidenceInspection label="Checking the requested retained report product" read={productRead}/>{productRead.error&&<p role="alert">{productRead.error}</p>}<div aria-label="Retained issue products">{Object.entries(groups).map(([purpose,products])=><section key={purpose}><h5>{purpose}</h5><ul>{products.map(product=><li key={product.path}><span>{product.path} · {product.bytes.toLocaleString()} bytes</span>{/\.(?:svg|png)$/i.test(product.path)&&<button disabled={busy||productRead.loading} onClick={()=>void inspectProduct(product,false)}>Preview {product.path}</button>}{/\.(?:pdf|png)$/i.test(product.path)&&<button disabled={busy||productRead.loading} onClick={()=>void inspectProduct(product,true)}>Open retained {product.path}</button>}</li>)}</ul></section>)}</div>
      {preview?.key===context&&<figure aria-label="Rendered issue product preview"><img src={preview.src} alt={`Retained issue ${inspected.summary.report_revision}: ${preview.path}`}/><figcaption>Actual product: {preview.path} · issue {inspected.summary.id}. Figures are reference/analytical drawings, not spatial simulation fields.</figcaption></figure>}
    </section>}
    <p>Complete issue export retains every issued report, required scope/reference history and retained evidence within the shared archive limits. It does not silently select only the current pair or include external original proprietary sources.</p><button disabled={disabled||busy||productRead.loading||!path||issues.length===0} onClick={()=>void perform({kind:'export'})}>Export complete issued project ZIP…</button>
    {busy&&<p role="status">Handling exact retained report inputs. Existing issue bytes remain unchanged.</p>}{message&&<p role="status">{message}</p>}{error&&<p role="alert">{error}</p>}
  </section>;
}
