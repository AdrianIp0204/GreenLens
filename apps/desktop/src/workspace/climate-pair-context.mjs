/** A read-only lookup projection. Never save this filtered document. */
export function selectedPairProject(project, a, b) {
  if (!project?.climate) return null;
  return { ...project, climate: { ...project.climate, runs: [a, b].filter(Boolean) } };
}

/** Display navigation follows its own snapshot, independently for each view. */
export function snapshotDisplayContext(run, request) {
  const annual = run?.study.calculation.mode === 'annual';
  return {
    annual,
    field: annual ? request.field : 'illuminance',
    interval: run?.study.calculation.mode === 'weather_hour' ? run.study.calculation.interval_index : request.interval,
  };
}
export function snapshotDisplayScale(field, values) {
  return field === 'illuminance' ? Math.max(1, values?.statistics.max ?? 0) : 100;
}

/** Rust computed these identities against the selected immutable record. */
export function selectedPairRelation(summary, condition, error = '') {
  if (!summary) return 'No selected snapshot.';
  if (!condition) return error || 'Checking relationship to the current study…';
  if (summary.physical_input_hash !== condition.physical_input_hash) return 'CURRENT-STUDY MISMATCH: geometry, weather or physical study settings differ.';
  if (condition.stored_metric_identity && condition.requested_metric_identity && !condition.metrics_current) return 'Physical inputs match; CURRENT METRIC SETTINGS DIFFER from this historical pair.';
  return 'Physical inputs match current study. This pair uses its saved metric conventions.';
}
