import type {ThermalModel} from './types';

/** Presentation-side ID allocation only; Rust validates the complete namespace on Apply. */
export function thermalObjectIds(model:ThermalModel):Set<string>{
  return new Set([
    ...model.materials,...model.constructions,...model.glazing,...model.curves,
    ...model.schedules,...model.zones,...model.detached_shading,
    ...model.zones.flatMap(zone=>[...zone.surfaces,...zone.windows,...(zone.cooling_system?[zone.cooling_system]:[])]),
  ].map(object=>object.id));
}
export function duplicateScheduleId(model:ThermalModel,sourceId:string):string{
  const used=thermalObjectIds(model);
  for(let number=1;;number++){
    const suffix=number===1?'_copy':`_copy${number}`;
    const candidate=sourceId.slice(0,80-suffix.length)+suffix;
    if(!used.has(candidate))return candidate;
  }
}
