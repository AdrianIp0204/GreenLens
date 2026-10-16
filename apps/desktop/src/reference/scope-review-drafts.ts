import type {Project} from '../types';
import type {ScopeCarryForwardPlan,ScopeReviewDecision,ReferenceScopeDraft} from './types';
import {isReferenceWorkbenchDraft,parseReferenceScopeDraft} from './reference-drafts';

export const MAX_REVIEW_RATIONALE_BYTES=2048;
export const rationaleBytes=(value:string)=>new TextEncoder().encode(value).length;
export const MAX_REVIEW_DRAFT_BYTES=20*1024*1024,MAX_REVIEW_STORE_BYTES=64*1024*1024,MAX_REVIEW_DRAFTS=4;
const hash=(v:unknown):v is string=>typeof v==='string'&&/^[a-f0-9]{64}$/.test(v);
const text=(v:unknown,max=16384):v is string=>typeof v==='string'&&v.length<=max;
const record=(v:unknown):v is Record<string,unknown>=>Boolean(v)&&typeof v==='object'&&!Array.isArray(v);
const bytes=(v:unknown)=>new TextEncoder().encode(JSON.stringify(v)).length;
/** IPC maps may order keys differently. This is a bounded exact structural
 * comparison, not a scientific hash or a tolerance-based normalization. */
export function sameReviewValue(left:unknown,right:unknown):boolean {
  const pending:[unknown,unknown,number][]=[[left,right,0]];let remaining=2_000_000;
  while(pending.length){const [a,b,depth]=pending.pop()!;if(--remaining<0||depth>64||typeof a!==typeof b)return false;
    if(a===null||b===null){if(a!==b)return false;continue;}
    if(typeof a!=='object'){if(a!==b||typeof a==='number'&&!Number.isFinite(a))return false;continue;}
    if(Array.isArray(a)||Array.isArray(b)){if(!Array.isArray(a)||!Array.isArray(b)||a.length!==b.length)return false;for(let i=0;i<a.length;i++)pending.push([a[i],b[i],depth+1]);continue;}
    const x=a as Record<string,unknown>,y=b as Record<string,unknown>,keys=Object.keys(x);if(keys.length!==Object.keys(y).length)return false;
    for(const key of keys){if(!Object.hasOwn(y,key))return false;pending.push([x[key],y[key],depth+1]);}
  }return true;
}
export type ScopeReviewDraft={schema_version:'greenlens.scope-review-draft.v1'|'greenlens.scope-review-draft.v2';document_id:string;design_id:'A'|'B';context_key:string;plan:ScopeCarryForwardPlan;candidate?:ReferenceScopeDraft;decisions:Record<string,ScopeReviewDecision>};

/** UI request binding only. Rust independently validates the complete plan and
 * target context before acceptance; this does not define scientific identity. */
