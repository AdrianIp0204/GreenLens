import test from 'node:test';
import assert from 'node:assert/strict';
import {assertEnergyResponse} from '../src/energy/response.mjs';
import {validDesignReportSummary} from '../src/energy/design-report-response.mjs';
import {designReportFixture,addDesignSensitivity} from './design-report-fixture.mjs';
const check=fixture=>assertEnergyResponse('design_report_inspect',fixture.request,fixture.inspection);

test('complete shape binds exact selected A/B without relabelling execution or qualification',()=>{
  const fixture=designReportFixture(),before=JSON.stringify(fixture);assert.ok(validDesignReportSummary(fixture.summary));assert.equal(check(fixture),fixture.inspection);assert.equal(JSON.stringify(fixture),before);
  assert.equal(fixture.inspection.content.workbench.base.baseline.assessment.scientific.status,'unassessed');
  const sorted=value=>Array.isArray(value)?value.map(sorted):value&&typeof value==='object'?Object.fromEntries(Object.keys(value).sort().map(key=>[key,sorted(value[key])])):value;
  fixture.inspection=sorted(fixture.inspection);check(fixture);
});

test('unknown codecs, mixed pair, altered immutable metadata and unchecked execution are rejected',()=>{
  for(const edit of [
    f=>f.inspection.content.schema_version='unknown',f=>f.inspection.content.content_sha256='0'.repeat(64),f=>f.inspection.manifest_file_sha256='0'.repeat(64),
    f=>f.inspection.content.workbench.base.baseline.run_id=f.summary.proposed.run_id,f=>f.inspection.content.workbench.base.baseline.id='B',f=>f.inspection.content.workbench.metadata.revision='R2',
    f=>f.inspection.content.workbench.base.baseline.assessment.execution='not_executed',f=>f.inspection.content.workbench.base.proposed.assessment.evidence_integrity='not_checked',
    f=>f.inspection.content.workbench.base.baseline.assessment.current_processed_results='verified',f=>f.inspection.content.workbench.base.baseline.assessment.scientific.status='certified',
    f=>f.inspection.content.workbench.base.baseline.assessment.scientific.method.engine_version='other',f=>delete f.inspection.content.workbench.base.baseline.assessment.scientific.evidence_ids,
    f=>f.inspection.content.include_pdf=false,f=>f.inspection.content.renderer.font_sha256.third='a'.repeat(64),f=>f.inspection.content.renderer.figure_method='unknown',
    f=>f.summary.proposed.run_id=f.summary.baseline.run_id,f=>f.summary.baseline.extra='unbounded',f=>f.project.energy.runs[0].record_sha256='0'.repeat(64),
  ]){const f=designReportFixture();edit(f);assert.throws(()=>check(f),/Design report/);}
});

test('scope and frame admission is UTF-8 aware and never accepts arbitrary scene recipes',()=>{
  const boundary=designReportFixture();boundary.inspection.content.analytical_scope='界'.repeat(1365)+'a';check(boundary);
  for(const edit of [
    f=>f.inspection.content.analytical_scope='界'.repeat(1366),f=>f.inspection.content.north_declaration={value:'north'},f=>f.inspection.content.north_declaration='界'.repeat(1366),
    f=>f.inspection.content.frames=[],f=>f.inspection.content.frames.push(f.inspection.content.frames[0]),f=>f.inspection.content.frames[0].id='../outside',
    f=>f.inspection.content.frames[0].up[0]=Infinity,f=>f.inspection.content.frames[1].aspect_ratio=0,f=>f.inspection.content.frames[1].section={axis:'sun',offset_m:0,keep:'above'},
    f=>f.inspection.content.frames[1].section={axis:'z',offset_m:NaN,keep:'below'},f=>f.inspection.content.frames[0].shell='anything',
  ]){const f=designReportFixture();edit(f);assert.throws(()=>check(f),/Design report/);}
});

