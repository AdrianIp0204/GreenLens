/** A read-only lookup projection. Never save this filtered document. */
export function selectedPairProject(project, a, b) {
  if (!project?.climate) return null;
  return { ...project, climate: { ...project.climate, runs: [a, b].filter(Boolean) } };
}

/** A single historical record needs its own Rust relationship, not the newest run's. */
export function selectedRecordProject(project, summary) {
  return selectedPairProject(project, summary, null);
}
export function climateRelationKey(path, project) {
  return JSON.stringify([path, project?.id, project?.alternatives, project?.climate?.studies, project?.climate?.runs]);
}
/** This is a response-identity check, never a frontend scientific comparison. */
export function selectedRecordMatches(summary, condition) {
  return Boolean(summary && condition?.summary && condition.run &&
    condition.summary.id === summary.id && condition.run.id === summary.id &&
    condition.summary.record_sha256 === summary.record_sha256 &&
    condition.physical_input_hash === summary.physical_input_hash);
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
  if (!selectedRecordMatches(summary, condition)) return 'CURRENT-STUDY MISMATCH: Rust did not match this snapshot to the current geometry, weather, study and numerical settings.';
  if (condition.stored_metric_identity && condition.requested_metric_identity && !condition.metrics_current) return 'Physical inputs match; CURRENT METRIC SETTINGS DIFFER from the saved snapshot metrics.';
  return 'Physical inputs and numerical settings match current study. Saved metric conventions belong to this snapshot.';
}
