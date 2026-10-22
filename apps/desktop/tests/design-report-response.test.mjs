import test from 'node:test';
import assert from 'node:assert/strict';
import {assertEnergyResponse} from '../src/energy/response.mjs';
import {validDesignReportSummary} from '../src/energy/design-report-response.mjs';
import {designReportFixture,addDesignSensitivity,scopedDesignReportFixture as scopedFixture} from './design-report-fixture.mjs';
const check=fixture=>assertEnergyResponse('design_report_inspect',fixture.request,fixture.inspection);
const preciseFixture=(bound=false,options={})=>{
  const f=bound?scopedFixture(options):designReportFixture();
  f.inspection.content.schema_version='greenlens.design-report-content.v3';
  f.inspection.content.renderer.method='greenlens.design-report-renderer/3';
  return f;
};

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

test('v2 exact historical scope, accepted Unicode views and neutral context use an explicit new codec without mutation',()=>{
  for(const options of [{},{context:false},{views:false},{context:false,views:false,history:'greenlens.scope-history.v1',revision:'greenlens.scope-revision.v1'}]){
    const f=scopedFixture(options),before=JSON.stringify(f);assert.equal(check(f),f.inspection);assert.equal(JSON.stringify(f),before);
    assert.equal(f.inspection.content.drawing.scope_revision.provenance.documentation_complete,true);assert.equal(f.inspection.content.drawing.scope_revision.review_decisions[0].resolution,'unresolved');
    assert.notEqual(f.inspection.content.drawing.scope_revision.revision_sha256,f.project.architecture.scope_history.active_revision_sha256);
  }
  const old=designReportFixture(),before=JSON.stringify(old);check(old);assert.equal(JSON.stringify(old),before);
  old.inspection.content.drawing=null;check(old); // serde Option::None, not a v2 fallback.
});

test('v2 rejects missing/unknown/mixed contracts, undefined booleans and incomplete inventories',()=>{
  for(const edit of [
    f=>delete f.inspection.content.drawing,f=>f.inspection.content.drawing=null,f=>f.inspection.content.drawing=undefined,f=>f.inspection.content.schema_version='greenlens.design-report-content.v1',
    f=>f.inspection.content.drawing.schema_version='future',f=>f.inspection.content.drawing.source_path='private-source',f=>f.inspection.content.drawing.selection.schema_version='future',
    f=>f.inspection.content.drawing.selection.include_reference_context=undefined,f=>delete f.inspection.content.drawing.selection.include_reference_context,f=>f.inspection.content.drawing.selection.include_reference_context='true',
    f=>f.inspection.content.drawing.selection.include_reference_context=false,f=>f.inspection.content.drawing.context=null,f=>delete f.inspection.content.drawing.context,
    f=>f.inspection.content.renderer.method='greenlens.analytical-design-report-renderer/1',f=>f.inspection.content.renderer.figure_method='greenlens.analytical-paired-drawing/1',f=>f.inspection.content.renderer.figure_method='greenlens.scoped-paired-drawing/3',
    f=>f.inspection.products.products=f.inspection.products.products.filter(p=>p.path!=='design-drawing.json'),f=>f.inspection.products.products=f.inspection.products.products.filter(p=>p.path!=='design-scope-revision.json'),
  ]){const f=scopedFixture();edit(f);assert.throws(()=>check(f),/Design report/);}
  const old=designReportFixture();old.inspection.content.drawing={};assert.throws(()=>check(old),/Design report/);
  const extra=designReportFixture();extra.inspection.products.products.push({path:'design-drawing.json',bytes:128,sha256:'2'.repeat(64)});assert.throws(()=>check(extra),/Design report/);
});

