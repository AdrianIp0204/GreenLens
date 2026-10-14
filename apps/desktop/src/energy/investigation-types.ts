import type {CoincidentDemand,EnergyEndUse,EnergyInputComparison,EnergyMethodIdentity,EnergyTimestamp,MonthDay} from './types';
export type LocalTime={date:MonthDay;minute:number};
export type NavigationRequest={kind:'at';time:LocalTime}|{kind:'day';date:MonthDay}|{kind:'range';start:LocalTime;end:LocalTime}|{kind:'facility_peak'}|{kind:'occupied_unmet';start:LocalTime;zone_id:string|null};
export type Anchor='baseline'|'proposed';
export type DemandPoint={timestamp:EnergyTimestamp;facility_kw:number;end_uses_kw:Record<EnergyEndUse,number>};
export type ComfortInterval={timestamp:{source_interval_index:number;common_interval_index:number;date:MonthDay;start_minute:number;end_minute:number};samples:{zone_id:string;occupied:boolean;air_temperature_c:number;cooling_setpoint_c:number}[];occupied_unmet_zone_ids:string[]};
export type InvestigationData={window:{start_interval:number;count:number;focus_interval:number|null};source_interval_minutes:5|10;demand_10min:DemandPoint[];comfort:ComfortInterval[]};
export type NavigationResult={schema_version:'greenlens.energy-investigation.v1';run_id:string;record_sha256:string;physical_input_sha256:string;method:EnergyMethodIdentity;retained_facility_peak:CoincidentDemand;data:InvestigationData|null;searched_interval_count:number;decoded_source_interval_count:number;integrity_scope:string};
export type PairNavigationResult={schema_version:'greenlens.energy-pair-investigation.v1';anchor:Anchor;baseline:NavigationResult;proposed:NavigationResult;shared_demand_scale:{minimum_kw:number;maximum_kw:number}|null;input_comparison:EnergyInputComparison;limitations:string[]};
