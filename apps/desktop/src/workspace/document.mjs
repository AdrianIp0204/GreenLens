// Accepted document changes only. Geometry and scientific hashes come from Rust.
export const clone = value => structuredClone(value);
export function documentSignature(project) {
  if (!project) return '';
  return JSON.stringify({ schema_version: project.schema_version, id: project.id, name: project.name, alternatives: project.alternatives, runs: project.runs, climate: project.climate ?? null, energy: project.energy ?? null, architecture:project.architecture??null });
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
// Only edited inputs belong in history. Restoring a batch must retain runs,
// weather, attachments and other evidence added after that transaction.
function energyBatchSnapshot(project, batch) {
  if (!project.energy) throw new Error('Energy project missing.');
  const designs = Object.fromEntries(Object.keys(batch.designs).sort().map(id => {
    if (!['A','B'].includes(id) || !project.energy.designs[id]) throw new Error('The thermal design no longer exists.');
    return [id, clone(project.energy.designs[id])];
  }));
  return { designs,
    ...('reportSettings' in batch ? {reportSettings:{carbon_factor:clone(project.energy.carbon_factor),lighting_service_evidence:project.energy.lighting_service_evidence}} : {}),
    ...('reportIssue' in batch ? {reportIssue:clone(project.energy.workbench?.report_metadata??null)} : {}) };
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
  async editEnergyBatch(project,alternativeId,batch,label,configure,canCommit=()=>true) {
    if(this.busy)throw new Error('Wait for the current document edit to finish.');
    this.busy=true;
    try {
      const before=energyBatchSnapshot(project,batch);
      // Services transform a disposable snapshot. Never adopt an intermediate
      // design or report change, even when a later Rust validation fails.
      const next=await configure(clone(project),clone(batch));
      const after=energyBatchSnapshot(next,batch);
      if(!canCommit())throw new Error('A newer draft was entered while Apply was pending. It is retained; review it before continuing.');
      for(const id of ['A','B'].filter(id=>!(id in batch.designs)))if(JSON.stringify(project.energy?.designs[id])!==JSON.stringify(next.energy?.designs[id]))throw new Error('Thermal transaction changed an untargeted design.');
      if(JSON.stringify(before)===JSON.stringify(after))return{project,changed:false};
      this.past.push({kind:'energy_batch',alternativeId,before,after,label});this.future=[];
      return{project:next,changed:true};
    }finally{this.busy=false;}
  }
  async editScope(project, alternativeId, candidate, label, configure) {
    if(this.busy)throw new Error('Wait for the current document edit to finish.');
    this.busy=true;
    try{
      const before=clone(project.architecture?.scope??null),next=await configure(clone(project),clone(candidate),false),after=clone(next.architecture?.scope??null);
      if(JSON.stringify(before)===JSON.stringify(after))return {project,changed:false};
      if(next.architecture?.scope_history){
        const prior=project.architecture?.scope_history?{kind:'revision',id:project.architecture.scope_history.active_revision_sha256}:{kind:'legacy_scope',scope:before};
        const accepted={kind:'revision',id:next.architecture.scope_history.active_revision_sha256};
        this.past.push({kind:'scope_revision',alternativeId,before:prior,after:accepted,label});
      }else this.past.push({kind:'architectural_scope',alternativeId,before,after,label});
      this.future=[];return {project:next,changed:true};
    }finally{this.busy=false;}
  }
  async acceptScopeRevision(project,alternativeId,label,configure,canCommit=()=>true){
    if(this.busy)throw new Error('Wait for the current document edit to finish.');
    this.busy=true;
    try{
      const history=project.architecture?.scope_history;
      const before=history?{kind:'revision',id:history.active_revision_sha256}:{kind:'legacy_scope',scope:clone(project.architecture?.scope??null)};
      const next=await configure(clone(project));
      if(!canCommit())throw new Error('The revision review changed while acceptance was pending. No revision was accepted.');
      const accepted=next.architecture?.scope_history;
      if(!accepted)throw new Error('Rust did not return a complete scope revision.');
      const after={kind:'revision',id:accepted.active_revision_sha256};
      if(before.kind==='revision'&&before.id===after.id)return{project,changed:false};
      this.past.push({kind:'scope_revision',alternativeId,before,after,label});this.future=[];return{project:next,changed:true};
    }finally{this.busy=false;}
  }
  async traverse(project, direction, validate, checkEdit = () => {}, restoreStudy, restoreEnergy, restoreEnergyMetadata,restoreEnergyGroup,restoreEnergyBatch,restoreScope) {
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
      if(entry.kind==='scope_revision') {
        if(!restoreScope)throw new Error('Scope revision restoration is unavailable.');
        next=candidate.kind==='revision'?await restoreScope(clone(project),null,true,candidate.id):await restoreScope(clone(project),candidate.scope,true);
        validated=await validate(clone(current.model),entry.alternativeId,next);
      }else if(entry.kind==='architectural_scope') {
        if(!restoreScope)throw new Error('Architectural scope restoration is unavailable.');
        next=await restoreScope(clone(project),candidate,true);validated=await validate(clone(current.model),entry.alternativeId,next);
      }else if(entry.kind==='energy_batch') {
        if(!restoreEnergyBatch)throw new Error('Atomic thermal draft restoration is unavailable.');
        next=await restoreEnergyBatch(clone(project),candidate);validated=await validate(clone(current.model),entry.alternativeId,next);
      }else if(entry.kind==='energy_group') {
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