test('v2 binds every historical field and selected view to the retained history and exact A/B snapshots',()=>{
  for(const edit of [
    f=>f.project.architecture.scope_history.revisions.shift(),f=>f.project.architecture.scope_history.schema_version='future',
    f=>f.inspection.content.drawing.selection.scope_revision_sha256='d'.repeat(64),f=>f.inspection.content.drawing.scope_revision.scope.title='Resealed different title',
    f=>f.inspection.content.drawing.scope_revision.review_decisions[0].resolution='reviewed',f=>f.inspection.content.drawing.scope_revision.provenance.documentation_complete=false,
    f=>f.inspection.content.drawing.scope_revision.reference.manifest.source_sha256='0'.repeat(64),f=>f.inspection.content.drawing.scope_revision.extra='unknown',
    f=>f.inspection.content.drawing.selection.accepted_view_ids=['頂層 平面','頂層 平面'],f=>f.inspection.content.drawing.selection.accepted_view_ids.push('third'),
    f=>f.inspection.content.drawing.selection.accepted_view_ids[0]='missing',f=>f.inspection.content.drawing.selection.accepted_view_ids[0]='界'.repeat(43),
    f=>f.inspection.content.drawing.frames[2].vertical_span_m=4.01,f=>{f.inspection.content.drawing.frames[3].section.keep='below';f.inspection.content.frames[3].section.keep='below';},
    f=>f.inspection.content.workbench.base.proposed.model.zones[0].id='different-applied-model',f=>f.inspection.content.workbench.base.proposed.study.calendar.holidays=[1],
    f=>f.inspection.content.workbench.base.proposed.study.weather.hash='different-weather',f=>f.inspection.content.workbench.base.proposed.model_input_sha256='0'.repeat(64),
  ]){const f=scopedFixture();edit(f);assert.throws(()=>check(f),/Design report/);}
});

test('v2 context bounds original occurrence identities, complete counts and finite ordered triangles',()=>{
  for(const edit of [
    c=>c.schema_version='future',c=>c.reference_revision_sha256='0'.repeat(64),c=>c.reference_geometry_sha256='0'.repeat(64),c=>c.manifest_sha256='0'.repeat(64),c=>c.scope_revision_sha256='0'.repeat(64),c=>c.content_sha256='malformed',
    c=>c.layers.pop(),c=>c.layers.push(c.layers[0]),c=>c.layers[0].frame_sha256='bad',c=>c.layers[0].source_triangle_count=4,c=>c.layers[0].source_triangle_count=1000001,
    c=>c.layers[0].visible_triangle_count=4,c=>c.layers[0].protected_visible_triangle_count=3,c=>c.layers[0].omitted_visible_triangle_count=1,c=>c.layers[0].visible_triangle_count=NaN,
    c=>c.layers[0].triangles.reverse(),c=>c.layers[0].triangles[0].source_ordinal=3,c=>c.layers[0].triangles[0].source_ordinal=-1,
    c=>c.layers[0].triangles[0].occurrence_id='name-not-an-identity',c=>c.layers[0].represented_occurrence_ids=[],c=>c.layers[0].represented_occurrence_ids.push('synthetic-occurrence'),
    c=>c.layers[0].triangles[0].vertices_m[0][0]=Infinity,c=>c.layers[0].triangles[0].vertices_m[0][0]=1e9+1,c=>c.layers[0].triangles[0].vertices_m.push([0,0,0]),
    c=>c.layers[0].triangles[0].source_path='private-source',c=>c.layers[0].shell='anything',c=>c.layers[0].triangles=Array(8001).fill(c.layers[0].triangles[0]),
    c=>c.layers[0].unavailable_reason='Cannot depict this frame',c=>c.layers[0].unavailable_reason='界'.repeat(5462),
  ]){const f=scopedFixture();edit(f.inspection.content.drawing.context);assert.throws(()=>check(f),/Design report/);}
  const unavailable=scopedFixture();Object.assign(unavailable.inspection.content.drawing.context.layers[0],{triangles:[],represented_occurrence_ids:[],omitted_visible_triangle_count:2,unavailable_reason:'No complete context can be depicted inside the fixed limit.'});check(unavailable);
  const excessive=scopedFixture();excessive.inspection.content.drawing.scope_revision.provenance.rows=Array(65537).fill({});assert.throws(()=>check(excessive),/Design report/);
  const tooMany=scopedFixture();tooMany.inspection.content.drawing.scope_revision.reference.manifest.occurrences=Array(50001).fill({id:'x'});assert.throws(()=>check(tooMany),/Design report/);
});

test('v3 analytical reports accept absent or null drawing without changing retained evidence',()=>{
  for(const explicitNull of [false,true]){
    const f=addDesignSensitivity(preciseFixture());if(explicitNull)f.inspection.content.drawing=null;
    const before=JSON.stringify(f);assert.equal(check(f),f.inspection);assert.equal(JSON.stringify(f),before);
    assert.equal(f.inspection.content.renderer.figure_method,'greenlens.analytical-paired-drawing/1');
    assert.equal(f.inspection.content.sensitivity.result.assessment.numerical_sensitivity_passed,false);
    assert.equal(f.inspection.content.workbench.base.baseline.assessment.scientific.status,'unassessed');
  }
});

