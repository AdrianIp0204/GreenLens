import { useRef, useState } from 'react';
import type { CameraState, ViewId } from '../viewport/types';
import type { ThermalInspectorTarget } from './thermal-inspector-target';
import { validReferencePresentation, type ReferencePresentation } from './reference-presentation';
import type { ReferenceOccurrenceSelection } from '../reference/selection';

export const THERMAL_VIEWS:ViewId[]=['top','perspective','front','right'];
export const THERMAL_LAYERS=['surface','window','overhang','shading'] as const;
export type ThermalLayer=typeof THERMAL_LAYERS[number];
export const ENERGY_PANELS=['model','reference','results','series','history','designs','compare','weather','preflight','sensitivity','reports','evidence'] as const;
export type EnergyPanel=typeof ENERGY_PANELS[number];
export type ReportContentsProfile='executive'|'technical';
export interface ThermalWorkspacePresentation {
  panel:EnergyPanel;
  inspector:ThermalInspectorTarget|null;
  reference_selection:ReferenceOccurrenceSelection|null;
  /** Additive v0.7 local state; never substituted for an active viewport pick. */
  reference_inspection?:ReferenceOccurrenceSelection|null;
  /** Which generated family's contents are being inspected; not an export filter. */
  report_contents_profile?:ReportContentsProfile;
  /** Read-only scope history choice; distinct from active scope and viewport. */
  scope_revision_sha256?:string|null;
  /** Report choices remain separate from live scope and model editing. */
  report_scope_revision_sha256?:string|null;
  report_issue_id?:string|null;
  /** Undefined selects Rust currentness; null explicitly selects no run. */
  energy_run_selection?:Partial<Record<'A'|'B',string|null>>;
}
export function defaultThermalWorkspacePresentation():ThermalWorkspacePresentation{return{panel:'model',inspector:null,reference_selection:null};}
export interface ThermalSceneObject {id:string;label:string;kind:'zone'|ThermalLayer;zone_id:string|null;storey:number|null;polygons_m:[number,number,number][][]}
export interface ThermalScene {schema_version:'greenlens.thermal-scene.v1';model_sha256:string;coordinates:string;objects:ThermalSceneObject[]}
export interface ThermalPresentation {
  schema_version:'greenlens.thermal-presentation.v1';
  cameras:Partial<Record<ViewId,CameraState>>;
  active_view:ViewId;
  maximized_view:ViewId|null;
  selected:string|null;
  hidden:string[];
  isolated:string[]|null;
  layers:Record<ThermalLayer,boolean>;
  grid:boolean;
  tree_width:number;
  inspector_width:number;
  reference?:ReferencePresentation;
  /** Optional additive local state; legacy v1 entries remain readable. */
  workspace?:ThermalWorkspacePresentation;
}
export function defaultThermalPresentation():ThermalPresentation{return{schema_version:'greenlens.thermal-presentation.v1',cameras:{},active_view:'perspective',maximized_view:null,selected:null,hidden:[],isolated:null,layers:{surface:true,window:true,overhang:true,shading:true},grid:true,tree_width:210,inspector_width:320};}
const finite=(value:unknown)=>typeof value==='number'&&Number.isFinite(value)&&Math.abs(value)<=1e9;
const point=(value:unknown):value is number[]=>Array.isArray(value)&&value.length===3&&value.every(finite);
const ids=(value:unknown):value is string[]=>Array.isArray(value)&&value.length<=1024&&new Set(value).size===value.length&&value.every(id=>typeof id==='string'&&id.length<=256);
const identifier=(value:unknown)=>typeof value==='string'&&value.length>0&&value.length<=256;
export function validThermalWorkspacePresentation(value:unknown):value is ThermalWorkspacePresentation {
  const p=value as ThermalWorkspacePresentation;if(!p||!ENERGY_PANELS.includes(p.panel))return false;
  if(p.energy_run_selection!==undefined){const picks=p.energy_run_selection;if(!picks||typeof picks!=='object'||Array.isArray(picks)||Object.keys(picks).length>2||Object.entries(picks).some(([key,value])=>!['A','B'].includes(key)||(value!==null&&value!==undefined&&!identifier(value))))return false;}
  if(p.report_contents_profile!==undefined&&!['executive','technical'].includes(p.report_contents_profile))return false;
  if(p.scope_revision_sha256!==undefined&&p.scope_revision_sha256!==null&&(typeof p.scope_revision_sha256!=='string'||!/^[a-f0-9]{64}$/.test(p.scope_revision_sha256)))return false;
  for(const value of [p.report_scope_revision_sha256,p.report_issue_id])if(value!==undefined&&value!==null&&(typeof value!=='string'||!/^[a-f0-9]{64}$/.test(value)))return false;
  if(p.inspector!==null){const target=p.inspector;if(!target?.object||!identifier(target.object.kind)||!identifier(target.object.id)||target.object.zone_id!==null&&!identifier(target.object.zone_id)||typeof target.field_path!=='string'||target.field_path.length>1024)return false;}
  const validReference=(selection:ReferenceOccurrenceSelection|null|undefined)=>selection===null||Boolean(selection&&typeof selection.revision_sha256==='string'&&/^[a-f0-9]{64}$/.test(selection.revision_sha256)&&identifier(selection.occurrence_id));
  return validReference(p.reference_selection)&&(p.reference_inspection===undefined||validReference(p.reference_inspection));
}
export function validThermalPresentation(value:unknown):value is ThermalPresentation {
  const p=value as ThermalPresentation;if(!p||p.schema_version!=='greenlens.thermal-presentation.v1'||!THERMAL_VIEWS.includes(p.active_view)||p.maximized_view!==null&&!THERMAL_VIEWS.includes(p.maximized_view)||p.selected!==null&&(typeof p.selected!=='string'||p.selected.length>256)||!ids(p.hidden)||p.isolated!==null&&!ids(p.isolated)||typeof p.grid!=='boolean')return false;
  if(!p.layers||!THERMAL_LAYERS.every(k=>typeof p.layers[k]==='boolean')||!Number.isFinite(p.tree_width)||p.tree_width<150||p.tree_width>420||!Number.isFinite(p.inspector_width)||p.inspector_width<260||p.inspector_width>520||!p.cameras||typeof p.cameras!=='object'||Array.isArray(p.cameras))return false;
  if(p.reference!==undefined&&!validReferencePresentation(p.reference))return false;
  if(p.workspace!==undefined&&!validThermalWorkspacePresentation(p.workspace))return false;
  return Object.entries(p.cameras).every(([id,c])=>THERMAL_VIEWS.includes(id as ViewId)&&c&&point(c.position)&&point(c.target)&&point(c.up)&&c.up.some(n=>n!==0)&&c.position.some((n,i)=>n!==c.target[i])&&finite(c.zoom)&&c.zoom>0&&c.zoom<=10000);
}

