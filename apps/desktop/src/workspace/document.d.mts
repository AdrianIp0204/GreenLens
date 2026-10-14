import type { Model, ValidatedModel, RunRecord } from '../types';
import type { ClimateProjectDocument as Project, ClimateJobStatus, ClimateRunRecord, Study, StudyHistorySnapshot } from '../climate-types';
import type { EnergyDesign, EnergyRunRecord, EnergyMetadataChange, EnergyReportMetadata } from '../energy/types';
import type { ReportSettings } from '../energy/drafts';
export type EnergyDraftBatch={designs:Record<string,EnergyDesign>;reportSettings?:ReportSettings;reportIssue?:EnergyReportMetadata|null};
export function clone<T>(value:T):T;
export function documentSignature(project:Project|null):string;
export function studySnapshot(project:Project,alternativeId:string):StudyHistorySnapshot;
export function completedJobResult(job:ClimateJobStatus|null):RunRecord|ClimateRunRecord|EnergyRunRecord|null;
export function hasPendingEvidence(job:ClimateJobStatus|null,results?:RunRecord[],climateResults?:ClimateRunRecord[],energyResults?:EnergyRunRecord[]):boolean;
export type HistoryEntry =
 | { kind:'geometry'; alternativeId:string; before:Model; after:Model; label:string }
 | { kind:'study'; alternativeId:string; before:StudyHistorySnapshot; after:StudyHistorySnapshot; label:string }
 | { kind:'energy'; alternativeId:string; before:EnergyDesign; after:EnergyDesign; label:string }
 | { kind:'energy_group'; alternativeId:string; before:Record<string,EnergyDesign>; after:Record<string,EnergyDesign>; label:string }
 | { kind:'energy_batch'; alternativeId:string; before:EnergyDraftBatch; after:EnergyDraftBatch; label:string }
 | { kind:'energy_metadata'; alternativeId:string; before:EnergyMetadataChange; after:EnergyMetadataChange; label:string };
export type CheckEdit = (before:Model,after:Model,validated:ValidatedModel)=>void|Promise<void>;
export type ValidateModel = (model:Model,alternativeId:string,project:Project)=>Promise<ValidatedModel>;
export type ConfigureStudy = (project:Project,alternativeId:string,study:Study)=>Promise<Project>;
export type RestoreStudy = (project:Project,alternativeId:string,snapshot:StudyHistorySnapshot)=>Promise<Project>;
export type ConfigureEnergy = (project:Project,alternativeId:string,snapshot:EnergyDesign)=>Promise<Project>;
export type ConfigureEnergyMetadata = (project:Project,change:EnergyMetadataChange)=>Promise<Project>;
export type RestoreEnergyGroup = (project:Project,designs:Record<string,EnergyDesign>)=>Promise<Project>;
export type RestoreEnergyBatch = (project:Project,batch:EnergyDraftBatch)=>Promise<Project>;
export class DocumentHistory {
 past:HistoryEntry[]; future:HistoryEntry[]; busy:boolean;
 reset():void;
 edit(project:Project,alternativeId:string,candidate:Model,label:string,validate:ValidateModel,checkEdit?:CheckEdit):Promise<{project:Project;validated:ValidatedModel;changed:boolean}>;
 editStudy(project:Project,alternativeId:string,candidate:Study,label:string,configure:ConfigureStudy):Promise<{project:Project;changed:boolean}>;
 editEnergy(project:Project,alternativeId:string,candidate:EnergyDesign,label:string,configure:ConfigureEnergy):Promise<{project:Project;changed:boolean}>;
 editEnergyMetadata(project:Project,alternativeId:string,change:EnergyMetadataChange,label:string,configure:ConfigureEnergyMetadata):Promise<{project:Project;changed:boolean}>;
 editEnergyGroup(project:Project,alternativeId:string,targets:string[],label:string,configure:(project:Project)=>Promise<Project>):Promise<{project:Project;changed:boolean}>;
 editEnergyBatch(project:Project,alternativeId:string,batch:EnergyDraftBatch,label:string,configure:RestoreEnergyBatch,canCommit?:()=>boolean):Promise<{project:Project;changed:boolean}>;
 traverse(project:Project,direction:'undo'|'redo',validate:ValidateModel,checkEdit?:CheckEdit,restoreStudy?:RestoreStudy,restoreEnergy?:ConfigureEnergy,restoreEnergyMetadata?:ConfigureEnergyMetadata,restoreEnergyGroup?:RestoreEnergyGroup,restoreEnergyBatch?:RestoreEnergyBatch):Promise<{project:Project;validated:ValidatedModel;alternativeId:string;label:string}>;
}
export function protectReplacement(options:{needsGuard:boolean;decide:()=>Promise<'save'|'discard'|'cancel'>;save:()=>Promise<boolean>;discard:()=>Promise<void>;proceed:()=>Promise<void>}):Promise<boolean>;
export function isTextTarget(target:unknown):boolean;
