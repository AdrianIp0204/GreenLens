import type {DraftSection} from './drafts';
import type {ThermalModel} from './types';

export interface ThermalInspectorTarget {
  object:{kind:string;id:string;zone_id:string|null};
  /** Rust JSON Pointer relative to the semantic object; never evaluated as code. */
  field_path:string;
}
export function inspectorIdentity(target:ThermalInspectorTarget){
  const {kind,id}=target.object;
  return {kind:kind==='shading'?'detached_shade':kind,id:kind==='overhang'&&id.startsWith('overhang:')?id.slice(9):id};
}
export function inspectorRoute(model:ThermalModel,target:ThermalInspectorTarget):{section:DraftSection;zoneId:string|null;exists:boolean}{
  const {kind,id}=inspectorIdentity(target),zone=model.zones.find(zone=>
    (target.object.zone_id===null||target.object.zone_id===zone.id)&&
    (['zone','gains'].includes(kind)?zone.id===id:
      kind==='surface'?zone.surfaces.some(s=>s.id===id):
      ['window','overhang'].includes(kind)?zone.windows.some(w=>w.id===id&&(kind!=='overhang'||Boolean(w.overhang))):
      ['cooling_system','supply_fan','balanced_ventilation'].includes(kind)?zone.cooling_system?.id===id:false));
  const section:DraftSection=kind==='model'&&['/exterior_lighting_w','/exterior_lighting_schedule'].includes(target.field_path)?'zone':
    kind==='model'&&target.field_path==='/north_rotation_deg'?'site':
    ['material','construction','glazing','surface','window','overhang','detached_shade'].includes(kind)?'envelope':
    ['curve','cooling_system','supply_fan','balanced_ventilation'].includes(kind)?'systems':kind==='schedule'?'schedules':
    ['site','study','calendar'].includes(kind)?'site':['zone','gains'].includes(kind)?'zone':'advanced';
  const resources={material:model.materials,construction:model.constructions,glazing:model.glazing,curve:model.curves,schedule:model.schedules,detached_shade:model.detached_shading};
  const resource=resources[kind as keyof typeof resources];
  return {section,zoneId:zone?.id??null,exists:Boolean(zone||(target.object.zone_id===null&&(resource?.some(item=>item.id===id)||(['model','site','study','calendar'].includes(kind)&&id===kind))))};
}

/** Open/focus only an exact semantic object and field, without changing draft values. */
export function focusInspectorTarget(root:HTMLElement,target:ThermalInspectorTarget):boolean{
  const {kind,id}=inspectorIdentity(target);
  const objects=[root,...root.querySelectorAll<HTMLElement>('[data-thermal-kind][data-thermal-id]')].filter(el=>el.dataset.thermalKind===kind&&el.dataset.thermalId===id);
  let object=objects[0],field:HTMLElement|undefined;
  if(target.field_path)for(const candidate of objects){const match=[...candidate.querySelectorAll<HTMLElement>('[data-thermal-field]')].find(el=>el.dataset.thermalField===target.field_path);if(match){object=candidate;field=match;break;}}
  if(!object)return false;
  const destination=field??object;
  for(let node:HTMLElement|null=destination;node&&node!==root;node=node.parentElement)if(node.tagName==='DETAILS')(node as HTMLDetailsElement).open=true;
  if(!field)object.tabIndex=-1;
  destination.focus({preventScroll:true});destination.scrollIntoView?.({block:'nearest'});
  return true;
}
