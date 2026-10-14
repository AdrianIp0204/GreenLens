import type { CarbonFactor, EnergyDesign, EnergyReportMetadata } from './types';

export type ReportSettings = {carbon_factor:CarbonFactor|null;lighting_service_evidence:string|null};
export type CarbonFields = {enabled:boolean;id:string;region:string;year:string;kg_per_kwh:string;unit:'kg_co2'|'kg_co2e';source:string;version:string;synthetic:boolean};
export type ReportSettingsDraft = {base:string;factor:string;carbon?:CarbonFields;carbonMode?:'fields'|'json';service:string;dirty:boolean;error:string};
export function carbonFields(value:CarbonFactor|null):CarbonFields{return{enabled:value!==null,id:value?.id??'',region:value?.region??'',year:value?String(value.year):'',kg_per_kwh:value?String(value.kg_per_kwh):'',unit:value?.unit??'kg_co2e',source:value?.source??'',version:value?.version??'',synthetic:value?.synthetic??false};}
export function validCarbonFields(value:unknown):value is CarbonFields{const c=value as CarbonFields;return Boolean(c)&&typeof c.enabled==='boolean'&&typeof c.synthetic==='boolean'&&['kg_co2','kg_co2e'].includes(c.unit)&&['id','region','year','kg_per_kwh','source','version'].every(key=>typeof c[key as keyof CarbonFields]==='string');}
export const reportKey=(value:ReportSettings)=>JSON.stringify([value.carbon_factor,value.lighting_service_evidence]);
export function cleanReportDraft(value:ReportSettings):ReportSettingsDraft{return{base:reportKey(value),factor:JSON.stringify(value.carbon_factor,null,2),carbon:carbonFields(value.carbon_factor),carbonMode:'fields',service:value.lighting_service_evidence??'',dirty:false,error:''};}
export function carbonCandidate(draft:ReportSettingsDraft):CarbonFactor|null {
  if(draft.carbonMode!=='fields'||!draft.carbon)return JSON.parse(draft.factor);
  const c=draft.carbon;if(!c.enabled)return null;
  if(!/^[0-9]+$/.test(c.year.trim())||!Number.isSafeInteger(Number(c.year)))throw new Error('Carbon year: enter a whole year before applying.');
  if(!/^[+]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$/i.test(c.kg_per_kwh.trim())||!Number.isFinite(Number(c.kg_per_kwh)))throw new Error('Carbon factor: enter a finite nonnegative kg/kWh value before applying.');
  return {id:c.id,fuel:'electricity',region:c.region,year:Number(c.year),kg_per_kwh:Number(c.kg_per_kwh),unit:c.unit,source:c.source,version:c.version,synthetic:c.synthetic};
}
export function reportCandidate(draft:ReportSettingsDraft,value:ReportSettings):ReportSettings {
  if(draft.base!==reportKey(value))throw new Error('Accepted report settings changed. Your draft is retained; copy needed text, then discard and edit the current settings.');
  return {carbon_factor:carbonCandidate(draft),lighting_service_evidence:draft.service.trim()||null};
}

