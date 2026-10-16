import test from 'node:test';
import assert from 'node:assert/strict';
import {assertEnergyResponse} from '../src/energy/response.mjs';
import {validIssueSummary} from '../src/energy/report-issue-response.mjs';
import {issueFixture,reviewFixture} from './report-issue-fixture.mjs';
// Small original IPC shape fixtures only; Rust tests establish evidence integrity.
function fixture(){
  const scope='c'.repeat(64),project={architecture:{scope_history:{revisions:[{revision_sha256:scope,scope:{title:'Original synthetic scope'},review_decisions:[],provenance:{documentation_complete:false,rows:[],unreviewed_count:0,missing_count:0,conflicting_count:0},reference:{asset_id:'fixture',manifest_sha256:'d'.repeat(64)}}]}},energy:{runs:[{id:'report-a',record_sha256:'a'.repeat(64)},{id:'report-b',record_sha256:'b'.repeat(64)}],designs:{A:{model:{name:'A'},study:{}},B:{model:{name:'B'},study:{}}}}};
  const issue=issueFixture(project,scope);project.report_issues={schema_version:'greenlens.report-issue-index.v1',issues:[structuredClone(issue.summary)]};return{request:{project,id:issue.summary.id},issue};
}
test('issue response binds an independent issue ID and content hash without altering historical bytes',()=>{
  const {request,issue}=fixture(),before=JSON.stringify(issue),project=JSON.stringify(request.project);assert.notEqual(issue.summary.id,issue.summary.content_sha256);assert.ok(validIssueSummary(issue.summary));assert.equal(assertEnergyResponse('report_issue_inspect',request,issue),issue);assert.equal(JSON.stringify(issue),before);assert.equal(JSON.stringify(request.project),project);
});
test('typed retained summaries accept sorted IPC map keys without changing historical data',()=>{
  const {request,issue}=fixture();
  // EnergyEvidenceQuery returns serde_json::Value, whose object maps sort keys;
  // the retained Project instead serializes IssueRunBinding in field order.
  const sorted=value=>Array.isArray(value)?value.map(sorted):value&&typeof value==='object'?Object.fromEntries(Object.keys(value).sort().map(key=>[key,sorted(value[key])])):value;
  const response=sorted(issue),before=JSON.stringify(response),project=JSON.stringify(request.project);
  assert.notDeepEqual(Object.keys(request.project.report_issues.issues[0].baseline),Object.keys(response.summary.baseline));
  assert.equal(assertEnergyResponse('report_issue_inspect',request,response),response);
  assert.equal(JSON.stringify(response),before);assert.equal(JSON.stringify(request.project),project);
});
test('summary structure rejects changed dropped or extra fields and reordered arrays',()=>{
  for(const edit of [
    s=>s.baseline.run_id='other-a',s=>delete s.baseline.record_sha256,s=>s.baseline.extra='unexpected',
    s=>delete s.title,s=>s.extra='unexpected',s=>s.include_pdf='true',
    s=>s.baseline={...s.baseline,design_id:['A']},s=>s.sensitivity_run_ids.reverse(),
    s=>s.sensitivity_run_ids[0]=null,s=>s.sensitivity_run_ids={0:'sensitivity-a',1:'sensitivity-b'},
  ]){
    const {request,issue}=fixture();issue.summary.sensitivity_run_ids=['sensitivity-a','sensitivity-b'];request.project.report_issues.issues[0].sensitivity_run_ids=[...issue.summary.sensitivity_run_ids];
    edit(issue.summary);assert.throws(()=>assertEnergyResponse('report_issue_inspect',request,issue),/selected immutable issue inventory/);
  }
});
test('summary structural comparison is bounded even when both sides contain oversized unknown data',()=>{
  for(const extra of [
    'x'.repeat(32769),{one:{two:{three:{four:{five:'too deep'}}}}},
    Object.fromEntries(Array.from({length:33},(_,i)=>[`field-${i}`,i])),
    Array.from({length:17},(_,i)=>i),
    Array.from({length:16},()=>Array.from({length:16},()=>true)),
  ]){
    const {request,issue}=fixture();issue.summary.extra=extra;request.project.report_issues.issues[0].extra=structuredClone(extra);
    assert.throws(()=>assertEnergyResponse('report_issue_inspect',request,issue),/selected immutable issue inventory/);
  }
});
test('another issue scope run seal or sensitivity cannot be represented as the selected history',()=>{
  for(const edit of [v=>v.summary.id='9'.repeat(64),v=>v.summary.products_sha256='9'.repeat(64),v=>v.content.scope_revision.revision_sha256='9'.repeat(64),v=>v.content.workbench.base.proposed.record_sha256='9'.repeat(64),v=>v.content.workbench.base.baseline.run_id='other-a',v=>v.content.sensitivity={session_id:'assessment-for-another-pair'},v=>v.content.workbench.base.baseline.assessment.scientific.status='certified']){
    const {request,issue}=fixture();edit(issue);assert.throws(()=>assertEnergyResponse('report_issue_inspect',request,issue),/Report issue/);
  }
});
test('issue product paths sizes and product counts have bounded response admission',()=>{
  for(const edit of [v=>v.products.products[0].path='../escape.pdf',v=>v.products.products[0].path='C:/private.pdf',v=>v.products.products[0].path='folder\\escape.pdf',v=>v.products.products[0].bytes=64*1024*1024+1,v=>v.products.products.push(v.products.products[0]),v=>v.products.products=Array.from({length:253},(_,i)=>({path:`p${i}`,bytes:1,sha256:'a'.repeat(64)})),v=>v.products.products=Array.from({length:5},(_,i)=>({path:`p${i}`,bytes:64*1024*1024,sha256:'a'.repeat(64)}))]){
    const {request,issue}=fixture();edit(issue);assert.throws(()=>assertEnergyResponse('report_issue_inspect',request,issue),/product inventory/);
  }
});
test('unsupported exact regeneration remains an inspection limitation rather than changing the old issue',()=>{
  const {request,issue}=fixture();issue.regeneration_available=false;issue.regeneration_message='Preserved products remain inspectable; exact older font recipe unavailable.';assert.equal(assertEnergyResponse('report_issue_inspect',request,issue).regeneration_available,false);
});
test('a bound failed sensitivity result remains failed and does not alter scientific status',()=>{
  const {request,issue}=fixture();issue.summary.sensitivity_session_id='exact-session';request.project.report_issues.issues[0].sensitivity_session_id='exact-session';issue.content.sensitivity={session_id:'exact-session',result:{assessment:{numerical_sensitivity_passed:false,criteria:[{id:'annual-electricity',passed:false,explanation:'Declared limit exceeded; outcome retained.'}],missing_slots:[]}}};
  const inspected=assertEnergyResponse('report_issue_inspect',request,issue);assert.equal(inspected.content.sensitivity.result.assessment.numerical_sensitivity_passed,false);assert.equal(inspected.content.workbench.base.baseline.assessment.scientific.status,'unassessed');
});

