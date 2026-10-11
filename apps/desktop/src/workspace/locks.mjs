export function editLockReason(before, after, display) {
  const changed = (a,b)=>JSON.stringify(a)!==JSON.stringify(b);
  const entities=[];
  for(const part of ['room','window','overhang','obstruction'])if(changed(before[part],after[part]))entities.push(part);
  if(before.materials.visible_transmittance!==after.materials.visible_transmittance)entities.push('window');
  for(const key of ['wall_reflectance','floor_reflectance','ceiling_reflectance'])if(before.materials[key]!==after.materials[key])entities.push('room');
  if(changed(before.sensors,after.sensors))entities.push('sensor-plane');
  for(const id of entities){const layer=id==='sensor-plane'?'sensors':id;
    if(display.layers[layer]?.locked||display.objects[id]?.locked)return `${id} is locked. Unlock its object or display layer before editing.`;
  }
  return null;
}
