function entityLockReason(id, layer, display) {
  const parent = id.startsWith('surface:') ? 'room' : id.startsWith('sensor:') ? 'sensor-plane' : null;
  if (display.layers[layer]?.locked || display.objects[id]?.locked || (parent && display.objects[parent]?.locked)) {
    return `${id} is locked. This edit changes its parameters or generated geometry. Unlock its object or display layer before editing.`;
  }
  return null;
}

export function editLockReason(before, after, display) {
  const changed = (a,b)=>JSON.stringify(a)!==JSON.stringify(b);
  const entities=[];
  for(const part of ['room','window','overhang','obstruction'])if(changed(before[part],after[part]))entities.push(part);
  if(before.materials.visible_transmittance!==after.materials.visible_transmittance)entities.push('window');
  for(const key of ['wall_reflectance','floor_reflectance','ceiling_reflectance'])if(before.materials[key]!==after.materials[key])entities.push('room');
  if(changed(before.sensors,after.sensors))entities.push('sensor-plane');
  for(const id of entities){const layer=id==='sensor-plane'?'sensors':id;
    const reason=entityLockReason(id,layer,display);if(reason)return reason;
  }
  return null;
}

// Consume authoritative Rust output. Do not recreate geometry or scientific
// dependency calculations in the frontend. Union IDs also covers removals.
export function validatedEditLockReason(before, after, display) {
  const direct = editLockReason(before.model, after.model, display);
  if (direct) return direct;
  const changed = (a,b) => JSON.stringify(a) !== JSON.stringify(b);
  const surfacesBefore = new Map(before.surfaces.map(surface => [surface.id,surface]));
  const surfacesAfter = new Map(after.surfaces.map(surface => [surface.id,surface]));
  for (const id of new Set([...surfacesBefore.keys(),...surfacesAfter.keys()])) {
    const a=surfacesBefore.get(id),b=surfacesAfter.get(id);
    if (!changed(a,b)) continue;
    for (const surface of [a,b]) {
      if (!surface) continue;
      const layer=surface.material==='glass'?'window':['overhang','obstruction'].includes(surface.material)?surface.material:'room';
      const entity=layer==='room'?`surface:${id}`:layer;
      const reason=entityLockReason(entity,layer,display);if(reason)return reason;
    }
  }
  const sensorsBefore = new Map(before.sensor_grid.sensors.map(sensor => [sensor.id,sensor]));
  const sensorsAfter = new Map(after.sensor_grid.sensors.map(sensor => [sensor.id,sensor]));
  for (const id of new Set([...sensorsBefore.keys(),...sensorsAfter.keys()])) {
    if (!changed(sensorsBefore.get(id),sensorsAfter.get(id))) continue;
    const reason=entityLockReason(`sensor:${id}`,'sensors',display);if(reason)return reason;
  }
  return null;
}