/** Local presentation only. No project, study, run or scientific hash is modified. */
export class ThermalPresentationStore {
  entries=new Map<string,ThermalPresentation>();warning='';
  constructor(private storage?:Pick<Storage,'getItem'|'setItem'>){try{const text=storage?.getItem('greenlens.thermal-presentation.v1');if(!text)return;if(new TextEncoder().encode(text).length>2*1024*1024)throw Error();const rows=JSON.parse(text);if(!Array.isArray(rows)||rows.length>64)throw Error();for(const [key,value] of rows){const identity=JSON.parse(key);if(!Array.isArray(identity)||identity.length!==2||typeof identity[0]!=='string'||identity[0].length>128||!['A','B'].includes(identity[1])||!validThermalPresentation(value))throw Error();this.entries.set(key,value);}}catch{this.entries.clear();this.warning='Saved thermal view settings were invalid; default views are used. Project inputs are unchanged.';}}
  get(key:string){let p=this.entries.get(key);if(!p){p=defaultThermalPresentation();this.entries.set(key,p);while(this.entries.size>64)this.entries.delete(this.entries.keys().next().value!);}return p;}
  set(key:string,value:ThermalPresentation){if(!validThermalPresentation(value))throw Error('Invalid thermal presentation settings.');this.entries.delete(key);this.entries.set(key,value);while(this.entries.size>64)this.entries.delete(this.entries.keys().next().value!);try{const text=JSON.stringify([...this.entries]);if(new TextEncoder().encode(text).length>2*1024*1024||!this.storage)throw Error();this.storage.setItem('greenlens.thermal-presentation.v1',text);this.warning='';}catch{this.warning='Thermal views are retained in this window but could not be saved locally.';}}
}
export function useThermalPresentation(projectId:string|undefined,alternative:string){
  const [store]=useState(()=>{try{return new ThermalPresentationStore(window.localStorage);}catch{return new ThermalPresentationStore();}}),[,refresh]=useState(0);
  const key=JSON.stringify([projectId??'unopened',alternative]);const current=useRef(key);current.current=key;
  const update=(change:(value:ThermalPresentation)=>ThermalPresentation)=>{store.set(key,change(store.get(key)));if(current.current===key)refresh(n=>n+1);};
  const updateDesign=(design:'A'|'B',change:(value:ThermalPresentation)=>ThermalPresentation)=>{const target=JSON.stringify([projectId??'unopened',design]);store.set(target,change(store.get(target)));if(current.current===target)refresh(n=>n+1);};
  return{key,value:store.get(key),update,updateDesign,warning:store.warning,resetLayout:()=>update(p=>({...p,active_view:'perspective',maximized_view:null,tree_width:210,inspector_width:320}))};
}
