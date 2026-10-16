// Local pending-review admission/capacity only; Rust owns accepted validation.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {transformWithOxc} from 'vite';
const cache=new Map();
async function load(file){if(cache.has(file))return cache.get(file);const task=(async()=>{const{code}=await transformWithOxc(fs.readFileSync(file,'utf8'),file);let linked=code;for(const m of [...code.matchAll(/import\s+([^;]+?)\s+from\s+(['"])([^'"]+)\2;/g)]){let f=path.resolve(path.dirname(file),m[3]+'.ts');linked=linked.replace(m[0],`import ${m[1]} from ${JSON.stringify(await load(f))};`);}return'data:text/javascript;base64,'+Buffer.from(linked).toString('base64');})();cache.set(file,task);return task;}
const{ScopeReviewDraftStore,scopeReviewContextKey,reviewDecisions,validScopeCarryForwardPlan,validScopeReviewDraft}=await import(await load(fileURLToPath(new URL('../src/reference/scope-review-drafts.ts',import.meta.url))));
const h='a'.repeat(64);
const scope={schema_version:'greenlens.architectural-scope.v1',title:'Original fixture',purpose:'Local draft test',scope_kind:'partial_building',reference_revision_sha256:h,reference_geometry_sha256:h,reference_options_sha256:h,design_model_sha256:{A:h},mappings:[],included_scope:[],excluded_scope:[],boundary_consequences:'None tested',assumptions:[],reconciliations:[],coverage:{status:'unknown',reason:'Not measured'},whole_building_review:null};
const plan=()=>({schema_version:'greenlens.scope-carry-forward.v1',parent_revision_sha256:h,target_context_sha256:h,plan_sha256:h,proposed_scope:structuredClone(scope),rows:[{id:'field',kind:'input',key:'COP',change:'changed',requires_review:true,before:3,after:4,reason:'Explicit efficiency change'}]});
const draft=(id='one',design='A')=>({schema_version:'greenlens.scope-review-draft.v1',document_id:id,design_id:design,context_key:'exact UI binding',plan:plan(),decisions:{}});
test('bounded local store preserves both design drafts, recovery and earlier entries on capacity rejection',()=>{
  let raw=null;const storage={getItem:()=>raw,setItem:(_k,v)=>{raw=v;}},store=new ScopeReviewDraftStore(storage);
  for(const d of [draft('one','A'),draft('one','B'),draft('two','A'),draft('two','B')])store.set(d);
  const original=raw;assert.throws(()=>store.set(draft('three')),/capacity reached/);assert.equal(raw,original);assert.equal(store.entries.size,4);
  const recovered=new ScopeReviewDraftStore(storage);assert.deepEqual(recovered.get('one','B'),draft('one','B'));assert.equal(recovered.get('unknown','A'),null);recovered.delete('one','A');assert.ok(recovered.get('one','B'));assert.equal(recovered.get('one','A'),null);
});
test('malformed persisted plans or decisions cannot become accepted review state',()=>{
  for(const corrupt of [d=>d.plan.schema_version='unknown',d=>d.plan.parent_revision_sha256='wrong',d=>d.plan.rows.push({...d.plan.rows[0]}),d=>d.plan.rows[0].change='equivalent-by-name',d=>d.decisions.field={row_id:'other',resolution:'reviewed',rationale:'No'},d=>d.decisions.extra={row_id:'extra',resolution:'reviewed',rationale:'No'},d=>d.design_id='C',d=>d.context_key='x'.repeat(4*1024*1024+1)]){
    const d=draft();corrupt(d);assert.equal(validScopeReviewDraft(d),false);const store=new ScopeReviewDraftStore({getItem:()=>JSON.stringify([d]),setItem(){}});assert.equal(store.entries.size,0);assert.match(store.warning,/could not be admitted/);
  }
  assert.equal(validScopeCarryForwardPlan(plan()),true);
});
test('incomplete decisions cannot apply; unresolved decisions need a retained rationale and stay unresolved',()=>{
  const d=draft();assert.throws(()=>reviewDecisions(d),/choose Reviewed or Unresolved/);d.decisions.field={row_id:'field',resolution:'unresolved',rationale:' '};assert.throws(()=>reviewDecisions(d),/rationale/);d.decisions.field.rationale='Source evidence is missing; retain this disclosure.';assert.deepEqual(reviewDecisions(d),[d.decisions.field]);assert.equal(d.decisions.field.resolution,'unresolved');
});
test('local recovery failure retains session draft and reports the limitation without accepting it',()=>{
  const store=new ScopeReviewDraftStore({getItem:()=>null,setItem(){throw Error('Storage unavailable');}}),d=draft();store.set(d);assert.equal(store.get('one','A'),d);assert.match(store.warning,/remain in this window/);store.delete('one','A');assert.equal(store.get('one','A'),null);
});
test('request context excludes display and unrelated historical additions but binds accepted models and source revision',()=>{
  const project={id:'original',energy:{designs:{A:{model:{name:'Applied'},study:null}},workbench:{calendars:{}}},architecture:{active_asset_id:'ref',assets:[{id:'ref',manifest_sha256:h}]}};
  const key=scopeReviewContextKey('saved',project,'A'),display=structuredClone(project);display.display_state={mode:'wireframe'};display.architecture.scope_history={revisions:[{revision_sha256:'historical'}]};assert.equal(scopeReviewContextKey('saved',display,'A'),key);
  const edited=structuredClone(project);edited.energy.designs.A.model.name='Changed accepted input';assert.notEqual(scopeReviewContextKey('saved',edited,'A'),key);assert.notEqual(scopeReviewContextKey('copy',project,'A'),key);assert.notEqual(scopeReviewContextKey('saved',project,'B'),key);edited.architecture.assets[0].manifest_sha256='b'.repeat(64);assert.notEqual(scopeReviewContextKey('saved',edited,'A'),key);
});
