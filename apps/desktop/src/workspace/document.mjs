// Accepted document changes only. Geometry and scientific hashes come from Rust.
export const clone = value => structuredClone(value);
export function documentSignature(project) {
  if (!project) return '';
  return JSON.stringify({ schema_version: project.schema_version, id: project.id, name: project.name, alternatives: project.alternatives, runs: project.runs, climate: project.climate ?? null });
}
// History records one alternative's study, never a whole project snapshot that
// could erase immutable runs or weather imported after the edit.
export function studySnapshot(project, alternativeId) {
  return { project_schema_version: project.schema_version, study: clone(project.climate?.studies[alternativeId] ?? null) };
}
export function completedJobResult(job) { return job?.result ?? job?.climate_result ?? null; }
export function hasPendingEvidence(job, results = [], climateResults = []) {
  return Boolean(
    (job?.result && !results.some(run => run.id === job.result.id)) ||
    (job?.climate_result && !climateResults.some(run => run.id === job.climate_result.id)) ||
    (job?.pending_evidence && !completedJobResult(job))
  );
}
export class DocumentHistory {
  past = [];
  future = [];
  busy = false;
  reset() { this.past = []; this.future = []; }
  async edit(project, alternativeId, candidate, label, validate, checkEdit = () => {}) {
    if (this.busy) throw new Error('Wait for the current document edit to finish.');
    const current = project.alternatives.find(a => a.id === alternativeId);
    if (!current) throw new Error('The design alternative no longer exists.');
    this.busy = true;
    try {
      const validated = await validate(clone(candidate), alternativeId, project);
      await checkEdit(current.model, validated.model, validated);
      if (JSON.stringify(current.model) === JSON.stringify(validated.model)) return { project, validated, changed: false };
      const entry = { kind: 'geometry', alternativeId, before: clone(current.model), after: clone(validated.model), label };
      const next = { ...project, alternatives: project.alternatives.map(a => a.id === alternativeId ? { ...a, model: clone(validated.model) } : a) };
      this.past.push(entry); this.future = [];
      return { project: next, validated, changed: true };
    } finally { this.busy = false; }
  }
  async editStudy(project, alternativeId, candidate, label, configure) {
    if (this.busy) throw new Error('Wait for the current document edit to finish.');
    if (!project.alternatives.some(a => a.id === alternativeId)) throw new Error('The design alternative no longer exists.');
    this.busy = true;
    try {
      const next = await configure(clone(project), alternativeId, clone(candidate));
      const before = studySnapshot(project, alternativeId), after = studySnapshot(next, alternativeId);
      if (JSON.stringify(before) === JSON.stringify(after)) return { project, changed: false };
      this.past.push({ kind: 'study', alternativeId, before, after, label }); this.future = [];
      return { project: next, changed: true };
    } finally { this.busy = false; }
  }
  async traverse(project, direction, validate, checkEdit = () => {}, restoreStudy) {
    if (this.busy) throw new Error('Wait for the current document edit to finish.');
    const source = direction === 'undo' ? this.past : this.future;
    const target = direction === 'undo' ? this.future : this.past;
    const entry = source.at(-1);
    if (!entry) throw new Error(`Nothing to ${direction}.`);
    this.busy = true;
    try {
      const current = project.alternatives.find(a => a.id === entry.alternativeId);
      if (!current) throw new Error('The design alternative no longer exists.');
      const candidate = clone(direction === 'undo' ? entry.before : entry.after);
      let next, validated;
      if (entry.kind === 'study') {
        if (!restoreStudy) throw new Error('The study restoration service is unavailable.');
        next = await restoreStudy(clone(project), entry.alternativeId, candidate);
        validated = await validate(clone(current.model), entry.alternativeId, next);
      } else {
        validated = await validate(candidate, entry.alternativeId, project);
        await checkEdit(current.model, validated.model, validated);
        next = { ...project, alternatives: project.alternatives.map(a => a.id === entry.alternativeId ? { ...a, model: clone(validated.model) } : a) };
      }
      source.pop(); target.push(entry);
      return { project: next, validated, alternativeId: entry.alternativeId, label: entry.label };
    } finally { this.busy = false; }
  }
}

// Resolve the user's decision before allowing any destructive document action.
export async function protectReplacement({ needsGuard, decide, save, discard, proceed }) {
  if (!needsGuard) { await proceed(); return true; }
  const decision = await decide();
  if (decision === 'cancel') return false;
  if (decision === 'save') {
    if (!(await save())) return false;
  } else if (decision === 'discard') await discard();
  else throw new Error('Unknown document protection decision.');
  await proceed(); return true;
}
export function isTextTarget(target) {
  return Boolean(target?.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target?.tagName ?? '') || target?.closest?.('[contenteditable="true"]'));
}