test('v3 bound reports retain exact historical scopes, accepted views and optional context',()=>{
  for(const options of [{},{context:false},{views:false},{context:false,views:false,history:'greenlens.scope-history.v1',revision:'greenlens.scope-revision.v1'}]){
    const f=preciseFixture(true,options),before=JSON.stringify(f);assert.equal(check(f),f.inspection);assert.equal(JSON.stringify(f),before);
    assert.equal(f.inspection.content.renderer.figure_method,'greenlens.scoped-paired-drawing/2');
    assert.equal(f.inspection.content.drawing.scope_revision.review_decisions[0].resolution,'unresolved');
    assert.notEqual(f.inspection.content.drawing.scope_revision.revision_sha256,f.project.architecture.scope_history.active_revision_sha256);
  }
});

test('content, renderer and drawing identities admit only the four supported version combinations',()=>{
  const schemas=['greenlens.design-report-content.v1','greenlens.design-report-content.v2','greenlens.design-report-content.v3'];
  const renderers=['greenlens.analytical-design-report-renderer/1','greenlens.bound-design-report-renderer/2','greenlens.design-report-renderer/3'];
  const figures=['greenlens.analytical-paired-drawing/1','greenlens.scoped-paired-drawing/2'];
  const supported=new Set(['0:0:0:false','1:1:1:true','2:2:0:false','2:2:1:true']);
  for(const [schemaIndex,schema] of schemas.entries())for(const [rendererIndex,renderer] of renderers.entries())for(const [figureIndex,figure] of figures.entries())for(const bound of [false,true]){
    const f=bound?scopedFixture():designReportFixture(),c=f.inspection.content;
    c.schema_version=schema;c.renderer.method=renderer;c.renderer.figure_method=figure;
    const contract=`${schemaIndex}:${rendererIndex}:${figureIndex}:${bound}`,before=JSON.stringify(f);
    if(supported.has(contract))assert.equal(check(f),f.inspection,contract);
    else assert.throws(()=>check(f),/Design report/,contract);
    assert.equal(JSON.stringify(f),before,contract);
  }
});

test('v3 rejects unknown or malformed drawings and preserves bound inventory and historical checks',()=>{
  for(const bound of [false,true])for(const edit of [
    f=>f.inspection.content.schema_version='greenlens.design-report-content.v4',
    f=>f.inspection.content.renderer.method='greenlens.design-report-renderer/4',
    f=>f.inspection.content.renderer.figure_method='greenlens.scoped-paired-drawing/3',
    f=>f.inspection.content.drawing=undefined,f=>f.inspection.content.drawing=[],f=>f.inspection.content.drawing='bound',f=>f.inspection.content.drawing={},
  ]){const f=preciseFixture(bound);edit(f);assert.throws(()=>check(f),/Design report/);}
  for(const edit of [
    f=>f.inspection.content.drawing.schema_version='future',
    f=>f.inspection.content.drawing.selection.scope_revision_sha256='d'.repeat(64),
    f=>f.inspection.content.drawing.scope_revision.scope.title='Different retained scope',
    f=>f.inspection.content.drawing.frames[2].vertical_span_m=4.01,
    f=>f.inspection.content.drawing.context.layers[0].triangles[0].occurrence_id='unknown-occurrence',
    f=>f.inspection.content.drawing.context.layers[0].omitted_visible_triangle_count=1,
    f=>f.inspection.content.workbench.base.proposed.model.zones[0].id='different-applied-model',
    f=>f.inspection.products.products=f.inspection.products.products.filter(p=>p.path!=='design-drawing.json'),
    f=>f.inspection.products.products=f.inspection.products.products.filter(p=>p.path!=='design-scope-revision.json'),
  ]){const f=preciseFixture(true);edit(f);assert.throws(()=>check(f),/Design report/);}
  const extra=preciseFixture();extra.inspection.products.products.push(...['design-drawing.json','design-scope-revision.json'].map(path=>({path,bytes:128,sha256:'2'.repeat(64)})));
  extra.inspection.products.products.sort((a,b)=>a.path<b.path?-1:1);extra.inspection.total_bytes+=256;
  assert.throws(()=>check(extra),/product inventory/);
});