export type ReportIssueDraft={base:string;enabled:boolean;title:string;revision:string;issue_date:string;author:string;preparer:string;revision_history:EnergyReportMetadata['revision_history'];dirty:boolean;error:string};
export const issueKey=(value:EnergyReportMetadata|null)=>JSON.stringify(value);
export function cleanIssueDraft(value:EnergyReportMetadata|null):ReportIssueDraft{return{base:issueKey(value),enabled:value!==null,title:value?.title??'',revision:value?.revision??'',issue_date:value?.issue_date??'',author:value?.author??'',preparer:value?.preparer??'',revision_history:structuredClone(value?.revision_history??[]),dirty:false,error:''};}
export function validIssueDraft(value:unknown):value is ReportIssueDraft{const d=value as ReportIssueDraft;return Boolean(d)&&['base','title','revision','issue_date','author','preparer'].every(k=>typeof d[k as keyof ReportIssueDraft]==='string')&&typeof d.enabled==='boolean'&&d.dirty===true&&Array.isArray(d.revision_history)&&d.revision_history.length<=32&&d.revision_history.every(row=>row&&['revision','issue_date','description'].every(k=>typeof row[k as keyof typeof row]==='string'));}
export function issueCandidate(draft:ReportIssueDraft,accepted:EnergyReportMetadata|null):EnergyReportMetadata|null{
  if(draft.base!==issueKey(accepted))throw new Error('Accepted report issue information changed. Your draft is retained; copy needed text, then discard and edit the current information.');
  if(!draft.enabled)return null;
  return{title:draft.title,revision:draft.revision,issue_date:draft.issue_date.trim()||null,author:draft.author.trim()||null,preparer:draft.preparer.trim()||null,revision_history:structuredClone(draft.revision_history)}; // Rust validates explicit dates, lengths and required fields.
}

export type DraftSection = 'zone'|'envelope'|'systems'|'schedules'|'site'|'advanced';
export interface EnergyDraft {
  base: string;
  structured: EnergyDesign;
  raw: string;
  rawDirty: boolean;
  fields: Record<string,string>;
  invalid: Record<string,string>;
  section: DraftSection;
  dirty: boolean;
  error: string;
  version: number;
}
const CACHE = 'greenlens.thermal-drafts.v1';
const MAX_ENTRY = 1024 * 1024, MAX_CACHE = 4 * 1024 * 1024, MAX_ENTRIES = 16;
const bytes = (text:string) => new TextEncoder().encode(text).byteLength;
const clone = <T,>(value:T):T => structuredClone(value);
function formShape(value:unknown,expected:unknown,depth=0):boolean {
  if(depth>32)return false;
  if(expected===null)return value===null;
  if(Array.isArray(expected))return Array.isArray(value)&&value.length===expected.length&&value.every((v,i)=>formShape(v,expected[i],depth+1));
  if(typeof expected==='object')return value!==null&&typeof value==='object'&&!Array.isArray(value)&&Object.keys(value).length===Object.keys(expected as object).length&&Object.entries(expected as Record<string,unknown>).every(([k,v])=>Object.hasOwn(value,k)&&formShape((value as Record<string,unknown>)[k],v,depth+1));
  return typeof value===typeof expected&&(typeof value!=='number'||Number.isFinite(value));
}
export const designKey = (design:EnergyDesign) => JSON.stringify(design);
export function cleanDraft(design:EnergyDesign, section:DraftSection='zone', version=0):EnergyDraft {
  return {base:designKey(design),structured:clone(design),raw:JSON.stringify(design,null,2),rawDirty:false,fields:{},invalid:{},section,dirty:false,error:'',version};
}
export function draftCandidate(draft:EnergyDraft, accepted:EnergyDesign):EnergyDesign {
  if(draft.base!==designKey(accepted))throw new Error('Accepted inputs changed while this draft was open. Your draft is retained. Copy any needed text, then discard and edit the current inputs.');
  if(draft.rawDirty){
    const value:unknown=JSON.parse(draft.raw);
    if(!value||typeof value!=='object'||!('model' in value)||!('study' in value))throw new Error('Advanced JSON must contain a complete model and study.');
    return value as EnergyDesign; // Rust validates the complete candidate before acceptance.
  }
  const invalid=Object.values(draft.invalid);
  if(invalid.length)throw new Error(invalid.join('\n'));
  return clone(draft.structured);
}
export function structuredEdit(draft:EnergyDraft, action:(next:EnergyDesign)=>void):EnergyDraft {
  if(draft.rawDirty)throw new Error('Apply or discard the Advanced JSON draft before changing structured fields.');
  const next=clone(draft.structured);action(next);
  return {...draft,structured:next,raw:JSON.stringify(next,null,2),dirty:true,error:'',version:draft.version+1};
}
export function fieldEdit(draft:EnergyDraft,key:string,text:string,action:(next:EnergyDesign,value:number[])=>void,list=false):EnergyDraft {
  if(draft.rawDirty)throw new Error('Apply or discard the Advanced JSON draft before changing structured fields.');
  const tokens=list?text.split(','):[text];
  const valid=tokens.every(t=>t.trim()!==''&&/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:e[+-]?\d+)?$/i.test(t.trim())&&Number.isFinite(Number(t)));
  const next=clone(draft.structured),invalid={...draft.invalid};
  if(valid){action(next,tokens.map(Number));delete invalid[key];}else invalid[key]=`${key}: enter finite numeric values before applying.`;
  return {...draft,structured:next,raw:JSON.stringify(next,null,2),fields:{...draft.fields,[key]:text},invalid,dirty:true,error:'',version:draft.version+1};
}

