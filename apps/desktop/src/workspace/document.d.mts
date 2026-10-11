import type { Model, ValidatedModel, RunRecord } from '../types';
import type { ClimateProjectDocument as Project, ClimateJobStatus, ClimateRunRecord, Study, StudyHistorySnapshot } from '../climate-types';
export function clone<T>(value:T):T;
export function documentSignature(project:Project|null):string;
export function studySnapshot(project:Project,alternativeId:string):StudyHistorySnapshot;
export function completedJobResult(job:ClimateJobStatus|null):RunRecord|ClimateRunRecord|null;
export function hasPendingEvidence(job:ClimateJobStatus|null,results?:RunRecord[],climateResults?:ClimateRunRecord[]):boolean;
export type HistoryEntry =
 | { kind:'geometry'; alternativeId:string; before:Model; after:Model; label:string }
 | { kind:'study'; alternativeId:string; before:StudyHistorySnapshot; after:StudyHistorySnapshot; label:string };
export type CheckEdit = (before:Model,after:Model,validated:ValidatedModel)=>void|Promise<void>;
export type ValidateModel = (model:Model,alternativeId:string,project:Project)=>Promise<ValidatedModel>;
export type ConfigureStudy = (project:Project,alternativeId:string,study:Study)=>Promise<Project>;
export type RestoreStudy = (project:Project,alternativeId:string,snapshot:StudyHistorySnapshot)=>Promise<Project>;
export class DocumentHistory {
 past:HistoryEntry[]; future:HistoryEntry[]; busy:boolean;
 reset():void;
 edit(project:Project,alternativeId:string,candidate:Model,label:string,validate:ValidateModel,checkEdit?:CheckEdit):Promise<{project:Project;validated:ValidatedModel;changed:boolean}>;
 editStudy(project:Project,alternativeId:string,candidate:Study,label:string,configure:ConfigureStudy):Promise<{project:Project;changed:boolean}>;
 traverse(project:Project,direction:'undo'|'redo',validate:ValidateModel,checkEdit?:CheckEdit,restoreStudy?:RestoreStudy):Promise<{project:Project;validated:ValidatedModel;alternativeId:string;label:string}>;
}
export function protectReplacement(options:{needsGuard:boolean;decide:()=>Promise<'save'|'discard'|'cancel'>;save:()=>Promise<boolean>;discard:()=>Promise<void>;proceed:()=>Promise<void>}):Promise<boolean>;
export function isTextTarget(target:unknown):boolean;
