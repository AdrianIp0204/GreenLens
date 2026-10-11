// Presentation only: hashes and result values are supplied by the Rust service.
export function visibleResult(validated, run, pending = false) {
  return !pending && validated && run && validated.scientific_input_hash === run.validated.scientific_input_hash ? run : null;
}
// The records remain immutable and ordered oldest first. An optional caller-owned
// study predicate lets future modes apply Rust-provided identities without
// interpreting or computing scientific fingerprints in the presentation layer.
export function newestCompatibleResult(validated, records, alternativeId, pending = false, matchesStudy = (_run) => true) {
  if (pending || !validated) return null;
  for (let i = records.length - 1; i >= 0; i--) {
    const run = records[i];
    if (run.alternative_id === alternativeId && visibleResult(validated, run) && matchesStudy(run)) return run;
  }
  return null;
}
export function luxColor(value, maximum) {
  const t = maximum > 0 ? Math.max(0, Math.min(1, value / maximum)) : 0;
  const stops = [[28, 57, 83], [28, 112, 120], [79, 171, 139], [191, 210, 122], [246, 211, 105]];
  const p = t * (stops.length - 1), i = Math.min(stops.length - 2, Math.floor(p)), f = p - i;
  return `rgb(${stops[i].map((channel, c) => Math.round(channel + f * (stops[i + 1][c] - channel))).join(',')})`;
}
export function activeJob(job) { return Boolean(job && ['queued', 'preparing', 'running'].includes(job.state)); }
