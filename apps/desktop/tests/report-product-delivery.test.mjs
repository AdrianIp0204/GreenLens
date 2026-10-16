import test from 'node:test';
import assert from 'node:assert/strict';
import {assertEnergyResponse} from '../src/energy/response.mjs';
// Actual transport guard with original synthetic receipts. No filesystem,
// Windows association, PDF rendering or native-viewer acceptance is implied.
const sha='a'.repeat(64),id='b'.repeat(64);
const request=(action='open')=>({id,product:'technical.pdf',action});
const prepared=()=>({schema_version:'greenlens.report-product-delivery.v1',issue_id:id,path:'technical.pdf',sha256:sha,status:'prepared',integrity:'verified',copy:'verified',prepared:{path:'original synthetic copies/issued 空間.pdf',sha256:sha,bytes:101,reused:false},handoff:'requested',viewer_display:'not_observed',message:null});
const cancelled=()=>({issue_id:id,path:'technical.pdf',sha256:sha,status:'cancelled',integrity:'verified',copy:'not_prepared',handoff:'not_requested',viewer_display:'not_observed'});
const check=(r,v)=>assertEnergyResponse('report_issue_product',r,v);

test('a verified local handoff remains requested, never proof of viewer display',()=>{
  for(const action of ['open','reveal']){const v=prepared(),before=JSON.stringify(v);assert.equal(check(request(action),v),v);assert.equal(v.viewer_display,'not_observed');assert.equal(v.handoff,'requested');assert.equal(JSON.stringify(v),before);}
});

test('Save Copy cancellation is admitted only as an unchanged verified-product receipt',()=>{
  const v=cancelled(),before=JSON.stringify(v);assert.equal(check(request('save_copy'),v),v);assert.equal(v.copy,'not_prepared');assert.equal(v.handoff,'not_requested');assert.equal(JSON.stringify(v),before);
  for(const edit of [x=>x.copy='verified',x=>x.handoff='requested',x=>x.viewer_display='observed',x=>x.integrity='unchecked']){const x=cancelled();edit(x);assert.throws(()=>check(request('save_copy'),x),/selected immutable issue/);}
});

test('a missing or broken viewer association retains the verified copy and failed handoff separately',()=>{
  const v={...prepared(),handoff:'failed',message:'No local association accepts this retained product.'};const result=check(request(),v);assert.equal(result.copy,'verified');assert.equal(result.prepared.sha256,sha);assert.equal(result.handoff,'failed');assert.equal(result.viewer_display,'not_observed');assert.match(result.message,/association/);
  const copy={...prepared(),handoff:'not_requested'};assert.equal(check(request('save_copy'),copy).handoff,'not_requested');
});

test('legacy opened acknowledgements and invented viewer success cannot become delivered evidence',()=>{
  for(const v of [{issue_id:id,path:'technical.pdf',sha256:sha,opened:true},{...prepared(),opened:false},{...prepared(),viewer_display:'confirmed'},{...prepared(),status:'viewed'},{...prepared(),handoff:'displayed'}])assert.throws(()=>check(request(),v),/selected immutable issue/);
});

test('wrong issue/product, changed copy digest, unchecked content and malformed byte counts are rejected',()=>{
  for(const edit of [x=>x.issue_id='c'.repeat(64),x=>x.path='executive.pdf',x=>x.sha256='not-a-hash',x=>x.prepared.sha256='d'.repeat(64),x=>x.integrity='unchecked',x=>x.copy='written',x=>delete x.prepared,x=>delete x.prepared.path,x=>x.prepared.bytes=0,x=>x.prepared.bytes=64*1024*1024+1,x=>x.prepared.bytes=NaN,x=>x.prepared.bytes=1.5]){const x=prepared();edit(x);assert.throws(()=>check(request(),x),/selected immutable issue/);}
});

test('image and PDF preview receipts retain exact selection and cannot claim an external open',()=>{
  const pdf={issue_id:id,path:'technical.pdf',sha256:sha,media_type:'application/pdf',bytes:[37,80,68,70,45]};assert.equal(check(request('preview_pdf'),pdf),pdf);
  assert.throws(()=>check(request('preview'),pdf),/selected immutable issue/);assert.throws(()=>check(request('preview_pdf'),{...pdf,media_type:'image/png'}),/selected immutable issue/);assert.throws(()=>check(request('preview_pdf'),{...pdf,opened:true}),/selected immutable issue/);
  const image={issue_id:id,path:'annual.png',sha256:sha,media_type:'image/png',bytes:[137,80,78,71]};assert.equal(check({...request('preview'),product:'annual.png'},image),image);
});

test('contradictory cancelled/prepared and unknown actions are not accepted transport states',()=>{
  assert.throws(()=>check(request('save_copy'),{...cancelled(),prepared:prepared().prepared}),/selected immutable issue/);
  assert.throws(()=>check(request('execute'),{issue_id:id,path:'technical.pdf',sha256:sha,media_type:'image/png',bytes:[137,80,78,71]}),/Unsupported product action/);
  assert.throws(()=>check(request(),{...prepared(),prepared:{...prepared().prepared,reused:'true'}}),/selected immutable issue/);
});
