// Bounded IPC admission only. Rust verifies actual bytes and computes physics,
// arithmetic, identities and qualification. These checks never establish them.
const FILE=64*1024*1024,TOTAL=256*1024*1024,CONTENT=48*1024*1024;
const encoder=new TextEncoder();
const object=value=>value!==null&&typeof value==='object'&&!Array.isArray(value);
const text=(value,max=2048)=>typeof value==='string'&&value.length<=max&&encoder.encode(value).length<=max;
const nonempty=(value,max=2048)=>text(value,max)&&value.trim().length>0;
const nullableText=(value,max=2048)=>value===null||text(value,max);
const hash=value=>typeof value==='string'&&/^[a-f0-9]{64}$/.test(value);
const uuid=value=>typeof value==='string'&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value);
const count=value=>Number.isSafeInteger(value)&&value>=0;
const keys=(value,required)=>object(value)&&Object.keys(value).length===required.length&&required.every(key=>Object.hasOwn(value,key));
const list=(value,max,predicate)=>Array.isArray(value)&&value.length<=max&&value.every(predicate);
const flatPath=value=>nonempty(value,256)&&!/[\\/:\u0000-\u001f]/.test(value)&&value!=='.'&&value!=='..';
const vector=value=>Array.isArray(value)&&value.length===3&&value.every(Number.isFinite);
const equal=(a,b)=>{
  if(a===b)return true;if(typeof a!==typeof b||a===null||b===null)return false;
  if(Array.isArray(a)||Array.isArray(b))return Array.isArray(a)&&Array.isArray(b)&&a.length===b.length&&a.every((value,index)=>equal(value,b[index]));
  return object(a)&&object(b)&&Object.keys(a).length===Object.keys(b).length&&Object.keys(a).every(key=>Object.hasOwn(b,key)&&equal(a[key],b[key]));
};
function boundedJson(value){
  let nodes=0,bytes=0;const active=new Set();
  function visit(item,depth){
    if(++nodes>2_000_000||depth>48)return false;
    if(item===null||typeof item==='boolean')return true;
    if(typeof item==='number')return Number.isFinite(item);
    if(typeof item==='string'){if(item.length>CONTENT)return false;bytes+=encoder.encode(item).length;return bytes<=CONTENT;}
    if(!object(item)&&!Array.isArray(item)||active.has(item))return false;
    active.add(item);const entries=Object.entries(item);if(entries.length>131072)return false;
    for(const [key,child] of entries){bytes+=encoder.encode(key).length;if(bytes>CONTENT||!visit(child,depth+1))return false;}
    active.delete(item);return true;
  }return visit(value,0);
}
const method=value=>keys(value,['method_id','adapter_version','engine_version','engine_package_sha256','engine_executable_sha256','engine_schema_sha256','recipe_sha256','numerical_settings_sha256'])&&['method_id','adapter_version','engine_version'].every(key=>nonempty(value[key],256))&&['engine_package_sha256','engine_executable_sha256','engine_schema_sha256','recipe_sha256','numerical_settings_sha256'].every(key=>hash(value[key]));
function assessment(value){
  const q=value?.scientific;
  return keys(value,['execution','evidence_integrity','current_physical_inputs','current_processed_results','current_derived_results','scientific'])&&value.execution==='completed'&&value.evidence_integrity==='complete_retained_evidence_checked'&&['current_physical_inputs','current_processed_results','current_derived_results'].every(key=>['current','changed','not_compared'].includes(value[key]))&&keys(q,['status','method','registry_version','reason','tested_scope','applicable_physical_input_sha256','evidence_ids'])&&['unassessed','failed','limited_qualified'].includes(q.status)&&method(q.method)&&nonempty(q.registry_version,256)&&text(q.reason,65536)&&text(q.tested_scope,65536)&&list(q.applicable_physical_input_sha256,256,hash)&&list(q.evidence_ids,256,value=>nonempty(value,2048));
}
const binding=(value,side)=>keys(value,['run_id','record_sha256','design_id'])&&uuid(value.run_id)&&hash(value.record_sha256)&&value.design_id===side;
export function validDesignReportSummary(value){
  return keys(value,['id','path','content_sha256','manifest_file_sha256','products_sha256','baseline','proposed','sensitivity_run_ids','title','report_revision','include_pdf'])&&hash(value.id)&&value.id===value.content_sha256&&value.path===`design-reports/${value.id}`&&hash(value.manifest_file_sha256)&&hash(value.products_sha256)&&binding(value.baseline,'A')&&binding(value.proposed,'B')&&value.baseline.run_id!==value.proposed.run_id&&list(value.sensitivity_run_ids,4,uuid)&&value.sensitivity_run_ids.every((id,index)=>index===0||value.sensitivity_run_ids[index-1]<id)&&nonempty(value.title,256)&&text(value.report_revision,80)&&typeof value.include_pdf==='boolean';
}
function frame(value){return keys(value,['id','title','kind','eye_m','target_m','up','vertical_span_m','aspect_ratio','section'])&&nonempty(value.id,80)&&/^[A-Za-z0-9_-]+$/.test(value.id)&&nonempty(value.title,4096)&&['axonometric','plan','front','right'].includes(value.kind)&&[value.eye_m,value.target_m,value.up].every(vector)&&Number.isFinite(value.vertical_span_m)&&value.vertical_span_m>=1e-6&&value.vertical_span_m<=1e9&&Number.isFinite(value.aspect_ratio)&&value.aspect_ratio>=0.25&&value.aspect_ratio<=4&&(value.section===null||keys(value.section,['axis','offset_m','keep'])&&['x','y','z'].includes(value.section.axis)&&Number.isFinite(value.section.offset_m)&&['above','below'].includes(value.section.keep));}
function renderer(value){return keys(value,['method','figure_method','template_sha256','typst_version','typst_executable_sha256','font_sha256'])&&value.method==='greenlens.analytical-design-report-renderer/1'&&value.figure_method==='greenlens.analytical-paired-drawing/1'&&hash(value.template_sha256)&&nonempty(value.typst_version,256)&&hash(value.typst_executable_sha256)&&object(value.font_sha256)&&Object.keys(value.font_sha256).length===2&&Object.entries(value.font_sha256).every(([name,digest])=>flatPath(name)&&hash(digest));}
function metadata(value,summary){return keys(value,['title','revision','issue_date','author','preparer','revision_history'])&&value.title===summary.title&&value.revision===summary.report_revision&&nullableText(value.issue_date,10)&&nullableText(value.author,256)&&nullableText(value.preparer,256)&&list(value.revision_history,32,row=>keys(row,['revision','issue_date','description'])&&text(row.revision,80)&&text(row.issue_date,10)&&text(row.description,2048));}
function attachments(value){return keys(value,['included_ids','excluded_count','missing_unselected_count','inapplicable_unselected_count'])&&Array.isArray(value.included_ids)&&value.included_ids.length===0&&['excluded_count','missing_unselected_count','inapplicable_unselected_count'].every(key=>count(value[key]));}
function differences(value){return keys(value,['schema_version','policy_id','baseline_model_input_sha256','proposed_model_input_sha256','baseline_study_sha256','proposed_study_sha256','comparable_geometry_weather_calendar','unchanged_service_inputs','total_changes','truncated','differences','limitations'])&&value.schema_version==='greenlens.thermal-input-differences.v1'&&value.policy_id==='greenlens.thermal-service-comparison/1'&&['baseline_model_input_sha256','proposed_model_input_sha256','baseline_study_sha256','proposed_study_sha256'].every(key=>hash(value[key]))&&typeof value.comparable_geometry_weather_calendar==='boolean'&&typeof value.unchanged_service_inputs==='boolean'&&count(value.total_changes)&&typeof value.truncated==='boolean'&&list(value.differences,4096,object)&&value.differences.length<=value.total_changes&&list(value.limitations,256,row=>text(row,65536));}
function exactCase(value,binding,project){
  const retained=project?.energy?.runs?.find(run=>run.id===binding.run_id);
  return object(value)&&value.id===binding.design_id&&value.run_id===binding.run_id&&value.record_sha256===binding.record_sha256&&retained?.record_sha256===binding.record_sha256&&retained.alternative_id===binding.design_id&&['model_input_sha256','physical_input_sha256','processing_sha256'].every(key=>hash(value[key]))&&nonempty(value.label,2048)&&object(value.model)&&value.model.schema_version==='greenlens.thermal-model.v0.4'&&list(value.model.zones,8,object)&&value.model.zones.length>0&&object(value.study)&&value.study.schema_version==='greenlens.energy-study.v0.4'&&object(value.study.calendar)&&object(value.study.weather)&&[6,12].includes(value.study.zone_timesteps_per_hour)&&method(value.method)&&assessment(value.assessment)&&equal(value.method,value.assessment.scientific.method)&&object(value.totals)&&object(value.meter_ledger)&&object(value.weather_summary)&&object(value.diagnostics)&&list(value.limitations,256,v=>text(v,65536));
}
function sensitivity(value,summary,project){
  if(value===null)return summary.sensitivity_run_ids.length===0;
  if(!keys(value,['session_id','plan','result','receipt_reference'])||!uuid(value.session_id))return false;
  const r=value.receipt_reference,retained=project.energy?.workbench?.sensitivity?.find(row=>row.id===value.session_id);
  if(!keys(r,['id','path','plan_sha256','plan_file_sha256','events'])||r.id!==value.session_id||r.path!==`energy-assessments/${r.id}`||!hash(r.plan_sha256)||!hash(r.plan_file_sha256)||!list(r.events,13,e=>keys(e,['sequence','file_sha256'])&&count(e.sequence)&&hash(e.file_sha256))||!r.events.every((event,index)=>event.sequence===index+1)||!equal(r,retained))return false;
  const plan=value.plan,result=value.result,a=result?.assessment;
  if(!object(plan)||plan.schema_version!=='greenlens.energy-sensitivity-plan.v1'||plan.execution_authorized!==false||!list(plan.slots,4,s=>object(s)&&nonempty(s.slot_id,256))||!object(result)||result.schema_version!=='greenlens.energy-sensitivity-result.v1'||result.plan_sha256!==r.plan_sha256||typeof result.current_inputs_match!=='boolean'||!text(result.evidence_scope,65536)||!object(a)||a.schema_version!=='greenlens.energy-sensitivity-assessment.v1'||typeof a.all_declared_runs_integrity_checked!=='boolean'||typeof a.numerical_sensitivity_passed!=='boolean'||a.qualification_promoted!==false||!list(a.runs,4,run=>object(run)&&uuid(run.run_id)&&hash(run.record_sha256)&&hash(run.physical_input_sha256)&&hash(run.execution_input_sha256)&&method(run.method))||!list(a.missing_slots,4,id=>nonempty(id,256))||!list(a.interruptions,4,row=>object(row)&&nonempty(row.slot_id,256)&&['failed','cancelled'].includes(row.kind)&&hash(row.receipt_sha256)&&text(row.reason,65536))||!list(a.criteria,128,row=>object(row)&&nonempty(row.id,256)&&typeof row.passed==='boolean'&&text(row.explanation,16384))||!list(a.limitations,256,row=>text(row,65536)))return false;
  const ids=a.runs.map(run=>run.run_id).sort();
  return equal(ids,summary.sensitivity_run_ids)&&ids.includes(summary.baseline.run_id)&&ids.includes(summary.proposed.run_id)&&a.runs.every(run=>project.energy.runs.some(r=>r.id===run.run_id&&r.record_sha256===run.record_sha256));
}
function products(value,content,summary,total){
  if(!keys(value,['schema_version','content_sha256','products_sha256','products'])||value.schema_version!=='greenlens.design-report-products.v1'||value.content_sha256!==summary.id||value.products_sha256!==summary.products_sha256||!list(value.products,252,p=>keys(p,['path','bytes','sha256'])&&flatPath(p.path)&&count(p.bytes)&&p.bytes>0&&p.bytes<=FILE&&hash(p.sha256))||!value.products.length)return false;
  const names=value.products.map(p=>p.path);if(!names.every((name,index)=>index===0||names[index-1]<name))return false;
  const required=['design-content.json','energy-report.json','workbench-report.json','performance.json','performance.csv','inputs.csv','design-figures.json','design-changes.json','design-changes.csv','sensitivity.json','design-proposals.json','typed-input-differences.json','baseline-model.svg','proposed-model.svg','annual.svg','monthly.svg','design-executive.html','design-technical.html','input-appendix.html','performance.html'];
  for(const frame of content.frames)for(const side of ['a','b','overlay'])required.push(`design-${frame.id}-${side}.svg`);
  const pages=names.filter(name=>name.startsWith('performance-page-'));
  if(summary.include_pdf){required.push('design-executive.pdf','design-technical.pdf','input-appendix.pdf','performance.pdf');if(pages.length<1||pages.length>128||!pages.every((name,index)=>name===`performance-page-${String(index+1).padStart(3,'0')}.png`))return false;required.push(...pages);}
  if(names.length!==required.length||required.some(name=>!names.includes(name)))return false;
  const sum=value.products.reduce((sum,p)=>sum+p.bytes,0);
  return count(total)&&total>sum&&total<=sum+256*1024&&total<=TOTAL;
}
export function assertDesignReportInspection(request,value){
  const summary=request.project?.design_reports?.reports?.find(row=>row.id===request.id);
  if(!validDesignReportSummary(summary)||!keys(value,['content','products','manifest_file_sha256','total_bytes'])||value.manifest_file_sha256!==summary.manifest_file_sha256||!boundedJson(value))throw Error('Design report response differs from the selected immutable evidence or bounded contract.');
  const c=value.content,w=c?.workbench;
  if(!keys(c,['schema_version','content_sha256','workbench','sensitivity','proposal_declarations','frames','analytical_scope','north_declaration','renderer','include_pdf'])||c.schema_version!=='greenlens.design-report-content.v1'||c.content_sha256!==summary.id||c.include_pdf!==summary.include_pdf||!nonempty(c.analytical_scope,4096)||!(c.north_declaration===null||nonempty(c.north_declaration,4096))||!renderer(c.renderer)||!list(c.frames,4,frame)||c.frames.length<2||new Set(c.frames.map(f=>f.id)).size!==c.frames.length||!list(c.proposal_declarations,64,p=>object(p)&&hash(p.receipt_sha256)&&hash(p.before_model_sha256)&&hash(p.after_model_sha256)&&hash(p.reviewed_context_sha256)&&object(p.draft))||!object(w)||w.schema_version!=='greenlens.energy-workbench-report.v1'||!metadata(w.metadata,summary)||!object(w.base)||w.base.schema_version!=='greenlens.energy-report.v0.4'||!['base_dataset_sha256','template_sha256','derived_sha256'].every(key=>hash(w[key]))||!differences(w.typed_comparison)||!attachments(w.attachments)||!list(w.profiles,2,p=>['executive','technical'].includes(p))||!w.profiles.length||!exactCase(w.base.baseline,summary.baseline,request.project)||!exactCase(w.base.proposed,summary.proposed,request.project))throw Error('Design report content, assessments, frames or selected runs differ from the supported immutable contract.');
  if(!sensitivity(c.sensitivity,summary,request.project))throw Error('Design report sensitivity differs from the exact retained receipt and selected run closure.');
  if(!products(value.products,c,summary,value.total_bytes))throw Error('Design report product inventory is incomplete or exceeds its path, sequence or byte bounds.');
  return value;
}
