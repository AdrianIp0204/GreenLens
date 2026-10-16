const hash=value=>typeof value==='string'&&/^[a-f0-9]{64}$/.test(value);
const text=(value,max=1024)=>typeof value==='string'&&value.length<=max;
const object=value=>value&&typeof value==='object'&&!Array.isArray(value);
const count=value=>Number.isSafeInteger(value)&&value>=0;
const relative=value=>text(value)&&value.length>0&&!/[\\:\u0000-\u001f]/.test(value)&&value.split('/').every(part=>part&&part!=='.'&&part!=='..');
const binding=value=>object(value)&&text(value.run_id,256)&&hash(value.record_sha256)&&['A','B'].includes(value.design_id);
const assessment=value=>object(value)&&['completed','failed','cancelled','not_executed'].includes(value.execution)&&['not_checked','metadata_checked','complete_retained_evidence_checked'].includes(value.evidence_integrity)&&['current','changed','not_compared'].includes(value.current_physical_inputs)&&['unassessed','failed','limited_qualified'].includes(value.scientific?.status)&&text(value.scientific.reason,65536);
function validReviewReadiness(value,scope){
  const bounded=value=>count(value)&&value<=65536;
  if(!object(value)||value.schema_version!=='greenlens.report-review-readiness.v1'||!hash(value.assessment_sha256)||value.scope_revision_sha256!==scope.revision_sha256||typeof value.documentation_complete!=='boolean'||value.documentation_complete!==scope.provenance.documentation_complete||!text(value.disclosure,16384)||!['supported_input_rows','unreviewed_input_rows','missing_input_rows','conflicting_input_rows','decision_count','reviewed_count','unresolved_count','category_metadata_unavailable_count'].every(key=>bounded(value[key]))||!['initial_no_carry_forward','no_recorded_decisions','all_recorded_reviewed','unresolved_retained'].includes(value.review_state)||!Array.isArray(value.rows)||value.rows.length!==value.decision_count||!Array.isArray(value.unresolved_row_ids)||value.unresolved_row_ids.length!==value.unresolved_count)return false;
  if(value.supported_input_rows!==scope.provenance.rows?.length||value.unreviewed_input_rows!==scope.provenance.unreviewed_count||value.missing_input_rows!==scope.provenance.missing_count||value.conflicting_input_rows!==scope.provenance.conflicting_count)return false;
  const decisions=new Map((scope.review_decisions??[]).map(row=>[row.row_id,row])),metadata=new Map((scope.reviewed_rows??[]).map(row=>[row.row_id,row])),ids=new Set(),unresolved=[];
  if(decisions.size!==value.rows.length)return false;
  let unavailable=0;
  for(const row of value.rows){
    if(!object(row)||!hash(row.row_id)||ids.has(row.row_id)||!(row.kind===null||['reference','scope','mapping','assumption','reconciliation','report_view','input'].includes(row.kind))||!(row.key===null||text(row.key,16384))||(row.kind===null)!==(row.key===null)||!['reviewed','unresolved'].includes(row.resolution)||!text(row.rationale,2048)||new TextEncoder().encode(row.rationale).length>2048)return false;
    const accepted=decisions.get(row.row_id);if(!accepted||accepted.resolution!==row.resolution||accepted.rationale!==row.rationale)return false;
    const binding=metadata.get(row.row_id);if(row.kind!==(binding?.kind??null)||row.key!==(binding?.key??null))return false;
    ids.add(row.row_id);if(row.kind===null)unavailable++;if(row.resolution==='unresolved')unresolved.push(row.row_id);
  }
  const expectedState=unresolved.length?'unresolved_retained':!scope.parent_revision_sha256?'initial_no_carry_forward':value.rows.length?'all_recorded_reviewed':'no_recorded_decisions';
  return value.reviewed_count+value.unresolved_count===value.decision_count&&unavailable===value.category_metadata_unavailable_count&&unresolved.length===value.unresolved_count&&unresolved.every((id,index)=>id===value.unresolved_row_ids[index])&&value.review_state===expectedState;
}
export function validIssueSummary(value){
  return Boolean(object(value)&&hash(value.id)&&hash(value.content_sha256)&&value.path===`report-issues/${value.id}`&&['content_file_sha256','products_sha256','products_file_sha256','receipt_file_sha256','scope_revision_sha256','reference_manifest_sha256'].every(key=>hash(value[key]))&&binding(value.baseline)&&value.baseline.design_id==='A'&&binding(value.proposed)&&value.proposed.design_id==='B'&&value.baseline.run_id!==value.proposed.run_id&&text(value.reference_asset_id,256)&&text(value.title,4096)&&text(value.report_revision,256)&&typeof value.include_pdf==='boolean'&&(value.sensitivity_session_id===null||text(value.sensitivity_session_id,256))&&Array.isArray(value.sensitivity_run_ids)&&value.sensitivity_run_ids.length<=4&&value.sensitivity_run_ids.every(id=>text(id,256)));
}
// Typed Project serialization and the query's serde_json::Value map can emit
// different object-key orders. Compare JSON structure without changing either
// value; array order, scalar types and the complete key sets remain binding.
// These limits exceed the supported summary shape, not the full issue content.
function sameSummary(left,right){
  let remainingNodes=128,remainingCharacters=32768;
  function same(a,b,depth){
    if(--remainingNodes<0||depth>4||typeof a!==typeof b)return false;
    if(a===null||b===null)return a===b;
    if(typeof a==='string'){remainingCharacters-=a.length;return remainingCharacters>=0&&a===b;}
    if(typeof a==='boolean')return a===b;
    if(typeof a==='number')return Number.isFinite(a)&&Object.is(a,b);
    if(typeof a!=='object')return false;
    if(Array.isArray(a)||Array.isArray(b)){
      if(!Array.isArray(a)||!Array.isArray(b)||a.length!==b.length||a.length>16)return false;
      for(let i=0;i<a.length;i++)if(!Object.hasOwn(a,i)||!Object.hasOwn(b,i)||!same(a[i],b[i],depth+1))return false;
      return true;
    }
    const keys=Object.keys(a),other=Object.keys(b);
    if(keys.length>32||keys.length!==other.length)return false;
    return keys.every(key=>{remainingCharacters-=key.length;return remainingCharacters>=0&&Object.hasOwn(b,key)&&same(a[key],b[key],depth+1);});
  }
  return same(left,right,0);
}
export function assertIssueInspection(request,value){
  const retained=request.project?.report_issues?.issues?.find(row=>row.id===request.id),s=value?.summary,c=value?.content,p=value?.products,r=value?.receipt;
  if(!retained||!validIssueSummary(s)||!sameSummary(retained,s))throw Error('Report issue response does not bind the selected immutable issue inventory.');
  const scope=c?.scope_revision,baseline=c?.workbench?.base?.baseline,proposed=c?.workbench?.base?.proposed;
  const exact=(row,expected)=>row?.run_id===expected.run_id&&row.record_sha256===expected.record_sha256&&row.id===expected.design_id;
  if(c?.schema_version!=='greenlens.energy-report-issue-content.v1'||c.content_sha256!==s.content_sha256||c.include_pdf!==s.include_pdf||scope?.revision_sha256!==s.scope_revision_sha256||!scope.scope||!scope.provenance||!exact(baseline,s.baseline)||!exact(proposed,s.proposed)||!assessment(baseline.assessment)||!assessment(proposed.assessment)||!c.workbench.metadata||!c.scope_assessment||!Array.isArray(c.figures)||c.figures.length>8||!(c.sensitivity===null||object(c.sensitivity))||typeof value.regeneration_available!=='boolean'||!text(value.regeneration_message,16384))throw Error('Report issue content differs from its exact scope, selected runs or supported contract.');
  const renderer=c.renderer;
  if(!validReviewReadiness(value.review_readiness,scope))throw Error('Report issue review readiness is missing, malformed or differs from the retained revision decisions.');
  if((c.sensitivity?.session_id??null)!==s.sensitivity_session_id)throw Error('Report issue sensitivity does not bind this exact retained assessment; another pair cannot be substituted.');
  if(c.sensitivity){const a=c.sensitivity.result?.assessment;if(!a||typeof a.numerical_sensitivity_passed!=='boolean'||!Array.isArray(a.criteria)||a.criteria.length>128||a.criteria.some(row=>!text(row.id)||!text(row.explanation,16384)||typeof row.passed!=='boolean')||!Array.isArray(a.missing_slots)||a.missing_slots.length>4||a.missing_slots.some(id=>!text(id,256)))throw Error('Report issue sensitivity result shape is invalid.');}
  if(!object(renderer)||!text(renderer.method)||!text(renderer.figure_method)||!hash(renderer.template_sha256)||!hash(renderer.typst_executable_sha256)||!object(renderer.font_sha256)||Object.keys(renderer.font_sha256).length>16||!Object.values(renderer.font_sha256).every(hash))throw Error('Report issue renderer identity is invalid.');
  if(p?.schema_version!=='greenlens.report-issue-products.v1'||p.content_sha256!==s.content_sha256||p.products_sha256!==s.products_sha256||!Array.isArray(p.products)||p.products.length===0||p.products.length>252||new Set(p.products.map(row=>row.path)).size!==p.products.length||p.products.some(row=>!relative(row.path)||!hash(row.sha256)||!count(row.bytes)||row.bytes===0||row.bytes>64*1024*1024)||p.products.reduce((sum,row)=>sum+row.bytes,0)>256*1024*1024)throw Error('Report issue product inventory exceeds its bounds or identity.');
  if(!object(r)||r.content_sha256!==s.content_sha256||!text(r.issued_utc,128)||!text(r.renderer_status,16384)||!['process_count','elapsed_ms','peak_working_set_bytes','peak_scratch_bytes'].every(key=>count(r[key]))||typeof r.process_trees_empty!=='boolean')throw Error('Report issue runtime receipt is invalid.');
  return value;
}
