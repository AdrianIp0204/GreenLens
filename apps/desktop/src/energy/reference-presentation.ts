import type { CameraState, ViewId } from '../viewport/types';
import type { MappingStatus, ReferenceMeasurement, ReferenceModel, ReferenceOccurrence, ReferencePoint, ReferenceTriangle } from '../reference/types';

/** Compact GPU display data only. Rust retains f64 geometry and measurements. */
export type ReferenceRenderChunk = { index:number; origin_m:ReferencePoint; positions:Float32Array; occurrences:Uint32Array };
export type ReferenceMappingRelationship = 'current'|'stale'|'pending'|'unassessed';
export type ReferenceOverlayData = { assetId:string; manifest:ReferenceModel; chunks:ReferenceRenderChunk[]; loading:boolean; status?:string; mappingStates?:Record<string,MappingStatus>; declaredMappingStates?:Record<string,MappingStatus>; mappingRelationship?:ReferenceMappingRelationship; mappingDisclosure?:string };
/** Relationship is supplied by the exact Rust assessment request, never inferred
 * from an occurrence's presence or a saved mapping declaration. */
export function referenceMappingLabel(data:ReferenceOverlayData,id:string):string {
  const status=(data.declaredMappingStates?.[id]??data.mappingStates?.[id]??'unresolved').replaceAll('_',' ');
  if(data.mappingRelationship==='current')return status;
  if(data.mappingRelationship==='stale')return `Historical ${status} · stale scope`;
  if(data.mappingRelationship==='pending')return `Declared ${status} · checking inputs`;
  return `Declared ${status} · unassessed`;
}
export type ReferenceVertexId = { chunk:number; triangle:number; corner:number };
export type ReferenceMeasureRequest = { assetId:string; a:ReferenceVertexId; b:ReferenceVertexId };
export type ReferenceMeasure = (request:ReferenceMeasureRequest)=>Promise<ReferenceMeasurement>;
export type ReferenceSection = { axis:'off'|'x'|'y'|'z'; offset_m:number; keep:'below'|'above' };
export type ReferenceNamedView = { name:string; cameras:Partial<Record<ViewId,CameraState>>; active_view:ViewId; maximized_view:ViewId|null; section:ReferenceSection };
export type ReferencePresentation = {
  schema_version:'greenlens.reference-presentation.v1'; revision_sha256:string;
  visible:boolean; analytical_visible:boolean; ghost:boolean; hidden:string[]; isolated:string[]|null;
  section:ReferenceSection; named_views:ReferenceNamedView[];
};
export function defaultReferencePresentation(revision=''):ReferencePresentation{return{schema_version:'greenlens.reference-presentation.v1',revision_sha256:revision,visible:true,analytical_visible:true,ghost:true,hidden:[],isolated:null,section:{axis:'off',offset_m:0,keep:'below'},named_views:[]};}
export function referencePresentation(value:ReferencePresentation|undefined,revision:string):ReferencePresentation {
  // Revision-scoped object IDs are never reused after reimport. Camera state is independent.
  return value?.revision_sha256===revision?value:defaultReferencePresentation(revision);
}
const validPoint=(p:unknown):p is ReferencePoint=>Array.isArray(p)&&p.length===3&&p.every(n=>typeof n==='number'&&Number.isFinite(n)&&Math.abs(n)<=1e9);
const ids=(v:unknown)=>Array.isArray(v)&&v.length<=1024&&new Set(v).size===v.length&&v.every(s=>typeof s==='string'&&s.length<=256);
const validSection=(s:ReferenceSection)=>s&&['off','x','y','z'].includes(s.axis)&&['below','above'].includes(s.keep)&&typeof s.offset_m==='number'&&Number.isFinite(s.offset_m)&&Math.abs(s.offset_m)<=1e9;
const views=['top','perspective','front','right'];
export function validReferencePresentation(p:ReferencePresentation):boolean {
  return Boolean(p&&p.schema_version==='greenlens.reference-presentation.v1'&&typeof p.revision_sha256==='string'&&p.revision_sha256.length<=64&&[p.visible,p.analytical_visible,p.ghost].every(b=>typeof b==='boolean')&&ids(p.hidden)&&(p.isolated===null||ids(p.isolated))&&validSection(p.section)&&Array.isArray(p.named_views)&&p.named_views.length<=12&&new Set(p.named_views.map(v=>v?.name)).size===p.named_views.length&&p.named_views.every(v=>v&&typeof v.name==='string'&&v.name.trim().length>0&&v.name.length<=64&&views.includes(v.active_view)&&(v.maximized_view===null||views.includes(v.maximized_view))&&validSection(v.section)&&v.cameras&&Object.entries(v.cameras).every(([id,c])=>views.includes(id)&&c&&validPoint(c.position)&&validPoint(c.target)&&validPoint(c.up)&&c.up.some(n=>n!==0)&&c.position.some((n,i)=>n!==c.target[i])&&Number.isFinite(c.zoom)&&c.zoom>0&&c.zoom<=10000)));
}
/** Call once per bounded IPC chunk, then discard its JSON triangles. No decimation. */
export function compactReferenceChunk(index:number,triangles:ReferenceTriangle[],origin_m:ReferencePoint=[0,0,0]):ReferenceRenderChunk {
  if(!Number.isSafeInteger(index)||index<0||triangles.length>8192||!validPoint(origin_m))throw Error('Invalid admitted reference display chunk.');
  const positions=new Float32Array(triangles.length*9),occurrences=new Uint32Array(triangles.length);
  triangles.forEach((t,i)=>{if(!Number.isSafeInteger(t.occurrence_index)||t.occurrence_index<0||t.occurrence_index>0xffffffff||t.vertices_m.length!==3||!t.vertices_m.every(validPoint))throw Error('Malformed admitted reference display triangle.');occurrences[i]=t.occurrence_index;for(let c=0;c<3;c++)for(let a=0;a<3;a++)positions[i*9+c*3+a]=t.vertices_m[c][a]-origin_m[a];});
  return{index,origin_m:[...origin_m],positions,occurrences};
}
export function occurrenceVisible(occurrence:ReferenceOccurrence,display:ReferencePresentation,occurrences:ReferenceOccurrence[]):boolean {
  if(!display.visible)return false;
  const parents=new Map(occurrences.map(o=>[o.id,o]));return occurrenceVisibility(parents,display)(occurrence.id);
}
/** Bounded, memoized ancestry visibility for one display update (including group nodes). */
export function occurrenceVisibility(occurrences:Map<string,ReferenceOccurrence>,display:ReferencePresentation):(id:string)=>boolean {
  const cache=new Map<string,boolean>(),hidden=new Set(display.hidden),isolated=display.isolated===null?null:new Set(display.isolated);
  return id=>{const saved=cache.get(id);if(saved!==undefined)return saved;let current=occurrences.get(id),included=isolated===null,visible=display.visible,depth=0;while(current&&depth++<=64){if(hidden.has(current.id))visible=false;if(isolated?.has(current.id))included=true;current=current.parent_id?occurrences.get(current.parent_id):undefined;}const result=visible&&included;cache.set(id,result);return result;};
}
export function pointWithinSection(point:ReferencePoint,section:ReferenceSection):boolean {if(section.axis==='off')return true;const value=point[{x:0,y:1,z:2}[section.axis]];return section.keep==='below'?value<=section.offset_m+1e-8:value>=section.offset_m-1e-8;}