test('inventory requires all data and image families, safe sorted paths and bounded aggregate bytes',()=>{
  for(const edit of [
    f=>f.inspection.products.products.pop(),f=>f.inspection.products.products[0].path='../outside.pdf',f=>f.inspection.products.products[0].path='folder/file.pdf',
    f=>f.inspection.products.products[0].path='C:\\private.pdf',f=>f.inspection.products.products[0].path='nul\u0000.png',f=>f.inspection.products.products.reverse(),
    f=>f.inspection.products.products.push(f.inspection.products.products[0]),f=>f.inspection.products.products[0].bytes=64*1024*1024+1,
    f=>f.inspection.products.products[0].bytes=0,f=>f.inspection.products.products[0].bytes=NaN,f=>f.inspection.products.products[0].sha256='not-a-hash',
    f=>f.inspection.products.products.find(p=>p.path==='performance-page-001.png').path='performance-page-002.png',
    f=>f.inspection.total_bytes=256*1024*1024+1,f=>f.inspection.total_bytes=1,
    f=>{for(const product of f.inspection.products.products)product.bytes=64*1024*1024;f.inspection.total_bytes=f.inspection.products.products.length*64*1024*1024+1;},
  ]){const f=designReportFixture();edit(f);assert.throws(()=>check(f),/Design report/);}
  const html=designReportFixture();html.summary.include_pdf=false;html.inspection.content.include_pdf=false;html.inspection.products.products=html.inspection.products.products.filter(p=>!p.path.endsWith('.pdf')&&!p.path.endsWith('.png'));html.inspection.total_bytes=html.inspection.products.products.reduce((sum,p)=>sum+p.bytes,0)+500;check(html);
});

test('failed or incomplete exact-pair sensitivity remains failed, with exact retained event binding',()=>{
  const f=addDesignSensitivity(designReportFixture()),before=JSON.stringify(f);check(f);assert.equal(f.inspection.content.sensitivity.result.assessment.numerical_sensitivity_passed,false);assert.equal(JSON.stringify(f),before);
  for(const edit of [
    f=>f.inspection.content.sensitivity.receipt_reference.events[0].file_sha256='0'.repeat(64),f=>f.inspection.content.sensitivity.receipt_reference.plan_file_sha256='0'.repeat(64),
    f=>f.inspection.content.sensitivity.receipt_reference.path='../session',f=>f.inspection.content.sensitivity.session_id='other',f=>f.project.energy.workbench.sensitivity=[],
    f=>f.inspection.content.sensitivity.result.plan_sha256='0'.repeat(64),f=>f.inspection.content.sensitivity.result.assessment.qualification_promoted=true,
    f=>f.inspection.content.sensitivity.result.assessment.runs.pop(),f=>f.inspection.content.sensitivity.result.assessment.runs[0].record_sha256='0'.repeat(64),
    f=>f.inspection.content.sensitivity.result.assessment.criteria[0].passed='yes',f=>f.summary.sensitivity_run_ids.push(f.summary.sensitivity_run_ids[0]),
    f=>f.inspection.content.sensitivity=null,
  ]){const f=addDesignSensitivity(designReportFixture());edit(f);assert.throws(()=>check(f),/Design report/);}
});

test('bounded traversal rejects cyclic, deeply nested and nonfinite hidden data without mutation',()=>{
  const cyclic=designReportFixture();cyclic.inspection.content.workbench.base.baseline.model.loop=cyclic.inspection;assert.throws(()=>check(cyclic),/bounded contract/);
  const deep=designReportFixture();let node=deep.inspection.content.workbench.base.baseline.model;for(let i=0;i<50;i++){node.child={};node=node.child;}assert.throws(()=>check(deep),/bounded contract/);
  const nonfinite=designReportFixture();nonfinite.inspection.content.workbench.base.baseline.totals.value=NaN;assert.throws(()=>check(nonfinite),/bounded contract/);
});
