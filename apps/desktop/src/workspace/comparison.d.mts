import type { Comparison, RunRecord } from '../types';
export interface HistoricalSelection { documentId:number; aId:string|null; bId:string|null }
export function historicalSelection(documentId:number,records:RunRecord[],previous:HistoricalSelection|null):HistoricalSelection;
export function selectedHistoricalRuns(records:RunRecord[],selection:HistoricalSelection):{a:RunRecord|null;b:RunRecord|null};
export function chooseHistoricalRun(records:RunRecord[],selection:HistoricalSelection,alternative:'A'|'B',id:string):HistoricalSelection;
export function comparisonKey(selection:HistoricalSelection):string;
export function snapshotRelation(run:RunRecord,scientificHash:string|undefined):string;
export class ComparisonRequestGate {
  generation:number;
  cancel():void;
  compare(a:RunRecord,b:RunRecord,backend:(a:RunRecord,b:RunRecord)=>Promise<Comparison>):Promise<{comparison:Comparison|null;error:string}|null>;
}