/** Bounded, local-only recovery cache. Untrusted cached data never bypasses Rust Apply. */
export class EnergyDraftStore {
  entries=new Map<string,EnergyDraft>();
  reports=new Map<string,ReportSettingsDraft>();
  issues=new Map<string,ReportIssueDraft>();
  warning='';
  private recovered=new Set<string>();
  private listeners=new Set<()=>void>();
  constructor(private storage?:Pick<Storage,'getItem'|'setItem'>){
    try{
      const text=storage?.getItem(CACHE);if(!text)return;
      if(bytes(text)>MAX_CACHE)throw new Error('Draft cache exceeds its local size limit.');
      const value=JSON.parse(text);
      if(value.schema!=='greenlens.thermal-drafts.v1'||!Array.isArray(value.entries)||value.entries.length>MAX_ENTRIES)throw new Error('Draft cache format is invalid.');
      for(const item of value.entries){
        if(!Array.isArray(item)||item.length!==2||typeof item[0]!=='string'||item[0].length>256)throw new Error('Draft cache identity is invalid.');
        const identity=JSON.parse(item[0]);if(!Array.isArray(identity)||identity.length!==2||typeof identity[0]!=='string'||!['A','B'].includes(identity[1]))throw new Error('Draft cache design identity is invalid.');
        const d=item[1];
        if(bytes(JSON.stringify(d))>MAX_ENTRY||!d||typeof d.base!=='string'||typeof d.raw!=='string'||typeof d.rawDirty!=='boolean'||d.dirty!==true||!Number.isSafeInteger(d.version)||d.version<0||!['zone','envelope','systems','schedules','site','advanced'].includes(d.section))throw new Error('Draft cache entry is invalid.');
        for(const fields of [d.fields,d.invalid])if(!fields||typeof fields!=='object'||Array.isArray(fields)||Object.values(fields).some(v=>typeof v!=='string'))throw new Error('Draft cache text is invalid.');
        // Do not render a cached object as a form until its shape matches the accepted snapshot.
        const base=JSON.parse(d.base);
        if(!base?.model?.zones?.length||!d.structured?.model?.zones?.length)throw new Error('Draft cache model is incomplete.');
        this.entries.set(item[0],{...d,error:'Recovered unsaved local draft. Review and Apply explicitly; this is not saved project evidence.'});
        this.recovered.add(item[0]);
      }
      if(!Array.isArray(value.reports??[])||(value.reports??[]).length+value.entries.length>MAX_ENTRIES)throw new Error('Report draft cache count is invalid.');
      for(const item of value.reports??[]){const [id,d]=item;if(typeof id!=='string'||id.length>128||!d||bytes(JSON.stringify(d))>MAX_ENTRY||typeof d.base!=='string'||typeof d.factor!=='string'||typeof d.service!=='string'||d.dirty!==true||d.carbon!==undefined&&!validCarbonFields(d.carbon)||d.carbonMode!==undefined&&!['fields','json'].includes(d.carbonMode))throw new Error('Report draft cache is invalid.');this.reports.set(id,{...d,error:'Recovered unsaved local report settings. Apply explicitly.'});}
      if(!Array.isArray(value.issues??[])||(value.issues??[]).length+(value.reports??[]).length+value.entries.length>MAX_ENTRIES)throw new Error('Report issue draft count is invalid.');
      for(const [id,d] of value.issues??[]){if(typeof id!=='string'||id.length>128||bytes(JSON.stringify(d))>MAX_ENTRY||!validIssueDraft(d))throw new Error('Report issue draft is invalid.');this.issues.set(id,{...d,error:'Recovered unsaved local report issue information. Apply explicitly.'});}
    }catch{this.entries.clear();this.reports.clear();this.issues.clear();this.warning='Local draft recovery is unavailable or invalid. Accepted project data was not changed.';}
  }
  subscribe=(listener:()=>void)=>{this.listeners.add(listener);return()=>{this.listeners.delete(listener);};};
  key(projectId:string,alternative:string){return JSON.stringify([projectId,alternative]);}
  get(projectId:string,alternative:string,design:EnergyDesign):EnergyDraft {
    const key=this.key(projectId,alternative),old=this.entries.get(key);
    if(!old||(!old.dirty&&old.base!==designKey(design))){const next=cleanDraft(design,old?.section,(old?.version??0)+1);this.entries.set(key,next);return next;}
    if(this.recovered.delete(key)&&!formShape(old.structured,design)){
      const next={...old,structured:clone(design),rawDirty:true,section:'advanced' as const,error:'Recovered draft structure differs from the current form. Its exact JSON is retained in Advanced for explicit Rust validation.'};
      this.entries.set(key,next);return next;
    }
    return old;
  }
  set(projectId:string,alternative:string,draft:EnergyDraft){this.entries.set(this.key(projectId,alternative),draft);this.persist();this.listeners.forEach(f=>f());}
  discard(projectId:string,alternative:string,design:EnergyDesign){const old=this.get(projectId,alternative,design);this.set(projectId,alternative,cleanDraft(design,old.section,old.version+1));}
  report(projectId:string,value:ReportSettings){const old=this.reports.get(projectId);if(!old||!old.dirty&&old.base!==reportKey(value)){const next=cleanReportDraft(value);this.reports.set(projectId,next);return next;}return old;}
  setReport(projectId:string,draft:ReportSettingsDraft){this.reports.set(projectId,draft);this.persist();this.listeners.forEach(f=>f());}
  issue(projectId:string,value:EnergyReportMetadata|null){const old=this.issues.get(projectId);if(!old||!old.dirty&&old.base!==issueKey(value)){const next=cleanIssueDraft(value);this.issues.set(projectId,next);return next;}return old;}
  setIssue(projectId:string,draft:ReportIssueDraft){this.issues.set(projectId,draft);this.persist();this.listeners.forEach(f=>f());}
  pending(projectId:string){return [...this.entries].filter(([key,d])=>d.dirty&&JSON.parse(key)[0]===projectId).map(([key])=>JSON.parse(key)[1] as string).sort().concat(this.reports.get(projectId)?.dirty?['report settings']:[],this.issues.get(projectId)?.dirty?['report issue']:[]);}
  private persist(){
    try{
      const entries=[...this.entries].filter(([,d])=>d.dirty),reports=[...this.reports].filter(([,d])=>d.dirty),issues=[...this.issues].filter(([,d])=>d.dirty);
      if(entries.length+reports.length+issues.length>MAX_ENTRIES||[...entries,...reports,...issues].some(([,d])=>bytes(JSON.stringify(d))>MAX_ENTRY))throw new Error('limit');
      const text=JSON.stringify({schema:'greenlens.thermal-drafts.v1',entries,reports,issues});if(bytes(text)>MAX_CACHE)throw new Error('limit');
      if(!this.storage)throw new Error('storage');this.storage.setItem(CACHE,text);this.warning='';
    }catch{this.warning='Drafts remain in this window but could not be cached locally. Keep this window open and Apply or copy your edits before closing.';}
  }
}
