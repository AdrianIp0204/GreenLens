import type { ClimateRunSummary, ClimateRunRecord, ClimateValues, CurrentClimateResult, MetricSettings, ResultField, Study, WeatherIntervalView } from '../climate-types';
import type { ValidatedModel } from '../types';
export const CLIMATE_FIELDS: [ResultField,string,string][];
export function displayedMetricSettings(selectedRunId:string,study:Study|null,run:ClimateRunRecord|null):MetricSettings|null;
export function metricConvention(field:ResultField,settings:MetricSettings|null):string;
export function climateRequestKey(path:string, summary:ClimateRunSummary|null, field:ResultField, interval:number, settings:MetricSettings|null):string;
export function climateGeometryMatches(summary:ClimateRunSummary|null,current:CurrentClimateResult|null,sensorIds:string[]|undefined,validated:ValidatedModel|null):boolean;
export function climateOverlayMatches(summary:ClimateRunSummary|null,current:CurrentClimateResult|null,values:ClimateValues|null,validated:ValidatedModel|null):boolean;
export function climateSeriesSensor(canOverlay:boolean,viewportSelection:string[],historicalId:string|null,sensorIds:string[]):string|null;
export class ClimateRequestGate {
  cancel():void;
  read<T>(request:()=>Promise<T>,expected?:(value:T)=>boolean):Promise<{value:T|null;error:string}|null>;
}
export function hourLabel(view:Pick<WeatherIntervalView,'hour'>|null):string;
