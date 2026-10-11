import type { Surface, ValidatedModel, Vec3 } from '../types';
import type { LayerId, SemanticObject } from './types';
import { localToWorld } from './interaction.mjs';
export { objectVisibility, selectableIds } from './interaction.mjs';

export const surfaceEntityId = (surface: Surface) => surface.material === 'glass' ? 'window' : surface.material === 'overhang' ? 'overhang' : surface.material === 'obstruction' ? 'obstruction' : `surface:${surface.id}`;
export const surfaceLayer = (surface: Surface): LayerId => surface.material === 'glass' ? 'window' : surface.material === 'overhang' ? 'overhang' : surface.material === 'obstruction' ? 'obstruction' : 'room';
export const objectLabel = (id: string) => id === 'sensor-plane' ? 'Sensor plane' : id.startsWith('sensor:') ? `Sensor ${id.slice(7)}` : id.startsWith('surface:') ? id.slice(8).replaceAll('_', ' ') : id === 'room' ? 'Room envelope' : id[0].toUpperCase() + id.slice(1);
export function semanticObjects(validated: ValidatedModel): SemanticObject[] {
  const objects = new Map<string, SemanticObject>();
  for (const surface of validated.surfaces) {
    const id = surfaceEntityId(surface);
    if (!objects.has(id)) objects.set(id, { id, label: objectLabel(id), layer: surfaceLayer(surface), vertices: [] });
    objects.get(id)!.vertices.push(...surface.vertices);
  }
  const { room: r, sensors: s } = validated.model;
  const corners: Vec3[] = [[s.wall_inset_m, s.wall_inset_m, s.workplane_height_m], [r.width_m - s.wall_inset_m, s.wall_inset_m, s.workplane_height_m], [r.width_m - s.wall_inset_m, r.depth_m - s.wall_inset_m, s.workplane_height_m], [s.wall_inset_m, r.depth_m - s.wall_inset_m, s.workplane_height_m]];
  objects.set('sensor-plane', { id: 'sensor-plane', label: 'Sensor plane', layer: 'sensors', vertices: corners.map(p => localToWorld(p, r.rotation_deg) as Vec3) });
  for (const sensor of validated.sensor_grid.sensors) objects.set(`sensor:${sensor.id}`, { id: `sensor:${sensor.id}`, label: `Sensor ${sensor.id}`, layer: 'sensors', vertices: [sensor.position] });
  const envelope = validated.surfaces.filter(s => ['wall', 'floor', 'ceiling'].includes(s.material)).flatMap(s => s.vertices);
  objects.set('room', { id: 'room', label: 'Room envelope', layer: 'room', vertices: envelope });
  return [...objects.values()];
}
