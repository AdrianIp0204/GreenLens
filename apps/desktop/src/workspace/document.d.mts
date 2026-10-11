import type { Model, Project, ValidatedModel } from '../types';
export function clone<T>(value:T):T;
export function documentSignature(project:Project|null):string;
export interface HistoryEntry { alternativeId:string; before:Model; after:Model; label:string }
export type CheckEdit = (before:Model,after:Model,validated:ValidatedModel)=>void|Promise<void>;
export class DocumentHistory {
 past:HistoryEntry[]; future:HistoryEntry[]; busy:boolean;
 reset():void;
 edit(project:Project,alternativeId:string,candidate:Model,label:string,validate:(model:Model)=>Promise<ValidatedModel>,checkEdit?:CheckEdit):Promise<{project:Project;validated:ValidatedModel;changed:boolean}>;
 traverse(project:Project,direction:'undo'|'redo',validate:(model:Model)=>Promise<ValidatedModel>,checkEdit?:CheckEdit):Promise<{project:Project;validated:ValidatedModel;alternativeId:string;label:string}>;
}
export function protectReplacement(options:{needsGuard:boolean;decide:()=>Promise<'save'|'discard'|'cancel'>;save:()=>Promise<boolean>;discard:()=>Promise<void>;proceed:()=>Promise<void>}):Promise<boolean>;
export function isTextTarget(target:unknown):boolean;
