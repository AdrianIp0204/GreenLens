import { selectedRecordMatches } from './climate-pair-context.mjs';

/** Display coordination only: authoritative identities/values come from Rust. */
export const CLIMATE_FIELDS = [
  ['illuminance', 'Hourly illuminance', 'lx'], ['da', 'DA', '% of occupied time'],
  ['cda', 'cDA', '% of occupied time'], ['udi', 'UDI', '% of occupied time'],
  ['underlit', 'Underlit', '% of occupied time'], ['overlit', 'Overlit', '% of occupied time'],
];
/** Select already Rust-validated settings; never derive metrics in the renderer. */
export function displayedMetricSettings(selectedRunId, study, run) {
  return selectedRunId === 'current' && study ? study.metrics : run?.study.metrics ?? null;
}
/** Caption only. Boundaries and weights remain evaluated by the Rust service. */
export function metricConvention(field, settings) {
  if (!settings || field === 'illuminance') return '';
  const threshold = settings.da_threshold_lux, lower = settings.udi_lower_lux, upper = settings.udi_upper_lux;
  const boundary = {
    da: `DA: E ≥ ${threshold} lx`, cda: `cDA: E / (${threshold} lx), capped at 1`,
    udi: `UDI: ${lower} lx ≤ E ≤ ${upper} lx`,
    underlit: `Underlit: E < ${lower} lx`, overlit: `Overlit: E > ${upper} lx`,
  }[field];
  if (!boundary) return '';
  const clock = minute => `${String(Math.floor(minute / 60)).padStart(2, '0')}:${String(minute % 60).padStart(2, '0')}`;
  const occupancy = settings.occupancy.kind === 'daily'
    ? `every day ${clock(settings.occupancy.start_minute)}–${clock(settings.occupancy.end_minute)} local standard time${settings.occupancy.end_minute < settings.occupancy.start_minute ? ' (across midnight)' : ''}`
    : 'explicit 8,760 interval weights';
  return `${boundary}. Occupancy: ${occupancy}; occupied nighttime is included.`;
}
export function climateRequestKey(path, summary, field, interval, settings) {
  return summary ? JSON.stringify([path, summary.id, summary.record_sha256, field, interval, settings]) : '';
}
export function climateGeometryMatches(summary, current, sensorIds, validated) {
  return Boolean(summary && current && sensorIds && validated &&
    selectedRecordMatches(summary, current) &&
    sensorIds.length === validated.sensor_grid.sensors.length &&
    sensorIds.every((id, i) => id === validated.sensor_grid.sensors[i].id));
}
export function climateOverlayMatches(summary, current, values, validated) {
  return Boolean(values && values.run_id === summary?.id &&
    climateGeometryMatches(summary, current, values.sensor_ids, validated));
}
/** A stable ID does not establish matching sensor positions across snapshots. */
export function climateSeriesSensor(canOverlay, viewportSelection, historicalId, sensorIds) {
  const id=canOverlay?viewportSelection.find(value=>value.startsWith('sensor:'))?.slice(7):historicalId;
  return id&&sensorIds.includes(id)?id:null;
}
/** Debounced requests may finish out of order; obsolete data never becomes visible. */
export class ClimateRequestGate {
  sequence = 0;
  cancel() { this.sequence++; }
  async read(request, expected) {
    const sequence = ++this.sequence;
    try {
      const value = await request();
      if (sequence !== this.sequence) return null;
      if (expected && !expected(value)) throw new Error('The result response does not match the requested snapshot, field or interval.');
      return { value, error: '' };
    } catch (failure) {
      if (sequence !== this.sequence) return null;
      return { value: null, error: typeof failure === 'string' ? failure : failure instanceof Error ? failure.message : String(failure) };
    }
  }
}
export function hourLabel(view) {
  if (!view) return 'Loading retained weather interval…';
  const h = view.hour;
  return `${String(h.month).padStart(2, '0')}/${String(h.day).padStart(2, '0')} · ${String(h.hour_ending - 1).padStart(2, '0')}:00–${String(h.hour_ending).padStart(2, '0')}:00 · midpoint ${String(h.hour_ending - 1).padStart(2, '0')}:30 local standard time`;
}
