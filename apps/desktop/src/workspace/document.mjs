// Accepted document changes only. Geometry and scientific hashes come from Rust.
export const clone = value => structuredClone(value);
export function documentSignature(project) {
  if (!project) return '';
  return JSON.stringify({ schema_version: project.schema_version, id: project.id, name: project.name, alternatives: project.alternatives, runs: project.runs, climate: project.climate ?? null, energy: project.energy ?? null });
}
// History records one alternative's study, never a whole project snapshot that
// could erase immutable runs or weather imported after the edit.
export function studySnapshot(project, alternativeId) {
  return { project_schema_version: project.schema_version, study: clone(project.climate?.studies[alternativeId] ?? null) };
}
export function completedJobResult(job) { return job?.result ?? job?.climate_result ?? job?.energy_result ?? null; }
export function hasPendingEvidence(job, results = [], climateResults = [], energyResults = []) {
  return Boolean(
    (job?.result && !results.some(run => run.id === job.result.id)) ||
    (job?.climate_result && !climateResults.some(run => run.id === job.climate_result.id)) ||
    (job?.energy_result && !energyResults.some(run => run.id === job.energy_result.id)) ||
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
  async editEnergy(project, alternativeId, candidate, label, configure) {
    if (this.busy) throw new Error('Wait for the current document edit to finish.');
    const before = project.energy?.designs[alternativeId];
    if (!before) throw new Error('The thermal design no longer exists.');
    this.busy = true;
    try {
      const next = await configure(clone(project), alternativeId, clone(candidate));
      const after = next.energy?.designs[alternativeId];
      if (!after) throw new Error('Rust returned no thermal design.');
      if (JSON.stringify(before) === JSON.stringify(after)) return { project, changed: false };
      this.past.push({ kind:'energy', alternativeId, before:clone(before), after:clone(after), label }); this.future=[];
      return {project:next,changed:true};
    } finally { this.busy=false; }
  }
  async editEnergyMetadata(project,alternativeId,change,label,configure) {
    if(this.busy)throw new Error('Wait for the current document edit to finish.');
    if(!project.energy)throw new Error('Energy project missing.');
    const snapshot=(p)=>change.kind==='attachment'
      ?{kind:'attachment',id:change.id,included:p.energy.attachments.find(a=>a.id===change.id)?.include_in_export}
      :change.kind==='report_issue'?{kind:'report_issue',metadata:clone(p.energy.workbench?.report_metadata??null)}
      :{kind:'report_settings',carbon_factor:clone(p.energy.carbon_factor),lighting_service_evidence:p.energy.lighting_service_evidence};
    this.busy=true;try {const before=snapshot(project),next=await configure(clone(project),clone(change)),after=snapshot(next);
      if(JSON.stringify(before)===JSON.stringify(after))return {project,changed:false};
      this.past.push({kind:'energy_metadata',alternativeId,before,after,label});this.future=[];return {project:next,changed:true};
    }finally{this.busy=false;}
  }
  async editEnergyGroup(project,alternativeId,targets,label,configure) {
    if(this.busy)throw new Error('Wait for the current document edit to finish.');
    const ids=[...new Set(targets)].sort();
    if(!ids.length||ids.some(id=>!['A','B'].includes(id)||!project.energy?.designs[id]))throw new Error('Select existing thermal design targets explicitly.');
    this.busy=true;
    try{
      const before=Object.fromEntries(ids.map(id=>[id,clone(project.energy.designs[id])]));
      const next=await configure(clone(project));
      if(ids.some(id=>!next.energy?.designs[id]))throw new Error('Rust returned an incomplete thermal transaction.');
      for(const id of ['A','B'].filter(id=>!ids.includes(id)))if(JSON.stringify(project.energy?.designs[id])!==JSON.stringify(next.energy?.designs[id]))throw new Error('Thermal transaction changed an untargeted design.');
      const after=Object.fromEntries(ids.map(id=>[id,clone(next.energy.designs[id])]));
      if(JSON.stringify(before)===JSON.stringify(after))return{project,changed:false};
      this.past.push({kind:'energy_group',alternativeId,before,after,label});this.future=[];
      return{project:next,changed:true};
    }finally{this.busy=false;}
  }
  async traverse(project, direction, validate, checkEdit = () => {}, restoreStudy, restoreEnergy, restoreEnergyMetadata,restoreEnergyGroup) {
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
      if(entry.kind==='energy_group') {
        if(!restoreEnergyGroup)throw new Error('Atomic thermal restoration is unavailable.');
        next=await restoreEnergyGroup(clone(project),candidate);validated=await validate(clone(current.model),entry.alternativeId,next);
      }else if(entry.kind==='energy_metadata') {
        if(!restoreEnergyMetadata)throw new Error('The report metadata restoration service is unavailable.');
        next=await restoreEnergyMetadata(clone(project),candidate);validated=await validate(clone(current.model),entry.alternativeId,next);
      } else if (entry.kind === 'energy') {
        if (!restoreEnergy) throw new Error('The thermal restoration service is unavailable.');
        next = await restoreEnergy(clone(project),entry.alternativeId,candidate);
        validated = await validate(clone(current.model),entry.alternativeId,next);
      } else if (entry.kind === 'study') {
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
