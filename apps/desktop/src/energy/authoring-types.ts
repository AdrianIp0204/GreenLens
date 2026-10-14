import type { ThermalBoundary, ThermalBoxShade, ThermalFace, ThermalModel, ThermalWindow } from './types';
export type ZoneBoxInput={id:string;name:string;storey:number;origin_m:[number,number,number];size_m:[number,number,number]};
export type TopologyAssignments={exterior_construction_id:string;ground_construction_id:string;interzone_construction_id:string;interzone_reverse_construction_id:string;insulated_ground_reason:string};
export type ThermalOperation=
  |{operation:'set_zone_box';zone:ZoneBoxInput}
  |{operation:'add_zone';source_zone_id:string;zone:ZoneBoxInput}
  |{operation:'add_adjacent';source_zone_id:string;id:string;name:string;face:ThermalFace;depth_m:number}
  |{operation:'remove_zone';zone_id:string}
  |{operation:'put_window';zone_id:string;window:ThermalWindow}
  |{operation:'remove_window';zone_id:string;window_id:string}
  |{operation:'put_shade';shade:ThermalBoxShade}
  |{operation:'remove_shade';shade_id:string}
  |{operation:'assign_surface';zone_id:string;surface_id:string;construction_id:string;boundary:ThermalBoundary};
export type ThermalTransaction={schema_version:'greenlens.thermal-transaction.v1';expected_model_sha256:string;operation:ThermalOperation;topology:TopologyAssignments};
export type TransactionRequests=Record<string,ThermalTransaction>;
export type OperationPlan={requests:TransactionRequests;preview:{schema_version:string;preview_sha256:string;valid:boolean;untouched_designs:string[];designs:Record<string,{candidate:ThermalModel;valid:boolean;problems:string[];affected_object_ids:string[];assumptions:string[]}>}};