test('separate review readiness binds complete documentation and all retained decisions including late exceptions',()=>{
  const {request,issue}=fixture(),scope=issue.content.scope_revision;
  scope.provenance.documentation_complete=true;
  scope.review_decisions=Array.from({length:101},(_,i)=>({row_id:i.toString(16).padStart(64,'0'),resolution:i>=99?'unresolved':'reviewed',rationale:'來源尚待覆核。'.repeat(30)}));
  scope.reviewed_rows=scope.review_decisions.map((row,i)=>({row_id:row.row_id,kind:i===100?'reference':'input',key:`supported-field-${i}`}));
  issue.review_readiness=reviewFixture(scope);const before=JSON.stringify(issue.content);
  const result=assertEnergyResponse('report_issue_inspect',request,issue);assert.equal(result.review_readiness.documentation_complete,true);assert.equal(result.review_readiness.unresolved_count,2);assert.equal(result.review_readiness.rows[100].kind,'reference');assert.equal(JSON.stringify(issue.content),before);
  for(const edit of [v=>delete v.review_readiness,v=>v.review_readiness.scope_revision_sha256='9'.repeat(64),v=>v.review_readiness.rows.pop(),v=>v.review_readiness.rows[100].rationale='Substituted rationale',v=>v.review_readiness.unresolved_row_ids.reverse(),v=>v.review_readiness.review_state='all_recorded_reviewed',v=>v.review_readiness.documentation_complete=false,v=>v.review_readiness.category_metadata_unavailable_count=1,v=>v.review_readiness.rows[100].kind='input',v=>v.review_readiness.rows[100].key='different-field',v=>v.review_readiness.supported_input_rows=1,v=>v.review_readiness.missing_input_rows=1]){
    const next=structuredClone(issue);edit(next);assert.throws(()=>assertEnergyResponse('report_issue_inspect',request,next),/review readiness/);
  }
});

test('review response limits are UTF-8 byte-aware and historical missing categories stay explicit',()=>{
  const {request,issue}=fixture(),scope=issue.content.scope_revision;
  scope.review_decisions=[{row_id:'b'.repeat(64),resolution:'unresolved',rationale:'界'.repeat(682)+'ab'}];
  issue.review_readiness=reviewFixture(scope);assert.equal(assertEnergyResponse('report_issue_inspect',request,issue).review_readiness.category_metadata_unavailable_count,1);
  scope.review_decisions[0].rationale+='c';issue.review_readiness=reviewFixture(scope);assert.throws(()=>assertEnergyResponse('report_issue_inspect',request,issue),/review readiness/);
});
