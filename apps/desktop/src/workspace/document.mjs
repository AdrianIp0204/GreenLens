// Accepted document changes only. Geometry and scientific hashes come from Rust.
export const clone = value => structuredClone(value);
export function documentSignature(project) {
  if (!project) return '';
  return JSON.stringify({ id: project.id, name: project.name, alternatives: project.alternatives, runs: project.runs });
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
      const validated = await validate(clone(candidate));
      await checkEdit(current.model, validated.model, validated);
      if (JSON.stringify(current.model) === JSON.stringify(validated.model)) return { project, validated, changed: false };
      const entry = { alternativeId, before: clone(current.model), after: clone(validated.model), label };
      const next = { ...project, alternatives: project.alternatives.map(a => a.id === alternativeId ? { ...a, model: clone(validated.model) } : a) };
      this.past.push(entry); this.future = [];
      return { project: next, validated, changed: true };
    } finally { this.busy = false; }
  }
  async traverse(project, direction, validate, checkEdit = () => {}) {
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
      const validated = await validate(candidate);
      await checkEdit(current.model, validated.model, validated);
      const next = { ...project, alternatives: project.alternatives.map(a => a.id === entry.alternativeId ? { ...a, model: clone(validated.model) } : a) };
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
