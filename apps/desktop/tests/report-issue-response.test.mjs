import test from 'node:test';
import assert from 'node:assert/strict';
import {assertEnergyResponse} from '../src/energy/response.mjs';
import {validIssueSummary} from '../src/energy/report-issue-response.mjs';
import {issueFixture} from './report-issue-fixture.mjs';
// Small original IPC shape fixtures only; Rust tests establish evidence integrity.
function fixture(){
  const scope='c'.repeat(64),project={architecture:{scope_history:{revisions:[{revision_sha256:scope,scope:{title:'Original synthetic scope'},provenance:{documentation_complete:false},reference:{asset_id:'fixture',manifest_sha256:'d'.repeat(64)}}]}},energy:{runs:[{id:'report-a',record_sha256:'a'.repeat(64)},{id:'report-b',record_sha256:'b'.repeat(64)}],designs:{A:{model:{name:'A'},study:{}},B:{model:{name:'B'},study:{}}}}};
  const issue=issueFixture(project,scope);project.report_issues={schema_version:'greenlens.report-issue-index.v1',issues:[structuredClone(issue.summary)]};return{request:{project,id:issue.summary.id},issue};
}
test('issue response binds an independent issue ID and content hash without altering historical bytes',()=>{
  const {request,issue}=fixture(),before=JSON.stringify(issue),project=JSON.stringify(request.project);assert.notEqual(issue.summary.id,issue.summary.content_sha256);assert.ok(validIssueSummary(issue.summary));assert.equal(assertEnergyResponse('report_issue_inspect',request,issue),issue);assert.equal(JSON.stringify(issue),before);assert.equal(JSON.stringify(request.project),project);
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
