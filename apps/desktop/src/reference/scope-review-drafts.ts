import type {Project} from '../types';
import type {ScopeCarryForwardPlan,ScopeReviewDecision} from './types';
import {isReferenceWorkbenchDraft} from './reference-drafts';

export const MAX_REVIEW_DRAFT_BYTES=20*1024*1024,MAX_REVIEW_STORE_BYTES=64*1024*1024,MAX_REVIEW_DRAFTS=4;
const hash=(v:unknown):v is string=>typeof v==='string'&&/^[a-f0-9]{64}$/.test(v);
const text=(v:unknown,max=16384):v is string=>typeof v==='string'&&v.length<=max;
const record=(v:unknown):v is Record<string,unknown>=>Boolean(v)&&typeof v==='object'&&!Array.isArray(v);
const bytes=(v:unknown)=>new TextEncoder().encode(JSON.stringify(v)).length;
export type ScopeReviewDraft={schema_version:'greenlens.scope-review-draft.v1';document_id:string;design_id:'A'|'B';context_key:string;plan:ScopeCarryForwardPlan;decisions:Record<string,ScopeReviewDecision>};

/** UI request binding only. Rust independently validates the complete plan and
 * target context before acceptance; this does not define scientific identity. */
export function scopeReviewContextKey(path:string|null,project:Project|null,designId:string):string {
  const architecture=project?.architecture,active=architecture?.assets.find(asset=>asset.id===architecture.active_asset_id);
  return JSON.stringify([path,project?.id,designId,project?.energy?.designs,project?.energy?.workbench?.calendars,active]);
}
export function validScopeCarryForwardPlan(value:unknown):value is ScopeCarryForwardPlan {
  if(!record(value)||value.schema_version!=='greenlens.scope-carry-forward.v1'||![value.parent_revision_sha256,value.target_context_sha256,value.plan_sha256].every(hash)||!Array.isArray(value.rows)||value.rows.length>65536)return false;
  if(!isReferenceWorkbenchDraft({schema_version:'greenlens.reference-workbench-draft.v1',document_id:'plan',import:null,scope:{value:value.proposed_scope,numbers:{}}},'plan'))return false;
  const seen=new Set<string>();
  if(!value.rows.every(row=>record(row)&&text(row.id)&&Boolean(row.id)&&!seen.has(row.id)&&Boolean(seen.add(row.id))&&text(row.key)&&['scope','reference','mapping','assumption','reconciliation','report_view','input'].includes(String(row.kind))&&['unchanged','changed','deleted','added','unresolved'].includes(String(row.change))&&typeof row.requires_review==='boolean'&&text(row.reason)&&Object.hasOwn(row,'before')&&Object.hasOwn(row,'after')))return false;
  try{return bytes(value)<=16*1024*1024;}catch{return false;}
}
function validDecision(v:unknown,rowId:string):v is ScopeReviewDecision {
  return record(v)&&v.row_id===rowId&&['reviewed','unresolved'].includes(String(v.resolution))&&text(v.rationale,16384);
}
export function validScopeReviewDraft(value:unknown):value is ScopeReviewDraft {
  if(!record(value)||value.schema_version!=='greenlens.scope-review-draft.v1'||!text(value.document_id,128)||!value.document_id||!['A','B'].includes(String(value.design_id))||!text(value.context_key,4*1024*1024)||!validScopeCarryForwardPlan(value.plan)||!record(value.decisions)||Object.keys(value.decisions).length>65536)return false;
  const rows=new Set(value.plan.rows.map(row=>row.id));
  try{return Object.entries(value.decisions).every(([id,d])=>rows.has(id)&&validDecision(d,id))&&bytes(value)<=MAX_REVIEW_DRAFT_BYTES;}catch{return false;}
}
export function reviewDecisions(draft:ScopeReviewDraft):ScopeReviewDecision[]{
  const required=draft.plan.rows.filter(row=>row.requires_review);
  for(const row of required){const decision=draft.decisions[row.id];if(!decision||!validDecision(decision,row.id)||!decision.rationale.trim())throw Error(`Review ${row.kind} ${row.key}: choose Reviewed or Unresolved and enter a rationale. The review draft is retained.`);}
  return draft.plan.rows.flatMap(row=>draft.decisions[row.id]?[draft.decisions[row.id]]:[]);
}
export const reviewDraftKey=(documentId:string,designId:string)=>JSON.stringify([documentId,designId]);

/** Bounded pending review store. Capacity errors never evict another draft. */
export class ScopeReviewDraftStore {
  entries=new Map<string,ScopeReviewDraft>();warning='';
  constructor(private storage?:Pick<Storage,'getItem'|'setItem'>){
    try{const raw=storage?.getItem('greenlens.scope-review-drafts.v1');if(!raw)return;if(new TextEncoder().encode(raw).length>8*1024*1024)throw Error();const values:unknown=JSON.parse(raw);if(!Array.isArray(values)||values.length>MAX_REVIEW_DRAFTS||!values.every(validScopeReviewDraft))throw Error();for(const d of values){const key=reviewDraftKey(d.document_id,d.design_id);if(this.entries.has(key))throw Error();this.entries.set(key,d);}}
    catch{this.entries.clear();this.warning='Saved review drafts could not be admitted. Accepted scope history is unchanged.';}
  }
  get(id:string|undefined,design:string){return id?this.entries.get(reviewDraftKey(id,design))??null:null;}
  set(draft:ScopeReviewDraft){
    if(!validScopeReviewDraft(draft))throw Error('Invalid or oversized scope review draft; the earlier draft is retained.');
    const next=new Map(this.entries);next.set(reviewDraftKey(draft.document_id,draft.design_id),draft);
    if(next.size>MAX_REVIEW_DRAFTS||bytes([...next.values()])>MAX_REVIEW_STORE_BYTES)throw Error('Pending review capacity reached (four reviews / 64 MiB). Resolve an existing review before preparing another; nothing was removed.');
    this.entries=next;this.persist();
  }
  delete(id:string,design:string){this.entries.delete(reviewDraftKey(id,design));this.persist();}
  private persist(){try{const raw=JSON.stringify([...this.entries.values()]);if(new TextEncoder().encode(raw).length>8*1024*1024||!this.storage)throw Error();this.storage.setItem('greenlens.scope-review-drafts.v1',raw);this.warning='';}catch{this.warning='Review drafts remain in this window but local recovery storage is unavailable or exceeds 8 MiB. Apply/discard them before closing.';}}
}
