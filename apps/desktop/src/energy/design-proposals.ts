import type {InputSource,ThermalModel,ThermalWindow,ThermalOverhang,ThermalBoxShade,ThermalGlazing} from './types';
export type ProposalAction =
 | {kind:'put_window';zone_id:string;window:ThermalWindow}
 | {kind:'remove_window';zone_id:string;window_id:string}
 | {kind:'set_overhang';zone_id:string;window_id:string;overhang:ThermalOverhang|null}
 | {kind:'put_detached_shade';shade:ThermalBoxShade}
 | {kind:'remove_detached_shade';shade_id:string}
 | {kind:'assign_window_glazing';zone_id:string;window_id:string;glazing:ThermalGlazing}
 | {kind:'assign_construction';zone_id:string;surface_id:string;construction_id:string};
export interface ProposalDraft {action:ProposalAction;source:InputSource;units:string;rationale:string}
export interface ProposalReview {schema_version:string;target:'B';context_sha256:string;figures?:{filename:string;frame_id:string;side:string;svg:string;svg_sha256:string}[];proposal:{candidate:ThermalModel;valid:boolean;problems:string[];affected_object_ids:string[];tradeoffs:string[];base_model_sha256:string;candidate_model_sha256:string|null;preview_sha256:string}}
