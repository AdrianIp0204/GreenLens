// Presentation only: hashes and result values are supplied by the Rust service.
export function visibleResult(validated, run, pending = false) {
  return !pending && validated && run && validated.scientific_input_hash === run.validated.scientific_input_hash ? run : null;
}
export function luxColor(value, maximum) {
  const t = maximum > 0 ? Math.max(0, Math.min(1, value / maximum)) : 0;
  const stops = [[28, 57, 83], [28, 112, 120], [79, 171, 139], [191, 210, 122], [246, 211, 105]];
  const p = t * (stops.length - 1), i = Math.min(stops.length - 2, Math.floor(p)), f = p - i;
  return `rgb(${stops[i].map((channel, c) => Math.round(channel + f * (stops[i + 1][c] - channel))).join(',')})`;
}
export function activeJob(job) { return Boolean(job && ['queued', 'preparing', 'running'].includes(job.state)); }