export function scopeReviewContextKey(path:string|null,project:Project|null,designId:string):string {
  const architecture=project?.architecture,active=architecture?.assets.find(asset=>asset.id===architecture.active_asset_id);
  return JSON.stringify([path,project?.id,designId,project?.energy?.designs,project?.energy?.workbench?.calendars,active]);
}
export function validScopeCarryForwardPlan(value:unknown):value is ScopeCarryForwardPlan {
  if(!record(value)||!['greenlens.scope-carry-forward.v1','greenlens.scope-carry-forward.v2'].includes(String(value.schema_version))||![value.parent_revision_sha256,value.target_context_sha256,value.plan_sha256].every(hash)||!Array.isArray(value.rows)||value.rows.length>65536)return false;
  const scopeVersion=value.schema_version==='greenlens.scope-carry-forward.v2'?'greenlens.architectural-scope.v2':'greenlens.architectural-scope.v1';
  if(!record(value.proposed_scope)||value.proposed_scope.schema_version!==scopeVersion)return false;
  if(!isReferenceWorkbenchDraft({schema_version:'greenlens.reference-workbench-draft.v1',document_id:'plan',import:null,scope:{value:value.proposed_scope,numbers:{}}},'plan'))return false;
  if(value.schema_version==='greenlens.scope-carry-forward.v2'&&(!record(value.candidate_scope)||value.candidate_scope.schema_version!==scopeVersion||!isReferenceWorkbenchDraft({schema_version:'greenlens.reference-workbench-draft.v1',document_id:'candidate',import:null,scope:{value:value.candidate_scope,numbers:{}}},'candidate')))return false;
  if(value.schema_version==='greenlens.scope-carry-forward.v1'&&value.candidate_scope!=null)return false;
  const seen=new Set<string>();
  if(!value.rows.every(row=>record(row)&&text(row.id)&&Boolean(row.id)&&!seen.has(row.id)&&Boolean(seen.add(row.id))&&text(row.key)&&['scope','reference','mapping','assumption','reconciliation','report_view','input'].includes(String(row.kind))&&['unchanged','changed','deleted','added','unresolved'].includes(String(row.change))&&typeof row.requires_review==='boolean'&&text(row.reason)&&Object.hasOwn(row,'before')&&Object.hasOwn(row,'after')))return false;
  try{return bytes(value)<=16*1024*1024;}catch{return false;}
}
function validDecision(v:unknown,rowId:string):v is ScopeReviewDecision {
  return record(v)&&v.row_id===rowId&&['reviewed','unresolved'].includes(String(v.resolution))&&text(v.rationale,16384);
}
export function validScopeReviewDraft(value:unknown):value is ScopeReviewDraft {
  if(!record(value)||!['greenlens.scope-review-draft.v1','greenlens.scope-review-draft.v2'].includes(String(value.schema_version))||!text(value.document_id,128)||!value.document_id||!['A','B'].includes(String(value.design_id))||!text(value.context_key,4*1024*1024)||!validScopeCarryForwardPlan(value.plan)||!record(value.decisions)||Object.keys(value.decisions).length>65536)return false;
  if(value.schema_version==='greenlens.scope-review-draft.v2'&&(value.plan.schema_version!=='greenlens.scope-carry-forward.v2'||!isReferenceWorkbenchDraft({schema_version:'greenlens.reference-workbench-draft.v1',document_id:value.document_id,import:null,scope:value.candidate},value.document_id)))return false;
  if(value.schema_version==='greenlens.scope-review-draft.v1'&&(value.plan.schema_version!=='greenlens.scope-carry-forward.v1'||value.candidate!==undefined))return false;
  const rows=new Set(value.plan.rows.map(row=>row.id));
  try{return Object.entries(value.decisions).every(([id,d])=>rows.has(id)&&validDecision(d,id))&&bytes(value)<=MAX_REVIEW_DRAFT_BYTES;}catch{return false;}
}
export function reviewCandidate(draft:ScopeReviewDraft){
  if(draft.plan.schema_version==='greenlens.scope-carry-forward.v1')return draft.plan.proposed_scope;
  if(!draft.candidate)throw Error('Editable candidate is missing; the review draft is retained.');
  const candidate=parseReferenceScopeDraft(draft.candidate);
  if(!sameReviewValue(candidate,draft.plan.candidate_scope))throw Error('Candidate edits have not been reviewed in Rust. Choose Review edited candidate; the draft is retained.');
  return candidate;
}
export function reviewDecisions(draft:ScopeReviewDraft):ScopeReviewDecision[]{
  reviewCandidate(draft);
  const required=draft.plan.rows.filter(row=>row.requires_review);
  for(const row of required){const decision=draft.decisions[row.id];if(!decision||!validDecision(decision,row.id)||!decision.rationale.trim())throw Error(`Review ${row.kind} ${row.key}: choose Reviewed or Unresolved and enter a rationale. The review draft is retained.`);}
  const decisions=draft.plan.rows.flatMap(row=>draft.decisions[row.id]?[draft.decisions[row.id]]:[]);
  for(const decision of decisions)if(rationaleBytes(decision.rationale)>MAX_REVIEW_RATIONALE_BYTES)throw Error(`Review rationale exceeds ${MAX_REVIEW_RATIONALE_BYTES} UTF-8 bytes (${rationaleBytes(decision.rationale)} used). Shorten it before Apply; the complete draft is retained.`);
  return decisions;
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
