import { selectedRecordMatches } from './climate-pair-context.mjs';

/** Display coordination only: authoritative identities/values come from Rust. */
export const CLIMATE_FIELDS = [
  ['illuminance', 'Hourly illuminance', 'lx'], ['da', 'DA', '% of occupied time'],
  ['cda', 'cDA', '% of occupied time'], ['udi', 'UDI', '% of occupied time'],
  ['underlit', 'Underlit', '% of occupied time'], ['overlit', 'Overlit', '% of occupied time'],
];
export function climateRequestKey(path, summary, field, interval, settings) {
  return summary ? JSON.stringify([path, summary.id, summary.record_sha256, field, interval, settings]) : '';
}
export function climateOverlayMatches(summary, current, values, validated) {
  return Boolean(summary && current && values && validated &&
    selectedRecordMatches(summary, current) && values.run_id === summary.id &&
    values.sensor_ids.length === validated.sensor_grid.sensors.length &&
    values.sensor_ids.every((id, i) => id === validated.sensor_grid.sensors[i].id));
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
